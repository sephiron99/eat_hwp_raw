//! 사용자 설정 파일. 위치는 `%USERPROFILE%\.eat_hwp_raw`다(대화 기록은 그 아래 `chats`).
//!
//! - `system_prompt.txt`: 사용자 지정 시스템 프롬프트
//! - `settings.txt`: GUI 설정 창의 서버 주소·모델·문서 형식. API 키는 저장하지 않는다

use crate::workflow::DocFormat;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// 설정 디렉터리 `%USERPROFILE%\.eat_hwp_raw`. `USERPROFILE`이 없으면 `None`.
pub fn settings_dir() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE")?;
    Some(Path::new(&home).join(".eat_hwp_raw"))
}

/// 사용자 지정 시스템 프롬프트 파일 `%USERPROFILE%\.eat_hwp_raw\system_prompt.txt`(UTF-8).
pub fn custom_system_prompt_file() -> Option<PathBuf> {
    Some(settings_dir()?.join("system_prompt.txt"))
}

/// 사용자 지정 시스템 프롬프트. 파일이 없거나 읽지 못하면 빈 문자열. 줄바꿈은 `\n`.
pub fn load_custom_system_prompt() -> String {
    custom_system_prompt_file()
        .and_then(|file| std::fs::read_to_string(file).ok())
        .map(|text| text.replace("\r\n", "\n"))
        .unwrap_or_default()
}

/// 사용자 지정 시스템 프롬프트를 저장한다. 앞뒤 공백을 지우고 줄바꿈을 `\n`으로 맞춘다.
pub fn save_custom_system_prompt(text: &str) -> Result<()> {
    let file = custom_system_prompt_file()
        .context("USERPROFILE이 없어 시스템 프롬프트를 저장할 곳을 정하지 못함")?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("설정 디렉터리를 만들지 못함: {}", dir.display()))?;
    }
    std::fs::write(&file, text.replace("\r\n", "\n").trim())
        .with_context(|| format!("시스템 프롬프트를 저장하지 못함: {}", file.display()))
}

/// GUI 설정 파일 `%USERPROFILE%\.eat_hwp_raw\settings.txt`(UTF-8, 한 줄에 `이름=값`).
pub fn gui_settings_file() -> Option<PathBuf> {
    Some(settings_dir()?.join("settings.txt"))
}

/// GUI 설정 창에서 저장하는 값. 빈 값은 환경 변수 설정을 쓴다는 뜻이다. API 키는 두지 않는다.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuiSettings {
    /// 로컬/OpenAI 호환 서버 주소(`EAT_HWP_LLM_BASE_URL` 대신).
    pub base_url: String,
    /// 모델(`EAT_HWP_LLM_MODEL` 대신).
    pub model: String,
    /// LLM에 보낼 문서 형식. 없으면 기본 형식.
    pub format: Option<DocFormat>,
}

/// GUI 설정을 읽는다. 파일이 없거나 읽지 못하면 빈 설정. 모르는 이름과 `#`으로 시작하는 줄은 건너뛴다.
pub fn load_gui_settings() -> GuiSettings {
    let text = gui_settings_file()
        .and_then(|file| std::fs::read_to_string(file).ok())
        .unwrap_or_default();
    parse_gui_settings(&text)
}

fn parse_gui_settings(text: &str) -> GuiSettings {
    let mut settings = GuiSettings::default();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match name.trim() {
            "base_url" => settings.base_url = value.to_string(),
            "model" => settings.model = value.to_string(),
            "format" => settings.format = DocFormat::from_id(value),
            _ => {}
        }
    }
    settings
}

/// GUI 설정을 저장한다.
pub fn save_gui_settings(settings: &GuiSettings) -> Result<()> {
    let file = gui_settings_file().context("USERPROFILE이 없어 설정을 저장할 곳을 정하지 못함")?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("설정 디렉터리를 만들지 못함: {}", dir.display()))?;
    }
    let mut text = String::from("# eat_hwp_raw GUI 설정. 빈 값은 환경 변수 설정을 쓴다. API 키는 저장하지 않는다.\n");
    text.push_str(&format!("base_url={}\n", settings.base_url.trim()));
    text.push_str(&format!("model={}\n", settings.model.trim()));
    if let Some(format) = settings.format {
        text.push_str(&format!("format={}\n", format.id()));
    }
    std::fs::write(&file, text).with_context(|| format!("설정을 저장하지 못함: {}", file.display()))
}
