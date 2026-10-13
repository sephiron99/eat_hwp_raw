//! HWPML 파싱: `GetTextFile("HWPML2X")`가 돌려준 XML에서 문단 텍스트를 추출한다.
//!
//! HWPML 2.x 구조: `HWPML > BODY > SECTION > P > TEXT > CHAR`. 표·글상자 등의 안쪽 문단은
//! 바깥 문단의 `TEXT` 아래에 다시 `P`로 중첩된다.

use anyhow::{Result, anyhow};
use roxmltree::{Node, ParsingOptions};

/// HWPML에서 추출한 문단 텍스트.
pub struct HwpmlDocument {
    /// 본문 문단(`SECTION` 바로 아래 `P`)의 텍스트, 문서 순서.
    body: Vec<String>,
    /// 표 셀 등 중첩 문단까지 포함한 모든 문단의 텍스트, 문서 순서.
    all: Vec<String>,
}

/// HWPML 문자열을 파싱한다.
pub fn parse_hwpml(hwpml: &str) -> Result<HwpmlDocument> {
    let options = ParsingOptions {
        allow_dtd: true,
        ..ParsingOptions::default()
    };
    let xml = roxmltree::Document::parse_with_options(hwpml, options)
        .map_err(|e| anyhow!("HWPML 파싱 실패: {e}"))?;

    let mut doc = HwpmlDocument {
        body: Vec::new(),
        all: Vec::new(),
    };
    for section in xml.descendants().filter(|n| n.has_tag_name("SECTION")) {
        collect_paragraphs(section, true, &mut doc);
    }
    Ok(doc)
}

/// 문서 전체 텍스트를 추출한다. (프롬프트 컨텍스트용)
pub fn document_text(doc: &HwpmlDocument) -> String {
    doc.all.join("\n")
}

/// 본문 문단 텍스트를 문서 순서대로 반환한다.
///
/// GetPos의 본문 리스트(list 0) 문단 순서에 대응한다. 커서 문단 인덱스로 컨텍스트 범위를
/// 축소(`Scope::CurrentParagraph`)할 때 사용한다.
pub fn paragraph_texts(doc: &HwpmlDocument) -> &[String] {
    &doc.body
}

/// `node` 아래의 `P`를 문서 순서로 모은다. `top`은 본문 문단(중첩되지 않은 `P`) 여부.
fn collect_paragraphs(node: Node, top: bool, doc: &mut HwpmlDocument) {
    for child in node.children().filter(Node::is_element) {
        if child.has_tag_name("P") {
            let text = paragraph_text(child);
            if top {
                doc.body.push(text.clone());
            }
            doc.all.push(text);
            collect_paragraphs(child, false, doc);
        } else {
            collect_paragraphs(child, top, doc);
        }
    }
}

/// 문단 자신의 텍스트(`P > TEXT > CHAR`). 중첩 문단의 텍스트는 포함하지 않는다.
fn paragraph_text(para: Node) -> String {
    let mut text = String::new();
    let chars = para
        .children()
        .filter(|n| n.has_tag_name("TEXT"))
        .flat_map(|t| t.children())
        .filter(|n| n.has_tag_name("CHAR"));
    for piece in chars.flat_map(|c| c.children()) {
        if piece.is_text() {
            text.push_str(piece.text().unwrap_or_default());
        } else {
            match piece.tag_name().name() {
                "TAB" => text.push('\t'),
                "LINEBREAK" => text.push('\n'),
                "NBSPACE" | "FWSPACE" => text.push(' '),
                "HYPHEN" => text.push('-'),
                _ => {}
            }
        }
    }
    text
}
