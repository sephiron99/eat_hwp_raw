#![forbid(unsafe_code)]
//! eat_hwp_core — eat_hwp_raw의 GUI·파이프라인 라이브러리.
//!
//! **삭제 대상이다.** 목표 구조(CLAUDE.md "구조 / 산출물")에서 GUI는 `eat_hwp_addon`이 갖고,
//! HWPML 파싱·프롬프트 구성·LLM 호출은 `eat_hwp_server`가 맡는다. 아래는 현재 구현의 설명이다.
//!
//! LLM이 문서 전체 구조와 내용을 읽고, 커서가 위치한 곳에 들어갈 글을 폰트도 맞춰서 입력하기
//! 위한 파이프라인(config/hwp/parse/llm/workflow)과 winsafe GUI를 제공한다.
//!
//! 별도 실행파일은 없다. 애드온 DLL(`eat_hwp_addon`)이 HWP 프로세스 안에서 단축키/버튼으로
//! rundll32를 spawn하고, 그 **독립 프로세스**가 [`run_gui`]를 호출해 GUI를 띄운다.
//!
//! 현재 구현의 흐름: GUI에 프롬프트 입력 → HWPML 컨텍스트 파싱 → rig LLM 실행 →
//! 답변 표시 + 완료 알람.

mod chat_store;
mod config;
mod gui;
mod hwp;
mod llm;
mod markdown;
mod parse;
mod workflow;

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
