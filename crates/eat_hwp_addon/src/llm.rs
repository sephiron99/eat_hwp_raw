//! LLM 백엔드 추상화 및 구현.
//!
//! 백엔드 정본은 rig-rs(CLAUDE.md Dependencies)다. OpenAI 호환 로컬/원격 엔드포인트와
//! Claude API를 지원한다. 트레이트로 추상화해 두어 백엔드를 동일 인터페이스로 교체한다.
//! Gemini는 추후 지원(설계만 반영).

use crate::config::{Config, Provider};
use anyhow::{Result, anyhow, bail};
use rig::completion::{CompletionRequest, Reasoning};
use rig::driver::Model;
use rig::error::ProviderError;
use rig::providers::anthropic::{Anthropic, AnthropicConfig};
use rig::providers::anthropic::wire::Messages;
use rig::providers::openai::{OpenAI, OpenAIConfig};
use rig::providers::openai::wire::Chat;

/// Claude API 응답의 최대 출력 토큰. Messages API는 이 값을 반드시 요구한다.
const ANTHROPIC_MAX_TOKENS: u64 = 16_000;

/// 시스템 메시지와 사용자 메시지를 받아 응답 텍스트를 생성하는 LLM 백엔드.
///
/// 시스템 메시지(지시)는 `workflow`가 요청마다 만든다.
pub trait LlmBackend {
    fn complete(&self, system: &str, user: &str) -> Result<String>;
}

/// rig-rs 기반 백엔드. OpenAI 호환 엔드포인트(로컬 Ollama/vLLM, 원격 DGX 등) 또는 Claude API로
/// 프롬프트를 실행한다.
///
/// rig API는 async이므로 내부에 tokio 런타임을 두고 `block_on`으로 동기 인터페이스에 맞춘다.
pub struct RigBackend {
    runtime: tokio::runtime::Runtime,
    model: RigModel,
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
        })
    }
}

impl LlmBackend for RigBackend {
    fn complete(&self, system: &str, user: &str) -> Result<String> {
        self.runtime.block_on(async {
            // rig는 preamble을 시스템 메시지로 보낸다(OpenAI 호환: 첫 system 메시지, Claude: system 필드).
            let request = CompletionRequest::new(user).preamble(system);
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
            let response = response.map_err(|e| {
                tracing::debug!(error = ?e, "rig 오류 상세");
                anyhow!("rig 호출 실패: {e}")
            })?;
            Ok(response.text())
        })
    }
}

/// LLM 서버 점검 결과.
pub struct ServerStatus {
    /// 점검한 대상. 예: `로컬 LLM 서버 http://127.0.0.1:11434/v1`, `Claude API`
    pub target: String,
    pub state: ServerState,
    /// 응답(또는 실패)까지 걸린 시간(밀리초).
    pub elapsed_ms: u128,
}

/// 점검한 서버의 상태.
pub enum ServerState {
    /// 응답했다. 서버가 제공하는 모델 ID 목록(이름순).
    Working(Vec<String>),
    /// Claude API 키가 없다.
    NoKey,
    /// 서버에 닿지 못했다(꺼져 있거나 주소·네트워크 문제).
    Unreachable(String),
    /// 서버가 키를 거부했다(HTTP 401/403).
    AuthFailed(String),
    /// 그 밖의 실패.
    Failed(String),
}

impl ServerState {
    /// 로그용 한 줄 요약.
    pub fn summary(&self) -> String {
        match self {
            ServerState::Working(models) => format!("작동 중(모델 {}개)", models.len()),
            ServerState::NoKey => "키 없음".to_string(),
            ServerState::Unreachable(e) => format!("연결 안 됨: {e}"),
            ServerState::AuthFailed(e) => format!("인증 실패: {e}"),
            ServerState::Failed(e) => format!("오류: {e}"),
        }
    }
}

/// 설정의 백엔드에 모델 목록을 요청해 서버 상태를 점검한다.
///
/// OpenAI 호환 서버는 `GET {base_url}/models`(Ollama는 받아 둔 모델), Claude API는 Models API를
/// 쓴다. 목록을 받으면 서버가 작동하고 키도 통과한 것으로 본다.
pub fn check_server(cfg: &Config) -> ServerStatus {
    let target = describe_target(cfg);
    let started = std::time::Instant::now();
    let state = match fetch_models(cfg) {
        Ok(models) => ServerState::Working(models),
        Err(state) => state,
    };
    ServerStatus {
        target,
        state,
        elapsed_ms: started.elapsed().as_millis(),
    }
}

fn fetch_models(cfg: &Config) -> std::result::Result<Vec<String>, ServerState> {
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| ServerState::Failed(format!("tokio 런타임 생성 실패: {e}")))?;
    let list = match cfg.llm_provider {
        Provider::OpenAiCompatible => runtime.block_on(openai_client(cfg).list_models()),
        Provider::Anthropic => {
            let client = anthropic_client(cfg).map_err(|_| ServerState::NoKey)?;
            runtime.block_on(client.list_models())
        }
    }
    .map_err(|e| classify(&e))?;
    let mut ids: Vec<String> = list.data.into_iter().map(|m| m.id).collect();
    ids.sort();
    ids.dedup();
    Ok(ids)
}

/// rig 오류를 서버 상태로 나눈다.
fn classify(error: &ProviderError) -> ServerState {
    let text = shorten(&error.to_string());
    match error {
        ProviderError::Http(_) => ServerState::Unreachable(text),
        ProviderError::InvalidAuthentication(_) => ServerState::AuthFailed(text),
        ProviderError::ProviderResponse(response) => match response.status.map(|s| s.as_u16()) {
            Some(401 | 403) => ServerState::AuthFailed(text),
            _ => ServerState::Failed(text),
        },
        _ => ServerState::Failed(text),
    }
}

/// 상태 표시줄에 넣을 수 있게 오류 문구를 줄인다.
fn shorten(text: &str) -> String {
    const MAX: usize = 120;
    match text.char_indices().nth(MAX) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_string(),
    }
}

/// 설정이 가리키는 LLM 서버를 사람이 읽을 수 있게 적는다.
pub fn describe_target(cfg: &Config) -> String {
    match cfg.llm_provider {
        Provider::OpenAiCompatible => {
            let url = cfg.llm_base_url.as_str();
            let local = ["://localhost", "://127.", "://[::1]"]
                .iter()
                .any(|host| url.contains(host));
            if local {
                format!("로컬 LLM 서버 {url}")
            } else {
                format!("원격 LLM 서버(OpenAI 호환) {url}")
            }
        }
        Provider::Anthropic if cfg.llm_base_url.is_empty() => "Claude API".to_string(),
        Provider::Anthropic => format!("Claude API {}", cfg.llm_base_url),
    }
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
    fn complete(&self, system: &str, user: &str) -> Result<String> {
        Ok(format!("[echo backend — rig 미연동]\n{system}\n\n{user}"))
    }
}
