//! winsafe 기반 GUI 셸 (CLAUDE.md "eat_hwp_raw GUI"의 현재 구현).
//!
//! 목표 구조에서 GUI는 `eat_hwp_addon`으로 옮기고 구성도 바뀐다(서버 지정, 프롬프트 수정,
//! 전송·통신 상황). LLM 설정은 서버에 두므로 Claude API 키 입력 칸은 남지 않고, 답변 칸과
//! 처리완료 알람은 구현을 보류한다.
//!
//! 구성:
//! - Claude API 키 입력 칸(비우면 환경 변수 설정대로 실행)
//! - 모델 칸: 백엔드가 제공하는 모델 목록에서 고르거나 직접 입력
//! - LLM 상태 줄: 지금 설정(키 칸 포함)이 가리키는 서버가 작동하는지 보인다. 창을 열 때, "상태 확인"을
//!   누를 때, 키 칸에서 나갈 때 서버에 모델 목록을 요청해 점검하고(모델 칸 목록도 이때 채운다),
//!   요청이 성공하면 "작동 중"으로, 실패하면 다시 점검해 원인(연결 안 됨, 인증 실패 등)을 보인다
//! - 대화 칸(read-only multiline Edit): 보낸 프롬프트와 받은 답변을 채팅처럼 차례로 쌓는다
//!   - `▶ 나` / `◀ AI (모델, 걸린 시간)` 머리줄로 말한 쪽을 구분하고, 한 차례가 끝날 때마다
//!     구분선을 넣는다. 응답을 기다리는 동안은 `◀ AI — 응답을 기다리는 중…`을 보이고, 답이 오면
//!     그 줄을 답변으로 바꾼다. 실패하면 `◀ 오류`로 남긴다
//!   - 각 요청은 독립이다. 이전 대화는 LLM에 보내지 않는다
//! - 프롬프트 입력 칸 + 보내기 버튼. 보내면 입력 칸을 비운다. 처리완료 팝업은 없다
//!
//! LLM에는 항상 문서 전체를 커서 위치 표시와 함께 보낸다.
//!
//! 문서 내용은 `hwp` 모듈이 실행 중인 한글에서 `GetTextFile`(HWPML2X)로 읽어 온다. 한글 없이
//! 테스트할 때는 환경변수 `EAT_HWP_DOC`에 HWPML 파일(UTF-8) 경로를 지정하면 그 파일을 사용한다.

use crate::config::Config;
use crate::hwp::{self, DocSnapshot};
use crate::llm::{self, LlmBackend, RigBackend, ServerState, ServerStatus};
use crate::workflow;
use anyhow::{Result, anyhow};
use std::cell::RefCell;
use std::sync::Arc;
use std::time::Instant;
use winsafe::{co, gui, prelude::*};

/// 보내기 Button의 고정 컨트롤 ID.
/// 워커 스레드 완료 후 UI 스레드에서 `HWND::GetDlgItem`으로 컨트롤을 찾는 데 쓴다
/// (winsafe 컨트롤은 Send가 아니라 스레드 경계를 넘길 수 없으므로 ID로 우회).
const ID_BTN_RUN: u16 = 1002;

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
    btn_models: gui::Button,
    #[allow(dead_code)]
    lbl_status: gui::Label,
    // 워커 스레드로 넘기기 위해 Arc(Send+Sync).
    cfg: Arc<Config>,
}

