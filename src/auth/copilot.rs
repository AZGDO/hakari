use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const CLIENT_ID: &str = "Ov23li8tweQw6odWQebz";
const VERSION: &str = "0.1.85";
const COPILOT_API: &str = "https://api.githubcopilot.com";
const MODELS_URL: &str = "https://models.dev/api.json";
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelLimit {
    #[serde(default)]
    pub context: usize,
    #[serde(default)]
    pub output: usize,
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
    let client = reqwest::Client::new();
    let res = client
        .get(MODELS_URL)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?;

    if !res.status().is_success() {
        anyhow::bail!("Failed to fetch models: {}", res.status());
    }

    let data: serde_json::Value = res.json().await?;
    let provider = data.get(PROVIDER_KEY)
        .ok_or_else(|| anyhow::anyhow!("No github-copilot provider found"))?;
    let models_obj = provider.get("models")
        .ok_or_else(|| anyhow::anyhow!("No models field"))?;

    let mut models: Vec<CopilotModel> = Vec::new();
    if let Some(obj) = models_obj.as_object() {
        for (_key, val) in obj {
            if let Ok(m) = serde_json::from_value::<CopilotModel>(val.clone()) {
                models.push(m);
            }
        }
    }

    models.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(models)
}

pub fn copilot_base_url() -> String {
    std::env::var("COPILOT_API").unwrap_or_else(|_| COPILOT_API.to_string())
}
