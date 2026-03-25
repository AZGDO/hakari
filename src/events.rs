use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use std::time::Instant;

use crate::config::{ConnectPhase, ConnectState};
use crate::state::AppState;
use crate::types::*;

pub enum AppAction {
    None,
    Quit,
    Redraw,
    SendMessage(String),
    TestConnection(String, String),
    StartCopilotDeviceFlow,
}

pub fn handle_event(state: &mut AppState, ev: Event) -> AppAction {
    match ev {
        Event::Key(key) if key.kind == KeyEventKind::Press => handle_key(state, key),
        Event::Mouse(mouse) => handle_mouse(state, mouse),
        Event::Resize(_, _) => AppAction::Redraw,
        _ => AppAction::None,
    }
}

fn handle_mouse(state: &mut AppState, mouse: MouseEvent) -> AppAction {
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            state.user_scrolled = true;
            state.scroll_offset = state.scroll_offset.saturating_sub(3);
            AppAction::Redraw
        }
        MouseEventKind::ScrollDown => {
            let max = state.total_message_lines;
            if state.scroll_offset + 3 >= max {
                state.scroll_offset = max;
                state.user_scrolled = false;
            } else {
                state.scroll_offset += 3;
            }
            AppAction::Redraw
        }
        MouseEventKind::Down(MouseButton::Left) => {
            // Start potential selection (clear any existing)
            state.selection = Some(crate::state::TextSelection {
                anchor: (mouse.column, mouse.row),
                end: (mouse.column, mouse.row),
                active: false,
            });
            AppAction::None
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(ref mut sel) = state.selection {
                sel.active = true;
                sel.end = (mouse.column, mouse.row);
            }
            AppAction::Redraw
        }
        MouseEventKind::Up(MouseButton::Left) => {
            let was_selecting = state.selection.as_ref().map_or(false, |s| s.active);
            if was_selecting {
                // Selection stays visible — user can Ctrl+C to copy
                AppAction::Redraw
            } else {
                // It was a plain click — handle ShizukaBlock collapse toggle
                state.selection = None;
                handle_click(state, mouse.column, mouse.row)
            }
        }
        _ => AppAction::None,
    }
}

fn handle_click(state: &mut AppState, _col: u16, row: u16) -> AppAction {
    let click_row = row as usize;
    let scroll = state.scroll_offset;
    let mut toggled = false;
    for (msg_idx, msg) in state.messages.iter_mut().enumerate() {
        for content in msg.content.iter_mut() {
            if let MessageContent::ShizukaBlock {
                ref mut collapsed, ..
            } = content
            {
                if let Some(&block_row) = state.shizuka_block_rows.get(&msg_idx) {
                    let rendered_row = block_row.saturating_sub(scroll);
                    if click_row == rendered_row || click_row == rendered_row + 1 {
                        *collapsed = !*collapsed;
                        toggled = true;
                        break;
                    }
                }
            }
        }
        if toggled {
            break;
        }
    }
    if toggled {
        AppAction::Redraw
    } else {
        AppAction::None
    }
}

