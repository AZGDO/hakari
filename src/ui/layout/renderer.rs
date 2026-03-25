use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, Padding, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Wrap,
    },
    Frame,
};

use unicode_width::UnicodeWidthStr;

use crate::ui::wrapping::{compute_visual_cursor, visual_lines_for_line, wrapped_height};

use crate::config::ConnectPhase;
use crate::dialog::{render_dialog, DialogConfig, DialogRow};
use crate::state::AppState;
use crate::theme::Theme;
use crate::types::*;

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

const GRADIENT_COLORS: &[(u8, u8, u8)] = &[
    (235, 95, 87),
    (245, 139, 87),
    (250, 195, 95),
    (145, 200, 130),
    (130, 170, 220),
    (155, 130, 200),
    (200, 130, 180),
];

const CLAWD_ART: &[&str] = &[
    "████████████▓▒░░░░░░░░░▒▓███████████████",
    "███████▓▒░░░░░░░▒▒░░░▒▒░░░▒█████████████",
    "█████▒░░░░░░░░░░░░░░░░░░░░░░████████████",
    "███▓░░░░░░░░░░░░░░░░░░░▒░░░▒░▒▒▒████████",
    "██▒░░▒░░░░░░░░░░▒▒▒░░▒▒▒▒░░░▒░░░░░░░▒▓██",
    "██▒░▒░░░░░░░░░░░▒▒▒▒▒▒▒▒▒▒▒▒░░░░░░░░░░░░",
    "██▒▒▒░░░▒▒▒▒▒▒▒▒▒▓▒▒▒▓▒▒░▒▒▒░░░░░░░░░░░░",
    "██▒▒▒▒▒▒▒▒▒▒▒▒▒▓▒░▒▒▒▒▒▒░░░▒▒░▒▒██▓░░░░░",
    "██▒▒▒▒▒▒▒▒▒▒▒▒▒▒░░▒▒▒▒░▒░▒▒▒▒▒▒▒██▓▓▒▒▓▓",
    "██▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒░░░░░░░░░░▒▒▒███▓▓▓▓▓",
    "██▒▒▒▒▒▒▒▒▒▒▒▒▓░░░░░░░░░░░░░▒▒▒▒▓████▓▓▓",
    "██▒░░▒▒▒▒▒▒▒▒░░░░░░░░░░░░░░░▒▒▒▒▓█████▓▓",
    "██░░░░░░▒▒▒▒▓▒░░░░░░░░░░░░░░▓▒▒▒▒█████▓▓",
    "█▒░░░░░▒▒▒▒▒▓▓▒░░░░░░░░░░░▒▓▓▒▒▒▓▓▓▓▓███",
    "██▒░░░░░▒▒▒▒▓▓▓▓▒▒▒░░░░░▒▓▓▓▒▒▒▒▓▓▓▓▓▓▓▓",
    "███▒▓▓▓▓▒▒▒▒▒▓▓▓▒▒▒▒▒▒▒▒▒▒▓▓▒▒▒▒▓▓▓▓▓▓▓▓",
    "████▓█▓█▓▒▒▒▒▓▓▒▒▒▒▒▒▒▒▒▒▒▓▒▒▒▒▓▓▓▓▓▓███",
    "██████▓▓▓▓▒▒▒▒▓▒▒▒▒▒▒▒▒▒▒░▒▒▒▒▓▓▓▓▓▓▓▓▓▓",
    "█████▓▓▓▓▓▓▓▒▒▒▒░░░▒▒▒▓░░░▒▒▓▓▓▓▓▓▓▓▓▓▓▓",
    "█████▓▓██▓▓▓▓▓▓▓▒▒░░▒▓▓▒░░░▓▓▓▓▓▓▓▓█▓▓██",
];

pub fn render(frame: &mut Frame, state: &mut AppState) {
    let area = frame.area();
    let theme = &state.theme;

    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background)),
        area,
    );

    if state.show_welcome && state.messages.is_empty() {
        render_welcome_screen(frame, state, area);
    } else {
        render_chat_screen(frame, state, area);
    }

    if state.mode == AppMode::SlashCommand {
        render_slash_menu(frame, state, area);
    }

    if state.file_mention_active {
        render_file_mention_menu(frame, state, area);
    }

    if state.mode == AppMode::PermissionPrompt {
        render_permission_dialog(frame, state, area);
    }

    if state.mode == AppMode::Help {
        render_help_overlay(frame, state, area);
    }

    if state.mode == AppMode::ModelPicker {
        render_model_picker(frame, state, area);
    }

    if state.mode == AppMode::Settings {
        render_settings_panel(frame, state, area);
    }

    if state.mode == AppMode::Connect {
        render_connect_dialog(frame, state, area);
    }

    if state.mode == AppMode::SessionPicker {
        render_session_picker(frame, state, area);
    }

    // Custom text selection: highlight + extract
    render_selection(frame, state);
}

/// Normalize selection so start ≤ end in reading order.
fn normalize_selection(a: (u16, u16), b: (u16, u16)) -> ((u16, u16), (u16, u16)) {
    if a.1 < b.1 || (a.1 == b.1 && a.0 <= b.0) {
        (a, b)
    } else {
        (b, a)
    }
}

fn render_selection(frame: &mut Frame, state: &mut AppState) {
    let sel = match state.selection {
        Some(ref s) if s.active => s,
        _ => return,
    };

    let (start, end) = normalize_selection(sel.anchor, sel.end);
    let buf = frame.buffer_mut();
    let w = buf.area.width;

    // Apply highlight and optionally extract text
    let extracting = state.clipboard_pending;
    let mut extracted = if extracting {
        Some(String::new())
    } else {
        None
    };

    for row in start.1..=end.1 {
        let col_start = if row == start.1 { start.0 } else { 0 };
        let col_end = if row == end.1 {
            end.0
        } else {
            w.saturating_sub(1)
        };

        let mut line_text = String::new();
        for col in col_start..=col_end {
            if let Some(cell) = buf.cell_mut(Position::new(col, row)) {
                // Highlight: swap fg/bg
                let fg = cell.fg;
                let bg = cell.bg;
                cell.set_fg(if bg == Color::Reset { Color::Black } else { bg });
                cell.set_bg(if fg == Color::Reset { Color::White } else { fg });

                if extracting {
                    line_text.push_str(cell.symbol());
                }
            }
        }

        if let Some(ref mut text) = extracted {
            text.push_str(line_text.trim_end());
            if row < end.1 {
                text.push('\n');
            }
        }
    }

    if let Some(text) = extracted {
        state.clipboard_text = Some(text);
        state.clipboard_pending = false;
    }
}

fn gradient_char(ch: char, pos: usize, total: usize, offset: f64) -> Span<'static> {
    let t = (pos as f64 / total.max(1) as f64 + offset) % 1.0;
    let idx = t * (GRADIENT_COLORS.len() - 1) as f64;
    let lower = idx.floor() as usize;
    let upper = (lower + 1).min(GRADIENT_COLORS.len() - 1);
    let frac = idx - lower as f64;

    let (r1, g1, b1) = GRADIENT_COLORS[lower];
    let (r2, g2, b2) = GRADIENT_COLORS[upper];
    let r = (r1 as f64 * (1.0 - frac) + r2 as f64 * frac) as u8;
    let g = (g1 as f64 * (1.0 - frac) + g2 as f64 * frac) as u8;
    let b = (b1 as f64 * (1.0 - frac) + b2 as f64 * frac) as u8;

    Span::styled(
        ch.to_string(),
        Style::default()
            .fg(Color::Rgb(r, g, b))
            .add_modifier(Modifier::BOLD),
    )
}

