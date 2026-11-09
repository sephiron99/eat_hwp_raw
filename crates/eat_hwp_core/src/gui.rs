//! winsafe 기반 GUI 셸 (CLAUDE.md "eat_hwp_raw GUI"의 현재 구현).
//!
//! 목표 구조에서 GUI는 `eat_hwp_addon`으로 옮기고 구성도 바뀐다(서버 지정, 프롬프트 수정,
//! 전송·통신 상황). LLM 설정은 서버에 두므로 Claude API 키 입력 칸은 남지 않고, 답변 칸과
//! 처리완료 알람은 구현을 보류한다.
//!
//! 구성:
//! - Claude API 키 입력 칸(비우면 환경 변수 설정대로 실행)
//! - 모델 칸: 백엔드가 제공하는 모델 목록에서 고르거나 직접 입력
//! - 문서 형식 칸: LLM에 보낼 문서 텍스트의 형식(평문 / HWPML 원문 / HWPX→Markdown /
//!   SaveAs HWPML 파일)을 고른다
//! - LLM 상태 줄: 지금 설정(키 칸 포함)이 가리키는 서버가 작동하는지 보인다. 창을 열 때, "상태 확인"을
//!   누를 때, 키 칸에서 나갈 때 서버에 모델 목록을 요청해 점검하고(모델 칸 목록도 이때 채운다),
//!   요청이 성공하면 "작동 중"으로, 실패하면 다시 점검해 원인(연결 안 됨, 인증 실패 등)을 보인다
//! - 대화 칸(read-only multiline Edit): 보낸 프롬프트와 받은 답변을 채팅처럼 차례로 쌓는다
//!   - `▶ 나 · 시각` / `◀ AI (모델, 걸린 시간) · 시각` 머리줄로 말한 쪽과 그 글이 대화 칸에 나온
//!     시각(시스템 로컬 시간, 초 단위)을 보이고, 한 차례가 끝날 때마다
//!     구분선을 넣는다. 응답을 기다리는 동안은 `◀ AI — 응답을 기다리는 중…`을 보이고, 답이 오면
//!     그 줄을 답변으로 바꾼다. 실패하면 `◀ 오류`로 남긴다
//!   - 각 요청은 독립이다. 이전 대화는 LLM에 보내지 않는다
//!   - 대화는 문서와 API 종류(로컬/OpenAI 호환, Claude)마다 `chat_store`가 정한 파일에 저장되고,
//!     같은 문서·API로 돌아오면 다시 불러온다. 창을 열 때 한글의 현재 문서를, 요청을 보낼 때 읽은
//!     문서를, 상태를 점검할 때 그 설정의 API 종류를 기준으로 바꾼다
//! - 프롬프트 입력 칸 + 보내기 버튼(Ctrl+Enter로도 보낸다). 보내면 입력 칸을 비운다. 처리완료
//!   팝업은 없다
//!
//! LLM에는 항상 문서 전체를 커서 위치 표시와 함께 보낸다.
//!
//! 문서 내용은 `hwp` 모듈이 실행 중인 한글에서 `GetTextFile`(HWPML2X)로 읽어 온다. 한글 없이
//! 테스트할 때는 환경변수 `EAT_HWP_DOC`에 HWPML 파일(UTF-8) 경로를 지정하면 그 파일을 사용한다.

use crate::chat_store;
use crate::config::{Config, Provider};
use crate::hwp::{self, DocContent, DocSnapshot};
use crate::markdown;
use crate::llm::{self, LlmBackend, RigBackend, ServerState, ServerStatus};
use crate::workflow::{self, DocFormat};
use anyhow::{Result, anyhow};
use std::cell::RefCell;
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

/// 대화 칸에서 한 차례(프롬프트 + 답변)가 끝날 때마다 넣는 구분선.
const TURN_SEPARATOR: &str = "────────────────────────────";

