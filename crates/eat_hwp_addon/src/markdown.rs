//! HWPX → Markdown 변환(hwp2md). 사용자가 LLM에 보낼 문서 형식으로 Markdown을 고른 경우에 쓴다.
//!
//! hwp2md는 책갈피를 읽지 않으므로, 커서 자리 책갈피(`ExtDoc::hwpx_with_caret`이 넣은 것)를
//! [`CARET_MARK`] 글자로 바꾼 HWPX 사본을 만들어 변환한다.

use crate::parse::CARET_MARK;
use anyhow::{Context, Result, anyhow};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// HWPX 파일을 Markdown으로 바꾼다. `caret_bookmark`가 있으면 그 책갈피 자리에 [`CARET_MARK`]를 둔다.
///
/// 책갈피를 찾지 못하면 표시 없이 바꾼다(결과에 [`CARET_MARK`]가 없는지로 알 수 있다).
pub fn hwpx_to_markdown(hwpx: &Path, caret_bookmark: Option<&str>) -> Result<String> {
    let Some(bookmark) = caret_bookmark else {
        return convert(hwpx);
    };
    let marked = with_caret_text(hwpx, bookmark)?;
    let result = convert(&marked);
    let _ = std::fs::remove_file(&marked);
    result
}

fn convert(hwpx: &Path) -> Result<String> {
    let doc = hwp2md::hwpx::read_hwpx(hwpx)
        .map_err(|e| anyhow!("HWPX를 Markdown으로 바꾸지 못함: {}: {e}", hwpx.display()))?;
    let md = hwp2md::md::write_markdown(&doc, false);
    tracing::debug!(hwpx = %hwpx.display(), md_chars = md.chars().count(), "hwp2md 변환");
    // Markdown 작성기가 대괄호를 이스케이프하면 원래 표시로 되돌린다.
    let escaped = CARET_MARK.replace('[', "\\[").replace(']', "\\]");
    Ok(md.replace(&escaped, CARET_MARK))
}

/// 책갈피를 [`CARET_MARK`] 글자로 바꾼 HWPX 사본을 원본 옆에 만든다.
fn with_caret_text(src: &Path, bookmark: &str) -> Result<PathBuf> {
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("caret");
    let dst = src.with_file_name(format!("{stem}_md.hwpx"));
    let mut archive = zip::ZipArchive::new(
        File::open(src).with_context(|| format!("HWPX를 열지 못함: {}", src.display()))?,
    )
    .map_err(|e| anyhow!("HWPX(zip)를 읽지 못함: {e}"))?;
    let mut writer = zip::ZipWriter::new(
        File::create(&dst).with_context(|| format!("HWPX 사본을 만들지 못함: {}", dst.display()))?,
    );
    let mut replaced = false;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| anyhow!("HWPX 항목을 읽지 못함: {e}"))?;
        let name = entry.name().to_string();
        let options =
            zip::write::SimpleFileOptions::default().compression_method(entry.compression());
        if entry.is_dir() {
            writer
                .add_directory(name, options)
                .map_err(|e| anyhow!("HWPX 사본을 쓰지 못함: {e}"))?;
            continue;
        }
        let mut data = Vec::new();
        entry.read_to_end(&mut data)?;
        if !replaced && name.ends_with(".xml") {
            if let Some(xml) = std::str::from_utf8(&data)
                .ok()
                .and_then(|xml| replace_bookmark(xml, bookmark))
            {
                data = xml.into_bytes();
                replaced = true;
            }
        }
        writer
            .start_file(name, options)
            .map_err(|e| anyhow!("HWPX 사본을 쓰지 못함: {e}"))?;
        writer.write_all(&data)?;
    }
    writer
        .finish()
        .map_err(|e| anyhow!("HWPX 사본을 마무리하지 못함: {e}"))?;
    if replaced {
        tracing::debug!(bookmark, "커서 책갈피를 표시 글자로 바꿈");
    } else {
        tracing::warn!(bookmark, "HWPX에서 커서 책갈피를 찾지 못함 — 커서 표시 없이 변환");
    }
    Ok(dst)
}

/// `name="{bookmark}"`인 책갈피 요소를 `<접두어:t>[[커서]]</접두어:t>`로 바꾼다.
///
/// HWPX(OWPML)에서 책갈피는 글자 묶음(`run`) 안의 `<hp:ctrl><hp:bookmark name=".."/></hp:ctrl>`이다.
/// 감싼 `ctrl`까지 바꿔 글자(`t`)가 `run` 바로 아래에 오게 한다. 찾지 못하면 `None`.
fn replace_bookmark(xml: &str, bookmark: &str) -> Option<String> {
    let at = xml.find(&format!("name=\"{bookmark}\""))?;
    let start = xml[..at].rfind('<')?;
    let tag: String = xml[start + 1..]
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '/' && *c != '>')
        .collect();
    let prefix = tag.split_once(':').map(|(p, _)| format!("{p}:")).unwrap_or_default();

    let gt = at + xml[at..].find('>')?;
    let end = if xml[..gt].ends_with('/') {
        gt + 1
    } else {
        let close = format!("</{tag}>");
        gt + 1 + xml[gt + 1..].find(&close)? + close.len()
    };

    let ctrl_open = format!("<{prefix}ctrl>");
    let ctrl_close = format!("</{prefix}ctrl>");
    let before = xml[..start].trim_end();
    let after = xml[end..].trim_start();
    let (cut_start, cut_end) = if before.ends_with(&ctrl_open) && after.starts_with(&ctrl_close) {
        (
            before.len() - ctrl_open.len(),
            xml.len() - after.len() + ctrl_close.len(),
        )
    } else {
        (start, end)
    };
    Some(format!(
        "{}<{prefix}t>{CARET_MARK}</{prefix}t>{}",
        &xml[..cut_start],
        &xml[cut_end..]
    ))
}
