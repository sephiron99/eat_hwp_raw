//! winsafe 기반 GUI 셸 (CLAUDE.md "eat_hwp_raw GUI"의 현재 구현).
//!
//! 목표 GUI 구성(CLAUDE.md "eat_hwp_raw GUI")은 LLM 서버·모델 지정, 프롬프트 수정, 사용자 지정 시스템
//! 프롬프트 편집, 전송·통신 상황 표시다.
//!
//! 메인 창:
//! - LLM 상태 줄: 지금 설정이 가리키는 서버가 작동하는지 보인다. 창을 열 때, 설정 창에서 키나 모델을
//!   바꿨을 때 서버에 모델 목록을 요청해 점검하고, 요청이 성공하면 "작동 중"으로, 실패하면 다시
//!   점검해 원인(연결 안 됨, 인증 실패 등)을 보인다
//! - 설정 요약 줄(모델, 보낼 문서 형식, 키 입력 여부)과 "설명서", "설정…" 버튼. "설명서"는 사용자
//!   설명서(`manual.txt`)를 보이는 창을 연다
//! - 대화 칸(read-only multiline Edit): 보낸 프롬프트와 받은 답변을 채팅처럼 차례로 쌓는다
//!   - `▶ 나 · 시각` / `◀ AI (모델, 걸린 시간) · 시각` 머리줄로 말한 쪽과 그 글이 대화 칸에 나온
//!     시각(시스템 로컬 시간, 초 단위)을 보이고, 한 차례가 끝날 때마다
//!     구분선을 넣는다. 응답을 기다리는 동안은 `◀ AI — 응답을 기다리는 중…`을 보이고, 답이 오면
//!     그 줄을 답변으로 바꾼다. 실패하면 `◀ 오류`로 남긴다
//!   - 각 요청은 독립이다. 이전 대화는 LLM에 보내지 않는다
//!   - 대화는 문서와 API 종류(로컬/OpenAI 호환, Claude)마다 `chat_store`가 정한 파일에 저장되고,
//!     같은 문서·API로 돌아오면 다시 불러온다. 문서는 창을 열 때 정해지고(대상 문서), API 종류는 메인
//!     창의 상태를 점검할 때 그 설정을 기준으로 바꾼다
//! - 창은 처음 띄운 문서(대상 문서)에 고정된다. "보내기"는 한글에서 어느 탭을 보고 있든 그 문서를
//!   읽고, 답도 그 문서에만 넣는다. 대상 문서가 닫히면 창도 닫는다(`bind_document`, `watch_document`).
//!   닫힘은 한글의 이벤트(`DocumentAfterClose`, `DocumentAfterSave`, `Quit`)로 안다
//! - "다른 열린 문서도 함께 보내기" 체크 칸: 켜면 한글에 열려 있는 다른 문서도 같은 형식으로 읽어 참고
//!   문서로 함께 보낸다. 바꾸면 `settings.txt`에 남는다. "문서 고르기…" 버튼으로 보낼 문서를 고를 수
//!   있다(고른 적이 없으면 모두 보낸다)
//! - 프롬프트 입력 칸 + 보내기 버튼(Ctrl+Enter로도 보낸다). 보내면 입력 칸을 비운다. 처리완료
//!   팝업은 없다
//! - "커서 위치에 삽입" 버튼: 마지막으로 받은 답을 원래 문서(요청 때 읽은 문서)의 커서 자리에 넣고,
//!   넣었는지를 대화 칸에 한 줄로 적는다. 받은 답이 없거나 요청·넣기 중에는 꺼진다
//!
//! 설정 창("설정…" 버튼, 모달):
//! - Claude API 키 칸(비우면 환경 변수 설정대로 실행, 저장하지 않음)
//! - 로컬/OpenAI 호환 서버 주소 칸(비우면 `EAT_HWP_LLM_BASE_URL` 설정)
//! - 모델 칸(목록에서 고르거나 직접 입력)과 "상태 확인" 버튼, 상태 줄. 상태 확인과 키·주소 칸에서 나갈
//!   때의 점검은 설정 창 안에서만 보이고, "확인"을 눌러야 메인 창에 적용된다
//! - LLM에 보낼 문서 형식(평문 / HWPML 원문 / HWPX→Markdown / SaveAs HWPML 파일)
//! - MCP 서버 칸(`settings`의 `mcp_servers.txt`, 형식은 `mcp::parse_servers`)과 "MCP 확인" 버튼
//! - 고정 프롬프트(코드가 정한 규약, 읽기 전용, 고른 형식 기준)와 사용자 지정 지침 칸
//!   (`settings`의 `system_prompt.txt`)
//! - "확인"은 사용자 지정 지침, 서버 주소·모델·형식(`settings.txt`), MCP 서버를 파일에 쓰고 적용한다.
//!   키는 저장하지 않고 이번 실행에만 쓴다. "취소"는 바꾼 값을 버린다
//!
//! 창을 열 때 `settings.txt`의 서버 주소·모델·형식을 읽어 적용한다.
//!
//! LLM에는 항상 문서 전체를 커서 위치 표시와 함께 보낸다. MCP 서버가 있으면 그 도구도 주고, 부른 도구
//! 기록(`⚙ …`)은 대화 칸의 답 앞에 보인다(문서에는 답만 넣는다). 답은 받기만 하고, 문서에는 "커서 위치에
//! 삽입" 버튼을 눌러야 넣는다(`hwp::insert_into`).
//!
//! 문서 내용은 `hwp` 모듈이 실행 중인 한글에서 `GetTextFile`(HWPML2X)로 읽어 온다. 한글 없이
//! 테스트할 때는 환경변수 `EAT_HWP_DOC`에 HWPML 파일(UTF-8) 경로를 지정하면 그 파일을 사용한다.

use crate::chat_store;
use crate::config::{Config, Provider};
use crate::hwp::{self, DocContent, DocSnapshot};
use crate::llm::{self, Completion, RigBackend, ServerState, ServerStatus};
use crate::markdown;
use crate::mcp;
use crate::settings;
use crate::workflow::{self, DocFormat};
use anyhow::{Result, anyhow};
use hwp_com::ext::HwpEvent;
use hwp_com::method::events::EventConnection;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use winsafe::{co, gui, prelude::*};

/// 보내기 Button의 고정 컨트롤 ID.
/// 워커 스레드 완료 후 UI 스레드에서 `HWND::GetDlgItem`으로 컨트롤을 찾는 데 쓴다
/// (winsafe 컨트롤은 Send가 아니라 스레드 경계를 넘길 수 없으므로 ID로 우회).
const ID_BTN_RUN: u16 = 1002;

/// Ctrl+Enter 가속기의 명령 ID. 창 어디에서 눌러도 보내기 버튼을 누른 것과 같다.
const ID_ACCEL_SEND: u16 = 2001;

/// 사용자 설명서. "설명서" 창에 보인다.
const MANUAL: &str = include_str!("manual.txt");

/// 대화 칸에서 한 차례(프롬프트 + 답변)가 끝날 때마다 넣는 구분선.
const TURN_SEPARATOR: &str = "────────────────────────────";

thread_local! {
    /// 메인 창의 상태 줄·요약 줄, 적용된 설정, 열려 있는 설정 창의 컨트롤. winsafe 컨트롤은 스레드를
    /// 못 넘기므로, 서버를 점검한 워커가 `run_ui_thread`로 UI 스레드에 돌아온 뒤 여기서 꺼내 쓴다.
    /// UI 스레드에서만 쓴다.
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };

    /// 대화 칸과, 응답 대기 줄이 시작하는 위치(UTF-16 글자 수). UI 스레드에서만 쓴다.
    static CHAT: RefCell<Option<Chat>> = const { RefCell::new(None) };
}

struct Ui {
    /// 환경 변수에서 읽은 설정. 설정 창의 키·서버 주소·모델을 여기에 덮어써 쓴다.
    base: Arc<Config>,
    /// 메인 창의 LLM 상태 줄.
    status: gui::Label,
    /// 메인 창의 설정 요약 줄.
    summary: gui::Label,
    session: Session,
    /// 열려 있는 설정 창의 컨트롤. 닫혀 있으면 `None`.
    settings: Option<SettingsUi>,
    /// "커서 위치에 삽입" 버튼.
    btn_insert: gui::Button,
    /// 마지막으로 받은 답. 보내기를 누르면 비운다.
    answer: Option<Answer>,
    /// "다른 열린 문서도 함께 보내기" 체크 칸과 "문서 고르기…" 버튼.
    chk_others: gui::CheckBox,
    btn_pick: gui::Button,
    /// "문서 고르기"에서 고른 문서. 고른 적이 없으면 `None`(열린 문서를 모두 보낸다). 한글의 문서 ID는
    /// 한글을 다시 띄우면 달라지므로 저장하지 않는다.
    picked_docs: Option<Vec<hwp::DocIdentity>>,
    /// 이 창이 고정된 대상 문서. 애드온이 넘긴 문서이고, 넘기지 않았으면 창을 열 때 한글의 활성 문서다.
    /// "보내기"는 항상 이 문서를 읽고 답도 이 문서에만 넣는다. 한글에 붙지 못했으면 `None`.
    bound: Option<hwp::DocIdentity>,
}

/// "커서 위치에 삽입"으로 넣을 답.
#[derive(Clone)]
struct Answer {
    text: String,
    /// 요청 때 읽은 문서. 이 문서에만 넣는다.
    origin: Option<hwp::DocIdentity>,
}

/// 설정 창에서 적용한 값. 키를 뺀 나머지는 `settings.txt`에 저장한다.
#[derive(Clone)]
struct Session {
    /// Claude API 키. 비우면 환경 변수 설정대로 실행한다. 저장하지 않는다.
    api_key: String,
    /// 로컬/OpenAI 호환 서버 주소. 비우면 `EAT_HWP_LLM_BASE_URL` 설정대로 실행한다.
    base_url: String,
    /// 모델. 비우면 `EAT_HWP_LLM_MODEL` 설정대로 실행한다.
    model: String,
    format: DocFormat,
    /// 한글에 열려 있는 다른 문서도 참고 문서로 함께 보낼지(메인 창의 체크 칸).
    include_open_docs: bool,
    /// 메인 창의 마지막 점검에서 받은 모델 목록. 설정 창의 모델 칸 목록으로 쓴다.
    models: Vec<String>,
}

/// 설정 창에서 점검 결과를 보일 컨트롤.
struct SettingsUi {
    cmb_model: gui::ComboBox,
    lbl_status: gui::Label,
    /// MCP 서버 점검 결과 줄과 "MCP 확인" 버튼.
    lbl_mcp_status: gui::Label,
    btn_mcp_check: gui::Button,
}

/// 서버 점검 결과를 보일 곳.
#[derive(Clone, Copy, Debug)]
enum Target {
    /// 메인 창: 적용된 설정의 상태. 모델을 고르고 대화의 API 종류를 바꾼다.
    Main,
    /// 설정 창: 아직 적용하지 않은 키·모델의 상태. 설정 창의 모델 칸만 채운다.
    Settings,
}

/// `UI`를 빌려 `f`를 부른다(UI 스레드). 아직 없으면 `None`.
fn with_ui<R>(f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    UI.with_borrow_mut(|ui| ui.as_mut().map(f))
}

