use crate::tui::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};

#[derive(Debug, Clone)]
pub enum MessageType {
    User,
    Nano,
    Shizuka,
    Thinking,
    ToolStreaming {
        name: String,
    },
    ToolResult {
        name: String,
        success: bool,
        file_path: Option<String>,
        diff: Option<String>,
        exit_code: Option<i32>,
        duration_ms: Option<u64>,
    },
    Warning,
    Error,
    System,
    Welcome,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub msg_type: MessageType,
    pub content: String,
    pub timestamp: Option<String>,
    pub collapsed: bool,
}

pub struct MessageList {
    pub messages: Vec<ChatMessage>,
    pub scroll_offset: usize,
    pub auto_scroll: bool,
    pub target_scroll: usize,
    pub smooth_scroll_active: bool,
    pub animation_frame: u64,
    pub welcome_animation_start: Option<u64>,
    pub last_line_map: Vec<Option<usize>>,
}

impl MessageList {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            scroll_offset: 0,
            auto_scroll: true,
            target_scroll: 0,
            smooth_scroll_active: false,
            animation_frame: 0,
            welcome_animation_start: None,
            last_line_map: Vec::new(),
        }
    }

    pub fn add_message(&mut self, msg: ChatMessage) {
        if matches!(msg.msg_type, MessageType::Welcome) {
            self.welcome_animation_start = Some(self.animation_frame);
        }
        self.messages.push(msg);
        if self.auto_scroll {
            self.scroll_to_bottom();
        }
    }

    pub fn append_to_last(&mut self, text: &str) {
        if let Some(last) = self.messages.last_mut() {
            last.content.push_str(text);
            if self.auto_scroll {
                self.target_scroll = usize::MAX;
                self.smooth_scroll_active = true;
            }
        }
    }

    pub fn begin_tool_stream(&mut self, name: &str) {
        let should_create = !matches!(
            self.messages.last().map(|msg| &msg.msg_type),
            Some(MessageType::ToolStreaming { name: active }) if active == name
        );

        if should_create {
            self.add_message(ChatMessage {
                msg_type: MessageType::ToolStreaming {
                    name: name.to_string(),
                },
                content: String::new(),
                timestamp: None,
                collapsed: false,
            });
        }
    }

    pub fn append_to_tool_stream(&mut self, name: &str, text: &str) {
        if let Some(last) = self.messages.last_mut() {
            if matches!(&last.msg_type, MessageType::ToolStreaming { name: active } if active == name)
            {
                last.content.push_str(text);
                if self.auto_scroll {
                    self.target_scroll = usize::MAX;
                    self.smooth_scroll_active = true;
                }
                return;
            }
        }

        self.begin_tool_stream(name);
        self.append_to_tool_stream(name, text);
    }

    pub fn finish_tool_stream(&mut self, name: &str, replacement: ChatMessage) -> bool {
        if let Some(last) = self.messages.last_mut() {
            if matches!(&last.msg_type, MessageType::ToolStreaming { name: active } if active == name)
            {
                *last = replacement;
                if self.auto_scroll {
                    self.scroll_to_bottom();
                }
                return true;
            }
        }

        false
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.target_scroll = self.scroll_offset.saturating_sub(amount);
        self.smooth_scroll_active = true;
        self.auto_scroll = false;
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.target_scroll = self.scroll_offset.saturating_add(amount);
        self.smooth_scroll_active = true;
    }

    pub fn scroll_to_bottom(&mut self) {
        self.target_scroll = usize::MAX;
        self.smooth_scroll_active = true;
        self.auto_scroll = true;
    }

    pub fn page_up(&mut self, page_size: usize) {
        self.scroll_up(page_size);
    }

    pub fn page_down(&mut self, page_size: usize) {
        self.scroll_down(page_size);
    }

    pub fn collapse_all_traces(&mut self) {
        for msg in &mut self.messages {
            if matches!(msg.msg_type, MessageType::Thinking | MessageType::Shizuka) {
                msg.collapsed = true;
            }
        }
    }

    pub fn toggle_collapse(&mut self, index: usize) {
        if let Some(msg) = self.messages.get_mut(index) {
            msg.collapsed = !msg.collapsed;
        }
    }

    pub fn message_at(&self, y_in_view: usize) -> Option<usize> {
        self.last_line_map
            .get(self.scroll_offset.saturating_add(y_in_view))
            .copied()
            .flatten()
    }

    pub fn tick_animations(&mut self) {
        self.animation_frame = self.animation_frame.saturating_add(1);
    }

    fn auto_collapse_old_thinking(&mut self) {
        let len = self.messages.len();
        if len < 3 {
            return;
        }
        for i in 0..len.saturating_sub(2) {
            let msg = &mut self.messages[i];
            if matches!(msg.msg_type, MessageType::Thinking | MessageType::Shizuka)
                && !msg.collapsed
            {
                msg.collapsed = true;
            }
        }
    }

    fn build_lines(&self) -> (Vec<Line<'static>>, Vec<Option<usize>>) {
        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut line_map: Vec<Option<usize>> = Vec::new();

        for (message_index, msg) in self.messages.iter().enumerate() {
            match &msg.msg_type {
                MessageType::Welcome => {
                    let reveal_lines = self
                        .welcome_animation_start
                        .map(|start| {
                            ((self.animation_frame.saturating_sub(start) as usize) * 2).max(1)
                        })
                        .unwrap_or(usize::MAX);

                    for line in msg.content.lines().take(reveal_lines) {
                        push_line(
                            &mut lines,
                            &mut line_map,
                            message_index,
                            Line::from(Span::styled(
                                line.to_string(),
                                Style::default().fg(Theme::text_dim()),
                            )),
                        );
                    }
                    push_blank(&mut lines, &mut line_map, message_index);
                }
                MessageType::Thinking => {
                    if msg.collapsed {
                        let preview = msg
                            .content
                            .lines()
                            .next()
                            .unwrap_or("")
                            .chars()
                            .take(50)
                            .collect::<String>();
                        push_line(
                            &mut lines,
                            &mut line_map,
                            message_index,
                            Line::from(vec![
                                Span::styled(
                                    "  \u{25b8} ",
                                    Style::default().fg(Theme::text_muted()),
                                ),
                                Span::styled(
                                    "thinking",
                                    Style::default()
                                        .fg(Theme::text_muted())
                                        .add_modifier(Modifier::ITALIC),
                                ),
                                Span::styled(
                                    format!("  {}...", preview),
                                    Style::default().fg(Theme::text_muted()),
                                ),
                            ]),
                        );
                    } else {
                        push_line(
                            &mut lines,
                            &mut line_map,
                            message_index,
                            Line::from(vec![
                                Span::styled(
                                    "  \u{25be} ",
                                    Style::default().fg(Theme::text_muted()),
                                ),
                                Span::styled(
                                    "thinking",
                                    Style::default()
                                        .fg(Theme::text_muted())
                                        .add_modifier(Modifier::ITALIC),
                                ),
                            ]),
                        );
                        for line in msg.content.lines() {
                            push_line(
                                &mut lines,
                                &mut line_map,
                                message_index,
                                Line::from(vec![
                                    Span::styled("    ", Style::default()),
                                    Span::styled(
                                        line.to_string(),
                                        Style::default()
                                            .fg(Theme::text_muted())
                                            .add_modifier(Modifier::ITALIC),
                                    ),
                                ]),
                            );
                        }
                    }
                    push_blank(&mut lines, &mut line_map, message_index);
                }
                MessageType::Shizuka => {
                    if msg.collapsed {
                        let preview = msg
                            .content
                            .lines()
                            .next()
                            .unwrap_or("")
                            .chars()
                            .take(50)
                            .collect::<String>();
                        push_line(
                            &mut lines,
                            &mut line_map,
                            message_index,
                            Line::from(vec![
                                Span::styled("  \u{25b8} ", Style::default().fg(Theme::cyan())),
                                Span::styled(
                                    "shizuka",
                                    Style::default()
                                        .fg(Theme::cyan())
                                        .add_modifier(Modifier::DIM),
                                ),
                                Span::styled(
                                    format!("  {}", preview),
                                    Style::default().fg(Theme::text_muted()),
                                ),
                            ]),
                        );
                    } else {
                        push_line(
                            &mut lines,
                            &mut line_map,
                            message_index,
                            Line::from(vec![
                                Span::styled("  \u{25be} ", Style::default().fg(Theme::cyan())),
                                Span::styled("shizuka ", Theme::shizuka_message()),
                            ]),
                        );
                        for line in msg.content.lines() {
                            push_line(
                                &mut lines,
                                &mut line_map,
                                message_index,
                                Line::from(vec![
                                    Span::styled("    ", Style::default()),
                                    Span::styled(line.to_string(), Theme::shizuka_message()),
                                ]),
                            );
                        }
                    }
                    push_blank(&mut lines, &mut line_map, message_index);
                }
                MessageType::ToolStreaming { name } => {
                    push_line(
                        &mut lines,
                        &mut line_map,
                        message_index,
                        Line::from(vec![
                            Span::styled("  ⏵ ", Style::default().fg(Theme::yellow())),
                            Span::styled(
                                format!("{} (live)", name),
                                Style::default()
                                    .fg(Theme::yellow())
                                    .add_modifier(Modifier::BOLD),
                            ),
                        ]),
                    );

                    for line in msg.content.lines().take(48) {
                        push_line(
                            &mut lines,
                            &mut line_map,
                            message_index,
                            Line::from(vec![
                                Span::styled("    ", Style::default()),
                                Span::styled(line.to_string(), Style::default().fg(Theme::text())),
                            ]),
                        );
                    }

                    let total = msg.content.lines().count();
                    if total > 48 {
                        push_line(
                            &mut lines,
                            &mut line_map,
                            message_index,
                            Line::from(Span::styled(
                                format!("    ... {} more live lines", total - 48),
                                Style::default().fg(Theme::text_muted()),
                            )),
                        );
                    }

                    push_blank(&mut lines, &mut line_map, message_index);
                }
                MessageType::ToolResult {
                    name,
                    success,
                    file_path,
                    diff,
                    exit_code,
                    duration_ms,
                } => {
                    let icon = if *success { "\u{2713}" } else { "\u{2717}" };
                    let icon_color = if *success {
                        Theme::green()
                    } else {
                        Theme::red()
                    };
                    let meta = build_tool_meta(file_path.as_deref(), *exit_code, *duration_ms);

                    let preview_source = diff.as_ref().unwrap_or(&msg.content);
                    let preview_line = preview_source
                        .lines()
                        .find(|line| !line.trim().is_empty())
                        .unwrap_or("");

                    if msg.collapsed {
                        push_line(
                            &mut lines,
                            &mut line_map,
                            message_index,
                            Line::from(vec![
                                Span::styled(
                                    "  \u{25b8} ",
                                    Style::default().fg(Theme::text_muted()),
                                ),
                                Span::styled(format!("{} ", icon), Style::default().fg(icon_color)),
                                Span::styled(
                                    format!("{} ", name),
                                    Style::default().fg(Theme::text_dim()),
                                ),
                                Span::styled(meta, Style::default().fg(Theme::text_muted())),
                                Span::styled(
                                    format!(
                                        "  {}",
                                        preview_line.chars().take(56).collect::<String>()
                                    ),
                                    Style::default().fg(Theme::text_muted()),
                                ),
                            ]),
                        );
                    } else {
                        push_line(
                            &mut lines,
                            &mut line_map,
                            message_index,
                            Line::from(vec![
                                Span::styled("  ", Style::default()),
                                Span::styled(format!("{} ", icon), Style::default().fg(icon_color)),
                                Span::styled(
                                    name.to_string(),
                                    Style::default()
                                        .fg(Theme::text_dim())
                                        .add_modifier(Modifier::BOLD),
                                ),
                                Span::styled(meta, Style::default().fg(Theme::text_muted())),
                            ]),
                        );

                        if let Some(diff) = diff {
                            for diff_line in diff.lines().take(32) {
                                push_line(
                                    &mut lines,
                                    &mut line_map,
                                    message_index,
                                    Line::from(vec![
                                        Span::styled("    ", Style::default()),
                                        Span::styled(diff_line.to_string(), diff_style(diff_line)),
                                    ]),
                                );
                            }
                            let total = diff.lines().count();
                            if total > 32 {
                                push_line(
                                    &mut lines,
                                    &mut line_map,
                                    message_index,
                                    Line::from(Span::styled(
                                        format!("    ... {} more diff lines", total - 32),
                                        Style::default().fg(Theme::text_muted()),
                                    )),
                                );
                            }
                        } else {
                            for content_line in msg.content.lines().take(20) {
                                push_line(
                                    &mut lines,
                                    &mut line_map,
                                    message_index,
                                    Line::from(vec![
                                        Span::styled("    ", Style::default()),
                                        Span::styled(
                                            content_line.to_string(),
                                            Style::default().fg(Theme::text_muted()),
                                        ),
                                    ]),
                                );
                            }
                            let total = msg.content.lines().count();
                            if total > 20 {
                                push_line(
                                    &mut lines,
                                    &mut line_map,
                                    message_index,
                                    Line::from(Span::styled(
                                        format!("    ... {} more lines", total - 20),
                                        Style::default().fg(Theme::text_muted()),
                                    )),
                                );
                            }
                        }
                    }
                    push_blank(&mut lines, &mut line_map, message_index);
                }
                MessageType::User => {
                    push_line(
                        &mut lines,
                        &mut line_map,
                        message_index,
                        Line::from(vec![
                            Span::styled("  > ", Style::default().fg(Theme::mauve())),
                            Span::styled(
                                msg.content.lines().next().unwrap_or("").to_string(),
                                Theme::user_message(),
                            ),
                        ]),
                    );
                    for line in msg.content.lines().skip(1) {
                        push_line(
                            &mut lines,
                            &mut line_map,
                            message_index,
                            Line::from(vec![
                                Span::styled("    ", Style::default()),
                                Span::styled(line.to_string(), Theme::user_message()),
                            ]),
                        );
                    }
                    push_blank(&mut lines, &mut line_map, message_index);
                }
                MessageType::Nano => {
                    for line in msg.content.lines() {
                        push_line(
                            &mut lines,
                            &mut line_map,
                            message_index,
                            Line::from(vec![
                                Span::styled("  ", Style::default()),
                                Span::styled(line.to_string(), Theme::nano_message()),
                            ]),
                        );
                    }
                    push_blank(&mut lines, &mut line_map, message_index);
                }
                MessageType::Warning => {
                    push_line(
                        &mut lines,
                        &mut line_map,
                        message_index,
                        Line::from(vec![
                            Span::styled("  ! ", Style::default().fg(Theme::yellow())),
                            Span::styled(msg.content.clone(), Theme::warning()),
                        ]),
                    );
                    push_blank(&mut lines, &mut line_map, message_index);
                }
                MessageType::Error => {
                    push_line(
                        &mut lines,
                        &mut line_map,
                        message_index,
                        Line::from(vec![
                            Span::styled("  \u{2717} ", Style::default().fg(Theme::red())),
                            Span::styled(msg.content.clone(), Theme::error()),
                        ]),
                    );
                    push_blank(&mut lines, &mut line_map, message_index);
                }
                MessageType::System => {
                    for line in msg.content.lines() {
                        push_line(
                            &mut lines,
                            &mut line_map,
                            message_index,
                            Line::from(vec![
                                Span::styled("  ", Style::default()),
                                Span::styled(
                                    line.to_string(),
                                    Style::default().fg(Theme::text_dim()),
                                ),
                            ]),
                        );
                    }
                    push_blank(&mut lines, &mut line_map, message_index);
                }
            }
        }

        (lines, line_map)
    }

    pub fn tick_smooth_scroll(&mut self) {
        if !self.smooth_scroll_active {
            return;
        }
        let diff = if self.target_scroll > self.scroll_offset {
            let d = self.target_scroll.saturating_sub(self.scroll_offset);
            (d / 3).max(1).min(d)
        } else if self.target_scroll < self.scroll_offset {
            let d = self.scroll_offset.saturating_sub(self.target_scroll);
            (d / 3).max(1).min(d)
        } else {
            self.smooth_scroll_active = false;
            return;
        };

        if self.target_scroll > self.scroll_offset {
            self.scroll_offset += diff;
        } else {
            self.scroll_offset -= diff;
        }

        if self.scroll_offset == self.target_scroll {
            self.smooth_scroll_active = false;
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        self.auto_collapse_old_thinking();
        self.tick_smooth_scroll();

        let (all_lines, line_map) = self.build_lines();
        self.last_line_map = line_map;
        let total_lines = all_lines.len();
        let visible_height = area.height as usize;

        let max_scroll = total_lines.saturating_sub(visible_height);
        if self.scroll_offset > max_scroll {
            self.scroll_offset = max_scroll;
        }
        if self.target_scroll > max_scroll {
            self.target_scroll = max_scroll;
        }
        if self.auto_scroll {
            self.scroll_offset = max_scroll;
            self.target_scroll = max_scroll;
        }

        let paragraph = Paragraph::new(all_lines)
            .scroll((self.scroll_offset as u16, 0))
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);

        if total_lines > visible_height {
            let mut scrollbar_state = ScrollbarState::new(total_lines)
                .position(self.scroll_offset)
                .viewport_content_length(visible_height);

            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(Theme::border_focus()));

            frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
        }
    }
}

