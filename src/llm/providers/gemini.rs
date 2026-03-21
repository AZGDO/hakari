use super::StreamEvent;
use crate::llm::messages::{Message, Role, ToolCall};
use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::mpsc;

pub struct GeminiProvider {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
}

impl GeminiProvider {
    pub fn new(api_key: String, base_url: String, model: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url,
            model,
        }
    }

    /// Convert our internal message history to Gemini `contents` array.
    /// System messages are handled via `system_instruction`.
    fn convert_messages(&self, messages: &[Message]) -> (Option<String>, Vec<Value>) {
        let mut system_instruction: Option<String> = None;
        let mut contents: Vec<Value> = Vec::new();

        for msg in messages {
            match msg.role {
                Role::System => {
                    system_instruction = Some(msg.content.to_text_string());
                }
                Role::User => {
                    contents.push(json!({
                        "role": "user",
                        "parts": [{"text": msg.content.to_text_string()}]
                    }));
                }
                Role::Assistant => {
                    let mut parts: Vec<Value> = Vec::new();
                    let text = msg.content.to_text_string();
                    if !text.is_empty() {
                        parts.push(json!({"text": text}));
                    }
                    if let Some(tool_calls) = &msg.tool_calls {
                        for tc in tool_calls {
                            parts.push(json!({
                                "functionCall": {
                                    "name": tc.name,
                                    "args": tc.arguments,
                                }
                            }));
                        }
                    }
                    if parts.is_empty() {
                        parts.push(json!({"text": ""}));
                    }
                    contents.push(json!({"role": "model", "parts": parts}));
                }
                Role::Tool => {
                    // Tool results go as "function" role with functionResponse
                    let call_id = msg.tool_call_id.as_deref().unwrap_or("");
                    // Gemini expects functionResponse inside a user-role turn
                    contents.push(json!({
                        "role": "user",
                        "parts": [{
                            "functionResponse": {
                                "name": call_id,
                                "response": {
                                    "output": msg.content.to_text_string()
                                }
                            }
                        }]
                    }));
                }
            }
        }

        (system_instruction, contents)
    }

    /// Convert OpenAI-style tool schema to Gemini function declarations.
    fn convert_tools(&self, tools: &[Value]) -> Vec<Value> {
        tools
            .iter()
            .filter_map(|t| {
                let func = t.get("function")?;
                Some(json!({
                    "name": func["name"],
                    "description": func["description"],
                    "parameters": func["parameters"],
                }))
            })
            .collect()
    }

    pub async fn chat(
        &self,
        messages: &[Message],
        tools: &[Value],
        stream_tx: Option<mpsc::UnboundedSender<StreamEvent>>,
    ) -> anyhow::Result<(String, Vec<ToolCall>)> {
        let (system_instruction, contents) = self.convert_messages(messages);
        let function_declarations = self.convert_tools(tools);

        let mut body = json!({
            "contents": contents,
            "generationConfig": {
                "temperature": 1.0,
            }
        });

        if let Some(sys) = system_instruction {
            body["system_instruction"] = json!({
                "parts": [{"text": sys}]
            });
        }

        if !function_declarations.is_empty() {
            body["tools"] = json!([{
                "function_declarations": function_declarations
            }]);
        }

        let url = format!(
            "{}/models/{}:generateContent?key={}",
            self.base_url, self.model, self.api_key
        );

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_default();
            anyhow::bail!("Gemini API error {}: {}", status, error_body);
        }

        let resp: Value = response.json().await?;
        let (text, tool_calls) = parse_gemini_response(&resp);

        if let Some(tx) = stream_tx {
            if !text.is_empty() {
                let _ = tx.send(StreamEvent::TextDelta(text.clone()));
            }
            for tc in &tool_calls {
                let _ = tx.send(StreamEvent::ToolCallStart {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                });
                let _ = tx.send(StreamEvent::ToolCallEnd);
            }
            let _ = tx.send(StreamEvent::Done);
        }

        Ok((text, tool_calls))
    }
}

fn parse_gemini_response(resp: &Value) -> (String, Vec<ToolCall>) {
    let mut text = String::new();
    let mut tool_calls = Vec::new();

    let candidates = match resp["candidates"].as_array() {
        Some(c) => c,
        None => return (text, tool_calls),
    };

    let parts = match candidates
        .first()
        .and_then(|c| c["content"]["parts"].as_array())
    {
        Some(p) => p,
        None => return (text, tool_calls),
    };

    for part in parts {
        if let Some(t) = part["text"].as_str() {
            text.push_str(t);
        }
        if let Some(fc) = part.get("functionCall") {
            let name = fc["name"].as_str().unwrap_or("").to_string();
            let args = fc["args"].clone();
            // Use name as id since Gemini doesn't assign call ids
            tool_calls.push(ToolCall {
                id: name.clone(),
                name,
                arguments: args,
            });
        }
    }

    (text, tool_calls)
}
