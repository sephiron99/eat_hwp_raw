//! 한글(HWP) 연동: `hwp_com` OLE 클라이언트로 현재 문서 내용(HWPML)과 커서 위치 확보.
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
use hwp_com::{HwpClient, HwpObject};

/// 현재 문서의 HWPML과 그 안에서의 커서 위치.
#[derive(Clone, Debug)]
pub struct DocSnapshot {
    /// 문서 전체의 HWPML 2.x 문자열.
    pub hwpml: String,
    /// `hwpml` 안에서 커서가 있는 바이트 위치. 커서를 알 수 없으면 `None`.
    pub caret: Option<usize>,
}

/// 실행 중인 한글 인스턴스에 연결한다.
pub fn connect() -> Result<HwpObject> {
    HwpClient::attach().map_err(|e| anyhow!("실행 중인 한글에 연결 실패: {e}"))
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
        hwpml: caret.hwpml,
        caret: Some(caret.offset),
    })
}
