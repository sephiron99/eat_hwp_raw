//! 설정: LLM 백엔드 엔드포인트 등 런타임 구성.
//!
//! features(eat_hwp_raw.md): "LLM Local or remote Server(nvidia spark dgx 등)를 지원".
//! 따라서 OpenAI 호환 base URL을 기본으로 하여 로컬(Ollama/vLLM 등) 및 원격 서버를 모두 수용한다.
//! `EAT_HWP_LLM_PROVIDER=anthropic`이면 Claude API를 쓴다. Gemini API는 추후 지원(설계만 반영).

/// Claude API의 기본 모델.
const DEFAULT_CLAUDE_MODEL: &str = "claude-opus-5-5";

/// LLM 백엔드 종류.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    /// OpenAI 호환 엔드포인트(로컬 Ollama/vLLM, 원격 서버).
    OpenAiCompatible,
    /// Anthropic Claude API.
    Anthropic,
}

/// 환경 변수에서 읽어들이는 런타임 설정.
#[derive(Clone, Debug)]
pub struct Config {
    /// LLM 백엔드 종류.
    pub llm_provider: Provider,
    /// API base URL. 비어 있으면 백엔드의 기본 주소를 쓴다.
    pub llm_base_url: String,
    /// 사용할 모델 이름.
    pub llm_model: String,
    /// API 키 (로컬 서버는 비어 있을 수 있음).
    pub llm_api_key: String,
}

impl Config {
    /// 환경 변수에서 설정을 로드한다. 미설정 시 로컬 기본값을 사용한다.
    ///
    /// `EAT_HWP_LLM_PROVIDER`: `openai`(기본) 또는 `anthropic`. 그 밖의 값은 오류.
    pub fn load() -> Result<Self, String> {
        let provider = match env_or("EAT_HWP_LLM_PROVIDER", "openai")
            .to_ascii_lowercase()
            .as_str()
        {
            "openai" => Provider::OpenAiCompatible,
            "anthropic" | "claude" => Provider::Anthropic,
            other => {
                return Err(format!(
                    "EAT_HWP_LLM_PROVIDER 값을 알 수 없음: {other} (openai 또는 anthropic)"
                ));
            }
        };
        Ok(match provider {
            Provider::OpenAiCompatible => Self {
                llm_provider: provider,
                llm_base_url: env_or("EAT_HWP_LLM_BASE_URL", "http://localhost:11434/v1"),
                // 기본 모델: 실행환경(Windows, Intel 내장 그래픽, RAM 16GiB) Ollama의 Gemma 4 E4B QAT.
                // `ollama list` 태그와 정확히 일치해야 함.
                llm_model: env_or("EAT_HWP_LLM_MODEL", "gemma4:e4b-it-qat"),
                llm_api_key: std::env::var("EAT_HWP_LLM_API_KEY").unwrap_or_default(),
            },
            Provider::Anthropic => Self {
                llm_provider: provider,
                // 기본값은 비워 두어 Anthropic 공식 주소를 쓴다.
                llm_base_url: env_or("EAT_HWP_LLM_BASE_URL", ""),
                llm_model: env_or("EAT_HWP_LLM_MODEL", DEFAULT_CLAUDE_MODEL),
                // 전용 변수가 없으면 Anthropic 표준 변수를 쓴다.
                llm_api_key: std::env::var("EAT_HWP_LLM_API_KEY")
                    .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
                    .unwrap_or_default(),
            },
        })
    }

    /// GUI에 입력한 Claude API 키로 Claude API를 쓰는 설정을 만든다.
    ///
    /// 환경 변수 설정이 이미 Claude API이면 모델과 주소는 그대로 두고 키만 바꾼다. 로컬 서버
    /// 설정이면 그 모델·주소는 Claude에 맞지 않으므로 Claude 기본값을 쓴다.
    pub fn with_claude_key(&self, api_key: &str) -> Self {
        match self.llm_provider {
            Provider::Anthropic => Self {
                llm_api_key: api_key.to_string(),
                ..self.clone()
            },
            Provider::OpenAiCompatible => Self {
                llm_provider: Provider::Anthropic,
                llm_base_url: String::new(),
                llm_model: DEFAULT_CLAUDE_MODEL.to_string(),
                llm_api_key: api_key.to_string(),
            },
        }
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}
