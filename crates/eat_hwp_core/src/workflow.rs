//! eat_hwp_raw 워크플로 오케스트레이션.
//!
//! 파이프라인: HWPML 파싱 → 컨텍스트+프롬프트 구성 → LLM 실행.
//! 목표 구조(CLAUDE.md Workflow)에서는 이 단계를 `eat_hwp_server`가 맡는다.
//! (결과 입력은 후속 증분에서 연결한다.)

use crate::config::Config;
use crate::llm::LlmBackend;
use crate::parse;
use anyhow::Result;

/// 문서의 HWPML과 사용자 프롬프트로 LLM 응답을 생성한다.
///
/// LLM에는 항상 문서 전체를 보낸다. `caret`은 `hwpml` 안에서 커서가 있는 바이트 위치(없으면
/// `None`)이고, 그 자리에 [`parse::CARET_MARK`]를 넣어 LLM이 커서 위치를 알게 한다.
pub fn run_prompt(
    _cfg: &Config,
    backend: &dyn LlmBackend,
    hwpml: &str,
    user_prompt: &str,
    caret: Option<usize>,
) -> Result<String> {
    let doc = parse::parse_hwpml(hwpml)?;

    // 커서 정보가 없으면(예: EAT_HWP_DOC 테스트 경로) 표시 없이 문서 전체만 보낸다.
    let context = parse::document_text(&doc, caret);
    let has_caret = context.contains(parse::CARET_MARK);

    let prompt = build_prompt(&context, user_prompt, has_caret);
    backend.complete(&prompt)
}

/// 문서 양식 준수 지침을 포함한 프롬프트를 구성한다.
///
/// features(CLAUDE.md): 양식(제목/소제목/문단 구조)을 최대한 준수하고, 인접 제목/소제목
/// 레벨에 상응하도록 작성한다.
///
/// 답변은 문서에 그대로 넣을 본문이어야 하므로, 설명·인사말·마크다운을 붙이지 말라고 지시한다.
/// `has_caret`이면 문서 안의 [`parse::CARET_MARK`]가 커서 위치라는 것을 알려 준다.
fn build_prompt(doc_context: &str, user_prompt: &str, has_caret: bool) -> String {
    let caret_rule = if has_caret {
        format!(
            "- 문서 내용 안의 {mark} 표시가 사용자의 커서 위치입니다. 사용자 요청의 \"커서 자리\", \
             \"이 자리\", \"여기\"는 이 위치를 뜻합니다.\n\
             - 답변은 {mark} 자리에 그대로 넣을 글만 씁니다. {mark} 표시는 답변에 넣지 않습니다.\n",
            mark = parse::CARET_MARK
        )
    } else {
        String::new()
    };
    format!(
        "당신은 HWP 문서 작성 도우미입니다. 아래 문서의 양식(제목/소제목/문단 구조)을 최대한 \
         준수하여, 사용자 요청에 맞는 내용을 한국어로 작성하세요.\n\
         {caret_rule}\
         - 답변은 문서에 바로 들어갈 본문만 씁니다. 인사말, 설명, 앞뒤 안내 문장, 마크다운 기호\
         (**, #, ---)를 붙이지 않습니다.\n\n\
         === 문서 내용 ===\n{doc_context}\n\n\
         === 사용자 요청 ===\n{user_prompt}\n"
    )
}
