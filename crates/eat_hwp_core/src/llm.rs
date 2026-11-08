//! LLM 백엔드 추상화 및 구현.
//!
//! 백엔드 정본은 rig-rs(CLAUDE.md Dependencies)다. OpenAI 호환 로컬/원격 엔드포인트와
//! Claude API를 지원한다. 트레이트로 추상화해 두어 백엔드를 동일 인터페이스로 교체한다.
//! Gemini는 추후 지원(설계만 반영).

use crate::config::{Config, Provider};
use anyhow::{Result, anyhow, bail};
use rig::completion::{CompletionRequest, Reasoning};
use rig::driver::Model;
use rig::providers::anthropic::{Anthropic, AnthropicConfig};
use rig::providers::anthropic::wire::Messages;
use rig::providers::openai::{OpenAI, OpenAIConfig};
use rig::providers::openai::wire::Chat;

/// Claude API 응답의 최대 출력 토큰. Messages API는 이 값을 반드시 요구한다.
const ANTHROPIC_MAX_TOKENS: u64 = 16_000;

/// 양식 준수를 위한 기본 시스템 지침(preamble).
/// features(CLAUDE.md): 제목/소제목/문단 구조 등 문서 양식을 최대한 준수한다.
const DEFAULT_PREAMBLE: &str = "당신은 HWP 문서 작성 도우미입니다. \
주어진 문서의 양식(제목/소제목/문단 구조)을 최대한 준수하여 한국어로 작성하세요.";

/// 프롬프트를 받아 응답 텍스트를 생성하는 LLM 백엔드.
pub trait LlmBackend {
    fn complete(&self, prompt: &str) -> Result<String>;
}

/// rig-rs 기반 백엔드. OpenAI 호환 엔드포인트(로컬 Ollama/vLLM, 원격 DGX 등) 또는 Claude API로
/// 프롬프트를 실행한다.
///
/// rig API는 async이므로 내부에 tokio 런타임을 두고 `block_on`으로 동기 인터페이스에 맞춘다.
pub struct RigBackend {
    runtime: tokio::runtime::Runtime,
    model: RigModel,
    preamble: String,
}

/// 설정의 `Provider`에 따라 고른 rig 모델.
enum RigModel {
    /// `/v1/chat/completions` 경로(Ollama/vLLM 등 OpenAI 호환 서버 공통).
    OpenAiCompatible(Model<Chat>),
    /// Anthropic Messages API.
    Anthropic(Model<Messages>),
}

impl RigBackend {
    /// 설정의 `llm_provider`에 맞는 클라이언트를 구성한다.
    pub fn new(cfg: &Config) -> Result<Self> {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| anyhow!("tokio 런타임 생성 실패: {e}"))?;
        let model = match cfg.llm_provider {
            Provider::OpenAiCompatible => {
                RigModel::OpenAiCompatible(openai_client(cfg).chat(cfg.llm_model.as_str()))
            }
            Provider::Anthropic => {
                RigModel::Anthropic(anthropic_client(cfg)?.completion(cfg.llm_model.as_str()))
            }
        };
        Ok(Self {
            runtime,
            model,
            preamble: DEFAULT_PREAMBLE.to_string(),
        })
    }
}

impl LlmBackend for RigBackend {
    fn complete(&self, prompt: &str) -> Result<String> {
        self.runtime.block_on(async {
            let request = CompletionRequest::new(prompt).preamble(self.preamble.as_str());
            let response = match &self.model {
                // 추론(thinking) 모델은 끄지 않으면 생각에 토큰을 소진해 본문이 빈 채로 온다(Gemma 4 확인).
                RigModel::OpenAiCompatible(model) => {
                    model.call(request.reasoning(Reasoning::Off)).await
                }
                // 추론을 끌 수 없는 Claude 모델이 있어 추론 설정은 API 기본값에 맡긴다.
                RigModel::Anthropic(model) => {
                    model.call(request.max_tokens(ANTHROPIC_MAX_TOKENS)).await
                }
            };
            let response = response.map_err(|e| anyhow!("rig 호출 실패: {e}"))?;
            Ok(response.text())
        })
    }
}

/// 설정의 백엔드가 제공하는 모델 ID 목록을 이름순으로 돌려준다.
///
/// OpenAI 호환 서버는 `GET {base_url}/models`(Ollama는 받아 둔 모델), Claude API는 Models API를 쓴다.
pub fn list_models(cfg: &Config) -> Result<Vec<String>> {
    let runtime =
        tokio::runtime::Runtime::new().map_err(|e| anyhow!("tokio 런타임 생성 실패: {e}"))?;
    let list = match cfg.llm_provider {
        Provider::OpenAiCompatible => runtime.block_on(openai_client(cfg).list_models()),
        Provider::Anthropic => runtime.block_on(anthropic_client(cfg)?.list_models()),
    }
    .map_err(|e| anyhow!("모델 목록 조회 실패: {e}"))?;
    let mut ids: Vec<String> = list.data.into_iter().map(|m| m.id).collect();
    ids.sort();
    ids.dedup();
    Ok(ids)
}

/// OpenAI 호환 클라이언트. 키가 비어 있어도 된다(로컬 서버).
fn openai_client(cfg: &Config) -> OpenAI {
    let mut provider = OpenAIConfig::new(cfg.llm_api_key.as_str());
    if !cfg.llm_base_url.is_empty() {
        provider = provider.with_base_url(cfg.llm_base_url.as_str());
    }
    provider.client()
}

/// Claude API 클라이언트. 키가 없으면 오류.
fn anthropic_client(cfg: &Config) -> Result<Anthropic> {
    if cfg.llm_api_key.is_empty() {
        bail!("Claude API 키가 없음: EAT_HWP_LLM_API_KEY 또는 ANTHROPIC_API_KEY를 설정하세요");
    }
    let mut provider = AnthropicConfig::new(cfg.llm_api_key.as_str());
    if !cfg.llm_base_url.is_empty() {
        provider = provider.with_base_url(cfg.llm_base_url.as_str());
    }
    Ok(provider.client())
}

/// rig 연동 없이 파이프라인을 검증하기 위한 임시 백엔드(테스트/폴백용).
#[allow(dead_code)]
pub struct EchoBackend;

impl LlmBackend for EchoBackend {
    fn complete(&self, prompt: &str) -> Result<String> {
        Ok(format!("[echo backend — rig 미연동]\n{prompt}"))
    }
}
