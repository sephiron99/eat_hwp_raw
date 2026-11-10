//! GUI 프로세스의 진입: 설정을 읽고 GUI를 띄운다.
//!
//! 애드온이 한글 안에서 단축키/버튼으로 rundll32를 spawn하면, 그 **독립 프로세스**가 `RunGuiW`를 거쳐
//! [`run_gui`]를 부른다. GUI 프로세스가 문서를 읽고(hwp), LLM에 보낼 텍스트를 만들고(parse, markdown,
//! workflow), LLM을 직접 호출한다(llm).

use crate::{chat_store, config, gui};
use anyhow::{Result, anyhow};
use std::path::PathBuf;

/// 로그 파일을 둘 디렉터리. 대화 기록과 같은 `%USERPROFILE%\.eat_hwp_raw\chats`다.
/// `USERPROFILE`이 없으면 `None`.
pub fn log_dir() -> Option<PathBuf> {
    chat_store::chat_dir()
}

/// GUI를 띄우고 워크플로를 구동한다.
///
/// 창이 닫힐 때까지 블로킹(자체 메시지 루프)하므로, HWP UI 스레드를 막지 않도록 애드온이
/// rundll32로 띄운 **독립 프로세스**에서 실행한다. GUI는 실행 중인 HWP에 COM으로 재접속한다.
pub fn run_gui() -> Result<()> {
    let cfg = config::Config::load().map_err(|e| anyhow!("설정 오류: {e}"))?;
    // 키는 남기지 않고 있는지 여부만 남긴다.
    tracing::info!(
        provider = ?cfg.llm_provider,
        model = %cfg.llm_model,
        base_url = %cfg.llm_base_url,
        has_key = !cfg.llm_api_key.is_empty(),
        "설정 읽음"
    );
    let window = gui::MainWindow::new(cfg);
    window.run().map_err(|e| anyhow!("GUI 실행 오류: {e}"))?;
    Ok(())
}
