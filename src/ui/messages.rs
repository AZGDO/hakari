use ratatui::layout::Rect;
use ratatui::Frame;

use crate::state::AppState;

pub fn render_welcome_screen(frame: &mut Frame, state: &AppState, area: Rect) {
    let is_busy = state.is_loading || state.streaming.is_some() || state.pending_stream.is_some();
    let input_line_count = crate::ui::layout::get_input_height(state, area.width, is_busy);

    // Layout: welcome box, blank, input area, status bar
    let main_chunks = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Min(0),
            ratatui::layout::Constraint::Length(1),                // blank line
            ratatui::layout::Constraint::Length(1),                // input separator top
            ratatui::layout::Constraint::Length(input_line_count), // input (dynamic)
            ratatui::layout::Constraint::Length(1),                // input separator bottom
            ratatui::layout::Constraint::Length(1),                // status bar
        ])
        .split(area);

    let welcome_area = main_chunks[0];
    let sep_top_area = main_chunks[2];
    let input_line_area = main_chunks[3];
    let sep_bottom_area = main_chunks[4];
    let status_area = main_chunks[5];

    // Welcome box with rounded border
    crate::ui::layout::render_welcome_box(frame, state, welcome_area);

    // Input area with horizontal rules
    crate::ui::input::render_input_with_rules(frame, state, sep_top_area, input_line_area, sep_bottom_area);

    // Status bar
    crate::ui::layout::render_status_bar(frame, state, status_area);
}