struct Chat {
    log: gui::Edit,
    pending_start: Option<i32>,
    /// 대화 칸이 지금 보여 주는 문서의 경로. 아직 모르면 `None`.
    doc_path: Option<String>,
    /// 지금 설정의 API 종류.
    provider: Provider,
    /// `doc_path`와 `provider`로 정한 기록 파일. 문서를 아직 모르면 `None`(저장하지 않음).
    file: Option<PathBuf>,
}

/// 메인 윈도우와 컨트롤을 보유한다. 설정과 상태 줄은 `UI`에 둔다.
pub struct MainWindow {
    wnd: gui::WindowMain,
    // 라벨은 컨트롤 수명 유지를 위해 보유만 한다(읽지 않음).
    #[allow(dead_code)]
    lbl_prompt: gui::Label,
    txt_prompt: gui::Edit,
    btn_run: gui::Button,
    #[allow(dead_code)]
    lbl_chat: gui::Label,
    txt_chat: gui::Edit,
    #[allow(dead_code)]
    lbl_status: gui::Label,
    #[allow(dead_code)]
    lbl_summary: gui::Label,
    btn_settings: gui::Button,
    btn_manual: gui::Button,
    btn_insert: gui::Button,
    chk_others: gui::CheckBox,
    btn_pick: gui::Button,
}

impl MainWindow {
    /// 윈도우와 컨트롤을 생성하고 이벤트를 연결한다.
    pub fn new(cfg: Config) -> Self {
        // Ctrl+Enter → ID_ACCEL_SEND. 가속기는 키 입력이 Edit에 가기 전에 처리되므로 프롬프트 칸에
        // 줄바꿈이 들어가지 않는다(Enter만 누르면 그대로 줄바꿈).
        let accel_table = winsafe::HACCEL::CreateAcceleratorTable(&[winsafe::ACCEL {
            fVirt: co::ACCELF::VIRTKEY | co::ACCELF::CONTROL,
            key: co::VK::RETURN,
            cmd: ID_ACCEL_SEND,
        }])
        .ok();
        // 애드온이 넘긴 대상 문서. 창 제목에 바로 넣어, 애드온이 이 문서의 창을 곧바로 찾을 수 있게 한다.
        let bound = crate::app::target_from_env();
        let wnd = gui::WindowMain::new(gui::WindowMainOpts {
            accel_table,
            title: &crate::app::window_title(bound.as_ref()),
            size: gui::dpi(520, 504),
            // 크기 조절 테두리와 최대화 버튼. 대화 칸이 늘고 줄며, 아래 줄은 바닥에 붙어 움직인다.
            style: co::WS::CAPTION
                | co::WS::SYSMENU
                | co::WS::CLIPCHILDREN
                | co::WS::BORDER
                | co::WS::VISIBLE
                | co::WS::SIZEBOX
                | co::WS::MINIMIZEBOX
                | co::WS::MAXIMIZEBOX,
            ..Default::default()
        });

        // 프롬프트 칸이 창을 열 때 포커스를 받도록 가장 먼저 만든다(배치는 맨 아래).
        let lbl_prompt = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "프롬프트 (Ctrl+Enter로 보내기)",
                position: gui::dpi(10, 386),
                resize_behavior: (gui::Horz::None, gui::Vert::Repos),
                ..Default::default()
            },
        );
        let txt_prompt = gui::Edit::new(
            &wnd,
            gui::EditOpts {
                position: gui::dpi(10, 404),
                resize_behavior: (gui::Horz::Resize, gui::Vert::Repos),
                width: gui::dpi_x(380),
                height: gui::dpi_y(70),
                control_style: co::ES::MULTILINE
                    | co::ES::WANTRETURN
                    | co::ES::AUTOVSCROLL
                    | co::ES::NOHIDESEL,
                ..Default::default()
            },
        );
        let btn_run = gui::Button::new(
            &wnd,
            gui::ButtonOpts {
                text: "보내기",
                position: gui::dpi(400, 404),
                resize_behavior: (gui::Horz::Repos, gui::Vert::Repos),
                width: gui::dpi_x(110),
                ctrl_id: ID_BTN_RUN,
                ..Default::default()
            },
        );
        let btn_insert = gui::Button::new(
            &wnd,
            gui::ButtonOpts {
                text: "커서 위치에 삽입",
                position: gui::dpi(400, 436),
                resize_behavior: (gui::Horz::Repos, gui::Vert::Repos),
                width: gui::dpi_x(110),
                ..Default::default()
            },
        );
        let chk_others = gui::CheckBox::new(
            &wnd,
            gui::CheckBoxOpts {
                text: "다른 열린 문서도 함께 보내기",
                position: gui::dpi(215, 384),
                resize_behavior: (gui::Horz::None, gui::Vert::Repos),
                ..Default::default()
            },
        );
        let btn_pick = gui::Button::new(
            &wnd,
            gui::ButtonOpts {
                text: "문서 고르기…",
                position: gui::dpi(410, 379),
                resize_behavior: (gui::Horz::None, gui::Vert::Repos),
                width: gui::dpi_x(100),
                ..Default::default()
            },
        );
        let lbl_chat = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "대화",
                position: gui::dpi(10, 58),
                ..Default::default()
            },
        );
        let txt_chat = gui::Edit::new(
            &wnd,
            gui::EditOpts {
                position: gui::dpi(10, 76),
                resize_behavior: (gui::Horz::Resize, gui::Vert::Resize),
                width: gui::dpi_x(500),
                height: gui::dpi_y(300),
                control_style: co::ES::MULTILINE
                    | co::ES::AUTOVSCROLL
                    | co::ES::NOHIDESEL
                    | co::ES::READONLY,
                window_style: co::WS::CHILD
                    | co::WS::VISIBLE
                    | co::WS::TABSTOP
                    | co::WS::GROUP
                    | co::WS::VSCROLL,
                ..Default::default()
            },
        );
        CHAT.with_borrow_mut(|chat| {
            *chat = Some(Chat {
                log: txt_chat.clone(),
                pending_start: None,
                doc_path: None,
                provider: cfg.llm_provider,
                file: None,
            })
        });
        // 맨 위에 놓이지만 마지막에 만들어, 창을 열면 프롬프트 칸이 먼저 포커스를 받게 한다.
        let lbl_status = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "LLM 상태: 확인 전",
                position: gui::dpi(10, 8),
                resize_behavior: (gui::Horz::Resize, gui::Vert::None),
                // 상태 문구가 바뀌어 길어지므로 글자 길이에 맞추지 않고 폭을 고정한다.
                size: gui::dpi(500, 16),
                ..Default::default()
            },
        );
        let lbl_summary = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "",
                position: gui::dpi(10, 33),
                resize_behavior: (gui::Horz::Resize, gui::Vert::None),
                size: gui::dpi(310, 16),
                ..Default::default()
            },
        );
        let btn_manual = gui::Button::new(
            &wnd,
            gui::ButtonOpts {
                text: "설명서",
                position: gui::dpi(330, 28),
                resize_behavior: (gui::Horz::Repos, gui::Vert::None),
                width: gui::dpi_x(80),
                ..Default::default()
            },
        );
        let btn_settings = gui::Button::new(
            &wnd,
            gui::ButtonOpts {
                text: "설정…",
                position: gui::dpi(420, 28),
                resize_behavior: (gui::Horz::Repos, gui::Vert::None),
                width: gui::dpi_x(90),
                ..Default::default()
            },
        );

        let cfg = Arc::new(cfg);
        let saved = settings::load_gui_settings();
        tracing::info!(
            base_url = %saved.base_url,
            model = %saved.model,
            format = ?saved.format,
            "GUI 설정 읽음"
        );
        UI.with_borrow_mut(|ui| {
            *ui = Some(Ui {
                base: Arc::clone(&cfg),
                status: lbl_status.clone(),
                summary: lbl_summary.clone(),
                session: Session {
                    api_key: String::new(),
                    base_url: saved.base_url,
                    model: saved.model,
                    format: saved.format.unwrap_or(DocFormat::DEFAULT),
                    include_open_docs: saved.include_open_docs,
                    models: Vec::new(),
                },
                settings: None,
                btn_insert: btn_insert.clone(),
                answer: None,
                chk_others: chk_others.clone(),
                btn_pick: btn_pick.clone(),
                picked_docs: None,
                bound: bound.clone(),
            })
        });

        let new_self = Self {
            wnd,
            lbl_prompt,
            txt_prompt,
            btn_run,
            lbl_chat,
            txt_chat,
            lbl_status,
            lbl_summary,
            btn_settings,
            btn_manual,
            btn_insert,
            chk_others,
            btn_pick,
        };
        new_self.events();
        new_self
    }

    /// 실행 버튼 클릭 → 워커 스레드에서 워크플로 실행 → UI 스레드로 결과 반영.
    ///
    /// LLM/COM/파싱은 시간이 걸리므로 워커 스레드(`std::thread::spawn`)에서 수행해 UI가 멈추지
    /// 않게 한다. winsafe 컨트롤은 Send가 아니므로 워커에는 Send인 `wnd`/`Arc` 상태만 넘기고,
    /// 완료 시 `wnd.run_ui_thread`로 UI 스레드에 복귀해 컨트롤을 갱신한다.
    fn events(&self) {
        let wnd = self.wnd.clone();
        let txt_prompt = self.txt_prompt.clone();
        let btn_run = self.btn_run.clone();

        // 창이 만들어지면 요약 줄을 채우고 서버를 점검한다(모델 목록도 이때 받는다).
        {
            let wnd2 = wnd.clone();
            let txt_chat = self.txt_chat.clone();
            let chk_others = self.chk_others.clone();
            self.wnd.on().wm_create(move |_| {
                // 여러 차례 쌓여도 잘리지 않게 글자 수 제한을 최대로 둔다(기본은 약 3만 자).
                txt_chat.limit_text(None);
                if let Some(cfg) = with_ui(|ui| {
                    chk_others.set_check(ui.session.include_open_docs);
                    ui.btn_insert.hwnd().EnableWindow(false);
                    update_summary(ui);
                    check_cfg(ui)
                }) {
                    check_server(&wnd2, cfg, Target::Main);
                }
                bind_document(&wnd2);
                Ok(0)
            });
        }

        // 창을 너무 작게 줄여 컨트롤이 겹치지 않게 최소 크기를 둔다(창 테두리 포함 크기).
        self.wnd.on().wm_get_min_max_info(|p| {
            let (w, h) = gui::dpi(440, 400);
            p.info.ptMinTrackSize = winsafe::POINT::with(w, h);
            Ok(())
        });

        // 다른 열린 문서도 함께 보내기: 바꾸면 바로 적용하고 설정 파일에 남긴다.
        {
            let chk_others = self.chk_others.clone();
            self.chk_others.on().bn_clicked(move || {
                let saved = with_ui(|ui| {
                    ui.session.include_open_docs = chk_others.is_checked();
                    // 하나도 고르지 않은 채 다시 켜면 보낼 문서가 없으므로 모두 보내는 것으로 되돌린다.
                    if ui.picked_docs.as_ref().is_some_and(|picked| picked.is_empty()) {
                        ui.picked_docs = None;
                    }
                    settings::save_gui_settings(&session_settings(&ui.session))
                });
                if let Some(Err(e)) = saved {
                    tracing::warn!(error = %format!("{e:#}"), "설정 저장 실패");
                }
                Ok(())
            });
        }

        // 문서 고르기: 워커 스레드에서 한글의 열린 문서 목록을 받아, UI 스레드에서 고르는 창을 연다.
        {
            let wnd2 = wnd.clone();
            let btn_pick = self.btn_pick.clone();
            self.btn_pick.on().bn_clicked(move || {
                btn_pick.hwnd().EnableWindow(false);
                let bound = with_ui(|ui| ui.bound.clone()).flatten();
                let wnd = wnd2.clone();
                std::thread::spawn(move || {
                    let docs = hwp::connect()
                        .and_then(|hwp| hwp::open_documents_except(&hwp, bound.as_ref()));
                    let wnd_ui = wnd.clone();
                    wnd.run_ui_thread(move || {
                        with_ui(|ui| ui.btn_pick.hwnd().EnableWindow(true));
                        match docs {
                            Ok(docs) if docs.is_empty() => {
                                let _ = wnd_ui.hwnd().MessageBox(
                                    "한글에 열려 있는 다른 문서가 없습니다.",
                                    "문서 고르기",
                                    co::MB::OK | co::MB::ICONINFORMATION,
                                );
                            }
                            Ok(docs) => pick_documents(&wnd_ui, docs),
                            Err(e) => {
                                let _ = wnd_ui.hwnd().MessageBox(
                                    &format!("{e:#}"),
                                    "문서 고르기",
                                    co::MB::OK | co::MB::ICONERROR,
                                );
                            }
                        }
                        Ok(())
                    });
                });
                Ok(())
            });
        }

        // 설명서 창.
        {
            let wnd2 = wnd.clone();
            self.btn_manual.on().bn_clicked(move || {
                open_manual(&wnd2);
                Ok(())
            });
        }

        // 설정 창.
        {
            let wnd2 = wnd.clone();
            self.btn_settings.on().bn_clicked(move || {
                open_settings(&wnd2);
                Ok(())
            });
        }

        // 커서 위치에 삽입: 마지막 답을 원래 문서의 커서 자리에 넣는다. COM 호출이라 워커 스레드에서
        // 하고, 그동안 보내기와 삽입을 막는다.
        {
            let wnd2 = wnd.clone();
            let btn_run = self.btn_run.clone();
            let btn_insert = self.btn_insert.clone();
            self.btn_insert.on().bn_clicked(move || {
                let Some(answer) = with_ui(|ui| {
                    // 창이 고정된 대상 문서에 넣는다. 답을 받은 뒤 새 문서를 처음 저장해 이름이 생겼으면
                    // 바뀐 이름으로 찾아야 한다.
                    let bound = ui.bound.clone();
                    ui.answer.clone().map(|mut answer| {
                        if bound.is_some() {
                            answer.origin = bound;
                        }
                        answer
                    })
                })
                .flatten() else {
                    return Ok(());
                };
                btn_insert.hwnd().EnableWindow(false);
                btn_run.hwnd().EnableWindow(false);
                let wnd = wnd2.clone();
                std::thread::spawn(move || {
                    let note = insert_answer(&answer.text, answer.origin.as_ref());
                    let wnd_ui = wnd.clone();
                    wnd.run_ui_thread(move || {
                        if let Ok(btn) = wnd_ui.hwnd().GetDlgItem(ID_BTN_RUN) {
                            btn.EnableWindow(true);
                        }
                        with_ui(|ui| {
                            ui.btn_insert.hwnd().EnableWindow(ui.answer.is_some());
                        });
                        chat_note(&note);
                        chat_save();
                        Ok(())
                    });
                });
                Ok(())
            });
        }

        // Ctrl+Enter: 보내기 버튼이 켜져 있을 때(응답 대기 중이 아닐 때)만 누른다.
        {
            let btn_run = self.btn_run.clone();
            self.wnd.on().wm_command_acc_menu(ID_ACCEL_SEND, move || {
                if btn_run.hwnd().IsWindowEnabled() {
                    btn_run.trigger_click();
                }
                Ok(())
            });
        }

        self.btn_run.on().bn_clicked(move || {
            let user_prompt = txt_prompt.text().unwrap_or_default();
            // 설정 창에서 키를 입력했으면 Claude API로, 비웠으면 환경 변수 설정대로 실행한다.
            // 모델이 비어 있지 않으면 그 모델을 쓴다.
            let Some((cfg, format, include_open_docs, picked_docs, bound)) = with_ui(|ui| {
                (
                    session_cfg(ui),
                    ui.session.format,
                    ui.session.include_open_docs,
                    ui.picked_docs.clone(),
                    ui.bound.clone(),
                )
            }) else {
                return Ok(());
            };

            // UI 스레드: 입력 칸을 비우고 재진입을 막는다. 프롬프트는 문서를 읽은 뒤 그 문서의
            // 대화에 쌓는다.
            let _ = txt_prompt.set_text("");
            btn_run.hwnd().EnableWindow(false);
            // 새 답이 오기 전에는 이전 답을 넣지 않는다.
            with_ui(|ui| {
                ui.answer = None;
                ui.btn_insert.hwnd().EnableWindow(false);
            });

            // 워커 스레드로 넘길 Send 상태만 복제.
            let wnd = wnd.clone();

            tracing::info!(
                ?format,
                provider = ?cfg.llm_provider,
                model = %cfg.llm_model,
                prompt_chars = user_prompt.chars().count(),
                "보내기"
            );
            std::thread::spawn(move || {
                let started = Instant::now();
                // 창을 연 뒤에 서버가 켜진 경우 등, 서버에 없는 모델이 설정에 남아 있을 수 있다.
                let cfg = preflight_model(cfg, &wnd);
                let doc = acquire_document(
                    format,
                    include_open_docs,
                    picked_docs.as_deref(),
                    bound.as_ref(),
                );
                match &doc {
                    Ok(d) => tracing::info!(
                        ?format,
                        path = %d.path,
                        elapsed_ms = started.elapsed().as_millis(),
                        "문서 읽음"
                    ),
                    Err(e) => tracing::warn!(?format, error = %format!("{e:#}"), "문서 읽기 실패"),
                }
                // 읽은 문서와 API 종류의 대화로 바꾼 뒤 프롬프트를 쌓는다. 읽기에 실패하면 지금
                // 대화에 쌓고 오류로 마친다.
                {
                    let doc_path = doc.as_ref().ok().map(|d| d.path.clone());
                    let target = doc.as_ref().ok().and_then(|d| d.origin.clone());
                    let provider = cfg.llm_provider;
                    let user_prompt = user_prompt.clone();
                    wnd.run_ui_thread(move || {
                        if let Some(path) = doc_path {
                            chat_set_context(Some(path), provider);
                        }
                        if target.is_some() {
                            show_target_document(target.as_ref());
                        }
                        chat_begin(&user_prompt);
                        Ok(())
                    });
                }
                // 답을 넣을 원래 문서.
                let origin = doc.as_ref().ok().and_then(|d| d.origin.clone());
                // 함께 보낸 다른 열린 문서를 대화 칸에 알린다.
                let reference_notes = doc.as_ref().map(reference_notes).unwrap_or_default();
                // 설정 창에서 저장한 내용이 바로 반영되도록 요청마다 읽는다.
                let custom_system = settings::load_custom_system_prompt();
                let (mcp_servers, mcp_errors) =
                    mcp::parse_servers(&settings::load_mcp_servers_text());
                if !mcp_errors.is_empty() {
                    tracing::warn!(?mcp_errors, "MCP 설정에 잘못된 줄이 있어 건너뜀");
                }
                // 키가 실행마다 달라질 수 있어 백엔드는 실행할 때 만든다.
                let result = doc.and_then(|doc| {
                    RigBackend::new(&cfg).and_then(|backend| {
                        workflow::run_prompt(
                            &cfg,
                            &backend,
                            &doc.content,
                            format,
                            &user_prompt,
                            &custom_system,
                            &mcp_servers,
                            &doc.others,
                        )
                    })
                });
                let result = result.map(|mut answer| {
                    answer.log.splice(0..0, reference_notes);
                    answer
                });
                match &result {
                    Ok(answer) => tracing::info!(
                        elapsed_ms = started.elapsed().as_millis(),
                        answer_chars = answer.text.chars().count(),
                        tool_log = answer.log.len(),
                        "응답 받음"
                    ),
                    Err(e) => tracing::warn!(
                        elapsed_ms = started.elapsed().as_millis(),
                        error = %format!("{e:#}"),
                        "요청 실패"
                    ),
                }
                let secs = started.elapsed().as_secs();
                let header = format!(
                    "◀ AI ({}, {}, {secs}초)",
                    cfg.llm_model,
                    format.short()
                );
                let wnd_ui = wnd.clone();
                wnd.run_ui_thread(move || {
                    match &result {
                        Ok(_) => set_status(
                            Target::Main,
                            &format!(
                                "LLM 상태: ✔ 작동 중 — {} · 마지막 요청 성공({secs}초)",
                                llm::describe_target(&cfg)
                            ),
                        ),
                        // 실패하면 서버를 다시 점검해 원인을 상태 줄에 보인다.
                        Err(_) => check_server(&wnd_ui, Arc::clone(&cfg), Target::Main),
                    }
                    finish_on_ui(&wnd_ui, &header, result, origin);
                    Ok(())
                });
            });
            Ok(())
        });
    }

    /// 메시지 루프 실행.
    pub fn run(&self) -> winsafe::AnyResult<i32> {
        self.wnd.run_main(None)
    }
}

