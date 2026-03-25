mod copilot;
mod dialog;
mod theme;
mod types;

fn safe_truncate(s: &str, max_chars: usize) -> &str {
    let mut end = 0;
    let mut count = 0;
    for (i, _) in s.char_indices() {
        if count == max_chars {
            break;
        }
        end = i;
        count += 1;
    }
    if count < max_chars {
        s
    } else {
        &s[..end]
    }
}
mod agent;
mod config;
mod events;
mod gemini;
mod memory;
mod state;
mod ui;

use std::io;
use std::io::Write;
use std::time::{Duration, Instant};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::mpsc;

use agent::AgentEvent;
use events::{handle_event, AppAction};
use memory::kms::{PersistedContent, PersistedMessage, PersistedSession};
use state::AppState;
use types::*;

#[tokio::main]
async fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let resume = args.iter().any(|a| a == "--resume" || a == "-r");

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let _ = execute!(
        stdout,
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
        )
    );

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let result = run_app(&mut terminal, resume).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        PopKeyboardEnhancementFlags,
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = result {
        eprintln!("Error: {}", err);
    }

    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    resume: bool,
) -> io::Result<()> {
    let mut state = AppState::new();

    // Resume: list all sessions and either auto-load (single) or show picker (multiple)
    if resume {
        let sessions = PersistedSession::list_all(&state.cwd);
        match sessions.len() {
            0 => {} // nothing to resume
            1 => {
                // Auto-load the only session
                let session = sessions.into_iter().next().unwrap();
                state.session_id = Some(session.id.clone());
                let msgs = session_to_messages(session);
                if !msgs.is_empty() {
                    state.messages = msgs;
                    state.show_welcome = false;
                }
            }
            _ => {
                // Show picker — sessions already sorted newest-first
                state.session_picker_sessions = sessions;
                state.dialog.reset();
                state.mode = AppMode::SessionPicker;
            }
        }
    }

    // Channel for receiving agent events
    let (agent_event_tx, mut agent_event_rx) = mpsc::channel::<AgentEvent>(64);
    // Store the sender so events.rs can clone it when spawning agents
    // We'll pass it through AppAction

    loop {
        terminal.draw(|frame| {
            ui::render(frame, &mut state);
        })?;

        // Copy selected text to clipboard (extracted during render)
        if let Some(text) = state.clipboard_text.take() {
            if !text.is_empty() {
                copy_to_clipboard(&text);
            }
            state.selection = None;
        }

        if state.should_quit {
            return Ok(());
        }

        let active = state.is_loading
            || state.streaming.is_some()
            || state.pending_stream.is_some()
            || state.agent_phase != AgentPhase::Idle;
        let timeout = if active {
            Duration::from_millis(30)
        } else {
            Duration::from_millis(100)
        };

        // Poll for terminal events
        if event::poll(timeout)? {
            let ev = event::read()?;
            match handle_event(&mut state, ev) {
                AppAction::Quit => return Ok(()),
                AppAction::Redraw => {}
                AppAction::None => {}
                AppAction::SendMessage(text) => {
                    spawn_agent(&mut state, &text, agent_event_tx.clone());
                }
                AppAction::TestConnection(provider, api_key) => {
                    spawn_connection_test(&mut state, &provider, &api_key, agent_event_tx.clone());
                }
                AppAction::StartCopilotDeviceFlow => {
                    spawn_copilot_device_flow(&mut state, agent_event_tx.clone());
                }
            }
        }

        // Poll for agent events (non-blocking)
        while let Ok(agent_event) = agent_event_rx.try_recv() {
            handle_agent_event(&mut state, agent_event);
        }

        let _now = Instant::now();

        // Streaming: feed chars from agent stream
        if let Some(ref mut stream) = state.streaming {
            if stream.current_content_idx > 0 || stream.current_char_idx > 0 {
                state.is_loading = false;
            }

            let mut advanced = false;
            for _ in 0..stream.chars_per_tick {
                if stream.current_content_idx >= stream.full_content.len() {
                    break;
                }
                let content = stream.full_content[stream.current_content_idx].clone();
                match content {
                    MessageContent::Text(ref text) => {
                        let chars: Vec<char> = text.chars().collect();
                        if stream.current_char_idx < chars.len() {
                            if let Some(msg) = state.messages.last_mut() {
                                if msg.role == MessageRole::Assistant {
                                    while msg.content.len() <= stream.current_content_idx {
                                        msg.content.push(MessageContent::Text(String::new()));
                                    }
                                    if let MessageContent::Text(ref mut t) =
                                        msg.content[stream.current_content_idx]
                                    {
                                        t.push(chars[stream.current_char_idx]);
                                    }
                                }
                            }
                            stream.current_char_idx += 1;
                            advanced = true;
                        } else {
                            stream.current_content_idx += 1;
                            stream.current_char_idx = 0;
                        }
                    }
                    other => {
                        if let Some(msg) = state.messages.last_mut() {
                            if msg.role == MessageRole::Assistant {
                                msg.content.push(other);
                            }
                        }
                        stream.current_content_idx += 1;
                        stream.current_char_idx = 0;
                        advanced = true;
                    }
                }
            }

            if !advanced && stream.current_content_idx >= stream.full_content.len() {
                let duration = stream.started_at.elapsed();
                state.last_response_duration = Some(duration);
                state.streaming = None;
                state.is_loading = false;
            }
        }

        // Spinner & shimmer animation
        if state.is_loading
            || state.pending_stream.is_some()
            || state.agent_phase != AgentPhase::Idle
        {
            state.spinner_frame = state.spinner_frame.wrapping_add(1);
            state.shimmer_offset += 0.05;
        }

        // Gradient animation on welcome screen
        if state.show_welcome {
            state.shimmer_offset += 0.003;
        }

        // Clean up old notifications
        state
            .compact_notifications
            .retain(|(_, t)| t.elapsed() < Duration::from_secs(5));
    }
}

