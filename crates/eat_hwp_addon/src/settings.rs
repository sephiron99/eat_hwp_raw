//! 사용자 설정 파일. 위치는 `%USERPROFILE%\.eat_hwp_raw`다(대화 기록은 그 아래 `chats`).

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
