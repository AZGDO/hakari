use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const GITHUB_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
const GITHUB_DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const COPILOT_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";
const COPILOT_CHAT_URL: &str = "https://api.githubcopilot.com/chat/completions";
const EDITOR_NAME: &str = "Hakari";
const EDITOR_VERSION: &str = "0.1.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokenResponse {
    pub access_token: Option<String>,
    pub token_type: Option<String>,
    pub scope: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotToken {
    pub token: String,
    pub expires_at: u64,
    #[serde(default)]
    pub endpoints: CopilotEndpoints,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CopilotEndpoints {
    #[serde(default)]
    pub api: String,
}

#[derive(Debug, Clone)]
pub struct CopilotRateLimits {
    pub total: u64,
    pub remaining: u64,
    pub reset_at: u64,
}

impl CopilotRateLimits {
    pub fn usage_percent(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        ((self.total - self.remaining) as f64 / self.total as f64) * 100.0
    }

    pub fn remaining_percent(&self) -> f64 {
        100.0 - self.usage_percent()
    }
}

#[derive(Debug, Clone)]
pub struct CopilotUsage {
    pub requests_used_this_prompt: u32,
    pub total_requests_used: u64,
    pub rate_limits: Option<CopilotRateLimits>,
}

impl Default for CopilotUsage {
    fn default() -> Self {
        Self {
            requests_used_this_prompt: 0,
            total_requests_used: 0,
            rate_limits: None,
        }
    }
}

#[allow(dead_code)]
impl CopilotUsage {
    pub fn remaining_percent(&self) -> f64 {
        self.rate_limits
            .as_ref()
            .map(|r| r.remaining_percent())
            .unwrap_or(100.0)
    }

    pub fn reset_prompt_counter(&mut self) {
        self.requests_used_this_prompt = 0;
    }
}

#[derive(Debug, Clone)]
pub struct CopilotClient {
    client: Client,
    oauth_token: String,
    copilot_token: Arc<Mutex<Option<CopilotToken>>>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotChoice {
    pub message: CopilotChoiceMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotChoiceMessage {
    pub role: String,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<CopilotToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: CopilotFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotChatResponse {
    pub choices: Vec<CopilotChoice>,
    pub usage: Option<CopilotTokenUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotTokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct CopilotStreamEvent {
    pub delta_content: Option<String>,
    pub delta_tool_calls: Vec<CopilotToolCallDelta>,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotToolCallDelta {
    pub index: usize,
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub call_type: Option<String>,
    pub function: Option<CopilotFunctionDelta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotFunctionDelta {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

pub struct CopilotModelInfo {
    pub id: &'static str,
    pub display_name: &'static str,
    pub rate_multiplier: f32,
    pub context_window: &'static str,
    pub description: &'static str,
}

pub fn copilot_models() -> Vec<CopilotModelInfo> {
    vec![
        // Included models (0x on paid plans)
        CopilotModelInfo {
            id: "gpt-5-mini",
            display_name: "GPT-5 mini",
            rate_multiplier: 0.0,
            context_window: "128K",
            description: "Included, no premium cost (0x)",
        },
        CopilotModelInfo {
            id: "gpt-4.1",
            display_name: "GPT-4.1",
            rate_multiplier: 0.0,
            context_window: "1M",
            description: "Included, no premium cost (0x)",
        },
        CopilotModelInfo {
            id: "gpt-4o",
            display_name: "GPT-4o",
            rate_multiplier: 0.0,
            context_window: "128K",
            description: "Included, no premium cost (0x)",
        },
        // Low-cost models
        CopilotModelInfo {
            id: "grok-code-fast-1",
            display_name: "Grok Code Fast 1",
            rate_multiplier: 0.25,
            context_window: "128K",
            description: "xAI fast coding (0.25x)",
        },
        CopilotModelInfo {
            id: "claude-haiku-4.5",
            display_name: "Claude Haiku 4.5",
            rate_multiplier: 0.33,
            context_window: "200K",
            description: "Anthropic fast (0.33x)",
        },
        CopilotModelInfo {
            id: "gemini-3-flash",
            display_name: "Gemini 3 Flash",
            rate_multiplier: 0.33,
            context_window: "1M",
            description: "Google fast (0.33x)",
        },
        CopilotModelInfo {
            id: "gpt-5.1-codex-mini",
            display_name: "GPT-5.1 Codex Mini",
            rate_multiplier: 0.33,
            context_window: "200K",
            description: "OpenAI small codex (0.33x)",
        },
        CopilotModelInfo {
            id: "gpt-5.4-mini",
            display_name: "GPT-5.4 mini",
            rate_multiplier: 0.33,
            context_window: "200K",
            description: "Latest small GPT (0.33x)",
        },
        // Standard models (1x)
        CopilotModelInfo {
            id: "gpt-5.1",
            display_name: "GPT-5.1",
            rate_multiplier: 1.0,
            context_window: "200K",
            description: "OpenAI GPT-5.1 (1x)",
        },
        CopilotModelInfo {
            id: "gpt-5.1-codex",
            display_name: "GPT-5.1 Codex",
            rate_multiplier: 1.0,
            context_window: "200K",
            description: "Code-optimized (1x)",
        },
        CopilotModelInfo {
            id: "gpt-5.1-codex-max",
            display_name: "GPT-5.1 Codex Max",
            rate_multiplier: 1.0,
            context_window: "200K",
            description: "Max codex variant (1x)",
        },
        CopilotModelInfo {
            id: "gpt-5.2",
            display_name: "GPT-5.2",
            rate_multiplier: 1.0,
            context_window: "200K",
            description: "OpenAI GPT-5.2 (1x)",
        },
        CopilotModelInfo {
            id: "gpt-5.2-codex",
            display_name: "GPT-5.2 Codex",
            rate_multiplier: 1.0,
            context_window: "200K",
            description: "Code-optimized 5.2 (1x)",
        },
        CopilotModelInfo {
            id: "gpt-5.3-codex",
            display_name: "GPT-5.3 Codex",
            rate_multiplier: 1.0,
            context_window: "200K",
            description: "Code-optimized 5.3 (1x)",
        },
        CopilotModelInfo {
            id: "gpt-5.4",
            display_name: "GPT-5.4",
            rate_multiplier: 1.0,
            context_window: "200K",
            description: "Latest GPT (1x)",
        },
        CopilotModelInfo {
            id: "claude-sonnet-4",
            display_name: "Claude Sonnet 4",
            rate_multiplier: 1.0,
            context_window: "200K",
            description: "Anthropic balanced (1x)",
        },
        CopilotModelInfo {
            id: "claude-sonnet-4.5",
            display_name: "Claude Sonnet 4.5",
            rate_multiplier: 1.0,
            context_window: "200K",
            description: "Anthropic latest (1x)",
        },
        CopilotModelInfo {
            id: "claude-sonnet-4.6",
            display_name: "Claude Sonnet 4.6",
            rate_multiplier: 1.0,
            context_window: "200K",
            description: "Anthropic newest (1x)",
        },
        CopilotModelInfo {
            id: "gemini-2.5-pro",
            display_name: "Gemini 2.5 Pro",
            rate_multiplier: 1.0,
            context_window: "1M",
            description: "Google 2.5 (1x)",
        },
        CopilotModelInfo {
            id: "gemini-3-pro",
            display_name: "Gemini 3 Pro",
            rate_multiplier: 1.0,
            context_window: "1M",
            description: "Google 3 Pro (1x)",
        },
        CopilotModelInfo {
            id: "gemini-3.1-pro",
            display_name: "Gemini 3.1 Pro",
            rate_multiplier: 1.0,
            context_window: "1M",
            description: "Google latest (1x)",
        },
        // Expensive models
        CopilotModelInfo {
            id: "claude-opus-4.5",
            display_name: "Claude Opus 4.5",
            rate_multiplier: 3.0,
            context_window: "200K",
            description: "Anthropic most capable (3x)",
        },
        CopilotModelInfo {
            id: "claude-opus-4.6",
            display_name: "Claude Opus 4.6",
            rate_multiplier: 3.0,
            context_window: "200K",
            description: "Anthropic latest opus (3x)",
        },
    ]
}

#[allow(dead_code)]
pub fn copilot_rate_multiplier(model_id: &str) -> f32 {
    copilot_models()
        .iter()
        .find(|m| m.id == model_id)
        .map(|m| m.rate_multiplier)
        .unwrap_or(1.0)
}

pub fn copilot_default_shizuka_model() -> &'static str {
    "gpt-5-mini"
}

fn copilot_token_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".hakari")
        .join("copilot_oauth_token")
}

pub fn save_oauth_token(token: &str) -> std::io::Result<()> {
    let path = copilot_token_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, token)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[allow(dead_code)]
pub fn load_oauth_token() -> Option<String> {
    let path = copilot_token_path();
    std::fs::read_to_string(&path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

impl CopilotClient {
    pub fn new(oauth_token: &str) -> Self {
        Self {
            client: Client::new(),
            oauth_token: oauth_token.to_string(),
            copilot_token: Arc::new(Mutex::new(None)),
        }
    }

    async fn ensure_copilot_token(&self) -> Result<String, String> {
        {
            let guard = self.copilot_token.lock().unwrap();
            if let Some(ref token) = *guard {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                if token.expires_at > now + 60 {
                    return Ok(token.token.clone());
                }
            }
        }
        // Refresh the copilot token
        let resp = self
            .client
            .get(COPILOT_TOKEN_URL)
            .header("Authorization", format!("token {}", self.oauth_token))
            .header("User-Agent", format!("{}/{}", EDITOR_NAME, EDITOR_VERSION))
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| format!("Failed to get Copilot token: {}", e))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| format!("Read error: {}", e))?;

        if !status.is_success() {
            return Err(format!("Copilot token error ({}): {}", status, text));
        }

        let token: CopilotToken = serde_json::from_str(&text).map_err(|e| {
            format!(
                "Parse Copilot token error: {} body: {}",
                e,
                &text[..text.len().min(200)]
            )
        })?;

        let token_str = token.token.clone();
        {
            let mut guard = self.copilot_token.lock().unwrap();
            *guard = Some(token);
        }
        Ok(token_str)
    }

    pub async fn chat_completion_raw(
        &self,
        model: &str,
        messages: &[Value],
        system: Option<&str>,
        tools: Option<&Value>,
    ) -> Result<(CopilotChatResponse, Option<CopilotRateLimits>), String> {
        let token = self.ensure_copilot_token().await?;

        let mut all_messages = Vec::new();
        if let Some(sys) = system {
            all_messages.push(json!({"role": "system", "content": sys}));
        }
        for msg in messages {
            all_messages.push(msg.clone());
        }

        let mut body = json!({
            "model": model,
            "messages": all_messages,
            "stream": false,
        });

        if let Some(tools_val) = tools {
            body["tools"] = tools_val.clone();
        }

        let resp = self
            .client
            .post(COPILOT_CHAT_URL)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .header("User-Agent", format!("{}/{}", EDITOR_NAME, EDITOR_VERSION))
            .header("Copilot-Integration-Id", "vscode-chat")
            .header(
                "Editor-Version",
                format!("{}/{}", EDITOR_NAME, EDITOR_VERSION),
            )
            .header(
                "Editor-Plugin-Version",
                format!("hakari/{}", EDITOR_VERSION),
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Copilot HTTP error: {}", e))?;

        let rate_limits = extract_rate_limits(&resp);
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| format!("Read error: {}", e))?;

        if !status.is_success() {
            return Err(format!("Copilot API error ({}): {}", status, text));
        }

        let response: CopilotChatResponse = serde_json::from_str(&text)
            .map_err(|e| format!("Parse error: {} body: {}", e, &text[..text.len().min(300)]))?;

        Ok((response, rate_limits))
    }

    #[allow(dead_code)]
    pub async fn chat_completion(
        &self,
        model: &str,
        messages: &[CopilotMessage],
        system: Option<&str>,
        tools: Option<&Value>,
    ) -> Result<(CopilotChatResponse, Option<CopilotRateLimits>), String> {
        let token = self.ensure_copilot_token().await?;

        let mut all_messages = Vec::new();
        if let Some(sys) = system {
            all_messages.push(json!({"role": "system", "content": sys}));
        }
        for msg in messages {
            all_messages.push(json!({"role": msg.role, "content": msg.content}));
        }

        let mut body = json!({
            "model": model,
            "messages": all_messages,
            "stream": false,
        });

        if let Some(tools_val) = tools {
            body["tools"] = tools_val.clone();
        }

        let resp = self
            .client
            .post(COPILOT_CHAT_URL)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .header("User-Agent", format!("{}/{}", EDITOR_NAME, EDITOR_VERSION))
            .header("Copilot-Integration-Id", "vscode-chat")
            .header(
                "Editor-Version",
                format!("{}/{}", EDITOR_NAME, EDITOR_VERSION),
            )
            .header(
                "Editor-Plugin-Version",
                format!("hakari/{}", EDITOR_VERSION),
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Copilot HTTP error: {}", e))?;

        let rate_limits = extract_rate_limits(&resp);
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| format!("Read error: {}", e))?;

        if !status.is_success() {
            return Err(format!("Copilot API error ({}): {}", status, text));
        }

        let response: CopilotChatResponse = serde_json::from_str(&text)
            .map_err(|e| format!("Parse error: {} body: {}", e, &text[..text.len().min(300)]))?;

        Ok((response, rate_limits))
    }

    pub async fn chat_completion_stream(
        &self,
        model: &str,
        messages: &[Value],
        system: Option<&str>,
        tools: Option<&Value>,
        tx: tokio::sync::mpsc::Sender<CopilotStreamChunk>,
    ) -> Result<Option<CopilotRateLimits>, String> {
        let token = self.ensure_copilot_token().await?;

        let mut all_messages = Vec::new();
        if let Some(sys) = system {
            all_messages.push(json!({"role": "system", "content": sys}));
        }
        for msg in messages {
            all_messages.push(msg.clone());
        }

        let mut body = json!({
            "model": model,
            "messages": all_messages,
            "stream": true,
            "stream_options": {"include_usage": true},
        });

        if let Some(tools_val) = tools {
            body["tools"] = tools_val.clone();
        }

        let resp = self
            .client
            .post(COPILOT_CHAT_URL)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .header("User-Agent", format!("{}/{}", EDITOR_NAME, EDITOR_VERSION))
            .header("Copilot-Integration-Id", "vscode-chat")
            .header(
                "Editor-Version",
                format!("{}/{}", EDITOR_NAME, EDITOR_VERSION),
            )
            .header(
                "Editor-Plugin-Version",
                format!("hakari/{}", EDITOR_VERSION),
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Copilot HTTP error: {}", e))?;

        let rate_limits = extract_rate_limits(&resp);
        let status = resp.status();

        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            let _ = tx
                .send(CopilotStreamChunk::Error(format!(
                    "Copilot API error ({}): {}",
                    status, text
                )))
                .await;
            return Err(format!("Copilot API error ({})", status));
        }

        use futures::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx
                        .send(CopilotStreamChunk::Error(format!("Stream error: {}", e)))
                        .await;
                    break;
                }
            };

            buffer.push_str(&String::from_utf8_lossy(&chunk));
            if buffer.contains('\r') {
                buffer = buffer.replace("\r\n", "\n");
            }

            // SSE: each event is "data: ...\n\n" or just lines separated by \n
            while let Some(pos) = buffer.find("\n\n") {
                let block = buffer[..pos].to_string();
                buffer = buffer[pos + 2..].to_string();

                for line in block.split('\n') {
                    let data = if let Some(d) = line.strip_prefix("data: ") {
                        d.trim()
                    } else if let Some(d) = line.strip_prefix("data:") {
                        d.trim()
                    } else {
                        continue;
                    };

                    if data.is_empty() || data == "[DONE]" {
                        continue;
                    }

                    let parsed: Value = match serde_json::from_str(data) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    if let Some(choices) = parsed.get("choices").and_then(|c| c.as_array()) {
                        for choice in choices {
                            let delta = match choice.get("delta") {
                                Some(d) => d,
                                None => continue,
                            };

                            if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                if !content.is_empty() {
                                    let _ = tx
                                        .send(CopilotStreamChunk::TextDelta(content.to_string()))
                                        .await;
                                }
                            }

                            if let Some(tool_calls) =
                                delta.get("tool_calls").and_then(|t| t.as_array())
                            {
                                for tc in tool_calls {
                                    if let Ok(delta) =
                                        serde_json::from_value::<CopilotToolCallDelta>(tc.clone())
                                    {
                                        let _ =
                                            tx.send(CopilotStreamChunk::ToolCallDelta(delta)).await;
                                    }
                                }
                            }

                            if let Some(reason) =
                                choice.get("finish_reason").and_then(|r| r.as_str())
                            {
                                if reason == "tool_calls" {
                                    let _ = tx.send(CopilotStreamChunk::FinishToolCalls).await;
                                }
                            }
                        }
                    }

                    if let Some(usage) = parsed.get("usage") {
                        if let Ok(u) = serde_json::from_value::<CopilotTokenUsage>(usage.clone()) {
                            let _ = tx.send(CopilotStreamChunk::Usage(u)).await;
                        }
                    }
                }
            }
        }

        let _ = tx.send(CopilotStreamChunk::Done).await;
        Ok(rate_limits)
    }

    pub async fn test_connection(&self) -> Result<String, String> {
        let _token = self.ensure_copilot_token().await?;
        Ok("Connected to GitHub Copilot".into())
    }
}

pub enum CopilotStreamChunk {
    TextDelta(String),
    ToolCallDelta(CopilotToolCallDelta),
    FinishToolCalls,
    Usage(CopilotTokenUsage),
    Error(String),
    Done,
}

fn extract_rate_limits(resp: &reqwest::Response) -> Option<CopilotRateLimits> {
    let total = resp
        .headers()
        .get("x-ratelimit-limit")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())?;
    let remaining = resp
        .headers()
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())?;
    let reset_at = resp
        .headers()
        .get("x-ratelimit-reset")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    Some(CopilotRateLimits {
        total,
        remaining,
        reset_at,
    })
}