impl MainWindow {
    /// 윈도우와 컨트롤을 생성하고 이벤트를 연결한다.
    pub fn new(cfg: Config) -> Self {
        let wnd = gui::WindowMain::new(gui::WindowMainOpts {
            title: "eat_hwp_raw — HWP AI Agent",
            size: gui::dpi(520, 504),
            ..Default::default()
        });

        // 프롬프트 칸이 창을 열 때 포커스를 받도록 가장 먼저 만든다(배치는 맨 아래).
        let lbl_prompt = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "프롬프트",
                position: gui::dpi(10, 386),
                ..Default::default()
            },
        );
        let txt_prompt = gui::Edit::new(
            &wnd,
            gui::EditOpts {
                position: gui::dpi(10, 404),
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
                width: gui::dpi_x(90),
                ctrl_id: ID_BTN_RUN,
                ..Default::default()
            },
        );
        let lbl_chat = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "대화",
                position: gui::dpi(10, 126),
                ..Default::default()
            },
        );
        let txt_chat = gui::Edit::new(
            &wnd,
            gui::EditOpts {
                position: gui::dpi(10, 144),
                width: gui::dpi_x(500),
                height: gui::dpi_y(232),
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
                // 나중에 더 긴 상태 문구로 바뀌므로 글자 길이에 맞추지 않고 폭을 고정한다.
                size: gui::dpi(500, 16),
                ..Default::default()
            },
        );
        let cmb_model = gui::ComboBox::new(
            &wnd,
            gui::ComboBoxOpts {
                position: gui::dpi(10, 76),
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
                width: gui::dpi_x(110),
                ..Default::default()
            },
        );
        let lbl_status = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "LLM 상태: 확인 전",
                position: gui::dpi(10, 104),
                // 상태 문구가 바뀌어 길어지므로 폭을 고정한다.
                size: gui::dpi(500, 16),
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

        self.btn_run.on().bn_clicked(move || {
            let user_prompt = txt_prompt.text().unwrap_or_default();
            // 키를 입력했으면 Claude API로, 비웠으면 환경 변수 설정대로 실행한다.
            // 모델 칸이 비어 있지 않으면 그 모델을 쓴다.
            let api_key = txt_api_key.text().unwrap_or_default();
            let model = cmb_model.hwnd().GetWindowText().unwrap_or_default();
            let cfg = effective_cfg(&cfg, &api_key, &model);

            // UI 스레드: 대화 칸에 프롬프트와 대기 줄을 쌓고, 입력 칸을 비우고, 재진입을 막는다.
            chat_begin(&user_prompt);
            let _ = txt_prompt.set_text("");
            btn_run.hwnd().EnableWindow(false);

            // 워커 스레드로 넘길 Send 상태만 복제.
            let wnd = wnd.clone();

            std::thread::spawn(move || {
                let started = Instant::now();
                // 키가 실행마다 달라질 수 있어 백엔드는 실행할 때 만든다.
                let result = RigBackend::new(&cfg)
                    .and_then(|backend| run_workflow(&cfg, &backend, &user_prompt));
                let secs = started.elapsed().as_secs();
                let header = format!("◀ AI ({}, {secs}초)", cfg.llm_model);
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
    let wnd = wnd.clone();
    std::thread::spawn(move || {
        let status = llm::check_server(&cfg);
        let default_model = cfg.llm_model.clone();
        wnd.run_ui_thread(move || {
            show_status(status, &default_model);
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
fn show_status(status: ServerStatus, default_model: &str) {
    let ServerStatus {
        target,
        state,
        elapsed_ms,
    } = status;
    let text = match state {
        ServerState::Working(models) => {
            let missing = !fill_models(&models, default_model);
            format!(
                "LLM 상태: ✔ 작동 중 — {target} (응답 {elapsed_ms}ms, 모델 {}개){}",
                models.len(),
                if missing { " · ⚠ 선택한 모델이 목록에 없음" } else { "" }
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

/// 모델 칸 목록을 `models`로 바꾼다. 칸에 남은 모델이 목록에 있으면 `true`.
fn fill_models(models: &[String], default_model: &str) -> bool {
    SERVER_UI.with_borrow(|ui| {
        let Some((cmb, _)) = ui else {
            return true;
        };
        let current = cmb.hwnd().GetWindowText().unwrap_or_default();
        let current = current.trim();
        let keep = if models.iter().any(|m| m == current) {
            current.to_string()
        } else {
            default_model.to_string()
        };
        cmb.items().delete_all();
        let _ = cmb.items().add(models);
        match models.iter().position(|m| *m == keep) {
            Some(i) => {
                cmb.items().select(Some(i as u32));
                true
            }
            None => {
                let _ = cmb.hwnd().SetWindowText(&keep);
                false
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
        text.push_str(&format!("▶ 나\r\n{}\r\n\r\n", to_crlf(prompt)));
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
            .replace_selection(&format!("{header}\r\n{}\r\n\r\n", to_crlf(body.trim_end())));
    });
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

/// 문서 내용(HWPML)과 커서 위치를 확보해 워크플로를 실행한다.
fn run_workflow(
    cfg: &Config,
    backend: &dyn LlmBackend,
    user_prompt: &str,
) -> Result<String> {
    let doc = acquire_document()?;
    workflow::run_prompt(cfg, backend, &doc.hwpml, user_prompt, doc.caret)
}

/// 문서의 HWPML과(가능하면) 커서 위치를 확보한다.
///
/// 1) 환경변수 `EAT_HWP_DOC`가 지정되면 그 HWPML 파일을 읽는다(테스트 경로, 커서 위치 없음).
/// 2) 아니면 실행 중인 한글에 연결 → HWPML과 그 안의 커서 위치 확보.
fn acquire_document() -> Result<DocSnapshot> {
    if let Ok(p) = std::env::var("EAT_HWP_DOC") {
        if !p.is_empty() {
            // 테스트 경로: 기존 HWPML 파일만 사용하고 라이브 커서 위치는 없다.
            let hwpml = std::fs::read_to_string(&p)
                .map_err(|e| anyhow!("EAT_HWP_DOC 파일을 읽지 못함: {p}: {e}"))?;
            return Ok(DocSnapshot { hwpml, caret: None });
        }
    }
    let hwp = hwp::connect()?;
    hwp::document_snapshot(&hwp)
}
