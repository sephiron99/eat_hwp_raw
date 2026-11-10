//! 한글(HWP) 연동: `hwp_com` OLE 클라이언트로 현재 문서 내용(HWPML)과 커서 위치 확보, LLM이 쓴 글 넣기.
//!
//! 목표 구조(CLAUDE.md)에서는 한글 안의 애드온이 HWPML을 획득해 GUI 프로세스에 넘긴다.
//! 현재 구현: addon이 GUI 프로세스를 띄우므로 한글은 이미 실행 중 →
//! `HwpClient::attach()`로 실행 중인 인스턴스에 연결한다.
//!
//! CLAUDE.md 준수사항:
//! - eat_hwp_raw는 hwp action/method/parameter를 직접 추가하지 않는다.
//! - 누락 기능은 hwp_automation_rs에 추가하도록 제안한다.
//! - 문서 내용은 `GetTextFile`(HWPML2X)로 읽는다. 파일로 저장하지 않는다.

use anyhow::{Result, anyhow, bail};
use hwp_com::ext::CharShape;
use hwp_com::method::hwp_object::{FileFormat, GetTextFileFormat};
use hwp_com::{HwpClient, HwpObject};

/// 현재 문서에서 읽은 내용.
#[derive(Clone, Debug)]
pub struct DocSnapshot {
    pub content: DocContent,
    /// 문서 파일 경로. 한 번도 저장하지 않은 문서는 빈 문자열. 대화 기록을 문서별로 나누는 데 쓴다.
    pub path: String,
    /// 읽은 문서. LLM이 쓴 글은 이 문서에만 넣는다. 한글에서 읽지 않았으면(`EAT_HWP_DOC`) `None`.
    pub origin: Option<DocIdentity>,
}

/// 한글에 열린 문서 하나를 가리키는 값. 문서 ID(`DocumentID`)와 전체 경로(`FullName`)가 모두 같아야
/// 같은 문서로 본다. 문서를 닫은 뒤 새로 연 문서가 같은 ID를 받는 경우를 경로로 한 번 더 거른다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocIdentity {
    pub id: i32,
    /// 저장하지 않은 문서는 빈 문자열일 수 있다.
    pub full_name: String,
}

/// 문서 내용. LLM에 보낼 형식에 따라 읽는 방법이 다르다.
#[derive(Clone, Debug)]
pub enum DocContent {
    /// `GetTextFile("HWPML2X")` 원문과 그 안의 커서 바이트 위치(모르면 `None`). 평문·HWPML 형식에 쓴다.
    Hwpml { hwpml: String, caret: Option<usize> },
    /// HWPX로 저장해 hwp2md로 바꾼 Markdown. 커서 자리에 `parse::CARET_MARK`가 들어 있을 수 있다.
    Markdown(String),
}

/// 실행 중인 한글 인스턴스에 연결한다.
pub fn connect() -> Result<HwpObject> {
    HwpClient::attach().map_err(|e| anyhow!("실행 중인 한글에 연결 실패: {e}"))
}

/// 지금 활성 문서(`XHwpDocuments.Active_XHwpDocument`).
pub fn active_document(hwp: &HwpObject) -> Result<DocIdentity> {
    let doc = hwp
        .documents()
        .and_then(|docs| docs.active())
        .map_err(|e| anyhow!("활성 문서를 얻지 못함: {e}"))?;
    identity(&doc)
}

fn identity(doc: &hwp_com::method::xhwp::XHwpDocument) -> Result<DocIdentity> {
    Ok(DocIdentity {
        id: doc
            .document_id()
            .map_err(|e| anyhow!("문서 ID를 얻지 못함: {e}"))?,
        full_name: doc.full_name().unwrap_or_default(),
    })
}

