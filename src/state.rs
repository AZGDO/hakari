use crate::config::{ConnectState, HakariConfig};
use crate::copilot::CopilotUsage;
use crate::dialog::DialogState;
use crate::theme::Theme;
use crate::types::*;
use std::time::{Duration, Instant};

#[allow(dead_code)]
pub struct AppState {
    pub messages: Vec<Message>,
    pub input: String,
    pub cursor_pos: usize,
    pub scroll_offset: usize,
    pub mode: AppMode,
    pub permission_mode: PermissionMode,
    pub model_name: String,
    pub theme: Theme,
    pub token_usage: TokenUsage,
    pub is_loading: bool,
    pub loading_text: String,
    pub spinner_frame: usize,
    pub slash_commands: Vec<SlashCommand>,
    pub slash_filter: String,
    pub slash_selected: usize,
    pub slash_scroll: usize,
    pub permission_request: Option<PermissionRequest>,
    pub cwd: String,
    pub session_start: Instant,
    pub last_response_duration: Option<Duration>,
    pub show_welcome: bool,
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,
    pub show_turn_duration: bool,
    pub should_quit: bool,
    pub welcome_anim_frame: usize,
    pub shimmer_offset: f64,
    pub compact_notifications: Vec<(String, Instant)>,
    pub last_esc_time: Option<Instant>,
    pub streaming: Option<StreamingState>,
    pub context_window_percent: f64,
    // Model picker
    pub model_options: Vec<ModelOption>,
    // Settings
    pub settings: Vec<SettingEntry>,
    // Scroll control
    pub user_scrolled: bool,
    pub total_message_lines: usize,
    /// Wrapped-row offset where each message starts (computed during render).
    pub message_row_starts: Vec<usize>,
    // Input display
    pub input_scroll: usize,
    // Pre-stream delay
    pub pending_stream: Option<PendingStream>,
    // Agent state
    pub agent_phase: AgentPhase,
    pub agent_response_start: Option<Instant>,
    pub config: HakariConfig,
    pub connect_state: Option<ConnectState>,
    // Channels for agent communication
    pub agent_cancel_tx: Option<tokio::sync::mpsc::Sender<()>>,
    // Clickable row tracking for ShizukaBlock collapse toggle: msg_idx -> line_row
    pub shizuka_block_rows: std::collections::HashMap<usize, usize>,
    // Current session ID (set after first save, or loaded from resume)
    pub session_id: Option<String>,
    // Session picker data
    pub session_picker_sessions: Vec<crate::memory::kms::PersistedSession>,
    // @file mention picker
    pub file_mention_active: bool,
    pub file_mention_filter: String,
    pub file_mention_options: Vec<String>,
    pub file_mention_selected: usize,
    // Custom model input in model picker
    pub model_custom_input: String,
    pub model_custom_cursor: usize,
    pub model_picker_typing: bool,
    pub model_picker_target: String, // "shizuka" or "nano"
    // Generic dialog state — shared across all popup dialogs (one open at a time)
    pub dialog: DialogState,
    // Copilot usage tracking
    pub copilot_usage: CopilotUsage,
}

#[allow(dead_code)]
pub struct PendingStream {
    pub content: Vec<MessageContent>,
    pub fire_at: std::time::Instant,
    pub loading_phrases: Vec<String>,
    pub phrase_idx: usize,
    pub phrase_deadline: std::time::Instant,
}

pub struct StreamingState {
    pub full_content: Vec<MessageContent>,
    pub current_content_idx: usize,
    pub current_char_idx: usize,
    pub chars_per_tick: usize,
    pub started_at: Instant,
}

impl AppState {
    pub fn new() -> Self {
        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "~".to_string());

        let config = HakariConfig::load();