fn spawn_agent(state: &mut AppState, text: &str, tx: mpsc::Sender<AgentEvent>) {
    state.is_loading = true;
    state.loading_text = "Preparing...".into();
    state.agent_phase = AgentPhase::Shizuka;
    state.agent_response_start = Some(Instant::now());
    state.user_scrolled = false;
    state.copilot_usage.reset_prompt_counter();

    let (cancel_tx, cancel_rx) = mpsc::channel::<()>(1);
    state.agent_cancel_tx = Some(cancel_tx);

    let prompt = text.to_string();
    let project_root = state.cwd.clone();
    let config = state.config.clone();

    tokio::spawn(async move {
        agent::run_agent(prompt, project_root, config, tx, cancel_rx).await;
    });
}

fn copy_to_clipboard(text: &str) {
    use std::process::{Command, Stdio};
    if let Ok(mut child) = Command::new("clip")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        if let Some(ref mut stdin) = child.stdin {
            let _ = stdin.write_all(text.as_bytes());
        }
        drop(child.stdin.take());
        let _ = child.wait();
    }
}

fn spawn_connection_test(
    _state: &mut AppState,
    provider: &str,
    api_key: &str,
    tx: mpsc::Sender<AgentEvent>,
) {
    let key = api_key.to_string();
    let provider = provider.to_string();
    tokio::spawn(async move {
        if provider == "copilot" {
            let client = copilot::CopilotClient::new(&key);
            match client.test_connection().await {
                Ok(msg) => {
                    let _ = tx
                        .send(AgentEvent::Done(format!("CONNECTION_OK:{}", msg)))
                        .await;
                }
                Err(e) => {
                    let _ = tx
                        .send(AgentEvent::Error(format!("CONNECTION_FAIL:{}", e)))
                        .await;
                }
            }
        } else {
            let client = gemini::GeminiClient::new(&key);
            match client.test_connection().await {
                Ok(msg) => {
                    let _ = tx
                        .send(AgentEvent::Done(format!("CONNECTION_OK:{}", msg)))
                        .await;
                }
                Err(e) => {
                    let _ = tx
                        .send(AgentEvent::Error(format!("CONNECTION_FAIL:{}", e)))
                        .await;
                }
            }
        }
    });
}