pub fn get_input_height(state: &AppState, area_width: u16, is_busy: bool) -> u16 {
    if is_busy {
        return 1;
    }
    let prompt_width: usize = 2; // "❯ "
    let available_width = area_width.saturating_sub(prompt_width as u16) as usize;

    let input_lines_for_height: Vec<Line> = state
        .input
        .split('\n')
        .map(|s| Line::from(s.to_string()))
        .collect();

    wrapped_height(&input_lines_for_height, available_width)
        .max(1)
        .min(10) as u16
}

fn render_welcome_screen(frame: &mut Frame, state: &AppState, area: Rect) {
    let is_busy = state.is_loading || state.streaming.is_some() || state.pending_stream.is_some();
    let input_line_count = get_input_height(state, area.width, is_busy);

    // Layout: welcome box, blank, input area, status bar
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(1),                // blank line
            Constraint::Length(1),                // input separator top
            Constraint::Length(input_line_count), // input (dynamic)
            Constraint::Length(1),                // input separator bottom
            Constraint::Length(1),                // status bar
        ])
        .split(area);

    let welcome_area = main_chunks[0];
    let sep_top_area = main_chunks[2];
    let input_line_area = main_chunks[3];
    let sep_bottom_area = main_chunks[4];
    let status_area = main_chunks[5];

    // Welcome box with rounded border
    render_welcome_box(frame, state, welcome_area);

    // Input area with horizontal rules: delegate to shared input renderer
    crate::ui::input::render_input_with_rules(
        frame,
        state,
        sep_top_area,
        input_line_area,
        sep_bottom_area,
    );

    // Status bar
    render_status_bar(frame, state, status_area);
}

pub fn render_welcome_box(frame: &mut Frame, state: &AppState, area: Rect) {
    let theme = &state.theme;

    // art width = 40 chars + 2 border + 2 padding
    let art_width: u16 = 40 + 4;
    let box_width = area.width.min(art_width.max(50));
    // art lines + header lines + info lines + borders
    let content_lines: u16 = CLAWD_ART.len() as u16 + 6;
    let box_height = area.height.min(content_lines + 2);
    let box_x = area.x + (area.width.saturating_sub(box_width)) / 2;
    let box_y = area.y + (area.height.saturating_sub(box_height)) / 2;
    let box_area = Rect::new(box_x, box_y, box_width, box_height);

    let title_spans: Vec<Span> =
        std::iter::once(Span::styled("─── ", Style::default().fg(theme.subtle)))
            .chain(
                "HAKARI"
                    .chars()
                    .enumerate()
                    .map(|(i, c)| gradient_char(c, i, 6, state.shimmer_offset)),
            )
            .chain(std::iter::once(Span::styled(
                " v0.1.0 ",
                Style::default().fg(theme.inactive),
            )))
            .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.subtle))
        .title(Line::from(title_spans))
        .padding(Padding::new(1, 1, 0, 0));

    let inner = block.inner(box_area);
    frame.render_widget(block, box_area);

    if inner.width < 10 || inner.height < 3 {
        return;
    }

    let mut lines: Vec<Line> = Vec::new();

    // Art centered in inner width
    let art_w = CLAWD_ART
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0) as u16;
    let art_pad = (inner.width.saturating_sub(art_w)) / 2;
    let pad_str = " ".repeat(art_pad as usize);
    for art_line in CLAWD_ART {
        lines.push(Line::from(Span::styled(
            format!("{}{}", pad_str, art_line),
            Style::default().fg(theme.claude),
        )));
    }

    lines.push(Line::from(""));

    // Connection status
    let has_provider = state.config.active_provider().is_some();
    if has_provider {
        let (provider_name, _) = state.config.active_provider().unwrap();
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("●", Style::default().fg(theme.success)),
            Span::styled(
                format!(" Connected: {}", provider_name),
                Style::default().fg(theme.inactive),
            ),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("●", Style::default().fg(theme.error)),
            Span::styled(
                " No provider configured. Run ",
                Style::default().fg(theme.inactive),
            ),
            Span::styled("/connect", Style::default().fg(theme.suggestion)),
        ]));
    }

    // Model + cwd
    let model_display = state.format_model_short();
    lines.push(Line::from(Span::styled(
        format!("  {} (1M context)", model_display),
        Style::default().fg(theme.inactive),
    )));

    let cwd_short = shorten_path(&state.cwd, inner.width.saturating_sub(4) as usize);
    lines.push(Line::from(Span::styled(
        format!("  {}", cwd_short),
        Style::default().fg(theme.inactive),
    )));

    // Build a single horizontal welcome line using per-character gradient spans
    let mut welcome_spans: Vec<Span> = Vec::new();
    // Optional left padding to give a little breathing room
    welcome_spans.push(Span::raw(" "));
    welcome_spans.extend(
        "HAKARI"
            .chars()
            .enumerate()
            .map(|(i, c)| gradient_char(c, i, 6, state.shimmer_offset)),
    );
    welcome_spans.push(Span::styled(
        " v0.1.0 ",
        Style::default().fg(theme.inactive),
    ));

    // Insert the welcome line before the other info lines so it's prominent
    // Place it as the first info line after the art
    // Find the index after the art lines (art len)
    let insert_idx = CLAWD_ART.len();
    if insert_idx <= lines.len() {
        lines.insert(insert_idx, Line::from(welcome_spans));
    } else {
        // fallback: append
        lines.push(Line::from(welcome_spans));
    }

    // Render paragraph centered horizontally. Wrapping kept default.
    let para = Paragraph::new(lines)
        .wrap(Wrap::default())
        .alignment(Alignment::Center);
    frame.render_widget(para, inner);
}

fn render_chat_screen(frame: &mut Frame, state: &mut AppState, area: Rect) {
    let is_busy = state.is_loading || state.streaming.is_some() || state.pending_stream.is_some();

    // Input height = number of lines in input (min 1, max 10)
    let input_line_count = if is_busy {
        1u16
    } else {
        state.input.split('\n').count().max(1).min(10) as u16
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(1),                // separator top
            Constraint::Length(input_line_count), // input (dynamic)
            Constraint::Length(1),                // separator bottom
            Constraint::Length(1),                // status bar
        ])
        .split(area);

    let messages_area = chunks[0];
    let sep_top_area = chunks[1];
    let input_line_area = chunks[2];
    let sep_bottom_area = chunks[3];
    let status_area = chunks[4];

    render_messages(frame, state, messages_area);
    // Delegate input rendering to ui::input to avoid duplication
    crate::ui::input::render_input_with_rules(
        frame,
        state,
        sep_top_area,
        input_line_area,
        sep_bottom_area,
    );
    render_status_bar(frame, state, status_area);
}

