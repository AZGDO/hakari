use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, BorderType, Clear, Paragraph, Wrap};
use crate::tui::theme::Theme;

pub enum PopupType {
    Confirmation {
        title: String,
        message: String,
    },
    Help,
    Escalation {
        summary: String,
    },
}

pub struct Popup {
    pub popup_type: PopupType,
    pub visible: bool,
}

impl Popup {
    pub fn confirmation(title: &str, message: &str) -> Self {
        Self {
            popup_type: PopupType::Confirmation {
                title: title.to_string(),
                message: message.to_string(),
            },
            visible: true,
        }
    }

    pub fn help() -> Self {
        Self {
            popup_type: PopupType::Help,
            visible: true,
        }
    }

    pub fn escalation(summary: &str) -> Self {
        Self {
            popup_type: PopupType::Escalation {
                summary: summary.to_string(),
            },
            visible: true,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        // Center popup
        let popup_area = centered_rect(70, 60, area);

        // Clear background
        frame.render_widget(Clear, popup_area);

        match &self.popup_type {
            PopupType::Confirmation { title, message } => {
                let block = Block::default()
                    .title(format!(" {} ", title))
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Theme::yellow()))
                    .style(Style::default().bg(Theme::surface()));

                let text = format!(
                    "{}\n\n  [Y] Yes   [N] No   [Esc] Cancel",
                    message
                );

                let paragraph = Paragraph::new(text)
                    .block(block)
                    .style(Style::default().fg(Theme::text()))
                    .wrap(Wrap { trim: false });

                frame.render_widget(paragraph, popup_area);
            }
            PopupType::Help => {
                let block = Block::default()
                    .title(" Key Bindings ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Theme::blue()))
                    .style(Style::default().bg(Theme::surface()));

                let help_text = vec![
                    Line::from(vec![
                        Span::styled("  Enter       ", Style::default().fg(Theme::yellow()).add_modifier(Modifier::BOLD)),
                        Span::styled("Submit message", Style::default().fg(Theme::text())),
                    ]),
                    Line::from(vec![
                        Span::styled("  Shift+Enter ", Style::default().fg(Theme::yellow()).add_modifier(Modifier::BOLD)),
                        Span::styled("New line in input", Style::default().fg(Theme::text())),
                    ]),
                    Line::from(vec![
                        Span::styled("  Ctrl+C      ", Style::default().fg(Theme::yellow()).add_modifier(Modifier::BOLD)),
                        Span::styled("Quit", Style::default().fg(Theme::text())),
                    ]),
                    Line::from(vec![
                        Span::styled("  Cmd+C       ", Style::default().fg(Theme::yellow()).add_modifier(Modifier::BOLD)),
                        Span::styled("Copy selected text", Style::default().fg(Theme::text())),
                    ]),
                    Line::from(vec![
                        Span::styled("  Cmd+V       ", Style::default().fg(Theme::yellow()).add_modifier(Modifier::BOLD)),
                        Span::styled("Paste from clipboard", Style::default().fg(Theme::text())),
                    ]),
                    Line::from(vec![
                        Span::styled("  Up/Down     ", Style::default().fg(Theme::yellow()).add_modifier(Modifier::BOLD)),
                        Span::styled("Input history", Style::default().fg(Theme::text())),
                    ]),
                    Line::from(vec![
                        Span::styled("  Scroll/PgUp ", Style::default().fg(Theme::yellow()).add_modifier(Modifier::BOLD)),
                        Span::styled("Scroll messages", Style::default().fg(Theme::text())),
                    ]),
                    Line::from(vec![
                        Span::styled("  Ctrl+W      ", Style::default().fg(Theme::yellow()).add_modifier(Modifier::BOLD)),
                        Span::styled("Delete word", Style::default().fg(Theme::text())),
                    ]),
                    Line::from(vec![
                        Span::styled("  ?           ", Style::default().fg(Theme::yellow()).add_modifier(Modifier::BOLD)),
                        Span::styled("Toggle this help", Style::default().fg(Theme::text())),
                    ]),
                    Line::default(),
                    Line::from(Span::styled(
                        "  Press Esc or ? to close",
                        Theme::label(),
                    )),
                ];

                let paragraph = Paragraph::new(help_text).block(block);
                frame.render_widget(paragraph, popup_area);
            }
            PopupType::Escalation { summary } => {
                let block = Block::default()
                    .title(" Agent Needs Help ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Theme::red()))
                    .style(Style::default().bg(Theme::surface()));

                let text = format!(
                    "{}\n\n  [Enter] Provide guidance   [Esc] Dismiss",
                    summary
                );

                let paragraph = Paragraph::new(text)
                    .block(block)
                    .style(Style::default().fg(Theme::text()))
                    .wrap(Wrap { trim: false });

                frame.render_widget(paragraph, popup_area);
            }
        }
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