fn build_tool_meta(
    file_path: Option<&str>,
    exit_code: Option<i32>,
    duration_ms: Option<u64>,
) -> String {
    let mut parts = Vec::new();

    if let Some(file_path) = file_path {
        parts.push(format!(" · {}", file_path));
    }
    if let Some(exit_code) = exit_code {
        parts.push(format!(" · exit {}", exit_code));
    }
    if let Some(duration_ms) = duration_ms {
        if duration_ms >= 1000 {
            parts.push(format!(" · {:.1}s", duration_ms as f64 / 1000.0));
        } else {
            parts.push(format!(" · {}ms", duration_ms));
        }
    }

    parts.join("")
}

fn diff_style(line: &str) -> Style {
    if line.starts_with("+++") || line.starts_with("---") {
        Style::default()
            .fg(Theme::lavender())
            .add_modifier(Modifier::BOLD)
    } else if line.starts_with("@@") {
        Style::default().fg(Theme::mauve())
    } else if line.starts_with('+') {
        Style::default().fg(Theme::green())
    } else if line.starts_with('-') {
        Style::default().fg(Theme::red())
    } else {
        Style::default().fg(Theme::text_muted())
    }
}

fn push_line(
    lines: &mut Vec<Line<'static>>,
    line_map: &mut Vec<Option<usize>>,
    message_index: usize,
    line: Line<'static>,
) {
    lines.push(line);
    line_map.push(Some(message_index));
}

fn push_blank(
    lines: &mut Vec<Line<'static>>,
    line_map: &mut Vec<Option<usize>>,
    message_index: usize,
) {
    lines.push(Line::default());
    line_map.push(Some(message_index));
}
