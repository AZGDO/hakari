use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

// ── Unified types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub args: Value,
    /// Gemini thought signature — must be echoed back for tool calls to work.
    pub thought_signature: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

pub fn build_tool_def(
    name: &str,
    description: &str,
    properties: Value,
    required: Vec<&str>,
) -> ToolDef {
    ToolDef {
        name: name.to_string(),
        description: description.to_string(),
        parameters: json!({
            "type": "object",
            "properties": properties,
            "required": required,
        }),
    }
}

#[derive(Debug, Clone)]
pub enum LlmMessage {
    System(String),
    User(String),
    Assistant {
        text: String,
        tool_calls: Vec<ToolCall>,
    },
    ToolResult {
        tool_call_id: String,
        name: String,
        content: String,
    },
}

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
}

#[derive(Debug, Clone)]
pub struct RateLimits {
    pub total: u64,
    pub remaining: u64,
    pub reset_at: u64,
}

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<TokenUsage>,
    pub rate_limits: Option<RateLimits>,
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    TextDelta(String),
    ToolCall(ToolCall),
    Usage(TokenUsage),
    Error(String),
    Done,
}

// ── Trait ────────────────────────────────────────────────────────────────────

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn generate(
        &self,
        model: &str,
        messages: &[LlmMessage],
        tools: &[ToolDef],
    ) -> Result<LlmResponse, String>;

    async fn generate_stream(
        &self,
        model: &str,
        messages: &[LlmMessage],
        tools: &[ToolDef],
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<(), String>;

    async fn generate_structured(
        &self,
        model: &str,
        messages: &[LlmMessage],
        schema: &Value,
    ) -> Result<String, String>;

    async fn test_connection(&self) -> Result<String, String>;

    fn provider_name(&self) -> &str;
}