/// 환경 변수 설정에 설정 창의 키·서버 주소·모델 값을 덮어쓴 설정. 빈 값은 무시한다.
/// 서버 주소는 로컬/OpenAI 호환 서버에만 쓴다(키를 넣어 Claude API로 실행하면 쓰지 않음).
fn effective_cfg(base: &Arc<Config>, api_key: &str, base_url: &str, model: &str) -> Arc<Config> {
    let mut cfg = Config::clone(base);
    if !api_key.trim().is_empty() {
        cfg = cfg.with_claude_key(api_key.trim());
    }
    if !base_url.trim().is_empty() {
        cfg = cfg.with_base_url(base_url.trim());
    }
    if !model.trim().is_empty() {
        cfg = cfg.with_model(model.trim());
    }
    Arc::new(cfg)
}

/// 설정 파일(`settings.txt`)에 쓸 값.
fn session_settings(session: &Session) -> settings::GuiSettings {
    settings::GuiSettings {
        base_url: session.base_url.clone(),
        model: session.model.clone(),
        format: Some(session.format),
        include_open_docs: session.include_open_docs,
    }
}

/// 함께 보낸 다른 열린 문서를 대화 칸에 알릴 줄(`📎 참고 문서 …`)과 읽지 못한 문서 알림.
fn reference_notes(doc: &DocSnapshot) -> Vec<String> {
    let mut notes = doc.other_notes.clone();
    if !doc.others.is_empty() {
        let names: Vec<&str> = doc.others.iter().map(|d| d.name.as_str()).collect();
        let target = doc
            .origin
            .as_ref()
            .map(|origin| origin.name())
            .unwrap_or_else(|| "제목없음".to_string());
        notes.insert(
            0,
            format!(
                "📄 대상 문서: {target}\n📎 참고 문서 {}개: {}",
                names.len(),
                names.join(", ")
            ),
        );
    }
    notes
}