fn render_messages(frame: &mut Frame, state: &mut AppState, area: Rect) {
    let theme = &state.theme;
    let mut lines: Vec<Line> = Vec::new();
    state.shizuka_block_rows.clear();
    // Track the raw line index where each message starts.
    let mut msg_line_starts: Vec<usize> = Vec::new();

    for (msg_idx, msg) in state.messages.iter().enumerate() {
        msg_line_starts.push(lines.len());
        match msg.role {
            MessageRole::User => {
                if msg_idx > 0 {
                    lines.push(Line::from(""));
                }
                let text = msg
                    .content
                    .iter()
                    .filter_map(|c| match c {
                        MessageContent::Text(t) => Some(t.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");

                // Render multi-line user messages
                let mut first = true;
                for line in text.lines() {
                    if first {
                        lines.push(Line::from(vec![
                            Span::styled(
                                "❯ ",
                                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                line.to_string(),
                                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
                            ),
                        ]));
                        first = false;
                    } else {
                        lines.push(Line::from(vec![
                            Span::styled("  ", Style::default()),
                            Span::styled(
                                line.to_string(),
                                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
                            ),
                        ]));
                    }
                }
                if first {
                    // empty message
                    lines.push(Line::from(Span::styled(
                        "❯ ",
                        Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
                    )));
                }
            }
            MessageRole::Assistant => {
                lines.push(Line::from(""));
                for content in &msg.content {
                    match content {
                        MessageContent::Text(text) => {
                            render_markdown_text(&mut lines, text, theme, area.width as usize);
                        }
                        MessageContent::CodeBlock { language, code } => {
                            render_code_block_lines(
                                &mut lines,
                                language,
                                code,
                                theme,
                                area.width as usize,
                            );
                        }
                        MessageContent::ToolUse(tool) => {
                            render_tool_use_lines(&mut lines, tool, theme, area.width as usize);
                        }
                        MessageContent::Thinking(text) => {
                            render_thinking_lines(&mut lines, text, theme);
                        }
                        MessageContent::DiffBlock {
                            file_path,
                            added,
                            removed,
                        } => {
                            render_diff_block_lines(
                                &mut lines,
                                file_path,
                                added,
                                removed,
                                theme,
                                area.width as usize,
                            );
                        }
                        MessageContent::ShizukaBlock { .. } => {}
                    }
                }

                if msg_idx == state.messages.len() - 1 {
                    if let Some(d) = state.last_response_duration {
                        if state.show_turn_duration {
                            lines.push(Line::from(""));
                            lines.push(Line::from(Span::styled(
                                format!("  Cooked for {}", AppState::format_duration(d)),
                                Style::default()
                                    .fg(theme.inactive)
                                    .add_modifier(Modifier::ITALIC),
                            )));
                        }
                    }
                }
            }
            MessageRole::System => {
                lines.push(Line::from(""));
                for content in &msg.content {
                    match content {
                        MessageContent::Text(text) => {
                            for line in text.lines() {
                                lines.push(Line::from(Span::styled(
                                    format!("  {}", line),
                                    Style::default()
                                        .fg(theme.inactive)
                                        .add_modifier(Modifier::ITALIC),
                                )));
                            }
                        }
                        MessageContent::ShizukaBlock {
                            preloaded,
                            referenced,
                            task_summary,
                            classification,
                            collapsed,
                        } => {
                            let block_start_row = lines.len();
                            state.shizuka_block_rows.insert(msg_idx, block_start_row);

                            let total = preloaded.len() + referenced.len();
                            let toggle_hint = if *collapsed { " ▶" } else { " ▼" };
                            lines.push(Line::from(vec![
                                Span::styled("  ◆ ", Style::default().fg(theme.permission)),
                                Span::styled(
                                    format!(
                                        "Shizuka · {} · {} file{}",
                                        classification,
                                        total,
                                        if total == 1 { "" } else { "s" }
                                    ),
                                    Style::default()
                                        .fg(theme.permission)
                                        .add_modifier(Modifier::BOLD),
                                ),
                                Span::styled(
                                    toggle_hint.to_string(),
                                    Style::default().fg(theme.subtle),
                                ),
                            ]));

                            if !collapsed {
                                // Task summary
                                lines.push(Line::from(vec![
                                    Span::styled("  │  ", Style::default().fg(theme.subtle)),
                                    Span::styled(
                                        task_summary.clone(),
                                        Style::default()
                                            .fg(theme.inactive)
                                            .add_modifier(Modifier::ITALIC),
                                    ),
                                ]));

                                if !preloaded.is_empty() {
                                    lines.push(Line::from(vec![
                                        Span::styled("  │  ", Style::default().fg(theme.subtle)),
                                        Span::styled(
                                            "Loaded for editing:",
                                            Style::default()
                                                .fg(theme.text)
                                                .add_modifier(Modifier::BOLD),
                                        ),
                                    ]));
                                    for path in preloaded {
                                        let short = shorten_path(path, 50);
                                        lines.push(Line::from(vec![
                                            Span::styled(
                                                "  │    ",
                                                Style::default().fg(theme.subtle),
                                            ),
                                            Span::styled("✎ ", Style::default().fg(theme.success)),
                                            Span::styled(short, Style::default().fg(theme.text)),
                                        ]));
                                    }
                                }

                                if !referenced.is_empty() {
                                    lines.push(Line::from(vec![
                                        Span::styled("  │  ", Style::default().fg(theme.subtle)),
                                        Span::styled(
                                            "Referenced:",
                                            Style::default()
                                                .fg(theme.text)
                                                .add_modifier(Modifier::BOLD),
                                        ),
                                    ]));
                                    for path in referenced {
                                        let short = shorten_path(path, 50);
                                        lines.push(Line::from(vec![
                                            Span::styled(
                                                "  │    ",
                                                Style::default().fg(theme.subtle),
                                            ),
                                            Span::styled("· ", Style::default().fg(theme.inactive)),
                                            Span::styled(
                                                short,
                                                Style::default().fg(theme.inactive),
                                            ),
                                        ]));
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Loading spinner
    if state.is_loading || state.pending_stream.is_some() || state.agent_phase != AgentPhase::Idle {
        lines.push(Line::from(""));
        let spinner = SPINNER_FRAMES[state.spinner_frame % SPINNER_FRAMES.len()];

        // Different colors for Shizuka (blue) vs Nano (orange) phases
        let (base_r, base_g, base_b, shim_r, shim_g, shim_b) = match state.agent_phase {
            AgentPhase::Shizuka => (177u8, 185u8, 249u8, 207u8, 215u8, 255u8),
            _ => (215u8, 119u8, 87u8, 245u8, 179u8, 117u8),
        };

        let shimmer_t = (state.shimmer_offset * 3.0).sin() * 0.5 + 0.5;
        let sr = (base_r as f64 * (1.0 - shimmer_t) + shim_r as f64 * shimmer_t) as u8;
        let sg = (base_g as f64 * (1.0 - shimmer_t) + shim_g as f64 * shimmer_t) as u8;
        let sb = (base_b as f64 * (1.0 - shimmer_t) + shim_b as f64 * shimmer_t) as u8;
        let shimmer_color = Color::Rgb(sr, sg, sb);

        lines.push(Line::from(vec![
            Span::styled(
                format!("  {} ", spinner),
                Style::default().fg(shimmer_color),
            ),
            Span::styled(
                state.loading_text.clone(),
                Style::default()
                    .fg(shimmer_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    // Compact notifications
    for notif in &state.compact_notifications {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  {}", notif.0),
            Style::default()
                .fg(theme.inactive)
                .add_modifier(Modifier::ITALIC),
        )));
    }

    let total_lines = lines.len();
    let visible = area.height as usize;
    let width = area.width as usize;

    // Compute the actual rendered row count (accounting for word-wrap)
    let rendered_rows = wrapped_height(&lines, width);
    // Store for scroll bounds (rows beyond visible area)
    state.total_message_lines = rendered_rows.saturating_sub(visible);

    // Compute wrapped-row offset for each message start.
    state.message_row_starts = msg_line_starts
        .iter()
        .map(|&raw_idx| wrapped_height(&lines[..raw_idx], width))
        .collect();

    // Auto-scroll to bottom only when not manually scrolled
    if !state.user_scrolled {
        state.scroll_offset = rendered_rows.saturating_sub(visible);
    }
    let _ = total_lines;

    let paragraph = Paragraph::new(lines.clone())
        .scroll((state.scroll_offset as u16, 0))
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);

    if total_lines > visible {
        let mut scrollbar_state = ScrollbarState::new(total_lines).position(state.scroll_offset);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_symbol(Some(" "))
                .thumb_symbol("┃"),
            area,
            &mut scrollbar_state,
        );
    }
}

fn render_markdown_text<'a>(lines: &mut Vec<Line<'a>>, text: &str, theme: &Theme, _width: usize) {
    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut code_lines = Vec::new();

    for line in text.lines() {
        if line.starts_with("```") {
            if in_code_block {
                render_code_block_lines(lines, &code_lang, &code_lines.join("\n"), theme, _width);
                code_lines.clear();
                code_lang.clear();
                in_code_block = false;
            } else {
                in_code_block = true;
                code_lang = line.trim_start_matches('`').to_string();
            }
            continue;
        }

        if in_code_block {
            code_lines.push(line.to_string());
            continue;
        }

        if line.is_empty() {
            lines.push(Line::from(""));
            continue;
        }

        if let Some(rest) = line.strip_prefix("### ") {
            lines.push(Line::from(Span::styled(
                format!("  {}", rest),
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            )));
        } else if let Some(rest) = line.strip_prefix("## ") {
            lines.push(Line::from(Span::styled(
                format!("  {}", rest),
                Style::default()
                    .fg(theme.text)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )));
        } else if let Some(rest) = line.strip_prefix("# ") {
            lines.push(Line::from(Span::styled(
                format!("  {}", rest),
                Style::default()
                    .fg(theme.text)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )));
        } else if line.starts_with("- ") || line.starts_with("* ") {
            let content = &line[2..];
            lines.push(Line::from(vec![
                Span::styled("  • ", Style::default().fg(theme.inactive)),
                Span::styled(content.to_string(), Style::default().fg(theme.text)),
            ]));
        } else if line.len() > 2
            && line.chars().next().is_some_and(|c| c.is_ascii_digit())
            && line.contains(". ")
        {
            lines.push(Line::from(Span::styled(
                format!("  {}", line),
                Style::default().fg(theme.text),
            )));
        } else if let Some(rest) = line.strip_prefix("> ") {
            lines.push(Line::from(vec![
                Span::styled("  │ ", Style::default().fg(theme.subtle)),
                Span::styled(rest.to_string(), Style::default().fg(theme.inactive)),
            ]));
        } else {
            let spans = parse_inline_markdown(line, theme);
            let mut full_spans = vec![Span::styled("  ", Style::default())];
            full_spans.extend(spans);
            lines.push(Line::from(full_spans));
        }
    }

    if in_code_block && !code_lines.is_empty() {
        render_code_block_lines(lines, &code_lang, &code_lines.join("\n"), theme, _width);
    }
}

fn parse_inline_markdown(text: &str, theme: &Theme) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '`' {
            if !current.is_empty() {
                spans.push(Span::styled(
                    current.clone(),
                    Style::default().fg(theme.text),
                ));
                current.clear();
            }
            i += 1;
            let mut code = String::new();
            while i < chars.len() && chars[i] != '`' {
                code.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            spans.push(Span::styled(
                code,
                Style::default()
                    .fg(theme.claude)
                    .add_modifier(Modifier::BOLD),
            ));
            continue;
        }

        if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
            if !current.is_empty() {
                spans.push(Span::styled(
                    current.clone(),
                    Style::default().fg(theme.text),
                ));
                current.clear();
            }
            i += 2;
            let mut bold = String::new();
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '*') {
                bold.push(chars[i]);
                i += 1;
            }
            if i + 1 < chars.len() {
                i += 2;
            }
            spans.push(Span::styled(
                bold,
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            ));
            continue;
        }

        current.push(chars[i]);
        i += 1;
    }

    if !current.is_empty() {
        spans.push(Span::styled(current, Style::default().fg(theme.text)));
    }

    spans
}

fn render_code_block_lines<'a>(
    lines: &mut Vec<Line<'a>>,
    language: &str,
    code: &str,
    theme: &Theme,
    width: usize,
) {
    let border_color = theme.bash_border;
    let lang_display = if language.is_empty() {
        "text"
    } else {
        language
    };
    let box_width = (width.saturating_sub(6)).max(20);

    let top_after_lang = box_width.saturating_sub(lang_display.len() + 3);
    lines.push(Line::from(vec![
        Span::styled("  ╭─ ", Style::default().fg(border_color)),
        Span::styled(
            lang_display.to_string(),
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", "─".repeat(top_after_lang)),
            Style::default().fg(border_color),
        ),
    ]));

    for line in code.lines() {
        lines.push(Line::from(vec![
            Span::styled("  │ ", Style::default().fg(border_color)),
            Span::styled(line.to_string(), Style::default().fg(theme.text)),
        ]));
    }

    lines.push(Line::from(vec![Span::styled(
        format!("  ╰{}", "─".repeat(box_width)),
        Style::default().fg(border_color),
    )]));
}

fn render_tool_use_lines<'a>(
    lines: &mut Vec<Line<'a>>,
    tool: &ToolCall,
    theme: &Theme,
    width: usize,
) {
    let (status_icon, status_color) = match &tool.status {
        ToolStatus::Running(_) => ("⏵", theme.claude),
        ToolStatus::Complete(_) => ("✓", theme.success),
        ToolStatus::Error(_) => ("✗", theme.error),
    };

    let border_color = match &tool.status {
        ToolStatus::Running(_) => theme.bash_border,
        ToolStatus::Complete(_) => theme.success,
        ToolStatus::Error(_) => theme.error,
    };

    lines.push(Line::from(vec![
        Span::styled(
            format!("  {} ", status_icon),
            Style::default().fg(status_color),
        ),
        Span::styled(
            tool.name.clone(),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            if tool.args_summary.is_empty() {
                String::new()
            } else {
                format!(" ({})", tool.args_summary)
            },
            Style::default().fg(theme.inactive),
        ),
    ]));

    if !tool.collapsed {
        if let Some(output) = &tool.output {
            let box_width = (width.saturating_sub(6)).max(20);
            lines.push(Line::from(vec![Span::styled(
                format!("  ╭{}", "─".repeat(box_width)),
                Style::default().fg(border_color),
            )]));
            for line in output.lines().take(20) {
                lines.push(Line::from(vec![
                    Span::styled("  │ ".to_string(), Style::default().fg(border_color)),
                    Span::styled(line.to_string(), Style::default().fg(theme.text)),
                ]));
            }
            let total_lines = output.lines().count();
            if total_lines > 20 {
                lines.push(Line::from(vec![
                    Span::styled("  │ ".to_string(), Style::default().fg(border_color)),
                    Span::styled(
                        format!("... ({} more lines)", total_lines - 20),
                        Style::default().fg(theme.inactive),
                    ),
                ]));
            }
            lines.push(Line::from(vec![Span::styled(
                format!("  ╰{}", "─".repeat(box_width)),
                Style::default().fg(border_color),
            )]));
        }
    }
}

fn render_thinking_lines<'a>(lines: &mut Vec<Line<'a>>, text: &str, theme: &Theme) {
    lines.push(Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(
            "Thinking...",
            Style::default()
                .fg(theme.inactive)
                .add_modifier(Modifier::ITALIC | Modifier::DIM),
        ),
    ]));
    if !text.is_empty() {
        for line in text.lines() {
            lines.push(Line::from(Span::styled(
                format!("    {}", line),
                Style::default()
                    .fg(theme.inactive)
                    .add_modifier(Modifier::DIM),
            )));
        }
    }
}

fn render_diff_block_lines<'a>(
    lines: &mut Vec<Line<'a>>,
    file_path: &str,
    added: &[String],
    removed: &[String],
    theme: &Theme,
    width: usize,
) {
    let box_width = (width.saturating_sub(6)).max(20);
    let border_color = theme.success;

    lines.push(Line::from(vec![
        Span::styled("  ╭─ ", Style::default().fg(border_color)),
        Span::styled(
            file_path.to_string(),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                " {}",
                "─".repeat(box_width.saturating_sub(file_path.len() + 3))
            ),
            Style::default().fg(border_color),
        ),
    ]));

    for line in removed {
        lines.push(Line::from(vec![
            Span::styled("  │ ", Style::default().fg(border_color)),
            Span::styled(
                format!("- {}", line),
                Style::default().fg(theme.diff_removed_word),
            ),
        ]));
    }

    for line in added {
        lines.push(Line::from(vec![
            Span::styled("  │ ", Style::default().fg(border_color)),
            Span::styled(
                format!("+ {}", line),
                Style::default().fg(theme.diff_added_word),
            ),
        ]));
    }

    lines.push(Line::from(vec![Span::styled(
        format!("  ╰{}", "─".repeat(box_width)),
        Style::default().fg(border_color),
    )]));
}

pub fn render_status_bar(frame: &mut Frame, state: &AppState, area: Rect) {
    let theme = &state.theme;
    let is_copilot = state.config.is_copilot();

    // Left side: ? for shortcuts + usage info
    let mut left_parts: Vec<Span> = vec![
        Span::styled("  ", Style::default()),
        Span::styled(
            "?",
            Style::default()
                .fg(theme.suggestion)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" for shortcuts", Style::default().fg(theme.inactive)),
    ];

    if is_copilot {
        // Show requests used and remaining percentage for Copilot
        let usage = &state.copilot_usage;
        left_parts.push(Span::styled(
            format!("  {} reqs", usage.total_requests_used),
            Style::default().fg(theme.inactive),
        ));
        if usage.requests_used_this_prompt > 0 {
            left_parts.push(Span::styled(
                format!(" ({}/ prompt)", usage.requests_used_this_prompt),
                Style::default().fg(theme.inactive),
            ));
        }
        if let Some(ref limits) = usage.rate_limits {
            let remaining_pct = limits.remaining_percent();
            let color = if remaining_pct < 10.0 {
                theme.error
            } else if remaining_pct < 30.0 {
                theme.warning
            } else {
                theme.success
            };
            left_parts.push(Span::styled(
                format!(" {:.0}% left", remaining_pct),
                Style::default().fg(color),
            ));
            left_parts.push(Span::styled(
                format!(" ({}/{})", limits.remaining, limits.total),
                Style::default().fg(theme.inactive),
            ));
        }
    } else {
        if state.token_usage.total() > 0 {
            left_parts.push(Span::styled(
                format!(
                    "  {} tokens",
                    AppState::format_token_count(state.token_usage.total())
                ),
                Style::default().fg(theme.inactive),
            ));
        }

        if state.token_usage.cost_usd() > 0.001 {
            left_parts.push(Span::styled(
                format!(" ${:.2}", state.token_usage.cost_usd()),
                Style::default().fg(theme.inactive),
            ));
        }
    }

    if state.context_window_percent > 0.0 {
        let ctx_color = if state.context_window_percent > 80.0 {
            theme.warning
        } else {
            theme.inactive
        };
        left_parts.push(Span::styled(
            format!(" {:.0}% context", state.context_window_percent),
            Style::default().fg(ctx_color),
        ));
    }

    let left_line = Line::from(left_parts);

    // Right side: agent phase + effort indicator
    let phase_indicator = match state.agent_phase {
        AgentPhase::Shizuka => vec![
            Span::styled("● ", Style::default().fg(theme.permission)),
            Span::styled("preparing", Style::default().fg(theme.inactive)),
            Span::styled(" · ", Style::default().fg(theme.subtle)),
        ],
        AgentPhase::Nano => vec![
            Span::styled("● ", Style::default().fg(theme.claude)),
            Span::styled("coding", Style::default().fg(theme.inactive)),
            Span::styled(" · ", Style::default().fg(theme.subtle)),
        ],
        AgentPhase::Idle => vec![
            Span::styled("● ", Style::default().fg(theme.success)),
            Span::styled("ready", Style::default().fg(theme.inactive)),
            Span::styled(" · ", Style::default().fg(theme.subtle)),
        ],
    };

    let mut right_parts = phase_indicator;
    // Show current model name (more useful than raw provider name)
    let model_display =
        if state.model_name.is_empty() || state.model_name == "auto (selected by Shizuka)" {
            // Show configured providers instead
            let shizuka_p = &state.config.preferences.shizuka_provider;
            let nano_p = &state.config.preferences.nano_provider;
            if !shizuka_p.is_empty() && !nano_p.is_empty() && shizuka_p != nano_p {
                format!("{}/{}", shizuka_p, nano_p)
            } else {
                state
                    .config
                    .active_provider()
                    .map(|(name, _)| name.to_string())
                    .unwrap_or_else(|| "no provider".into())
            }
        } else {
            state.model_name.clone()
        };
    right_parts.push(Span::styled(
        model_display,
        Style::default().fg(theme.inactive),
    ));
    right_parts.push(Span::styled(" ", Style::default()));

    let right_line = Line::from(right_parts);

    let left_widget = Paragraph::new(left_line);
    let right_widget = Paragraph::new(right_line).alignment(Alignment::Right);

    frame.render_widget(left_widget, area);
    frame.render_widget(right_widget, area);
}

fn render_slash_menu(frame: &mut Frame, state: &mut AppState, area: Rect) {
    // Collect filtered commands into owned data to release borrow on state
    let commands: Vec<(String, String)> = state
        .filtered_commands()
        .iter()
        .map(|cmd| (cmd.name.clone(), cmd.description.clone()))
        .collect();

    if commands.is_empty() {
        return;
    }

    let theme = &state.theme;
    let visible_count = commands.len().min(12);
    let menu_height = (visible_count + 2) as u16;
    let menu_width = 55u16.min(area.width.saturating_sub(4));

    let input_top = area.height.saturating_sub(5);
    let menu_y = input_top.saturating_sub(menu_height);
    let menu_area = Rect::new(1, menu_y, menu_width, menu_height);

    frame.render_widget(Clear, menu_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.prompt_border))
        .title(Span::styled(
            " Commands ",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(menu_area);
    frame.render_widget(block, menu_area);

    let visible = inner.height as usize;
    let total = commands.len();

    // Keep selected item visible with persistent scroll
    if state.slash_selected < state.slash_scroll {
        state.slash_scroll = state.slash_selected;
    } else if state.slash_selected >= state.slash_scroll + visible {
        state.slash_scroll = state.slash_selected.saturating_sub(visible) + 1;
    }
    let scroll = state.slash_scroll.min(total.saturating_sub(visible));

    let mut lines = Vec::new();
    for (i, (name, desc)) in commands.iter().skip(scroll).take(visible).enumerate() {
        let actual_idx = i + scroll;
        let is_selected = actual_idx == state.slash_selected;

        let bg = if is_selected {
            theme.subtle
        } else {
            Color::Reset
        };
        let name_style = if is_selected {
            Style::default()
                .fg(theme.text)
                .bg(bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text)
        };
        let desc_style = Style::default().fg(theme.inactive);

        lines.push(Line::from(vec![
            Span::styled(format!(" /{:<16}", name), name_style),
            Span::styled(format!(" {}", desc), desc_style),
        ]));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

fn render_file_mention_menu(frame: &mut Frame, state: &AppState, area: Rect) {
    let theme = &state.theme;
    let options = &state.file_mention_options;

    if options.is_empty() {
        return;
    }

    let visible_count = options.len().min(10);
    let menu_height = (visible_count + 2) as u16;
    let menu_width = 55u16.min(area.width.saturating_sub(4));

    // Position above the input area (same as slash menu)
    let input_top = area.height.saturating_sub(5);
    let menu_y = input_top.saturating_sub(menu_height);
    let menu_area = Rect::new(1, menu_y, menu_width, menu_height);

    frame.render_widget(Clear, menu_area);

    let filter = &state.file_mention_filter;
    let title = if filter.is_empty() {
        " @file ".to_string()
    } else {
        format!(" @{} ", filter)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.success))
        .title(Span::styled(
            title,
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(menu_area);
    frame.render_widget(block, menu_area);

    let scroll = if state.file_mention_selected >= inner.height as usize {
        state.file_mention_selected - inner.height as usize + 1
    } else {
        0
    };

    let mut lines = Vec::new();
    for (i, path) in options
        .iter()
        .skip(scroll)
        .take(inner.height as usize)
        .enumerate()
    {
        let actual_idx = i + scroll;
        let is_selected = actual_idx == state.file_mention_selected;
        let bg = if is_selected {
            theme.subtle
        } else {
            Color::Reset
        };
        let style = if is_selected {
            Style::default()
                .fg(theme.text)
                .bg(bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text)
        };
        let prefix = if is_selected { " ▸ " } else { "   " };
        lines.push(Line::from(vec![
            Span::styled(prefix.to_string(), style),
            Span::styled(path.clone(), style),
        ]));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

fn render_permission_dialog(frame: &mut Frame, state: &AppState, area: Rect) {
    let theme = &state.theme;

    let Some(req) = &state.permission_request else {
        return;
    };

    let dialog_width = 60u16.min(area.width.saturating_sub(4));
    let has_command = req.command.is_some();
    let dialog_height = if has_command { 14u16 } else { 11u16 };
    let dialog_x = (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.permission))
        .title(Span::styled(
            " Permission Required ",
            Style::default()
                .fg(theme.permission)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let mut lines = Vec::new();

    lines.push(Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::styled(
            req.tool_name.clone(),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" wants to run:", Style::default().fg(theme.text)),
    ]));
    lines.push(Line::from(""));

    if let Some(cmd) = &req.command {
        lines.push(Line::from(vec![
            Span::styled(
                "  $ ",
                Style::default()
                    .fg(theme.bash_border)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(cmd.clone(), Style::default().fg(theme.text)),
        ]));
        lines.push(Line::from(""));
    }

    lines.push(Line::from(Span::styled(
        format!("  {}", &req.description),
        Style::default().fg(theme.inactive),
    )));
    lines.push(Line::from(""));

    let options = PermissionRequest::options();
    for (i, opt) in options.iter().enumerate() {
        let is_selected = i == req.selected_option;
        let prefix = if is_selected { " ▸ " } else { "   " };
        let style = if is_selected {
            Style::default()
                .fg(theme.permission)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text)
        };
        lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, opt),
            style,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(
            "y",
            Style::default()
                .fg(theme.suggestion)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" allow once · ", Style::default().fg(theme.inactive)),
        Span::styled(
            "n",
            Style::default()
                .fg(theme.suggestion)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" deny · ", Style::default().fg(theme.inactive)),
        Span::styled(
            "↑↓",
            Style::default()
                .fg(theme.suggestion)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" navigate", Style::default().fg(theme.inactive)),
    ]));

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

fn render_help_overlay(frame: &mut Frame, state: &mut AppState, area: Rect) {
    let theme = &state.theme;

    let help_entries: Vec<(&str, &str)> = vec![
        ("Keyboard Shortcuts", ""),
        ("  Enter", "Send message"),
        ("  Esc (x2)", "Exit"),
        ("  Ctrl+C", "Interrupt / quit"),
        ("  Ctrl+L", "Clear screen"),
        ("  Ctrl+T", "Toggle theme"),
        ("  ↑ / ↓", "Navigate history"),
        ("  /", "Open command menu"),
        ("  Tab", "Accept autocomplete"),
        ("", ""),
        ("Commands", ""),
        ("  /help", "Show help"),
        ("  /clear", "Clear conversation"),
        ("  /compact", "Compact context"),
        ("  /model", "Show provider config"),
        ("  /shizuka", "Set Shizuka provider"),
        ("  /nano", "Set Nano provider"),
        ("  /theme", "Toggle theme"),
        ("  /cost", "Show usage stats"),
        ("  /status", "Session info"),
        ("", ""),
        ("  Press Esc to close", ""),
    ];

    let mut rows: Vec<DialogRow> = Vec::new();
    for (key, desc) in help_entries {
        if key.is_empty() {
            rows.push(DialogRow::blank());
        } else if desc.is_empty() {
            rows.push(DialogRow::text(Line::from(Span::styled(
                format!(" {}", key),
                Style::default()
                    .fg(theme.text)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ))));
        } else {
            rows.push(DialogRow::text(Line::from(vec![
                Span::styled(
                    format!(" {:16}", key),
                    Style::default().fg(theme.suggestion),
                ),
                Span::styled(desc.to_string(), Style::default().fg(theme.text)),
            ])));
        }
    }

    let cfg = DialogConfig::new(" Help ", theme.prompt_border, theme.text);
    // Help has no selectable items, so we use a temporary state that discards scroll
    let mut help_state = crate::dialog::DialogState::new();
    render_dialog(frame, area, &cfg, &rows, &mut help_state);
}

fn render_model_picker(frame: &mut Frame, state: &mut AppState, area: Rect) {
    let theme = &state.theme;
    let models = &state.model_options;

    let title_text = match state.model_picker_target.as_str() {
        "shizuka" => " Shizuka Provider & Model ",
        "nano" => " Nano Provider & Model ",
        _ => " Select Model ",
    };

    // Count selectable items to know which one is the "custom" row
    let mut selectable_idx: usize = 0;
    let mut rows: Vec<DialogRow> = Vec::new();

    for (i, model) in models.iter().enumerate() {
        if model.is_header {
            if i > 0 {
                rows.push(DialogRow::blank());
            }
            rows.push(DialogRow::header(Line::from(Span::styled(
                format!(" ─ {} ─", model.display_name),
                Style::default()
                    .fg(theme.suggestion)
                    .add_modifier(Modifier::BOLD),
            ))));
            continue;
        }

        let is_selected = selectable_idx == state.dialog.selected && !state.model_picker_typing;
        let is_current = model.id == state.model_name;
        let prefix = if is_selected { " ▸ " } else { "   " };
        let suffix = if is_current { " (current)" } else { "" };
        let name_style = if is_selected {
            Style::default()
                .fg(theme.suggestion)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text)
        };

        let mut spans = vec![
            Span::styled(prefix.to_string(), name_style),
            Span::styled(model.display_name.to_string(), name_style),
        ];
        if !model.context_window.is_empty() {
            spans.push(Span::styled(
                format!(" ({})", model.context_window),
                Style::default().fg(theme.inactive),
            ));
        }
        if let Some(rate) = model.rate_multiplier {
            let rate_color = if rate < 0.5 {
                theme.success
            } else if rate <= 1.0 {
                theme.inactive
            } else {
                theme.warning
            };
            spans.push(Span::styled(
                format!(" {}x", rate),
                Style::default().fg(rate_color).add_modifier(Modifier::BOLD),
            ));
        }
        spans.push(Span::styled(
            suffix.to_string(),
            Style::default().fg(theme.success),
        ));

        let mut item_lines = vec![Line::from(spans)];
        if !model.description.is_empty() {
            item_lines.push(Line::from(Span::styled(
                format!("     {}", model.description),
                Style::default().fg(theme.inactive),
            )));
        }
        rows.push(DialogRow::item(item_lines));
        selectable_idx += 1;
    }

    // Custom model row (also selectable)
    let custom_is_selected = selectable_idx == state.dialog.selected;
    rows.push(DialogRow::blank());
    let custom_style =
        if (custom_is_selected && !state.model_picker_typing) || state.model_picker_typing {
            Style::default()
                .fg(theme.suggestion)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.inactive)
        };
    let custom_prefix = if custom_is_selected && !state.model_picker_typing {
        " ▸ "
    } else {
        "   "
    };
    let mut custom_lines = vec![Line::from(vec![
        Span::styled(custom_prefix.to_string(), custom_style),
        Span::styled("Custom model ID...".to_string(), custom_style),
    ])];
    if state.model_picker_typing {
        let input = &state.model_custom_input;
        let display = if input.is_empty() {
            "type model ID and press Enter".to_string()
        } else {
            input.clone()
        };
        custom_lines.push(Line::from(vec![
            Span::styled("     > ", Style::default().fg(theme.suggestion)),
            Span::styled(
                display,
                if input.is_empty() {
                    Style::default().fg(theme.subtle)
                } else {
                    Style::default().fg(theme.text)
                },
            ),
        ]));
    }
    rows.push(DialogRow::item(custom_lines));

    // Hint row (non-selectable)
    rows.push(DialogRow::blank());
    if state.model_picker_typing {
        rows.push(DialogRow::text(Line::from(vec![
            Span::styled(
                "  Enter",
                Style::default()
                    .fg(theme.suggestion)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" confirm · ", Style::default().fg(theme.inactive)),
            Span::styled(
                "Esc",
                Style::default()
                    .fg(theme.suggestion)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" back", Style::default().fg(theme.inactive)),
        ])));
    } else {
        rows.push(DialogRow::text(Line::from(vec![
            Span::styled(
                "  ↑↓",
                Style::default()
                    .fg(theme.suggestion)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" navigate · ", Style::default().fg(theme.inactive)),
            Span::styled(
                "Enter",
                Style::default()
                    .fg(theme.suggestion)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" select · ", Style::default().fg(theme.inactive)),
            Span::styled(
                "Esc",
                Style::default()
                    .fg(theme.suggestion)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" cancel", Style::default().fg(theme.inactive)),
        ])));
    }

    let cfg = DialogConfig::new(title_text, theme.suggestion, theme.text);
    render_dialog(frame, area, &cfg, &rows, &mut state.dialog);
}

fn render_settings_panel(frame: &mut Frame, state: &mut AppState, area: Rect) {
    let theme = &state.theme;
    let settings = &state.settings;

    let mut rows: Vec<DialogRow> = Vec::new();

    for (i, entry) in settings.iter().enumerate() {
        let is_selected = i == state.dialog.selected;
        let prefix = if is_selected { " ▸ " } else { "   " };

        let label_style = if is_selected {
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text)
        };

        let value_str = match &entry.value {
            SettingValue::Bool(v) => {
                if *v {
                    "✓ on".to_string()
                } else {
                    "✗ off".to_string()
                }
            }
            SettingValue::Choice { options, selected } => format!("◀ {} ▶", options[*selected]),
            SettingValue::Info(s) => s.clone(),
        };

        let value_color = match &entry.value {
            SettingValue::Bool(true) => theme.success,
            SettingValue::Bool(false) => theme.inactive,
            SettingValue::Choice { .. } => theme.suggestion,
            SettingValue::Info(_) => theme.inactive,
        };

        let label_width = 22;
        let padded_label = format!("{:<width$}", entry.label, width = label_width);

        rows.push(DialogRow::item1(Line::from(vec![
            Span::styled(prefix.to_string(), label_style),
            Span::styled(padded_label, label_style),
            Span::styled(
                value_str,
                Style::default()
                    .fg(value_color)
                    .add_modifier(if is_selected {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
        ])));
    }

    rows.push(DialogRow::blank());
    rows.push(DialogRow::text(Line::from(vec![
        Span::styled(
            "  ↑↓",
            Style::default()
                .fg(theme.suggestion)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" navigate · ", Style::default().fg(theme.inactive)),
        Span::styled(
            "Enter/←→",
            Style::default()
                .fg(theme.suggestion)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" change · ", Style::default().fg(theme.inactive)),
        Span::styled(
            "Esc",
            Style::default()
                .fg(theme.suggestion)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" close ", Style::default().fg(theme.inactive)),
    ])));

    let cfg = DialogConfig::new(" Settings ", theme.prompt_border, theme.text).width(70);
    render_dialog(frame, area, &cfg, &rows, &mut state.dialog);
}

fn mode_color(mode: &PermissionMode, theme: &Theme) -> Color {
    match mode {
        PermissionMode::Default => theme.text,
        PermissionMode::PlanMode => theme.plan_mode,
        PermissionMode::AcceptEdits => theme.auto_accept,
        PermissionMode::BypassPermissions => theme.error,
        PermissionMode::DontAsk => theme.error,
        PermissionMode::Auto => theme.warning,
    }
}

fn render_connect_dialog(frame: &mut Frame, state: &AppState, area: Rect) {
    let theme = &state.theme;
    let Some(ref cs) = state.connect_state else {
        return;
    };

    let dialog_width = 60u16.min(area.width.saturating_sub(4));
    let dialog_height = 18u16.min(area.height.saturating_sub(4));
    let dialog_x = (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    let title = match cs.phase {
        ConnectPhase::SelectProvider => " Connect Provider ",
        ConnectPhase::EnterApiKey => " Enter API Key ",
        ConnectPhase::Testing => " Testing Connection... ",
        ConnectPhase::Done => " Connected! ",
        ConnectPhase::CopilotDeviceFlow { .. } => " HAKARI - GitHub Auth ",
        ConnectPhase::CopilotPolling => " HAKARI - Connecting... ",
    };

    let border_color = match cs.phase {
        ConnectPhase::Done => theme.success,
        ConnectPhase::Testing | ConnectPhase::CopilotPolling => theme.claude,
        ConnectPhase::CopilotDeviceFlow { .. } => Color::Rgb(255, 150, 180),
        _ => theme.suggestion,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            title,
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let mut lines = Vec::new();

    match cs.phase {
        ConnectPhase::SelectProvider => {
            lines.push(Line::from(Span::styled(
                " Select a provider to connect:",
                Style::default().fg(theme.text),
            )));
            lines.push(Line::from(""));

            for (i, provider) in cs.providers.iter().enumerate() {
                let is_selected = i == cs.selected;
                let prefix = if is_selected { " > " } else { "   " };
                let status = if provider.connected {
                    " (connected)"
                } else {
                    ""
                };

                let name_style = if is_selected {
                    Style::default()
                        .fg(theme.suggestion)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text)
                };

                lines.push(Line::from(vec![
                    Span::styled(prefix.to_string(), name_style),
                    Span::styled(provider.display_name.clone(), name_style),
                    Span::styled(status.to_string(), Style::default().fg(theme.success)),
                ]));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    "  Enter",
                    Style::default()
                        .fg(theme.suggestion)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" select  ", Style::default().fg(theme.inactive)),
                Span::styled(
                    "Esc",
                    Style::default()
                        .fg(theme.suggestion)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" cancel", Style::default().fg(theme.inactive)),
            ]));
        }
        ConnectPhase::EnterApiKey => {
            let provider_name = &cs.providers[cs.selected].display_name;
            lines.push(Line::from(Span::styled(
                format!(" API key for {}:", provider_name),
                Style::default().fg(theme.text),
            )));
            lines.push(Line::from(""));

            // Masked API key input
            let masked: String = if cs.api_key_input.is_empty() {
                String::new()
            } else {
                let len = cs.api_key_input.len();
                if len <= 4 {
                    "*".repeat(len)
                } else {
                    format!(
                        "{}...{}",
                        safe_truncate_chars(&cs.api_key_input, 3),
                        &cs.api_key_input[cs
                            .api_key_input
                            .char_indices()
                            .nth(len.saturating_sub(3))
                            .map(|(i, _)| i)
                            .unwrap_or(0)..]
                    )
                }
            };

            lines.push(Line::from(vec![
                Span::styled("  > ", Style::default().fg(theme.suggestion)),
                Span::styled(
                    if masked.is_empty() {
                        "paste your API key here".to_string()
                    } else {
                        masked
                    },
                    Style::default().fg(if cs.api_key_input.is_empty() {
                        theme.inactive
                    } else {
                        theme.text
                    }),
                ),
            ]));

            // Show test result if any
            if let Some(ref result) = cs.test_result {
                lines.push(Line::from(""));
                match result {
                    Ok(msg) => {
                        lines.push(Line::from(Span::styled(
                            format!("  {} {}", "OK", msg),
                            Style::default().fg(theme.success),
                        )));
                    }
                    Err(msg) => {
                        lines.push(Line::from(Span::styled(
                            format!("  {} {}", "FAIL", msg),
                            Style::default().fg(theme.error),
                        )));
                    }
                }
            }

            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    "  Enter",
                    Style::default()
                        .fg(theme.suggestion)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" test & save  ", Style::default().fg(theme.inactive)),
                Span::styled(
                    "Esc",
                    Style::default()
                        .fg(theme.suggestion)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" back", Style::default().fg(theme.inactive)),
            ]));
        }
        ConnectPhase::Testing => {
            let spinner = SPINNER_FRAMES[state.spinner_frame % SPINNER_FRAMES.len()];
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(format!("  {} ", spinner), Style::default().fg(theme.claude)),
                Span::styled("Testing connection...", Style::default().fg(theme.text)),
            ]));
        }
        ConnectPhase::Done => {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Provider connected successfully!",
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Press any key to continue",
                Style::default().fg(theme.inactive),
            )));
        }
        ConnectPhase::CopilotDeviceFlow {
            ref user_code,
            ref verification_uri,
        } => {
            let pink = Color::Rgb(255, 150, 180);
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("        ", Style::default()),
                Span::styled("*", Style::default().fg(Color::Rgb(255, 255, 255))),
                Span::styled(
                    "  HAKARI  ",
                    Style::default().fg(pink).add_modifier(Modifier::BOLD),
                ),
                Span::styled("*", Style::default().fg(Color::Rgb(255, 255, 255))),
            ]));
            lines.push(Line::from(Span::styled(
                "     GitHub Copilot Auth",
                Style::default().fg(theme.inactive),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                " Enter this code at GitHub:",
                Style::default().fg(theme.text),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("       {}", user_code),
                Style::default().fg(pink).add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  Open: ", Style::default().fg(theme.inactive)),
                Span::styled(
                    verification_uri.clone(),
                    Style::default()
                        .fg(theme.suggestion)
                        .add_modifier(Modifier::UNDERLINED),
                ),
            ]));
            lines.push(Line::from(""));
            let spinner = SPINNER_FRAMES[state.spinner_frame % SPINNER_FRAMES.len()];
            lines.push(Line::from(vec![
                Span::styled(format!("  {} ", spinner), Style::default().fg(pink)),
                Span::styled(
                    "Waiting for authorization...",
                    Style::default().fg(theme.inactive),
                ),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    "  Esc",
                    Style::default()
                        .fg(theme.suggestion)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" cancel", Style::default().fg(theme.inactive)),
            ]));
        }
        ConnectPhase::CopilotPolling => {
            lines.push(Line::from(""));
            let spinner = SPINNER_FRAMES[state.spinner_frame % SPINNER_FRAMES.len()];
            lines.push(Line::from(vec![
                Span::styled(format!("  {} ", spinner), Style::default().fg(theme.claude)),
                Span::styled(
                    "Starting GitHub device flow...",
                    Style::default().fg(theme.text),
                ),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    "  Esc",
                    Style::default()
                        .fg(theme.suggestion)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" cancel", Style::default().fg(theme.inactive)),
            ]));
        }
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

