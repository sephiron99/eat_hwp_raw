# eat_hwp_raw

## Project Overview
LLM이 현재 HWP Document의 양식과 내용, 기초자료를 바탕으로 문서를 작성·수정·완성하는
HWP AI AGENT. 한글로 진행.

## project 이름
"eat_hwp_raw"전체가 프로젝트 이름임

## Build
Windows 전용(WinSafe/COM)이므로 빌드/검사는 Windows 타깃에서 수행한다.
WSL/Linux 개발 환경에서는 `--target x86_64-pc-windows-msvc`를 지정하지 않으면 winsafe에서 컴파일 실패한다.
### build Commands
```bash
cargo build --target x86_64-pc-windows-msvc
cargo check --target x86_64-pc-windows-msvc   # type-check only
```
### unsafe policy
크레이트 루트(`src/main.rs` 최상단)에 `#![forbid(unsafe_code)]` 적용.

## Dependencies
1. 백엔드는 [rig-rs](https://docs.rig.rs/)
1. Windows GUI는 [WinSafe](https://docs.rs/winsafe/latest/winsafe/gui/index.html) 
1. [hwp_automation_rs](https://github.com/sephiron99/hwp_automation_rs) 에서 hwp_com을 기반으로 작성하되, 필요할 때에는 hwp_addon도 사용한다.
1. Windows 10 이상 전용
1. Hwp/Hwpx 파서: SaveAs로 hwpx 변환 후 [hwpers](https://github.com/Indosaram/hwpers)로 파싱 (GetTextFile 금지와 연계)

## 유의사항 및 준수사항
1. hwp_automation_rs에 없는 hwp com/action/기능은 hwp_automation_rs에 해당 기능을 추가하도록 제시.
1. eat_hwp_raw 프로젝트에 hwp action/method/parameter을 추가하지 못함.
1. Hwp sdk의 GetTextFile은 사용 금지


## Features
[Features](./eat_hwp_raw.md#features)

## Workflow
[Workflow](./eat_hwp_raw.md#workflow)

## eat_hwp_raw GUI 구성
[GUI](./eat_hwp_raw.md#eat_hwp_raw-gui)
