//! eat_hwp_raw 워크플로 오케스트레이션.
//!
//! 파이프라인: HWPML 파싱 → 컨텍스트+프롬프트 구성 → LLM 실행(MCP 서버가 있으면 도구 호출 포함).
//! GUI 프로세스가 LLM을 직접 호출한다(별도 서버는 보류, CLAUDE.md "구조 / 산출물").
//! 답은 GUI의 "커서 위치에 삽입" 버튼으로 문서에 넣는다(`hwp::insert_into`).

use crate::config::Config;
use crate::hwp::DocContent;
use crate::llm::{Completion, LlmBackend};
use crate::mcp;
use crate::parse;
use anyhow::Result;

/// 문서의 HWPML과 사용자 프롬프트로 LLM 응답을 생성한다.
///
/// LLM에는 항상 문서 전체를 보낸다. `caret`은 `hwpml` 안에서 커서가 있는 바이트 위치(없으면
/// `None`)이고, 그 자리에 [`parse::CARET_MARK`]를 넣어 LLM이 커서 위치를 알게 한다.
/// `mcp_servers`가 있으면 그 도구를 LLM에 준다.
pub fn run_prompt(
    _cfg: &Config,
    backend: &dyn LlmBackend,
    content: &DocContent,
    format: DocFormat,
    user_prompt: &str,
    custom_system: &str,
    mcp_servers: &[mcp::ServerSpec],
) -> Result<Completion> {
    // 커서 정보가 없으면(예: EAT_HWP_DOC 테스트 경로) 표시 없이 문서 전체만 보낸다.
    let context = match (content, format) {
        (DocContent::Hwpml { hwpml, caret }, DocFormat::Plain) => {
            parse::document_text(&parse::parse_hwpml(hwpml)?, *caret)
        }
        (DocContent::Hwpml { hwpml, caret }, DocFormat::Hwpml | DocFormat::HwpmlFile) => {
            parse::hwpml_for_llm(hwpml, *caret)
        }
        (DocContent::Markdown(md), DocFormat::Markdown) => md.clone(),
        _ => anyhow::bail!("읽은 문서 내용이 고른 형식({})과 맞지 않음", format.label()),
    };
    let has_caret = context.contains(parse::CARET_MARK);

    let system = build_system(format, has_caret, !mcp_servers.is_empty(), custom_system);
    let user = build_user(&context, user_prompt);
    tracing::debug!(
        ?format,
        has_caret,
        context_chars = context.chars().count(),
        system_chars = system.chars().count(),
        custom_system_chars = custom_system.chars().count(),
        user_chars = user.chars().count(),
        "프롬프트 구성"
    );
    tracing::trace!(%system, "시스템 메시지 전문");
    // 문서 내용이 통째로 들어가므로 TRACE에서만 남긴다(EAT_HWP_LOG=trace).
    tracing::trace!(%user, "사용자 메시지 전문");
    backend.complete(&system, &user, mcp_servers)
}

/// LLM에 보낼 문서 형식. GUI에서 고른다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocFormat {
    /// HWPML에서 글자만 뽑아 한 문단에 한 줄로 이은 평문.
    Plain,
    /// `GetTextFile("HWPML2X")` 원문(XML). 글자·문단 모양까지 들어 있다.
    Hwpml,
    /// `SaveAs(경로, "HWPML2X", "lock:false")`로 저장한 HWPML 파일(XML). 커서 위치는 없다.
    HwpmlFile,
    /// `SaveAs`로 HWPX를 저장해 hwp2md로 바꾼 Markdown. 제목·표 구조가 남는다.
    Markdown,
}

impl DocFormat {
    /// GUI에서 처음 골라져 있는 형식.
    pub const DEFAULT: DocFormat = DocFormat::Markdown;

    /// GUI 목록 순서.
    pub const ALL: [DocFormat; 4] = [
        DocFormat::Plain,
        DocFormat::Hwpml,
        DocFormat::Markdown,
        DocFormat::HwpmlFile,
    ];

    /// GUI 목록에 보일 이름.
    pub fn label(self) -> &'static str {
        match self {
            DocFormat::Plain => "평문 (글자만)",
            DocFormat::Hwpml => "HWPML (GetTextFile 원문)",
            DocFormat::Markdown => "Markdown (HWPX 저장 → hwp2md)",
            DocFormat::HwpmlFile => "HWPML (SaveAs 파일, 커서 위치 없음)",
        }
    }

    /// 설정 파일에 쓰는 이름.
    pub fn id(self) -> &'static str {
        match self {
            DocFormat::Plain => "plain",
            DocFormat::Hwpml => "hwpml",
            DocFormat::Markdown => "markdown",
            DocFormat::HwpmlFile => "hwpml_file",
        }
    }

    /// [`DocFormat::id`]의 반대. 모르는 이름이면 `None`.
    pub fn from_id(id: &str) -> Option<DocFormat> {
        DocFormat::ALL.into_iter().find(|f| f.id() == id)
    }

    /// 대화 머리줄에 붙일 짧은 이름.
    pub fn short(self) -> &'static str {
        match self {
            DocFormat::Plain => "평문",
            DocFormat::Hwpml => "HWPML",
            DocFormat::Markdown => "Markdown",
            DocFormat::HwpmlFile => "HWPML 파일",
        }
    }

    /// 프롬프트에서 문서 내용의 형식을 설명하는 문장.
    fn describe(self) -> &'static str {
        match self {
            DocFormat::Plain => "문서 내용은 한글 문서에서 글자만 뽑은 것으로, 한 줄이 한 문단입니다.",
            DocFormat::Hwpml | DocFormat::HwpmlFile => {
                "문서 내용은 한글 문서의 HWPML 2.x(XML) 원문입니다. 본문은 BODY의 P(문단) > TEXT > CHAR에 \
                 있고, 글자 모양·문단 모양은 HEAD의 CHARSHAPE·PARASHAPE를 CharShape·ParaShape 번호로 \
                 참조합니다."
            }
            DocFormat::Markdown => {
                "문서 내용은 한글 문서를 Markdown으로 바꾼 것입니다. 제목은 #, 표는 | 구문으로 \
                 나타냅니다."
            }
        }
    }
}

