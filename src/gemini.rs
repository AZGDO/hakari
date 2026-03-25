use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

fn safe_bytes(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[derive(Debug, Clone)]
pub struct GeminiClient {
    client: Client,
    api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiMessage {
    pub role: String,
    pub parts: Vec<GeminiPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GeminiPart {
    Text {
        text: String,
        #[serde(rename = "thoughtSignature", skip_serializing_if = "Option::is_none", default)]
        thought_signature: Option<String>,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: FunctionCall,
        #[serde(rename = "thoughtSignature", skip_serializing_if = "Option::is_none", default)]
        thought_signature: Option<String>,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: FunctionResponse,
    },
}

impl GeminiPart {
    /// Create a plain text part (no thought signature).
    pub fn text(s: impl Into<String>) -> Self {
        Self::Text { text: s.into(), thought_signature: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    #[serde(default)]
    pub args: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionResponse {
    pub name: String,
    pub response: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDeclaration {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct GeminiResponse {
    pub parts: Vec<GeminiPart>,
    pub finish_reason: Option<String>,
    pub usage: Option<UsageMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageMetadata {
    #[serde(default, rename = "promptTokenCount")]
    pub prompt_token_count: u64,
    #[serde(default, rename = "candidatesTokenCount")]
    pub candidates_token_count: u64,
    #[serde(default, rename = "totalTokenCount")]
    pub total_token_count: u64,
    #[serde(default, rename = "cachedContentTokenCount")]
    pub cached_content_token_count: u64,
}

pub enum StreamEvent {
    TextDelta(String),
    /// (function_call, thought_signature at part level)
    FunctionCall(FunctionCall, Option<String>),
    Done(Option<UsageMetadata>),
    Error(String),
}

impl GeminiClient {
    pub fn new(api_key: &str) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.to_string(),
        }
    }

    /// Build the endpoint URL (no key in query string).
    fn url(&self, model: &str, action: &str) -> String {
        format!("{}/models/{}:{}", BASE_URL, model, action)
    }

    /// Build the endpoint URL for streaming (includes alt=sse).
    fn stream_url(&self, model: &str) -> String {
        format!("{}/models/{}:streamGenerateContent?alt=sse", BASE_URL, model)
    }

    pub async fn generate(
        &self,
        model: &str,
        messages: &[GeminiMessage],
        system_instruction: Option<&str>,
        tools: &[FunctionDeclaration],
    ) -> Result<GeminiResponse, String> {
        let url = self.url(model, "generateContent");

        let mut body = json!({
            "contents": messages,
            "generationConfig": {
                "thinkingConfig": {
                    "thinkingLevel": "low"
                }
            }
        });

        if let Some(sys) = system_instruction {
            body["systemInstruction"] = json!({
                "parts": [{"text": sys}]
            });
        }

        if !tools.is_empty() {
            body["tools"] = json!([{
                "functionDeclarations": tools
            }]);
        }

        let resp = self
            .client
            .post(&url)
            .header("x-goog-api-key", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| format!("Read error: {}", e))?;

        if !status.is_success() {
            return Err(format!("API error ({}): {}", status, text));
        }

        let parsed: Value = serde_json::from_str(&text)
            .map_err(|e| format!("Parse error: {} (body: {})", e, safe_bytes(&text, 500)))?;

        Self::parse_response(&parsed)
    }

    pub async fn generate_stream(
        &self,
        model: &str,
        messages: &[GeminiMessage],
        system_instruction: Option<&str>,
        tools: &[FunctionDeclaration],
        tx: tokio::sync::mpsc::Sender<StreamEvent>,
    ) -> Result<(), String> {
        let url = self.stream_url(model);

        let mut body = json!({
            "contents": messages,
            "generationConfig": {
                "thinkingConfig": {
                    "thinkingLevel": "low"
                }
            }
        });

        if let Some(sys) = system_instruction {
            body["systemInstruction"] = json!({
                "parts": [{"text": sys}]
            });
        }

        if !tools.is_empty() {
            body["tools"] = json!([{
                "functionDeclarations": tools
            }]);
        }

        let resp = self
            .client
            .post(&url)
            .header("x-goog-api-key", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            let _ = tx
                .send(StreamEvent::Error(format!(
                    "API error ({}): {}",
                    status, text
                )))
                .await;
            return Err(format!("API error ({})", status));
        }

        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();
        let mut last_usage: Option<UsageMetadata> = None;

        while let Some(chunk_result) = stream.next().await {
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx
                        .send(StreamEvent::Error(format!("Stream error: {}", e)))
                        .await;
                    break;
                }
            };

            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Normalize CRLF to LF for SSE parsing (Gemini API sends \r\n)
            if buffer.contains('\r') {
                buffer = buffer.replace("\r\n", "\n");
            }

            // SSE format: "data: {...}\n\n"
            while let Some(pos) = buffer.find("\n\n") {
                let line = buffer[..pos].to_string();
                buffer = buffer[pos + 2..].to_string();

                let data = if let Some(d) = line.strip_prefix("data: ") {
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

                // Surface API-level errors embedded in the stream
                if let Some(err) = parsed.get("error") {
                    let msg = err
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("unknown");
                    let code = err.get("code").and_then(|c| c.as_u64()).unwrap_or(0);
                    let _ = tx
                        .send(StreamEvent::Error(format!("API error {}: {}", code, msg)))
                        .await;
                    return Err(format!("API error {}", code));
                }

                if let Some(usage) = parsed.get("usageMetadata") {
                    if let Ok(u) = serde_json::from_value::<UsageMetadata>(usage.clone()) {
                        last_usage = Some(u);
                    }
                }

                if let Some(candidates) = parsed.get("candidates").and_then(|c| c.as_array()) {
                    for candidate in candidates {
                        // Check for blocking finishReasons
                        if let Some(reason) = candidate.get("finishReason").and_then(|r| r.as_str())
                        {
                            match reason {
                                "MALFORMED_FUNCTION_CALL" => {
                                    let _ = tx.send(StreamEvent::Error(
                                        "MALFORMED_FUNCTION_CALL: The model tried to produce a tool call that was too large or malformed. The request will be retried.".into()
                                    )).await;
                                    return Err("MALFORMED_FUNCTION_CALL".into());
                                }
                                "SAFETY" => {
                                    let _ = tx
                                        .send(StreamEvent::Error(
                                            "Response blocked by safety filter.".into(),
                                        ))
                                        .await;
                                    return Err("SAFETY".into());
                                }
                                _ => {}
                            }
                        }
                        if let Some(parts) = candidate
                            .get("content")
                            .and_then(|c| c.get("parts"))
                            .and_then(|p| p.as_array())
                        {
                            for part in parts {
                                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                    if !text.is_empty() {
                                        let _ =
                                            tx.send(StreamEvent::TextDelta(text.to_string())).await;
                                    }
                                } else if let Some(fc) = part.get("functionCall") {
                                    let thought_sig = part.get("thoughtSignature")
                                        .and_then(|t| t.as_str())
                                        .map(|s| s.to_string());
                                    if let Ok(call) =
                                        serde_json::from_value::<FunctionCall>(fc.clone())
                                    {
                                        let _ = tx.send(StreamEvent::FunctionCall(call, thought_sig)).await;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let _ = tx.send(StreamEvent::Done(last_usage)).await;
        Ok(())
    }

    fn parse_response(value: &Value) -> Result<GeminiResponse, String> {
        // Handle top-level error object from API
        if let Some(err) = value.get("error") {
            let msg = err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown API error");
            let code = err.get("code").and_then(|c| c.as_u64()).unwrap_or(0);
            return Err(format!("API error {}: {}", code, msg));
        }

        let candidates = value
            .get("candidates")
            .and_then(|c| c.as_array())
            .ok_or_else(|| {
                // Return the raw body snippet for debugging
                let raw = serde_json::to_string(value).unwrap_or_default();
                format!("No candidates in response. Body: {}", safe_bytes(&raw, 300))
            })?;

        let candidate = candidates.first().ok_or("Empty candidates array")?;

        let finish_reason = candidate
            .get("finishReason")
            .and_then(|f| f.as_str())
            .map(|s| s.to_string());

        let usage = value
            .get("usageMetadata")
            .and_then(|u| serde_json::from_value::<UsageMetadata>(u.clone()).ok());

        // content may be absent when finishReason is SAFETY or MAX_TOKENS — treat as empty
        let parts_raw = candidate
            .get("content")
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.as_array());

        let mut parts = Vec::new();
        if let Some(parts_raw) = parts_raw {
            for part in parts_raw {
                let thought_sig = part.get("thoughtSignature")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string());

                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                    // Keep even empty text parts if they carry a thoughtSignature
                    if !text.is_empty() || thought_sig.is_some() {
                        parts.push(GeminiPart::Text {
                            text: text.to_string(),
                            thought_signature: thought_sig,
                        });
                    }
                } else if let Some(fc) = part.get("functionCall") {
                    if let Ok(call) = serde_json::from_value::<FunctionCall>(fc.clone()) {
                        parts.push(GeminiPart::FunctionCall {
                            function_call: call,
                            thought_signature: thought_sig,
                        });
                    }
                }
            }
        }

        // If SAFETY or other blocking finish, surface it clearly
        if parts.is_empty() {
            if let Some(ref reason) = finish_reason {
                if reason != "STOP" && reason != "MAX_TOKENS" {
                    return Err(format!("Response blocked: finishReason={}", reason));
                }
            }
        }

        Ok(GeminiResponse {
            parts,
            finish_reason,
            usage,
        })
    }

    /// Like `generate` but requests structured JSON output via responseSchema.
    /// Returns the raw JSON text from the model's text part.
    pub async fn generate_structured(
        &self,
        model: &str,
        messages: &[GeminiMessage],
        system_instruction: Option<&str>,
        response_schema: &Value,
    ) -> Result<String, String> {
        let url = self.url(model, "generateContent");

        let mut body = json!({
            "contents": messages,
            "generationConfig": {
                "responseMimeType": "application/json",
                "responseSchema": response_schema,
                "thinkingConfig": {
                    "thinkingLevel": "low"
                }
            }
        });

        if let Some(sys) = system_instruction {
            body["systemInstruction"] = json!({
                "parts": [{"text": sys}]
            });
        }

        let resp = self
            .client
            .post(&url)
            .header("x-goog-api-key", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| format!("Read error: {}", e))?;

        if !status.is_success() {
            return Err(format!("API error ({}): {}", status, text));
        }

        let parsed: Value = serde_json::from_str(&text)
            .map_err(|e| format!("Parse error: {} (body: {})", e, safe_bytes(&text, 500)))?;

        if let Some(err) = parsed.get("error") {
            let msg = err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown API error");
            let code = err.get("code").and_then(|c| c.as_u64()).unwrap_or(0);
            return Err(format!("API error {}: {}", code, msg));
        }

        let candidates = parsed
            .get("candidates")
            .and_then(|c| c.as_array())
            .ok_or_else(|| {
                let raw = serde_json::to_string(&parsed).unwrap_or_default();
                format!("No candidates. Body: {}", safe_bytes(&raw, 400))
            })?;

        let candidate = candidates.first().ok_or("Empty candidates")?;

        let finish_reason = candidate
            .get("finishReason")
            .and_then(|f| f.as_str())
            .unwrap_or("STOP");

        if finish_reason != "STOP" && finish_reason != "MAX_TOKENS" {
            return Err(format!("Response blocked: finishReason={}", finish_reason));
        }

        let json_text = candidate
            .get("content")
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.as_array())
            .and_then(|parts| parts.first())
            .and_then(|p| p.get("text"))
            .and_then(|t| t.as_str())
            .ok_or("No text in structured response")?;

        Ok(json_text.to_string())
    }

    pub async fn test_connection(&self) -> Result<String, String> {
        let url = format!("{}/models", BASE_URL);
        let resp = self
            .client
            .get(&url)
            .header("x-goog-api-key", &self.api_key)
            .send()
            .await
            .map_err(|e| format!("Connection failed: {}", e))?;

        if resp.status().is_success() {
            Ok("Connected successfully".into())
        } else {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            Err(format!(
                "Auth failed ({}): {}",
                status,
                safe_bytes(&text, 200)
            ))
        }
    }
}

pub fn build_function_declaration(
    name: &str,
    description: &str,
    properties: Value,
    required: Vec<&str>,
) -> FunctionDeclaration {
    FunctionDeclaration {
        name: name.to_string(),
        description: description.to_string(),
        parameters: json!({
            "type": "object",
            "properties": properties,
            "required": required,
        }),
    }
}
