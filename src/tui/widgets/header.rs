use ratatui::prelude::*;
use ratatui::widgets::Paragraph;
use crate::tui::theme::Theme;

pub struct HeaderData {
    pub project_name: String,
    pub session_id: String,
    pub has_kpms: bool,
    pub has_kkm: bool,
    pub model_name: String,
    pub auth_status: AuthDisplay,
}

#[derive(Debug, Clone)]
pub enum AuthDisplay {
    None,
    Connected(String),
    NotConnected,
}

pub fn render_header(frame: &mut Frame, area: Rect, data: &HeaderData) {
    let mut spans = Vec::new();

    spans.push(Span::styled(
        " HAKARI ",
        Style::default().fg(Theme::mauve()).add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(" │ ", Style::default().fg(Theme::border())));

    spans.push(Span::styled(
        data.project_name.to_string(),
        Style::default().fg(Theme::text()),
    ));
    spans.push(Span::styled(" │ ", Style::default().fg(Theme::border())));

    if !data.model_name.is_empty() {
        spans.push(Span::styled(
            data.model_name.to_string(),
            Style::default().fg(Theme::cyan()),
        ));
        spans.push(Span::styled(" │ ", Style::default().fg(Theme::border())));
    }

    let short_session = if data.session_id.len() > 8 {
        &data.session_id[..8]
    } else {
        &data.session_id
    };
    spans.push(Span::styled(
        format!("{}", short_session),
        Theme::label(),
    ));

    let mut right_spans = Vec::new();

    match &data.auth_status {
        AuthDisplay::Connected(preview) => {
            right_spans.push(Span::styled(
                format!(" ● {} ", preview),
                Style::default().fg(Theme::green()),
            ));
        }
        AuthDisplay::NotConnected => {
            right_spans.push(Span::styled(
                " ○ /connect ",
                Style::default().fg(Theme::text_dim()),
            ));
        }
        AuthDisplay::None => {}
    }

    right_spans.push(Span::styled("│", Style::default().fg(Theme::border())));

    if data.has_kpms {
        right_spans.push(Span::styled(" KPMS", Style::default().fg(Theme::green())));
    } else {
        right_spans.push(Span::styled(" KPMS", Theme::label()));
    }
    right_spans.push(Span::styled(" ", Style::default()));

    let left_width: usize = spans.iter().map(|s| s.width()).sum();
    let right_width: usize = right_spans.iter().map(|s| s.width()).sum();
    let padding = (area.width as usize).saturating_sub(left_width + right_width);

    spans.push(Span::raw(" ".repeat(padding)));
    spans.extend(right_spans);

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line).style(Theme::header());
    frame.render_widget(paragraph, area);
}
