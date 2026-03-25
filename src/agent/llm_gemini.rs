use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::gemini::{
    FunctionCall, FunctionDeclaration, FunctionResponse, GeminiClient, GeminiMessage, GeminiPart,
};

use super::llm::*;

pub struct GeminiLlm {
    client: GeminiClient,
}

impl GeminiLlm {
    pub fn new(api_key: &str) -> Self {
        Self {
            client: GeminiClient::new(api_key),
        }
    }
}

// ── Message conversion ──────────────────────────────────────────────────────

fn split_messages(messages: &[LlmMessage]) -> (Option<String>, Vec<GeminiMessage>) {
    let mut system = None;
    let mut result: Vec<GeminiMessage> = Vec::new();

    for msg in messages {
        match msg {
            LlmMessage::System(text) => {
                system = Some(text.clone());
            }
            LlmMessage::User(text) => {
                result.push(GeminiMessage {
                    role: "user".into(),
                    parts: vec![GeminiPart::text(text)],
                });
            }
            LlmMessage::Assistant { text, tool_calls } => {
                let mut parts = Vec::new();
                if !text.is_empty() {
                    parts.push(GeminiPart::text(text));
                }
                for tc in tool_calls {
                    parts.push(GeminiPart::FunctionCall {
                        function_call: FunctionCall {
                            name: tc.name.clone(),
                            args: tc.args.clone(),
                            id: Some(tc.id.clone()),
                        },
                        thought_signature: tc.thought_signature.clone(),
                    });
                }
                if parts.is_empty() {
                    parts.push(GeminiPart::text(""));
                }
                result.push(GeminiMessage {
                    role: "model".into(),
                    parts,
                });
            }
            LlmMessage::ToolResult {
                tool_call_id,
                name,
                content,
            } => {
                let part = GeminiPart::FunctionResponse {
                    function_response: FunctionResponse {
                        name: name.clone(),
                        response: json!({"result": content}),
                        id: Some(tool_call_id.clone()),
                    },
                };
                // Gemini groups adjacent tool results into a single "user" message
                let can_append = result.last().map_or(false, |m| {
                    m.role == "user"
                        && m.parts
                            .iter()
                            .all(|p| matches!(p, GeminiPart::FunctionResponse { .. }))
                });
                if can_append {
                    result.last_mut().unwrap().parts.push(part);
                } else {
                    result.push(GeminiMessage {
                        role: "user".into(),
                        parts: vec![part],
                    });
                }
            }
        }
    }

    (system, result)
}

fn convert_tools(tools: &[ToolDef]) -> Vec<FunctionDeclaration> {
    tools
        .iter()
        .map(|t| FunctionDeclaration {
            name: t.name.clone(),
            description: t.description.clone(),
            parameters: t.parameters.clone(),
        })
        .collect()
}

fn convert_response(resp: crate::gemini::GeminiResponse) -> LlmResponse {
    let mut text = String::new();
    let mut tool_calls = Vec::new();

    for part in resp.parts {
        match part {
            GeminiPart::Text { text: t, .. } => text.push_str(&t),
            GeminiPart::FunctionCall { function_call: fc, thought_signature: ts } => {
                tool_calls.push(ToolCall {
                    id: fc.id.unwrap_or_default(),
                    name: fc.name,
                    args: fc.args,
                    thought_signature: ts,
                });
            }
            _ => {}
        }
    }

    LlmResponse {
        text,
        tool_calls,
        usage: resp.usage.map(|u| TokenUsage {
            input_tokens: u.prompt_token_count,
            output_tokens: u.candidates_token_count,
            cached_tokens: u.cached_content_token_count,
        }),
        rate_limits: None,
    }
}

// ── Trait impl ──────────────────────────────────────────────────────────────

#[async_trait]
impl LlmClient for GeminiLlm {
    async fn generate(
        &self,
        model: &str,
        messages: &[LlmMessage],
        tools: &[ToolDef],
    ) -> Result<LlmResponse, String> {
        let (system, gemini_msgs) = split_messages(messages);
        let gemini_tools = convert_tools(tools);
        let resp = self
            .client
            .generate(model, &gemini_msgs, system.as_deref(), &gemini_tools)
            .await?;
        Ok(convert_response(resp))
    }

    async fn generate_stream(
        &self,
        model: &str,
        messages: &[LlmMessage],
        tools: &[ToolDef],
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<(), String> {
        let (system, gemini_msgs) = split_messages(messages);
        let gemini_tools = convert_tools(tools);

        let (inner_tx, mut inner_rx) = mpsc::channel::<crate::gemini::StreamEvent>(32);

        let client = self.client.clone();
        let model = model.to_string();
        let handle = tokio::spawn(async move {
            let _ = client
                .generate_stream(&model, &gemini_msgs, system.as_deref(), &gemini_tools, inner_tx)
                .await;
        });

        while let Some(event) = inner_rx.recv().await {
            match event {
                crate::gemini::StreamEvent::TextDelta(delta) => {
                    let _ = tx.send(StreamEvent::TextDelta(delta)).await;
                }
                crate::gemini::StreamEvent::FunctionCall(fc, ts) => {
                    let _ = tx
                        .send(StreamEvent::ToolCall(ToolCall {
                            id: fc.id.unwrap_or_default(),
                            name: fc.name,
                            args: fc.args,
                            thought_signature: ts,
                        }))
                        .await;
                }
                crate::gemini::StreamEvent::Done(usage) => {
                    if let Some(u) = usage {
                        let _ = tx
                            .send(StreamEvent::Usage(TokenUsage {
                                input_tokens: u.prompt_token_count,
                                output_tokens: u.candidates_token_count,
                                cached_tokens: u.cached_content_token_count,
                            }))
                            .await;
                    }
                    let _ = tx.send(StreamEvent::Done).await;
                }
                crate::gemini::StreamEvent::Error(e) => {
                    let _ = tx.send(StreamEvent::Error(e)).await;
                }
            }
        }

        let _ = handle.await;
        Ok(())
    }

    async fn generate_structured(
        &self,
        model: &str,
        messages: &[LlmMessage],
        schema: &Value,
    ) -> Result<String, String> {
        let (system, gemini_msgs) = split_messages(messages);
        self.client
            .generate_structured(model, &gemini_msgs, system.as_deref(), schema)
            .await
    }

    async fn test_connection(&self) -> Result<String, String> {
        self.client.test_connection().await
    }

    fn provider_name(&self) -> &str {
        "gemini"
    }
}
