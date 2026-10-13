//! HWPML 파싱: `GetTextFile("HWPML2X")`가 돌려준 XML에서 문단 텍스트를 추출한다.
//!
//! HWPML 2.x 구조: `HWPML > BODY > SECTION > P > TEXT > CHAR`. 표·글상자 등의 안쪽 문단은
//! 바깥 문단의 `TEXT` 아래에 다시 `P`로 중첩된다.

use anyhow::{Result, anyhow};
use roxmltree::{Node, ParsingOptions};
use std::ops::Range;

/// HWPML에서 추출한 문단 텍스트.
pub struct HwpmlDocument {
    /// 표 셀 등 중첩 문단까지 포함한 모든 문단, 문서 순서.
    paras: Vec<Paragraph>,
}

struct Paragraph {
    text: String,
    /// 이 문단을 담은 바깥 문단의 인덱스. 본문 문단은 `None`.
    /// 한 표의 셀들, 한 각주의 문단들은 같은 바깥 문단을 가진다.
    outer: Option<usize>,
    /// HWPML 문자열 안에서 `P` 요소가 차지하는 바이트 범위.
    range: Range<usize>,
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
        collect_paragraphs(section, None, &mut doc);
    }
    Ok(doc)
}

/// 문서 전체 텍스트를 추출한다. (프롬프트 컨텍스트용)
pub fn document_text(doc: &HwpmlDocument) -> String {
    let texts: Vec<&str> = doc.paras.iter().map(|p| p.text.as_str()).collect();
    texts.join("\n")
}

/// 커서가 있는 문단과 같은 묶음의 문단 텍스트를 문서 순서대로 반환한다.
///
/// `caret`은 파싱한 HWPML 문자열 안의 바이트 위치(`HwpExt::hwpml_with_caret`의 `offset`)다.
/// 반환값은 (문단 텍스트들, 그 안에서 커서 문단의 인덱스). 묶음은 본문 문단 전체이거나,
/// 커서가 표 셀·각주 등 안에 있으면 같은 바깥 문단에 든 문단들이다. 커서 위치가 어느
/// 문단에도 속하지 않으면 `None`.
pub fn caret_paragraphs(doc: &HwpmlDocument, caret: usize) -> Option<(Vec<&str>, usize)> {
    // 중첩 문단은 바깥 문단보다 뒤에 나오므로, 범위에 드는 마지막 문단이 가장 안쪽이다.
    let target = doc.paras.iter().rposition(|p| p.range.contains(&caret))?;
    let outer = doc.paras[target].outer;

    let mut texts = Vec::new();
    let mut center = 0;
    for (i, para) in doc.paras.iter().enumerate() {
        if para.outer == outer {
            if i == target {
                center = texts.len();
            }
            texts.push(para.text.as_str());
        }
    }
    Some((texts, center))
}

/// `node` 아래의 `P`를 문서 순서로 모은다. `outer`는 `node`를 담은 문단의 인덱스.
fn collect_paragraphs(node: Node, outer: Option<usize>, doc: &mut HwpmlDocument) {
    for child in node.children().filter(Node::is_element) {
        if child.has_tag_name("P") {
            let index = doc.paras.len();
            doc.paras.push(Paragraph {
                text: paragraph_text(child),
                outer,
                range: child.range(),
            });
            collect_paragraphs(child, Some(index), doc);
        } else {
            collect_paragraphs(child, outer, doc);
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
