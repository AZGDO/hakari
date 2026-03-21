use crate::memory::kms::TaskClassification;
use crate::tui::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

pub struct StatusBarData {
    pub classification: Option<TaskClassification>,
    pub step: usize,
    pub max_steps: usize,
    pub context_tokens: usize,
    pub status: AgentStatus,
    pub modified_files: usize,
    pub pinned_files: usize,
    pub activity: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus {
    Ready,
    Preparing,
    Thinking,
    ToolRunning(String),
    Complete,
    Error,
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ready => write!(f, "ready"),
            Self::Preparing => write!(f, "preparing"),
            Self::Thinking => write!(f, "thinking"),
            Self::ToolRunning(name) => write!(f, "{}", name),
            Self::Complete => write!(f, "complete"),
            Self::Error => write!(f, "error"),
        }
    }
}

pub fn render_status_bar(frame: &mut Frame, area: Rect, data: &StatusBarData) {
    let bg = Theme::surface();
    let mut spans = Vec::new();

    // Status indicator
    let (icon, status_color) = match &data.status {
        AgentStatus::Ready => ("\u{25cf}", Theme::green()),
        AgentStatus::Preparing => ("\u{25cb}", Theme::mauve()),
        AgentStatus::Thinking => ("\u{25cb}", Theme::mauve()),
        AgentStatus::ToolRunning(_) => ("\u{25cb}", Theme::yellow()),
        AgentStatus::Complete => ("\u{25cf}", Theme::green()),
        AgentStatus::Error => ("\u{25cf}", Theme::red()),
    };

    spans.push(Span::styled(
        format!(" {} {} ", icon, data.status),
        Style::default().fg(status_color).bg(bg),
    ));

    // Classification badge
    if let Some(ref class) = data.classification {
        let (text, badge_color) = match class {
            TaskClassification::Trivial => ("trivial", Theme::green()),
            TaskClassification::Small => ("small", Theme::blue()),
            TaskClassification::Medium => ("medium", Theme::yellow()),
            TaskClassification::Large => ("large", Theme::red()),
        };
        spans.push(Span::styled(
            format!(" {} ", text),
            Style::default().fg(badge_color).bg(bg),
        ));
    }

    // Step counter
    if data.max_steps > 0 {
        spans.push(Span::styled(
            format!(" {}/{}", data.step, data.max_steps),
            Style::default().fg(Theme::text_muted()).bg(bg),
        ));
    }

    if let Some(activity) = &data.activity {
        spans.push(Span::styled(
            format!("  {}", activity),
            Style::default().fg(Theme::mauve()).bg(bg),
        ));
    }

    // Right side: context tokens + hints
    let mut right = Vec::new();

    let ctx_str = if data.context_tokens > 1000 {
        format!("{:.1}k tokens", data.context_tokens as f64 / 1000.0)
    } else {
        format!("{} tokens", data.context_tokens)
    };
    right.push(Span::styled(
        format!("{} ", ctx_str),
        Style::default().fg(Theme::text_muted()).bg(bg),
    ));
    right.push(Span::styled(
        format!("{} modified  ", data.modified_files),
        Style::default().fg(Theme::peach()).bg(bg),
    ));
    right.push(Span::styled(
        format!("{} pinned  ", data.pinned_files),
        Style::default().fg(Theme::blue()).bg(bg),
    ));
    right.push(Span::styled(
        " /help ",
        Style::default().fg(Theme::text_muted()).bg(bg),
    ));
    right.push(Span::styled(
        "ctrl+c quit ",
        Style::default().fg(Theme::text_muted()).bg(bg),
    ));

    let left_width: usize = spans.iter().map(|s| s.width()).sum();
    let right_width: usize = right.iter().map(|s| s.width()).sum();
    let padding = (area.width as usize).saturating_sub(left_width + right_width);
    spans.push(Span::styled(" ".repeat(padding), Style::default().bg(bg)));
    spans.extend(right);

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line).style(Style::default().bg(bg));
    frame.render_widget(paragraph, area);
}