thread_local! {
    /// 모델 콤보박스와 LLM 상태 줄. winsafe 컨트롤은 스레드를 못 넘기므로, 서버를 점검한 워커가
    /// `run_ui_thread`로 UI 스레드에 돌아온 뒤 여기서 꺼내 쓴다. UI 스레드에서만 채워진다.
    static SERVER_UI: RefCell<Option<(gui::ComboBox, gui::Label)>> = const { RefCell::new(None) };

    /// 대화 칸과, 응답 대기 줄이 시작하는 위치(UTF-16 글자 수). UI 스레드에서만 쓴다.
    static CHAT: RefCell<Option<Chat>> = const { RefCell::new(None) };
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

/// 메인 윈도우와 컨트롤, 공유 상태(설정)를 보유한다.
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
    lbl_api_key: gui::Label,
    txt_api_key: gui::Edit,
    #[allow(dead_code)]
    lbl_model: gui::Label,
    cmb_model: gui::ComboBox,
    #[allow(dead_code)]
    lbl_format: gui::Label,
    cmb_format: gui::ComboBox,
    btn_models: gui::Button,
    #[allow(dead_code)]
    lbl_status: gui::Label,
    // 워커 스레드로 넘기기 위해 Arc(Send+Sync).
    cfg: Arc<Config>,
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
        let wnd = gui::WindowMain::new(gui::WindowMainOpts {
            accel_table,
            title: "eat_hwp_raw — HWP AI Agent",
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
                width: gui::dpi_x(400),
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
                position: gui::dpi(420, 404),
                resize_behavior: (gui::Horz::Repos, gui::Vert::Repos),
                width: gui::dpi_x(90),
                ctrl_id: ID_BTN_RUN,
                ..Default::default()
            },
        );
        let lbl_chat = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "대화",
                position: gui::dpi(10, 156),
                ..Default::default()
            },
        );
        let txt_chat = gui::Edit::new(
            &wnd,
            gui::EditOpts {
                position: gui::dpi(10, 174),
                resize_behavior: (gui::Horz::Resize, gui::Vert::Resize),
                width: gui::dpi_x(500),
                height: gui::dpi_y(202),
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
        let lbl_api_key = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "Claude API 키 (비우면 환경 변수 설정 사용)",
                position: gui::dpi(10, 8),
                ..Default::default()
            },
        );
        let txt_api_key = gui::Edit::new(
            &wnd,
            gui::EditOpts {
                position: gui::dpi(10, 26),
                resize_behavior: (gui::Horz::Resize, gui::Vert::None),
                width: gui::dpi_x(500),
                control_style: co::ES::AUTOHSCROLL | co::ES::PASSWORD,
                ..Default::default()
            },
        );
        let lbl_model = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "모델 (목록에서 고르거나 직접 입력)",
                position: gui::dpi(10, 58),
                resize_behavior: (gui::Horz::Resize, gui::Vert::None),
                // 나중에 더 긴 상태 문구로 바뀌므로 글자 길이에 맞추지 않고 폭을 고정한다.
                size: gui::dpi(500, 16),
                ..Default::default()
            },
        );
        let cmb_model = gui::ComboBox::new(
            &wnd,
            gui::ComboBoxOpts {
                position: gui::dpi(10, 76),
                resize_behavior: (gui::Horz::Resize, gui::Vert::None),
                width: gui::dpi_x(380),
                // 목록에 없는 모델 이름도 직접 입력할 수 있게 편집형으로 둔다.
                control_style: co::CBS::DROPDOWN,
                ..Default::default()
            },
        );
        let btn_models = gui::Button::new(
            &wnd,
            gui::ButtonOpts {
                text: "상태 확인",
                position: gui::dpi(400, 75),
                resize_behavior: (gui::Horz::Repos, gui::Vert::None),
                width: gui::dpi_x(110),
                ..Default::default()
            },
        );
        let lbl_status = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "LLM 상태: 확인 전",
                position: gui::dpi(10, 104),
                resize_behavior: (gui::Horz::Resize, gui::Vert::None),
                // 상태 문구가 바뀌어 길어지므로 폭을 고정한다.
                size: gui::dpi(500, 16),
                ..Default::default()
            },
        );
        let lbl_format = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "LLM에 보낼 문서",
                position: gui::dpi(10, 130),
                ..Default::default()
            },
        );
        let format_labels: Vec<&str> = DocFormat::ALL.iter().map(|f| f.label()).collect();
        let cmb_format = gui::ComboBox::new(
            &wnd,
            gui::ComboBoxOpts {
                position: gui::dpi(120, 126),
                width: gui::dpi_x(270),
                items: &format_labels,
                selected_item: DocFormat::ALL
                    .iter()
                    .position(|f| *f == DocFormat::DEFAULT)
                    .map(|i| i as u32),
                ..Default::default()
            },
        );
        SERVER_UI.with_borrow_mut(|ui| *ui = Some((cmb_model.clone(), lbl_status.clone())));

        let new_self = Self {
            wnd,
            lbl_prompt,
            txt_prompt,
            btn_run,
            lbl_chat,
            txt_chat,
            lbl_api_key,
            txt_api_key,
            lbl_model,
            cmb_model,
            lbl_format,
            cmb_format,
            btn_models,
            lbl_status,
            cfg: Arc::new(cfg),
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
        let txt_api_key = self.txt_api_key.clone();
        let cmb_model = self.cmb_model.clone();
        let cmb_format = self.cmb_format.clone();
        let cfg = Arc::clone(&self.cfg);

        // 창이 만들어지면 모델 칸에 설정의 모델을 넣고 목록을 받아 온다.
        {
            let wnd2 = wnd.clone();
            let cmb_model = self.cmb_model.clone();
            let txt_chat = self.txt_chat.clone();
            let cfg = Arc::clone(&self.cfg);
            self.wnd.on().wm_create(move |_| {
                // 여러 차례 쌓여도 잘리지 않게 글자 수 제한을 최대로 둔다(기본은 약 3만 자).
                txt_chat.limit_text(None);
                let _ = cmb_model.hwnd().SetWindowText(&cfg.llm_model);
                check_server(&wnd2, Arc::clone(&cfg));
                load_current_document_chat(&wnd2);
                Ok(0)
            });
        }

        // 상태 확인: 키 칸의 값까지 반영한 설정으로 서버를 점검한다(키를 넣으면 Claude API).
        {
            let wnd2 = wnd.clone();
            let txt_api_key = self.txt_api_key.clone();
            let cfg = Arc::clone(&self.cfg);
            self.btn_models.on().bn_clicked(move || {
                let key = txt_api_key.text().unwrap_or_default();
                check_server(&wnd2, effective_cfg(&cfg, &key, ""));
                Ok(())
            });
        }

        // 키 칸에서 나가면 바뀐 키로 다시 점검한다. 같은 키면 다시 묻지 않는다.
        {
            let wnd2 = wnd.clone();
            let txt_api_key = self.txt_api_key.clone();
            let cfg = Arc::clone(&self.cfg);
            let last_key = RefCell::new(String::new());
            self.txt_api_key.on().en_kill_focus(move || {
                let key = txt_api_key.text().unwrap_or_default();
                if *last_key.borrow() != key {
                    *last_key.borrow_mut() = key.clone();
                    check_server(&wnd2, effective_cfg(&cfg, &key, ""));
                }
                Ok(())
            });
        }

        // 창을 너무 작게 줄여 컨트롤이 겹치지 않게 최소 크기를 둔다(창 테두리 포함 크기).
        self.wnd.on().wm_get_min_max_info(|p| {
            let (w, h) = gui::dpi(440, 440);
            p.info.ptMinTrackSize = winsafe::POINT::with(w, h);
            Ok(())
        });

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
            let format = cmb_format
                .items()
                .selected_index()
                .and_then(|i| DocFormat::ALL.get(i as usize).copied())
                .unwrap_or(DocFormat::DEFAULT);
            // 키를 입력했으면 Claude API로, 비웠으면 환경 변수 설정대로 실행한다.
            // 모델 칸이 비어 있지 않으면 그 모델을 쓴다.
            let api_key = txt_api_key.text().unwrap_or_default();
            let model = cmb_model.hwnd().GetWindowText().unwrap_or_default();
            let cfg = effective_cfg(&cfg, &api_key, &model);

            // UI 스레드: 입력 칸을 비우고 재진입을 막는다. 프롬프트는 문서를 읽은 뒤 그 문서의
            // 대화에 쌓는다.
            let _ = txt_prompt.set_text("");
            btn_run.hwnd().EnableWindow(false);

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
                let doc = acquire_document(format);
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
                    let provider = cfg.llm_provider;
                    let user_prompt = user_prompt.clone();
                    wnd.run_ui_thread(move || {
                        if let Some(path) = doc_path {
                            chat_set_context(Some(path), provider);
                        }
                        chat_begin(&user_prompt);
                        Ok(())
                    });
                }
                // 키가 실행마다 달라질 수 있어 백엔드는 실행할 때 만든다.
                let result = doc.and_then(|doc| {
                    RigBackend::new(&cfg)
                        .and_then(|backend| run_workflow(&cfg, &backend, &doc, format, &user_prompt))
                });
                match &result {
                    Ok(answer) => tracing::info!(
                        elapsed_ms = started.elapsed().as_millis(),
                        answer_chars = answer.chars().count(),
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
                        Ok(_) => set_status(&format!(
                            "LLM 상태: ✔ 작동 중 — {} · 마지막 요청 성공({secs}초)",
                            llm::describe_target(&cfg)
                        )),
                        // 실패하면 서버를 다시 점검해 원인을 상태 줄에 보인다.
                        Err(_) => check_server(&wnd_ui, Arc::clone(&cfg)),
                    }
                    finish_on_ui(&wnd_ui, &header, result);
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

/// 환경 변수 설정에 GUI의 키 칸과 모델 칸 값을 덮어쓴 설정. 빈 칸은 무시한다.
fn effective_cfg(base: &Arc<Config>, api_key: &str, model: &str) -> Arc<Config> {
    let mut cfg = Config::clone(base);
    if !api_key.trim().is_empty() {
        cfg = cfg.with_claude_key(api_key.trim());
    }
    if !model.trim().is_empty() {
        cfg = cfg.with_model(model.trim());
    }
    Arc::new(cfg)
}

/// 워커 스레드에서 서버를 점검하고, UI 스레드에서 상태 줄과 모델 칸을 채운다.
fn check_server(wnd: &gui::WindowMain, cfg: Arc<Config>) {
    set_status(&format!("LLM 상태: … 확인 중 — {}", llm::describe_target(&cfg)));
    // API 종류가 바뀌었으면 그 종류의 대화로 바꾼다.
    chat_set_provider(cfg.llm_provider);
    let wnd = wnd.clone();
    std::thread::spawn(move || {
        let status = llm::check_server(&cfg);
        tracing::info!(
            target = %status.target,
            state = %status.state.summary(),
            elapsed_ms = status.elapsed_ms,
            "LLM 서버 점검"
        );
        let default_model = cfg.llm_model.clone();
        let provider = cfg.llm_provider;
        wnd.run_ui_thread(move || {
            show_status(status, &default_model, provider);
            Ok(())
        });
    });
}

fn set_status(text: &str) {
    SERVER_UI.with_borrow(|ui| {
        if let Some((_, lbl)) = ui {
            let _ = lbl.hwnd().SetWindowText(text);
        }
    });
}

/// 점검 결과를 상태 줄에 보이고, 성공했으면 모델 칸 목록을 채운다(UI 스레드).
///
/// 칸에 있던 모델이 새 목록에 있으면 그대로 두고, 없으면(백엔드가 바뀐 경우 등) 설정의 기본
/// 모델로 바꾼다. 그 모델도 목록에 없으면 상태 줄에 경고를 붙인다.
fn show_status(status: ServerStatus, default_model: &str, provider: Provider) {
    let ServerStatus {
        target,
        state,
        elapsed_ms,
    } = status;
    let text = match state {
        ServerState::Working(models) => {
            // 로컬/OpenAI 호환 서버의 목록은 쓸 수 있는 모델 전부다. Claude API는 목록에 없는
            // 별칭도 받으므로 목록에 없다고 다른 모델로 바꾸지 않는다.
            let note = match fill_models(&models, default_model, provider == Provider::OpenAiCompatible) {
                ModelPick::Kept => String::new(),
                ModelPick::Replaced { missing, picked } => {
                    format!(" · 모델 {missing}이(가) 서버에 없어 {picked}을(를) 선택함")
                }
                ModelPick::Missing => " · ⚠ 선택한 모델이 목록에 없음".to_string(),
            };
            format!(
                "LLM 상태: ✔ 작동 중 — {target} (응답 {elapsed_ms}ms, 모델 {}개){note}",
                models.len()
            )
        }
        ServerState::NoKey => "LLM 상태: ✖ 키 없음 — Claude API 키를 입력하세요".to_string(),
        ServerState::Unreachable(e) => {
            format!("LLM 상태: ✖ 연결 안 됨 — {target} (서버가 꺼져 있거나 주소가 틀림): {e}")
        }
        ServerState::AuthFailed(e) => {
            format!("LLM 상태: ✖ 인증 실패 — {target} (키를 확인하세요): {e}")
        }
        ServerState::Failed(e) => format!("LLM 상태: ✖ 오류 — {target}: {e}"),
    };
    set_status(&text);
}

/// [`fill_models`]가 모델 칸에 남긴 모델.
enum ModelPick {
    /// 칸의 모델이나 설정의 기본 모델이 목록에 있어 그대로 두었다.
    Kept,
    /// 둘 다 목록에 없어 목록의 첫 모델로 바꿨다.
    Replaced { missing: String, picked: String },
    /// 목록에 없는 모델이 칸에 남았다.
    Missing,
}

/// 모델 칸 목록을 `models`로 바꾸고 쓸 모델을 고른다.
///
/// 칸의 모델 → 설정의 기본 모델 순으로 목록에 있는 것을 고른다. 둘 다 없고 `replace_missing`이면
/// 목록의 첫 모델을 고른다. 없는 모델이 남아 있으면 요청이 "model not found"로 실패하기 때문이다.
fn fill_models(models: &[String], default_model: &str, replace_missing: bool) -> ModelPick {
    SERVER_UI.with_borrow(|ui| {
        let Some((cmb, _)) = ui else {
            return ModelPick::Kept;
        };
        let current = cmb.hwnd().GetWindowText().unwrap_or_default();
        let current = current.trim();
        let wanted = if models.iter().any(|m| m == current) {
            current
        } else {
            default_model
        };
        cmb.items().delete_all();
        let _ = cmb.items().add(models);
        if let Some(i) = models.iter().position(|m| m == wanted) {
            cmb.items().select(Some(i as u32));
            return ModelPick::Kept;
        }
        match models.first() {
            Some(first) if replace_missing => {
                cmb.items().select(Some(0));
                ModelPick::Replaced {
                    missing: wanted.to_string(),
                    picked: first.clone(),
                }
            }
            _ => {
                let _ = cmb.hwnd().SetWindowText(wanted);
                ModelPick::Missing
            }
        }
    })
}

/// 워커 결과를 UI 스레드에서 반영한다(버튼 재활성화, 대기 줄을 답변이나 오류로 바꿈).
fn finish_on_ui(wnd: &gui::WindowMain, header: &str, result: Result<String>) {
    if let Ok(btn) = wnd.hwnd().GetDlgItem(ID_BTN_RUN) {
        btn.EnableWindow(true);
    }
    match result {
        Ok(answer) => chat_finish(header, &answer),
        Err(e) => chat_finish("◀ 오류", &format!("{e:#}")),
    }
    chat_save();
}

/// 워커 스레드에서 한글의 현재 문서 경로를 읽어, UI 스레드에서 그 문서의 대화를 불러온다.
/// 한글에 붙지 못하면 문서를 모르는 채로 둔다(첫 요청 때 정해진다).
fn load_current_document_chat(wnd: &gui::WindowMain) {
    let wnd = wnd.clone();
    std::thread::spawn(move || {
        let path = match std::env::var("EAT_HWP_DOC") {
            Ok(p) if !p.is_empty() => Some(p),
            _ => hwp::current_document_path().ok(),
        };
        if let Some(path) = path {
            wnd.run_ui_thread(move || {
                let provider = CHAT.with_borrow(|chat| chat.as_ref().map(|c| c.provider));
                if let Some(provider) = provider {
                    chat_set_context(Some(path), provider);
                }
                Ok(())
            });
        }
    });
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
        set_status(&format!("대화 기록 저장 실패: {e:#}"));
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

/// 확보한 문서 내용으로 워크플로를 실행한다.
fn run_workflow(
    cfg: &Config,
    backend: &dyn LlmBackend,
    doc: &DocSnapshot,
    format: DocFormat,
    user_prompt: &str,
) -> Result<String> {
    workflow::run_prompt(cfg, backend, &doc.content, format, user_prompt)
}

/// 고른 형식에 맞게 문서 내용과(가능하면) 커서 위치를 확보한다.
///
/// 1) 환경변수 `EAT_HWP_DOC`가 지정되면 그 파일을 읽는다(테스트 경로, 커서 위치 없음). 평문·HWPML
///    형식은 HWPML 파일, Markdown 형식은 `.hwpx` 파일이어야 한다.
/// 2) 아니면 실행 중인 한글에 연결한다. 평문·HWPML은 `GetTextFile`(HWPML2X)과 커서 위치를,
///    Markdown은 커서 책갈피를 넣어 임시 HWPX로 저장(`SaveAs`)한 뒤 hwp2md로 바꾼 것을,
///    HWPML 파일은 `SaveAs(HWPML2X, "lock:false")`로 임시 저장한 파일을 얻는다.
fn acquire_document(format: DocFormat) -> Result<DocSnapshot> {
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
            return Ok(DocSnapshot { content, path: p });
        }
    }
    let hwp = hwp::connect()?;
    match format {
        DocFormat::Plain | DocFormat::Hwpml => hwp::document_snapshot(&hwp),
        DocFormat::Markdown => hwp::document_markdown(&hwp),
        DocFormat::HwpmlFile => hwp::document_hwpml_file(&hwp),
    }
}