pub async fn start_device_flow(client: &Client) -> Result<DeviceCodeResponse, String> {
    let resp = client
        .post(GITHUB_DEVICE_CODE_URL)
        .header("Accept", "application/json")
        .form(&[("client_id", GITHUB_CLIENT_ID), ("scope", "read:user")])
        .send()
        .await
        .map_err(|e| format!("Device flow request failed: {}", e))?;

    let text = resp
        .text()
        .await
        .map_err(|e| format!("Read error: {}", e))?;
    serde_json::from_str(&text)
        .map_err(|e| format!("Parse error: {} body: {}", e, &text[..text.len().min(300)]))
}

pub async fn poll_for_token(
    client: &Client,
    device_code: &str,
    interval: u64,
) -> Result<String, String> {
    let wait = std::time::Duration::from_secs(interval.max(5));
    loop {
        tokio::time::sleep(wait).await;

        let resp = client
            .post(GITHUB_TOKEN_URL)
            .header("Accept", "application/json")
            .form(&[
                ("client_id", GITHUB_CLIENT_ID),
                ("device_code", device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await
            .map_err(|e| format!("Token poll failed: {}", e))?;

        let text = resp
            .text()
            .await
            .map_err(|e| format!("Read error: {}", e))?;
        let token_resp: OAuthTokenResponse =
            serde_json::from_str(&text).map_err(|e| format!("Parse error: {}", e))?;

        if let Some(token) = token_resp.access_token {
            return Ok(token);
        }

        match token_resp.error.as_deref() {
            Some("authorization_pending") => continue,
            Some("slow_down") => {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
            Some("expired_token") => return Err("Device code expired. Please try again.".into()),
            Some("access_denied") => return Err("Authorization denied by user.".into()),
            Some(err) => {
                return Err(format!(
                    "OAuth error: {} - {}",
                    err,
                    token_resp.error_description.unwrap_or_default()
                ))
            }
            None => return Err("Unknown OAuth response".into()),
        }
    }
}