/// 원래 문서(`origin`)의 커서 자리에 `text`를 넣는다. 원래 문서가 아닌 곳에는 넣지 않는다.
///
/// 원래 문서가 활성 문서가 아니면(응답을 기다리는 사이 다른 탭으로 간 경우) 원래 문서를 찾아
/// 활성화한 뒤 넣는다. 그 문서의 커서 자리에 들어간다. 원래 문서를 찾지 못하거나(닫힘, 다른 이름으로
/// 저장해 경로가 바뀜) 활성화하지 못하면 넣지 않고 오류를 돌려준다. 활성 문서를 바꿨으면 `true`.
pub fn insert_into(hwp: &HwpObject, origin: &DocIdentity, text: &str) -> Result<bool> {
    if active_document(hwp)? == *origin {
        insert_text(hwp, text)?;
        return Ok(false);
    }
    let doc = hwp
        .documents()
        .and_then(|docs| docs.find_item(origin.id))
        .map_err(|e| anyhow!("원래 문서를 찾지 못함(닫혔을 수 있음): {e}"))?;
    let found = identity(&doc)?;
    if found != *origin {
        bail!(
            "원래 문서를 찾지 못함: 읽을 때 {:?}(ID {}), 지금 그 ID는 {:?}",
            origin.full_name,
            origin.id,
            found.full_name
        );
    }
    doc.set_active()
        .map_err(|e| anyhow!("원래 문서를 활성화하지 못함: {e}"))?;
    // 활성화가 실제로 됐는지 넣기 직전에 다시 확인한다.
    let active = active_document(hwp)?;
    if active != *origin {
        bail!("원래 문서를 활성화하지 못함: 활성 문서가 {:?}", active.full_name);
    }
    insert_text(hwp, text)?;
    Ok(true)
}

/// 지금 커서 자리에 `text`를 넣는다(`ExtText::insert_with_shape`). 선택 블록이 있으면 블록을 지우고
/// 그 자리에 넣는다.
///
/// 글자 모양은 아직 정하지 않는다. 모든 항목이 `None`인 `CharShape`를 넘겨 커서 자리의 모양을 그대로
/// 따른다. 줄바꿈은 `\r\n`으로 맞춘다.
pub fn insert_text(hwp: &HwpObject, text: &str) -> Result<()> {
    let text = text.replace("\r\n", "\n").replace('\n', "\r\n");
    hwp.ext_text()
        .insert_with_shape(&text, &CharShape::default())
        .map_err(|e| anyhow!("문서에 글 넣기 실패: {e}"))
}

/// 현재 문서 전체의 HWPML과 커서 위치를 읽는다. (`ExtDoc::hwpml_with_caret`)
///
/// 커서 자리에 책갈피를 잠깐 넣었다가 실행 취소로 되돌리는 방식이라 문서의 수정 상태와
/// 다시 실행 기록이 바뀌고, 선택 블록이 있으면 선택이 풀린다. `GetPos`의 번호는 HWPML
/// 요소에 대응시킬 수 없어 쓰지 않는다.
///
/// 한글이 빈 문자열을 돌려주면 실패로 본다.
pub fn document_snapshot(hwp: &HwpObject) -> Result<DocSnapshot> {
    let caret = hwp
        .ext_doc()
        .hwpml_with_caret()
        .map_err(|e| anyhow!("HWPML과 커서 위치 확보 실패: {e}"))?;
    if caret.hwpml.is_empty() {
        bail!("GetTextFile(HWPML2X)가 빈 문자열을 반환했습니다");
    }
    Ok(DocSnapshot {
        content: DocContent::Hwpml {
            hwpml: caret.hwpml,
            caret: Some(caret.offset),
        },
        path: hwp.path().unwrap_or_default(),
        origin: None,
    })
}