/// 적용된 설정으로 요청할 때의 설정.
fn session_cfg(ui: &Ui) -> Arc<Config> {
    effective_cfg(&ui.base, &ui.session.api_key, &ui.session.base_url, &ui.session.model)
}

/// 적용된 설정의 서버를 점검할 때의 설정. 모델은 설정의 기본 모델로 둔다([`pick_model`]이 적용된
/// 모델이 목록에 없을 때 대신 고르는 모델이다).
fn check_cfg(ui: &Ui) -> Arc<Config> {
    effective_cfg(&ui.base, &ui.session.api_key, &ui.session.base_url, "")
}

/// 메인 창의 요약 줄을 적용된 설정으로 고친다(UI 스레드).
fn update_summary(ui: &Ui) {
    let cfg = session_cfg(ui);
    let key = if ui.session.api_key.trim().is_empty() {
        ""
    } else {
        " · Claude API 키 입력함"
    };
    let text = format!(
        "모델: {} · 보낼 문서: {}{key}",
        cfg.llm_model,
        ui.session.format.short()
    );
    let _ = ui.summary.hwnd().SetWindowText(&text);
}

/// 보내기 직전(워커 스레드)에 로컬/OpenAI 호환 서버의 모델 목록을 받아, 보낼 모델이 서버에 없으면
/// 목록의 첫 모델로 바꾼다. 메인 창의 상태 줄과 적용된 모델도 그 결과로 갱신한다.
///
/// 창을 열 때 서버가 아직 꺼져 있어 점검이 실패하면 설정의 기본 모델이 그대로 남는다.
/// 그 뒤 서버가 켜졌을 때 그 모델이 서버에 없으면 "model not found"(404)로 실패하므로 여기서 막는다.
/// Claude API는 목록에 없는 별칭도 받으므로 점검하지 않는다.
fn preflight_model(cfg: Arc<Config>, wnd: &gui::WindowMain) -> Arc<Config> {
    if cfg.llm_provider != Provider::OpenAiCompatible {
        return cfg;
    }
    let status = llm::check_server(&cfg);
    let mut note = None;
    let mut chosen = Arc::clone(&cfg);
    if let ServerState::Working(models) = &status.state
        && !models.iter().any(|m| *m == cfg.llm_model)
        && let Some(first) = models.first()
    {
        tracing::warn!(missing = %cfg.llm_model, picked = %first, "보낼 모델이 서버에 없어 바꿈");
        note = Some(format!(" · 모델 {}이(가) 서버에 없어 {first}(으)로 보냄", cfg.llm_model));
        chosen = Arc::new(cfg.with_model(first));
    }
    let default_model = chosen.llm_model.clone();
    let provider = chosen.llm_provider;
    wnd.run_ui_thread(move || {
        show_status(Target::Main, status, &default_model, provider);
        if let Some(note) = note {
            append_status(&note);
        }
        Ok(())
    });
    chosen
}

/// 메인 창의 상태 줄 끝에 글을 덧붙인다(UI 스레드).
fn append_status(text: &str) {
    with_ui(|ui| {
        let current = ui.status.hwnd().GetWindowText().unwrap_or_default();
        let _ = ui.status.hwnd().SetWindowText(&format!("{current}{text}"));
    });
}

/// 워커 스레드에서 서버를 점검하고, UI 스레드에서 `target`의 상태 줄과 모델을 채운다.
fn check_server(wnd: &gui::WindowMain, cfg: Arc<Config>, target: Target) {
    set_status(target, &format!("LLM 상태: … 확인 중 — {}", llm::describe_target(&cfg)));
    // 적용된 설정의 API 종류가 바뀌었으면 그 종류의 대화로 바꾼다.
    if let Target::Main = target {
        chat_set_provider(cfg.llm_provider);
    }
    let wnd = wnd.clone();
    std::thread::spawn(move || {
        let status = llm::check_server(&cfg);
        tracing::info!(
            ?target,
            target_server = %status.target,
            state = %status.state.summary(),
            elapsed_ms = status.elapsed_ms,
            "LLM 서버 점검"
        );
        let default_model = cfg.llm_model.clone();
        let provider = cfg.llm_provider;
        wnd.run_ui_thread(move || {
            show_status(target, status, &default_model, provider);
            Ok(())
        });
    });
}

/// `target`의 상태 줄을 바꾼다(UI 스레드). 설정 창이 닫혀 있으면 설정 창 쪽은 버린다.
fn set_status(target: Target, text: &str) {
    with_ui(|ui| {
        let lbl = match target {
            Target::Main => Some(&ui.status),
            Target::Settings => ui.settings.as_ref().map(|s| &s.lbl_status),
        };
        if let Some(lbl) = lbl {
            let _ = lbl.hwnd().SetWindowText(text);
        }
    });
}

/// 점검 결과를 `target`의 상태 줄에 보이고, 성공했으면 모델 목록을 채운다(UI 스레드).
///
/// 지금 모델이 새 목록에 있으면 그대로 두고, 없으면(백엔드가 바뀐 경우 등) 설정의 기본 모델로
/// 바꾼다. 그 모델도 목록에 없으면 상태 줄에 경고를 붙인다.
fn show_status(target: Target, status: ServerStatus, default_model: &str, provider: Provider) {
    let ServerStatus {
        target: server,
        state,
        elapsed_ms,
    } = status;
    let text = match state {
        ServerState::Working(models) => {
            // 로컬/OpenAI 호환 서버의 목록은 쓸 수 있는 모델 전부다. Claude API는 목록에 없는
            // 별칭도 받으므로 목록에 없다고 다른 모델로 바꾸지 않는다.
            let pick = fill_models(
                target,
                &models,
                default_model,
                provider == Provider::OpenAiCompatible,
            );
            let note = match pick {
                ModelPick::Kept => String::new(),
                ModelPick::Replaced { missing, picked } => {
                    format!(" · 모델 {missing}이(가) 서버에 없어 {picked}을(를) 선택함")
                }
                ModelPick::Missing => " · ⚠ 선택한 모델이 목록에 없음".to_string(),
            };
            format!(
                "LLM 상태: ✔ 작동 중 — {server} (응답 {elapsed_ms}ms, 모델 {}개){note}",
                models.len()
            )
        }
        ServerState::NoKey => "LLM 상태: ✖ 키 없음 — Claude API 키를 입력하세요".to_string(),
        ServerState::Unreachable(e) => {
            format!("LLM 상태: ✖ 연결 안 됨 — {server} (서버가 꺼져 있거나 주소가 틀림): {e}")
        }
        ServerState::AuthFailed(e) => {
            format!("LLM 상태: ✖ 인증 실패 — {server} (키를 확인하세요): {e}")
        }
        ServerState::Failed(e) => format!("LLM 상태: ✖ 오류 — {server}: {e}"),
    };
    set_status(target, &text);
}

/// [`pick_model`]이 고른 결과.
enum ModelPick {
    /// 지금 모델이나 설정의 기본 모델이 목록에 있어 그대로 두었다.
    Kept,
    /// 둘 다 목록에 없어 목록의 첫 모델로 바꿨다.
    Replaced { missing: String, picked: String },
    /// 목록에 없는 모델이 남았다.
    Missing,
}

/// 받은 모델 목록으로 `target`의 모델을 고른다(UI 스레드).
///
/// 메인 창이면 적용된 모델과 모델 목록을 바꾸고 요약 줄을 고친다. 설정 창이면 그 창의 모델 칸만
/// 바꾼다.
fn fill_models(
    target: Target,
    models: &[String],
    default_model: &str,
    replace_missing: bool,
) -> ModelPick {
    with_ui(|ui| match target {
        Target::Main => {
            let (chosen, pick) =
                pick_model(models, &ui.session.model, default_model, replace_missing);
            ui.session.model = chosen;
            ui.session.models = models.to_vec();
            update_summary(ui);
            pick
        }
        Target::Settings => {
            let Some(s) = &ui.settings else {
                return ModelPick::Kept;
            };
            let current = s.cmb_model.hwnd().GetWindowText().unwrap_or_default();
            let (chosen, pick) = pick_model(models, &current, default_model, replace_missing);
            fill_combo(&s.cmb_model, models, &chosen);
            pick
        }
    })
    .unwrap_or(ModelPick::Kept)
}

/// 쓸 모델을 고른다: 지금 모델 → 설정의 기본 모델 순으로 목록에 있는 것. 둘 다 없고
/// `replace_missing`이면 목록의 첫 모델. 없는 모델이 남아 있으면 요청이 "model not found"로 실패하기
/// 때문이다.
fn pick_model(
    models: &[String],
    current: &str,
    default_model: &str,
    replace_missing: bool,
) -> (String, ModelPick) {
    let current = current.trim();
    let wanted = if models.iter().any(|m| m == current) {
        current
    } else {
        default_model
    };
    if models.iter().any(|m| m == wanted) {
        return (wanted.to_string(), ModelPick::Kept);
    }
    match models.first() {
        Some(first) if replace_missing => (
            first.clone(),
            ModelPick::Replaced {
                missing: wanted.to_string(),
                picked: first.clone(),
            },
        ),
        _ => (wanted.to_string(), ModelPick::Missing),
    }
}

/// 모델 칸 목록을 `models`로 바꾸고 `chosen`을 칸에 둔다.
fn fill_combo(cmb: &gui::ComboBox, models: &[String], chosen: &str) {
    cmb.items().delete_all();
    let _ = cmb.items().add(models);
    match models.iter().position(|m| m == chosen) {
        Some(i) => cmb.items().select(Some(i as u32)),
        None => {
            let _ = cmb.hwnd().SetWindowText(chosen);
        }
    }
}

/// 받은 답을 원래 문서(`origin`, 요청 때 읽은 문서)의 커서 자리에 넣는다(워커 스레드). 대화 칸에
/// 적을 결과 문구를 돌려준다.
///
/// 원래 문서에만 넣는다. 다른 탭으로 갔으면 원래 문서를 활성화해 넣고, 찾지 못하면 넣지 않는다
/// ([`hwp::insert_into`]). 그 문서 안에서 커서를 옮겼으면 옮긴 자리에 넣는다(호출된 위치로 돌아가지
/// 않음). 문서 내용이 바뀌었는지 확인하는 일은 아직 하지 않는다. `EAT_HWP_DOC`로 시험할 때는 넣지
/// 않는다.
fn insert_answer(answer: &str, origin: Option<&hwp::DocIdentity>) -> String {
    let Some(origin) = origin else {
        return "한글에서 읽은 문서가 아니라 문서에 넣지 않음".to_string();
    };
    let text = answer.trim();
    if text.is_empty() {
        return "빈 답이라 문서에 넣지 않음".to_string();
    }
    match hwp::connect().and_then(|hwp| hwp::insert_into(&hwp, origin, text)) {
        Ok(switched) => {
            tracing::info!(
                chars = text.chars().count(),
                doc_id = origin.id,
                doc = %origin.full_name,
                switched,
                "원래 문서 커서 자리에 넣음"
            );
            if switched {
                "원래 문서로 돌아가 커서 자리에 넣음".to_string()
            } else {
                "문서의 커서 자리에 넣음".to_string()
            }
        }
        Err(e) => {
            tracing::warn!(error = %format!("{e:#}"), "문서에 넣지 못함");
            format!("⚠ 문서에 넣지 못함: {e:#}")
        }
    }
}

