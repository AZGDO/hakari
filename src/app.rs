use crate::auth::copilot;
use crate::config::{HakariConfig, ModelCategory, ReasoningLevel};
use crate::llm::client::LlmClient;
use crate::memory::kkm::Kkm;
use crate::memory::kms::Kms;
use crate::memory::kpms::Kpms;
use crate::memory::improvement;
use crate::nano::agent::{AgentEvent, NanoAgent};
use crate::project::detector;
use crate::shizuka::preparation;
use crate::tui::commands;
use crate::tui::event::{self, AppEvent};
use crate::tui::layout::AppLayout;
use crate::tui::theme::Theme;
use crate::tui::widgets::header::{self, AuthDisplay, HeaderData};
use crate::tui::widgets::input_bar::InputBar;
use crate::tui::widgets::message_list::{ChatMessage, MessageList, MessageType};
use crate::tui::widgets::popup::{ConnectState, ModelEntry, ModelListDisplay, ModelTarget, Popup, PopupType, SettingEntry};
use crate::tui::widgets::progress::Spinner;
use crate::tui::widgets::status_bar::{self, AgentStatus, StatusBarData};
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

pub enum AppMode {
    Input,
    Scrolling,
    Popup,
}

enum PopupEnterAction {
    SelectModel(String, ModelTarget),
    SelectReasoning(String),
    StartSettingsEdit,
    Dismiss,
}

pub struct App {
    pub project_dir: PathBuf,
    pub config: Arc<HakariConfig>,
    pub llm_client: Option<Arc<LlmClient>>,
    pub kms: Kms,
    pub kpms: Kpms,
    pub kkm: Kkm,
    pub input_bar: InputBar,
    pub message_list: MessageList,
    pub status: StatusBarData,
    pub mode: AppMode,
    pub popup: Option<Popup>,
    pub spinner: Spinner,
    pub running: bool,
    pub agent_running: bool,
    pub agent_rx: Option<mpsc::UnboundedReceiver<AgentEvent>>,
    pub clipboard: Option<arboard::Clipboard>,
    pub connect_polling: bool,
    pub connect_state: Option<copilot::DeviceFlowState>,
    pub connect_rx: Option<mpsc::UnboundedReceiver<ConnectEvent>>,
    pub model_rx: Option<mpsc::UnboundedReceiver<Vec<copilot::CopilotModel>>>,
    pub usage_rx: Option<mpsc::UnboundedReceiver<copilot::CopilotUsage>>,
    pub copilot_usage: Option<copilot::CopilotUsage>,
    pub tick_count: u64,
    pub model_fetch_target: ModelTarget,
    pub welcome_shown: bool,
    pub reinstall_pending: bool,
}

#[derive(Debug)]
pub enum ConnectEvent {
    FlowStarted(copilot::DeviceFlowState),
    TokenReceived,
    Error(String),
    Pending,
}

impl App {
    pub fn new(project_dir: PathBuf, config: HakariConfig) -> Self {
        let session_id = uuid::Uuid::new_v4().to_string();
        let kms = Kms::new(session_id.clone());
        let kpms = Kpms::load(&project_dir).unwrap_or_default();
        let kkm = Kkm::load().unwrap_or_default();
        let config = Arc::new(config);
        let llm_client = LlmClient::new(&config).ok().map(Arc::new);
        let clipboard = arboard::Clipboard::new().ok();

        let mut app = Self {
            project_dir: project_dir.clone(),
            config,
            llm_client,
            kms,
            kpms,
            kkm,
            input_bar: InputBar::new(),
            message_list: MessageList::new(),
            status: StatusBarData {
                classification: None,
                step: 0,
                max_steps: 0,
                context_tokens: 0,
                status: AgentStatus::Ready,
            },
            mode: AppMode::Input,
            popup: None,
            spinner: Spinner::new(""),
            running: true,
            agent_running: false,
            agent_rx: None,
            clipboard,
            connect_polling: false,
            connect_state: None,
            connect_rx: None,
            model_rx: None,
            usage_rx: None,
            copilot_usage: None,
            tick_count: 0,
            model_fetch_target: ModelTarget::Nano,
            welcome_shown: false,
            reinstall_pending: false,
        };

        if app.kpms.project.name.is_empty() {
            let detected = detector::detect_project(&project_dir);
            app.kpms.project = detected;
            let _ = app.kpms.save(&project_dir);
        }

        if copilot::is_authenticated() {
            app.start_usage_fetch();
        }

        app
    }

    fn start_usage_fetch(&mut self) {
        let (tx, rx) = mpsc::unbounded_channel();
        self.usage_rx = Some(rx);
        tokio::spawn(async move {
            if let Ok(usage) = copilot::fetch_usage().await {
                let _ = tx.send(usage);
            }
        });
    }

