//! 대화 기록 저장: 문서와 API 종류마다 파일 하나.
//!
//! 위치는 `%USERPROFILE%\.eat_hwp_raw\chats`이고, 파일 이름은
//! `{문서 파일 이름}_{문서 전체 경로의 해시}_{api}.txt`다. 같은 이름의 문서가 다른 폴더에 있어도
//! 해시로 구분한다. 한 번도 저장하지 않은 문서는 경로가 없어 모두 `제목없음_{api}.txt`를 함께 쓴다.
//! 내용은 GUI 대화 칸의 글 그대로(UTF-8, 줄바꿈 `\r\n`)라 메모장으로도 읽을 수 있다.

use crate::config::Provider;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// 대화 기록 디렉터리. `USERPROFILE`이 없으면 `None`.
pub fn chat_dir() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE")?;
    Some(Path::new(&home).join(".eat_hwp_raw").join("chats"))
}

/// 문서 경로(저장 안 된 문서는 빈 문자열)와 API 종류에 해당하는 기록 파일.
pub fn chat_file(doc_path: &str, provider: Provider) -> Option<PathBuf> {
    let api = match provider {
        Provider::OpenAiCompatible => "openai",
        Provider::Anthropic => "claude",
    };
    let name = if doc_path.trim().is_empty() {
        format!("제목없음_{api}.txt")
    } else {
        // 한글이 주는 경로는 Windows 형식이라 두 구분자를 모두 본다.
        let file_name = doc_path.rsplit(['\\', '/']).next().unwrap_or(doc_path);
        // Windows 경로는 대소문자를 가리지 않으므로 소문자로 해시한다.
        let hash = fnv1a32(doc_path.to_lowercase().as_bytes());
        format!("{}_{hash:08x}_{api}.txt", sanitize(file_name))
    };
    Some(chat_dir()?.join(name))
}

/// 기록 파일을 읽는다. 없으면 빈 문자열.
pub fn load(file: &Path) -> String {
    std::fs::read_to_string(file).unwrap_or_default()
}

/// 기록 파일을 통째로 쓴다. 디렉터리가 없으면 만든다.
pub fn save(file: &Path, text: &str) -> Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("대화 기록 디렉터리를 만들지 못함: {}", dir.display()))?;
    }
    std::fs::write(file, text)
        .with_context(|| format!("대화 기록을 저장하지 못함: {}", file.display()))
}

/// 파일 이름에 쓸 수 없는 문자를 `_`로 바꾼다.
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect()
}

/// FNV-1a 32비트. Rust 버전이 바뀌어도 같은 값을 내야 파일 이름이 유지된다.
fn fnv1a32(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c_9dc5, |hash, b| {
        (hash ^ u32::from(*b)).wrapping_mul(0x0100_0193)
    })
}
