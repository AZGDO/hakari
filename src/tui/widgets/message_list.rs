use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap, Scrollbar, ScrollbarOrientation, ScrollbarState};
use crate::tui::theme::Theme;

#[derive(Debug, Clone)]
pub enum MessageType {
    User,
    Nano,
    Shizuka,
    ToolResult { name: String, success: bool },
    Warning,
    Error,
    System,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub msg_type: MessageType,
    pub content: String,
    pub timestamp: Option<String>,
}

pub struct MessageList {
    pub messages: Vec<ChatMessage>,
    pub scroll_offset: usize,
    pub auto_scroll: bool,
}

impl MessageList {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            scroll_offset: 0,
            auto_scroll: true,
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
                self.scroll_to_bottom();
            }
        }
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
        self.auto_scroll = false;
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.scroll_offset += amount;
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = usize::MAX;
        self.auto_scroll = true;
    }

    pub fn page_up(&mut self, page_size: usize) {
        self.scroll_up(page_size);
    }

    pub fn page_down(&mut self, page_size: usize) {
        self.scroll_down(page_size);
    }

    fn build_lines(&self) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();

        for msg in &self.messages {
            match &msg.msg_type {
                MessageType::ToolResult { name, success } => {
                    let icon = if *success { "✓" } else { "✗" };
                    let style = if *success { Theme::tool_success() } else { Theme::tool_error() };
                    lines.push(Line::from(Span::styled(
                        format!("  ┌─ {} {} ─", icon, name),
                        Theme::tool_header(),
                    )));
                    for content_line in msg.content.lines() {
                        lines.push(Line::from(vec![
                            Span::styled("  │ ".to_string(), Theme::tool_header()),
                            Span::styled(content_line.to_string(), style),
                        ]));
                    }
                    lines.push(Line::from(Span::styled(
                        "  └───────────────────────────────────".to_string(),
                        Theme::tool_header(),
                    )));
                    lines.push(Line::default());
                }
                other => {
                    let (prefix, style) = match other {
                        MessageType::User => (" [You] ".to_string(), Theme::user_message()),
                        MessageType::Nano => (" [Nano] ".to_string(), Theme::nano_message()),
                        MessageType::Shizuka => (" [Shizuka] ".to_string(), Theme::shizuka_message()),
                        MessageType::Warning => (" ⚠ ".to_string(), Theme::warning()),
                        MessageType::Error => (" ✗ ".to_string(), Theme::error()),
                        MessageType::System => ("   ".to_string(), Theme::label()),
                        _ => unreachable!(),
                    };

                    let content_lines: Vec<&str> = msg.content.lines().collect();
                    if content_lines.is_empty() {
                        lines.push(Line::from(Span::styled(prefix.clone(), style)));
                    } else {
                        lines.push(Line::from(vec![
                            Span::styled(prefix.clone(), style),
                            Span::styled(content_lines[0].to_string(), style),
                        ]));
                        let indent = " ".repeat(prefix.len());
                        for line in &content_lines[1..] {
                            lines.push(Line::from(vec![
                                Span::styled(indent.clone(), style),
                                Span::styled(line.to_string(), style),
                            ]));
                        }
                    }
                    lines.push(Line::default());
                }
            }
        }

        lines
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let all_lines = self.build_lines();
        let total_lines = all_lines.len();
        let visible_height = area.height as usize;

        let max_scroll = total_lines.saturating_sub(visible_height);
        if self.scroll_offset > max_scroll {
            self.scroll_offset = max_scroll;
        }
        if self.auto_scroll {
            self.scroll_offset = max_scroll;
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
                .style(Style::default().fg(Theme::border()));

            frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
        }
    }
}