fn spawn_copilot_device_flow(state: &mut AppState, tx: mpsc::Sender<AgentEvent>) {
    let http_client = reqwest::Client::new();
    // Show "starting device flow..." in the connect dialog
    if let Some(ref mut cs) = state.connect_state {
        cs.phase = crate::config::ConnectPhase::CopilotPolling;
    }

    tokio::spawn(async move {
        match copilot::start_device_flow(&http_client).await {
            Ok(device_resp) => {
                // Signal the UI with the user code
                let _ = tx
                    .send(AgentEvent::Done(format!(
                        "COPILOT_DEVICE_CODE:{}:{}:{}",
                        device_resp.user_code,
                        device_resp.verification_uri,
                        device_resp.device_code,
                    )))
                    .await;

                // Now poll for the token
                match copilot::poll_for_token(
                    &http_client,
                    &device_resp.device_code,
                    device_resp.interval,
                )
                .await
                {
                    Ok(oauth_token) => {
                        let _ = copilot::save_oauth_token(&oauth_token);
                        let _ = tx
                            .send(AgentEvent::Done(format!("COPILOT_AUTH_OK:{}", oauth_token)))
                            .await;
                    }
                    Err(e) => {
                        let _ = tx
                            .send(AgentEvent::Error(format!("COPILOT_AUTH_FAIL:{}", e)))
                            .await;
                    }
                }
            }
            Err(e) => {
                let _ = tx
                    .send(AgentEvent::Error(format!("COPILOT_AUTH_FAIL:{}", e)))
                    .await;
            }
        }
    });
}

