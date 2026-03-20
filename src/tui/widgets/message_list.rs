use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Wrap, Scrollbar, ScrollbarOrientation, ScrollbarState};
use crate::tui::theme::Theme;

#[derive(Debug, Clone)]
pub enum MessageType {
    User,
    Nano,
    Shizuka,
    Thinking,
    ToolResult { name: String, success: bool },
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
}

impl MessageList {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            scroll_offset: 0,
            auto_scroll: true,
            target_scroll: 0,
            smooth_scroll_active: false,
        }
    }

    pub fn add_message(&mut self, msg: ChatMessage) {
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

    fn auto_collapse_old_thinking(&mut self) {
        let len = self.messages.len();
        if len < 3 { return; }
        for i in 0..len.saturating_sub(2) {
            let msg = &mut self.messages[i];
            if matches!(msg.msg_type, MessageType::Thinking | MessageType::Shizuka) && !msg.collapsed {
                msg.collapsed = true;
            }
        }
    }

    fn build_lines(&self) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();

        for msg in &self.messages {
            match &msg.msg_type {
                MessageType::Welcome => {
                    for line in msg.content.lines() {
                        lines.push(Line::from(Span::styled(
                            line.to_string(),
                            Style::default().fg(Theme::text_dim()),
                        )));
                    }
                    lines.push(Line::default());
                }
                MessageType::Thinking => {
                    if msg.collapsed {
                        let preview = msg.content.lines().next().unwrap_or("").chars().take(50).collect::<String>();
                        lines.push(Line::from(vec![
                            Span::styled("  \u{25b8} ", Style::default().fg(Theme::text_muted())),
                            Span::styled("thinking", Style::default().fg(Theme::text_muted()).add_modifier(Modifier::ITALIC)),
                            Span::styled(format!("  {}...", preview), Style::default().fg(Theme::text_muted())),
                        ]));
                    } else {
                        lines.push(Line::from(vec![
                            Span::styled("  \u{25be} ", Style::default().fg(Theme::text_muted())),
                            Span::styled("thinking", Style::default().fg(Theme::text_muted()).add_modifier(Modifier::ITALIC)),
                        ]));
                        for line in msg.content.lines() {
                            lines.push(Line::from(vec![
                                Span::styled("    ", Style::default()),
                                Span::styled(line.to_string(), Style::default().fg(Theme::text_muted()).add_modifier(Modifier::ITALIC)),
                            ]));
                        }
                    }
                    lines.push(Line::default());
                }
                MessageType::Shizuka => {
                    if msg.collapsed {
                        let preview = msg.content.lines().next().unwrap_or("").chars().take(50).collect::<String>();
                        lines.push(Line::from(vec![
                            Span::styled("  \u{25b8} ", Style::default().fg(Theme::cyan())),
                            Span::styled("shizuka", Style::default().fg(Theme::cyan()).add_modifier(Modifier::DIM)),
                            Span::styled(format!("  {}", preview), Style::default().fg(Theme::text_muted())),
                        ]));
                    } else {
                        lines.push(Line::from(vec![
                            Span::styled("  \u{25be} ", Style::default().fg(Theme::cyan())),
                            Span::styled("shizuka ", Theme::shizuka_message()),
                        ]));
                        for line in msg.content.lines() {
                            lines.push(Line::from(vec![
                                Span::styled("    ", Style::default()),
                                Span::styled(line.to_string(), Theme::shizuka_message()),
                            ]));
                        }
                    }
                    lines.push(Line::default());
                }
                MessageType::ToolResult { name, success } => {
                    let icon = if *success { "\u{2713}" } else { "\u{2717}" };
                    let icon_color = if *success { Theme::green() } else { Theme::red() };

                    if msg.collapsed {
                        lines.push(Line::from(vec![
                            Span::styled("  \u{25b8} ", Style::default().fg(Theme::text_muted())),
                            Span::styled(format!("{} ", icon), Style::default().fg(icon_color)),
                            Span::styled(format!("{} ", name), Style::default().fg(Theme::text_dim())),
                            Span::styled(
                                msg.content.lines().next().unwrap_or("").chars().take(60).collect::<String>(),
                                Style::default().fg(Theme::text_muted()),
                            ),
                        ]));
                    } else {
                        lines.push(Line::from(vec![
                            Span::styled("  ", Style::default()),
                            Span::styled(format!("{} ", icon), Style::default().fg(icon_color)),
                            Span::styled(
                                format!("{}", name),
                                Style::default().fg(Theme::text_dim()),
                            ),
                        ]));
                        for content_line in msg.content.lines().take(20) {
                            lines.push(Line::from(vec![
                                Span::styled("    ", Style::default()),
                                Span::styled(
                                    content_line.to_string(),
                                    Style::default().fg(Theme::text_muted()),
                                ),
                            ]));
                        }
                        let total = msg.content.lines().count();
                        if total > 20 {
                            lines.push(Line::from(Span::styled(
                                format!("    ... {} more lines", total - 20),
                                Style::default().fg(Theme::text_muted()),
                            )));
                        }
                    }
                    lines.push(Line::default());
                }
                MessageType::User => {
                    lines.push(Line::from(vec![
                        Span::styled("  > ", Style::default().fg(Theme::mauve())),
                        Span::styled(
                            msg.content.lines().next().unwrap_or("").to_string(),
                            Theme::user_message(),
                        ),
                    ]));
                    for line in msg.content.lines().skip(1) {
                        lines.push(Line::from(vec![
                            Span::styled("    ", Style::default()),
                            Span::styled(line.to_string(), Theme::user_message()),
                        ]));
                    }
                    lines.push(Line::default());
                }
                MessageType::Nano => {
                    for line in msg.content.lines() {
                        lines.push(Line::from(vec![
                            Span::styled("  ", Style::default()),
                            Span::styled(line.to_string(), Theme::nano_message()),
                        ]));
                    }
                    lines.push(Line::default());
                }
                MessageType::Warning => {
                    lines.push(Line::from(vec![
                        Span::styled("  ! ", Style::default().fg(Theme::yellow())),
                        Span::styled(msg.content.clone(), Theme::warning()),
                    ]));
                    lines.push(Line::default());
                }
                MessageType::Error => {
                    lines.push(Line::from(vec![
                        Span::styled("  \u{2717} ", Style::default().fg(Theme::red())),
                        Span::styled(msg.content.clone(), Theme::error()),
                    ]));
                    lines.push(Line::default());
                }
                MessageType::System => {
                    for line in msg.content.lines() {
                        lines.push(Line::from(vec![
                            Span::styled("  ", Style::default()),
                            Span::styled(line.to_string(), Style::default().fg(Theme::text_dim())),
                        ]));
                    }
                    lines.push(Line::default());
                }
            }
        }

        lines
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

        let all_lines = self.build_lines();
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

        // Scrollbar
        if total_lines > visible_height {
            let mut scrollbar_state = ScrollbarState::new(total_lines)
                .position(self.scroll_offset)
                .viewport_content_length(visible_height);

            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(Theme::border()));

            frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
        }
    }
}
