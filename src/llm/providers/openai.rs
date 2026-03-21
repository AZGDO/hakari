use super::StreamEvent;
use crate::llm::messages::{Message, Role, ToolCall};
use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::mpsc;

pub struct OpenAiProvider {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
    is_copilot: bool,
    uses_responses_api: bool,
    reasoning_effort: Option<String>,
}

impl OpenAiProvider {
    pub fn new(
        api_key: String,
        base_url: String,
        model: String,
        reasoning_effort: Option<String>,
    ) -> Self {
        let is_copilot = base_url.contains("githubcopilot.com");
        let uses_responses_api = is_copilot && requires_responses_api(&model);
        Self {
            client: Client::new(),
            api_key,
            base_url,
            model,
            is_copilot,
            uses_responses_api,
            reasoning_effort,
        }
    }

    fn convert_messages(&self, messages: &[Message]) -> Vec<Value> {
        messages
            .iter()
            .map(|msg| {
                let mut obj = json!({
                    "role": match msg.role {
                        Role::System => "system",
                        Role::User => "user",
                        Role::Assistant => "assistant",
                        Role::Tool => "tool",
                    },
                    "content": msg.content.to_text_string(),
                });
                if let Some(tool_calls) = &msg.tool_calls {
                    obj["tool_calls"] = json!(tool_calls
                        .iter()
                        .map(|tc| json!({
                            "id": tc.id,
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                "arguments": tc.arguments.to_string(),
                            }
                        }))
                        .collect::<Vec<_>>());
                }
                if let Some(tool_call_id) = &msg.tool_call_id {
                    obj["tool_call_id"] = json!(tool_call_id);
                }
                obj
            })
            .collect()
    }

    pub async fn chat(
        &self,
        messages: &[Message],
        tools: &[Value],
        stream_tx: Option<mpsc::UnboundedSender<StreamEvent>>,
    ) -> anyhow::Result<(String, Vec<ToolCall>)> {
        if self.uses_responses_api {
            return self.chat_via_responses(messages, tools, stream_tx).await;
        }
        let converted_messages = self.convert_messages(messages);

        let mut body = json!({
            "model": self.model,
            "messages": converted_messages,
            "stream": stream_tx.is_some(),
        });

        if !tools.is_empty() {
            body["tools"] = json!(tools);
        }

        if supports_reasoning_effort(&self.model) {
            if let Some(reasoning_effort) = &self.reasoning_effort {
                body["reasoning_effort"] = json!(reasoning_effort);
            }
        }

        let request_was_streaming = stream_tx.is_some();

        let mut request = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json");

        if self.is_copilot {
            request = request
                .header("Copilot-Integration-Id", "vscode-chat")
                .header("Editor-Version", "vscode/1.99.0")
                .header("Editor-Plugin-Version", "copilot-chat/0.1.85")
                .header("User-Agent", "hakari/0.1.0");
        }

        let response = request.json(&body).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_default();

            if self.is_copilot && request_was_streaming {
                if let Ok((text, tool_calls)) = self.chat_without_stream(messages, tools).await {
                    if let Some(tx) = stream_tx {
                        if !text.is_empty() {
                            let _ = tx.send(StreamEvent::TextDelta(text.clone()));
                        }
                        let _ = tx.send(StreamEvent::Done);
                    }
                    return Ok((text, tool_calls));
                }
            }

            anyhow::bail!("OpenAI API error {}: {}", status, error_body);
        }

        if let Some(tx) = stream_tx {
            let mut text_content = String::new();
            let mut tool_calls: Vec<ToolCall> = Vec::new();
            let mut current_tool_args = String::new();
            let mut current_tool_idx: Option<usize> = None;

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

                    if line.is_empty() || line == "data: [DONE]" {
                        if line == "data: [DONE]" {
                            if let Some(idx) = current_tool_idx.take() {
                                if let Some(tc) = tool_calls.get_mut(idx) {
                                    tc.arguments = serde_json::from_str(&current_tool_args)
                                        .unwrap_or(json!(current_tool_args));
                                }
                                current_tool_args.clear();
                                let _ = tx.send(StreamEvent::ToolCallEnd);
                            }
                            let _ = tx.send(StreamEvent::Done);
                        }
                        continue;
                    }

                    if let Some(data) = line.strip_prefix("data: ") {
                        if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                            if let Some(choices) = parsed["choices"].as_array() {
                                for choice in choices {
                                    let delta = &choice["delta"];

                                    if let Some(content) = delta["content"].as_str() {
                                        text_content.push_str(content);
                                        let _ =
                                            tx.send(StreamEvent::TextDelta(content.to_string()));
                                    }

                                    if let Some(tcs) = delta["tool_calls"].as_array() {
                                        for tc in tcs {
                                            let idx = tc["index"].as_u64().unwrap_or(0) as usize;
                                            if let Some(func) = tc.get("function") {
                                                if let Some(name) = func["name"].as_str() {
                                                    let id =
                                                        tc["id"].as_str().unwrap_or("").to_string();
                                                    while tool_calls.len() <= idx {
                                                        tool_calls.push(ToolCall {
                                                            id: String::new(),
                                                            name: String::new(),
                                                            arguments: json!({}),
                                                        });
                                                    }
                                                    tool_calls[idx].id = id.clone();
                                                    tool_calls[idx].name = name.to_string();

                                                    if let Some(prev_idx) = current_tool_idx.take()
                                                    {
                                                        if let Some(prev_tc) =
                                                            tool_calls.get_mut(prev_idx)
                                                        {
                                                            prev_tc.arguments = serde_json::from_str(&current_tool_args)
                                                                .unwrap_or(json!(current_tool_args));
                                                        }
                                                        current_tool_args.clear();
                                                        let _ = tx.send(StreamEvent::ToolCallEnd);
                                                    }
                                                    current_tool_idx = Some(idx);
                                                    let _ = tx.send(StreamEvent::ToolCallStart {
                                                        id,
                                                        name: name.to_string(),
                                                    });
                                                }
                                                if let Some(args) = func["arguments"].as_str() {
                                                    current_tool_args.push_str(args);
                                                    let _ = tx.send(
                                                        StreamEvent::ToolCallArgumentsDelta(
                                                            args.to_string(),
                                                        ),
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(idx) = current_tool_idx.take() {
                if let Some(tc) = tool_calls.get_mut(idx) {
                    tc.arguments = serde_json::from_str(&current_tool_args)
                        .unwrap_or(json!(current_tool_args));
                }
                let _ = tx.send(StreamEvent::ToolCallEnd);
            }

            if self.is_copilot && text_content.is_empty() && tool_calls.is_empty() {
                let (text, fallback_tool_calls) = self.chat_without_stream(messages, tools).await?;
                if !text.is_empty() {
                    let _ = tx.send(StreamEvent::TextDelta(text.clone()));
                }
                let _ = tx.send(StreamEvent::Done);
                return Ok((text, fallback_tool_calls));
            }

            let _ = tx.send(StreamEvent::Done);

            Ok((text_content, tool_calls))
        } else {
            let response_body: Value = response.json().await?;
            Ok(parse_non_stream_response(&response_body))
        }
    }

    async fn chat_via_responses(
        &self,
        messages: &[Message],
        tools: &[Value],
        stream_tx: Option<mpsc::UnboundedSender<StreamEvent>>,
    ) -> anyhow::Result<(String, Vec<ToolCall>)> {
        let input = convert_messages_to_responses_input(messages);

        let responses_tools: Vec<Value> = tools
            .iter()
            .filter_map(|t| {
                let func = t.get("function")?;
                Some(json!({
                    "type": "function",
                    "name": func["name"],
                    "description": func["description"],
                    "parameters": func["parameters"],
                }))
            })
            .collect();

        let mut body = json!({
            "model": self.model,
            "input": input,
        });

        if !responses_tools.is_empty() {
            body["tools"] = json!(responses_tools);
        }

        let request = self
            .client
            .post(format!("{}/responses", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .header("Copilot-Integration-Id", "vscode-chat")
            .header("Editor-Version", "vscode/1.99.0")
            .header("Editor-Plugin-Version", "copilot-chat/0.1.85")
            .header("User-Agent", "hakari/0.1.0");

        let response = request.json(&body).send().await?;
        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_default();
            anyhow::bail!("Responses API error {}: {}", status, error_body);
        }

        let resp: Value = response.json().await?;
        let (text, tool_calls) = parse_responses_response(&resp);

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

    async fn chat_without_stream(
        &self,
        messages: &[Message],
        tools: &[Value],
    ) -> anyhow::Result<(String, Vec<ToolCall>)> {
        let converted_messages = self.convert_messages(messages);

        let mut body = json!({
            "model": self.model,
            "messages": converted_messages,
            "stream": false,
        });

        if !tools.is_empty() {
            body["tools"] = json!(tools);
        }

        if supports_reasoning_effort(&self.model) {
            if let Some(reasoning_effort) = &self.reasoning_effort {
                body["reasoning_effort"] = json!(reasoning_effort);
            }
        }

        let mut request = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json");

        if self.is_copilot {
            request = request
                .header("Copilot-Integration-Id", "vscode-chat")
                .header("Editor-Version", "vscode/1.99.0")
                .header("Editor-Plugin-Version", "copilot-chat/0.1.85")
                .header("User-Agent", "hakari/0.1.0");
        }

        let response = request.json(&body).send().await?;
        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API error {}: {}", status, error_body);
        }

        let response_body: Value = response.json().await?;
        Ok(parse_non_stream_response(&response_body))
    }
}

fn requires_responses_api(model: &str) -> bool {
    let lower = model.to_lowercase();
    // gpt-5.4 and future models that only support /responses
    lower.starts_with("gpt-5.") && !lower.starts_with("gpt-5-")
}

fn convert_messages_to_responses_input(messages: &[Message]) -> Vec<Value> {
    let mut input: Vec<Value> = Vec::new();

    for msg in messages {
        match msg.role {
            Role::System => {
                // system messages become instructions-style user items in responses API
                input.push(json!({
                    "role": "user",
                    "content": [{"type": "input_text", "text": msg.content.to_text_string()}]
                }));
            }
            Role::User => {
                input.push(json!({
                    "role": "user",
                    "content": [{"type": "input_text", "text": msg.content.to_text_string()}]
                }));
            }
            Role::Assistant => {
                if let Some(tool_calls) = &msg.tool_calls {
                    for tc in tool_calls {
                        input.push(json!({
                            "type": "function_call",
                            "call_id": tc.id,
                            "name": tc.name,
                            "arguments": tc.arguments.to_string(),
                        }));
                    }
                } else {
                    let text = msg.content.to_text_string();
                    if !text.is_empty() {
                        input.push(json!({
                            "role": "assistant",
                            "content": [{"type": "output_text", "text": text}]
                        }));
                    }
                }
            }
            Role::Tool => {
                if let Some(call_id) = &msg.tool_call_id {
                    input.push(json!({
                        "type": "function_call_output",
                        "call_id": call_id,
                        "output": msg.content.to_text_string(),
                    }));
                }
            }
        }
    }

    input
}

fn parse_responses_response(resp: &Value) -> (String, Vec<ToolCall>) {
    let mut text = String::new();
    let mut tool_calls = Vec::new();

    if let Some(output) = resp["output"].as_array() {
        for item in output {
            match item["type"].as_str() {
                Some("message") => {
                    if let Some(content) = item["content"].as_array() {
                        for block in content {
                            if let Some(t) = block["text"].as_str() {
                                text.push_str(t);
                            }
                        }
                    }
                }
                Some("function_call") => {
                    let call_id = item["call_id"].as_str().unwrap_or("").to_string();
                    let name = item["name"].as_str().unwrap_or("").to_string();
                    let args_str = item["arguments"].as_str().unwrap_or("{}");
                    let arguments: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
                    tool_calls.push(ToolCall {
                        id: call_id,
                        name,
                        arguments,
                    });
                }
                _ => {}
            }
        }
    }

    (text, tool_calls)
}

fn supports_reasoning_effort(model: &str) -> bool {
    let lower = model.to_lowercase();
    lower.contains("gpt") || lower.contains("o1") || lower.contains("o3") || lower.contains("o4")
}

fn extract_text_content(message: &Value) -> String {
    if let Some(text) = message["content"].as_str() {
        return text.to_string();
    }

    if let Some(blocks) = message["content"].as_array() {
        return blocks
            .iter()
            .filter_map(|block| block["text"].as_str().or_else(|| block["content"].as_str()))
            .collect::<Vec<_>>()
            .join("\n");
    }

    String::new()
}

fn parse_non_stream_response(response_body: &Value) -> (String, Vec<ToolCall>) {
    let choice = &response_body["choices"][0]["message"];
    let text = extract_text_content(choice);

    let mut tool_calls = Vec::new();
    if let Some(tcs) = choice["tool_calls"].as_array() {
        for tc in tcs {
            let id = tc["id"].as_str().unwrap_or("").to_string();
            let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
            let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
            let arguments: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
            tool_calls.push(ToolCall {
                id,
                name,
                arguments,
            });
        }
    }

    (text, tool_calls)
}