/// 워커 결과를 UI 스레드에서 반영한다(버튼 재활성화, 대기 줄을 답변이나 오류로 바꿈). 답을 받았으면
/// "커서 위치에 삽입"으로 넣을 수 있게 둔다.
fn finish_on_ui(
    wnd: &gui::WindowMain,
    header: &str,
    result: Result<Completion>,
    origin: Option<hwp::DocIdentity>,
) {
    if let Ok(btn) = wnd.hwnd().GetDlgItem(ID_BTN_RUN) {
        btn.EnableWindow(true);
    }
    if let Ok(answer) = &result {
        with_ui(|ui| {
            ui.answer = Some(Answer {
                text: answer.text.clone(),
                origin,
            });
            ui.btn_insert.hwnd().EnableWindow(true);
        });
    }
    match result {
        // MCP 도구 기록은 답 앞에 보인다. 문서에는 답만 넣는다.
        Ok(answer) if answer.log.is_empty() => chat_finish(header, &answer.text),
        Ok(answer) => chat_finish(
            header,
            &format!("{}\n\n{}", answer.log.join("\n"), answer.text),
        ),
        Err(e) => chat_finish("◀ 오류", &format!("{e:#}")),
    }
    chat_save();
}

/// 한글이 보낸 이벤트를 받으려고 워커 스레드의 메시지를 처리하는 간격.
const PUMP_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

/// 이벤트와 별도로 대상 문서가 아직 열려 있는지 훑어 보는 간격(이벤트 없이 한글이 사라진 경우 대비).
const WATCH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);

/// 창을 대상 문서에 고정한다. 워커 스레드에서 한글에 붙어 대상 문서를 정하고(애드온이 넘긴 문서, 없으면
/// 지금 활성 문서), UI 스레드에서 창 제목과 그 문서의 대화를 맞춘다. 그 뒤 같은 스레드에서 대상 문서를
/// 지켜보다가 닫히면 창을 닫는다([`watch_document`]).
///
/// `EAT_HWP_DOC`로 시험할 때는 그 파일의 대화만 불러온다. 한글에 붙지 못하고 넘겨받은 문서도 없으면
/// 고정하지 않는다(요청 때 활성 문서를 읽는다).
fn bind_document(wnd: &gui::WindowMain) {
    let wnd = wnd.clone();
    std::thread::spawn(move || {
        if let Ok(path) = std::env::var("EAT_HWP_DOC")
            && !path.is_empty()
        {
            wnd.run_ui_thread(move || {
                let provider = CHAT.with_borrow(|chat| chat.as_ref().map(|c| c.provider));
                if let Some(provider) = provider {
                    chat_set_context(Some(path), provider);
                }
                Ok(())
            });
            return;
        }
        let hwp = hwp::connect().ok();
        let active = hwp.as_ref().and_then(|hwp| hwp::active_document(hwp).ok());
        let Some(bound) = crate::app::target_from_env().or_else(|| active.clone()) else {
            tracing::warn!("대상 문서를 알 수 없어 창을 문서에 고정하지 않음");
            return;
        };
        // 대화 기록은 문서 경로로 나눈다. 대상 문서가 활성 문서면 한글이 주는 경로를 쓴다.
        let path = match &hwp {
            Some(hwp) if active.as_ref() == Some(&bound) => {
                hwp.path().unwrap_or_else(|_| bound.full_name.clone())
            }
            _ => bound.full_name.clone(),
        };
        tracing::info!(doc_id = bound.id, doc = %bound.full_name, "창을 대상 문서에 고정");
        {
            let bound = bound.clone();
            wnd.run_ui_thread(move || {
                let provider = CHAT.with_borrow(|chat| chat.as_ref().map(|c| c.provider));
                if let Some(provider) = provider {
                    chat_set_context(Some(path), provider);
                }
                show_target_document(Some(&bound));
                with_ui(|ui| ui.bound = Some(bound));
                Ok(())
            });
        }
        watch_document(&wnd, hwp, bound);
    });
}

/// 저장하지 않은 새 문서가 처음 저장돼 이름이 생겼을 때 창을 그 이름으로 다시 고정한다(UI 스레드).
/// 창 제목과 대상 문서를 바꾸고, 대화 칸의 글은 그대로 둔 채 기록 파일만 새 이름의 것으로 옮겨 쓴다.
fn rebind_document(saved: hwp::DocIdentity) {
    CHAT.with_borrow_mut(|chat| {
        if let Some(chat) = chat {
            chat.file = chat_store::chat_file(&saved.full_name, chat.provider);
            chat.doc_path = Some(saved.full_name.clone());
        }
    });
    chat_save();
    show_target_document(Some(&saved));
    with_ui(|ui| ui.bound = Some(saved));
}

/// 한글이 보낸 이벤트 가운데 대상 문서에 관한 것. 이벤트 클로저가 적어 두고 [`watch_document`]가 읽는다.
#[derive(Default)]
struct DocSignals {
    /// 대상 문서가 닫혔거나 한글이 끝났다.
    closed: Cell<bool>,
    /// 대상 문서가 저장됐다. 이름이 바뀌었는지 확인해야 한다.
    saved: Cell<bool>,
}

/// 한글의 이벤트를 받아 대상 문서(`doc_id`)에 관한 것을 `signals`에 적는다
/// (hwp_automation_rs의 `ExtEvents::subscribe`). 돌려받은 연결이 살아 있는 동안 받는다.
///
/// 이벤트는 이 함수를 부른 스레드가 메시지를 처리할 때 전달된다([`watch_document`]가 처리한다).
/// - `DocumentAfterClose`(대상 문서), `Quit`: 닫힘
/// - `DocumentAfterSave`(대상 문서): 저장됨. 다른 이름으로 저장했는지는 [`watch_document`]가 확인한다
fn subscribe_document(
    hwp: &hwp_com::HwpObject,
    doc_id: i32,
    signals: &Rc<DocSignals>,
) -> Option<EventConnection> {
    let signals = Rc::clone(signals);
    let connection = hwp.ext_events().subscribe(move |event| match event {
        HwpEvent::Quit => signals.closed.set(true),
        HwpEvent::DocumentAfterClose { doc_id: id } if id == doc_id => signals.closed.set(true),
        HwpEvent::DocumentAfterSave { doc_id: id } if id == doc_id => signals.saved.set(true),
        _ => {}
    });
    match connection {
        Ok(connection) => Some(connection),
        Err(e) => {
            tracing::warn!(error = %e, "한글 이벤트에 연결하지 못함. 주기적 확인만 한다");
            None
        }
    }
}

/// 대상 문서(`bound`)가 한글에서 닫히면 창을 닫는다(워커 스레드, 창이 닫힐 때까지 돈다).
///
/// 한글의 이벤트로 안다([`subscribe_document`]). 이 스레드의 메시지를 [`PUMP_INTERVAL`]마다 처리해
/// 이벤트를 받는다.
/// - 대상 문서를 닫았거나 한글을 끝냈으면 바로 창을 닫는다
/// - 대상 문서를 저장했으면 이름을 확인한다. 다른 이름으로 저장해 경로가 바뀌었으면 창을 닫고,
///   저장하지 않은 새 문서를 처음 저장해 이름이 생긴 것이면 창을 그 이름으로 다시 고정한다
///   ([`rebind_document`])
///
/// 이벤트 없이 한글이 사라지는 경우(강제 종료 등)와 이벤트에 연결하지 못한 경우에 대비해
/// [`WATCH_INTERVAL`]마다 열린 문서를 훑어 본다. 두 번 이어서 대상 문서가 없거나 한글에 붙지 못하면
/// 창을 닫는다. 한글이 바빠(대화 상자 등) 훑지 못한 것은 닫힌 것으로 치지 않고 다시 붙어 본다.
fn watch_document(
    wnd: &gui::WindowMain,
    mut hwp: Option<hwp_com::HwpObject>,
    mut bound: hwp::DocIdentity,
) {
    let signals = Rc::new(DocSignals::default());
    let mut connection = hwp
        .as_ref()
        .and_then(|hwp| subscribe_document(hwp, bound.id, &signals));
    let mut gone = 0;
    let mut checked = Instant::now();
    loop {
        std::thread::sleep(PUMP_INTERVAL);
        // 쌓인 메시지를 처리한다. 이벤트 클로저는 이 안에서 불린다.
        hwp_com::pump_pending_messages();
        let saved = signals.saved.replace(false);
        let mut closed = signals.closed.get();
        if !closed && (saved || checked.elapsed() >= WATCH_INTERVAL) {
            checked = Instant::now();
            let mut open = match &hwp {
                Some(hwp) => hwp::document_open(hwp, &bound),
                None => Err(anyhow!("한글에 붙어 있지 않음")),
            };
            if let (Ok(false), Some(hwp), true) = (&open, &hwp, bound.full_name.is_empty())
                && let Ok(Some(named)) = hwp::document_by_id(hwp, bound.id)
                && !named.full_name.is_empty()
            {
                tracing::info!(doc_id = named.id, doc = %named.full_name, "새 문서가 저장돼 창을 그 이름에 다시 고정");
                bound = named.clone();
                wnd.run_ui_thread(move || {
                    rebind_document(named);
                    Ok(())
                });
                open = Ok(true);
            }
            match open {
                Ok(true) => gone = 0,
                // 저장 이벤트 뒤에 이름이 달라졌으면 다른 이름으로 저장한 것이다. 바로 닫는다.
                Ok(false) if saved => closed = true,
                Ok(false) => gone += 1,
                Err(_) => match hwp::connect() {
                    // 다시 붙었다. 이벤트도 다시 연결하고, 다음 차례에 훑어 본다.
                    Ok(again) => {
                        connection = subscribe_document(&again, bound.id, &signals);
                        hwp = Some(again);
                    }
                    Err(_) => {
                        connection = None;
                        hwp = None;
                        gone += 1;
                    }
                },
            }
        }
        if closed || gone >= 2 {
            tracing::info!(
                doc_id = bound.id,
                doc = %bound.full_name,
                by_event = closed,
                "대상 문서가 닫혀 창을 닫음"
            );
            // 한글이 끝난 뒤에는 연결을 끊는 호출이 실패할 수 있으나 무시된다.
            drop(connection);
            let wnd_ui = wnd.clone();
            wnd.run_ui_thread(move || {
                wnd_ui.close();
                Ok(())
            });
            return;
        }
    }
}

/// API 종류만 바꾼다(UI 스레드).
fn chat_set_provider(provider: Provider) {
    let doc_path = CHAT.with_borrow(|chat| chat.as_ref().and_then(|c| c.doc_path.clone()));
    chat_set_context(doc_path, provider);
}

/// 대화 칸의 문서와 API 종류를 정하고, 기록 파일이 바뀌면 그 파일의 대화를 불러온다(UI 스레드).
///
/// 응답을 기다리는 중에는 바꾸지 않는다. 대기 줄이 다른 대화로 넘어가기 때문이다.
fn chat_set_context(doc_path: Option<String>, provider: Provider) {
    CHAT.with_borrow_mut(|chat| {
        let Some(chat) = chat else {
            return;
        };
        if chat.pending_start.is_some() {
            return;
        }
        chat.doc_path = doc_path;
        chat.provider = provider;
        let file = chat
            .doc_path
            .as_deref()
            .and_then(|path| chat_store::chat_file(path, provider));
        if file == chat.file {
            return;
        }
        tracing::debug!(file = ?file, "대화 기록 전환");
        chat.file = file;
        let text = chat
            .file
            .as_deref()
            .map(chat_store::load)
            .unwrap_or_default();
        let _ = chat.log.set_text(&to_crlf(&text));
        // 끝으로 스크롤한다.
        chat_append(&chat.log, "");
    });
}

