use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HakariConfig {
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
    #[serde(default)]
    pub models: ModelTiers,
    #[serde(default)]
    pub preferences: Preferences,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub api_key: String,
    #[serde(default)]
    pub default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelTiers {
    pub tier_1: String,
    pub tier_2: String,
    pub tier_3: String,
    pub tier_4: String,
}

impl Default for ModelTiers {
    fn default() -> Self {
        Self {
            tier_1: "gemini-3.1-flash-lite-preview".into(),
            tier_2: "gemini-3-flash-preview".into(),
            tier_3: "gemini-3.1-pro-preview".into(),
            tier_4: "gemini-3.1-pro-preview".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preferences {
    pub theme: String,
    pub shizuka_model: String,
    #[serde(default)]
    pub shizuka_provider: String,
    #[serde(default)]
    pub nano_provider: String,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            shizuka_model: "gemini-3.1-flash-lite-preview".into(),
            shizuka_provider: String::new(),
            nano_provider: String::new(),
        }
    }
}

#[allow(dead_code)]
impl HakariConfig {
    pub fn config_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".hakari")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => toml::from_str(&content).unwrap_or_default(),
                Err(_) => Self::default(),
            }
        } else {
            Self::default()
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)?;
        let content = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(Self::config_path(), content)
    }

    pub fn active_provider(&self) -> Option<(&str, &ProviderConfig)> {
        // First look for explicitly default provider
        if let Some((name, cfg)) = self.providers.iter().find(|(_, v)| v.default) {
            return Some((name.as_str(), cfg));
        }
        // Fall back to first provider
        self.providers.iter().next().map(|(k, v)| (k.as_str(), v))
    }

    pub fn active_api_key(&self) -> Option<&str> {
        self.active_provider().map(|(_, cfg)| cfg.api_key.as_str())
    }

    pub fn set_provider(&mut self, name: &str, api_key: String, make_default: bool) {
        if make_default {
            for cfg in self.providers.values_mut() {
                cfg.default = false;
            }
        }
        self.providers.insert(
            name.to_string(),
            ProviderConfig {
                api_key,
                default: make_default,
            },
        );
    }

    pub fn model_for_classification(&self, classification: &str) -> &str {
        match classification {
            "trivial" => &self.models.tier_1,
            "small" => &self.models.tier_2,
            "medium" => &self.models.tier_3,
            "large" => &self.models.tier_4,
            _ => &self.models.tier_2,
        }
    }

    pub fn provider_names() -> Vec<&'static str> {
        vec!["gemini", "anthropic", "openai", "copilot"]
    }

    pub fn is_copilot(&self) -> bool {
        self.active_provider()
            .map(|(name, _)| name == "copilot")
            .unwrap_or(false)
    }
}

// Connect flow state for the TUI
#[derive(Debug, Clone)]
pub struct ConnectState {
    pub providers: Vec<ConnectProvider>,
    pub selected: usize,
    pub phase: ConnectPhase,
    pub api_key_input: String,
    pub api_key_cursor: usize,
    pub test_result: Option<Result<String, String>>,
}

#[derive(Debug, Clone)]
pub struct ConnectProvider {
    pub name: String,
    pub display_name: String,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectPhase {
    SelectProvider,
    EnterApiKey,
    Testing,
    Done,
    CopilotDeviceFlow {
        user_code: String,
        verification_uri: String,
    },
    CopilotPolling,
}

impl ConnectState {
    pub fn new(config: &HakariConfig) -> Self {
        let providers = vec![
            ConnectProvider {
                name: "gemini".into(),
                display_name: "Google Gemini".into(),
                connected: config.providers.contains_key("gemini"),
            },
            ConnectProvider {
                name: "anthropic".into(),
                display_name: "Anthropic".into(),
                connected: config.providers.contains_key("anthropic"),
            },
            ConnectProvider {
                name: "openai".into(),
                display_name: "OpenAI".into(),
                connected: config.providers.contains_key("openai"),
            },
            ConnectProvider {
                name: "copilot".into(),
                display_name: "GitHub Copilot".into(),
                connected: config.providers.contains_key("copilot"),
            },
        ];
        Self {
            providers,
            selected: 0,
            phase: ConnectPhase::SelectProvider,
            api_key_input: String::new(),
            api_key_cursor: 0,
            test_result: None,
        }
    }
}
