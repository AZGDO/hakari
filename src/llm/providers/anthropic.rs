use super::StreamEvent;
use crate::llm::messages::{Message, Role, ToolCall};
use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::mpsc;

pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
}

impl AnthropicProvider {
    pub fn new(api_key: String, base_url: String, model: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url,
            model,
        }
    }

    fn convert_messages(&self, messages: &[Message]) -> (Option<String>, Vec<Value>) {
        let mut system_prompt = None;
        let mut converted = Vec::new();

        for msg in messages {
            match msg.role {
                Role::System => {
                    system_prompt = Some(msg.content.to_text_string());
                }
                Role::User => {
                    converted.push(json!({
                        "role": "user",
                        "content": msg.content.to_text_string(),
                    }));
                }
                Role::Assistant => {
                    let mut content_blocks: Vec<Value> = Vec::new();
                    let text = msg.content.to_text_string();
                    if !text.is_empty() {
                        content_blocks.push(json!({
                            "type": "text",
                            "text": text,
                        }));
                    }
                    if let Some(tool_calls) = &msg.tool_calls {
                        for tc in tool_calls {
                            content_blocks.push(json!({
                                "type": "tool_use",
                                "id": tc.id,
                                "name": tc.name,
                                "input": tc.arguments,
                            }));
                        }
                    }
                    converted.push(json!({
                        "role": "assistant",
                        "content": content_blocks,
                    }));
                }
                Role::Tool => {
                    converted.push(json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": msg.tool_call_id.as_deref().unwrap_or(""),
                            "content": msg.content.to_text_string(),
                        }],
                    }));
                }
            }
        }

        (system_prompt, converted)
    }

    pub async fn chat(
        &self,
        messages: &[Message],
        tools: &[Value],
        stream_tx: Option<mpsc::UnboundedSender<StreamEvent>>,
    ) -> anyhow::Result<(String, Vec<ToolCall>)> {
        let (system_prompt, converted_messages) = self.convert_messages(messages);

        let mut body = json!({
            "model": self.model,
            "max_tokens": 8192,
            "messages": converted_messages,
            "stream": stream_tx.is_some(),
        });

        if let Some(sys) = &system_prompt {
            body["system"] = json!(sys);
        }
        if !tools.is_empty() {
            body["tools"] = json!(tools);
        }

        let response = self.client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API error {}: {}", status, error_body);
        }

        if let Some(tx) = stream_tx {
            let mut text_content = String::new();
            let mut tool_calls: Vec<ToolCall> = Vec::new();
            let mut current_tool_args = String::new();
            let mut current_tool_id = String::new();
            let mut current_tool_name = String::new();
            let mut in_tool = false;

            let bytes_stream = response.bytes_stream();
            use futures::StreamExt;
            let mut stream = bytes_stream;
            let mut buffer = String::new();

            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(line_end) = buffer.find('\n') {
                    let line = buffer[..line_end].trim().to_string();
                    buffer = buffer[line_end + 1..].to_string();

                    if line.is_empty() {
                        continue;
                    }

                    if let Some(data) = line.strip_prefix("data: ") {
                        if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                            let event_type = parsed["type"].as_str().unwrap_or("");
                            match event_type {
                                "content_block_start" => {
                                    if let Some(block) = parsed.get("content_block") {
                                        if block["type"].as_str() == Some("tool_use") {
                                            current_tool_id = block["id"].as_str().unwrap_or("").to_string();
                                            current_tool_name = block["name"].as_str().unwrap_or("").to_string();
                                            in_tool = true;
                                            let _ = tx.send(StreamEvent::ToolCallStart {
                                                id: current_tool_id.clone(),
                                                name: current_tool_name.clone(),
                                            });
                                        }
                                    }
                                }
                                "content_block_delta" => {
                                    if let Some(delta) = parsed.get("delta") {
                                        if let Some(text) = delta["text"].as_str() {
                                            text_content.push_str(text);
                                            let _ = tx.send(StreamEvent::TextDelta(text.to_string()));
                                        }
                                        if let Some(partial_json) = delta["partial_json"].as_str() {
                                            current_tool_args.push_str(partial_json);
                                            let _ = tx.send(StreamEvent::ToolCallArgumentsDelta(partial_json.to_string()));
                                        }
                                    }
                                }
                                "content_block_stop" => {
                                    if in_tool {
                                        let arguments: Value = serde_json::from_str(&current_tool_args)
                                            .unwrap_or(json!({}));
                                        tool_calls.push(ToolCall {
                                            id: current_tool_id.clone(),
                                            name: current_tool_name.clone(),
                                            arguments,
                                        });
                                        current_tool_args.clear();
                                        in_tool = false;
                                        let _ = tx.send(StreamEvent::ToolCallEnd);
                                    }
                                }
                                "message_stop" => {
                                    let _ = tx.send(StreamEvent::Done);
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }

            let _ = tx.send(StreamEvent::Done);
            Ok((text_content, tool_calls))
        } else {
            let response_body: Value = response.json().await?;
            let mut text_content = String::new();
            let mut tool_calls = Vec::new();

            if let Some(content) = response_body["content"].as_array() {
                for block in content {
                    match block["type"].as_str() {
                        Some("text") => {
                            if let Some(text) = block["text"].as_str() {
                                text_content.push_str(text);
                            }
                        }
                        Some("tool_use") => {
                            tool_calls.push(ToolCall {
                                id: block["id"].as_str().unwrap_or("").to_string(),
                                name: block["name"].as_str().unwrap_or("").to_string(),
                                arguments: block["input"].clone(),
                            });
                        }
                        _ => {}
                    }
                }
            }

            Ok((text_content, tool_calls))
        }
    }
}