fn handle_agent_event(state: &mut AppState, event: AgentEvent) {
    match event {
        AgentEvent::PhaseChange(phase) => {
            match phase.as_str() {
                "shizuka" => {
                    state.agent_phase = AgentPhase::Shizuka;
                    state.loading_text = "Preparing...".into();
                }
                "nano" => {
                    state.agent_phase = AgentPhase::Nano;
                    state.loading_text = "Coding...".into();
                    // Start the assistant message (ShizukaReady already inserted the block)
                    state.messages.push(Message {
                        role: MessageRole::Assistant,
                        content: Vec::new(),
                        timestamp: Instant::now(),
                    });
                }
                _ => {}
            }
        }
        AgentEvent::ShizukaToolCall { name, args } => {
            // Update loading text only — block is built from ShizukaReady
            let args_short = if let Ok(v) = serde_json::from_str::<serde_json::Value>(&args) {
                v.get("path")
                    .or_else(|| v.get("pattern"))
                    .or_else(|| v.get("query"))
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default()
            } else {
                String::new()
            };
            let label = name.trim_start_matches("shizuka_").to_string();
            state.loading_text = if args_short.is_empty() {
                format!("Shizuka: {}...", label)
            } else {
                format!("Shizuka: {} {}", label, args_short)
            };
        }
        AgentEvent::ShizukaReady {
            preloaded,
            referenced,
            task_summary,
            classification,
        } => {
            // Insert the clean block into the chat
            state.messages.push(Message {
                role: MessageRole::System,
                content: vec![MessageContent::ShizukaBlock {
                    preloaded,
                    referenced,
                    task_summary,
                    classification,
                    collapsed: true,
                }],
                timestamp: Instant::now(),
            });
        }
        AgentEvent::StreamChunk(text) => {
            state.is_loading = false;
            // Append text to current assistant message
            if let Some(msg) = state.messages.last_mut() {
                if msg.role == MessageRole::Assistant {
                    if let Some(MessageContent::Text(ref mut t)) = msg.content.last_mut() {
                        t.push_str(&text);
                    } else {
                        msg.content.push(MessageContent::Text(text));
                    }
                }
            }
        }
        AgentEvent::ToolStart { name, args } => {
            state.is_loading = true;
            let args_short = if args.chars().count() > 80 {
                format!("{}...", safe_truncate(&args, 80))
            } else {
                args.clone()
            };
            state.loading_text = format!("{}: {}", name, args_short);

            // Add tool use to assistant message
            if let Some(msg) = state.messages.last_mut() {
                if msg.role == MessageRole::Assistant {
                    msg.content.push(MessageContent::ToolUse(ToolCall {
                        name: name.clone(),
                        args_summary: args_short,
                        status: ToolStatus::Running("...".into()),
                        output: None,
                        collapsed: false,
                    }));
                }
            }
        }
        AgentEvent::ToolComplete {
            name,
            result,
            is_error,
        } => {
            state.is_loading = false;

            // Update the matching tool call in the last assistant message
            if let Some(msg) = state.messages.last_mut() {
                if msg.role == MessageRole::Assistant {
                    for content in msg.content.iter_mut().rev() {
                        if let MessageContent::ToolUse(ref mut tool) = content {
                            if tool.name == name {
                                let result_short = if result.chars().count() > 200 {
                                    format!("{}...", safe_truncate(&result, 200))
                                } else {
                                    result.clone()
                                };
                                tool.status = if is_error {
                                    ToolStatus::Error(result_short.clone())
                                } else {
                                    ToolStatus::Complete(result_short.clone())
                                };
                                tool.output = Some(result);
                                // Auto-collapse large outputs
                                if let Some(ref output) = tool.output {
                                    if output.lines().count() > 30 {
                                        tool.collapsed = true;
                                    }
                                }
                                break;
                            }
                        }
                    }
                }
            }
        }
        AgentEvent::TokenUpdate {
            input,
            output,
            cached,
        } => {
            state.token_usage.input_tokens += input;
            state.token_usage.output_tokens += output;
            state.token_usage.cache_read_tokens += cached;
        }
        AgentEvent::ContextUpdate(percent) => {
            state.context_window_percent = percent;
        }
        AgentEvent::CopilotRateLimitUpdate(limits) => {
            state.copilot_usage.rate_limits = Some(limits);
        }
        AgentEvent::CopilotRequestUsed => {
            state.copilot_usage.requests_used_this_prompt += 1;
            state.copilot_usage.total_requests_used += 1;
        }
        AgentEvent::DirectAnswer(answer) => {
            state.is_loading = false;
            state.agent_phase = AgentPhase::Idle;
            // ShizukaReady was already sent before DirectAnswer
            state.messages.push(Message {
                role: MessageRole::Assistant,
                content: vec![MessageContent::Text(answer)],
                timestamp: Instant::now(),
            });
            if let Some(start) = state.agent_response_start.take() {
                state.last_response_duration = Some(start.elapsed());
            }
        }
        AgentEvent::Done(text) => {
            // Handle connection test success
            if text.starts_with("CONNECTION_OK:") {
                if let Some(ref mut cs) = state.connect_state {
                    let provider_name = cs.providers[cs.selected].name.clone();
                    let api_key = cs.api_key_input.clone();
                    state.config.set_provider(&provider_name, api_key, true);
                    let _ = state.config.save();
                    cs.phase = crate::config::ConnectPhase::Done;
                    cs.providers[cs.selected].connected = true;
                    cs.test_result = Some(Ok("Connected!".into()));
                }
                return;
            }

            // Copilot device code received — show to user
            if text.starts_with("COPILOT_DEVICE_CODE:") {
                let parts: Vec<&str> = text
                    .strip_prefix("COPILOT_DEVICE_CODE:")
                    .unwrap()
                    .splitn(3, ':')
                    .collect();
                if parts.len() == 3 {
                    if let Some(ref mut cs) = state.connect_state {
                        cs.phase = crate::config::ConnectPhase::CopilotDeviceFlow {
                            user_code: parts[0].to_string(),
                            verification_uri: parts[1].to_string(),
                        };
                    }
                }
                return;
            }

            // Copilot auth completed
            if text.starts_with("COPILOT_AUTH_OK:") {
                let token = text.strip_prefix("COPILOT_AUTH_OK:").unwrap();
                if let Some(ref mut cs) = state.connect_state {
                    state
                        .config
                        .set_provider("copilot", token.to_string(), true);
                    let _ = state.config.save();
                    cs.phase = crate::config::ConnectPhase::Done;
                    // Mark copilot provider as connected
                    for p in &mut cs.providers {
                        if p.name == "copilot" {
                            p.connected = true;
                        }
                    }
                    cs.test_result = Some(Ok("GitHub Copilot connected!".into()));
                }
                return;
            }

            state.is_loading = false;
            state.agent_phase = AgentPhase::Idle;
            state.agent_cancel_tx = None;
            if let Some(start) = state.agent_response_start.take() {
                state.last_response_duration = Some(start.elapsed());
            }
            // Persist session for --resume (reuse existing ID if resuming, otherwise generate new)
            let id = state.session_id.clone().unwrap_or_else(|| {
                // Use timestamp + thread-id as a simple unique ID
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                format!("{}", ts)
            });
            state.session_id = Some(id.clone());
            let session = messages_to_session(&state.messages, &state.cwd, &id);
            let _ = session.save(&state.cwd);
        }
        AgentEvent::Error(e) => {
            state.is_loading = false;
            state.agent_phase = AgentPhase::Idle;
            state.agent_cancel_tx = None;

            // Handle connection test results
            if e.starts_with("CONNECTION_FAIL:") {
                let msg = e.strip_prefix("CONNECTION_FAIL:").unwrap_or(&e);
                if let Some(ref mut cs) = state.connect_state {
                    cs.test_result = Some(Err(msg.to_string()));
                    cs.phase = crate::config::ConnectPhase::EnterApiKey;
                }
                return;
            }

            // Handle Copilot auth failure
            if e.starts_with("COPILOT_AUTH_FAIL:") {
                let msg = e.strip_prefix("COPILOT_AUTH_FAIL:").unwrap_or(&e);
                if let Some(ref mut cs) = state.connect_state {
                    cs.test_result = Some(Err(msg.to_string()));
                    cs.phase = crate::config::ConnectPhase::SelectProvider;
                }
                return;
            }

            flush_shizuka_block(state);
            state.messages.push(Message {
                role: MessageRole::System,
                content: vec![MessageContent::Text(format!("Error: {}", e))],
                timestamp: Instant::now(),
            });
            if let Some(start) = state.agent_response_start.take() {
                state.last_response_duration = Some(start.elapsed());
            }
        }
    }
}