fn handle_key(state: &mut AppState, key: KeyEvent) -> AppAction {
    // Ctrl+C with active selection → copy and deselect
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        if state.selection.as_ref().map_or(false, |s| s.active) {
            state.clipboard_pending = true;
            return AppAction::Redraw;
        }
        if state.agent_phase != AgentPhase::Idle {
            // Cancel running agent
            if let Some(ref tx) = state.agent_cancel_tx {
                let _ = tx.try_send(());
            }
            state.is_loading = false;
            state.streaming = None;
            state.pending_stream = None;
            state.agent_phase = AgentPhase::Idle;
            state.agent_cancel_tx = None;
            state.messages.push(Message {
                role: MessageRole::System,
                content: vec![MessageContent::Text("Agent cancelled.".into())],
                timestamp: Instant::now(),
            });
            return AppAction::Redraw;
        }
        if state.is_loading || state.streaming.is_some() || state.pending_stream.is_some() {
            state.is_loading = false;
            state.streaming = None;
            state.pending_stream = None;
            return AppAction::Redraw;
        }
        return AppAction::Quit;
    }

    // Any other keypress clears an active selection
    if state.selection.as_ref().map_or(false, |s| s.active) {
        state.selection = None;
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('l') {
        state.messages.clear();
        state.show_welcome = true;
        state.scroll_offset = 0;
        return AppAction::Redraw;
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('t') {
        state.theme = state.theme.toggle();
        return AppAction::Redraw;
    }

    match state.mode {
        AppMode::Normal => handle_normal_mode(state, key),
        AppMode::SlashCommand => handle_slash_mode(state, key),
        AppMode::PermissionPrompt => handle_permission_mode(state, key),
        AppMode::Help => handle_overlay_dismiss(state, key),
        AppMode::ModelPicker => handle_model_picker(state, key),
        AppMode::Settings => handle_settings(state, key),
        AppMode::ThemePicker => handle_overlay_dismiss(state, key),
        AppMode::Connect => handle_connect_mode(state, key),
        AppMode::SessionPicker => handle_session_picker(state, key),
    }
}

fn handle_normal_mode(state: &mut AppState, key: KeyEvent) -> AppAction {
    // File mention picker takes priority when active
    if state.file_mention_active {
        return handle_file_mention(state, key);
    }

    match key.code {
        KeyCode::Esc => {
            // Cancel running agent/loading if active
            if state.agent_phase != AgentPhase::Idle {
                if let Some(ref tx) = state.agent_cancel_tx {
                    let _ = tx.try_send(());
                }
                state.is_loading = false;
                state.streaming = None;
                state.pending_stream = None;
                state.agent_phase = AgentPhase::Idle;
                state.agent_cancel_tx = None;
                state.messages.push(Message {
                    role: MessageRole::System,
                    content: vec![MessageContent::Text("Agent cancelled.".into())],
                    timestamp: Instant::now(),
                });
                return AppAction::Redraw;
            }
            if state.is_loading || state.streaming.is_some() || state.pending_stream.is_some() {
                state.is_loading = false;
                state.streaming = None;
                state.pending_stream = None;
                return AppAction::Redraw;
            }
            AppAction::Redraw
        }
        KeyCode::Enter => {
            let want_newline = key.modifiers.contains(KeyModifiers::SHIFT)
                || key.modifiers.contains(KeyModifiers::ALT);
            if want_newline {
                let byte_idx = state
                    .input
                    .char_indices()
                    .nth(state.cursor_pos)
                    .map(|(b, _)| b)
                    .unwrap_or(state.input.len());
                state.input.insert(byte_idx, '\n');
                state.cursor_pos += 1;
                return AppAction::Redraw;
            }
            if state.is_loading
                || state.streaming.is_some()
                || state.pending_stream.is_some()
                || state.agent_phase != AgentPhase::Idle
            {
                return AppAction::None;
            }
            let text = state.input.trim().to_string();
            if text.is_empty() {
                return AppAction::None;
            }
            if text.starts_with('/') {
                let action = handle_slash_command(state, &text);
                state.input.clear();
                state.cursor_pos = 0;
                state.input_scroll = 0;
                return action;
            }

            // Expand @file mentions before sending to agent
            let expanded = if text.contains('@') {
                expand_file_mentions(&text, &state.cwd)
            } else {
                text.clone()
            };

            // Show the original text (with @tokens) in the chat, send expanded to agent
            state.add_user_message(text.clone());
            state.input_history.push(text.clone());
            state.history_index = None;
            state.show_welcome = false;
            state.user_scrolled = false;
            state.file_mention_active = false;

            state.input.clear();
            state.cursor_pos = 0;
            state.input_scroll = 0;
            state.scroll_offset = 0;

            AppAction::SendMessage(expanded)
        }
        KeyCode::Backspace => {
            if state.cursor_pos > 0 {
                state.cursor_pos -= 1;
                if let Some(byte_idx) = state
                    .input
                    .char_indices()
                    .nth(state.cursor_pos)
                    .map(|(b, _)| b)
                {
                    state.input.remove(byte_idx);
                }
                if state.input.is_empty() && state.mode == AppMode::SlashCommand {
                    state.mode = AppMode::Normal;
                }
            }
            check_slash_mode(state);
            check_file_mention_mode(state);
            AppAction::Redraw
        }
        KeyCode::Delete => {
            if let Some(byte_idx) = state
                .input
                .char_indices()
                .nth(state.cursor_pos)
                .map(|(b, _)| b)
            {
                state.input.remove(byte_idx);
            }
            AppAction::Redraw
        }
        KeyCode::Left => {
            state.cursor_pos = state.cursor_pos.saturating_sub(1);
            AppAction::Redraw
        }
        KeyCode::Right => {
            if state.cursor_pos < state.input.chars().count() {
                state.cursor_pos += 1;
            }
            AppAction::Redraw
        }
        KeyCode::Up => {
            let input_lines: Vec<&str> = state.input.split('\n').collect();
            let (cursor_row, cursor_col) = flat_to_row_col(&input_lines, state.cursor_pos);
            if cursor_row > 0 {
                let prev_line_len = input_lines[cursor_row - 1].chars().count();
                let new_col = cursor_col.min(prev_line_len);
                state.cursor_pos = row_col_to_flat(&input_lines, cursor_row - 1, new_col);
            } else if !state.input_history.is_empty() {
                let idx = match state.history_index {
                    Some(i) => i.saturating_sub(1),
                    None => state.input_history.len() - 1,
                };
                state.history_index = Some(idx);
                state.input = state.input_history[idx].clone();
                state.cursor_pos = state.input.len();
            }
            AppAction::Redraw
        }
        KeyCode::Down => {
            let input_lines: Vec<&str> = state.input.split('\n').collect();
            let (cursor_row, cursor_col) = flat_to_row_col(&input_lines, state.cursor_pos);
            if cursor_row + 1 < input_lines.len() {
                let next_line_len = input_lines[cursor_row + 1].chars().count();
                let new_col = cursor_col.min(next_line_len);
                state.cursor_pos = row_col_to_flat(&input_lines, cursor_row + 1, new_col);
            } else if let Some(idx) = state.history_index {
                if idx + 1 < state.input_history.len() {
                    state.history_index = Some(idx + 1);
                    state.input = state.input_history[idx + 1].clone();
                    state.cursor_pos = state.input.len();
                } else {
                    state.history_index = None;
                    state.input.clear();
                    state.cursor_pos = 0;
                    state.input_scroll = 0;
                }
            }
            AppAction::Redraw
        }
        KeyCode::Home => {
            state.cursor_pos = 0;
            state.input_scroll = 0;
            AppAction::Redraw
        }
        KeyCode::End => {
            state.cursor_pos = state.input.chars().count();
            AppAction::Redraw
        }
        KeyCode::Char(c) => {
            let byte_idx = state
                .input
                .char_indices()
                .nth(state.cursor_pos)
                .map(|(b, _)| b)
                .unwrap_or(state.input.len());
            state.input.insert(byte_idx, c);
            state.cursor_pos += 1;
            check_slash_mode(state);
            check_file_mention_mode(state);
            AppAction::Redraw
        }
        KeyCode::Tab => {
            if state.mode == AppMode::SlashCommand {
                let commands = state.filtered_commands();
                if let Some(cmd) = commands.get(state.slash_selected) {
                    state.input = format!("/{}", cmd.name);
                    state.cursor_pos = state.input.len();
                }
            }
            AppAction::Redraw
        }
        _ => AppAction::None,
    }
}

fn handle_slash_mode(state: &mut AppState, key: KeyEvent) -> AppAction {
    match key.code {
        KeyCode::Esc => {
            state.mode = AppMode::Normal;
            AppAction::Redraw
        }
        KeyCode::Up => {
            state.slash_selected = state.slash_selected.saturating_sub(1);
            AppAction::Redraw
        }
        KeyCode::Down => {
            let max = state.filtered_commands().len().saturating_sub(1);
            if state.slash_selected < max {
                state.slash_selected += 1;
            }
            AppAction::Redraw
        }
        KeyCode::Enter => {
            let commands = state.filtered_commands();
            if let Some(cmd) = commands.get(state.slash_selected) {
                let cmd_name = format!("/{}", cmd.name);
                state.mode = AppMode::Normal;
                let action = handle_slash_command(state, &cmd_name);
                state.input.clear();
                state.cursor_pos = 0;
                return action;
            }
            AppAction::Redraw
        }
        KeyCode::Tab => {
            let commands = state.filtered_commands();
            if let Some(cmd) = commands.get(state.slash_selected) {
                state.input = format!("/{}", cmd.name);
                state.cursor_pos = state.input.len();
                state.mode = AppMode::Normal;
            }
            AppAction::Redraw
        }
        KeyCode::Backspace => {
            if state.cursor_pos > 0 {
                state.cursor_pos -= 1;
                state.input.remove(state.cursor_pos);
            }
            if state.input.is_empty() || !state.input.starts_with('/') {
                state.mode = AppMode::Normal;
            } else {
                state.slash_filter = state.input[1..].to_string();
                state.slash_selected = 0;
                state.slash_scroll = 0;
            }
            AppAction::Redraw
        }
        KeyCode::Char(c) => {
            let byte_idx = state
                .input
                .char_indices()
                .nth(state.cursor_pos)
                .map(|(b, _)| b)
                .unwrap_or(state.input.len());
            state.input.insert(byte_idx, c);
            state.cursor_pos += 1;
            state.slash_filter = if state.input.len() > 1 {
                state.input[1..].to_string()
            } else {
                String::new()
            };
            state.slash_selected = 0;
            state.slash_scroll = 0;
            AppAction::Redraw
        }
        _ => AppAction::None,
    }
}

fn handle_permission_mode(state: &mut AppState, key: KeyEvent) -> AppAction {
    let Some(req) = &mut state.permission_request else {
        state.mode = AppMode::Normal;
        return AppAction::Redraw;
    };
    match key.code {
        KeyCode::Up => {
            req.selected_option = req.selected_option.saturating_sub(1);
            AppAction::Redraw
        }
        KeyCode::Down => {
            let max = PermissionRequest::options().len() - 1;
            if req.selected_option < max {
                req.selected_option += 1;
            }
            AppAction::Redraw
        }
        KeyCode::Enter => {
            let option = req.selected_option;
            let tool_name = req.tool_name.clone();
            state.permission_request = None;
            state.mode = AppMode::Normal;
            let msg = match option {
                0 => format!("Allowed {} once", tool_name),
                1 => format!("Allowed {} for session", tool_name),
                _ => format!("Denied {}", tool_name),
            };
            state.messages.push(Message {
                role: MessageRole::System,
                content: vec![MessageContent::Text(msg)],
                timestamp: Instant::now(),
            });
            AppAction::Redraw
        }
        KeyCode::Esc => {
            state.permission_request = None;
            state.mode = AppMode::Normal;
            AppAction::Redraw
        }
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            let tool_name = state.permission_request.as_ref().unwrap().tool_name.clone();
            state.permission_request = None;
            state.mode = AppMode::Normal;
            state.messages.push(Message {
                role: MessageRole::System,
                content: vec![MessageContent::Text(format!("Allowed {} once", tool_name))],
                timestamp: Instant::now(),
            });
            AppAction::Redraw
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            let tool_name = state.permission_request.as_ref().unwrap().tool_name.clone();
            state.permission_request = None;
            state.mode = AppMode::Normal;
            state.messages.push(Message {
                role: MessageRole::System,
                content: vec![MessageContent::Text(format!("Denied {}", tool_name))],
                timestamp: Instant::now(),
            });
            AppAction::Redraw
        }
        _ => AppAction::None,
    }
}

