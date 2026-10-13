//! winsafe 기반 GUI 셸 (eat_hwp_raw.md "eat_hwp_raw GUI").
//!
//! 구성:
//! - 현재문단/전체문서 컨텍스트 지정 체크박스
//! - 프롬프트/답변 textarea (multiline Edit; 답변은 read-only)
//! - 실행(commit) 버튼 → 워크플로 실행 → 답변 표시 + 처리완료 알람(MessageBox)
//!
//! 문서 내용은 `hwp` 모듈이 실행 중인 한글에서 `GetTextFile`(HWPML2X)로 읽어 온다. 한글 없이
//! 테스트할 때는 환경변수 `EAT_HWP_DOC`에 HWPML 파일(UTF-8) 경로를 지정하면 그 파일을 사용한다.

use crate::config::Config;
use crate::hwp::{self, DocContext};
use crate::llm::LlmBackend;
use crate::workflow::{self, Scope};
use anyhow::{Result, anyhow};
use std::sync::Arc;
use winsafe::{co, gui, prelude::*};

/// 답변 Edit / 실행 Button의 고정 컨트롤 ID.
/// 워커 스레드 완료 후 UI 스레드에서 `HWND::GetDlgItem`으로 컨트롤을 찾는 데 쓴다
/// (winsafe 컨트롤은 Send가 아니라 스레드 경계를 넘길 수 없으므로 ID로 우회).
const ID_TXT_ANSWER: u16 = 1001;
const ID_BTN_RUN: u16 = 1002;

/// 메인 윈도우와 컨트롤, 공유 상태(설정/백엔드)를 보유한다.
pub struct MainWindow {
    wnd: gui::WindowMain,
    // 라벨은 컨트롤 수명 유지를 위해 보유만 한다(읽지 않음).
    #[allow(dead_code)]
    lbl_prompt: gui::Label,
    txt_prompt: gui::Edit,
    chk_whole: gui::CheckBox,
    btn_run: gui::Button,
    #[allow(dead_code)]
    lbl_answer: gui::Label,
    txt_answer: gui::Edit,
    // 워커 스레드로 넘기기 위해 Arc(Send+Sync). 백엔드도 Send+Sync 요구.
    cfg: Arc<Config>,
    backend: Arc<dyn LlmBackend + Send + Sync>,
}

