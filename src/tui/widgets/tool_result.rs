use crate::tui::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

pub fn render_tool_result(name: &str, content: &str, success: bool) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let icon = if success { "✓" } else { "✗" };
    let header_style = Theme::tool_header();
    let content_style = if success {
        Theme::tool_success()
    } else {
        Theme::tool_error()
    };

    // Top border
    let header_text = format!("─ {} {} ", icon, name);
    let padding = 40usize.saturating_sub(header_text.len() + 4);
    lines.push(Line::from(Span::styled(
        format!("  ┌{}{}┐", header_text, "─".repeat(padding)),
        header_style,
    )));

    // Content lines
    for line in content.lines() {
        lines.push(Line::from(vec![
            Span::styled("  │ ".to_string(), header_style),
            Span::styled(line.to_string(), content_style),
        ]));
    }

    // Bottom border
    lines.push(Line::from(Span::styled(
        format!("  └{}┘", "─".repeat(header_text.len() + padding)),
        header_style,
    )));

    lines
}