fn handle_model_picker(state: &mut AppState, key: KeyEvent) -> AppAction {
    let selectable_count = model_picker_selectable_count(state);

    if state.model_picker_typing {
        match key.code {
            KeyCode::Esc => {
                state.model_picker_typing = false;
                AppAction::Redraw
            }
            KeyCode::Enter => {
                let id = state.model_custom_input.trim().to_string();
                if !id.is_empty() {
                    apply_model_selection(state, &id, "", &id);
                }
                AppAction::Redraw
            }
            KeyCode::Backspace => {
                if state.model_custom_cursor > 0 {
                    state.model_custom_cursor -= 1;
                    state.model_custom_input.remove(state.model_custom_cursor);
                }
                AppAction::Redraw
            }
            KeyCode::Left => {
                state.model_custom_cursor = state.model_custom_cursor.saturating_sub(1);
                AppAction::Redraw
            }
            KeyCode::Right => {
                if state.model_custom_cursor < state.model_custom_input.len() {
                    state.model_custom_cursor += 1;
                }
                AppAction::Redraw
            }
            KeyCode::Char(c) => {
                state
                    .model_custom_input
                    .insert(state.model_custom_cursor, c);
                state.model_custom_cursor += 1;
                AppAction::Redraw
            }
            _ => AppAction::None,
        }
    } else {
        match key.code {
            KeyCode::Esc => {
                state.model_picker_typing = false;
                state.mode = AppMode::Normal;
                AppAction::Redraw
            }
            KeyCode::Up => {
                state.dialog.move_up();
                AppAction::Redraw
            }
            KeyCode::Down => {
                state.dialog.move_down(selectable_count);
                AppAction::Redraw
            }
            KeyCode::Enter => {
                if let Some(opt_idx) = model_picker_selected_to_option(state) {
                    let model = state.model_options[opt_idx].clone();
                    apply_model_selection(state, &model.id, &model.provider, &model.display_name);
                } else {
                    // Custom row
                    state.model_picker_typing = true;
                    state.model_custom_input.clear();
                    state.model_custom_cursor = 0;
                }
                AppAction::Redraw
            }
            _ => AppAction::None,
        }
    }
}

