//! eat_hwp_raw 런처 애드인 DLL (GUI 통합형, 독립 프로세스 기동).
//!
//! 애드인 DLL은 한글(HWP) 프로세스 안에 로드되어 동작한다. 리본 버튼/단축키 `Ctrl+]`를 누르면
//! **별도 실행파일 없이** 같은 DLL에 통합된 GUI를 **독립 프로세스**로 띄운다. 한글이 종료돼도
//! GUI 프로세스는 살아남는다.
//!
//! 기동: `do_action`이 32비트 `rundll32.exe <self.dll>,RunGuiW` 를 자식 프로세스로 spawn한다.
//! rundll32가 같은 DLL을 새 프로세스에 로드하고 `RunGuiW`를 호출 → 그 안에서 통합 GUI
//! (`eat_hwp_core::run_gui`)가 실행된다. Windows는 부모(HWP) 종료 시 자식을 강제 종료하지
//! 않으므로(추가로 `CREATE_BREAKAWAY_FROM_JOB` 시도) GUI는 독립적으로 유지된다.
//!
//! 크레이트 루트에 `#![forbid(unsafe_code)]`를 둔다. DLL export(`QueryUserActionInterface`,
//! `RunGuiW`)는 `hwp_addon`의 `export_hwp_addon!`/`export_rundll_entry!` 매크로가 생성한다.
//! `hwp_addon`/`eat_hwp_core`는 의존성으로만 사용한다(CLAUDE.md 경계).

#![forbid(unsafe_code)]

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use hwp_addon::debug::{log, msgbox_err};
use hwp_addon::{export_hwp_addon, export_rundll_entry};
use hwp_addon::hwp_user_action::{ActionMeta, HwpUserAction, ToolbarConfig, ToolbarTarget};
use hwp_addon::shortcut::{Modifiers, ShortcutKey};
use hwp_core::error::Result;
use hwp_core::hwp_obj::HwpObject;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_OEM_6;

/// 툴바/리본 아이콘 스트립(960×16, 32bit BMP). `tools/make_icon.py`로 생성.
const TOOLBAR_DATA: &[u8] = include_bytes!("../toolbar.bmp");

/// 액션 식별자.
const ACTION_LAUNCH: &str = "LaunchEatHwpRaw";

/// 이 DLL의 파일 이름(cargo 산출물 이름).
const SELF_DLL_NAME: &str = concat!(env!("CARGO_PKG_NAME"), ".dll");

/// rundll32가 호출할 export 이름.
const RUNDLL_ENTRY: &str = "RunGuiW";

// CreateProcess 플래그(독립성 보강용).
const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
const DETACHED_PROCESS: u32 = 0x0000_0008;

static CONFIG: ToolbarConfig = ToolbarConfig {
    name: "eat_hwp_raw",
    serialize_path: "EatHwpRaw",
    // 아이콘 스트립의 0번 셀(16×16)을 버튼 아이콘으로 사용(setup_toolbar가 btn_style 3 처리).
    bitmap_data: TOOLBAR_DATA,
    // HWP SDK는 내장 탭 접근을 지원하지 않으므로 새 리본 탭으로 노출한다.
    target: ToolbarTarget::Ribbon("AI 문서"),
    ribbon_toolbox_index: -1,
};

/// eat_hwp_raw 런처 플러그인.
pub struct EatHwpLauncher;

impl HwpUserAction for EatHwpLauncher {
    fn toolbar_config(&self) -> Option<&ToolbarConfig> {
        Some(&CONFIG)
    }

    fn actions(&self) -> &'static [ActionMeta] {
        static ACTIONS: [ActionMeta; 1] = [ActionMeta {
            name: ACTION_LAUNCH,
            label: "AI 작성",
            image_index: 0,
            // Ctrl+]  (']' = VK_OEM_6, US 표준 키보드 기준)
            shortcut: Some(ShortcutKey {
                modifiers: Modifiers {
                    alt: false,
                    ctrl: true,
                    shift: false,
                },
                key: VK_OEM_6,
            }),
        }];
        &ACTIONS
    }

    /// 기본 `on_load`(= `setup_toolbar`)를 감싸 설정 실패를 메시지 박스로 표면화한다(브링업 편의).
    fn on_load(&self, hwp: &HwpObject) -> Result<bool> {
        match self.setup_toolbar(hwp) {
            Ok(v) => Ok(v),
            Err(e) => {
                msgbox_err("eat_hwp_addon on_load", &format!("{e}"));
                Err(e)
            }
        }
    }

    /// 버튼/단축키(`Ctrl+]`)로 호출. GUI를 **독립 프로세스**로 띄운다(한글 종료와 무관히 생존).
    fn do_action(&self, action_name: &str, _hwp: &HwpObject) -> Result<bool> {
        if action_name != ACTION_LAUNCH {
            return Ok(false);
        }
        match launch_gui_process() {
            Ok(()) => Ok(true),
            Err(e) => {
                msgbox_err("eat_hwp_raw GUI 실행 실패", &e);
                Ok(false)
            }
        }
    }
}

