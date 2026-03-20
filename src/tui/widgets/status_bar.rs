use ratatui::prelude::*;
use ratatui::widgets::Paragraph;
use crate::tui::theme::Theme;
use crate::memory::kms::TaskClassification;

pub struct StatusBarData {
    pub classification: Option<TaskClassification>,
    pub step: usize,
    pub max_steps: usize,
    pub context_tokens: usize,
    pub status: AgentStatus,
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
            Self::Ready => write!(f, "✓ ready"),
            Self::Preparing => write!(f, "◐ preparing"),
            Self::Thinking => write!(f, "◑ thinking"),
            Self::ToolRunning(name) => write!(f, "◒ {}", name),
            Self::Complete => write!(f, "✓ complete"),
            Self::Error => write!(f, "✗ error"),
        }
    }
}

pub fn render_status_bar(frame: &mut Frame, area: Rect, data: &StatusBarData) {
    let mut spans = Vec::new();

    // Classification badge
    if let Some(ref class) = data.classification {
        let (text, style) = match class {
            TaskClassification::Trivial => ("trivial", Theme::badge_trivial()),
            TaskClassification::Small => ("small", Theme::badge_small()),
            TaskClassification::Medium => ("medium", Theme::badge_medium()),
            TaskClassification::Large => ("large", Theme::badge_large()),
        };
        spans.push(Span::styled(format!(" {} ", text), style));
        spans.push(Span::styled(" │ ", Theme::label()));
    }

    // Step counter
    if data.max_steps > 0 {
        spans.push(Span::styled(
            format!("step {}/{}", data.step, data.max_steps),
            Theme::label(),
        ));
        spans.push(Span::styled(" │ ", Theme::label()));
    }

    // Context tokens
    let ctx_str = if data.context_tokens > 1000 {
        format!("{:.1}k", data.context_tokens as f64 / 1000.0)
    } else {
        format!("{}", data.context_tokens)
    };
    spans.push(Span::styled(format!("ctx: {}", ctx_str), Theme::label()));
    spans.push(Span::styled(" │ ", Theme::label()));

    // Status
    let status_style = match &data.status {
        AgentStatus::Ready | AgentStatus::Complete => Theme::tool_success(),
        AgentStatus::Error => Theme::error(),
        _ => Theme::spinner(),
    };
    spans.push(Span::styled(data.status.to_string(), status_style));

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line).style(Theme::status_bar());
    frame.render_widget(paragraph, area);
}
