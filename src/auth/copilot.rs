use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const CLIENT_ID: &str = "Ov23li8tweQw6odWQebz";
const VERSION: &str = "0.1.85";
const COPILOT_API: &str = "https://api.githubcopilot.com";
const PROVIDER_KEY: &str = "github-copilot";
const MULTIPLIERS_URL: &str =
    "https://raw.githubusercontent.com/github/docs/main/data/tables/copilot/model-multipliers.yml";
const RELEASE_STATUS_URL: &str = "https://raw.githubusercontent.com/github/docs/main/data/tables/copilot/model-release-status.yml";
const REGISTRY_CACHE_TTL_SECS: u64 = 60 * 60 * 24;

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
    pub provider: Option<String>,
    #[serde(default)]
    pub release_status: Option<String>,
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
    #[serde(default)]
    pub premium_multiplier_paid: Option<f64>,
    #[serde(default)]
    pub premium_multiplier_free: Option<f64>,
    #[serde(default)]
    pub premium_multiplier_paid_display: Option<String>,
    #[serde(default)]
    pub premium_multiplier_free_display: Option<String>,
    #[serde(default)]
    pub included_in_paid: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegistryCache {
    fetched_at: u64,
    entries: Vec<ModelRegistryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModelRegistryEntry {
    name: String,
    normalized: String,
    provider: Option<String>,
    release_status: Option<String>,
    multiplier_paid: Option<f64>,
    multiplier_free: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
struct RemoteMultiplierEntry {
    name: String,
    multiplier_paid: serde_yaml::Value,
    multiplier_free: serde_yaml::Value,
}

#[derive(Debug, Clone, Deserialize)]
struct RemoteReleaseStatusEntry {
    name: String,
    provider: String,
    release_status: String,
}

fn auth_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Some(config_dir) = dirs::config_dir() {
        paths.push(config_dir.join("hakari").join("auth.json"));
        paths.push(config_dir.join("opencode").join("auth.json"));
    }

    if let Some(home_dir) = dirs::home_dir() {
        paths.push(
            home_dir
                .join(".local")
                .join("share")
                .join("opencode")
                .join("auth.json"),
        );
    }

    paths
}

fn auth_path() -> PathBuf {
    auth_paths()
        .into_iter()
        .next()
        .unwrap_or_else(|| PathBuf::from("auth.json"))
}

fn registry_cache_path() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("hakari")
        .join("copilot-model-registry.json")
}

pub fn read_auth() -> Option<OAuthInfo> {
    for path in auth_paths() {
        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(_) => continue,
        };
        let data: serde_json::Value = match serde_json::from_str(&raw) {
            Ok(data) => data,
            Err(_) => continue,
        };
        if let Some(entry) = data.get(PROVIDER_KEY) {
            if let Ok(auth) = serde_json::from_value(entry.clone()) {
                return Some(auth);
            }
        }
    }

    None
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

pub async fn ping() -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{}/_ping", copilot_base_url()))
        .timeout(Duration::from_secs(10))
        .send()
        .await?;

    if !res.status().is_success() {
        anyhow::bail!("Copilot API ping failed: {}", res.status());
    }

    Ok(())
}

