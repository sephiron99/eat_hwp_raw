//! MCP(Model Context Protocol) 클라이언트.
//!
//! 설정 창에 적은 MCP 서버에 붙어 도구 목록을 받고, LLM이 부른 도구를 대신 실행한다. 서버는 두 가지다.
//! - 로컬 프로그램(stdio): 프로그램을 자식 프로세스로 띄워 표준 입출력으로 주고받는다
//! - 네트워크 서버(Streamable HTTP): `http(s)://` 주소로 붙는다(같은 PC나 LAN의 서버)
//!
//! 연결은 요청마다 새로 맺고 요청이 끝나면 닫는다. 서버 설정 글의 형식은 [`parse_servers`] 참고.

use anyhow::{Result, anyhow};
use rig::completion::ToolDefinition;
use rig::message::ToolName;
use rmcp::model::{CallToolRequestParams, CallToolResult, Tool};
use rmcp::service::{RoleClient, RunningService, ServiceExt};
use rmcp::transport::{StreamableHttpClientTransport, TokioChildProcess};
use std::time::{Duration, Instant};

/// 서버에 붙어 도구 목록을 받기까지 기다리는 시간.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// 도구 한 번 실행을 기다리는 시간.
const CALL_TIMEOUT: Duration = Duration::from_secs(120);
/// LLM에 돌려줄 도구 결과의 최대 글자 수. 로컬 서버의 컨텍스트가 작아(실행환경 8K) 넘치지 않게 자른다.
const MAX_RESULT_CHARS: usize = 16_000;
/// LLM에 보이는 도구 이름의 최대 길이(OpenAI·Claude 공통 제한 `^[a-zA-Z0-9_-]{1,64}$`).
const MAX_TOOL_NAME: usize = 64;
/// 자식 프로세스의 콘솔 창을 띄우지 않는다(GUI 프로세스에는 콘솔이 없어 새 창이 깜박인다).
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 설정에 적은 MCP 서버 하나.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerSpec {
    /// 설정에 적은 이름. LLM에 보이는 도구 이름의 앞부분이 된다.
    pub name: String,
    pub endpoint: Endpoint,
}

/// MCP 서버에 붙는 방법.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Endpoint {
    /// 네트워크 서버(Streamable HTTP) 주소.
    Http(String),
    /// 로컬 프로그램(stdio). 프로그램 경로와 인자.
    Stdio { program: String, args: Vec<String> },
}

/// 서버 설정 글을 읽는다. 잘못된 줄은 `"N번째 줄: 이유"`로 모은다.
///
/// 한 줄에 서버 하나를 `이름 = 값`으로 적는다. 빈 줄과 `#`으로 시작하는 줄은 건너뛴다.
/// - 값이 `http://`나 `https://`로 시작하면 네트워크 서버 주소
/// - 그 밖의 값은 로컬 프로그램의 명령줄(앞의 `stdio:`는 있어도 된다). 공백이 든 경로는 `"`로 감싼다
pub fn parse_servers(text: &str) -> (Vec<ServerSpec>, Vec<String>) {
    let mut servers: Vec<ServerSpec> = Vec::new();
    let mut errors = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let n = i + 1;
        let Some((name, value)) = line.split_once('=') else {
            errors.push(format!("{n}번째 줄: `이름 = 주소 또는 명령` 형식이 아님"));
            continue;
        };
        let name = name.trim();
        let value = value.trim();
        if name.is_empty() {
            errors.push(format!("{n}번째 줄: 이름이 비어 있음"));
            continue;
        }
        if servers.iter().any(|s| s.name == name) {
            errors.push(format!("{n}번째 줄: 이름 `{name}`이(가) 이미 있음"));
            continue;
        }
        let lower = value.to_ascii_lowercase();
        let endpoint = if lower.starts_with("http://") || lower.starts_with("https://") {
            Endpoint::Http(value.to_string())
        } else {
            let command = value.strip_prefix("stdio:").unwrap_or(value).trim();
            let mut words = split_command(command).into_iter();
            match words.next() {
                Some(program) => Endpoint::Stdio {
                    program,
                    args: words.collect(),
                },
                None => {
                    errors.push(format!("{n}번째 줄: 주소나 명령이 비어 있음"));
                    continue;
                }
            }
        };
        servers.push(ServerSpec {
            name: name.to_string(),
            endpoint,
        });
    }
    (servers, errors)
}

/// 명령줄을 낱말로 나눈다. 공백으로 나누고, `"`로 감싼 부분은 공백이 있어도 한 낱말로 둔다.
/// 역슬래시는 그대로 둔다(Windows 경로).
fn split_command(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut has_word = false;
    for c in command.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                has_word = true;
            }
            c if c.is_whitespace() && !quoted => {
                if has_word {
                    words.push(std::mem::take(&mut word));
                    has_word = false;
                }
            }
            c => {
                word.push(c);
                has_word = true;
            }
        }
    }
    if has_word {
        words.push(word);
    }
    words
}

