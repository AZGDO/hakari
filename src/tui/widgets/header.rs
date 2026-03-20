use ratatui::prelude::*;
use ratatui::widgets::Paragraph;
use crate::tui::theme::Theme;

pub struct HeaderData {
    pub project_name: String,
    pub session_id: String,
    pub has_kpms: bool,
    pub has_kkm: bool,
}

pub fn render_header(frame: &mut Frame, area: Rect, data: &HeaderData) {
    let mut spans = Vec::new();

    // Logo
    spans.push(Span::styled(
        " HAKARI ",
        Style::default()
            .fg(Theme::mauve())
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(" │ ", Theme::label()));

    // Project name
    spans.push(Span::styled(
        data.project_name.to_string(),
        Style::default().fg(Theme::text()),
    ));
    spans.push(Span::styled(" │ ", Theme::label()));

    // Session
    let short_session = if data.session_id.len() > 8 {
        &data.session_id[..8]
    } else {
        &data.session_id
    };
    spans.push(Span::styled(
        format!("session:{}", short_session),
        Theme::label(),
    ));

    // Memory indicators (right-aligned)
    let right_spans = vec![
        if data.has_kpms {
            Span::styled(" KPMS ", Style::default().fg(Theme::green()))
        } else {
            Span::styled(" KPMS ", Theme::label())
        },
        Span::styled("│", Theme::label()),
        if data.has_kkm {
            Span::styled(" KKM ", Style::default().fg(Theme::green()))
        } else {
            Span::styled(" KKM ", Theme::label())
        },
        Span::raw(" "),
    ];

    // Calculate right offset
    let left_width: usize = spans.iter().map(|s| s.width()).sum();
    let right_width: usize = right_spans.iter().map(|s| s.width()).sum();
    let padding = (area.width as usize).saturating_sub(left_width + right_width);

    spans.push(Span::raw(" ".repeat(padding)));
    spans.extend(right_spans);

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line).style(Theme::header());
    frame.render_widget(paragraph, area);
}