fn apply_model_selection(state: &mut AppState, model_id: &str, provider: &str, display: &str) {
    let target = state.model_picker_target.clone();

    // Update the provider preference for the target phase
    if !provider.is_empty() {
        match target.as_str() {
            "shizuka" => {
                state.config.preferences.shizuka_provider = provider.to_string();
                state.config.preferences.shizuka_model = model_id.to_string();
            }
            "nano" => {
                state.config.preferences.nano_provider = provider.to_string();
            }
            _ => {}
        }
    }

    // Save config to disk
    let _ = state.config.save();

    state.model_name = model_id.to_string();
    state.model_picker_typing = false;
    state.mode = AppMode::Normal;
    state.messages.push(Message {
        role: MessageRole::System,
        content: vec![MessageContent::Text(format!(
            "{} set to {} ({})",
            if target == "shizuka" {
                "Shizuka"
            } else {
                "Nano"
            },
            display,
            if provider.is_empty() {
                "custom"
            } else {
                provider
            },
        ))],
        timestamp: Instant::now(),
    });
}

fn handle_settings(state: &mut AppState, key: KeyEvent) -> AppAction {
    let count = state.settings.len();
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            state.mode = AppMode::Normal;
            AppAction::Redraw
        }
        KeyCode::Up => {
            state.dialog.move_up();
            AppAction::Redraw
        }
        KeyCode::Down => {
            state.dialog.move_down(count);
            AppAction::Redraw
        }
        KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Right => {
            let entry = &mut state.settings[state.dialog.selected];
            match &mut entry.value {
                SettingValue::Bool(ref mut v) => {
                    *v = !*v;
                }
                SettingValue::Choice {
                    options,
                    ref mut selected,
                } => {
                    *selected = (*selected + 1) % options.len();
                }
                SettingValue::Info(_) => {}
            }
            AppAction::Redraw
        }
        KeyCode::Left => {
            let entry = &mut state.settings[state.dialog.selected];
            match &mut entry.value {
                SettingValue::Bool(ref mut v) => {
                    *v = !*v;
                }
                SettingValue::Choice {
                    options,
                    ref mut selected,
                } => {
                    *selected = if *selected == 0 {
                        options.len() - 1
                    } else {
                        *selected - 1
                    };
                }
                SettingValue::Info(_) => {}
            }
            AppAction::Redraw
        }
        _ => AppAction::None,
    }
}

