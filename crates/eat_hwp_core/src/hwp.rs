//! 한글(HWP) 연동: `hwp_com` OLE 클라이언트로 현재 문서 컨텍스트와 문서 내용(HWPML) 확보.
//!
//! 워크플로(eat_hwp_raw.md): addon이 eat_hwp_raw를 호출하므로 한글은 이미 실행 중 →
//! `HwpClient::attach()`로 실행 중인 인스턴스에 연결한다.
//!
//! CLAUDE.md 준수사항:
//! - eat_hwp_raw는 hwp action/method/parameter를 직접 추가하지 않는다.
//! - 누락 기능은 hwp_automation_rs에 추가하도록 제안한다.
//! - 문서 내용은 `GetTextFile`(HWPML2X)로 읽는다. 파일로 저장하지 않는다.

use anyhow::{Result, anyhow, bail};
use hwp_com::method::hwp_object::GetTextFileFormat;
use hwp_com::{HwpClient, HwpObject};

/// 현재 커서/선택 컨텍스트. (SDK `GetPos`: list/para/pos)
#[derive(Clone, Debug)]
pub struct DocContext {
    /// 커서가 위치한 리스트 id. 본문은 0.
    pub list: i32,
    /// 커서가 위치한 문단 인덱스.
    pub para: i32,
    /// 문단 내 글자 위치. (문단 단위 축소에는 미사용 — 향후 글자 단위 처리용 예약)
    #[allow(dead_code)]
    pub pos: i32,
}

impl DocContext {
    /// 본문(list 0)에 커서가 있을 때만 본문 문단 인덱스를 반환한다.
    ///
    /// 표/캡션 등 다른 리스트(list != 0)에서는 본문 문단 순서와 매핑되지 않으므로 `None`.
    pub fn body_paragraph_index(&self) -> Option<usize> {
        (self.list == 0 && self.para >= 0).then_some(self.para as usize)
    }
}

/// 실행 중인 한글 인스턴스에 연결한다.
pub fn connect() -> Result<HwpObject> {
    HwpClient::attach().map_err(|e| anyhow!("실행 중인 한글에 연결 실패: {e}"))
}

/// 현재 커서 위치 컨텍스트를 확보한다. (SDK `GetPos`)
pub fn current_context(hwp: &HwpObject) -> Result<DocContext> {
    let (list, para, pos) = hwp.get_pos().map_err(|e| anyhow!("GetPos 실패: {e}"))?;
    Ok(DocContext { list, para, pos })
}

/// 현재 문서 전체를 HWPML 2.x 문자열로 읽는다. (SDK `GetTextFile("HWPML2X", "")`)
///
/// 한글이 빈 문자열을 돌려주면 실패로 본다.
pub fn document_hwpml(hwp: &HwpObject) -> Result<String> {
    let hwpml = hwp
        .get_text_file(GetTextFileFormat::HwpMl2X, false)
        .map_err(|e| anyhow!("GetTextFile(HWPML2X) 실패: {e}"))?;
    if hwpml.is_empty() {
        bail!("GetTextFile(HWPML2X)가 빈 문자열을 반환했습니다");
    }
    Ok(hwpml)
}