fn messages_to_session(messages: &[Message], cwd: &str, id: &str) -> PersistedSession {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Preview = first user message text, truncated
    let preview = messages
        .iter()
        .find(|m| m.role == MessageRole::User)
        .and_then(|m| {
            m.content.iter().find_map(|c| {
                if let MessageContent::Text(t) = c {
                    Some(t.clone())
                } else {
                    None
                }
            })
        })
        .unwrap_or_default();
    let preview = if preview.chars().count() > 80 {
        format!("{}…", safe_truncate(&preview, 80))
    } else {
        preview
    };

    let persisted: Vec<PersistedMessage> = messages
        .iter()
        .map(|msg| {
            let role = match msg.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::System => "system",
            }
            .to_string();

            let content = msg
                .content
                .iter()
                .map(|c| match c {
                    MessageContent::Text(t) => PersistedContent::Text { text: t.clone() },
                    MessageContent::ToolUse(tool) => PersistedContent::ToolUse {
                        name: tool.name.clone(),
                        args_summary: tool.args_summary.clone(),
                        status: match &tool.status {
                            ToolStatus::Running(s) => format!("running:{}", s),
                            ToolStatus::Complete(s) => format!("done:{}", s),
                            ToolStatus::Error(s) => format!("error:{}", s),
                        },
                        output: tool.output.clone(),
                    },
                    MessageContent::ShizukaBlock {
                        preloaded,
                        referenced,
                        task_summary,
                        classification,
                        ..
                    } => PersistedContent::ShizukaBlock {
                        preloaded: preloaded.clone(),
                        referenced: referenced.clone(),
                        task_summary: task_summary.clone(),
                        classification: classification.clone(),
                    },
                    _ => PersistedContent::Other,
                })
                .collect();

            PersistedMessage { role, content }
        })
        .collect();

    PersistedSession {
        id: id.to_string(),
        messages: persisted,
        cwd: cwd.to_string(),
        session_ts: ts,
        preview,
    }
}

fn session_to_messages(session: PersistedSession) -> Vec<Message> {
    session
        .messages
        .into_iter()
        .filter_map(|pm| {
            let role = match pm.role.as_str() {
                "user" => MessageRole::User,
                "assistant" => MessageRole::Assistant,
                "system" => MessageRole::System,
                _ => return None,
            };

            let content: Vec<MessageContent> = pm
                .content
                .into_iter()
                .filter_map(|pc| match pc {
                    PersistedContent::Text { text } => Some(MessageContent::Text(text)),
                    PersistedContent::ToolUse {
                        name,
                        args_summary,
                        status,
                        output,
                    } => {
                        let tool_status = if status.starts_with("error:") {
                            ToolStatus::Error(status[6..].to_string())
                        } else if status.starts_with("running:") {
                            ToolStatus::Complete("(session ended)".into())
                        } else {
                            ToolStatus::Complete(status.trim_start_matches("done:").to_string())
                        };
                        Some(MessageContent::ToolUse(ToolCall {
                            name,
                            args_summary,
                            status: tool_status,
                            output,
                            collapsed: true,
                        }))
                    }
                    PersistedContent::ShizukaBlock {
                        preloaded,
                        referenced,
                        task_summary,
                        classification,
                    } => Some(MessageContent::ShizukaBlock {
                        preloaded,
                        referenced,
                        task_summary,
                        classification,
                        collapsed: true,
                    }),
                    PersistedContent::Other => None,
                })
                .collect();

            if content.is_empty() {
                return None;
            }

            Some(Message {
                role,
                content,
                timestamp: Instant::now(),
            })
        })
        .collect()
}

fn flush_shizuka_block(_state: &mut AppState) {}
