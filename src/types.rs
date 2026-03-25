use std::time::Instant;

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionMode {
    Default,
    PlanMode,
    AcceptEdits,
    BypassPermissions,
    DontAsk,
    Auto,
}

impl PermissionMode {
    pub fn title(&self) -> &str {
        match self {
            Self::Default => "Default",
            Self::PlanMode => "Plan Mode",
            Self::AcceptEdits => "Accept edits",
            Self::BypassPermissions => "Bypass Permissions",
            Self::DontAsk => "Don't Ask",
            Self::Auto => "Auto mode",
        }
    }

    pub fn short_title(&self) -> &str {
        match self {
            Self::Default => "Default",
            Self::PlanMode => "Plan",
            Self::AcceptEdits => "Accept",
            Self::BypassPermissions => "Bypass",
            Self::DontAsk => "DontAsk",
            Self::Auto => "Auto",
        }
    }

    pub fn symbol(&self) -> &str {
        match self {
            Self::Default => "",
            Self::PlanMode => "⏸",
            Self::AcceptEdits => "⏵⏵",
            Self::BypassPermissions => "⏵⏵",
            Self::DontAsk => "⏵⏵",
            Self::Auto => "⏵⏵",
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum ToolStatus {
    Running(String),
    Complete(String),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub name: String,
    pub args_summary: String,
    pub status: ToolStatus,
    pub output: Option<String>,
    pub collapsed: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum MessageContent {
    Text(String),
    CodeBlock {
        language: String,
        code: String,
    },
    ToolUse(ToolCall),
    Thinking(String),
    DiffBlock {
        file_path: String,
        added: Vec<String>,
        removed: Vec<String>,
    },
    ShizukaBlock {
        preloaded: Vec<String>,
        referenced: Vec<String>,
        task_summary: String,
        classification: String,
        collapsed: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Message {
    pub role: MessageRole,
    pub content: Vec<MessageContent>,
    pub timestamp: Instant,
}

#[derive(Debug, Clone)]
pub struct SlashCommand {
    pub name: String,
    pub description: String,
    pub is_enabled: bool,
}

impl SlashCommand {
    pub fn all_commands() -> Vec<Self> {
        vec![
            Self {
                name: "bug".into(),
                description: "Report a bug".into(),
                is_enabled: true,
            },
            Self {
                name: "clear".into(),
                description: "Clear conversation history".into(),
                is_enabled: true,
            },
            Self {
                name: "compact".into(),
                description: "Compact conversation context".into(),
                is_enabled: true,
            },
            Self {
                name: "config".into(),
                description: "View & change settings".into(),
                is_enabled: true,
            },
            Self {
                name: "connect".into(),
                description: "Connect an API provider".into(),
                is_enabled: true,
            },
            Self {
                name: "cost".into(),
                description: "Show token usage and cost".into(),
                is_enabled: true,
            },
            Self {
                name: "doctor".into(),
                description: "Check Claude Code health".into(),
                is_enabled: true,
            },
            Self {
                name: "exit".into(),
                description: "Exit Claude Code".into(),
                is_enabled: true,
            },
            Self {
                name: "help".into(),
                description: "Show available commands".into(),
                is_enabled: true,
            },
            Self {
                name: "init".into(),
                description: "Initialize project with CLAUDE.md".into(),
                is_enabled: true,
            },
            Self {
                name: "login".into(),
                description: "Sign in to your account".into(),
                is_enabled: true,
            },
            Self {
                name: "logout".into(),
                description: "Sign out of your account".into(),
                is_enabled: true,
            },
            Self {
                name: "memory".into(),
                description: "Edit CLAUDE.md memory files".into(),
                is_enabled: true,
            },
            Self {
                name: "model".into(),
                description: "Show current model config".into(),
                is_enabled: true,
            },
            Self {
                name: "nano".into(),
                description: "Set Nano (executor) provider & model".into(),
                is_enabled: true,
            },
            Self {
                name: "permissions".into(),
                description: "Manage tool permissions".into(),
                is_enabled: true,
            },
            Self {
                name: "review".into(),
                description: "Review a pull request".into(),
                is_enabled: true,
            },
            Self {
                name: "shizuka".into(),
                description: "Set Shizuka (explorer) provider & model".into(),
                is_enabled: true,
            },
            Self {
                name: "status".into(),
                description: "Show session info".into(),
                is_enabled: true,
            },
            Self {
                name: "theme".into(),
                description: "Change the theme".into(),
                is_enabled: true,
            },
            Self {
                name: "vim".into(),
                description: "Toggle vim mode".into(),
                is_enabled: true,
            },
        ]
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMode {
    Normal,
    SlashCommand,
    PermissionPrompt,
    ModelPicker,
    ThemePicker,
    Settings,
    Help,
    Connect,
    SessionPicker,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentPhase {
    Idle,
    Shizuka,
    Nano,
}

#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub tool_name: String,
    pub description: String,
    pub command: Option<String>,
    pub selected_option: usize,
}

impl PermissionRequest {
    pub fn options() -> Vec<&'static str> {
        vec!["Allow once", "Allow for this session", "Deny"]
    }
}

pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

impl TokenUsage {
    pub fn total(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }

    pub fn cost_usd(&self) -> f64 {
        let input_cost = self.input_tokens as f64 * 3.0 / 1_000_000.0;
        let output_cost = self.output_tokens as f64 * 15.0 / 1_000_000.0;
        let cache_read_cost = self.cache_read_tokens as f64 * 0.3 / 1_000_000.0;
        let cache_write_cost = self.cache_write_tokens as f64 * 3.75 / 1_000_000.0;
        input_cost + output_cost + cache_read_cost + cache_write_cost
    }
}

#[derive(Debug, Clone)]
pub struct ModelOption {
    pub id: String,
    pub display_name: String,
    pub context_window: &'static str,
    pub description: &'static str,
    pub rate_multiplier: Option<f32>,
    pub provider: String,
    pub is_header: bool,
}

impl ModelOption {
    pub fn for_provider(provider: &str) -> Vec<Self> {
        let prov = provider.to_string();
        match provider {
            "gemini" => vec![
                Self {
                    id: "gemini-3.1-pro-preview".into(),
                    display_name: "Gemini 3.1 Pro Preview".into(),
                    context_window: "1M",
                    description: "Most capable, best for complex tasks",
                    rate_multiplier: None,
                    provider: prov.clone(),
                    is_header: false,
                },
                Self {
                    id: "gemini-3-flash-preview".into(),
                    display_name: "Gemini 3 Flash Preview".into(),
                    context_window: "1M",
                    description: "Balanced speed & quality",
                    rate_multiplier: None,
                    provider: prov.clone(),
                    is_header: false,
                },
                Self {
                    id: "gemini-3.1-flash-lite-preview".into(),
                    display_name: "Gemini 3.1 Flash Lite Preview".into(),
                    context_window: "1M",
                    description: "Fastest and cheapest",
                    rate_multiplier: None,
                    provider: prov,
                    is_header: false,
                },
            ],
            "anthropic" => vec![
                Self {
                    id: "claude-opus-4-20250514".into(),
                    display_name: "Claude Opus 4".into(),
                    context_window: "200K",
                    description: "Most capable, best for complex tasks",
                    rate_multiplier: None,
                    provider: prov.clone(),
                    is_header: false,
                },
                Self {
                    id: "claude-sonnet-4-20250514".into(),
                    display_name: "Claude Sonnet 4".into(),
                    context_window: "200K",
                    description: "Balanced speed & quality",
                    rate_multiplier: None,
                    provider: prov.clone(),
                    is_header: false,
                },
                Self {
                    id: "claude-haiku-3-20250307".into(),
                    display_name: "Claude Haiku 3.5".into(),
                    context_window: "200K",
                    description: "Fastest, cheapest",
                    rate_multiplier: None,
                    provider: prov,
                    is_header: false,
                },
            ],
            "openai" => vec![
                Self {
                    id: "o3".into(),
                    display_name: "o3".into(),
                    context_window: "200K",
                    description: "Most capable reasoning model",
                    rate_multiplier: None,
                    provider: prov.clone(),
                    is_header: false,
                },
                Self {
                    id: "gpt-4.1".into(),
                    display_name: "GPT-4.1".into(),
                    context_window: "1M",
                    description: "Flagship GPT model",
                    rate_multiplier: None,
                    provider: prov.clone(),
                    is_header: false,
                },
                Self {
                    id: "gpt-4.1-mini".into(),
                    display_name: "GPT-4.1 Mini".into(),
                    context_window: "1M",
                    description: "Fast and affordable",
                    rate_multiplier: None,
                    provider: prov.clone(),
                    is_header: false,
                },
                Self {
                    id: "gpt-4.1-nano".into(),
                    display_name: "GPT-4.1 Nano".into(),
                    context_window: "1M",
                    description: "Fastest, cheapest",
                    rate_multiplier: None,
                    provider: prov,
                    is_header: false,
                },
            ],
            "copilot" => crate::copilot::copilot_models()
                .into_iter()
                .map(|m| Self {
                    id: m.id.to_string(),
                    display_name: m.display_name.to_string(),
                    context_window: m.context_window,
                    description: m.description,
                    rate_multiplier: Some(m.rate_multiplier),
                    provider: "copilot".into(),
                    is_header: false,
                })
                .collect(),
            _ => vec![],
        }
    }

    /// Build a flat list of models from all connected providers, with header rows separating them.
    pub fn for_connected_providers(config: &crate::config::HakariConfig) -> Vec<Self> {
        let provider_display: &[(&str, &str)] = &[
            ("gemini", "Google Gemini"),
            ("copilot", "GitHub Copilot"),
            ("anthropic", "Anthropic"),
            ("openai", "OpenAI"),
        ];
        let mut all = Vec::new();
        for &(name, display) in provider_display {
            if config.providers.contains_key(name) {
                let models = Self::for_provider(name);
                if !models.is_empty() {
                    // Insert a header row
                    all.push(Self {
                        id: String::new(),
                        display_name: display.to_string(),
                        context_window: "",
                        description: "",
                        rate_multiplier: None,
                        provider: name.to_string(),
                        is_header: true,
                    });
                    all.extend(models);
                }
            }
        }
        all
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SettingEntry {
    pub key: String,
    pub label: String,
    pub value: SettingValue,
    pub description: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum SettingValue {
    Bool(bool),
    Choice {
        options: Vec<String>,
        selected: usize,
    },
    Info(String),
}

impl SettingEntry {
    pub fn defaults() -> Vec<Self> {
        vec![
            Self {
                key: "theme".into(),
                label: "Theme".into(),
                value: SettingValue::Choice {
                    options: vec![
                        "dark".into(),
                        "light".into(),
                        "dark-daltonized".into(),
                        "light-daltonized".into(),
                    ],
                    selected: 0,
                },
                description: "Color theme for the UI".into(),
            },
            Self {
                key: "editorMode".into(),
                label: "Editor mode".into(),
                value: SettingValue::Choice {
                    options: vec!["normal".into(), "vim".into(), "emacs".into()],
                    selected: 0,
                },
                description: "Key binding mode".into(),
            },
            Self {
                key: "verbose".into(),
                label: "Verbose output".into(),
                value: SettingValue::Bool(false),
                description: "Show detailed debug output".into(),
            },
            Self {
                key: "autoCompact".into(),
                label: "Auto-compact".into(),
                value: SettingValue::Bool(true),
                description: "Automatically compact when context is full".into(),
            },
            Self {
                key: "showTurnDuration".into(),
                label: "Show turn duration".into(),
                value: SettingValue::Bool(true),
                description: "Show \"Cooked for\" after responses".into(),
            },
            Self {
                key: "syntaxHighlighting".into(),
                label: "Syntax highlighting".into(),
                value: SettingValue::Bool(true),
                description: "Enable syntax highlighting in code blocks".into(),
            },
            Self {
                key: "autoMemory".into(),
                label: "Auto memory".into(),
                value: SettingValue::Bool(false),
                description: "Automatically save observations to memory".into(),
            },
            Self {
                key: "spinnerTips".into(),
                label: "Spinner tips".into(),
                value: SettingValue::Bool(true),
                description: "Show tips while loading".into(),
            },
            Self {
                key: "notifications".into(),
                label: "Notifications".into(),
                value: SettingValue::Choice {
                    options: vec![
                        "auto".into(),
                        "terminal".into(),
                        "system".into(),
                        "off".into(),
                    ],
                    selected: 0,
                },
                description: "Preferred notification channel".into(),
            },
            Self {
                key: "diffTool".into(),
                label: "Diff tool".into(),
                value: SettingValue::Choice {
                    options: vec!["auto".into(), "builtin".into(), "delta".into()],
                    selected: 0,
                },
                description: "Tool for displaying diffs".into(),
            },
        ]
    }
}
