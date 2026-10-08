//! eat_hwp_raw 워크플로 오케스트레이션.
//!
//! 파이프라인(eat_hwp_raw.md Workflow): hwpx 파싱 → 컨텍스트+프롬프트 구성 → LLM 실행.
//! (라이브 컨텍스트 확보/SaveAs/결과 입력은 hwp 모듈 및 후속 증분에서 연결한다.)

use crate::config::Config;
use crate::llm::LlmBackend;
use crate::parse;
use anyhow::Result;
use std::path::Path;

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

/// hwpx 파일과 사용자 프롬프트로 LLM 응답을 생성한다.
///
/// `cursor_para`는 본문 커서 문단 인덱스(없으면 `None`). `Scope::CurrentParagraph`일 때만 쓰인다.
pub fn run_prompt(
    _cfg: &Config,
    backend: &dyn LlmBackend,
    hwpx_path: &Path,
    user_prompt: &str,
    scope: Scope,
    cursor_para: Option<usize>,
) -> Result<String> {
    let doc = parse::parse_hwpx(hwpx_path)?;

    let context = match (scope, cursor_para) {
        (Scope::CurrentParagraph, Some(center)) => {
            current_paragraph_context(&parse::paragraph_texts(&doc), center)
        }
        // 전체 문서이거나, 커서 정보가 없는 경우(예: EAT_HWP_DOC 테스트 경로)는 전체 문서로 폴백.
        _ => parse::document_text(&doc),
    };

    let prompt = build_prompt(&context, user_prompt);
    backend.complete(&prompt)
}

/// 커서 문단과 그 직전/직후 문단을 묶어 로컬 컨텍스트를 만든다.
///
/// 대상(현재) 문단은 `>>> ` 표시로 구분해 LLM이 수정/작성 대상 문단을 인지하도록 한다.
/// (단일 섹션 문서 기준. 다중 섹션에서는 본문 문단 순서가 GetPos para와 어긋날 수 있어 인덱스를 clamp.)
fn current_paragraph_context(paras: &[String], center: usize) -> String {
    if paras.is_empty() {
        return String::new();
    }
    let center = center.min(paras.len() - 1);
    let start = center.saturating_sub(CONTEXT_BEFORE);
    let end = (center + CONTEXT_AFTER + 1).min(paras.len());

    paras[start..end]
        .iter()
        .enumerate()
        .map(|(i, para)| {
            if start + i == center {
                format!(">>> {para}")
            } else {
                para.clone()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 문서 양식 준수 지침을 포함한 프롬프트를 구성한다.
///
/// features(eat_hwp_raw.md): 양식(제목/소제목/문단 구조)을 최대한 준수하고, 인접 제목/소제목
/// 레벨에 상응하도록 작성한다.
fn build_prompt(doc_context: &str, user_prompt: &str) -> String {
    format!(
        "당신은 HWP 문서 작성 도우미입니다. 아래 문서의 양식(제목/소제목/문단 구조)을 최대한 \
         준수하여, 사용자 요청에 맞는 내용을 한국어로 작성하세요.\n\n\
         === 문서 내용 ===\n{doc_context}\n\n\
         === 사용자 요청 ===\n{user_prompt}\n"
    )
}
