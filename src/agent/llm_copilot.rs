use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::sync::mpsc;

use crate::copilot::{CopilotClient, CopilotStreamChunk};

use super::llm::*;

pub struct CopilotLlm {
    client: CopilotClient,
}

impl CopilotLlm {
    pub fn new(oauth_token: &str) -> Self {
        Self {
            client: CopilotClient::new(oauth_token),
        }
    }
}

// ── Message conversion ──────────────────────────────────────────────────────

fn split_messages(messages: &[LlmMessage]) -> (Option<String>, Vec<Value>) {
    let mut system = None;
    let mut result = Vec::new();

    for msg in messages {
        match msg {
            LlmMessage::System(text) => {
                system = Some(text.clone());
            }
            LlmMessage::User(text) => {
                result.push(json!({"role": "user", "content": text}));
            }
            LlmMessage::Assistant { text, tool_calls } => {
                if tool_calls.is_empty() {
                    result.push(json!({"role": "assistant", "content": text}));
                } else {
                    let tc_json: Vec<Value> = tool_calls
                        .iter()
                        .map(|tc| {
                            json!({
                                "id": tc.id,
                                "type": "function",
                                "function": {
                                    "name": tc.name,
                                    "arguments": tc.args.to_string()
                                }
                            })
                        })
                        .collect();
                    let mut msg = json!({
                        "role": "assistant",
                        "tool_calls": tc_json
                    });
                    if !text.is_empty() {
                        msg["content"] = json!(text);
                    }
                    result.push(msg);
                }
            }
            LlmMessage::ToolResult {
                tool_call_id,
                content,
                ..
            } => {
                result.push(json!({
                    "role": "tool",
                    "tool_call_id": tool_call_id,
                    "content": content
                }));
            }
        }
    }

    (system, result)
}

fn convert_tools(tools: &[ToolDef]) -> Value {
    let arr: Vec<Value> = tools
        .iter()
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters
                }
            })
        })
        .collect();
    Value::Array(arr)
}

fn convert_rate_limits(limits: &crate::copilot::CopilotRateLimits) -> RateLimits {
    RateLimits {
        total: limits.total,
        remaining: limits.remaining,
        reset_at: limits.reset_at,
    }
}

// ── Structured output (Copilot lacks responseSchema) ────────────────────────

fn build_structured_messages(messages: &[LlmMessage]) -> Vec<Value> {
    // Collect the structure prompt (last system message)
    let structure_prompt = messages
        .iter()
        .rev()
        .find_map(|m| {
            if let LlmMessage::System(text) = m {
                Some(text.as_str())
            } else {
                None
            }
        })
        .unwrap_or("");

    let mut summary = String::new();
    for msg in messages {
        if let LlmMessage::User(text) = msg {
            summary.push_str("USER REQUEST:\n");
            summary.push_str(&text.chars().take(2000).collect::<String>());
            break;
        }
    }

    let did_explore = messages.iter().any(|m| {
        matches!(m, LlmMessage::Assistant { tool_calls, .. } if !tool_calls.is_empty())
    });

    if did_explore {
        summary.push_str("\n\nEXPLORATION RESULTS:\n");
        for msg in messages {
            match msg {
                LlmMessage::Assistant { text, tool_calls } => {
                    for tc in tool_calls {
                        let args_short: String = tc.args.to_string().chars().take(200).collect();
                        summary.push_str(&format!("Called: {}({})\n", tc.name, args_short));
                    }
                    if !text.is_empty() {
                        summary.push_str(&format!(
                            "Assistant: {}\n",
                            &text.chars().take(500).collect::<String>()
                        ));
                    }
                }
                LlmMessage::ToolResult { content, .. } => {
                    let short: String = content.chars().take(1500).collect();
                    summary.push_str(&format!("Result: {}\n", short));
                    if content.chars().count() > 1500 {
                        summary.push_str("... (truncated)\n");
                    }
                }
                _ => {}
            }
        }
    }

    summary.push_str(&format!(
        "\n\nINSTRUCTIONS:\n{}\n\nRespond with ONLY a valid JSON object. No markdown fences, no explanation text. The JSON must have these fields:\n- task_classification: one of \"trivial\", \"small\", \"medium\", \"large\"\n- task_summary: string\n- direct_answer: string (answer here if no code changes needed; empty string otherwise)\n- context_files: array of objects with path, role, compact_summary, annotations, focus_regions\n- approach: string with step-by-step plan\n- learnings: array of strings\n- warnings: array of strings\n- sub_tasks: array of objects with description",
        structure_prompt
    ));

    vec![json!({"role": "user", "content": summary})]
}

