# eat_hwp_raw (Hwp Ai Agent)

## Project Overview
LLM이 문서 전체 구조와 내용을 읽고, 커서가 위치한 곳에 들어갈 글을 폰트도 맞춰서 입력하는
hwp addon과 서버. 사용자는 타겟과 유사한 문서들을 미리 제공하여 출력 품질을 높일 수 있음. 한글로 진행.

"eat_hwp_raw" 전체가 프로젝트 이름임.

## 구조 / 산출물
애드온(클라이언트)과 서버로 나눈다. 여러 클라이언트가 한 서버를 쓸 수 있게 서버로 구성한다.

| 크레이트                | 종류                   | 타깃                     | 역할                                                                                                                                                     |
|-------------------------|------------------------|--------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------|
| `crates/eat_hwp_addon`  | cdylib                 | `i686-pc-windows-msvc`   | HWP 애드온 셸 + 클라이언트 GUI. **별도 실행파일 없음** — 아래 두 부분이 같은 DLL에 들어 있다                                                             |
| `crates/eat_hwp_server` | 서버 (axum + REST API) | `x86_64-pc-windows-msvc` | 클라이언트에게서 HWPML을 받아 추가하거나 수정할 글을 HWPML 조각으로 회신한다. LLM 호출과 LLM 설정을 맡고, 시스템 프롬프트 구성과 RAG 기능도 필요하다 |

`eat_hwp_addon`의 두 부분:
- **애드온(한글 프로세스 안)**: 단축키(`Ctrl+]`)/버튼으로 호출되면 한글에서 현재
  문서의 HWPML을 획득해 hwp_com 프로세스(GUI 포함)를 호출하고 넘기는 역할만 한다. 호출될 때 문서 파일을 저장한다.
- **GUI(독립 프로세스, hwp_com)**: 넘겨받은 HWPML을 메모리에 보관하다가  프롬프트 등과 함께 서버로 전송한다. 서버 지정,
  프롬프트 수정, 전송·통신 상황 표시를 맡고, 회신받은 HWPML 조각을 `SetTextFile`로 문서에 넣는다. 독립
  프로세스이므로 서버 응답을 기다리는 동안 한글이 멈추지 않는다.

그 밖의 결정:
- **서버의 위치**: 클라이언트와 같은 기기(로컬) 또는 동일 LAN에 두고, LLM 서버(Ollama 등)와 같은 기기에 둔다.
- **REST API 형식**(엔드포인트, 요청/응답, 커서 위치와 프롬프트 전달, 클라이언트 인증)은 별도 문서에 작성 예정.
- **LLM 설정**(`EAT_HWP_LLM_*`, Claude API 키)은 서버에 둔다. 사용자 지정 시스템 프롬프트만 예외로 클라이언트
  GUI 설정에 둔다(아래 서버 Features의 "시스템 프롬프트 구성").

**현재 코드는 이 구조가 아니다.** 서버 구조는 문서와 주석에만 반영된 상태다. `eat_hwp_server` 크레이트는
아직 없고 `eat_hwp_core`(GUI·파이프라인)가 남아 있다. 애드온은 rundll32로 `eat_hwp_core`의 GUI를 띄우며,
GUI 프로세스가 한글에 직접 붙어 HWPML을 읽고 LLM을 직접 호출해 답변을 답변 칸에만 보여 준다. 아래
Features와 Workflow는 목표 구조 기준이고, 정하지 않은 것은 "미정 사항"에 있다.

## `crates/eat_hwp_addon` Features
1. AI가 현재 XHwpDocument의 양식(제목, 소제목, 글꼴, 문단모양)을 이해할 수 있게 conversion한다.
1. AI가 내용을 작성할 때에는 문단의 직전, 직후 문단에 기재된 제목/소제목(문서
   양식마다 1~4개의 레벨이 존재한다)을 이해하도록 conversion한다.
1. AI가 커서의 컨텍스트(제목/소제목/본문)를 알 수 있게 한다.