/// 서버 하나를 점검한 결과.
pub struct CheckResult {
    pub name: String,
    /// 받은 도구 이름 목록, 또는 실패 이유.
    pub outcome: std::result::Result<Vec<String>, String>,
    pub elapsed_ms: u128,
}

/// 서버마다 붙어 도구 목록을 받아 본다(설정 창의 "MCP 확인"). 스레드를 막는다.
pub fn check_servers(specs: &[ServerSpec]) -> Vec<CheckResult> {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(e) => {
            return specs
                .iter()
                .map(|s| CheckResult {
                    name: s.name.clone(),
                    outcome: Err(format!("tokio 런타임 생성 실패: {e}")),
                    elapsed_ms: 0,
                })
                .collect();
        }
    };
    runtime.block_on(async {
        let mut results = Vec::new();
        for spec in specs {
            let started = Instant::now();
            let outcome = match connect(spec).await {
                Ok((client, tools)) => {
                    let _ = client.cancel().await;
                    Ok(tools.into_iter().map(|t| t.name.to_string()).collect())
                }
                Err(e) => Err(format!("{e:#}")),
            };
            tracing::info!(
                server = %spec.name,
                endpoint = ?spec.endpoint,
                ok = outcome.is_ok(),
                elapsed_ms = started.elapsed().as_millis(),
                "MCP 서버 점검"
            );
            results.push(CheckResult {
                name: spec.name.clone(),
                outcome,
                elapsed_ms: started.elapsed().as_millis(),
            });
        }
        results
    })
}

/// 서버에 붙어 도구 목록을 받는다.
async fn connect(spec: &ServerSpec) -> Result<(RunningService<RoleClient, ()>, Vec<Tool>)> {
    let work = async {
        let client = match &spec.endpoint {
            Endpoint::Http(url) => {
                let transport = StreamableHttpClientTransport::from_uri(url.as_str());
                ().serve(transport)
                    .await
                    .map_err(|e| anyhow!("연결 실패({url}): {e}"))?
            }
            Endpoint::Stdio { program, args } => {
                let mut command = tokio::process::Command::new(program);
                command.args(args);
                #[cfg(windows)]
                command.creation_flags(CREATE_NO_WINDOW);
                let (transport, _) = TokioChildProcess::builder(command)
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .map_err(|e| anyhow!("프로그램 실행 실패({program}): {e}"))?;
                ().serve(transport)
                    .await
                    .map_err(|e| anyhow!("연결 실패({program}): {e}"))?
            }
        };
        let tools = client
            .list_all_tools()
            .await
            .map_err(|e| anyhow!("도구 목록을 받지 못함: {e}"))?;
        Ok((client, tools))
    };
    tokio::time::timeout(CONNECT_TIMEOUT, work)
        .await
        .map_err(|_| anyhow!("{}초 안에 응답이 없음", CONNECT_TIMEOUT.as_secs()))?
}

/// 요청 하나 동안 붙어 있는 MCP 서버들과, LLM에 보일 도구 목록.
pub struct Hub {
    clients: Vec<RunningService<RoleClient, ()>>,
    tools: Vec<ExposedTool>,
    /// 붙지 못한 서버 등, 대화 칸에 알릴 것.
    pub notes: Vec<String>,
}

/// LLM에 보이는 도구 하나.
struct ExposedTool {
    /// `clients`의 번호.
    client: usize,
    /// 서버가 쓰는 도구 이름.
    tool: String,
    definition: ToolDefinition,
}

impl Hub {
    /// 서버마다 붙어 도구 목록을 받는다. 붙지 못한 서버는 빼고 `notes`에 남긴다.
    pub async fn connect(specs: &[ServerSpec]) -> Hub {
        let mut hub = Hub {
            clients: Vec::new(),
            tools: Vec::new(),
            notes: Vec::new(),
        };
        for (i, spec) in specs.iter().enumerate() {
            let (client, tools) = match connect(spec).await {
                Ok(connected) => connected,
                Err(e) => {
                    tracing::warn!(server = %spec.name, error = %format!("{e:#}"), "MCP 서버에 붙지 못함");
                    hub.notes.push(format!("⚠ MCP {}: {e:#}", spec.name));
                    continue;
                }
            };
            let index = hub.clients.len();
            hub.clients.push(client);
            let prefix = match sanitize(&spec.name) {
                p if p.chars().all(|c| c == '_') => format!("mcp{}", i + 1),
                p => p,
            };
            for tool in tools {
                let exposed = hub.unique_name(&format!("{prefix}__{}", sanitize(&tool.name)));
                let description = tool
                    .description
                    .as_deref()
                    .or(tool.title.as_deref())
                    .unwrap_or_default()
                    .to_string();
                let parameters = serde_json::Value::Object((*tool.input_schema).clone());
                let Ok(name) = ToolName::new(exposed) else {
                    continue;
                };
                hub.tools.push(ExposedTool {
                    client: index,
                    tool: tool.name.to_string(),
                    definition: ToolDefinition::new(
                        name,
                        format!("[MCP {}] {description}", spec.name),
                        parameters,
                    ),
                });
            }
            tracing::info!(server = %spec.name, tools = hub.tools.len(), "MCP 서버에 붙음");
        }
        hub
    }