pub async fn fetch_models() -> anyhow::Result<Vec<CopilotModel>> {
    let token = get_token().ok_or_else(|| anyhow::anyhow!("Not authenticated with Copilot"))?;
    let registry = load_model_registry().await;
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
            if id.is_empty() {
                continue;
            }

            let caps = &val["capabilities"];
            let family = caps["family"].as_str().map(|s| s.to_string());
            let reasoning = caps["supports"]["reasoning_effort"].is_array()
                || caps["supports"]["adaptive_thinking"]
                    .as_bool()
                    .unwrap_or(false);
            let tool_call = caps["supports"]["tool_calls"].as_bool().unwrap_or(false);
            let context = caps["limits"]["max_context_window_tokens"]
                .as_u64()
                .unwrap_or(0) as usize;
            let output = caps["limits"]["max_output_tokens"].as_u64().unwrap_or(0) as usize;
            let live_paid_multiplier = extract_live_multiplier(val, "paid");
            let live_free_multiplier = extract_live_multiplier(val, "free");
            let matched = match_registry_entry(&registry, &id, &name, family.as_deref());

            let premium_multiplier_paid = live_paid_multiplier
                .or_else(|| matched.as_ref().and_then(|entry| entry.multiplier_paid));
            let premium_multiplier_free = live_free_multiplier
                .or_else(|| matched.as_ref().and_then(|entry| entry.multiplier_free));

            models.push(CopilotModel {
                id,
                name,
                family,
                provider: matched.as_ref().and_then(|entry| entry.provider.clone()),
                release_status: matched
                    .as_ref()
                    .and_then(|entry| entry.release_status.clone()),
                reasoning,
                tool_call,
                limit: Some(ModelLimit { context, output }),
                input_rate: None,
                output_rate: None,
                premium_multiplier_paid,
                premium_multiplier_free,
                premium_multiplier_paid_display: format_multiplier(premium_multiplier_paid),
                premium_multiplier_free_display: format_multiplier(premium_multiplier_free),
                included_in_paid: premium_multiplier_paid == Some(0.0),
            });
        }
    }

    models.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
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
    let used = data["chat_messages_used"]
        .as_u64()
        .or_else(|| data["total_used"].as_u64())
        .unwrap_or(0);
    let limit = data["chat_messages_limit"]
        .as_u64()
        .or_else(|| data["total_limit"].as_u64())
        .unwrap_or(0);
    let (percent_left, requests_left) = if limit > 0 {
        let left = limit.saturating_sub(used);
        ((left as f64 / limit as f64) * 100.0, left)
    } else {
        (100.0, 0)
    };

    Ok(CopilotUsage {
        used,
        limit,
        percent_left,
        requests_left,
    })
}

pub fn model_multiplier_display(model_id_or_name: &str) -> Option<String> {
    match_registry_entry(
        &builtin_registry_entries(),
        model_id_or_name,
        model_id_or_name,
        None,
    )
    .and_then(|entry| format_multiplier(entry.multiplier_paid))
}

async fn load_model_registry() -> Vec<ModelRegistryEntry> {
    let built_in = builtin_registry_entries();

    if let Ok(remote) = fetch_remote_registry().await {
        let merged = merge_registry_entries(built_in, remote.clone());
        let _ = save_registry_cache(&remote);
        return merged;
    }

    if let Some(cached) = load_registry_cache() {
        return merge_registry_entries(built_in, cached.entries);
    }

    built_in
}

async fn fetch_remote_registry() -> anyhow::Result<Vec<ModelRegistryEntry>> {
    let client = reqwest::Client::new();
    let multipliers_text = client
        .get(MULTIPLIERS_URL)
        .timeout(Duration::from_secs(10))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let release_status_text = client
        .get(RELEASE_STATUS_URL)
        .timeout(Duration::from_secs(10))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let multiplier_rows: Vec<RemoteMultiplierEntry> = serde_yaml::from_str(&multipliers_text)?;
    let release_rows: Vec<RemoteReleaseStatusEntry> = serde_yaml::from_str(&release_status_text)?;

    let mut release_map: HashMap<String, RemoteReleaseStatusEntry> = HashMap::new();
    for row in release_rows {
        release_map.insert(normalize_model_key(&row.name), row);
    }

    let mut entries = Vec::new();
    for row in multiplier_rows {
        let normalized = normalize_model_key(&row.name);
        let release = release_map.get(&normalized);
        entries.push(ModelRegistryEntry {
            name: row.name.clone(),
            normalized,
            provider: release.map(|entry| entry.provider.clone()),
            release_status: release.map(|entry| entry.release_status.clone()),
            multiplier_paid: parse_yaml_multiplier(&row.multiplier_paid),
            multiplier_free: parse_yaml_multiplier(&row.multiplier_free),
        });
    }

    Ok(entries)
}

