#![forbid(unsafe_code)]
//! eat_hwp_core — eat_hwp_raw의 GUI·파이프라인 라이브러리.
//!
//! 현재 HWP 문서의 양식·내용·기초자료를 바탕으로 문서를 작성/수정/완성하는 파이프라인
//! (config/hwp/parse/llm/workflow)과 winsafe GUI를 제공한다.
//!
//! 별도 실행파일은 없다. 애드인 DLL(`eat_hwp_addon`)이 HWP 프로세스 안에서 단축키/버튼으로
//! rundll32를 spawn하고, 그 **독립 프로세스**가 [`run_gui`]를 호출해 GUI를 띄운다.
//!
//! 워크플로(eat_hwp_raw.md): GUI에 프롬프트 입력 → hwpx 컨텍스트 파싱 → rig LLM 실행 →
//! 답변 표시 + 완료 알람.

mod config;
mod gui;
mod hwp;
mod llm;
mod parse;
mod workflow;

use anyhow::{Result, anyhow};
use std::sync::Arc;

/// GUI를 띄우고 워크플로를 구동한다.
///
/// 창이 닫힐 때까지 블로킹(자체 메시지 루프)하므로, HWP UI 스레드를 막지 않도록 애드인이
/// rundll32로 띄운 **독립 프로세스**에서 실행한다. GUI는 실행 중인 HWP에 COM으로 재접속한다.
pub fn run_gui() -> Result<()> {
    let cfg = config::Config::load();
    let backend: Arc<dyn llm::LlmBackend + Send + Sync> = Arc::new(llm::RigBackend::new(&cfg)?);

    let window = gui::MainWindow::new(cfg, backend);
    window.run().map_err(|e| anyhow!("GUI 실행 오류: {e}"))?;
    Ok(())
}