    /// 이미 쓴 이름과 겹치지 않고 길이 제한 안에 드는 이름.
    fn unique_name(&self, base: &str) -> String {
        let taken = |name: &str| self.tools.iter().any(|t| t.definition.name.as_str() == name);
        let base: String = base.chars().take(MAX_TOOL_NAME).collect();
        if !taken(&base) {
            return base;
        }
        (2..)
            .map(|n| {
                let suffix = format!("_{n}");
                let head: String = base.chars().take(MAX_TOOL_NAME - suffix.len()).collect();
                format!("{head}{suffix}")
            })
            .find(|name| !taken(name))
            .unwrap_or(base)
    }

    /// LLM에 보낼 도구 정의.
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.iter().map(|t| t.definition.clone()).collect()
    }

    /// 쓸 수 있는 도구가 없으면 `true`.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// LLM이 `name`으로 부른 도구를 실행해 결과 글을 돌려준다. 도구가 오류를 알렸으면 `Err`.
    pub async fn call(
        &self,
        name: &str,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> Result<String> {
        let tool = self
            .tools
            .iter()
            .find(|t| t.definition.name.as_str() == name)
            .ok_or_else(|| anyhow!("없는 도구: {name}"))?;
        let params = CallToolRequestParams::new(tool.tool.clone()).with_arguments(arguments);
        let result = tokio::time::timeout(CALL_TIMEOUT, self.clients[tool.client].call_tool(params))
            .await
            .map_err(|_| anyhow!("{}초 안에 끝나지 않음", CALL_TIMEOUT.as_secs()))?
            .map_err(|e| anyhow!("도구 실행 실패: {e}"))?;
        let is_error = result.is_error == Some(true);
        let text = result_text(result);
        if is_error { Err(anyhow!(text)) } else { Ok(text) }
    }

    /// 서버 연결을 닫는다(로컬 프로그램은 끝낸다).
    pub async fn close(self) {
        for client in self.clients {
            let _ = client.cancel().await;
        }
    }
}

/// 도구 이름에 쓸 수 있는 글자(영문·숫자·`_`·`-`)만 남기고 나머지는 `_`로 바꾼다.
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect()
}

/// 도구 결과를 LLM에 돌려줄 글로 바꾼다. 글이 아닌 내용(그림 등)은 생략했다고 적고, 너무 길면 자른다.
fn result_text(result: CallToolResult) -> String {
    let mut parts: Vec<String> = result
        .content
        .iter()
        .map(|block| match block.as_text() {
            Some(text) => text.text.clone(),
            None => "[글이 아닌 내용 생략]".to_string(),
        })
        .collect();
    if parts.is_empty()
        && let Some(value) = &result.structured_content
    {
        parts.push(value.to_string());
    }
    let text = parts.join("\n");
    if text.chars().count() <= MAX_RESULT_CHARS {
        return text;
    }
    let head: String = text.chars().take(MAX_RESULT_CHARS).collect();
    format!("{head}\n[결과가 길어 {MAX_RESULT_CHARS}자에서 자름]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_servers() {
        let text = "# 주석\n\
                    docs = http://192.168.0.10:8000/mcp\n\
                    hwp2md = stdio: \"C:\\Program Files\\hwp2md\\hwp2md-mcp.exe\" --root C:\\참고\n\
                    \n\
                    잘못된 줄\n\
                    docs = https://x\n";
        let (servers, errors) = parse_servers(text);
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].endpoint, Endpoint::Http("http://192.168.0.10:8000/mcp".into()));
        assert_eq!(
            servers[1].endpoint,
            Endpoint::Stdio {
                program: "C:\\Program Files\\hwp2md\\hwp2md-mcp.exe".into(),
                args: vec!["--root".into(), "C:\\참고".into()],
            }
        );
        assert_eq!(errors.len(), 2, "{errors:?}");
    }
}
