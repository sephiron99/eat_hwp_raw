//! 설정: LLM 백엔드 엔드포인트 등 런타임 구성.
//!
//! features(eat_hwp_raw.md): "LLM Local or remote Server(nvidia spark dgx 등)를 지원".
//! 따라서 OpenAI 호환 base URL을 기본으로 하여 로컬(Ollama/vLLM 등) 및 원격 서버를 모두 수용한다.
//! Claude/Gemini API는 추후 지원(설계만 반영).

/// 환경 변수에서 읽어들이는 런타임 설정.
#[derive(Clone, Debug)]
pub struct Config {
    /// OpenAI 호환 API base URL (로컬/원격 추론 서버).
    pub llm_base_url: String,
    /// 사용할 모델 이름.
    pub llm_model: String,
    /// API 키 (로컬 서버는 비어 있을 수 있음).
    pub llm_api_key: String,
}

impl Config {
    /// 환경 변수에서 설정을 로드한다. 미설정 시 로컬 기본값을 사용한다.
    pub fn load() -> Self {
        Self {
            llm_base_url: env_or("EAT_HWP_LLM_BASE_URL", "http://localhost:11434/v1"),
            // 기본 모델: 실행환경(Windows, Intel 내장 그래픽, RAM 16GiB) Ollama의 Gemma 4 E4B QAT.
            // `ollama list` 태그와 정확히 일치해야 함.
            llm_model: env_or("EAT_HWP_LLM_MODEL", "gemma4:e4b-it-qat"),
            llm_api_key: std::env::var("EAT_HWP_LLM_API_KEY").unwrap_or_default(),
        }
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}