/// 메인 창의 제목에 대상 문서(답을 넣을 원래 문서)를 보인다(UI 스레드). 함께 보내는 참고 문서와 가려 볼
/// 수 있게 하고, 애드온이 이 문서의 GUI 창을 제목으로 찾을 수 있게 한다([`crate::app::window_title`]).
fn show_target_document(doc: Option<&hwp::DocIdentity>) {
    let title = crate::app::window_title(doc);
    with_ui(|ui| {
        if let Ok(main) = ui.status.hwnd().GetParent() {
            let _ = main.SetWindowText(&title);
        }
    });
}

/// 대화 칸의 글을 기록 파일에 쓴다(UI 스레드). 실패하면 상태 줄에 알린다.
fn chat_save() {
    let saved = CHAT.with_borrow(|chat| {
        let chat = chat.as_ref()?;
        let file = chat.file.as_deref()?;
        let text = chat.log.hwnd().GetWindowText().unwrap_or_default();
        Some(chat_store::save(file, &text))
    });
    if let Some(Err(e)) = saved {
        tracing::warn!(error = %format!("{e:#}"), "대화 기록 저장 실패");
        set_status(Target::Main, &format!("대화 기록 저장 실패: {e:#}"));
    }
}

/// 대화 칸에 보낸 프롬프트와 응답 대기 줄을 덧붙인다(UI 스레드).
fn chat_begin(prompt: &str) {
    CHAT.with_borrow_mut(|chat| {
        let Some(chat) = chat else {
            return;
        };
        let mut text = String::new();
        if chat_len(&chat.log) > 0 {
            text.push_str(TURN_SEPARATOR);
            text.push_str("\r\n");
        }
        let prompt = prompt.trim_end();
        let prompt = if prompt.trim().is_empty() {
            "(프롬프트 없음)"
        } else {
            prompt
        };
        text.push_str(&format!("▶ 나 · {}\r\n{}\r\n\r\n", now_text(), to_crlf(prompt)));
        chat_append(&chat.log, &text);
        chat.pending_start = Some(chat_len(&chat.log));
        chat_append(&chat.log, "◀ AI — 응답을 기다리는 중…\r\n");
    });
}

/// 응답 대기 줄을 머리줄과 본문으로 바꾼다(UI 스레드).
fn chat_finish(header: &str, body: &str) {
    CHAT.with_borrow_mut(|chat| {
        let Some(chat) = chat else {
            return;
        };
        let end = chat_len(&chat.log);
        let start = chat.pending_start.take().unwrap_or(end);
        chat.log.set_selection(start, end);
        chat.log
            .replace_selection(&format!(
                "{header} · {}\r\n{}\r\n\r\n",
                now_text(),
                to_crlf(body.trim_end())
            ));
    });
}

/// 대화 칸 끝에 알림 줄(`↳ 알림 · 시각`)을 덧붙인다(UI 스레드).
fn chat_note(note: &str) {
    CHAT.with_borrow(|chat| {
        if let Some(chat) = chat {
            chat_append(&chat.log, &format!("↳ {note} · {}\r\n\r\n", now_text()));
        }
    });
}

/// 지금 시스템 로컬 시간. 예: `2026-11-09 14:03:21`
fn now_text() -> String {
    let t = winsafe::GetLocalTime();
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
    )
}

/// 대화 칸 끝에 글을 덧붙인다. 바뀐 자리로 스크롤된다.
fn chat_append(log: &gui::Edit, text: &str) {
    let end = chat_len(log);
    log.set_selection(end, end);
    log.replace_selection(text);
}

/// 대화 칸의 글자 수(UTF-16 단위). Edit의 선택 위치와 같은 단위다.
fn chat_len(log: &gui::Edit) -> i32 {
    log.hwnd().GetWindowTextLength().unwrap_or(0)
}

/// 여러 줄 Edit는 줄바꿈으로 `\r\n`만 인식한다.
fn to_crlf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\n', "\r\n")
}

/// "문서 고르기" 창에 보일 문서의 최대 개수.
const MAX_PICK_DOCS: usize = 20;

/// 문서 고르기 창(모달). 한글에 열려 있는 다른 문서(`docs`)마다 체크 칸을 두고, 참고 문서로 함께 보낼
/// 문서를 고른다.
///
/// 처음에는 지난번에 고른 문서가(고른 적이 없으면 모두) 체크돼 있다. "확인"을 누르면 고른 문서를
/// 기억하고, 하나라도 골랐으면 "다른 열린 문서도 함께 보내기"를 켜고 하나도 없으면 끈다.
fn pick_documents(parent: &gui::WindowMain, docs: Vec<hwp::DocIdentity>) {
    let picked = with_ui(|ui| ui.picked_docs.clone()).flatten();
    let shown = docs.len().min(MAX_PICK_DOCS);
    let rows = shown as i32;
    let dlg = gui::WindowModal::new(gui::WindowModalOpts {
        title: "참고 문서 고르기",
        size: gui::dpi(560, 86 + rows * 24),
        ..Default::default()
    });
    let hint = if docs.len() > shown {
        format!("함께 보낼 문서를 고르세요. (열린 문서 {}개 중 {shown}개만 보임)", docs.len())
    } else {
        "함께 보낼 문서를 고르세요. 글은 원래 문서에만 넣습니다.".to_string()
    };
    let _lbl = gui::Label::new(
        &dlg,
        gui::LabelOpts {
            text: &hint,
            position: gui::dpi(10, 10),
            ..Default::default()
        },
    );
    let checks: Vec<(gui::CheckBox, hwp::DocIdentity)> = docs
        .into_iter()
        .take(shown)
        .enumerate()
        .map(|(i, doc)| {
            // 같은 이름의 문서를 가릴 수 있게 저장된 문서는 전체 경로를 보인다.
            let text = if doc.full_name.is_empty() {
                doc.name()
            } else {
                doc.full_name.clone()
            };
            let check = gui::CheckBox::new(
                &dlg,
                gui::CheckBoxOpts {
                    text: &text,
                    position: gui::dpi(10, 36 + i as i32 * 24),
                    size: gui::dpi(540, 20),
                    control_style: co::BS::AUTOCHECKBOX,
                    ..Default::default()
                },
            );
            (check, doc)
        })
        .collect();
    let btn_ok = gui::Button::new(
        &dlg,
        gui::ButtonOpts {
            text: "확인",
            position: gui::dpi(370, 50 + rows * 24),
            width: gui::dpi_x(85),
            ..Default::default()
        },
    );
    let btn_cancel = gui::Button::new(
        &dlg,
        gui::ButtonOpts {
            text: "취소",
            position: gui::dpi(465, 50 + rows * 24),
            width: gui::dpi_x(85),
            ..Default::default()
        },
    );
    {
        let checks = checks.clone();
        dlg.on().wm_create(move |_| {
            for (check, doc) in &checks {
                check.set_check(picked.as_ref().is_none_or(|picked| picked.contains(doc)));
            }
            Ok(0)
        });
    }
    {
        let dlg2 = dlg.clone();
        btn_ok.on().bn_clicked(move || {
            let chosen: Vec<hwp::DocIdentity> = checks
                .iter()
                .filter(|(check, _)| check.is_checked())
                .map(|(_, doc)| doc.clone())
                .collect();
            let saved = with_ui(|ui| {
                let include = !chosen.is_empty();
                tracing::info!(picked = chosen.len(), "참고 문서 고름");
                ui.picked_docs = Some(chosen);
                ui.session.include_open_docs = include;
                ui.chk_others.set_check(include);
                settings::save_gui_settings(&session_settings(&ui.session))
            });
            if let Some(Err(e)) = saved {
                tracing::warn!(error = %format!("{e:#}"), "설정 저장 실패");
            }
            dlg2.close();
            Ok(())
        });
    }
    {
        let dlg2 = dlg.clone();
        btn_cancel.on().bn_clicked(move || {
            dlg2.close();
            Ok(())
        });
    }
    if let Err(e) = dlg.show_modal(parent) {
        tracing::warn!(error = %e, "문서 고르기 창을 열지 못함");
    }
}

/// 설명서 창(모달). 사용자 설명서([`MANUAL`])를 읽기 전용 칸에 보인다.
fn open_manual(parent: &gui::WindowMain) {
    let dlg = gui::WindowModal::new(gui::WindowModalOpts {
        title: "eat_hwp_raw 설명서",
        size: gui::dpi(640, 560),
        ..Default::default()
    });
    let txt = gui::Edit::new(
        &dlg,
        gui::EditOpts {
            position: gui::dpi(10, 10),
            width: gui::dpi_x(620),
            height: gui::dpi_y(504),
            control_style: co::ES::MULTILINE | co::ES::AUTOVSCROLL | co::ES::READONLY,
            window_style: co::WS::CHILD
                | co::WS::VISIBLE
                | co::WS::TABSTOP
                | co::WS::GROUP
                | co::WS::VSCROLL,
            ..Default::default()
        },
    );
    let btn_close = gui::Button::new(
        &dlg,
        gui::ButtonOpts {
            text: "닫기",
            position: gui::dpi(545, 526),
            width: gui::dpi_x(85),
            ..Default::default()
        },
    );
    {
        let txt = txt.clone();
        dlg.on().wm_create(move |_| {
            txt.limit_text(None);
            let _ = txt.set_text(&to_crlf(MANUAL));
            Ok(0)
        });
    }
    {
        let dlg2 = dlg.clone();
        btn_close.on().bn_clicked(move || {
            dlg2.close();
            Ok(())
        });
    }
    if let Err(e) = dlg.show_modal(parent) {
        tracing::warn!(error = %e, "설명서 창을 열지 못함");
    }
}

/// 설정 창의 고정 프롬프트 제목.
fn fixed_prompt_title(format: DocFormat, has_tools: bool) -> String {
    format!(
        "고정 프롬프트 — 코드가 정한 규약, 고칠 수 없음 (형식: {}, 커서 표시{} 있을 때)",
        format.short(),
        if has_tools { "·MCP 도구가" } else { "가" }
    )
}

/// 설정 창의 고정 프롬프트 칸과 제목을 `format`과 MCP 칸의 서버 유무에 맞게 다시 쓴다.
fn show_fixed_prompt(lbl: &gui::Label, txt: &gui::Edit, format: DocFormat, mcp_text: &str) {
    let has_tools = !mcp::parse_servers(mcp_text).0.is_empty();
    // 메인 창에서 "다른 열린 문서도 함께 보내기"를 켰으면 참고 문서 규칙도 보인다.
    let has_references = with_ui(|ui| ui.session.include_open_docs).unwrap_or(false);
    let _ = lbl.hwnd().SetWindowText(&fixed_prompt_title(format, has_tools));
    let _ = txt.set_text(&to_crlf(&workflow::system_preview(format, has_tools, has_references)));
}

