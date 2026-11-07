//! eat_hwp_raw 워크플로 오케스트레이션.
//!
//! 파이프라인: HWPML 파싱 → 컨텍스트+프롬프트 구성 → LLM 실행.
//! 목표 구조(CLAUDE.md Workflow)에서는 이 단계를 `eat_hwp_server`가 맡는다.
//! (결과 입력은 후속 증분에서 연결한다.)

use crate::config::Config;
use crate::llm::LlmBackend;
use crate::parse;
use anyhow::Result;

/// 컨텍스트 범위 (GUI의 "현재문단/전체문서" 체크버튼에 대응).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// 현재 커서가 위치한 문단(및 인접 제목/소제목).
    CurrentParagraph,
    /// 전체 문서.
    WholeDocument,
}

/// 현재 문단 범위 컨텍스트에 포함할 직전/직후 문단 수.
/// 직전·직후 문단(인접 제목/소제목)을 함께 제공해 양식에 상응하도록 한다(features).
const CONTEXT_BEFORE: usize = 3;
const CONTEXT_AFTER: usize = 3;

/// 문서의 HWPML과 사용자 프롬프트로 LLM 응답을 생성한다.
///
/// `caret`은 `hwpml` 안에서 커서가 있는 바이트 위치(없으면 `None`). `Scope::CurrentParagraph`일
/// 때만 쓰인다.
pub fn run_prompt(
    _cfg: &Config,
    backend: &dyn LlmBackend,
    hwpml: &str,
    user_prompt: &str,
    scope: Scope,
    caret: Option<usize>,
) -> Result<String> {
    let doc = parse::parse_hwpml(hwpml)?;

    let caret_paras = match scope {
        Scope::CurrentParagraph => caret.and_then(|at| parse::caret_paragraphs(&doc, at)),
        Scope::WholeDocument => None,
    };
    let context = match caret_paras {
        Some((paras, center)) => current_paragraph_context(&paras, center),
        // 전체 문서이거나, 커서 정보가 없는 경우(예: EAT_HWP_DOC 테스트 경로)는 전체 문서로 폴백.
        None => parse::document_text(&doc),
    };

    let prompt = build_prompt(&context, user_prompt);
    backend.complete(&prompt)
}

/// 커서 문단과 그 직전/직후 문단을 묶어 로컬 컨텍스트를 만든다.
///
/// 대상(현재) 문단은 `>>> ` 표시로 구분해 LLM이 수정/작성 대상 문단을 인지하도록 한다.
/// `center`는 `paras` 안의 인덱스여야 한다.
fn current_paragraph_context(paras: &[&str], center: usize) -> String {
    let start = center.saturating_sub(CONTEXT_BEFORE);
    let end = (center + CONTEXT_AFTER + 1).min(paras.len());

    paras[start..end]
        .iter()
        .enumerate()
        .map(|(i, para)| {
            if start + i == center {
                format!(">>> {para}")
            } else {
                para.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 문서 양식 준수 지침을 포함한 프롬프트를 구성한다.
///
/// features(CLAUDE.md): 양식(제목/소제목/문단 구조)을 최대한 준수하고, 인접 제목/소제목
/// 레벨에 상응하도록 작성한다.
fn build_prompt(doc_context: &str, user_prompt: &str) -> String {
    format!(
        "당신은 HWP 문서 작성 도우미입니다. 아래 문서의 양식(제목/소제목/문단 구조)을 최대한 \
         준수하여, 사용자 요청에 맞는 내용을 한국어로 작성하세요.\n\n\
         === 문서 내용 ===\n{doc_context}\n\n\
         === 사용자 요청 ===\n{user_prompt}\n"
    )
}
