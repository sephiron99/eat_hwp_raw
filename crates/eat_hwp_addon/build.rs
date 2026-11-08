//! DLL에 매니페스트를 리소스 ID 123으로 넣는다(`eat_hwp_addon.manifest` 참고).
//!
//! rundll32는 대상 DLL의 ID 123 매니페스트로 활성화 컨텍스트를 만든 뒤 진입점(`RunGuiW`)을
//! 부른다. 이것이 없으면 GUI가 공용 컨트롤 v5로 떠서 콤보박스 드롭다운 목록이 펼쳐지지 않는다.

fn main() {
    println!("cargo:rerun-if-changed=eat_hwp_addon.manifest");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    println!("cargo:rustc-link-arg-cdylib=/MANIFEST:EMBED,ID=123");
    println!("cargo:rustc-link-arg-cdylib=/MANIFESTINPUT:{dir}/eat_hwp_addon.manifest");
}
