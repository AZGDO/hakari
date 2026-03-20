use crate::config::HakariConfig;
use crate::llm::client::LlmClient;
use crate::memory::kkm::Kkm;
use crate::memory::kms::Kms;
use crate::memory::kpms::Kpms;
use crate::memory::improvement;
use crate::nano::agent::{AgentEvent, NanoAgent};
use crate::project::detector;
use crate::shizuka::preparation;
use crate::tui::event::{self, AppEvent};
use crate::tui::layout::AppLayout;
use crate::tui::theme::Theme;
use crate::tui::widgets::header::{self, HeaderData};
use crate::tui::widgets::input_bar::InputBar;
use crate::tui::widgets::message_list::{ChatMessage, MessageList, MessageType};
use crate::tui::widgets::popup::Popup;
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
    pub show_help: bool,
    pub clipboard: Option<arboard::Clipboard>,
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
            show_help: false,
            clipboard,
        };

        // Detect project and update KPMS if needed
        if app.kpms.project.name.is_empty() {
            let detected = detector::detect_project(&project_dir);
            app.kpms.project = detected;
            let _ = app.kpms.save(&project_dir);
        }

        // Welcome message
        app.message_list.add_message(ChatMessage {
            msg_type: MessageType::System,
            content: format!(
                "Welcome to HAKARI — {} ({})\nType your task below and press Enter.",
                app.kpms.project.name,
                if app.kpms.project.language.is_empty() { "unknown" } else { &app.kpms.project.language }
            ),
            timestamp: None,
        });

        if app.llm_client.is_none() {
            app.message_list.add_message(ChatMessage {
                msg_type: MessageType::Warning,
                content: "No LLM API key configured. Set OPENAI_API_KEY or ANTHROPIC_API_KEY environment variable.".to_string(),
                timestamp: None,
            });
        }

        app
    }

    pub fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Key(key) => self.handle_key(key),
            AppEvent::Mouse(mouse) => self.handle_mouse(mouse),
            AppEvent::Resize(_, _) => {} // Layout recomputes automatically
            AppEvent::Tick => self.handle_tick(),
        }
    }

    fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        // Global keys
        if event::is_quit(&key) {
            self.running = false;
            return;
        }

        // Popup handling
        if self.popup.is_some() {
            match key.code {
                KeyCode::Esc => {
                    self.popup = None;
                    self.mode = AppMode::Input;
                }
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    // Handle confirmation
                    self.popup = None;
                    self.mode = AppMode::Input;
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    self.popup = None;
                    self.mode = AppMode::Input;
                }
                _ => {}
            }
            return;
        }

        // Help toggle
        if key.code == KeyCode::Char('?') && !self.agent_running {
            if self.show_help {
                self.show_help = false;
                self.popup = None;
                self.mode = AppMode::Input;
            } else {
                self.show_help = true;
                self.popup = Some(Popup::help());
                self.mode = AppMode::Popup;
            }
            return;
        }

        // Escape from scrolling
        if key.code == KeyCode::Esc {
            if self.show_help {
                self.show_help = false;
                self.popup = None;
            }
            self.mode = AppMode::Input;
            self.message_list.scroll_to_bottom();
            return;
        }

        // Copy
        if event::is_copy(&key) {
            if let Some(ref mut clipboard) = self.clipboard {
                // Copy all messages as text
                let text: String = self.message_list.messages.iter()
                    .map(|m| m.content.as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n");
                let _ = clipboard.set_text(text);
            }
            return;
        }

        // Paste
        if event::is_paste(&key) {
            if let Some(ref mut clipboard) = self.clipboard {
                if let Ok(text) = clipboard.get_text() {
                    self.input_bar.insert_str(&text);
                }
            }
            return;
        }

        match &self.mode {
            AppMode::Input => {
                match key.code {
                    KeyCode::Enter if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                        if let Some(text) = self.input_bar.submit() {
                            self.submit_prompt(text);
                        }
                    }
                    KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                        self.input_bar.insert_newline();
                    }
                    KeyCode::Backspace => self.input_bar.delete_char_before(),
                    KeyCode::Delete => self.input_bar.delete_char_after(),
                    KeyCode::Left => self.input_bar.move_cursor_left(),
                    KeyCode::Right => self.input_bar.move_cursor_right(),
                    KeyCode::Home => self.input_bar.move_cursor_home(),
                    KeyCode::End => self.input_bar.move_cursor_end(),
                    KeyCode::Up => {
                        if self.input_bar.content.is_empty() || !self.input_bar.content.contains('\n') {
                            self.input_bar.history_prev();
                        }
                    }
                    KeyCode::Down => {
                        if self.input_bar.content.is_empty() || !self.input_bar.content.contains('\n') {
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
                    }
                    _ => {}
                }
            }
            AppMode::Scrolling => {
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
                        // Switch back to input mode on regular typing
                        self.mode = AppMode::Input;
                        self.message_list.scroll_to_bottom();
                        self.input_bar.insert_char(c);
                    }
                    _ => {}
                }
            }
            AppMode::Popup => {} // Handled above
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
        if self.agent_running {
            self.spinner.tick();
        }

        // Collect agent events first to avoid borrow issues
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
                    if let Some(last) = self.message_list.messages.last() {
                        if matches!(last.msg_type, MessageType::Nano) {
                            self.message_list.append_to_last(&text);
                        } else {
                            self.message_list.add_message(ChatMessage {
                                msg_type: MessageType::Nano,
                                content: text,
                                timestamp: None,
                            });
                        }
                    } else {
                        self.message_list.add_message(ChatMessage {
                            msg_type: MessageType::Nano,
                            content: text,
                            timestamp: None,
                        });
                    }
                }
                AgentEvent::ToolCallStart { name, .. } => {
                    self.status.status = AgentStatus::ToolRunning(name.clone());
                    self.spinner.label = format!("running {}...", name);
                }
                AgentEvent::ToolCallEnd { name, result, success } => {
                    self.message_list.add_message(ChatMessage {
                        msg_type: MessageType::ToolResult { name, success },
                        content: result,
                        timestamp: None,
                    });
                    self.status.step = self.kms.steps.current;
                    self.status.context_tokens = self.kms.context.total_tokens_estimate;
                }
                AgentEvent::Warning(msg) => {
                    self.message_list.add_message(ChatMessage {
                        msg_type: MessageType::Warning,
                        content: msg,
                        timestamp: None,
                    });
                }
                AgentEvent::Escalation(summary) => {
                    self.popup = Some(Popup::escalation(&summary));
                    self.mode = AppMode::Popup;
                }
                AgentEvent::Complete(_msg) => {
                    self.agent_running = false;
                    self.status.status = AgentStatus::Complete;
                    self.persist_session_data();
                }
                AgentEvent::Error(msg) => {
                    self.message_list.add_message(ChatMessage {
                        msg_type: MessageType::Error,
                        content: msg,
                        timestamp: None,
                    });
                    self.agent_running = false;
                    self.status.status = AgentStatus::Error;
                }
            }
        }
    }

    fn submit_prompt(&mut self, prompt: String) {
        if self.agent_running {
            self.message_list.add_message(ChatMessage {
                msg_type: MessageType::Warning,
                content: "Agent is currently running. Please wait.".to_string(),
                timestamp: None,
            });
            return;
        }

        let Some(ref llm_client) = self.llm_client else {
            self.message_list.add_message(ChatMessage {
                msg_type: MessageType::Error,
                content: "No LLM API key configured. Set OPENAI_API_KEY or ANTHROPIC_API_KEY.".to_string(),
                timestamp: None,
            });
            return;
        };

        // Add user message to display
        self.message_list.add_message(ChatMessage {
            msg_type: MessageType::User,
            content: prompt.clone(),
            timestamp: None,
        });

        self.kms.task.original_prompt = prompt.clone();
        self.agent_running = true;
        self.status.status = AgentStatus::Preparing;
        self.spinner.label = "preparing...".to_string();

        // Start agent in background
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        self.agent_rx = Some(event_rx);

        let llm_client = llm_client.clone();
        let config = self.config.clone();
        let project_dir = self.project_dir.clone();
        let kpms = self.kpms.clone();
        let kkm = self.kkm.clone();
        let mut kms = self.kms.clone();

        tokio::spawn(async move {
            // Phase 1: Preparation (fast-path or KLM)
            let prep = if let Some(fast) = preparation::try_fast_path(&prompt, &project_dir) {
                let _ = event_tx.send(AgentEvent::TextDelta(String::new()));
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

            // Update KMS from preparation
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

            // Shizuka status message
            let _ = event_tx.send(AgentEvent::TextDelta(String::new()));

            // Phase 2: Nano execution
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
        // Collect improvement signals
        let prep_files: Vec<String> = Vec::new(); // Would need to store from last prep
        let misses = improvement::collect_preparation_misses(&self.kms, &prep_files, &[]);
        let record = improvement::collect_iteration_record(&self.kms);
        improvement::persist_improvements(
            &mut self.kpms,
            &self.kms,
            &misses,
            &record,
            &self.kms.session_id,
        );
        let _ = self.kpms.save(&self.project_dir);
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();

        // Fill background
        frame.render_widget(
            ratatui::widgets::Block::default().style(Style::default().bg(Theme::bg())),
            area,
        );

        let input_height = self.input_bar.desired_height();
        let layout = AppLayout::compute(area, input_height);

        // Header
        header::render_header(
            frame,
            layout.header,
            &HeaderData {
                project_name: self.kpms.project.name.clone(),
                session_id: self.kms.session_id.clone(),
                has_kpms: !self.kpms.learnings.is_empty() || !self.kpms.file_index.is_empty(),
                has_kkm: !self.kkm.tools.is_empty(),
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
