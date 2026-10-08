//! hwpx 파싱: `hwpers`로 문서 구조/텍스트를 추출한다.

use anyhow::{Result, anyhow};
use hwpers::HwpxReader;
use hwpers::model::HwpDocument;
use std::path::Path;

/// hwpx 파일을 파싱하여 문서 모델을 반환한다.
pub fn parse_hwpx(path: &Path) -> Result<HwpDocument> {
    HwpxReader::from_file(path).map_err(|e| anyhow!("hwpx 파싱 실패: {e}"))
}

/// 문서 전체 텍스트를 추출한다. (프롬프트 컨텍스트용)
pub fn document_text(doc: &HwpDocument) -> String {
    doc.extract_text()
}

/// 본문 문단 텍스트를 문서 순서대로 반환한다.
///
/// GetPos의 본문 리스트(list 0) 문단 순서에 대응한다. 커서 문단 인덱스로 컨텍스트 범위를
/// 축소(`Scope::CurrentParagraph`)할 때 사용한다.
pub fn paragraph_texts(doc: &HwpDocument) -> Vec<String> {
    doc.sections()
        .flat_map(|section| section.paragraphs.iter())
        .map(|para| {
            para.text
                .as_ref()
                .map(|t| t.content.clone())
                .unwrap_or_default()
        })
        .collect()
}