        Self {
            messages: Vec::new(),
            input: String::new(),
            cursor_pos: 0,
            scroll_offset: 0,
            mode: AppMode::Normal,
            permission_mode: PermissionMode::Default,
            model_name: "auto (selected by Shizuka)".to_string(),
            theme: Theme::dark(),
            token_usage: TokenUsage {
                input_tokens: 0,
                output_tokens: 0,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
            },
            is_loading: false,
            loading_text: "Thinking".to_string(),
            spinner_frame: 0,
            slash_commands: SlashCommand::all_commands(),
            slash_filter: String::new(),
            slash_selected: 0,
            slash_scroll: 0,
            permission_request: None,
            cwd,
            session_start: Instant::now(),
            last_response_duration: None,
            show_welcome: true,
            input_history: Vec::new(),
            history_index: None,
            show_turn_duration: true,
            should_quit: false,
            welcome_anim_frame: 0,
            shimmer_offset: 0.0,
            compact_notifications: Vec::new(),
            last_esc_time: None,
            streaming: None,
            context_window_percent: 0.0,
            model_options: Vec::new(),
            settings: SettingEntry::defaults(),
            user_scrolled: false,
            total_message_lines: 0,
            message_row_starts: Vec::new(),
            input_scroll: 0,
            pending_stream: None,
            agent_phase: AgentPhase::Idle,
            agent_response_start: None,
            config,
            connect_state: None,
            agent_cancel_tx: None,
            shizuka_block_rows: std::collections::HashMap::new(),
            session_id: None,
            session_picker_sessions: Vec::new(),
            file_mention_active: false,
            file_mention_filter: String::new(),
            file_mention_options: Vec::new(),
            file_mention_selected: 0,
            model_custom_input: String::new(),
            model_custom_cursor: 0,
            model_picker_typing: false,
            model_picker_target: String::new(),
            dialog: DialogState::new(),
            copilot_usage: CopilotUsage::default(),
        }
    }

    pub fn filtered_commands(&self) -> Vec<&SlashCommand> {
        if self.slash_filter.is_empty() {
            self.slash_commands
                .iter()
                .filter(|c| c.is_enabled)
                .collect()
        } else {
            self.slash_commands
                .iter()
                .filter(|c| c.is_enabled && c.name.starts_with(&self.slash_filter))
                .collect()
        }
    }

    pub fn add_user_message(&mut self, text: String) {
        self.messages.push(Message {
            role: MessageRole::User,
            content: vec![MessageContent::Text(text)],
            timestamp: Instant::now(),
        });
    }

    pub fn format_model_short(&self) -> String {
        let name = &self.model_name;
        if name.contains("opus") {
            "Opus".to_string()
        } else if name.contains("sonnet") && name.contains("fast") {
            "Sonnet (fast)".to_string()
        } else if name.contains("sonnet") {
            "Sonnet".to_string()
        } else if name.contains("haiku") {
            "Haiku".to_string()
        } else {
            name.split('-').next().unwrap_or(name).to_string()
        }
    }

    pub fn format_duration(d: Duration) -> String {
        let secs = d.as_secs();
        if secs >= 60 {
            format!("{}m {}s", secs / 60, secs % 60)
        } else {
            format!("{}s", secs)
        }
    }

    pub fn format_token_count(n: u64) -> String {
        if n >= 1_000_000 {
            format!("{:.1}M", n as f64 / 1_000_000.0)
        } else if n >= 1_000 {
            format!("{:.1}K", n as f64 / 1_000.0)
        } else {
            format!("{}", n)
        }
    }

    pub fn scan_files_for_mention(&self, filter: &str) -> Vec<String> {
        let root = std::path::Path::new(&self.cwd);
        let mut files = Vec::new();
        let walker = ignore::WalkBuilder::new(root)
            .hidden(true)
            .git_ignore(true)
            .max_depth(Some(6))
            .build();
        for entry in walker.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Ok(rel) = path.strip_prefix(root) {
                    let rel_str = rel.display().to_string();
                    if rel_str.starts_with("target/") || rel_str.starts_with("node_modules/") {
                        continue;
                    }
                    if filter.is_empty() || rel_str.contains(filter) {
                        files.push(rel_str);
                    }
                }
            }
        }
        files.sort();
        files.truncate(20);
        files
    }
}