fn handle_connect_mode(state: &mut AppState, key: KeyEvent) -> AppAction {
    let Some(ref mut cs) = state.connect_state else {
        state.mode = AppMode::Normal;
        return AppAction::Redraw;
    };

    match cs.phase {
        ConnectPhase::SelectProvider => match key.code {
            KeyCode::Esc => {
                state.connect_state = None;
                state.mode = AppMode::Normal;
                AppAction::Redraw
            }
            KeyCode::Up => {
                cs.selected = cs.selected.saturating_sub(1);
                AppAction::Redraw
            }
            KeyCode::Down => {
                if cs.selected + 1 < cs.providers.len() {
                    cs.selected += 1;
                }
                AppAction::Redraw
            }
            KeyCode::Enter => {
                let provider_name = cs.providers[cs.selected].name.clone();
                if provider_name == "copilot" {
                    return AppAction::StartCopilotDeviceFlow;
                }
                cs.phase = ConnectPhase::EnterApiKey;
                cs.api_key_input.clear();
                cs.api_key_cursor = 0;
                cs.test_result = None;
                AppAction::Redraw
            }
            _ => AppAction::None,
        },
        ConnectPhase::EnterApiKey => match key.code {
            KeyCode::Esc => {
                cs.phase = ConnectPhase::SelectProvider;
                cs.test_result = None;
                AppAction::Redraw
            }
            KeyCode::Enter => {
                if cs.api_key_input.is_empty() {
                    return AppAction::Redraw;
                }
                let provider = cs.providers[cs.selected].name.clone();
                let api_key = cs.api_key_input.clone();
                cs.phase = ConnectPhase::Testing;
                cs.test_result = None;
                AppAction::TestConnection(provider, api_key)
            }
            KeyCode::Backspace => {
                if cs.api_key_cursor > 0 {
                    cs.api_key_cursor -= 1;
                    cs.api_key_input.remove(cs.api_key_cursor);
                }
                cs.test_result = None;
                AppAction::Redraw
            }
            KeyCode::Char(c) => {
                cs.api_key_input.insert(cs.api_key_cursor, c);
                cs.api_key_cursor += 1;
                cs.test_result = None;
                AppAction::Redraw
            }
            KeyCode::Left => {
                cs.api_key_cursor = cs.api_key_cursor.saturating_sub(1);
                AppAction::Redraw
            }
            KeyCode::Right => {
                if cs.api_key_cursor < cs.api_key_input.len() {
                    cs.api_key_cursor += 1;
                }
                AppAction::Redraw
            }
            _ => AppAction::None,
        },
        ConnectPhase::Testing => {
            // During testing, only allow Esc to cancel
            match key.code {
                KeyCode::Esc => {
                    cs.phase = ConnectPhase::EnterApiKey;
                    AppAction::Redraw
                }
                _ => AppAction::None,
            }
        }
        ConnectPhase::Done => {
            // Any key returns to normal mode
            state.connect_state = None;
            state.mode = AppMode::Normal;
            AppAction::Redraw
        }
        ConnectPhase::CopilotDeviceFlow { .. } => match key.code {
            KeyCode::Esc => {
                cs.phase = ConnectPhase::SelectProvider;
                AppAction::Redraw
            }
            _ => AppAction::None,
        },
        ConnectPhase::CopilotPolling => match key.code {
            KeyCode::Esc => {
                cs.phase = ConnectPhase::SelectProvider;
                AppAction::Redraw
            }
            _ => AppAction::None,
        },
    }
}