impl MainWindow {
    /// 윈도우와 컨트롤을 생성하고 이벤트를 연결한다.
    pub fn new(cfg: Config, backend: Arc<dyn LlmBackend + Send + Sync>) -> Self {
        let wnd = gui::WindowMain::new(gui::WindowMainOpts {
            title: "eat_hwp_raw — HWP AI Agent",
            size: gui::dpi(520, 470),
            ..Default::default()
        });

        let lbl_prompt = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "프롬프트",
                position: gui::dpi(10, 8),
                ..Default::default()
            },
        );
        let txt_prompt = gui::Edit::new(
            &wnd,
            gui::EditOpts {
                position: gui::dpi(10, 26),
                width: gui::dpi_x(500),
                height: gui::dpi_y(120),
                control_style: co::ES::MULTILINE
                    | co::ES::WANTRETURN
                    | co::ES::AUTOVSCROLL
                    | co::ES::NOHIDESEL,
                ..Default::default()
            },
        );
        let chk_whole = gui::CheckBox::new(
            &wnd,
            gui::CheckBoxOpts {
                text: "전체 문서 컨텍스트 (해제 시 현재 문단)",
                position: gui::dpi(10, 156),
                ..Default::default()
            },
        );
        let btn_run = gui::Button::new(
            &wnd,
            gui::ButtonOpts {
                text: "실행 (Commit)",
                position: gui::dpi(10, 182),
                width: gui::dpi_x(140),
                ctrl_id: ID_BTN_RUN,
                ..Default::default()
            },
        );
        let lbl_answer = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "답변",
                position: gui::dpi(10, 222),
                ..Default::default()
            },
        );
        let txt_answer = gui::Edit::new(
            &wnd,
            gui::EditOpts {
                position: gui::dpi(10, 240),
                width: gui::dpi_x(500),
                height: gui::dpi_y(200),
                control_style: co::ES::MULTILINE
                    | co::ES::AUTOVSCROLL
                    | co::ES::NOHIDESEL
                    | co::ES::READONLY,
                ctrl_id: ID_TXT_ANSWER,
                ..Default::default()
            },
        );

        let new_self = Self {
            wnd,
            lbl_prompt,
            txt_prompt,
            chk_whole,
            btn_run,
            lbl_answer,
            txt_answer,
            cfg: Arc::new(cfg),
            backend,
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
        let chk = self.chk_whole.clone();
        let txt_prompt = self.txt_prompt.clone();
        let txt_answer = self.txt_answer.clone();
        let btn_run = self.btn_run.clone();
        let cfg = Arc::clone(&self.cfg);
        let backend = Arc::clone(&self.backend);

        self.btn_run.on().bn_clicked(move || {
            let scope = if chk.is_checked() {
                Scope::WholeDocument
            } else {
                Scope::CurrentParagraph
            };
            let user_prompt = txt_prompt.text().unwrap_or_default();

            // UI 스레드: 즉시 피드백 + 재진입 방지(버튼 비활성화).
            let _ = txt_answer.set_text("처리 중...");
            btn_run.hwnd().EnableWindow(false);

            // 워커 스레드로 넘길 Send 상태만 복제.
            let wnd = wnd.clone();
            let cfg = Arc::clone(&cfg);
            let backend = Arc::clone(&backend);

            std::thread::spawn(move || {
                let result = run_workflow(&cfg, backend.as_ref(), &user_prompt, scope);
                let wnd_ui = wnd.clone();
                wnd.run_ui_thread(move || {
                    finish_on_ui(&wnd_ui, result);
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

/// 워커 결과를 UI 스레드에서 반영한다(버튼 재활성화, 답변 표시, 완료 알람).
///
/// 컨트롤은 스레드를 못 넘기므로 고정 ID로 `GetDlgItem`하여 접근한다.
fn finish_on_ui(wnd: &gui::WindowMain, result: Result<String>) {
    let root = wnd.hwnd();
    if let Ok(btn) = root.GetDlgItem(ID_BTN_RUN) {
        btn.EnableWindow(true);
    }
    match result {
        Ok(answer) => {
            if let Ok(ans) = root.GetDlgItem(ID_TXT_ANSWER) {
                let _ = ans.SetWindowText(&answer);
            }
            let _ = root.MessageBox(
                "처리 완료",
                "eat_hwp_raw",
                co::MB::OK | co::MB::ICONINFORMATION,
            );
        }
        Err(e) => {
            if let Ok(ans) = root.GetDlgItem(ID_TXT_ANSWER) {
                let _ = ans.SetWindowText(&format!("[오류] {e:#}"));
            }
        }
    }
}

/// 문서 내용(HWPML)과 커서 컨텍스트를 확보해 워크플로를 실행한다.
fn run_workflow(
    cfg: &Config,
    backend: &dyn LlmBackend,
    user_prompt: &str,
    scope: Scope,
) -> Result<String> {
    let (hwpml, ctx) = acquire_hwpml()?;
    // 본문(list 0) 커서일 때만 현재 문단 인덱스를 넘긴다. (그 외엔 전체 문서로 폴백)
    let cursor_para = ctx.and_then(|c| c.body_paragraph_index());
    workflow::run_prompt(cfg, backend, &hwpml, user_prompt, scope, cursor_para)
}

/// 문서의 HWPML과(가능하면) 커서 컨텍스트를 확보한다.
///
/// 1) 환경변수 `EAT_HWP_DOC`가 지정되면 그 HWPML 파일을 읽는다(테스트 경로, 커서 컨텍스트 없음).
/// 2) 아니면 실행 중인 한글에 연결 → 현재 컨텍스트(GetPos) 확보 → GetTextFile(HWPML2X).
fn acquire_hwpml() -> Result<(String, Option<DocContext>)> {
    if let Ok(p) = std::env::var("EAT_HWP_DOC") {
        if !p.is_empty() {
            // 테스트 경로: 기존 HWPML 파일만 사용하고 라이브 커서 컨텍스트는 없다.
            let hwpml = std::fs::read_to_string(&p)
                .map_err(|e| anyhow!("EAT_HWP_DOC 파일을 읽지 못함: {p}: {e}"))?;
            return Ok((hwpml, None));
        }
    }
    let hwp = hwp::connect()?;
    let ctx = hwp::current_context(&hwp)?;
    let hwpml = hwp::document_hwpml(&hwp)?;
    Ok((hwpml, Some(ctx)))
}
