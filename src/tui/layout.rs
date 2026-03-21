use ratatui::prelude::*;

#[derive(Debug, Clone, Copy)]
pub struct AppLayout {
    pub header: Rect,
    pub messages: Rect,
    pub input: Rect,
    pub status: Rect,
}

impl AppLayout {
    pub fn compute(area: Rect, input_height: u16) -> Self {
        let clamped_input = input_height.clamp(3, area.height.saturating_sub(6).max(3));

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),             // header (2-line)
                Constraint::Min(4),                // messages
                Constraint::Length(clamped_input), // input
                Constraint::Length(1),             // status
            ])
            .split(area);

        Self {
            header: chunks[0],
            messages: chunks[1],
            input: chunks[2],
            status: chunks[3],
        }
    }
}