fn load_registry_cache() -> Option<RegistryCache> {
    let path = registry_cache_path();
    let raw = std::fs::read_to_string(path).ok()?;
    let cache: RegistryCache = serde_json::from_str(&raw).ok()?;

    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    if now.saturating_sub(cache.fetched_at) > REGISTRY_CACHE_TTL_SECS {
        return None;
    }

    Some(cache)
}

fn save_registry_cache(entries: &[ModelRegistryEntry]) -> anyhow::Result<()> {
    let path = registry_cache_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let fetched_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let cache = RegistryCache {
        fetched_at,
        entries: entries.to_vec(),
    };

    std::fs::write(path, serde_json::to_string_pretty(&cache)?)?;
    Ok(())
}

fn merge_registry_entries(
    primary: Vec<ModelRegistryEntry>,
    secondary: Vec<ModelRegistryEntry>,
) -> Vec<ModelRegistryEntry> {
    let mut merged: HashMap<String, ModelRegistryEntry> = HashMap::new();

    for entry in primary.into_iter().chain(secondary.into_iter()) {
        merged.insert(entry.normalized.clone(), entry);
    }

    let mut entries: Vec<ModelRegistryEntry> = merged.into_values().collect();
    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    entries
}

fn match_registry_entry(
    registry: &[ModelRegistryEntry],
    id: &str,
    name: &str,
    family: Option<&str>,
) -> Option<ModelRegistryEntry> {
    let mut candidates = vec![normalize_model_key(id), normalize_model_key(name)];
    if let Some(family) = family {
        let normalized = normalize_model_key(family);
        if !normalized.is_empty() {
            candidates.push(normalized);
        }
    }

    for candidate in &candidates {
        if let Some(entry) = registry.iter().find(|entry| entry.normalized == *candidate) {
            return Some(entry.clone());
        }
    }

    registry
        .iter()
        .filter(|entry| {
            candidates.iter().any(|candidate| {
                candidate.ends_with(&entry.normalized)
                    || candidate.starts_with(&entry.normalized)
                    || entry.normalized.ends_with(candidate)
            })
        })
        .max_by_key(|entry| entry.normalized.len())
        .cloned()
}

fn extract_live_multiplier(value: &serde_json::Value, kind: &str) -> Option<f64> {
    let candidates = [
        &value["billing"][kind]["premium_request_multiplier"],
        &value["billing"][kind]["multiplier"],
        &value["billing"][format!("{}_multiplier", kind)],
        &value["pricing"][kind]["multiplier"],
        &value["pricing"][format!("{}_multiplier", kind)],
        &value["capabilities"]["billing"][kind]["multiplier"],
    ];

    candidates.iter().find_map(|candidate| {
        candidate
            .as_f64()
            .or_else(|| candidate.as_str().and_then(|s| s.parse().ok()))
    })
}

fn parse_yaml_multiplier(value: &serde_yaml::Value) -> Option<f64> {
    match value {
        serde_yaml::Value::Number(num) => num.as_f64(),
        serde_yaml::Value::String(text) => text.parse::<f64>().ok(),
        _ => None,
    }
}

fn normalize_model_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn format_multiplier(value: Option<f64>) -> Option<String> {
    value.map(|value| {
        if (value.fract()).abs() < f64::EPSILON {
            format!("{:.0}x", value)
        } else if ((value * 10.0).fract()).abs() < f64::EPSILON {
            format!("{:.1}x", value)
        } else {
            format!("{:.2}x", value)
        }
    })
}

