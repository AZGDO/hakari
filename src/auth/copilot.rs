use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const CLIENT_ID: &str = "Ov23li8tweQw6odWQebz";
const VERSION: &str = "0.1.85";
const COPILOT_API: &str = "https://api.githubcopilot.com";
const PROVIDER_KEY: &str = "github-copilot";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthInfo {
    #[serde(rename = "type")]
    pub auth_type: String,
    pub refresh: String,
    pub access: String,
    pub expires: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotModel {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub family: Option<String>,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default)]
    pub tool_call: bool,
    #[serde(default)]
    pub limit: Option<ModelLimit>,
    #[serde(default)]
    pub input_rate: Option<f64>,
    #[serde(default)]
    pub output_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelLimit {
    #[serde(default)]
    pub context: usize,
    #[serde(default)]
    pub output: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotUsage {
    pub used: u64,
    pub limit: u64,
    pub percent_left: f64,
    pub requests_left: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeviceCodeResponse {
    verification_uri: String,
    user_code: String,
    device_code: String,
    interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    error: Option<String>,
    interval: Option<u64>,
}

fn auth_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local")
        .join("share")
        .join("opencode")
        .join("auth.json")
}

pub fn read_auth() -> Option<OAuthInfo> {
    let path = auth_path();
    let raw = std::fs::read_to_string(path).ok()?;
    let data: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let entry = data.get(PROVIDER_KEY)?;
    serde_json::from_value(entry.clone()).ok()
}

pub fn save_auth(token: &str) -> anyhow::Result<()> {
    let path = auth_path();
    let mut data: serde_json::Value = if path.exists() {
        let raw = std::fs::read_to_string(&path)?;
        serde_json::from_str(&raw).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    data[PROVIDER_KEY] = serde_json::json!({
        "type": "oauth",
        "access": token,
        "refresh": token,
        "expires": 0
    });

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(&data)?;
    std::fs::write(&path, content)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }

    Ok(())
}

pub fn get_token() -> Option<String> {
    read_auth().map(|a| a.refresh)
}

pub fn is_authenticated() -> bool {
    get_token().is_some()
}

pub fn token_preview() -> Option<String> {
    get_token().map(|t| {
        if t.len() > 8 {
            format!("{}...", &t[..8])
        } else {
            t
        }
    })
}

#[derive(Debug, Clone)]
pub struct DeviceFlowState {
    pub verification_uri: String,
    pub user_code: String,
    pub device_code: String,
    pub interval: u64,
}

pub async fn start_device_flow() -> anyhow::Result<DeviceFlowState> {
    let client = reqwest::Client::new();
    let res = client
        .post("https://github.com/login/device/code")
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("User-Agent", format!("opencode/{}", VERSION))
        .json(&serde_json::json!({
            "client_id": CLIENT_ID,
            "scope": "read:user"
        }))
        .send()
        .await?;

    if !res.status().is_success() {
        anyhow::bail!("Failed to initiate device authorization: {}", res.status());
    }

    let device: DeviceCodeResponse = res.json().await?;
    Ok(DeviceFlowState {
        verification_uri: device.verification_uri,
        user_code: device.user_code,
        device_code: device.device_code,
        interval: device.interval,
    })
}

pub async fn poll_for_token(state: &DeviceFlowState) -> anyhow::Result<Option<String>> {
    let client = reqwest::Client::new();
    let res = client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("User-Agent", format!("opencode/{}", VERSION))
        .json(&serde_json::json!({
            "client_id": CLIENT_ID,
            "device_code": state.device_code,
            "grant_type": "urn:ietf:params:oauth:grant-type:device_code"
        }))
        .send()
        .await?;

    if !res.status().is_success() {
        anyhow::bail!("Token request failed: {}", res.status());
    }

    let data: TokenResponse = res.json().await?;

    if let Some(token) = data.access_token {
        save_auth(&token)?;
        return Ok(Some(token));
    }

    if let Some(ref error) = data.error {
        match error.as_str() {
            "authorization_pending" => return Ok(None),
            "slow_down" => return Ok(None),
            _ => anyhow::bail!("Auth failed: {}", error),
        }
    }

    Ok(None)
}

pub async fn fetch_models() -> anyhow::Result<Vec<CopilotModel>> {
    let token = get_token().ok_or_else(|| anyhow::anyhow!("Not authenticated with Copilot"))?;
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{}/models", copilot_base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .header("Copilot-Integration-Id", "vscode-chat")
        .header("Accept", "application/json")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?;

    if !res.status().is_success() {
        anyhow::bail!("Failed to fetch models: {}", res.status());
    }

    let data: serde_json::Value = res.json().await?;

    let mut models: Vec<CopilotModel> = Vec::new();
    if let Some(arr) = data["data"].as_array() {
        for val in arr {
            let id = val["id"].as_str().unwrap_or("").to_string();
            let name = val["name"].as_str().unwrap_or(&id).to_string();
            if id.is_empty() { continue; }

            let caps = &val["capabilities"];
            let family = caps["family"].as_str().map(|s| s.to_string());
            let reasoning = caps["supports"]["reasoning_effort"].is_array()
                || caps["supports"]["adaptive_thinking"].as_bool().unwrap_or(false);
            let tool_call = caps["supports"]["tool_calls"].as_bool().unwrap_or(false);
            let context = caps["limits"]["max_context_window_tokens"].as_u64().unwrap_or(0) as usize;
            let output = caps["limits"]["max_output_tokens"].as_u64().unwrap_or(0) as usize;

            // Copilot API doesn't expose pricing — leave as None
            models.push(CopilotModel {
                id,
                name,
                family,
                reasoning,
                tool_call,
                limit: Some(ModelLimit { context, output }),
                input_rate: None,
                output_rate: None,
            });
        }
    }

    models.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(models)
}

pub fn copilot_base_url() -> String {
    std::env::var("COPILOT_API").unwrap_or_else(|_| COPILOT_API.to_string())
}

pub async fn fetch_usage() -> anyhow::Result<CopilotUsage> {
    let token = get_token().ok_or_else(|| anyhow::anyhow!("Not authenticated"))?;
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{}/usage", copilot_base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .header("Copilot-Integration-Id", "vscode-chat")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?;

    if !res.status().is_success() {
        // Fallback: return unknown usage
        return Ok(CopilotUsage {
            used: 0,
            limit: 0,
            percent_left: 100.0,
            requests_left: 0,
        });
    }

    let data: serde_json::Value = res.json().await?;
    let used = data["chat_messages_used"].as_u64()
        .or_else(|| data["total_used"].as_u64())
        .unwrap_or(0);
    let limit = data["chat_messages_limit"].as_u64()
        .or_else(|| data["total_limit"].as_u64())
        .unwrap_or(0);
    let (percent_left, requests_left) = if limit > 0 {
        let left = limit.saturating_sub(used);
        ((left as f64 / limit as f64) * 100.0, left)
    } else {
        (100.0, 0)
    };

    Ok(CopilotUsage { used, limit, percent_left, requests_left })
}