fn handle_session_picker(state: &mut AppState, key: KeyEvent) -> AppAction {
    let count = state.session_picker_sessions.len();
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            state.mode = AppMode::Normal;
            state.session_picker_sessions.clear();
            AppAction::Redraw
        }
        KeyCode::Up => {
            state.dialog.move_up();
            AppAction::Redraw
        }
        KeyCode::Down => {
            state.dialog.move_down(count);
            AppAction::Redraw
        }
        KeyCode::Enter => {
            let idx = state.dialog.selected;
            if let Some(session) = state.session_picker_sessions.get(idx).cloned() {
                state.session_id = Some(session.id.clone());
                let msgs =
                    crate::memory::kms::PersistedSession::load_by_id(&state.cwd, &session.id)
                        .map(session_to_messages_helper)
                        .unwrap_or_default();
                if !msgs.is_empty() {
                    state.messages = msgs;
                    state.show_welcome = false;
                }
            }
            state.session_picker_sessions.clear();
            state.mode = AppMode::Normal;
            AppAction::Redraw
        }
        _ => AppAction::None,
    }
}

fn session_to_messages_helper(session: crate::memory::kms::PersistedSession) -> Vec<Message> {
    use crate::memory::kms::PersistedContent;
    use std::time::Instant;

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

fn handle_overlay_dismiss(state: &mut AppState, key: KeyEvent) -> AppAction {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
            state.mode = AppMode::Normal;
            AppAction::Redraw
        }
        _ => AppAction::None,
    }
}

fn check_slash_mode(state: &mut AppState) {
    if state.input.starts_with('/') && !state.input.is_empty() {
        state.mode = AppMode::SlashCommand;
        state.slash_filter = if state.input.len() > 1 {
            state.input[1..].to_string()
        } else {
            String::new()
        };
        state.slash_selected = 0;
        state.slash_scroll = 0;
    } else if state.mode == AppMode::SlashCommand {
        state.mode = AppMode::Normal;
    }
}

fn open_model_picker(state: &mut AppState, target: &str) {
    state.model_options = ModelOption::for_connected_providers(&state.config);
    state.model_picker_typing = false;
    state.model_picker_target = target.to_string();
    state.model_custom_input.clear();
    state.model_custom_cursor = 0;
    state.dialog.reset();
    state.mode = AppMode::ModelPicker;
}

/// Map dialog.selected (selectable index) to the index in model_options.
/// Selectable items are non-header model_options + 1 custom row at the end.
fn model_picker_selected_to_option(state: &AppState) -> Option<usize> {
    let mut selectable = 0;
    for (i, m) in state.model_options.iter().enumerate() {
        if m.is_header {
            continue;
        }
        if selectable == state.dialog.selected {
            return Some(i);
        }
        selectable += 1;
    }
    None // selected is the custom row (past all model options)
}

/// Total number of selectable items: non-header models + 1 custom row.
fn model_picker_selectable_count(state: &AppState) -> usize {
    state.model_options.iter().filter(|m| !m.is_header).count() + 1
}

