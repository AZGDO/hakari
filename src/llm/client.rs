use super::messages::{Message, ToolCall};
use super::providers::{Provider, StreamEvent};
use super::providers::openai::OpenAiProvider;
use super::providers::anthropic::AnthropicProvider;
use crate::config::{HakariConfig, LlmProvider as LlmProviderConfig};
use tokio::sync::mpsc;

pub struct LlmClient {
    nano_provider: Provider,
    shizuka_provider: Provider,
}

impl LlmClient {
    pub fn new(config: &HakariConfig) -> anyhow::Result<Self> {
        let nano_provider = match config.nano_provider {
            LlmProviderConfig::OpenAI => {
                let api_key = config.openai_api_key.clone()
                    .ok_or_else(|| anyhow::anyhow!("OPENAI_API_KEY not set for Nano provider"))?;
                Provider::OpenAI(OpenAiProvider::new(api_key, config.openai_base_url.clone(), config.nano_model.clone()))
            }
            LlmProviderConfig::Anthropic => {
                let api_key = config.anthropic_api_key.clone()
                    .ok_or_else(|| anyhow::anyhow!("ANTHROPIC_API_KEY not set for Nano provider"))?;
                Provider::Anthropic(AnthropicProvider::new(api_key, config.anthropic_base_url.clone(), config.nano_model.clone()))
            }
        };

        let shizuka_provider = match config.shizuka_provider {
            LlmProviderConfig::OpenAI => {
                let api_key = config.openai_api_key.clone()
                    .ok_or_else(|| anyhow::anyhow!("OPENAI_API_KEY not set for Shizuka provider"))?;
                Provider::OpenAI(OpenAiProvider::new(api_key, config.openai_base_url.clone(), config.shizuka_model.clone()))
            }
            LlmProviderConfig::Anthropic => {
                let api_key = config.anthropic_api_key.clone()
                    .ok_or_else(|| anyhow::anyhow!("ANTHROPIC_API_KEY not set for Shizuka provider"))?;
                Provider::Anthropic(AnthropicProvider::new(api_key, config.anthropic_base_url.clone(), config.shizuka_model.clone()))
            }
        };

        Ok(Self { nano_provider, shizuka_provider })
    }

    pub async fn nano_chat(
        &self,
        messages: &[Message],
        tools: &[serde_json::Value],
        stream_tx: Option<mpsc::UnboundedSender<StreamEvent>>,
    ) -> anyhow::Result<(String, Vec<ToolCall>)> {
        self.nano_provider.chat(messages, tools, stream_tx).await
    }

    pub async fn shizuka_chat(
        &self,
        messages: &[Message],
    ) -> anyhow::Result<(String, Vec<ToolCall>)> {
        self.shizuka_provider.chat(messages, &[], None).await
    }
}