/// 문서 형식 칸에서 고른 형식. 고른 것이 없으면 기본 형식.
fn selected_format(cmb: &gui::ComboBox) -> DocFormat {
    cmb.items()
        .selected_index()
        .and_then(|i| DocFormat::ALL.get(i as usize).copied())
        .unwrap_or(DocFormat::DEFAULT)
}

/// MCP 서버 칸이 비었을 때 보일 설명(주석 줄이라 서버로 읽지 않는다).
const MCP_TEMPLATE: &str = "# 한 줄에 서버 하나: 이름 = 주소 또는 명령. #으로 시작하는 줄은 쓰지 않는다.\n\
# 네트워크 서버(Streamable HTTP):  docs = http://192.168.0.10:8000/mcp\n\
# 로컬 프로그램(stdio):            hwp2md = \"C:\\tools\\hwp2md-mcp.exe\" --root C:\\참고문서\n";

/// 설정 창(모달). 왼쪽은 LLM 연결(키·서버 주소·모델·상태 확인)·보낼 문서 형식·MCP 서버, 오른쪽은
/// 시스템 프롬프트(고정 규약 보기, 사용자 지정 지침)다.
///
/// "확인"을 누르면 사용자 지정 지침·서버 주소·모델·형식·MCP 서버를 파일에 쓰고 적용한다. 키는 저장하지
/// 않고 이번 실행에만 쓴다. 키·주소·모델이 바뀌었으면 메인 창의 상태를 다시 점검한다. MCP 칸에 잘못된
/// 줄이 있으면 알리고 닫지 않는다. "취소"는 바꾼 값을 버린다.
fn open_settings(parent: &gui::WindowMain) {
    let Some((base, session, main_status)) = with_ui(|ui| {
        (
            Arc::clone(&ui.base),
            ui.session.clone(),
            ui.status.hwnd().GetWindowText().unwrap_or_default(),
        )
    }) else {
        return;
    };

    let dlg = gui::WindowModal::new(gui::WindowModalOpts {
        title: "설정",
        size: gui::dpi(1040, 604),
        ..Default::default()
    });

    // ── 왼쪽: LLM 연결 ──
    let _lbl_key = gui::Label::new(
        &dlg,
        gui::LabelOpts {
            text: "Claude API 키 (비우면 환경 변수 설정 사용, 저장하지 않음)",
            position: gui::dpi(10, 10),
            ..Default::default()
        },
    );
    let txt_key = gui::Edit::new(
        &dlg,
        gui::EditOpts {
            position: gui::dpi(10, 28),
            width: gui::dpi_x(500),
            control_style: co::ES::AUTOHSCROLL | co::ES::PASSWORD,
            ..Default::default()
        },
    );
    let env_url = if base.llm_provider == Provider::OpenAiCompatible {
        base.llm_base_url.clone()
    } else {
        "환경 변수 설정은 Claude API".to_string()
    };
    let _lbl_url = gui::Label::new(
        &dlg,
        gui::LabelOpts {
            text: &format!(
                "로컬/OpenAI 호환 서버 주소 (비우면 {env_url}, Claude API에는 쓰지 않음)"
            ),
            position: gui::dpi(10, 58),
            size: gui::dpi(500, 16),
            ..Default::default()
        },
    );
    let txt_url = gui::Edit::new(
        &dlg,
        gui::EditOpts {
            position: gui::dpi(10, 76),
            width: gui::dpi_x(500),
            control_style: co::ES::AUTOHSCROLL,
            ..Default::default()
        },
    );
    let _lbl_model = gui::Label::new(
        &dlg,
        gui::LabelOpts {
            text: "모델 (목록에서 고르거나 직접 입력, 비우면 환경 변수 설정 사용)",
            position: gui::dpi(10, 106),
            ..Default::default()
        },
    );
    let cmb_model = gui::ComboBox::new(
        &dlg,
        gui::ComboBoxOpts {
            position: gui::dpi(10, 124),
            width: gui::dpi_x(380),
            // 목록에 없는 모델 이름도 직접 입력할 수 있게 편집형으로 둔다.
            control_style: co::CBS::DROPDOWN,
            ..Default::default()
        },
    );
    let btn_check = gui::Button::new(
        &dlg,
        gui::ButtonOpts {
            text: "상태 확인",
            position: gui::dpi(400, 123),
            width: gui::dpi_x(110),
            ..Default::default()
        },
    );
    let lbl_status = gui::Label::new(
        &dlg,
        gui::LabelOpts {
            text: "",
            position: gui::dpi(10, 152),
            // 상태 문구가 바뀌어 길어지므로 폭을 고정한다.
            size: gui::dpi(500, 16),
            ..Default::default()
        },
    );
    let _lbl_format = gui::Label::new(
        &dlg,
        gui::LabelOpts {
            text: "LLM에 보낼 문서",
            position: gui::dpi(10, 180),
            ..Default::default()
        },
    );
    let format_labels: Vec<&str> = DocFormat::ALL.iter().map(|f| f.label()).collect();
    let cmb_format = gui::ComboBox::new(
        &dlg,
        gui::ComboBoxOpts {
            position: gui::dpi(120, 176),
            width: gui::dpi_x(260),
            items: &format_labels,
            selected_item: DocFormat::ALL
                .iter()
                .position(|f| *f == session.format)
                .map(|i| i as u32),
            ..Default::default()
        },
    );

    // ── 왼쪽: MCP 서버 ──
    let _lbl_mcp = gui::Label::new(
        &dlg,
        gui::LabelOpts {
            text: "MCP 서버 — 한 줄에 하나: 이름 = http(s)://주소 또는 실행 명령 (#은 꺼짐)",
            position: gui::dpi(10, 212),
            size: gui::dpi(500, 16),
            ..Default::default()
        },
    );
    let txt_mcp = gui::Edit::new(
        &dlg,
        gui::EditOpts {
            position: gui::dpi(10, 230),
            width: gui::dpi_x(500),
            height: gui::dpi_y(272),
            control_style: co::ES::MULTILINE
                | co::ES::WANTRETURN
                | co::ES::AUTOVSCROLL
                | co::ES::AUTOHSCROLL
                | co::ES::NOHIDESEL,
            window_style: co::WS::CHILD
                | co::WS::VISIBLE
                | co::WS::TABSTOP
                | co::WS::GROUP
                | co::WS::VSCROLL
                | co::WS::HSCROLL,
            ..Default::default()
        },
    );
    let btn_mcp_check = gui::Button::new(
        &dlg,
        gui::ButtonOpts {
            text: "MCP 확인",
            position: gui::dpi(10, 512),
            width: gui::dpi_x(110),
            ..Default::default()
        },
    );
    let lbl_mcp_status = gui::Label::new(
        &dlg,
        gui::LabelOpts {
            text: "MCP: 확인 전 (요청할 때마다 붙고, 끝나면 닫는다)",
            position: gui::dpi(130, 510),
            // 서버마다 결과를 이어 적으므로 두 줄 높이로 둔다.
            size: gui::dpi(380, 48),
            ..Default::default()
        },
    );

    // ── 오른쪽: 시스템 프롬프트 ──
    let lbl_fixed = gui::Label::new(
        &dlg,
        gui::LabelOpts {
            text: "",
            position: gui::dpi(530, 10),
            size: gui::dpi(500, 16),
            ..Default::default()
        },
    );
    let txt_fixed = gui::Edit::new(
        &dlg,
        gui::EditOpts {
            position: gui::dpi(530, 28),
            width: gui::dpi_x(500),
            height: gui::dpi_y(250),
            control_style: co::ES::MULTILINE | co::ES::AUTOVSCROLL | co::ES::READONLY,
            window_style: co::WS::CHILD
                | co::WS::VISIBLE
                | co::WS::TABSTOP
                | co::WS::GROUP
                | co::WS::VSCROLL,
            ..Default::default()
        },
    );
    let _lbl_custom = gui::Label::new(
        &dlg,
        gui::LabelOpts {
            text: "사용자 지정 지침 — 위 규약 안의 표시된 자리에 들어갑니다. 비우면 붙이지 않습니다.",
            position: gui::dpi(530, 290),
            ..Default::default()
        },
    );
    let txt_custom = gui::Edit::new(
        &dlg,
        gui::EditOpts {
            position: gui::dpi(530, 308),
            width: gui::dpi_x(500),
            height: gui::dpi_y(250),
            control_style: co::ES::MULTILINE
                | co::ES::WANTRETURN
                | co::ES::AUTOVSCROLL
                | co::ES::NOHIDESEL,
            window_style: co::WS::CHILD
                | co::WS::VISIBLE
                | co::WS::TABSTOP
                | co::WS::GROUP
                | co::WS::VSCROLL,
            ..Default::default()
        },
    );
    let btn_ok = gui::Button::new(
        &dlg,
        gui::ButtonOpts {
            text: "확인",
            position: gui::dpi(850, 570),
            width: gui::dpi_x(85),
            ..Default::default()
        },
    );
    let btn_cancel = gui::Button::new(
        &dlg,
        gui::ButtonOpts {
            text: "취소",
            position: gui::dpi(945, 570),
            width: gui::dpi_x(85),
            ..Default::default()
        },
    );

    // 창이 만들어지면 적용된 값을 채우고, 점검 결과를 받을 수 있게 컨트롤을 맡긴다.
    {
        let txt_key = txt_key.clone();
        let txt_url = txt_url.clone();
        let cmb_model = cmb_model.clone();
        let lbl_status = lbl_status.clone();
        let txt_mcp = txt_mcp.clone();
        let lbl_mcp_status = lbl_mcp_status.clone();
        let btn_mcp_check = btn_mcp_check.clone();
        let lbl_fixed = lbl_fixed.clone();
        let txt_fixed = txt_fixed.clone();
        let txt_custom = txt_custom.clone();
        let session = session.clone();
        dlg.on().wm_create(move |_| {
            let _ = txt_key.set_text(&session.api_key);
            let _ = txt_url.set_text(&session.base_url);
            fill_combo(&cmb_model, &session.models, &session.model);
            let _ = lbl_status.hwnd().SetWindowText(&main_status);
            txt_mcp.limit_text(None);
            let mcp_text = settings::load_mcp_servers_text();
            let mcp_text = if mcp_text.trim().is_empty() {
                MCP_TEMPLATE.to_string()
            } else {
                mcp_text
            };
            let _ = txt_mcp.set_text(&to_crlf(&mcp_text));
            show_fixed_prompt(&lbl_fixed, &txt_fixed, session.format, &mcp_text);
            txt_custom.limit_text(None);
            let _ = txt_custom.set_text(&to_crlf(&settings::load_custom_system_prompt()));
            with_ui(|ui| {
                ui.settings = Some(SettingsUi {
                    cmb_model: cmb_model.clone(),
                    lbl_status: lbl_status.clone(),
                    lbl_mcp_status: lbl_mcp_status.clone(),
                    btn_mcp_check: btn_mcp_check.clone(),
                })
            });
            Ok(0)
        });
    }

    // 형식을 바꾸거나 MCP 칸을 고치면 고정 프롬프트도 그에 맞게 다시 보인다(MCP 서버가 있으면 도구
    // 규칙이 들어간다).
    {
        let cmb_format2 = cmb_format.clone();
        let txt_mcp = txt_mcp.clone();
        let lbl_fixed = lbl_fixed.clone();
        let txt_fixed = txt_fixed.clone();
        cmb_format.on().cbn_sel_change(move || {
            let mcp_text = txt_mcp.text().unwrap_or_default();
            show_fixed_prompt(&lbl_fixed, &txt_fixed, selected_format(&cmb_format2), &mcp_text);
            Ok(())
        });
    }
    {
        let cmb_format = cmb_format.clone();
        let txt_mcp2 = txt_mcp.clone();
        let lbl_fixed = lbl_fixed.clone();
        let txt_fixed = txt_fixed.clone();
        txt_mcp.on().en_kill_focus(move || {
            let mcp_text = txt_mcp2.text().unwrap_or_default();
            show_fixed_prompt(&lbl_fixed, &txt_fixed, selected_format(&cmb_format), &mcp_text);
            Ok(())
        });
    }

    // MCP 확인: 칸에 적은 서버마다 붙어 도구 목록을 받아 본다(워커 스레드).
    {
        let parent = parent.clone();
        let txt_mcp = txt_mcp.clone();
        let btn_mcp_check2 = btn_mcp_check.clone();
        btn_mcp_check.on().bn_clicked(move || {
            let (servers, errors) = mcp::parse_servers(&txt_mcp.text().unwrap_or_default());
            if !errors.is_empty() {
                set_mcp_status(&format!("MCP: ✖ 잘못된 줄 — {}", errors.join(" / ")));
                return Ok(());
            }
            if servers.is_empty() {
                set_mcp_status("MCP: 서버 없음 — 도구 없이 요청한다");
                return Ok(());
            }
            set_mcp_status(&format!("MCP: … 서버 {}개 확인 중", servers.len()));
            btn_mcp_check2.hwnd().EnableWindow(false);
            let wnd = parent.clone();
            std::thread::spawn(move || {
                let results = mcp::check_servers(&servers);
                let text = results
                    .iter()
                    .map(|r| match &r.outcome {
                        Ok(tools) => format!("{} ✔ 도구 {}개({}ms)", r.name, tools.len(), r.elapsed_ms),
                        Err(e) => format!("{} ✖ {e}", r.name),
                    })
                    .collect::<Vec<_>>()
                    .join(" · ");
                wnd.run_ui_thread(move || {
                    set_mcp_status(&format!("MCP: {text}"));
                    with_ui(|ui| {
                        if let Some(s) = &ui.settings {
                            s.btn_mcp_check.hwnd().EnableWindow(true);
                        }
                    });
                    Ok(())
                });
            });
            Ok(())
        });
    }

    // 키·주소 칸의 값까지 반영한 설정으로 서버를 점검한다(키를 넣으면 Claude API). 같은 키·주소로
    // 이미 점검했으면 `force`가 아닐 때 다시 묻지 않는다.
    let check = {
        let parent = parent.clone();
        let txt_key = txt_key.clone();
        let txt_url = txt_url.clone();
        let base = Arc::clone(&base);
        let last = Rc::new(RefCell::new((session.api_key.clone(), session.base_url.clone())));
        Rc::new(move |force: bool| {
            let key = txt_key.text().unwrap_or_default();
            let url = txt_url.text().unwrap_or_default();
            let now = (key, url);
            if !force && *last.borrow() == now {
                return;
            }
            let cfg = effective_cfg(&base, &now.0, &now.1, "");
            *last.borrow_mut() = now;
            check_server(&parent, cfg, Target::Settings);
        })
    };
    {
        let check = Rc::clone(&check);
        btn_check.on().bn_clicked(move || {
            check(true);
            Ok(())
        });
    }
    {
        let check = Rc::clone(&check);
        txt_key.on().en_kill_focus(move || {
            check(false);
            Ok(())
        });
    }
    txt_url.on().en_kill_focus(move || {
        check(false);
        Ok(())
    });

    {
        let dlg2 = dlg.clone();
        let parent = parent.clone();
        btn_ok.on().bn_clicked(move || {
            let mcp_text = txt_mcp.text().unwrap_or_default().replace("\r\n", "\n");
            let (mcp_servers, mcp_errors) = mcp::parse_servers(&mcp_text);
            if !mcp_errors.is_empty() {
                let _ = dlg2.hwnd().MessageBox(
                    &format!("MCP 서버 칸에 잘못된 줄이 있습니다.\n\n{}", mcp_errors.join("\n")),
                    "설정 확인",
                    co::MB::OK | co::MB::ICONWARNING,
                );
                return Ok(());
            }
            // 설명 줄만 있으면 저장하지 않는다(다음에 열 때 다시 보인다).
            let mcp_to_save = if mcp_text.trim() == MCP_TEMPLATE.trim() {
                ""
            } else {
                mcp_text.as_str()
            };
            let custom = txt_custom.text().unwrap_or_default();
            let key = txt_key.text().unwrap_or_default().trim().to_string();
            let saved = settings::GuiSettings {
                base_url: txt_url.text().unwrap_or_default().trim().to_string(),
                model: cmb_model
                    .hwnd()
                    .GetWindowText()
                    .unwrap_or_default()
                    .trim()
                    .to_string(),
                format: Some(selected_format(&cmb_format)),
                include_open_docs: with_ui(|ui| ui.session.include_open_docs).unwrap_or(false),
            };
            let written = settings::save_custom_system_prompt(&custom)
                .and_then(|()| settings::save_gui_settings(&saved))
                .and_then(|()| settings::save_mcp_servers_text(mcp_to_save));
            if let Err(e) = written {
                tracing::warn!(error = %format!("{e:#}"), "설정 저장 실패");
                let _ = dlg2.hwnd().MessageBox(
                    &format!("{e:#}"),
                    "저장 실패",
                    co::MB::OK | co::MB::ICONERROR,
                );
                return Ok(());
            }
            let check = with_ui(|ui| {
                let s = &mut ui.session;
                let changed =
                    s.api_key != key || s.base_url != saved.base_url || s.model != saved.model;
                s.api_key = key.clone();
                s.base_url = saved.base_url.clone();
                s.model = saved.model.clone();
                s.format = saved.format.unwrap_or(DocFormat::DEFAULT);
                update_summary(ui);
                changed.then(|| check_cfg(ui))
            })
            .flatten();
            tracing::info!(
                key_set = !key.is_empty(),
                base_url = %saved.base_url,
                model = %saved.model,
                format = ?saved.format,
                mcp_servers = mcp_servers.len(),
                custom_chars = custom.trim().chars().count(),
                "설정 저장·적용"
            );
            dlg2.close();
            // 키·주소·모델이 바뀌었으면 메인 창의 상태를 다시 점검한다(대화의 API 종류도 이때 바뀐다).
            if let Some(cfg) = check {
                check_server(&parent, cfg, Target::Main);
            }
            Ok(())
        });
    }
    {
        let dlg2 = dlg.clone();
        btn_cancel.on().bn_clicked(move || {
            dlg2.close();
            Ok(())
        });
    }
    if let Err(e) = dlg.show_modal(parent) {
        tracing::warn!(error = %e, "설정 창을 열지 못함");
    }
    // 창이 닫힌 뒤 도착한 점검 결과는 버린다.
    with_ui(|ui| ui.settings = None);
}