## `crates/eat_hwp_server` Features
1. LLM Local or remote Server(nvidia spark dgx 등)를 지원. LLM은 서버(`eat_hwp_server`)가 호출한다
1. Claude api 지원(`EAT_HWP_LLM_PROVIDER=anthropic`). Gemini api는 추후지원으로 일단 설계에만 반영
1. 서버는 여러 클라이언트의 요청을 받는다
1. 시스템 프롬프트 구성. 시스템 메시지는 두 부분으로 나눈다
   - ① 규약(역할, 답변은 넣을 본문만, 문서 형식 설명, `[[커서]]` 규칙)은 서버 코드가 갖고 사용자가 고칠 수
     없다(바꾸면 결과를 문서에 넣는 흐름이 깨진다)
   - ② 사용자 지정 지침(문체·분야 용어·기관 작성 규칙 등)은 클라이언트 GUI 설정으로 두고 사용자가 고친다.
     GUI가 요청과 함께 서버로 보내고, 서버는 ① 뒤에 "이 메시지의 다른 규칙과 어긋나지 않는 범위에서
     따른다"는 말과 함께 붙인다
   - 사용자 메시지에는 문서 내용과 사용자 요청만 담는다
   - 현재 구현: ①은 `workflow::build_system`, 사용자 메시지는 `build_user`. ②는 GUI의 "시스템 프롬프트…"
     창에서 고치고 `%USERPROFILE%\.eat_hwp_raw\system_prompt.txt`(UTF-8)에 저장하며, 요청마다 읽어 붙인다
1. AI서버에 미리 제공된 파일(hwp 등, 타겟과 유사한 문서, Retrieval-Augmented Generation)들에 기재된 사실/근거들을 바탕으로, 사용자가 원하는 내용을 제시한다. RAG 문서는 서버에서만 추가하며, 이 기능은 향후 과제로 미룬다.
1. AI가 글을 작성할 때 준수사항 / 우선순위
    1. 최대 줄 수
    1. 줄간격, 문단여백
    1. 자간

## Workflow
1. 사용자가 `eat_hwp_addon` 의 `버튼1` 클릭 or 단축키 `Ctrl+]`
1. 커서 위치 또는 선택 범위를 `eat_hwp_addon`이 받는다.
1. 애드온은 선택범위가 있는지 먼저 확인하고 선택범위가 있으면
   1. `GetSelectedPosBySet(Method)`로 확보
   내용의 다른 이름저장(사용자 계정 전용 디렉토리의 임시 파일, 포맷은
   HwpObject의 FileFormat::Hwpml)과 선택범위를 `eat_hwp_server`로 전달
1. 애드온은 선택범위가 없다면 hwp_automation_rs의 `hwpml_with_caret`으로 현재 문서 내용(HWPML2X 문자열)과
   그 안의 커서 위치를 함께 확보. 커서 자리에 책갈피를 잠깐 넣었다가 실행 취소로 되돌리는
   방식이라 문서의 수정 상태와 다시 실행 기록이 바뀐다.
1. 애드온이 문서 파일을 저장한다. 이후의 문서 변경 여부를 판단하는 기준점이다. 앞 단계가 문서를 수정된
   상태로 만들므로 HWPML을 확보한 뒤에 저장한다
1. 애드온이 HWPML을 GUI 프로세스에 넘긴다. 애드온의 역할은 여기까지다
1. GUI는 HWPML을 메모리에 보관하고, 사용자는 GUI에서 서버를 지정하고 프롬프트를 수정한다
1. GUI가 서버의 REST API를 호출해 HWPML을 프롬프트, 사용자 지정 시스템 프롬프트 등과 함께 보내고,
   전송·통신 상황을 보여 준다
1. 서버는 받은 HWPML을 파싱해 문서 전체 구조와 내용을 읽고, 시스템 프롬프트(규약 + 사용자 지정)를 더해
   LLM을 실행한다
   (미리 제공된 유사 문서를 더하는 RAG는 향후 과제)
1. 서버는 넣을 글을 그 자리의 서식(폰트 등)에 맞춘 HWPML 조각으로 회신한다. 문서 전체를 회신하지 않는다
1. GUI가 문서 변경 여부를 파일 변경 여부 api로 판단해 조각을 `SetTextFile`로 넣는다
   - 변경이 없으면 원래 문서에 넣는다. 선택 범위가 있었으면 그 범위를 조각으로 고치고(고치는 것은 선택
     범위에 한정한다), 선택이 없었으면 애드온이 호출된 캐럿 위치에 삽입한다
   - 변경이 확인되면 원래 문서 대신 새 탭 문서에 조각을 넣는다

