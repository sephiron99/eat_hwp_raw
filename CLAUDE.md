# eat_hwp_raw

## Project Overview
LLM이 현재 HWP Document의 양식과 내용, 기초자료를 바탕으로 문서를 작성·수정·완성하는
HWP AI AGENT. 한글로 진행.

## project 이름
"eat_hwp_raw"전체가 프로젝트 이름임

## Build
Windows 전용(WinSafe/COM)이고 한글 COM이 32비트이므로, 기본 타깃은 `i686-pc-windows-msvc`다
(`.cargo/config.toml`에 설정됨 → 아래 명령에 `--target` 불필요).
- `cargo build`: WSL에서는 [msvc-wsl-rust](https://github.com/strickczq/msvc-wsl-rust) 링커가 필요
  (`.cargo/config.toml`의 `linker` 경로를 환경에 맞게 수정).
- `cargo check`: 링크를 하지 않으므로 링커 없이 WSL에서 타입 검증 가능.
### build Commands
```bash
cargo build
cargo check
```
### 구조 / 산출물
워크스페이스: `crates/eat_hwp_core`(lib — GUI·파이프라인) + `crates/eat_hwp_addon`(cdylib — HWP
애드인 셸 + rundll32 진입점). **별도 실행파일 없음** — 애드인이 한글 안에서 단축키(`Ctrl+]`)/버튼으로
32비트 `rundll32.exe <self.dll>,RunGuiW`를 spawn해 통합 GUI를 **독립 프로세스**로 띄운다(한글이
종료돼도 GUI 생존). `cargo build`가 워크스페이스 전체를 빌드하며 산출물은
`target/i686-pc-windows-msvc/debug/eat_hwp_addon.dll` 하나다.
### unsafe policy
워크스페이스의 모든 크레이트(`crates/eat_hwp_core`, `crates/eat_hwp_addon`) 루트에
`#![forbid(unsafe_code)]` 적용. DLL export(`QueryUserActionInterface`, `RunGuiW`)는 `hwp_addon`의
`export_hwp_addon!`/`export_rundll_entry!` 매크로로 생성하고, Win32 호출은 WinSafe의 safe 래퍼를 쓴다.
unsafe가 필요한 기능은 이 프로젝트에 두지 않고 hwp_automation_rs에 추가하도록 제시한다.

## Dependencies
1. 백엔드는 [rig-rs](https://docs.rig.rs/)
1. Windows GUI는 [WinSafe](https://docs.rs/winsafe/latest/winsafe/gui/index.html) 
1. [hwp_automation_rs](https://github.com/sephiron99/hwp_automation_rs) 에서 hwp_com을 기반으로 작성하되, 필요할 때에는 hwp_addon도 사용한다.
1. Windows 10 이상 전용
1. 문서 내용: `GetTextFile`(`GetTextFileFormat::HwpMl2X`)로 HWPML 문자열을 받아 [roxmltree](https://docs.rs/roxmltree)로 파싱

## 유의사항 및 준수사항
1. hwp_automation_rs에 없는 hwp com/action/api는 hwp_automation_rs에 해당
   기능을 추가하도록 제시. 단, addin같은 산출물은 hwp_automation_rs에 속하지
   않고, 이 프로젝트의 일부임.
1. eat_hwp_raw 프로젝트에 hwp action/method/parameter을 추가하지 못함.
1. 문서 내용은 Hwp sdk의 GetTextFile(HWPML2X)로 읽는다. SaveAs로 임시 파일을 만들거나 hwpx를 파싱하지 않는다.


## Features
[Features](./eat_hwp_raw.md#features)

## Workflow
[Workflow](./eat_hwp_raw.md#workflow)

## eat_hwp_raw GUI 구성
[GUI](./eat_hwp_raw.md#eat_hwp_raw-gui)
