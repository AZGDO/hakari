use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, BorderType, Clear, Paragraph, Wrap, List, ListItem, ListState};
use crate::tui::theme::Theme;

pub enum PopupType {
    Confirmation { title: String, message: String },
    Help,
    Escalation { summary: String },
    ModelSelector { models: Vec<ModelEntry>, selected: usize, loading: bool },
    Settings { entries: Vec<SettingEntry>, selected: usize },
    ConnectFlow { state: ConnectState },
}

#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub id: String,
    pub name: String,
    pub reasoning: bool,
    pub context: usize,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct SettingEntry {
    pub key: String,
    pub label: String,
    pub value: String,
    pub editable: bool,
}

#[derive(Debug, Clone)]
pub enum ConnectState {
    Starting,
    WaitingForAuth { uri: String, code: String },
    Polling,
    Success,
    Error(String),
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
        Self { popup_type: PopupType::Help, visible: true }
    }

    pub fn escalation(summary: &str) -> Self {
        Self {
            popup_type: PopupType::Escalation { summary: summary.to_string() },
            visible: true,
        }
    }

    pub fn model_selector(models: Vec<ModelEntry>, current_model: &str) -> Self {
        let selected = models.iter().position(|m| m.id == current_model).unwrap_or(0);
        Self {
            popup_type: PopupType::ModelSelector { models, selected, loading: false },
            visible: true,
        }
    }

    pub fn model_selector_loading() -> Self {
        Self {
            popup_type: PopupType::ModelSelector { models: Vec::new(), selected: 0, loading: true },
            visible: true,
        }
    }

    pub fn settings(entries: Vec<SettingEntry>) -> Self {
        Self {
            popup_type: PopupType::Settings { entries, selected: 0 },
            visible: true,
        }
    }

    pub fn connect_flow() -> Self {
        Self {
            popup_type: PopupType::ConnectFlow { state: ConnectState::Starting },
            visible: true,
        }
    }

    pub fn select_up(&mut self) {
        match &mut self.popup_type {
            PopupType::ModelSelector { selected, .. } => {
                *selected = selected.saturating_sub(1);
            }
            PopupType::Settings { selected, .. } => {
                *selected = selected.saturating_sub(1);
            }
            _ => {}
        }
    }

    pub fn select_down(&mut self) {
        match &mut self.popup_type {
            PopupType::ModelSelector { selected, models, .. } => {
                *selected = (*selected + 1).min(models.len().saturating_sub(1));
            }
            PopupType::Settings { selected, entries, .. } => {
                *selected = (*selected + 1).min(entries.len().saturating_sub(1));
            }
            _ => {}
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.visible { return; }

        match &self.popup_type {
            PopupType::Confirmation { title, message } => {
                let popup_area = centered_rect(60, 30, area);
                frame.render_widget(Clear, popup_area);
                let block = Block::default()
                    .title(format!(" {} ", title))
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Theme::yellow()))
                    .style(Style::default().bg(Theme::surface()));

                let text = format!("{}\n\n  [Y] Yes   [N] No   [Esc] Cancel", message);
                let paragraph = Paragraph::new(text)
                    .block(block)
                    .style(Style::default().fg(Theme::text()))
                    .wrap(Wrap { trim: false });
                frame.render_widget(paragraph, popup_area);
            }

            PopupType::Help => {
                let popup_area = centered_rect(72, 70, area);
                frame.render_widget(Clear, popup_area);
                let block = Block::default()
                    .title(" Help — Key Bindings & Commands ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Theme::blue()))
                    .style(Style::default().bg(Theme::surface()));

                let help_text = vec![
                    Line::from(Span::styled("  Keys", Style::default().fg(Theme::mauve()).add_modifier(Modifier::BOLD))),
                    Line::default(),
                    help_row("Enter", "Submit message"),
                    help_row("Shift+Enter", "New line in input"),
                    help_row("Ctrl+C", "Quit"),
                    help_row("Cmd+C / Ctrl+Shift+C", "Copy text"),
                    help_row("Cmd+V / Ctrl+Shift+V", "Paste from clipboard"),
                    help_row("Up/Down", "Input history / navigate suggestions"),
                    help_row("Tab", "Accept suggestion"),
                    help_row("Scroll / PgUp/PgDn", "Scroll messages"),
                    help_row("Ctrl+W", "Delete word"),
                    help_row("Esc", "Dismiss popup / scroll to bottom"),
                    Line::default(),
                    Line::from(Span::styled("  Commands", Style::default().fg(Theme::mauve()).add_modifier(Modifier::BOLD))),
                    Line::default(),
                    help_row("/model", "Select AI model"),
                    help_row("/connect", "Connect GitHub Copilot"),
                    help_row("/settings", "Open settings"),
                    help_row("/clear", "Clear chat"),
                    help_row("/compact", "Collapse thinking traces"),
                    help_row("/pin @file", "Pin file to context"),
                    help_row("/status", "Session status"),
                    help_row("/undo", "Undo last change"),
                    help_row("/diff", "Show session changes"),
                    help_row("/cost", "Token usage & cost"),
                    help_row("@filename", "Mention file (with autocomplete)"),
                    Line::default(),
                    Line::from(Span::styled("  Press Esc to close", Theme::label())),
                ];

                let paragraph = Paragraph::new(help_text).block(block);
                frame.render_widget(paragraph, popup_area);
            }

            PopupType::Escalation { summary } => {
                let popup_area = centered_rect(70, 60, area);
                frame.render_widget(Clear, popup_area);
                let block = Block::default()
                    .title(" Agent Needs Help ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Theme::red()))
                    .style(Style::default().bg(Theme::surface()));

                let text = format!("{}\n\n  [Enter] Provide guidance   [Esc] Dismiss", summary);
                let paragraph = Paragraph::new(text)
                    .block(block)
                    .style(Style::default().fg(Theme::text()))
                    .wrap(Wrap { trim: false });
                frame.render_widget(paragraph, popup_area);
            }

            PopupType::ModelSelector { models, selected, loading } => {
                let popup_area = centered_rect(65, 70, area);
                frame.render_widget(Clear, popup_area);
                let block = Block::default()
                    .title(" Select Model ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Theme::mauve()))
                    .style(Style::default().bg(Theme::surface()));

                if *loading {
                    let paragraph = Paragraph::new("  Loading models...")
                        .block(block)
                        .style(Style::default().fg(Theme::text_dim()));
                    frame.render_widget(paragraph, popup_area);
                    return;
                }

                let inner = block.inner(popup_area);
                frame.render_widget(block, popup_area);

                let mut lines = Vec::new();
                lines.push(Line::from(Span::styled(
                    "  Use ↑↓ to navigate, Enter to select, Esc to cancel",
                    Theme::label(),
                )));
                lines.push(Line::default());

                for (i, model) in models.iter().enumerate() {
                    let is_sel = i == *selected;
                    let bg = if is_sel { Theme::surface_bright() } else { Theme::surface() };
                    let indicator = if model.active { " ● " } else { "   " };
                    let indicator_style = if model.active {
                        Style::default().fg(Theme::green()).bg(bg)
                    } else {
                        Style::default().fg(Theme::text_dim()).bg(bg)
                    };

                    let name_style = if is_sel {
                        Style::default().fg(Theme::text_bright()).bg(bg).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Theme::text()).bg(bg)
                    };

                    let ctx_str = if model.context > 0 {
                        format!("{}k ctx", model.context / 1000)
                    } else {
                        String::new()
                    };

                    let mut tags = Vec::new();
                    if model.reasoning { tags.push("reasoning"); }

                    let tag_str = if tags.is_empty() {
                        String::new()
                    } else {
                        format!("  [{}]", tags.join(", "))
                    };

                    lines.push(Line::from(vec![
                        Span::styled(indicator.to_string(), indicator_style),
                        Span::styled(format!("{:<36}", model.name), name_style),
                        Span::styled(format!("{:>8}", ctx_str), Style::default().fg(Theme::text_dim()).bg(bg)),
                        Span::styled(tag_str, Style::default().fg(Theme::cyan()).bg(bg)),
                    ]));
                }

                let paragraph = Paragraph::new(lines).scroll((0, 0));
                frame.render_widget(paragraph, inner);
            }

            PopupType::Settings { entries, selected } => {
                let popup_area = centered_rect(65, 60, area);
                frame.render_widget(Clear, popup_area);
                let block = Block::default()
                    .title(" Settings ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Theme::blue()))
                    .style(Style::default().bg(Theme::surface()));

                let inner = block.inner(popup_area);
                frame.render_widget(block, popup_area);

                let mut lines = Vec::new();
                lines.push(Line::from(Span::styled(
                    "  Use ↑↓ to navigate, Esc to close",
                    Theme::label(),
                )));
                lines.push(Line::default());

                for (i, entry) in entries.iter().enumerate() {
                    let is_sel = i == *selected;
                    let bg = if is_sel { Theme::surface_bright() } else { Theme::surface() };
                    let label_style = Style::default().fg(Theme::text()).bg(bg);
                    let value_style = Style::default().fg(Theme::green()).bg(bg);

                    lines.push(Line::from(vec![
                        Span::styled(
                            if is_sel { "  ▸ " } else { "    " },
                            Style::default().fg(Theme::mauve()).bg(bg),
                        ),
                        Span::styled(format!("{:<24}", entry.label), label_style),
                        Span::styled(entry.value.clone(), value_style),
                    ]));
                }

                let paragraph = Paragraph::new(lines);
                frame.render_widget(paragraph, inner);
            }

            PopupType::ConnectFlow { state } => {
                let popup_area = centered_rect(60, 40, area);
                frame.render_widget(Clear, popup_area);
                let block = Block::default()
                    .title(" Connect GitHub Copilot ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Theme::green()))
                    .style(Style::default().bg(Theme::surface()));

                let text = match state {
                    ConnectState::Starting => {
                        "  Initiating device flow...".to_string()
                    }
                    ConnectState::WaitingForAuth { uri, code } => {
                        format!(
                            "  1. Open: {}\n\n  2. Enter code:\n\n       {}\n\n  Waiting for authorization...\n\n  Press Esc to cancel",
                            uri,
                            code,
                        )
                    }
                    ConnectState::Polling => {
                        "  Checking authorization...".to_string()
                    }
                    ConnectState::Success => {
                        "  ✓ Authentication successful!\n\n  Token saved. Press Esc to close.".to_string()
                    }
                    ConnectState::Error(e) => {
                        format!("  ✗ Authentication failed: {}\n\n  Press Esc to close.", e)
                    }
                };

                let paragraph = Paragraph::new(text)
                    .block(block)
                    .style(Style::default().fg(Theme::text()))
                    .wrap(Wrap { trim: false });
                frame.render_widget(paragraph, popup_area);
            }
        }
    }
}

fn help_row<'a>(key: &'a str, desc: &'a str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {:<24}", key),
            Style::default().fg(Theme::yellow()).add_modifier(Modifier::BOLD),
        ),
        Span::styled(desc.to_string(), Style::default().fg(Theme::text())),
    ])
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