// ── rundll32 진입점 (독립 프로세스에서 실행됨) ──

/// `rundll32.exe eat_hwp_addon.dll,RunGuiW` 로 호출되는 진입점의 본체. 통합 GUI를 실행한다.
/// 콘솔이 없으므로 오류는 메시지 박스로 표시한다.
fn run_gui_entry() {
    log("eat_hwp_addon", "RunGuiW 진입 — 독립 프로세스에서 GUI 시작");
    if let Err(e) = eat_hwp_core::run_gui() {
        msgbox_err("eat_hwp_raw GUI", &format!("{e:?}"));
    }
}

// ── GUI 프로세스 기동 ──

/// 통합 GUI를 독립 프로세스로 띄운다: 32비트 `rundll32.exe <self.dll>,RunGuiW`.
/// 자식은 부모(HWP)와 독립이라 한글이 종료돼도 유지된다. 부모가 kill-on-close Job에 있을
/// 경우까지 대비해 `CREATE_BREAKAWAY_FROM_JOB`를 먼저 시도하고, 거부되면 일반 spawn으로 폴백한다.
fn launch_gui_process() -> std::result::Result<(), String> {
    let dll = self_dll_path().ok_or_else(|| "자기 DLL 경로 조회 실패".to_string())?;
    let rundll = rundll32_path();
    // rundll32는 콤마 앞 경로에 공백이 있으면 파싱이 깨지므로 경로를 따옴표로 감싼다.
    // `arg`는 인자 전체를 다시 인용하므로 `raw_arg`로 그대로 전달한다.
    let entry = format!("\"{}\",{RUNDLL_ENTRY}", dll.display());
    log("eat_hwp_addon", &format!("spawn: {} {}", rundll.display(), entry));

    let spawn = |flags: u32| {
        Command::new(&rundll)
            .raw_arg(&entry)
            .creation_flags(flags)
            .spawn()
    };
    // Child 핸들은 drop돼도 프로세스는 계속 실행된다(wait 안 함).
    spawn(CREATE_BREAKAWAY_FROM_JOB | DETACHED_PROCESS)
        .or_else(|_| spawn(DETACHED_PROCESS))
        .map(|_child| ())
        .map_err(|e| format!("rundll32 실행 불가: {e}"))
}

/// 32비트 rundll32 경로. 한글(32비트)이 로드한 32비트 DLL을 띄우려면 32비트 호스트가 필요하다.
/// `<windir>\SysWOW64\rundll32.exe` 우선, 없으면 PATH의 `rundll32.exe`.
fn rundll32_path() -> PathBuf {
    if let Ok(windir) = std::env::var("SystemRoot").or_else(|_| std::env::var("windir")) {
        let p = PathBuf::from(&windir).join("SysWOW64").join("rundll32.exe");
        if p.exists() {
            return p;
        }
    }
    PathBuf::from("rundll32.exe")
}

/// 현재 DLL의 전체 경로. 로드된 모듈 중 이 DLL의 파일 이름(`SELF_DLL_NAME`)으로 핸들을 찾는다.
/// 빌드 산출물의 파일 이름을 바꿔 등록하면 찾지 못한다.
fn self_dll_path() -> Option<PathBuf> {
    let hmod = winsafe::HINSTANCE::GetModuleHandle(Some(SELF_DLL_NAME)).ok()?;
    hmod.GetModuleFileName().ok().map(PathBuf::from)
}

export_hwp_addon!(EatHwpLauncher, EatHwpLauncher);
// `RUNDLL_ENTRY`와 같은 이름으로 export한다.
export_rundll_entry!(RunGuiW, run_gui_entry);