fn extract_json_object(text: &str) -> String {
    // Try ```json fences first
    if let Some(start) = text.find("```json") {
        let after = &text[start + 7..];
        if let Some(end) = after.find("```") {
            let candidate = after[..end].trim();
            if candidate.starts_with('{') {
                return candidate.to_string();
            }
        }
    }
    // Try generic ``` fences
    if let Some(start) = text.find("```") {
        let after = &text[start + 3..];
        let after = if let Some(nl) = after.find('\n') {
            &after[nl + 1..]
        } else {
            after
        };
        if let Some(end) = after.find("```") {
            let candidate = after[..end].trim();
            if candidate.starts_with('{') {
                return candidate.to_string();
            }
        }
    }
    // Brace matching
    if let Some(start) = text.find('{') {
        let mut depth = 0;
        for (i, ch) in text[start..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return text[start..=start + i].to_string();
                    }
                }
                _ => {}
            }
        }
        if let Some(end) = text.rfind('}') {
            return text[start..=end].to_string();
        }
    }
    text.to_string()
}

// ── Trait impl ──────────────────────────────────────────────────────────────

#[async_trait]
impl LlmClient for CopilotLlm {
    async fn generate(
        &self,
        model: &str,
        messages: &[LlmMessage],
        tools: &[ToolDef],
    ) -> Result<LlmResponse, String> {
        let (system, oai_msgs) = split_messages(messages);
        let tools_val = if tools.is_empty() {
            None
        } else {
            Some(convert_tools(tools))
        };

        let (resp, rate_limits) = self
            .client
            .chat_completion_raw(model, &oai_msgs, system.as_deref(), tools_val.as_ref())
            .await?;

        let choice = resp.choices.first().ok_or("No choices in response")?;
        let text = choice.message.content.clone().unwrap_or_default();
        let tool_calls = choice
            .message
            .tool_calls
            .as_ref()
            .map(|tcs| {
                tcs.iter()
                    .map(|tc| {
                        let args: Value =
                            serde_json::from_str(&tc.function.arguments).unwrap_or(json!({}));
                        ToolCall {
                            id: tc.id.clone(),
                            name: tc.function.name.clone(),
                            args,
                            thought_signature: None,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(LlmResponse {
            text,
            tool_calls,
            usage: resp.usage.map(|u| TokenUsage {
                input_tokens: u.prompt_tokens,
                output_tokens: u.completion_tokens,
                cached_tokens: 0,
            }),
            rate_limits: rate_limits.map(|r| convert_rate_limits(&r)),
        })
    }

    async fn generate_stream(
        &self,
        model: &str,
        messages: &[LlmMessage],
        tools: &[ToolDef],
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<(), String> {
        let (system, oai_msgs) = split_messages(messages);
        let tools_val = if tools.is_empty() {
            None
        } else {
            Some(convert_tools(tools))
        };

        let (inner_tx, mut inner_rx) = mpsc::channel::<CopilotStreamChunk>(64);

        let client = self.client.clone();
        let model = model.to_string();
        let system_owned = system;
        let handle = tokio::spawn(async move {
            let _ = client
                .chat_completion_stream(
                    &model,
                    &oai_msgs,
                    system_owned.as_deref(),
                    tools_val.as_ref(),
                    inner_tx,
                )
                .await;
        });

        // Accumulate tool call deltas, emit complete ToolCalls on Done
        let mut tool_call_acc: HashMap<usize, (String, String, String)> = HashMap::new();

        while let Some(chunk) = inner_rx.recv().await {
            match chunk {
                CopilotStreamChunk::TextDelta(delta) => {
                    let _ = tx.send(StreamEvent::TextDelta(delta)).await;
                }
                CopilotStreamChunk::ToolCallDelta(delta) => {
                    let entry = tool_call_acc
                        .entry(delta.index)
                        .or_insert_with(|| (String::new(), String::new(), String::new()));
                    if let Some(id) = &delta.id {
                        entry.0 = id.clone();
                    }
                    if let Some(ref func) = delta.function {
                        if let Some(ref name) = func.name {
                            entry.1 = name.clone();
                        }
                        if let Some(ref args) = func.arguments {
                            entry.2.push_str(args);
                        }
                    }
                }
                CopilotStreamChunk::FinishToolCalls => {}
                CopilotStreamChunk::Usage(usage) => {
                    let _ = tx
                        .send(StreamEvent::Usage(TokenUsage {
                            input_tokens: usage.prompt_tokens,
                            output_tokens: usage.completion_tokens,
                            cached_tokens: 0,
                        }))
                        .await;
                }
                CopilotStreamChunk::Error(e) => {
                    let _ = tx.send(StreamEvent::Error(e)).await;
                }
                CopilotStreamChunk::Done => {
                    // Emit accumulated tool calls in order
                    let mut indices: Vec<usize> = tool_call_acc.keys().cloned().collect();
                    indices.sort();
                    for idx in indices {
                        let (id, name, arguments) = &tool_call_acc[&idx];
                        let args: Value = serde_json::from_str(arguments).unwrap_or(json!({}));
                        let _ = tx
                            .send(StreamEvent::ToolCall(ToolCall {
                                id: id.clone(),
                                name: name.clone(),
                                args,
                                thought_signature: None,
                            }))
                            .await;
                    }
                    let _ = tx.send(StreamEvent::Done).await;
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
        _schema: &Value,
    ) -> Result<String, String> {
        let phase2_messages = build_structured_messages(messages);

        let (resp, _) = self
            .client
            .chat_completion_raw(model, &phase2_messages, None, None)
            .await?;

        let text = resp
            .choices
            .first()
            .and_then(|c| c.message.content.as_deref())
            .unwrap_or("");

        let json_text = extract_json_object(text);

        if serde_json::from_str::<Value>(&json_text).is_ok() {
            return Ok(json_text);
        }

        // Retry with forceful prompt
        let retry_messages = vec![json!({"role": "user", "content": format!(
            "Your previous response was not valid JSON. Return ONLY a JSON object (no markdown, no explanation) with these exact fields:\n{{\n  \"task_classification\": \"trivial|small|medium|large\",\n  \"task_summary\": \"...\",\n  \"direct_answer\": \"...\",\n  \"context_files\": [],\n  \"approach\": \"...\",\n  \"learnings\": [],\n  \"warnings\": [],\n  \"sub_tasks\": []\n}}\n\nPrevious error:\nPrevious text started with: {}",
            &json_text.chars().take(200).collect::<String>()
        )})];

        let (retry_resp, _) = self
            .client
            .chat_completion_raw(model, &retry_messages, None, None)
            .await?;

        let retry_text = retry_resp
            .choices
            .first()
            .and_then(|c| c.message.content.as_deref())
            .unwrap_or("");

        Ok(extract_json_object(retry_text))
    }

    async fn test_connection(&self) -> Result<String, String> {
        let messages = vec![json!({"role": "user", "content": "Say hi in one word."})];
        match self
            .client
            .chat_completion_raw("gpt-5-mini", &messages, None, None)
            .await
        {
            Ok(_) => Ok("Connected to GitHub Copilot successfully".into()),
            Err(e) => Err(e),
        }
    }

    fn provider_name(&self) -> &str {
        "copilot"
    }
}
