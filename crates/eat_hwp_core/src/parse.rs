//! HWPML 파싱: `GetTextFile("HWPML2X")`가 돌려준 XML에서 문단 텍스트를 추출한다.
//!
//! HWPML 2.x 구조: `HWPML > BODY > SECTION > P > TEXT > CHAR`. 표·글상자 등의 안쪽 문단은
//! 바깥 문단의 `TEXT` 아래에 다시 `P`로 중첩된다.

use anyhow::{Result, anyhow};
use roxmltree::{Node, ParsingOptions};
use std::ops::Range;

/// LLM에 보내는 문서 텍스트 안에서 커서 위치를 나타내는 표시.
pub const CARET_MARK: &str = "[[커서]]";

/// HWPML에서 추출한 문단 텍스트.
pub struct HwpmlDocument {
    /// 표 셀 등 중첩 문단까지 포함한 모든 문단, 문서 순서.
    paras: Vec<Paragraph>,
}

struct Paragraph {
    text: String,
    /// 문단 텍스트를 이루는 조각과 그 조각이 HWPML 문자열 안에서 차지하는 바이트 범위.
    /// 커서 표시를 글자 사이에 끼워 넣을 때 쓴다.
    pieces: Vec<Piece>,
    /// HWPML 문자열 안에서 `P` 요소가 차지하는 바이트 범위.
    range: Range<usize>,
}

struct Piece {
    range: Range<usize>,
    text: String,
    /// XML 텍스트 노드면 `true`. 이때만 조각 안쪽에 커서가 올 수 있다.
    is_text: bool,
    /// 텍스트 노드의 원문(엔티티를 풀기 전). 텍스트 노드가 아니면 빈 문자열.
    raw: String,
}

/// HWPML 문자열을 파싱한다.
pub fn parse_hwpml(hwpml: &str) -> Result<HwpmlDocument> {
    let options = ParsingOptions {
        allow_dtd: true,
        ..ParsingOptions::default()
    };
    let xml = roxmltree::Document::parse_with_options(hwpml, options)
        .map_err(|e| anyhow!("HWPML 파싱 실패: {e}"))?;

    let mut doc = HwpmlDocument { paras: Vec::new() };
    for section in xml.descendants().filter(|n| n.has_tag_name("SECTION")) {
        collect_paragraphs(section, &mut doc);
    }
    Ok(doc)
}

/// 문서 전체 텍스트를 추출한다. (프롬프트 컨텍스트용)
///
/// `caret`(HWPML 문자열 안의 바이트 위치)이 어느 문단 안에 있으면 그 자리에 [`CARET_MARK`]를
/// 끼워 넣는다.
pub fn document_text(doc: &HwpmlDocument, caret: Option<usize>) -> String {
    let marked = caret.and_then(|at| caret_target(doc, at).map(|i| (i, marked_text(&doc.paras[i], at))));
    let texts: Vec<&str> = doc
        .paras
        .iter()
        .enumerate()
        .map(|(i, p)| match &marked {
            Some((target, text)) if *target == i => text.as_str(),
            _ => p.text.as_str(),
        })
        .collect();
    texts.join("\n")
}

/// 커서가 든 가장 안쪽 문단의 인덱스.
fn caret_target(doc: &HwpmlDocument, caret: usize) -> Option<usize> {
    // 중첩 문단은 바깥 문단보다 뒤에 나오므로, 범위에 드는 마지막 문단이 가장 안쪽이다.
    doc.paras.iter().rposition(|p| p.range.contains(&caret))
}

/// 문단 텍스트의 커서 자리에 [`CARET_MARK`]를 끼워 넣는다.
///
/// 커서가 글자 사이(텍스트 노드 안쪽)면 그 자리에, 요소 경계면 그 앞 조각까지 쓴 뒤에 넣는다.
fn marked_text(para: &Paragraph, caret: usize) -> String {
    let mut out = String::new();
    let mut placed = false;
    for piece in &para.pieces {
        if !placed && caret <= piece.range.start {
            out.push_str(CARET_MARK);
            placed = true;
        }
        if !placed && piece.is_text && piece.range.contains(&caret) {
            let before = decode_xml_text(&para_source_slice(piece, caret));
            match piece.text.strip_prefix(before.as_str()) {
                Some(after) => {
                    out.push_str(&before);
                    out.push_str(CARET_MARK);
                    out.push_str(after);
                }
                // 원문과 맞춰지지 않으면 조각 뒤에 둔다.
                None => {
                    out.push_str(&piece.text);
                    out.push_str(CARET_MARK);
                }
            }
            placed = true;
            continue;
        }
        out.push_str(&piece.text);
    }
    if !placed {
        out.push_str(CARET_MARK);
    }
    out
}

/// 텍스트 조각의 원문 중 커서 앞부분.
fn para_source_slice(piece: &Piece, caret: usize) -> String {
    let end = caret - piece.range.start;
    match piece.raw.get(..end) {
        Some(before) => before.to_string(),
        None => piece.raw.clone(),
    }
}

/// XML 텍스트 원문의 엔티티(`&lt;` 등, 숫자 참조)를 풀어 쓴다.
fn decode_xml_text(raw: &str) -> String {
    let mut out = String::new();
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let tail = &rest[amp..];
        let Some(semi) = tail.find(';') else {
            out.push_str(tail);
            return out;
        };
        let entity = &tail[1..semi];
        let decoded = match entity {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => out.push(c),
            None => out.push_str(&tail[..=semi]),
        }
        rest = &tail[semi + 1..];
    }
    out.push_str(rest);
    out
}

/// `node` 아래의 `P`를 문서 순서로 모은다. 중첩 문단은 바깥 문단 바로 뒤에 온다.
fn collect_paragraphs(node: Node, doc: &mut HwpmlDocument) {
    for child in node.children().filter(Node::is_element) {
        if child.has_tag_name("P") {
            let pieces = paragraph_pieces(child);
            doc.paras.push(Paragraph {
                text: pieces.iter().map(|p| p.text.as_str()).collect(),
                pieces,
                range: child.range(),
            });
            collect_paragraphs(child, doc);
        } else {
            collect_paragraphs(child, doc);
        }
    }
}

/// 문단 자신의 텍스트 조각(`P > TEXT > CHAR`). 중첩 문단의 텍스트는 포함하지 않는다.
fn paragraph_pieces(para: Node) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let chars = para
        .children()
        .filter(|n| n.has_tag_name("TEXT"))
        .flat_map(|t| t.children())
        .filter(|n| n.has_tag_name("CHAR"));
    for node in chars.flat_map(|c| c.children()) {
        let range = node.range();
        if node.is_text() {
            let raw = node.document().input_text()[range.clone()].to_string();
            pieces.push(Piece {
                range,
                text: node.text().unwrap_or_default().to_string(),
                is_text: true,
                raw,
            });
        } else {
            let text = match node.tag_name().name() {
                "TAB" => "\t",
                "LINEBREAK" => "\n",
                "NBSPACE" | "FWSPACE" => " ",
                "HYPHEN" => "-",
                _ => "",
            };
            pieces.push(Piece {
                range,
                text: text.to_string(),
                is_text: false,
                raw: String::new(),
            });
        }
    }
    pieces
}