fn builtin_registry_entries() -> Vec<ModelRegistryEntry> {
    vec![
        registry_entry(
            "Claude Haiku 4.5",
            Some("Anthropic"),
            Some("GA"),
            Some(0.33),
            Some(1.0),
        ),
        registry_entry(
            "Claude Opus 4.5",
            Some("Anthropic"),
            Some("GA"),
            Some(3.0),
            None,
        ),
        registry_entry(
            "Claude Opus 4.6",
            Some("Anthropic"),
            Some("GA"),
            Some(3.0),
            None,
        ),
        registry_entry(
            "Claude Opus 4.6 (fast mode) (preview)",
            Some("Anthropic"),
            Some("Public preview"),
            Some(30.0),
            None,
        ),
        registry_entry(
            "Claude Sonnet 4",
            Some("Anthropic"),
            Some("GA"),
            Some(1.0),
            None,
        ),
        registry_entry(
            "Claude Sonnet 4.5",
            Some("Anthropic"),
            Some("GA"),
            Some(1.0),
            None,
        ),
        registry_entry(
            "Claude Sonnet 4.6",
            Some("Anthropic"),
            Some("GA"),
            Some(1.0),
            None,
        ),
        registry_entry(
            "Gemini 2.5 Pro",
            Some("Google"),
            Some("GA"),
            Some(1.0),
            None,
        ),
        registry_entry(
            "Gemini 3 Flash",
            Some("Google"),
            Some("Public preview"),
            Some(0.33),
            None,
        ),
        registry_entry(
            "Gemini 3 Pro",
            Some("Google"),
            Some("Public preview"),
            Some(1.0),
            None,
        ),
        registry_entry(
            "Gemini 3.1 Pro",
            Some("Google"),
            Some("Public preview"),
            Some(1.0),
            None,
        ),
        registry_entry(
            "Goldeneye",
            Some("Fine-tuned GPT-5.1-Codex"),
            Some("Public preview"),
            None,
            Some(1.0),
        ),
        registry_entry("GPT-4.1", Some("OpenAI"), Some("GA"), Some(0.0), Some(1.0)),
        registry_entry("GPT-4o", Some("OpenAI"), Some("GA"), Some(0.0), Some(1.0)),
        registry_entry(
            "GPT-5 mini",
            Some("OpenAI"),
            Some("GA"),
            Some(0.0),
            Some(1.0),
        ),
        registry_entry("GPT-5.1", Some("OpenAI"), Some("GA"), Some(1.0), None),
        registry_entry("GPT-5.1-Codex", Some("OpenAI"), Some("GA"), Some(1.0), None),
        registry_entry(
            "GPT-5.1-Codex-Mini",
            Some("OpenAI"),
            Some("Public preview"),
            Some(0.33),
            None,
        ),
        registry_entry(
            "GPT-5.1-Codex-Max",
            Some("OpenAI"),
            Some("GA"),
            Some(1.0),
            None,
        ),
        registry_entry("GPT-5.2", Some("OpenAI"), Some("GA"), Some(1.0), None),
        registry_entry("GPT-5.2-Codex", Some("OpenAI"), Some("GA"), Some(1.0), None),
        registry_entry("GPT-5.3-Codex", Some("OpenAI"), Some("GA"), Some(1.0), None),
        registry_entry("GPT-5.4", Some("OpenAI"), Some("GA"), Some(1.0), None),
        registry_entry("GPT-5.4 mini", Some("OpenAI"), Some("GA"), Some(0.33), None),
        registry_entry(
            "Grok Code Fast 1",
            Some("xAI"),
            Some("GA"),
            Some(0.25),
            Some(1.0),
        ),
        registry_entry(
            "Raptor mini",
            Some("Fine-tuned GPT-5 mini"),
            Some("Public preview"),
            Some(0.0),
            Some(1.0),
        ),
    ]
}

fn registry_entry(
    name: &str,
    provider: Option<&str>,
    release_status: Option<&str>,
    multiplier_paid: Option<f64>,
    multiplier_free: Option<f64>,
) -> ModelRegistryEntry {
    ModelRegistryEntry {
        name: name.to_string(),
        normalized: normalize_model_key(name),
        provider: provider.map(|value| value.to_string()),
        release_status: release_status.map(|value| value.to_string()),
        multiplier_paid,
        multiplier_free,
    }
}
