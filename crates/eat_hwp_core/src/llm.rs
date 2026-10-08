//! LLM 백엔드 추상화 및 구현.
//!
//! 백엔드 정본은 rig-rs(OpenAI 호환 로컬/원격 엔드포인트, CLAUDE.md Dependencies)다.
//! 트레이트로 추상화해 두어 rig / Claude / Gemini 백엔드를 동일 인터페이스로 교체한다.
//! Claude/Gemini는 추후 지원(설계만 반영).

use crate::config::Config;
use anyhow::{Result, anyhow};
use rig::client::CompletionClient;
use rig::completion::Prompt;
use rig::providers::openai;

/// 양식 준수를 위한 기본 시스템 지침(preamble).
/// features(eat_hwp_raw.md): 제목/소제목/문단 구조 등 문서 양식을 최대한 준수한다.
const DEFAULT_PREAMBLE: &str = "당신은 HWP 문서 작성 도우미입니다. \
주어진 문서의 양식(제목/소제목/문단 구조)을 최대한 준수하여 한국어로 작성하세요.";

/// 프롬프트를 받아 응답 텍스트를 생성하는 LLM 백엔드.
pub trait LlmBackend {
    fn complete(&self, prompt: &str) -> Result<String>;
}

/// rig-rs 기반 백엔드. OpenAI 호환 엔드포인트(로컬 Ollama/vLLM, 원격 DGX 등)로 프롬프트를 실행한다.
///
/// rig API는 async이므로 내부에 tokio 런타임을 두고 `block_on`으로 동기 인터페이스에 맞춘다.
pub struct RigBackend {
    runtime: tokio::runtime::Runtime,
    client: openai::Client,
    model: String,
    preamble: String,
}

impl RigBackend {
    /// 설정으로부터 OpenAI 호환 클라이언트를 구성한다.
    pub fn new(cfg: &Config) -> Result<Self> {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| anyhow!("tokio 런타임 생성 실패: {e}"))?;
        let mut builder = openai::Client::builder().api_key(&cfg.llm_api_key);
        if !cfg.llm_base_url.is_empty() {
            builder = builder.base_url(&cfg.llm_base_url);
        }
        let client = builder
            .build()
            .map_err(|e| anyhow!("OpenAI 호환 클라이언트 생성 실패: {e}"))?;
        Ok(Self {
            runtime,
            client,
            model: cfg.llm_model.clone(),
            preamble: DEFAULT_PREAMBLE.to_string(),
        })
    }
}

impl LlmBackend for RigBackend {
    fn complete(&self, prompt: &str) -> Result<String> {
        self.runtime.block_on(async {
            let agent = self
                .client
                .agent(self.model.as_str())
                .preamble(&self.preamble)
                .build();
            agent
                .prompt(prompt)
                .await
                .map_err(|e| anyhow!("rig prompt 실패: {e}"))
        })
    }
}

/// rig 연동 없이 파이프라인을 검증하기 위한 임시 백엔드(테스트/폴백용).
#[allow(dead_code)]
pub struct EchoBackend;

impl LlmBackend for EchoBackend {
    fn complete(&self, prompt: &str) -> Result<String> {
        Ok(format!("[echo backend — rig 미연동]\n{prompt}"))
    }
}