fn render_session_picker(frame: &mut Frame, state: &mut AppState, area: Rect) {
    let theme = &state.theme;
    let sessions = &state.session_picker_sessions;

    let mut rows: Vec<DialogRow> = Vec::new();
    rows.push(DialogRow::text(Line::from(Span::styled(
        "Select a session to resume (Enter to select, Esc to cancel)",
        Style::default().fg(theme.inactive),
    ))));
    rows.push(DialogRow::blank());

    for (i, session) in sessions.iter().enumerate() {
        let is_selected = i == state.dialog.selected;
        let prefix = if is_selected { " > " } else { "   " };

        let preview = if session.preview.chars().count() > 50 {
            format!("{}...", safe_truncate_chars(&session.preview, 50))
        } else {
            session.preview.clone()
        };

        let ts = session.session_ts;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let ago = now.saturating_sub(ts);
        let time_str = if ago < 60 {
            "just now".into()
        } else if ago < 3600 {
            format!("{}m ago", ago / 60)
        } else if ago < 86400 {
            format!("{}h ago", ago / 3600)
        } else {
            format!("{}d ago", ago / 86400)
        };

        let style = if is_selected {
            Style::default()
                .fg(theme.suggestion)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text)
        };
        let time_style = Style::default().fg(theme.inactive);

        rows.push(DialogRow::item(vec![
            Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(preview, style),
            ]),
            Line::from(vec![
                Span::raw("   "),
                Span::styled(time_str, time_style),
                Span::styled(format!("  ({} msgs)", session.messages.len()), time_style),
            ]),
        ]));
    }

    let cfg = DialogConfig::new(" Resume Session ", theme.suggestion, theme.text).width(72);
    render_dialog(frame, area, &cfg, &rows, &mut state.dialog);
}

fn shorten_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        return path.to_string();
    }

    if let Ok(home) = std::env::var("HOME") {
        if let Some(stripped) = path.strip_prefix(&home) {
            let shortened = format!("~{}", stripped);
            if shortened.len() <= max_len {
                return shortened;
            }
        }
    }

    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() <= 3 {
        return path.to_string();
    }

    format!(".../{}", parts.last().unwrap_or(&""))
}

fn safe_truncate_chars(s: &str, max_chars: usize) -> &str {
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