/// 현재 문서를 `SaveAs(경로, "HWPML2X", "lock:false")`로 사용자 임시 디렉터리에 저장하고 읽은 뒤
/// 지운다. 커서 위치는 알 수 없다(책갈피를 넣는 API가 hwp_automation_rs에 공개돼 있지 않다).
///
/// `lock:false`라 열린 문서는 원래 파일에 그대로 묶여 있다. 한글은 마지막으로 `SaveAs`한 파일을
/// 삭제 공유 없이 열어 두므로, `GetTextFile("TEXT")`을 한 번 불러 놓게 한 뒤 지운다
/// (hwp_automation_rs의 `hwpx_with_caret`과 같은 방법).
pub fn document_hwpml_file(hwp: &HwpObject) -> Result<DocSnapshot> {
    let file = std::env::temp_dir().join(format!(
        "eat_hwp_raw_{}_{}.hml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default()
    ));
    let path = file
        .to_str()
        .ok_or_else(|| anyhow!("임시 파일 경로를 문자열로 바꿀 수 없음: {}", file.display()))?;
    hwp.save_as(path, &FileFormat::HWPML2X, "lock:false")
        .map_err(|e| anyhow!("SaveAs(HWPML2X) 실패: {e}"))?;
    let _ = hwp.get_text_file(GetTextFileFormat::Text, false);
    let bytes = std::fs::read(&file);
    let _ = std::fs::remove_file(&file);
    let bytes = bytes.map_err(|e| anyhow!("저장한 HWPML 파일을 읽지 못함: {}: {e}", file.display()))?;
    tracing::debug!(
        file = %file.display(),
        bytes = bytes.len(),
        bom = ?bytes.get(..3),
        "SaveAs(HWPML2X) 파일 읽음"
    );
    let hwpml = decode_text(&bytes);
    if hwpml.is_empty() {
        bail!("SaveAs(HWPML2X)로 저장한 파일이 비어 있음");
    }
    Ok(DocSnapshot {
        content: DocContent::Hwpml { hwpml, caret: None },
        path: hwp.path().unwrap_or_default(),
        origin: None,
    })
}

/// 저장된 텍스트 파일을 BOM으로 판단해 문자열로 바꾼다(UTF-16LE/BE, UTF-8, BOM 없으면 UTF-8로 보고
/// 실패하면 UTF-16LE).
fn decode_text(bytes: &[u8]) -> String {
    let utf16 = |data: &[u8], le: bool| {
        let units: Vec<u16> = data
            .chunks_exact(2)
            .map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) })
            .collect();
        String::from_utf16_lossy(&units)
    };
    match bytes {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, true),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, false),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        _ => match std::str::from_utf8(bytes) {
            Ok(s) => s.to_string(),
            Err(_) => utf16(bytes, true),
        },
    }
}

/// 현재 문서를 커서 책갈피와 함께 임시 HWPX로 저장(`ExtDoc::hwpx_with_caret`, `SaveAs`)하고
/// hwp2md로 Markdown으로 바꾼다. 임시 HWPX는 바꾼 뒤 지운다.
///
/// 열린 문서의 경로·형식은 그대로지만, 책갈피를 넣었다 되돌리므로 문서가 "변경됨"이 되고 다시 실행
/// 기록이 바뀐다.
pub fn document_markdown(hwp: &HwpObject) -> Result<DocSnapshot> {
    let hwpx = hwp
        .ext_doc()
        .hwpx_with_caret()
        .map_err(|e| anyhow!("커서 표시가 든 HWPX 저장 실패: {e}"))?;
    tracing::debug!(hwpx = %hwpx.path().display(), bookmark = hwpx.bookmark_name(), "임시 HWPX 저장");
    let md = crate::markdown::hwpx_to_markdown(hwpx.path(), Some(hwpx.bookmark_name()))?;
    Ok(DocSnapshot {
        content: DocContent::Markdown(md),
        path: hwp.path().unwrap_or_default(),
        origin: None,
    })
}

/// 실행 중인 한글의 현재 문서 파일 경로. 저장하지 않은 문서는 빈 문자열.
pub fn current_document_path() -> Result<String> {
    let hwp = connect()?;
    hwp.path().map_err(|e| anyhow!("문서 경로를 읽지 못함: {e}"))
}