/// 시스템 메시지 미리보기: `format` 형식에 커서 표시가 있을 때의 고정 규약(①)과, 사용자 지정 지침(②)이
/// 들어갈 자리. `has_tools`면 MCP 도구 규칙도 넣는다. GUI의 설정 창에 보인다.
pub fn system_preview(format: DocFormat, has_tools: bool) -> String {
    build_system(
        format,
        true,
        has_tools,
        "(아래 칸에 쓴 사용자 지정 지침이 이 자리에 들어갑니다. 비우면 이 부분은 빠집니다.)",
    )
}

/// 시스템 메시지: ① 코드가 정하는 규약 지시(역할, 답변 규칙, 문서 형식 설명, 커서 표시 규칙, 도구
/// 규칙)와 ② 사용자가 GUI에서 고치는 사용자 지정 지침(`custom_system`, 비어 있으면 붙이지 않음).
///
/// features(CLAUDE.md): 양식(제목/소제목/문단 구조)을 최대한 준수하고, 인접 제목/소제목
/// 레벨에 상응하도록 작성한다. 답변은 문서에 그대로 넣을 본문이어야 하므로 설명·인사말·마크다운을
/// 붙이지 말라고 지시한다. `has_caret`이면 문서 안의 [`parse::CARET_MARK`]가 커서 위치라고 알린다.
/// `has_tools`(MCP 서버가 설정됨)면 도구로 사실·근거를 찾되 그 과정은 답에 쓰지 말라고 알린다.
///
/// 매 요청 같은 문장을 앞에 두고 형식·커서에 따라 바뀌는 문장을 뒤에 둔다. 앞부분이 같으면 로컬
/// 서버가 이전 계산을 재사용할 여지가 있다. 규약 부분은 코드가 갖고 사용자가 고칠 수 없다(바꾸면
/// 결과를 문서에 넣는 흐름이 깨진다).
fn build_system(format: DocFormat, has_caret: bool, has_tools: bool, custom_system: &str) -> String {
    let mut system = String::from(
        "당신은 HWP 문서 작성 도우미입니다. 사용자가 보낸 문서의 양식(제목/소제목/문단 구조)을 최대한 \
         준수하여, 사용자 요청에 맞는 내용을 한국어로 작성하세요.\n\
         - 사용자 메시지는 \"=== 문서 내용 ===\"과 \"=== 사용자 요청 ===\" 두 부분으로 되어 있습니다.\n\
         - 답변은 문서에 바로 들어갈 본문만 씁니다. 인사말, 설명, 앞뒤 안내 문장, 마크다운 기호\
         (**, #, ---)를 붙이지 않습니다.\n",
    );
    // ② 사용자 지정 지침: GUI에서 사용자가 고친다. 자주 바뀌지 않으므로 규약 바로 뒤에 둔다.
    let custom_system = custom_system.trim();
    if !custom_system.is_empty() {
        system.push_str(&format!(
            "\n[사용자 지정 지침] 이 메시지의 다른 규칙과 어긋나지 않는 범위에서 따릅니다.\n\
             {custom_system}\n\n"
        ));
    }
    system.push_str(&format!("- {}\n", format.describe()));
    if has_caret {
        system.push_str(&format!(
            "- 문서 내용 안의 {mark} 표시가 사용자의 커서 위치입니다. 사용자 요청의 \"커서 자리\", \
             \"이 자리\", \"여기\"는 이 위치를 뜻합니다.\n\
             - 답변은 {mark} 자리에 그대로 넣을 글만 씁니다. {mark} 표시는 답변에 넣지 않습니다.\n",
            mark = parse::CARET_MARK
        ));
    }
    if has_tools {
        system.push_str(
            "- 도구가 주어지면, 요청에 필요한 사실과 근거를 도구로 찾아 확인한 뒤 작성합니다. 도구를              부른 과정이나 출처 설명은 답변에 쓰지 않습니다.\n",
        );
    }
    system
}

/// 사용자 메시지: 요청마다 바뀌는 데이터(문서 내용, 사용자 요청)만 담는다.
fn build_user(doc_context: &str, user_prompt: &str) -> String {
    format!("=== 문서 내용 ===\n{doc_context}\n\n=== 사용자 요청 ===\n{user_prompt}\n")
}