    fn show_welcome(&mut self) {
        if self.welcome_shown {
            return;
        }
        self.welcome_shown = true;

        let ascii_content = match std::fs::read_to_string("ascii.txt") {
            Ok(content) => content,
            Err(_) => "ASCII art not found.".to_string(),
        };

        let lines: Vec<String> = ascii_content.lines().map(|s| s.to_string()).collect();
        let max_width = lines.iter().map(|l| l.len()).max().unwrap_or(0);
        let border_line = "─".repeat(max_width);
        let enhanced_ascii = format!("┌{}┐\n", border_line) +
            &lines.iter().map(|l| format!("│{}│\n", l)).collect::<String>() +
            &format!("└{}┘", border_line);

        self.message_list.add_message(ChatMessage {
            msg_type: MessageType::Welcome,
            content: enhanced_ascii,
            timestamp: None,
            collapsed: false,
        });

        let provider = if copilot::is_authenticated() {
            "GitHub Copilot (connected)"
        } else if self.config.openai_api_key.is_some() {
            "OpenAI API Key"
        } else if self.config.anthropic_api_key.is_some() {
            "Anthropic API Key"
        } else {
            "not configured -- use /connect"
        };

        let mut info = format!(
            "  {} v0.1.0 \u{2502} {} ({}) \u{2502} #{}\n",
            "HAKARI",
            self.kpms.project.name,
            if self.kpms.project.language.is_empty() { "unknown" } else { &self.kpms.project.language },
            &self.kms.session_id[..8],
        );
        info.push_str(&format!(
            "  nano: {} [{}] reason={}\n",
            self.config.nano_model, self.config.nano_category, self.config.nano_reasoning
        ));
        info.push_str(&format!(
            "  shizuka: {} [{}]\n",
            self.config.shizuka_model, self.config.shizuka_category
        ));
        info.push_str(&format!("  provider: {}\n", provider));

        if let Some(ref usage) = self.copilot_usage {
            if usage.limit > 0 {
                info.push_str(&format!(
                    "  copilot: {:.0}% left ({}/{})\n",
                    usage.percent_left, usage.requests_left, usage.limit
                ));
            }
        }

        info.push_str("\n  Type a task, use / for commands, @ to mention files.");

        self.message_list.add_message(ChatMessage {
            msg_type: MessageType::System,
            content: info,
            timestamp: None,
            collapsed: false,
        });

        if !copilot::is_authenticated() && self.llm_client.is_none() {
            self.message_list.add_message(ChatMessage {
                msg_type: MessageType::System,
                content: "  Use /connect to authenticate, or set OPENAI_API_KEY / ANTHROPIC_API_KEY.".to_string(),
                timestamp: None,
                collapsed: false,
            });
        }
    }