fn handle_slash_command(state: &mut AppState, cmd: &str) -> AppAction {
    let command = cmd.split(' ').next().unwrap_or(cmd);

    match command {
        "/exit" => {
            state.should_quit = true;
            AppAction::Redraw
        }
        "/help" => {
            state.mode = AppMode::Help;
            AppAction::Redraw
        }
        "/model" => {
            // Show current provider/model configuration
            let active = state
                .config
                .active_provider()
                .map(|(n, _)| n.to_string())
                .unwrap_or_else(|| "none".into());
            let shizuka_prov = if state.config.preferences.shizuka_provider.is_empty() {
                active.clone()
            } else {
                state.config.preferences.shizuka_provider.clone()
            };
            let nano_prov = if state.config.preferences.nano_provider.is_empty() {
                active.clone()
            } else {
                state.config.preferences.nano_provider.clone()
            };
            let info = format!(
                "Provider config:\n  Shizuka (explorer): {}\n  Nano (executor):    {}\n  Default provider:   {}\n\nUse /shizuka or /nano to change.",
                shizuka_prov, nano_prov, active
            );
            state.messages.push(Message {
                role: MessageRole::System,
                content: vec![MessageContent::Text(info)],
                timestamp: Instant::now(),
            });
            AppAction::Redraw
        }
        "/shizuka" => {
            open_model_picker(state, "shizuka");
            AppAction::Redraw
        }
        "/nano" => {
            open_model_picker(state, "nano");
            AppAction::Redraw
        }
        "/config" => {
            state.dialog.reset();
            state.mode = AppMode::Settings;
            AppAction::Redraw
        }
        "/connect" => {
            state.connect_state = Some(ConnectState::new(&state.config));
            state.mode = AppMode::Connect;
            AppAction::Redraw
        }
        "/clear" => {
            state.messages.clear();
            state.show_welcome = true;
            state.scroll_offset = 0;
            state
                .compact_notifications
                .push(("Conversation cleared.".into(), Instant::now()));
            AppAction::Redraw
        }
        "/compact" => {
            state
                .compact_notifications
                .push(("Compacting conversation...".into(), Instant::now()));
            AppAction::Redraw
        }
        "/theme" => {
            state.theme = state.theme.toggle();
            AppAction::Redraw
        }
        "/cost" => {
            if state.config.is_copilot() {
                let usage = &state.copilot_usage;
                let mut info = format!(
                    "Copilot usage: {} total requests",
                    usage.total_requests_used,
                );
                if let Some(ref limits) = usage.rate_limits {
                    info.push_str(&format!(
                        "\n  Remaining: {}/{} ({:.0}% left) | Reset: {}s",
                        limits.remaining,
                        limits.total,
                        limits.remaining_percent(),
                        limits.reset_at.saturating_sub(
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_secs())
                                .unwrap_or(0)
                        ),
                    ));
                }
                state.messages.push(Message {
                    role: MessageRole::System,
                    content: vec![MessageContent::Text(info)],
                    timestamp: Instant::now(),
                });
            } else {
                let cost = state.token_usage.cost_usd();
                let total = state.token_usage.total();
                state.messages.push(Message {
                    role: MessageRole::System,
                    content: vec![MessageContent::Text(format!(
                        "Token usage: {} tokens (${:.4})\n  Input: {} | Output: {} | Cache read: {} | Cache write: {}",
                        AppState::format_token_count(total), cost,
                        AppState::format_token_count(state.token_usage.input_tokens),
                        AppState::format_token_count(state.token_usage.output_tokens),
                        AppState::format_token_count(state.token_usage.cache_read_tokens),
                        AppState::format_token_count(state.token_usage.cache_write_tokens),
                    ))],
                    timestamp: Instant::now(),
                });
            }
            AppAction::Redraw
        }
        "/status" => {
            let elapsed = state.session_start.elapsed();
            let provider_info = state
                .config
                .active_provider()
                .map(|(name, _)| name.to_string())
                .unwrap_or_else(|| "none (use /connect)".into());
            state.messages.push(Message {
                role: MessageRole::System,
                content: vec![MessageContent::Text(format!(
                    "Session: {} | Provider: {} | Mode: {} | cwd: {}\n  Messages: {} | Context: {:.0}%",
                    AppState::format_duration(elapsed),
                    provider_info,
                    state.permission_mode.title(),
                    state.cwd,
                    state.messages.len(),
                    state.context_window_percent,
                ))],
                timestamp: Instant::now(),
            });
            AppAction::Redraw
        }
        "/vim" => {
            state.messages.push(Message {
                role: MessageRole::System,
                content: vec![MessageContent::Text("Vim mode toggled".into())],
                timestamp: Instant::now(),
            });
            AppAction::Redraw
        }
        _ => {
            state.messages.push(Message {
                role: MessageRole::System,
                content: vec![MessageContent::Text(format!(
                    "Unknown command: {}",
                    command
                ))],
                timestamp: Instant::now(),
            });
            AppAction::Redraw
        }
    }
}

fn flat_to_row_col(lines: &[&str], pos: usize) -> (usize, usize) {
    let mut rem = pos;
    for (i, ln) in lines.iter().enumerate() {
        let len = ln.chars().count();
        if rem <= len || i == lines.len() - 1 {
            return (i, rem.min(len));
        }
        rem -= len + 1;
    }
    (0, 0)
}

fn row_col_to_flat(lines: &[&str], row: usize, col: usize) -> usize {
    let mut pos = 0;
    for (i, ln) in lines.iter().enumerate() {
        if i == row {
            return pos + col.min(ln.chars().count());
        }
        pos += ln.chars().count() + 1;
    }
    pos
}