/// 설정 창의 MCP 상태 줄을 바꾼다(UI 스레드). 설정 창이 닫혀 있으면 버린다.
fn set_mcp_status(text: &str) {
    with_ui(|ui| {
        if let Some(s) = &ui.settings {
            let _ = s.lbl_mcp_status.hwnd().SetWindowText(text);
        }
    });
}

/// 고른 형식에 맞게 문서 내용과(가능하면) 커서 위치를 확보한다.
///
/// 1) 환경변수 `EAT_HWP_DOC`가 지정되면 그 파일을 읽는다(테스트 경로, 커서 위치 없음). 평문·HWPML
///    형식은 HWPML 파일, Markdown 형식은 `.hwpx` 파일이어야 한다.
/// 2) 아니면 실행 중인 한글에 연결한다. 평문·HWPML은 `GetTextFile`(HWPML2X)과 커서 위치를,
///    Markdown은 커서 책갈피를 넣어 임시 HWPX로 저장(`SaveAs`)한 뒤 hwp2md로 바꾼 것을,
///    HWPML 파일은 `SaveAs(HWPML2X, "lock:false")`로 임시 저장한 파일을 얻는다.
///
/// `include_open_docs`면 한글에 열려 있는 다른 문서도 같은 형식으로 읽어 `others`에 담는다
/// ([`hwp::other_documents`]). `picked`가 있으면 그 문서만 읽는다. `EAT_HWP_DOC`로 시험할 때는 읽지 않는다.
fn acquire_document(
    format: DocFormat,
    include_open_docs: bool,
    picked: Option<&[hwp::DocIdentity]>,
    bound: Option<&hwp::DocIdentity>,
) -> Result<DocSnapshot> {
    if let Ok(p) = std::env::var("EAT_HWP_DOC") {
        if !p.is_empty() {
            let content = match format {
                DocFormat::Markdown => {
                    DocContent::Markdown(markdown::hwpx_to_markdown(std::path::Path::new(&p), None)?)
                }
                DocFormat::Plain | DocFormat::Hwpml | DocFormat::HwpmlFile => DocContent::Hwpml {
                    hwpml: std::fs::read_to_string(&p)
                        .map_err(|e| anyhow!("EAT_HWP_DOC 파일을 읽지 못함: {p}: {e}"))?,
                    caret: None,
                },
            };
            return Ok(DocSnapshot {
                content,
                path: p,
                origin: None,
                others: Vec::new(),
                other_notes: Vec::new(),
            });
        }
    }
    let hwp = hwp::connect()?;
    // 이 창이 고정된 문서(`bound`)를 읽는다. 한글에서 다른 탭을 보고 있으면 그 문서를 활성화해 읽고,
    // 다 읽은 뒤 보던 탭으로 되돌린다. 답은 읽은 문서에만 넣는다. 읽는 사이 활성 문서가 바뀌면 어느
    // 문서를 읽었는지 확실하지 않으므로 실패로 본다.
    let previous = hwp::active_document(&hwp)?;
    let before = match bound {
        Some(bound) => {
            hwp::activate(&hwp, bound)?;
            bound.clone()
        }
        None => previous.clone(),
    };
    let mut snapshot = match format {
        DocFormat::Plain | DocFormat::Hwpml => hwp::document_snapshot(&hwp),
        DocFormat::Markdown => hwp::document_markdown(&hwp),
        DocFormat::HwpmlFile => hwp::document_hwpml_file(&hwp),
    }?;
    let after = hwp::active_document(&hwp)?;
    if before != after {
        return Err(anyhow!(
            "문서를 읽는 사이 활성 문서가 바뀌었습니다({:?} → {:?}). 다시 보내 주세요",
            before.full_name,
            after.full_name
        ));
    }
    if include_open_docs {
        let (others, notes) =
            hwp::other_documents(&hwp, &before, format == DocFormat::Markdown, picked)?;
        snapshot.others = others;
        snapshot.other_notes = notes;
    }
    if previous != before
        && let Err(e) = hwp::activate(&hwp, &previous)
    {
        tracing::warn!(error = %format!("{e:#}"), "보던 문서로 돌아가지 못함");
    }
    snapshot.origin = Some(before);
    Ok(snapshot)
}
