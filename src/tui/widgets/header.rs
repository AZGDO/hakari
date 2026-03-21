use crate::auth::copilot::CopilotUsage;
use crate::tui::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

pub struct HeaderData {
    pub project_name: String,
    pub session_id: String,
    pub has_kpms: bool,
    pub has_kkm: bool,
    pub model_name: String,
    pub model_category: String,
    pub reasoning: String,
    pub shizuka_model: String,
    pub auth_status: AuthDisplay,
    pub copilot_usage: Option<CopilotUsage>,
    pub animation_frame: u64,
}

#[derive(Debug, Clone)]
pub enum AuthDisplay {
    None,
    Connected(String),
    NotConnected,
}

pub fn render_header(frame: &mut Frame, area: Rect, data: &HeaderData) {
    let bg = Theme::surface();

    // Line 1: branding + project + auth
    let mut line1_spans = Vec::new();

    line1_spans.push(Span::styled(
        " HAKARI ",
        Style::default()
            .fg(Theme::mauve())
            .bg(bg)
            .add_modifier(Modifier::BOLD | Modifier::ITALIC),
    ));

    line1_spans.push(Span::styled(
        format!(" {} ", data.project_name),
        Style::default().fg(Theme::text()).bg(bg),
    ));

    line1_spans.push(Span::styled(
        format!("#{} ", &data.session_id[..data.session_id.len().min(8)]),
        Style::default().fg(Theme::text_muted()).bg(bg),
    ));

    // Right side of line 1
    let mut right1 = Vec::new();

    if let Some(ref usage) = data.copilot_usage {
        if usage.limit > 0 {
            let color = if usage.percent_left > 50.0 {
                Theme::green()
            } else if usage.percent_left > 20.0 {
                Theme::yellow()
            } else {
                Theme::red()
            };
            let bar_width: usize = 8;
            let filled = ((usage.percent_left / 100.0) * bar_width as f64).round() as usize;
            let empty = bar_width.saturating_sub(filled);
            right1.push(Span::styled(
                format!("{}/{} ", usage.requests_left, usage.limit),
                Style::default().fg(color).bg(bg),
            ));
            right1.push(Span::styled(
                format!("{:.0}% ", usage.percent_left),
                Style::default()
                    .fg(color)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ));
            right1.push(Span::styled(
                "\u{2588}".repeat(filled),
                Style::default().fg(color).bg(bg),
            ));
            right1.push(Span::styled(
                "\u{2591}".repeat(empty),
                Style::default().fg(Theme::text_muted()).bg(bg),
            ));
            right1.push(Span::styled(" ", Style::default().bg(bg)));
        }
    }

    match &data.auth_status {
        AuthDisplay::Connected(_) => {
            let pulse = if (data.animation_frame / 6).is_multiple_of(2) {
                "◉"
            } else {
                "●"
            };
            right1.push(Span::styled(
                format!(" {} connected ", pulse),
                Style::default()
                    .fg(Theme::green())
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        AuthDisplay::NotConnected => {
            right1.push(Span::styled(
                " /connect ",
                Style::default().fg(Theme::text_muted()).bg(bg),
            ));
        }
        AuthDisplay::None => {}
    }

    if data.has_kpms {
        right1.push(Span::styled(
            "KPMS ",
            Style::default().fg(Theme::green()).bg(bg),
        ));
    }

    let left1_width: usize = line1_spans.iter().map(|s| s.width()).sum();
    let right1_width: usize = right1.iter().map(|s| s.width()).sum();
    let pad1 = (area.width as usize).saturating_sub(left1_width + right1_width);
    line1_spans.push(Span::styled(" ".repeat(pad1), Style::default().bg(bg)));
    line1_spans.extend(right1);

    // Line 2: model info bar (subtle)
    let mut line2_spans = Vec::new();
    let dim_bg = Theme::surface();

    let cat_color = match data.model_category.as_str() {
        "Max" => Theme::red(),
        "High" => Theme::peach(),
        "Medium" => Theme::blue(),
        "Light" => Theme::green(),
        _ => Theme::text_dim(),
    };

    line2_spans.push(Span::styled(
        " nano ",
        Style::default().fg(Theme::text_dim()).bg(dim_bg),
    ));
    line2_spans.push(Span::styled(
        data.model_name.to_string(),
        Style::default()
            .fg(Theme::lavender())
            .bg(dim_bg)
            .add_modifier(Modifier::BOLD),
    ));
    line2_spans.push(Span::styled(
        format!(" [{}]", data.model_category),
        Style::default().fg(cat_color).bg(dim_bg),
    ));

    if !data.reasoning.is_empty() && data.reasoning != "none" {
        line2_spans.push(Span::styled(
            format!(" reason:{}", data.reasoning),
            Style::default().fg(Theme::text_muted()).bg(dim_bg),
        ));
    }

    line2_spans.push(Span::styled(
        "  \u{2502}  ",
        Style::default().fg(Theme::border()).bg(dim_bg),
    ));

    line2_spans.push(Span::styled(
        "shizuka ",
        Style::default().fg(Theme::text_dim()).bg(dim_bg),
    ));
    line2_spans.push(Span::styled(
        data.shizuka_model.to_string(),
        Style::default()
            .fg(Theme::cyan())
            .bg(dim_bg)
            .add_modifier(Modifier::BOLD),
    ));

    if data.has_kkm {
        line2_spans.push(Span::styled(
            "  │  tools learned",
            Style::default().fg(Theme::green()).bg(dim_bg),
        ));
    }

    let left2_width: usize = line2_spans.iter().map(|s| s.width()).sum();
    let pad2 = (area.width as usize).saturating_sub(left2_width);
    line2_spans.push(Span::styled(" ".repeat(pad2), Style::default().bg(dim_bg)));

    let lines = vec![Line::from(line1_spans), Line::from(line2_spans)];

    let paragraph = Paragraph::new(lines).style(Style::default().bg(bg));
    frame.render_widget(paragraph, area);
}