    pub fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Key(key) => self.handle_key(key),
            AppEvent::Mouse(mouse) => self.handle_mouse(mouse),
            AppEvent::Resize(_, _) => {}
            AppEvent::Tick => self.handle_tick(),
        }
    }

    fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        if event::is_quit(&key) {
            self.running = false;
            return;
        }

        // Popup mode
        if self.popup.is_some() {
            self.handle_popup_key(key);
            return;
        }

        if key.code == KeyCode::Esc {
            self.mode = AppMode::Input;
            self.message_list.scroll_to_bottom();
            return;
        }

        if event::is_copy(&key) {
            self.do_copy();
            return;
        }

        if event::is_paste(&key) {
            self.do_paste();
            return;
        }

        match &self.mode {
            AppMode::Input => self.handle_input_key(key),
            AppMode::Scrolling => self.handle_scroll_key(key),
            AppMode::Popup => {}
        }
    }

    fn handle_popup_key(&mut self, key: crossterm::event::KeyEvent) {
        // If settings popup is in edit mode, route all keys to the editor
        if let Some(ref popup) = self.popup {
            if popup.settings_is_editing() {
                match key.code {
                    KeyCode::Enter => {
                        if let Some(popup) = &mut self.popup {
                            if let Some((k, v)) = popup.settings_commit_edit() {
                                self.apply_setting(&k, &v);
                            }
                        }
                    }
                    KeyCode::Esc => {
                        if let Some(popup) = &mut self.popup {
                            popup.settings_cancel_edit();
                        }
                    }
                    KeyCode::Backspace => {
                        if let Some(popup) = &mut self.popup {
                            popup.settings_edit_backspace();
                        }
                    }
                    KeyCode::Char(c) => {
                        if let Some(popup) = &mut self.popup {
                            popup.settings_edit_push(c);
                        }
                    }
                    _ => {}
                }
                return;
            }
        }

        // Handle connect menu enter first
        if let Some(ref popup) = self.popup {
            if matches!(popup.popup_type, PopupType::ConnectMenu { .. }) {
                match key.code {
                    KeyCode::Esc => {
                        self.popup = None;
                        self.mode = AppMode::Input;
                        return;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if let Some(ref mut popup) = self.popup {
                            popup.select_up();
                        }
                        return;
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if let Some(ref mut popup) = self.popup {
                            popup.select_down();
                        }
                        return;
                    }
                    KeyCode::Enter => {
                        self.handle_connect_menu_enter();
                        return;
                    }
                    _ => return,
                }
            }
        }

        match key.code {
            KeyCode::Esc => {
                self.popup = None;
                self.connect_polling = false;
                self.mode = AppMode::Input;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(ref mut popup) = self.popup {
                    popup.select_up();
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(ref mut popup) = self.popup {
                    popup.select_down();
                }
            }
            KeyCode::Enter => {
                self.handle_popup_enter();
            }
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(ref popup) = self.popup {
                    if matches!(popup.popup_type, PopupType::Confirmation { .. }) {
                        self.popup = None;
                        self.mode = AppMode::Input;
                    }
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                if let Some(ref popup) = self.popup {
                    if matches!(popup.popup_type, PopupType::Confirmation { .. }) {
                        self.popup = None;
                        self.mode = AppMode::Input;
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_connect_menu_enter(&mut self) {
        let selected = if let Some(ref popup) = self.popup {
            if let PopupType::ConnectMenu { selected } = &popup.popup_type {
                *selected
            } else {
                return;
            }
        } else {
            return;
        };

        match selected {
            0 => {
                self.popup = None;
                self.start_connect_flow();
            }
            1 => {
                self.popup = None;
                self.mode = AppMode::Input;
                self.message_list.add_message(ChatMessage {
                    msg_type: MessageType::System,
                    content: "  Set OPENAI_API_KEY and optionally OPENAI_BASE_URL.\n  Restart HAKARI after setting.".to_string(),
                    timestamp: None,
                    collapsed: false,
                });
            }
            _ => {}
        }
    }

    fn handle_popup_enter(&mut self) {
        let action = if let Some(ref popup) = self.popup {
            match &popup.popup_type {
                PopupType::ModelSelector { models, selected, target, .. } => {
                    models.get(*selected).map(|m| PopupEnterAction::SelectModel(m.id.clone(), target.clone()))
                }
                PopupType::ReasoningSelector { levels, selected } => {
                    levels.get(*selected).map(|l| PopupEnterAction::SelectReasoning(l.clone()))
                }
                PopupType::Help | PopupType::ModelList { .. }
                | PopupType::Escalation { .. } | PopupType::ConnectFlow { .. } => {
                    Some(PopupEnterAction::Dismiss)
                }
                PopupType::Settings { .. } => {
                    Some(PopupEnterAction::StartSettingsEdit)
                }
                _ => None,
            }
        } else {
            None
        };

        match action {
            Some(PopupEnterAction::SelectModel(model_id, target)) => {
                match target {
                    ModelTarget::Nano => {
                        self.set_model(&model_id);
                        self.message_list.add_message(ChatMessage {
                            msg_type: MessageType::System,
                            content: format!("  Nano model set to: {}", model_id),
                            timestamp: None,
                            collapsed: false,
                        });
                    }
                    ModelTarget::Shizuka => {
                        self.set_shizuka_model(&model_id);
                        self.message_list.add_message(ChatMessage {
                            msg_type: MessageType::System,
                            content: format!("  Shizuka model set to: {}", model_id),
                            timestamp: None,
                            collapsed: false,
                        });
                    }
                }
                self.popup = None;
                self.mode = AppMode::Input;
            }
            Some(PopupEnterAction::SelectReasoning(level)) => {
                self.set_reasoning(&level);
                self.message_list.add_message(ChatMessage {
                    msg_type: MessageType::System,
                    content: format!("  Reasoning set to: {}", level),
                    timestamp: None,
                    collapsed: false,
                });
                self.popup = None;
                self.mode = AppMode::Input;
            }
            Some(PopupEnterAction::StartSettingsEdit) => {
                if let Some(ref mut popup) = self.popup {
                    popup.settings_start_edit();
                }
            }
            Some(PopupEnterAction::Dismiss) => {
                self.popup = None;
                self.connect_polling = false;
                self.mode = AppMode::Input;
            }
            None => {}
        }
    }

    fn handle_input_key(&mut self, key: crossterm::event::KeyEvent) {
        if self.input_bar.has_suggestions() {
            match key.code {
                KeyCode::Tab | KeyCode::Enter if self.input_bar.has_suggestions() && key.code == KeyCode::Tab => {
                    self.input_bar.accept_suggestion();
                    return;
                }
                KeyCode::Up => {
                    self.input_bar.suggestion_up();
                    return;
                }
                KeyCode::Down => {
                    self.input_bar.suggestion_down();
                    return;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Enter if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                if self.input_bar.has_suggestions() {
                    if !self.input_bar.slash_suggestions.is_empty() {
                        self.input_bar.accept_suggestion();
                        return;
                    }
                }
                if let Some(text) = self.input_bar.submit() {
                    self.process_input(text);
                }
            }
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.input_bar.insert_newline();
            }
            KeyCode::Tab => {
                if self.input_bar.has_suggestions() {
                    self.input_bar.accept_suggestion();
                } else {
                    self.input_bar.insert_str("  ");
                }
            }
            KeyCode::Backspace => self.input_bar.delete_char_before(),
            KeyCode::Delete => self.input_bar.delete_char_after(),
            KeyCode::Left => self.input_bar.move_cursor_left(),
            KeyCode::Right => self.input_bar.move_cursor_right(),
            KeyCode::Home => self.input_bar.move_cursor_home(),
            KeyCode::End => self.input_bar.move_cursor_end(),
            KeyCode::Up => {
                if !self.input_bar.has_suggestions() {
                    self.input_bar.history_prev();
                }
            }
            KeyCode::Down => {
                if !self.input_bar.has_suggestions() {
                    self.input_bar.history_next();
                }
            }
            KeyCode::PageUp => {
                self.message_list.page_up(10);
                self.mode = AppMode::Scrolling;
            }
            KeyCode::PageDown => {
                self.message_list.page_down(10);
            }
            KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input_bar.delete_word_before();
            }
            KeyCode::Char(c) => {
                self.input_bar.insert_char(c);
                self.input_bar.update_file_suggestions(&self.project_dir);
            }
            _ => {}
        }
    }

    fn handle_scroll_key(&mut self, key: crossterm::event::KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.message_list.scroll_down(1),
            KeyCode::Char('k') | KeyCode::Up => self.message_list.scroll_up(1),
            KeyCode::PageUp => self.message_list.page_up(10),
            KeyCode::PageDown => self.message_list.page_down(10),
            KeyCode::Home | KeyCode::Char('g') => {
                self.message_list.scroll_offset = 0;
                self.message_list.auto_scroll = false;
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.message_list.scroll_to_bottom();
                self.mode = AppMode::Input;
            }
            KeyCode::Char(c) => {
                self.mode = AppMode::Input;
                self.message_list.scroll_to_bottom();
                self.input_bar.insert_char(c);
            }
            _ => {}
        }
    }

    fn handle_mouse(&mut self, mouse: crossterm::event::MouseEvent) {
        if event::is_scroll_up(&mouse) {
            self.message_list.scroll_up(3);
            if !matches!(self.mode, AppMode::Popup) {
                self.mode = AppMode::Scrolling;
            }
        } else if event::is_scroll_down(&mouse) {
            self.message_list.scroll_down(3);
        }
    }

    fn handle_tick(&mut self) {
        self.tick_count += 1;

        // Show welcome on first tick so usage data can load
        if !self.welcome_shown && self.tick_count >= 3 {
            self.show_welcome();
        }

        if self.agent_running {
            self.spinner.tick();
        }

        self.process_agent_events();
        self.process_connect_events();
        self.process_model_events();
        self.process_usage_events();
    }

    fn process_agent_events(&mut self) {
        let events: Vec<AgentEvent> = if let Some(ref mut rx) = self.agent_rx {
            let mut collected = Vec::new();
            while let Ok(event) = rx.try_recv() {
                collected.push(event);
            }
            collected
        } else {
            Vec::new()
        };

        for event in events {
            match event {
                AgentEvent::ThinkingStart => {
                    self.status.status = AgentStatus::Thinking;
                    self.spinner.label = "thinking...".to_string();
                }
                AgentEvent::TextDelta(text) => {
                    if text.is_empty() { continue; }
                    if let Some(last) = self.message_list.messages.last() {
                        if matches!(last.msg_type, MessageType::Nano) {
                            self.message_list.append_to_last(&text);
                        } else {
                            self.message_list.add_message(ChatMessage {
                                msg_type: MessageType::Nano,
                                content: text,
                                timestamp: None,
                                collapsed: false,
                            });
                        }
                    } else {
                        self.message_list.add_message(ChatMessage {
                            msg_type: MessageType::Nano,
                            content: text,
                            timestamp: None,
                            collapsed: false,
                        });
                    }
                }
                AgentEvent::ToolCallStart { name, .. } => {
                    self.status.status = AgentStatus::ToolRunning(name.clone());
                    self.spinner.label = format!("{}...", name);
                }
                AgentEvent::ToolCallEnd { name, result, success } => {
                    self.message_list.add_message(ChatMessage {
                        msg_type: MessageType::ToolResult { name, success },
                        content: result,
                        timestamp: None,
                        collapsed: false,
                    });
                    self.status.step = self.kms.steps.current;
                    self.status.context_tokens = self.kms.context.total_tokens_estimate;
                }
                AgentEvent::Warning(msg) => {
                    self.message_list.add_message(ChatMessage {
                        msg_type: MessageType::Warning,
                        content: msg,
                        timestamp: None,
                        collapsed: false,
                    });
                }
                AgentEvent::Escalation(summary) => {
                    self.popup = Some(Popup::escalation(&summary));
                    self.mode = AppMode::Popup;
                }
                AgentEvent::Complete(_) => {
                    self.agent_running = false;
                    self.status.status = AgentStatus::Complete;
                    self.persist_session_data();
                }
                AgentEvent::Error(msg) => {
                    self.message_list.add_message(ChatMessage {
                        msg_type: MessageType::Error,
                        content: msg,
                        timestamp: None,
                        collapsed: false,
                    });
                    self.agent_running = false;
                    self.status.status = AgentStatus::Error;
                }
            }
        }
    }

    fn process_connect_events(&mut self) {
        let events: Vec<ConnectEvent> = if let Some(ref mut rx) = self.connect_rx {
            let mut collected = Vec::new();
            while let Ok(event) = rx.try_recv() {
                collected.push(event);
            }
            collected
        } else {
            Vec::new()
        };

        for event in events {
            match event {
                ConnectEvent::FlowStarted(state) => {
                    self.connect_state = Some(state.clone());
                    if let Some(ref mut popup) = self.popup {
                        popup.popup_type = PopupType::ConnectFlow {
                            state: ConnectState::WaitingForAuth {
                                uri: state.verification_uri,
                                code: state.user_code,
                            },
                        };
                    }
                }
                ConnectEvent::TokenReceived => {
                    self.connect_polling = false;
                    if let Some(ref mut popup) = self.popup {
                        popup.popup_type = PopupType::ConnectFlow {
                            state: ConnectState::Success,
                        };
                    }
                    self.rebuild_llm_client();
                    self.start_usage_fetch();
                }
                ConnectEvent::Error(e) => {
                    self.connect_polling = false;
                    if let Some(ref mut popup) = self.popup {
                        popup.popup_type = PopupType::ConnectFlow {
                            state: ConnectState::Error(e),
                        };
                    }
                }
                ConnectEvent::Pending => {}
            }
        }
    }

    fn process_model_events(&mut self) {
        if let Some(ref mut rx) = self.model_rx {
            if let Ok(models) = rx.try_recv() {
                let current = match self.model_fetch_target {
                    ModelTarget::Nano => &self.config.nano_model,
                    ModelTarget::Shizuka => &self.config.shizuka_model,
                };
                let entries: Vec<ModelEntry> = models.iter().map(|m| ModelEntry {
                    id: m.id.clone(),
                    name: m.name.clone(),
                    reasoning: m.reasoning,
                    context: m.limit.as_ref().map(|l| l.context).unwrap_or(0),
                    active: m.id == *current,
                    input_rate: m.input_rate,
                    output_rate: m.output_rate,
                    category: categorize_model(&m.id),
                }).collect();
                self.popup = Some(Popup::model_selector(entries, current, self.model_fetch_target.clone()));
                self.mode = AppMode::Popup;
                self.model_rx = None;
            }
        }
    }

    fn process_usage_events(&mut self) {
        if let Some(ref mut rx) = self.usage_rx {
            if let Ok(usage) = rx.try_recv() {
                self.copilot_usage = Some(usage);
                self.usage_rx = None;
            }
        }
    }

    fn process_input(&mut self, input: String) {
        if let Some((cmd, args)) = commands::parse_command(&input) {
            self.execute_command(cmd, args);
            return;
        }

        let mentions = commands::extract_at_mentions(&input);
        for mention in &mentions {
            self.input_bar.pin_file(mention);
        }

        self.submit_prompt(input);
    }

    fn execute_command(&mut self, cmd: &str, args: &str) {
        match cmd {
            "/help" | "/?" => {
                self.popup = Some(Popup::help());
                self.mode = AppMode::Popup;
            }
            "/clear" => {
                self.message_list.messages.clear();
                self.message_list.scroll_offset = 0;
                self.message_list.add_message(ChatMessage {
                    msg_type: MessageType::System,
                    content: "  Chat cleared.".to_string(),
                    timestamp: None,
                    collapsed: false,
                });
            }
            "/compact" => {
                self.message_list.collapse_all_traces();
                self.message_list.add_message(ChatMessage {
                    msg_type: MessageType::System,
                    content: "  All traces collapsed.".to_string(),
                    timestamp: None,
                    collapsed: false,
                });
            }
            "/model" | "/models" => {
                self.show_model_selector(args, ModelTarget::Nano);
            }
            "/shizuka" => {
                self.show_model_selector(args, ModelTarget::Shizuka);
            }
            "/reasoning" => {
                self.show_reasoning_selector(args);
            }
            "/modellist" => {
                self.show_model_list();
            }
            "/connect" => {
                self.popup = Some(Popup::connect_menu());
                self.mode = AppMode::Popup;
            }
            "/settings" => {
                self.show_settings();
            }
            "/status" => {
                let auth = if copilot::is_authenticated() { "connected" } else { "not connected" };
                let mut status = format!(
                    "  Session: #{}\n  Nano: {} [{}] reason={}\n  Shizuka: {} [{}]\n  Auth: {}\n  Steps: {}\n  Files modified: {}\n  Pinned: {}",
                    &self.kms.session_id[..8],
                    self.config.nano_model,
                    self.config.nano_category,
                    self.config.nano_reasoning,
                    self.config.shizuka_model,
                    self.config.shizuka_category,
                    auth,
                    self.kms.steps.current,
                    self.kms.files.index.values().filter(|f| f.is_modified).count(),
                    self.input_bar.pinned_files.join(", "),
                );
                if let Some(ref usage) = self.copilot_usage {
                    if usage.limit > 0 {
                        status.push_str(&format!("\n  Copilot: {:.0}% left ({}/{})",
                            usage.percent_left, usage.requests_left, usage.limit));
                    }
                }
                self.message_list.add_message(ChatMessage {
                    msg_type: MessageType::System,
                    content: status,
                    timestamp: None,
                    collapsed: false,
                });
            }
            "/reset" => {
                let session_id = uuid::Uuid::new_v4().to_string();
                self.kms = Kms::new(session_id);
                self.status.step = 0;
                self.status.context_tokens = 0;
                self.status.classification = None;
                self.message_list.add_message(ChatMessage {
                    msg_type: MessageType::System,
                    content: "  Session reset.".to_string(),
                    timestamp: None,
                    collapsed: false,
                });
            }
            "/pin" => {
                if !args.is_empty() {
                    let file = args.trim_start_matches('@');
                    let path = self.project_dir.join(file);
                    if path.exists() {
                        self.input_bar.pin_file(file);
                        self.message_list.add_message(ChatMessage {
                            msg_type: MessageType::System,
                            content: format!("  Pinned: @{}", file),
                            timestamp: None,
                            collapsed: false,
                        });
                    } else {
                        self.message_list.add_message(ChatMessage {
                            msg_type: MessageType::Warning,
                            content: format!("  File not found: {}", file),
                            timestamp: None,
                            collapsed: false,
                        });
                    }
                }
            }
            "/unpin" => {
                let file = args.trim_start_matches('@');
                self.input_bar.unpin_file(file);
                self.message_list.add_message(ChatMessage {
                    msg_type: MessageType::System,
                    content: format!("  Unpinned: @{}", file),
                    timestamp: None,
                    collapsed: false,
                });
            }
            "/files" => {
                if self.input_bar.pinned_files.is_empty() {
                    self.message_list.add_message(ChatMessage {
                        msg_type: MessageType::System,
                        content: "  No pinned files. Use @filename to pin.".to_string(),
                        timestamp: None,
                        collapsed: false,
                    });
                } else {
                    let list = self.input_bar.pinned_files.iter()
                        .map(|f| format!("    @{}", f))
                        .collect::<Vec<_>>()
                        .join("\n");
                    self.message_list.add_message(ChatMessage {
                        msg_type: MessageType::System,
                        content: format!("  Pinned files:\n{}", list),
                        timestamp: None,
                        collapsed: false,
                    });
                }
            }
            "/diff" => {
                let diffs: Vec<String> = self.kms.files.index.iter()
                    .filter(|(_, info)| info.is_modified)
                    .map(|(path, _)| format!("    M {}", path))
                    .collect();
                if diffs.is_empty() {
                    self.message_list.add_message(ChatMessage {
                        msg_type: MessageType::System,
                        content: "  No files modified this session.".to_string(),
                        timestamp: None,
                        collapsed: false,
                    });
                } else {
                    self.message_list.add_message(ChatMessage {
                        msg_type: MessageType::System,
                        content: format!("  Modified files:\n{}", diffs.join("\n")),
                        timestamp: None,
                        collapsed: false,
                    });
                }
            }
            "/undo" => {
                let undone: Vec<String> = self.kms.files.backups.keys().cloned().collect();
                if undone.is_empty() {
                    self.message_list.add_message(ChatMessage {
                        msg_type: MessageType::System,
                        content: "  Nothing to undo.".to_string(),
                        timestamp: None,
                        collapsed: false,
                    });
                } else {
                    for (path, content) in &self.kms.files.backups {
                        let full = self.project_dir.join(path);
                        let _ = std::fs::write(&full, content);
                    }
                    self.message_list.add_message(ChatMessage {
                        msg_type: MessageType::System,
                        content: format!("  Restored {} file(s).", undone.len()),
                        timestamp: None,
                        collapsed: false,
                    });
                }
            }
            "/cost" => {
                self.message_list.add_message(ChatMessage {
                    msg_type: MessageType::System,
                    content: format!(
                        "  Context: ~{} tokens\n  Steps: {}",
                        self.kms.context.total_tokens_estimate,
                        self.kms.steps.current,
                    ),
                    timestamp: None,
                    collapsed: false,
                });
            }
            "/export" => {
                let path = if args.is_empty() { "hakari-chat.txt" } else { args };
                let content: String = self.message_list.messages.iter()
                    .map(|m| m.content.as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n---\n\n");
                match std::fs::write(self.project_dir.join(path), &content) {
                    Ok(_) => {
                        self.message_list.add_message(ChatMessage {
                            msg_type: MessageType::System,
                            content: format!("  Exported to {}", path),
                            timestamp: None,
                            collapsed: false,
                        });
                    }
                    Err(e) => {
                        self.message_list.add_message(ChatMessage {
                            msg_type: MessageType::Error,
                            content: format!("  Export failed: {}", e),
                            timestamp: None,
                            collapsed: false,
                        });
                    }
                }
            }
            "/reinstall" => {
                self.message_list.add_message(ChatMessage {
                    msg_type: MessageType::System,
                    content: "  Reinstalling hakari...".to_string(),
                    timestamp: None,
                    collapsed: false,
                });
                self.reinstall_pending = true;
                self.running = false;
            }
            "/exit" | "/quit" => {
                self.running = false;
            }
            _ => {
                self.message_list.add_message(ChatMessage {
                    msg_type: MessageType::Warning,
                    content: format!("  Unknown command: {}. Type /help for commands.", cmd),
                    timestamp: None,
                    collapsed: false,
                });
            }
        }
    }

    fn show_model_selector(&mut self, args: &str, target: ModelTarget) {
        if !args.is_empty() {
            match target {
                ModelTarget::Nano => {
                    self.set_model(args);
                    self.message_list.add_message(ChatMessage {
                        msg_type: MessageType::System,
                        content: format!("  Nano model set to: {}", args),
                        timestamp: None,
                        collapsed: false,
                    });
                }
                ModelTarget::Shizuka => {
                    self.set_shizuka_model(args);
                    self.message_list.add_message(ChatMessage {
                        msg_type: MessageType::System,
                        content: format!("  Shizuka model set to: {}", args),
                        timestamp: None,
                        collapsed: false,
                    });
                }
            }
            return;
        }

        self.model_fetch_target = target.clone();
        self.popup = Some(Popup::model_selector_loading(target));
        self.mode = AppMode::Popup;

        let (tx, rx) = mpsc::unbounded_channel();
        self.model_rx = Some(rx);

        tokio::spawn(async move {
            match copilot::fetch_models().await {
                Ok(models) => { let _ = tx.send(models); }
                Err(e) => {
                    tracing::warn!("Failed to fetch models: {}", e);
                    let _ = tx.send(Vec::new());
                }
            }
        });
    }

    fn show_reasoning_selector(&mut self, args: &str) {
        if !args.is_empty() {
            self.set_reasoning(args);
            self.message_list.add_message(ChatMessage {
                msg_type: MessageType::System,
                content: format!("  Reasoning set to: {}", args),
                timestamp: None,
                collapsed: false,
            });
            return;
        }

        let levels = vec![
            "none".to_string(),
            "low".to_string(),
            "medium".to_string(),
            "high".to_string(),
            "xhigh".to_string(),
        ];
        self.popup = Some(Popup::reasoning_selector(levels, &self.config.nano_reasoning.to_string()));
        self.mode = AppMode::Popup;
    }

    fn show_model_list(&mut self) {
        let mut entries: Vec<ModelListDisplay> = Vec::new();

        entries.push(ModelListDisplay {
            demand: "Nano (agent)".to_string(),
            model_id: self.config.nano_model.clone(),
            category: self.config.nano_category.to_string(),
            reasoning: self.config.nano_reasoning.to_string(),
        });
        entries.push(ModelListDisplay {
            demand: "Shizuka (prep)".to_string(),
            model_id: self.config.shizuka_model.clone(),
            category: self.config.shizuka_category.to_string(),
            reasoning: "n/a".to_string(),
        });

        for entry in &self.config.model_list {
            entries.push(ModelListDisplay {
                demand: entry.demand.clone(),
                model_id: entry.model_id.clone(),
                category: entry.category.to_string(),
                reasoning: entry.reasoning.to_string(),
            });
        }

        self.popup = Some(Popup::model_list(entries));
        self.mode = AppMode::Popup;
    }

    fn show_settings(&mut self) {
        let entries = vec![
            SettingEntry {
                key: "nano_model".to_string(),
                label: "Nano Model".to_string(),
                value: self.config.nano_model.clone(),
                editable: true,
            },
            SettingEntry {
                key: "nano_category".to_string(),
                label: "Nano Category".to_string(),
                value: self.config.nano_category.to_string(),
                editable: true,
            },
            SettingEntry {
                key: "nano_reasoning".to_string(),
                label: "Nano Reasoning".to_string(),
                value: self.config.nano_reasoning.to_string(),
                editable: true,
            },
            SettingEntry {
                key: "shizuka_model".to_string(),
                label: "Shizuka Model".to_string(),
                value: self.config.shizuka_model.clone(),
                editable: true,
            },
            SettingEntry {
                key: "shizuka_category".to_string(),
                label: "Shizuka Category".to_string(),
                value: self.config.shizuka_category.to_string(),
                editable: true,
            },
            SettingEntry {
                key: "nano_provider".to_string(),
                label: "Nano Provider".to_string(),
                value: format!("{:?}", self.config.nano_provider),
                editable: true,
            },
            SettingEntry {
                key: "max_context".to_string(),
                label: "Max Context".to_string(),
                value: format!("{}", self.config.max_context_tokens),
                editable: true,
            },
            SettingEntry {
                key: "auth".to_string(),
                label: "GitHub Copilot".to_string(),
                value: if copilot::is_authenticated() { "Connected".to_string() } else { "Not connected".to_string() },
                editable: false,
            },
            SettingEntry {
                key: "copilot_usage".to_string(),
                label: "Copilot Usage".to_string(),
                value: match &self.copilot_usage {
                    Some(u) if u.limit > 0 => format!("{:.0}% left ({}/{})", u.percent_left, u.requests_left, u.limit),
                    _ => "N/A".to_string(),
                },
                editable: false,
            },
            SettingEntry {
                key: "project".to_string(),
                label: "Project".to_string(),
                value: self.kpms.project.name.clone(),
                editable: false,
            },
            SettingEntry {
                key: "language".to_string(),
                label: "Language".to_string(),
                value: self.kpms.project.language.clone(),
                editable: false,
            },
        ];
        self.popup = Some(Popup::settings(entries));
        self.mode = AppMode::Popup;
    }

    fn start_connect_flow(&mut self) {
        self.popup = Some(Popup::connect_flow());
        self.mode = AppMode::Popup;
        self.connect_polling = true;

        let (tx, rx) = mpsc::unbounded_channel();
        self.connect_rx = Some(rx);

        tokio::spawn(async move {
            match copilot::start_device_flow().await {
                Ok(state) => {
                    let _ = tx.send(ConnectEvent::FlowStarted(state.clone()));
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(state.interval + 2)).await;
                        match copilot::poll_for_token(&state).await {
                            Ok(Some(_)) => {
                                let _ = tx.send(ConnectEvent::TokenReceived);
                                break;
                            }
                            Ok(None) => {
                                let _ = tx.send(ConnectEvent::Pending);
                            }
                            Err(e) => {
                                let _ = tx.send(ConnectEvent::Error(e.to_string()));
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(ConnectEvent::Error(e.to_string()));
                }
            }
        });
    }

    fn apply_setting(&mut self, key: &str, value: &str) {
        match key {
            "nano_model" => self.set_model(value),
            "nano_reasoning" => self.set_reasoning(value),
            "shizuka_model" => self.set_shizuka_model(value),
            "nano_category" => {
                let mut config = (*self.config).clone();
                config.nano_category = parse_model_category(value);
                self.config = Arc::new(config);
            }
            "shizuka_category" => {
                let mut config = (*self.config).clone();
                config.shizuka_category = parse_model_category(value);
                self.config = Arc::new(config);
            }
            "max_context" => {
                if let Ok(n) = value.parse::<usize>() {
                    let mut config = (*self.config).clone();
                    config.max_context_tokens = n;
                    self.config = Arc::new(config);
                }
            }
            _ => {}
        }
        self.rebuild_llm_client();
    }

    fn set_model(&mut self, model_id: &str) {
        let mut config = (*self.config).clone();
        config.nano_model = model_id.to_string();
        config.nano_category = parse_model_category(model_id);
        config.nano_reasoning = ReasoningLevel::default_for_model(model_id);
        self.config = Arc::new(config);
        self.rebuild_llm_client();
    }

    fn set_shizuka_model(&mut self, model_id: &str) {
        let mut config = (*self.config).clone();
        config.shizuka_model = model_id.to_string();
        config.shizuka_category = parse_model_category(model_id);
        self.config = Arc::new(config);
        self.rebuild_llm_client();
    }

    fn set_reasoning(&mut self, level: &str) {
        let mut config = (*self.config).clone();
        config.nano_reasoning = match level {
            "none" => ReasoningLevel::None,
            "low" => ReasoningLevel::Low,
            "medium" => ReasoningLevel::Medium,
            "high" => ReasoningLevel::High,
            "xhigh" => ReasoningLevel::XHigh,
            _ => ReasoningLevel::High,
        };
        self.config = Arc::new(config);
    }

    fn rebuild_llm_client(&mut self) {
        let mut config = (*self.config).clone();
        if let Some(token) = copilot::get_token() {
            let base = copilot::copilot_base_url();
            config.openai_api_key = Some(token.clone());
            config.openai_base_url = base.clone();
        }
        self.config = Arc::new(config);
        self.llm_client = LlmClient::new(&self.config).ok().map(Arc::new);
    }

    fn do_copy(&mut self) {
        if let Some(ref mut clipboard) = self.clipboard {
            let text: String = self.message_list.messages.iter()
                .map(|m| m.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");
            let _ = clipboard.set_text(text);
        }
    }

    fn do_paste(&mut self) {
        if let Some(ref mut clipboard) = self.clipboard {
            if let Ok(text) = clipboard.get_text() {
                self.input_bar.insert_str(&text);
            }
        }
    }

    fn submit_prompt(&mut self, prompt: String) {
        if self.agent_running {
            self.message_list.add_message(ChatMessage {
                msg_type: MessageType::Warning,
                content: "  Agent is running. Please wait.".to_string(),
                timestamp: None,
                collapsed: false,
            });
            return;
        }

        if self.llm_client.is_none() {
            self.rebuild_llm_client();
        }

        let Some(ref llm_client) = self.llm_client else {
            self.message_list.add_message(ChatMessage {
                msg_type: MessageType::Error,
                content: "  No LLM configured. Use /connect or set API keys.".to_string(),
                timestamp: None,
                collapsed: false,
            });
            return;
        };

        self.message_list.add_message(ChatMessage {
            msg_type: MessageType::User,
            content: prompt.clone(),
            timestamp: None,
            collapsed: false,
        });

        self.kms.task.original_prompt = prompt.clone();
        self.agent_running = true;
        self.status.status = AgentStatus::Preparing;
        self.spinner.label = "preparing...".to_string();

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        self.agent_rx = Some(event_rx);

        let llm_client = llm_client.clone();
        let config = self.config.clone();
        let project_dir = self.project_dir.clone();
        let kpms = self.kpms.clone();
        let kkm = self.kkm.clone();
        let mut kms = self.kms.clone();
        let pinned = self.input_bar.pinned_files.clone();

        tokio::spawn(async move {
            // Single shizuka request: try fast path first, only call shizuka if needed
            let mut prep = if let Some(fast) = preparation::try_fast_path(&prompt, &project_dir) {
                fast
            } else {
                match preparation::run_preparation(&llm_client, &prompt, &kms, &kpms, &kkm, &project_dir).await {
                    Ok(p) => p,
                    Err(e) => {
                        let _ = event_tx.send(AgentEvent::Error(format!("Preparation failed: {}", e)));
                        return;
                    }
                }
            };

            for file in &pinned {
                if !prep.files_to_preload.contains(file) {
                    prep.files_to_preload.push(file.clone());
                }
            }

            kms.task.goal = prep.kms_updates.goal.clone();
            kms.task.classification = prep.task_classification.clone();
            for (i, sub) in prep.kms_updates.sub_tasks.iter().enumerate() {
                kms.task.sub_tasks.push(crate::memory::kms::SubTask {
                    id: format!("sub_{}", i),
                    description: sub.clone(),
                    status: crate::memory::kms::SubTaskStatus::Pending,
                    assigned_to: "nano".to_string(),
                });
            }

            // Single nano agent run per prompt
            let agent = NanoAgent::new(config, llm_client, project_dir, 0);
            match agent.run(&prep, &mut kms, &kpms, &kkm, event_tx.clone()).await {
                Ok(_) => {}
                Err(e) => {
                    let _ = event_tx.send(AgentEvent::Error(format!("Agent error: {}", e)));
                }
            }
        });
    }

    fn persist_session_data(&mut self) {
        let prep_files: Vec<String> = Vec::new();
        let misses = improvement::collect_preparation_misses(&self.kms, &prep_files, &[]);
        let record = improvement::collect_iteration_record(&self.kms);
        improvement::persist_improvements(&mut self.kpms, &self.kms, &misses, &record, &self.kms.session_id);
        let _ = self.kpms.save(&self.project_dir);
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();

        frame.render_widget(
            ratatui::widgets::Block::default().style(Style::default().bg(Theme::bg())),
            area,
        );

        let input_height = self.input_bar.desired_height();
        let layout = AppLayout::compute(area, input_height);

        // Header
        let auth_display = if copilot::is_authenticated() {
            AuthDisplay::Connected(copilot::token_preview().unwrap_or_default())
        } else if self.config.openai_api_key.is_some() || self.config.anthropic_api_key.is_some() {
            AuthDisplay::Connected("API Key".to_string())
        } else {
            AuthDisplay::NotConnected
        };

        header::render_header(
            frame,
            layout.header,
            &HeaderData {
                project_name: self.kpms.project.name.clone(),
                session_id: self.kms.session_id.clone(),
                has_kpms: !self.kpms.learnings.is_empty() || !self.kpms.file_index.is_empty(),
                has_kkm: !self.kkm.tools.is_empty(),
                model_name: self.config.nano_model.clone(),
                model_category: self.config.nano_category.to_string(),
                reasoning: self.config.nano_reasoning.to_string(),
                shizuka_model: self.config.shizuka_model.clone(),
                auth_status: auth_display,
                copilot_usage: self.copilot_usage.clone(),
            },
        );

        // Messages
        self.message_list.render(frame, layout.messages);

        // Input bar
        self.input_bar.render(frame, layout.input);

        // Status bar
        status_bar::render_status_bar(frame, layout.status, &self.status);

        // Popup overlay
        if let Some(ref popup) = self.popup {
            popup.render(frame, area);
        }
    }
}

fn categorize_model(model_id: &str) -> String {
    let lower = model_id.to_lowercase();
    if lower.contains("mini") || lower.contains("lite") || lower.contains("flash") || lower.contains("haiku") {
        "Light".to_string()
    } else if lower.contains("nano") || lower.contains("small") {
        "Light".to_string()
    } else if lower.contains("pro") || lower.contains("medium") {
        "Medium".to_string()
    } else if lower.contains("max") || lower.contains("opus") || lower.contains("o1") || lower.contains("o3") || lower.contains("o4") {
        "Max".to_string()
    } else {
        "High".to_string()
    }
}

fn parse_model_category(model_id: &str) -> ModelCategory {
    let lower = model_id.to_lowercase();
    if lower.contains("mini") || lower.contains("lite") || lower.contains("flash") || lower.contains("haiku") || lower.contains("nano") || lower.contains("small") {
        ModelCategory::Light
    } else if lower.contains("pro") || lower.contains("medium") {
        ModelCategory::Medium
    } else if lower.contains("max") || lower.contains("opus") || lower.contains("o1") || lower.contains("o3") || lower.contains("o4") {
        ModelCategory::Max
    } else {
        ModelCategory::High
    }
}