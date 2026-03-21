pub mod anthropic;
pub mod gemini;
pub mod openai;

use super::messages::{Message, ToolCall};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum StreamEvent {
    TextDelta(String),
    ToolCallStart { id: String, name: String },
    ToolCallArgumentsDelta(String),
    ToolCallEnd,
    Done,
    Error(String),
}

pub enum Provider {
    OpenAI(openai::OpenAiProvider),
    Anthropic(anthropic::AnthropicProvider),
    Gemini(gemini::GeminiProvider),
}

impl Provider {
    pub async fn chat(
        &self,
        messages: &[Message],
        tools: &[serde_json::Value],
        stream_tx: Option<mpsc::UnboundedSender<StreamEvent>>,
    ) -> anyhow::Result<(String, Vec<ToolCall>)> {
        match self {
            Provider::OpenAI(p) => p.chat(messages, tools, stream_tx).await,
            Provider::Anthropic(p) => p.chat(messages, tools, stream_tx).await,
            Provider::Gemini(p) => p.chat(messages, tools, stream_tx).await,
        }
    }
}
