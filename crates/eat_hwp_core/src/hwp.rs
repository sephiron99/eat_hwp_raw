//! 한글(HWP) 연동: `hwp_com` OLE 클라이언트로 현재 문서 컨텍스트 확보 및 hwpx 저장.
//!
//! 워크플로(eat_hwp_raw.md): addon이 eat_hwp_raw를 호출하므로 한글은 이미 실행 중 →
//! `HwpClient::attach()`로 실행 중인 인스턴스에 연결한다.
//!
//! CLAUDE.md 준수사항:
//! - eat_hwp_raw는 hwp action/method/parameter를 직접 추가하지 않는다.
//! - 누락 기능은 hwp_automation_rs에 추가하도록 제안한다.
//! - `GetTextFile` 사용 금지(여기서는 호출하지 않는다).

use anyhow::{Result, anyhow, bail};
use hwp_com::HwpClient;
use hwp_core::hwp_obj::HwpObject;
use std::path::Path;

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

/// 현재 문서를 hwpx 포맷으로 `path`에 저장한다.
///
/// CLAUDE.md 준수사항(1번)에 따라 hwp SaveAs는 hwp_automation_rs에 추가한 typed 메서드
/// `HwpObject::save_as_hwpx(&str)`를 호출한다(VARIANT/파라미터는 eat_hwp_raw에서 직접 구성하지 않음).
pub fn save_as_hwpx(hwp: &HwpObject, path: &Path) -> Result<()> {
    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow!("저장 경로가 UTF-8이 아님: {}", path.display()))?;
    let ok = hwp
        .save_as_hwpx(path_str)
        .map_err(|e| anyhow!("SaveAs(HWPX) 실패: {e}"))?;
    if !ok {
        bail!("SaveAs(HWPX)가 실패를 반환했습니다: {path_str}");
    }
    Ok(())
}