## eat_hwp_raw GUI
GUI는 `eat_hwp_addon`에 두고 독립 프로세스로 실행한다. 구성:

1. 서버 지정
1. 프롬프트 수정
1. 사용자 지정 시스템 프롬프트 편집(설정)
1. 전송·통신 상황 표시
1. etc

현재문단/전체문서 선택은 두지 않고 항상 문서 전체를 보낸다(현재 구현에서도 삭제함). 처리완료 알람
팝업과 답변 칸은 구현을 보류한다.

현재 구현된 GUI는 `eat_hwp_core`(삭제 대상)에 있고 구성이 다르다. 창 크기를 조절할 수 있고(최소 크기
있음), 대화 칸이 창과 함께 늘고 줄며 프롬프트 줄은 아래에 붙어 움직인다.

1. 대화 칸: 보낸 프롬프트와 받은 답변을 한 칸에 채팅처럼 쌓는다. `▶ 나 · 시각` / `◀ AI (모델, 걸린 시간) · 시각`
   머리줄로 말한 쪽과 그 글이 나온 시각(시스템 로컬 시간, `2026-11-09 14:03:21` 형식)을 보이고, 한 차례가 끝날 때마다 구분선을 넣는다. 응답 대기 중에는 대기 줄을 보이고,
   실패하면 `◀ 오류`로 남긴다. 각 요청은 독립이라 이전 대화를 LLM에 보내지 않는다
1. 대화 기록 저장: 문서와 API 종류(로컬/OpenAI 호환 `openai`, Claude `claude`)마다
   `%USERPROFILE%\.eat_hwp_raw\chats\{문서 파일 이름}_{문서 경로 해시}_{api}.txt`에 대화 칸 글 그대로
   (UTF-8) 저장한다. 요청이 끝날 때마다 쓰고, 같은 문서·API로 돌아오면(창을 열 때 한글의 현재 문서,
   요청 때 읽은 문서, 상태 점검 때의 API 종류) 그 기록을 불러와 이어서 쌓는다. 저장하지 않은 새 문서는
   모두 `제목없음_{api}.txt`를 함께 쓴다
1. 프롬프트 입력 칸과 보내기 버튼. Ctrl+Enter로도 보낸다(창의 가속기, Enter는 줄바꿈). 보내면 입력 칸을
   비운다. 처리완료 팝업은 없다
1. Claude API 키 입력 칸(가림 표시). 키를 넣으면 Claude API로 실행하고, 비우면 환경 변수 설정대로
   실행한다. 입력한 키는 저장하지 않으며 창을 닫으면 사라진다. LLM 설정을 서버에 두는 목표 구조에서는
   이 칸이 클라이언트 GUI에 남지 않는다
1. "시스템 프롬프트…" 버튼: 사용자 지정 시스템 프롬프트를 고치는 창(여러 줄 입력, 저장/취소)을 연다.
   `%USERPROFILE%\.eat_hwp_raw\system_prompt.txt`에 저장하고, 요청마다 읽어 규약 지시 뒤에 붙인다. 비우면
   붙이지 않는다
1. LLM에 보낼 문서 형식 칸(목록). 기본은 Markdown(`workflow::DocFormat::DEFAULT`)
   - **평문**: `GetTextFile`(HWPML2X)에서 글자만 뽑아 한 문단에 한 줄로 이은 것. 커서 자리에 `[[커서]]`
   - **HWPML (GetTextFile 원문)**: HWPML2X XML 그대로. 커서 자리에 `[[커서]]`, 그림 등 `BINDATASTORAGE`는 비움
   - **Markdown (HWPX 저장 → hwp2md)**: 임시 HWPX를 hwp2md로 바꾼 것. 제목·표 구조가 남고 커서 자리에 `[[커서]]`
   - **HWPML (SaveAs 파일)**: `SaveAs(경로, "HWPML2X", "lock:false")`로 저장한 파일. 커서 위치는 없다(책갈피를
     넣는 API가 hwp_automation_rs에 공개돼 있지 않음)

   형식마다 프롬프트에 문서 내용의 형식 설명이 붙고, 대화의 AI 머리줄에 형식 이름이 보인다
