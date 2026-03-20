use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "hakari", about = "HAKARI — Harness for Agents, Keeping Agents Reasonably Iterate")]
pub struct CliArgs {
    /// Project directory (defaults to current directory)
    #[arg(short, long)]
    pub project_dir: Option<PathBuf>,

    /// Path to configuration file
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Enable debug logging
    #[arg(long)]
    pub debug: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ModelCategory {
    Max,
    High,
    Medium,
    Light,
}

impl std::fmt::Display for ModelCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Max => write!(f, "Max"),
            Self::High => write!(f, "High"),
            Self::Medium => write!(f, "Medium"),
            Self::Light => write!(f, "Light"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningLevel {
    None,
    Low,
    Medium,
    High,
    XHigh,
}

impl std::fmt::Display for ReasoningLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
            Self::XHigh => write!(f, "xhigh"),
        }
    }
}

impl ReasoningLevel {
    pub fn default_for_model(model_id: &str) -> Self {
        let lower = model_id.to_lowercase();
        if lower.contains("gpt") || lower.contains("codex") || lower.contains("o1") || lower.contains("o3") || lower.contains("o4") {
            Self::XHigh
        } else {
            Self::High
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelListEntry {
    pub demand: String,
    pub model_id: String,
    pub category: ModelCategory,
    pub reasoning: ReasoningLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HakariConfig {
    pub nano_provider: LlmProvider,
    pub nano_model: String,
    pub nano_category: ModelCategory,
    pub nano_reasoning: ReasoningLevel,
    pub shizuka_provider: LlmProvider,
    pub shizuka_model: String,
    pub shizuka_category: ModelCategory,
    pub openai_api_key: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub openai_base_url: String,
    pub anthropic_base_url: String,
    pub max_context_tokens: usize,
    pub iteration_budgets: IterationBudgets,
    pub model_list: Vec<ModelListEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    OpenAI,
    Anthropic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationBudgets {
    pub trivial_max_tool_calls: usize,
    pub small_max_tool_calls: usize,
    pub medium_max_tool_calls: usize,
    pub large_max_tool_calls: usize,
    pub trivial_max_writes_per_file: usize,
    pub small_max_writes_per_file: usize,
    pub medium_max_writes_per_file: usize,
    pub large_max_writes_per_file: usize,
    pub trivial_max_execute_retries: usize,
    pub small_max_execute_retries: usize,
    pub medium_max_execute_retries: usize,
    pub large_max_execute_retries: usize,
}

impl Default for IterationBudgets {
    fn default() -> Self {
        Self {
            trivial_max_tool_calls: 4,
            small_max_tool_calls: 10,
            medium_max_tool_calls: 25,
            large_max_tool_calls: 15,
            trivial_max_writes_per_file: 2,
            small_max_writes_per_file: 3,
            medium_max_writes_per_file: 5,
            large_max_writes_per_file: 4,
            trivial_max_execute_retries: 1,
            small_max_execute_retries: 3,
            medium_max_execute_retries: 5,
            large_max_execute_retries: 4,
        }
    }
}

impl Default for HakariConfig {
    fn default() -> Self {
        Self {
            nano_provider: LlmProvider::OpenAI,
            nano_model: "gpt-4.1".to_string(),
            nano_category: ModelCategory::Max,
            nano_reasoning: ReasoningLevel::XHigh,
            shizuka_provider: LlmProvider::OpenAI,
            shizuka_model: "gpt-4.1-mini".to_string(),
            shizuka_category: ModelCategory::Light,
            openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
            anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
            openai_base_url: "https://api.openai.com/v1".to_string(),
            anthropic_base_url: "https://api.anthropic.com".to_string(),
            max_context_tokens: 128_000,
            iteration_budgets: IterationBudgets::default(),
            model_list: Vec::new(),
        }
    }
}

impl HakariConfig {
    pub fn load(path: Option<&PathBuf>) -> anyhow::Result<Self> {
        if let Some(path) = path {
            let content = std::fs::read_to_string(path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            let config_path = dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("hakari")
                .join("config.json");
            if config_path.exists() {
                let content = std::fs::read_to_string(&config_path)?;
                Ok(serde_json::from_str(&content)?)
            } else {
                Ok(Self::default())
            }
        }
    }

    pub fn nano_budget(&self, classification: &str) -> (usize, usize, usize) {
        let b = &self.iteration_budgets;
        match classification {
            "trivial" => (b.trivial_max_tool_calls, b.trivial_max_writes_per_file, b.trivial_max_execute_retries),
            "small" => (b.small_max_tool_calls, b.small_max_writes_per_file, b.small_max_execute_retries),
            "medium" => (b.medium_max_tool_calls, b.medium_max_writes_per_file, b.medium_max_execute_retries),
            "large" => (b.large_max_tool_calls, b.large_max_writes_per_file, b.large_max_execute_retries),
            _ => (b.medium_max_tool_calls, b.medium_max_writes_per_file, b.medium_max_execute_retries),
        }
    }
}