fn check_file_mention_mode(state: &mut AppState) {
    // Find the last `@` before cursor that starts a mention
    let text_before_cursor: String = state.input.chars().take(state.cursor_pos).collect();
    if let Some(at_pos) = text_before_cursor.rfind('@') {
        let after_at = &text_before_cursor[at_pos + 1..];
        // Only trigger if after_at has no whitespace (still typing)
        if !after_at.contains(' ') && !after_at.contains('\n') {
            let filter = after_at.to_string();
            if !state.file_mention_active || state.file_mention_filter != filter {
                state.file_mention_filter = filter.clone();
                state.file_mention_options = state.scan_files_for_mention(&filter);
                state.file_mention_selected = 0;
                state.file_mention_active = true;
            }
            return;
        }
    }
    state.file_mention_active = false;
}

fn handle_file_mention(state: &mut AppState, key: KeyEvent) -> AppAction {
    match key.code {
        KeyCode::Esc => {
            state.file_mention_active = false;
            AppAction::Redraw
        }
        KeyCode::Up => {
            state.file_mention_selected = state.file_mention_selected.saturating_sub(1);
            AppAction::Redraw
        }
        KeyCode::Down => {
            let max = state.file_mention_options.len().saturating_sub(1);
            if state.file_mention_selected < max {
                state.file_mention_selected += 1;
            }
            AppAction::Redraw
        }
        KeyCode::Tab | KeyCode::Enter => {
            if let Some(path) = state
                .file_mention_options
                .get(state.file_mention_selected)
                .cloned()
            {
                // Replace @filter with @path  in input
                let text_before: String = state.input.chars().take(state.cursor_pos).collect();
                let text_after: String = state.input.chars().skip(state.cursor_pos).collect();
                if let Some(at_pos) = text_before.rfind('@') {
                    let new_before = format!("@{} ", path);
                    let new_text =
                        format!("{}{}{}", &text_before[..at_pos], new_before, text_after);
                    let new_cursor = at_pos + new_before.chars().count();
                    state.input = new_text;
                    state.cursor_pos = new_cursor;
                }
                state.file_mention_active = false;
            }
            AppAction::Redraw
        }
        KeyCode::Backspace => {
            // Let normal backspace run, then recheck
            if state.cursor_pos > 0 {
                state.cursor_pos -= 1;
                if let Some(byte_idx) = state
                    .input
                    .char_indices()
                    .nth(state.cursor_pos)
                    .map(|(b, _)| b)
                {
                    state.input.remove(byte_idx);
                }
            }
            check_file_mention_mode(state);
            AppAction::Redraw
        }
        KeyCode::Char(c) => {
            let byte_idx = state
                .input
                .char_indices()
                .nth(state.cursor_pos)
                .map(|(b, _)| b)
                .unwrap_or(state.input.len());
            state.input.insert(byte_idx, c);
            state.cursor_pos += 1;
            check_file_mention_mode(state);
            AppAction::Redraw
        }
        _ => AppAction::None,
    }
}

/// Expand @path references in the message text, appending file contents.
pub fn expand_file_mentions(text: &str, cwd: &str) -> String {
    let mut result = String::new();
    let mut appended_files = Vec::new();

    // Split off any @path tokens and collect their contents
    let mut remaining = text.to_string();
    while let Some(at_pos) = remaining.find('@') {
        result.push_str(&remaining[..at_pos]);
        let after = &remaining[at_pos + 1..];
        // Find end of the path token (space, newline, or end)
        let end = after
            .find(|c: char| c == ' ' || c == '\n')
            .unwrap_or(after.len());
        let path_str = &after[..end];
        if path_str.is_empty() {
            result.push('@');
            remaining = remaining[at_pos + 1..].to_string();
            continue;
        }
        let _full_path = std::path::Path::new(cwd).join(path_str);
        if !appended_files.contains(&path_str.to_string()) {
            appended_files.push(path_str.to_string());
        }
        // Inline reference: just keep the @path token visually, content appended at the end
        result.push_str(&format!("@{}", path_str));
        remaining = after[end..].to_string();
    }
    result.push_str(&remaining);

    // Append file contents at the end
    if !appended_files.is_empty() {
        result.push_str("\n\n---");
        for path_str in &appended_files {
            let full_path = std::path::Path::new(cwd).join(path_str);
            match std::fs::read_to_string(&full_path) {
                Ok(content) => {
                    result.push_str(&format!("\n@{}\n```\n{}\n```", path_str, content));
                }
                Err(e) => {
                    result.push_str(&format!("\n@{} (error reading: {})", path_str, e));
                }
            }
        }
        result.push_str("\n---");
    }

    result
}