1. 모델 칸(편집형 콤보박스)과 "상태 확인" 버튼. 목록에서 고르거나 직접 입력하며, 비우면
   `EAT_HWP_LLM_MODEL` 설정대로 실행한다
1. LLM 상태 줄: 지금 설정(키 칸 포함)이 가리키는 서버의 상태를 보인다(✔ 작동 중 / ✖ 연결 안 됨 / ✖ 인증
   실패 / ✖ 키 없음 / ✖ 오류, 대상 서버, 응답 시간, 모델 수). 창을 열 때, "상태 확인"을 누를 때, 키 칸에서
   나갈 때 서버에 모델 목록을 요청해 점검하고(OpenAI 호환 서버는 `/models`, Claude API는 Models API) 모델
   칸 목록도 채운다. 요청이 성공하면 "작동 중"으로 바꾸고, 실패하면 다시 점검해 원인을 보인다.
   로컬/OpenAI 호환 서버에서 모델 칸의 모델과 설정의 기본 모델이 둘 다 서버 목록에 없으면 목록의 첫
   모델을 고르고 상태 줄에 알린다(Claude API는 목록에 없는 별칭도 받으므로 바꾸지 않고 경고만 한다)

## 미정 사항
서버 구조에서 아직 정하지 않았거나, 정한 내용끼리 맞지 않아 정리가 필요한 것들이다.

1. **애드온이 GUI 프로세스에 HWPML을 넘기는 방법**: 두 프로세스 사이의 전달 수단이 정해지지 않았다.
   임시 파일로 넘기는 방법은 유의사항의 "임시 파일을 만들지 않는다"와 맞는지 따져야 한다.
1. **호출 시 저장**: 한 번도 저장하지 않은 새 문서는 저장할 경로가 없다. 이때의 처리와, 사용자가 원하지
   않은 시점에 문서가 저장되는 점을 어떻게 다룰지.
1. **선택 범위**: `hwpml_with_caret`은 선택 블록이 있으면 블록의 앞쪽 끝만 돌려주고 선택을 푼다. 고칠
   범위의 끝을 HWPML 안에서 알 방법과, 삽입 전에 그 범위를 다시 선택할 방법이 없다. `GetSelectedPos`는
   hwp_automation_rs 0.4.0에 `HwpObject::get_selected_pos`(→ `ListParaRange`)로 들어왔다. 그 번호를 HWPML
   안의 위치로 바꾸는 방법은 아직 없다.
1. **원래 문서와 캐럿 위치로 돌아가기**: 응답을 기다리는 사이 사용자가 내용은 고치지 않고 캐럿만
   옮기거나 다른 문서 탭으로 갈 수 있다. GUI 프로세스가 원래 문서를 다시 찾는 방법, 호출된 캐럿 위치로
   돌아가는 방법이 필요하다. `SetPos`는 hwp_automation_rs에 아직 없다(추가 제시 대상).
1. **문단에 쓴 프롬프트**: 문단에 입력한 프롬프트를 GUI의 프롬프트 수정 칸에 어떻게 가져오는지, 결과를
   넣을 때 그 문단을 어떻게 하는지.
1. **컨텍스트 초과**: 문서 전체를 보내는데 실행환경 LLM 구성의 컨텍스트는 8K다. 문서가 이를 넘을 때의 처리.
1. **타깃이 둘인 워크스페이스의 빌드 설정**: `.cargo/config.toml`의 기본 타깃은 워크스페이스에 하나만
   둘 수 있고 지금은 `i686-pc-windows-msvc`다. 서버만 `x86_64-pc-windows-msvc`로 빌드하게 하는 방법
   (명령에 `--target` 지정, 워크스페이스 분리 등)과 WSL용 x64 링커 설정.

## Build
Windows 전용이다. 애드온은 WinSafe/COM을 쓰고 한글 COM이 32비트이므로 타깃이 `i686-pc-windows-msvc`다
(`.cargo/config.toml`에 기본 타깃으로 설정됨 → 아래 명령에 `--target` 불필요). 서버의 타깃은
`x86_64-pc-windows-msvc`이며, 빌드 설정은 아직 없다("미정 사항" 참조).
- `cargo build`: WSL에서는 [msvc-wsl-rust](https://github.com/strickczq/msvc-wsl-rust) 링커가 필요
  (`.cargo/config.toml`의 `linker` 경로를 환경에 맞게 수정).
- `cargo check`: 링크를 하지 않으므로 링커 없이 WSL에서 타입 검증 가능.
- 매니페스트: `crates/eat_hwp_addon/build.rs`가 `eat_hwp_addon.manifest`(공용 컨트롤 v6)를 링커
  `/MANIFEST:EMBED,ID=123`으로 DLL에 넣는다. rundll32는 대상 DLL의 ID 123 매니페스트를 활성화한 뒤
  진입점을 부르므로 GUI가 v6 컨트롤로 뜬다. 이것이 없으면 콤보박스 드롭다운이 펼쳐지지 않는다.

```bash
cargo build
cargo check
```

### 로그
개발 편의용 `tracing` 로그. 구독자는 애드온 크레이트가 `hwp_addon::logging`으로 설치하고, 파일은 대화 기록과
같은 `%USERPROFILE%\.eat_hwp_raw\chats`에 둔다(한글 프로세스의 애드온은 `eat_hwp_addon.log`, rundll32 GUI
프로세스는 `eat_hwp_raw_gui.log`, 시작할 때마다 비움). 수준은 `EAT_HWP_LOG`(error/warn/info/debug/trace, 기본
debug). trace면 LLM에 보내는 프롬프트 전문(문서 내용 포함)까지 남는다. API 키는 남기지 않는다.

### unsafe policy
워크스페이스의 모든 크레이트(`crates/eat_hwp_addon`, `crates/eat_hwp_server`, 삭제 전까지의 `crates/eat_hwp_core`) 루트에
`#![forbid(unsafe_code)]` 적용. DLL export(`QueryUserActionInterface`, `RunGuiW`)는 `hwp_addon`의
`export_hwp_addon!`/`export_rundll_entry!` 매크로로 생성하고, Win32 호출은 WinSafe의 safe 래퍼를 쓴다.
unsafe가 필요한 기능은 이 프로젝트에 두지 않고 hwp_automation_rs에 추가하도록 제시한다.

## Dependencies
### 서버
1. 백엔드는 [rig-rs](https://docs.rig.rs/)
1. 서버(`eat_hwp_server`)는 [axum](https://docs.rs/axum) 기반 REST API
1. Windows 10 이상 전용
1. 받은 HWPML 문자열은 [roxmltree](https://docs.rs/roxmltree)로 파싱

### 클라이언트
1. Windows GUI는 [WinSafe](https://docs.rs/winsafe/latest/winsafe/gui/index.html)
1. [hwp_automation_rs](https://github.com/sephiron99/hwp_automation_rs) 에서 hwp_com을 기반으로 작성하되, 필요할 때에는 hwp_addon도 사용한다.
1. 문서 내용: `GetTextFile`(`GetTextFileFormat::HwpMl2X`)로 HWPML 문자열을 받는다.
   커서 위치는 `hwp.ext_doc().hwpml_with_caret()`(`ExtDoc`)이 돌려주는 HWPML 안의 바이트 위치로 찾는다(`GetPos` 번호는 쓰지 않음)
1. Markdown 형식: `hwp.ext_doc().hwpx_with_caret()`로 커서 책갈피가 든 임시 HWPX를 저장하고, 책갈피를
   `[[커서]]` 글자로 바꾼 사본(zip)을 [hwp2md](https://github.com/sephiron99/hwp2md)(로컬 `../hwp2md`,
   GPL-3.0-only)로 Markdown으로 바꾼다. hwp2md의 comrak은 기본 기능(syntect의 C 라이브러리)을 꺼야
   i686-pc-windows-msvc로 교차 빌드된다

## 유의사항 및 준수사항
1. hwp_automation_rs에 없는 hwp com/action/api는 hwp_automation_rs에 해당
   기능을 추가하도록 제시. 단, addon같은 산출물은 hwp_automation_rs에 속하지
   않고, 이 프로젝트의 일부임.
1. eat_hwp_raw 프로젝트에 hwp action/method/parameter을 추가하지 못함.
1. 문서 내용은 GUI에서 고른 형식에 따라 읽고, 결과는 SetTextFile로 넣는다. 기본 형식인 Markdown과 HWPML
   파일 형식은 SaveAs(`lock:false`)로 사용자 임시 디렉터리에 HWPX·HWPML 파일을 만들고 읽은 뒤 지운다.
   평문과 HWPML 원문 형식은 GetTextFile(HWPML2X)로 읽는다.

## 라이선스
GPL-3.0-or-later. 전문은 `LICENSE`, 각 크레이트 `Cargo.toml`의 `license` 필드도 같다. GPL은 이 저장소의
코드에만 적용되며, 한글 프로그램이나 한글 오토메이션을 이용할 권리는 포함하지 않는다.

### 한글 오토메이션 이용 조건
- **비공식 프로젝트**: 이 소프트웨어는 주식회사 한글과컴퓨터(이하 한컴)와 무관한 독립적 결과물이며, 한컴의
  승인·보증·지원을 받지 않았다.
- **이용 조건**: 한컴은 한글 오토메이션을 개인이 비상업적인 목적으로 이용하는 경우에 한해 자유롭게 쓸 수
  있다고 안내하며, 상업적으로 판매되는 솔루션이나 응용 프로그램 개발 등 상업적 목적의 이용에는 한컴의
  승인을 통한 별도 라이선스를 요구한다(hwp_automation_rs README 기준, 2026년 10월 확인). 이 프로젝트는
  hwp_automation_rs를 통해 한글 오토메이션을 쓰므로 이 프로젝트를 쓰는 사람에게도 같은 조건이 적용된다.
  최신 조건은 [한컴 디벨로퍼의 한글 오토메이션 안내](https://developer.hancom.com/hwpautomation)에서 직접
  확인한다.
- **상표**: "한글", "한컴", "HWP", "HWPX"는 한컴의 상표다. 이 프로젝트는 호환 대상을 가리키기 위해서만 이
  이름을 쓴다.

This is an unofficial project, not affiliated with or endorsed by Hancom Inc. It uses HWP Automation
through hwp_automation_rs. Hancom states that HWP Automation is free only for personal, non-commercial
use and that commercial use requires a separate license from Hancom; the same applies to anyone using
this project. "Hangul", "Hancom", "HWP", and "HWPX" are trademarks of Hancom Inc., used here only to
identify the product this project works with.

## LLM 서버
LLM을 돌리는 곳은 세 가지다: 개발환경의 로컬 서버, 실행환경의 로컬 서버, 외부 llm service(Claude API).
로컬 서버는 local or intranet llm server(gemma4 or etc)다. `eat_hwp_server`는 LLM 서버와 같은 기기에 둔다.
개발환경에서는 `eat_hwp_server`(Windows 실행파일)가 Windows에서 돌고, 같은 기기의 WSL2에 있는 Ollama에
localhost로 닿는다.

### LLM 연결 설정 (env)
LLM 설정은 서버(`eat_hwp_server`)에 둔다. 현재 코드에서는 `eat_hwp_core`가 Windows 측에서 읽는다.

| 환경 변수 | 기본값 | 개발환경 (WSL2 Ollama) | 실행환경 (Windows Ollama) | Claude API |
|---|---|---|---|---|
| `EAT_HWP_LLM_PROVIDER` | `openai` (OpenAI 호환 로컬/원격 서버) | 지정 안 함 | 지정 안 함 | `anthropic` |
| `EAT_HWP_LLM_BASE_URL` | `http://localhost:11434/v1` | 기본값 그대로(mirrored라 그대로 닿음) | 기본값 그대로 | 지정하지 않는다(공식 주소 사용) |
| `EAT_HWP_LLM_MODEL` | `gemma4:e4b-it-qat` (실행환경 모델) | 12B를 쓸 때만 `gemma4:12b` 지정 | 기본값 그대로. E2B 폴백 시에만 `gemma4:e2b-it-qat` 지정 | 모델 ID. 기본값 `claude-opus-5-5` |
| `EAT_HWP_LLM_API_KEY` | 빈 값 | 빈 값(Ollama는 불필요) | 빈 값 | Claude API 키. 없으면 `ANTHROPIC_API_KEY`를 읽는다 |

- 모델 이름은 `ollama list` 태그와 정확히 일치해야 한다.
- 현재 구현에서는 Claude API 키를 GUI의 키 입력 칸에 넣어도 되며, 그때는 `EAT_HWP_LLM_PROVIDER` 설정
  없이도 Claude API로 실행한다.
- Claude API를 쓰면 문서 내용이 외부 서버로 전송된다.

### 개발환경 (WSL2, RTX 5070)
1. windows -> wsl2(localhost) 접속은 설정완료(ex: windows chrome browser에서
    wsl2에서 실행한 웹서버 localhost:8000 으로 접속가능)
1. vram 12GB, RTX5070

서버 구성 (개발용: RTX 5070 12GB)
- **용도**: WSL2는 개발환경이다. 실제 실행환경은 아래 "실행환경"이며, 최종 검증은
  실행환경 모델(`gemma4:e4b-it-qat`)로 한다 — 이 서버에 함께 pull해 두면 모델 env만 바꿔 확인 가능.
- **서빙: Ollama** — 포트 `11434`, OpenAI 호환 `/v1`. 단일 사용자 GUI엔 충분하고 GGUF
  양자화로 12GB에 적합. (대규모 동시성/원격 DGX로 확장 시 vLLM `:8000/v1`로 교체 — 앱은
  base URL env만 변경.)
- **모델: Gemma 4 12B dense, `Q4_K_M`** — 가중치 ~7–8GB로 12GB에 KV 캐시 여유. 26B A4B(MoE)는
  Q4 ~15GB로 12GB 초과(CPU 스필) → 제외. E4B는 양식 준수가 약해 폴백용.
- **컨텍스트: `num_ctx` 16K–32K** — 256K 풀은 KV 캐시가 VRAM을 초과. 문서+RAG를 커버하는 실용값.
- **드라이버/주의**: RTX 5070은 Blackwell(sm_120) → Windows에 최신 NVIDIA WSL 지원 드라이버
  (R570+) + 최신 Ollama 빌드 필수(구버전은 GPU 미인식). WSL 내부엔 Linux NVIDIA 드라이버 설치 금지.
- **`.wslconfig`**(`%UserProfile%\.wslconfig`): `[wsl2]` `memory=24GB`, `processors=<대부분>`,
  `swap=8GB`, `networkingMode=mirrored`(localhost 접속 보장, 이미 동작). vhdx 여유 60GB+.

구동
```bash
# WSL2(Ubuntu 24.04) 안에서
curl -fsSL https://ollama.com/install.sh | sh        # 최신 = Blackwell 지원
ollama --version
ollama pull gemma4:12b                                 # 정확 태그는 `ollama list`로 확인
# (라이브러리에 없으면) HF GGUF를 import:
#   printf 'FROM ./gemma4-12b-Q4_K_M.gguf\nPARAMETER num_ctx 32768\n' > Modelfile
#   ollama create gemma4:12b -f Modelfile
ollama run gemma4:12b "한국어로 인사해줘"             # GPU 로드 + 한국어 확인
```

### 실행환경 (Windows 10/11, Intel 내장 그래픽)
1. 실제 실행환경: windows 10/11, intel 내장 그래픽, minimum ram 16GiB
1. intel 내장 그래픽 구동 가능한 llm

서버 구성 (Intel 내장 그래픽, 최소 RAM 16GiB)
- **용도**: 실제 사용자 PC 구성. 앱 기본값과 양식 준수 품질 평가는 이 구성을 기준으로 한다.
- **서빙: Ollama (Windows 네이티브)** — 포트 `11434`, OpenAI 호환 `/v1`. 한글·eat_hwp_raw와
  같은 Windows에서 직접 실행하므로 WSL2가 필요 없다.
- **모델: 기본 `gemma4:e4b-it-qat`(6.1GB)** — RAM 16GiB를 Windows·한글·내장 그래픽 공유 메모리와
  나눠 쓰므로 가중치 ~6GB가 상한. 후보와 선정 기준은 아래 "모델 후보" 참조(실기 측정 전 잠정).
- **컨텍스트: `num_ctx` 8K** — OpenAI 호환 API로는 요청별 지정이 안 되므로 서버 env
  `OLLAMA_CONTEXT_LENGTH=8192`로 설정. 문서 전체를 보내는 목표 구조에서는 초과하기 쉽다("미정 사항" 참조). 현재 구현도 항상 문서 전체를 보낸다.
- **가속/주의**: **CPU 추론을 기준선으로 잡는다.** 내장 그래픽 가속은 Vulkan 백엔드 경유인데
  (Windows는 Intel 그래픽 드라이버에 Vulkan 포함), Ollama가 내장 그래픽을 기본으로 건너뛰어
  `OLLAMA_IGPU_ENABLE=1`이 필요하다는 보고(공식 문서엔 없음)와 Iris Xe 미인식·출력 깨짐·러너 크래시
  사례가 있다. 가속이 되면 덤이고, 증상이 있으면 `OLLAMA_VULKAN=0`으로 CPU 전환. 어느 쪽이든
  개발환경(RTX)보다 응답이 크게 느리므로 모델 크기가 곧 체감 속도다.

모델 후보 (16GiB·CPU/내장 그래픽에서 구동 가능)

크기는 Ollama 태그 목록 기준, 벤치마크는 각 모델 카드의 자체 발표치다. **이 환경에서 직접 돌려본
결과가 아니므로** 실기에서 속도(tok/s)와 양식 준수를 측정해 확정한다.

| 후보 | 크기 | 라이선스 | 구동 | 비고 |
|---|---|---|---|---|
| Gemma 4 E4B QAT | 6.1GB | 확인 필요 | `gemma4:e4b-it-qat` | 현 기본값. 후보 중 가장 크고 느림. 컨텍스트 128K |
| Gemma 4 E2B QAT | 4.3GB | 확인 필요 | `gemma4:e2b-it-qat` | E4B가 느리거나 메모리 부족일 때 |
| Mi:dm 2.0 Mini (KT, 2.3B) | Q4 GGUF(소형) | MIT | 커뮤니티 GGUF `hf.co/mykor/Midm-2.0-Mini-Instruct-gguf` | 한국어 특화. 모델 카드상 한국어 지시 이행 73.6로 Qwen3-4B(69.4)·EXAONE 3.5 2.4B(68.9)보다 높음. 공식 GGUF는 Base만 제공 |
| Kanana 1.5 2.1B (Kakao) | Q4 GGUF(소형) | Apache-2.0 | 커뮤니티 GGUF(HF) | 한국어 특화. 컨텍스트 32K 기본 |
| Qwen3 4B / 8B | 2.5GB / 5.2GB | 확인 필요 | `qwen3:4b`, `qwen3:8b` | 다국어 범용. Ollama 공식 라이브러리 |
| EXAONE 3.5 2.4B / 7.8B (LG) | 1.6GB / 4.8GB | **비상업(NC)** | `exaone3.5:2.4b`, `exaone3.5:7.8b` | 한국어 품질은 좋으나 연구·비상업 용도 한정 → 배포용 기본값으로 부적합 |

- **제외**: Gemma 4 12B(7.2–8.0GB)·Mi:dm 2.0 Base(11.5B)는 16GiB에서 스왑 위험, 26B 이상·EXAONE 32B·
  HyperCLOVA X SEED 14B/32B는 메모리 초과.
- **선정 순서(제안)**: ① `gemma4:e4b-it-qat`가 실기에서 쓸 만한 속도면 유지 → ② 느리면 한국어 특화
  소형인 Mi:dm 2.0 Mini(MIT)와 `gemma4:e2b-it-qat`를 같은 문서로 비교 → ③ 그래도 부족하면 `qwen3:4b`.
  어느 것이든 앱은 `EAT_HWP_LLM_MODEL`만 바꾸면 된다.

구동
```powershell
# Windows(PowerShell). Ollama for Windows 최신 버전 설치 후
setx OLLAMA_CONTEXT_LENGTH 8192                        # 설정 후 Ollama 재시작(트레이 종료 → 재실행)
ollama pull gemma4:e4b-it-qat
ollama run gemma4:e4b-it-qat "한국어로 인사해줘"       # 한국어 출력 확인
ollama ps                                              # PROCESSOR 열로 GPU/CPU 로드 확인
ollama run gemma4:e4b-it-qat --verbose "한 문단 써줘"   # eval rate(tok/s) 측정 → 모델 선정 근거
# 출력이 깨지거나 크래시하면: setx OLLAMA_VULKAN 0 후 Ollama 재시작(CPU 추론)
```

### Claude API (외부 llm service)
로컬 서버 대신 Anthropic Claude API를 쓴다. 설정은 위 환경 변수 표의 "Claude API" 열을 따른다.
