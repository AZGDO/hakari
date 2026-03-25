use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
    Frame,
};

// ── DialogRow ───────────────────────────────────────────────────────────────────
/// A single logical row in a dialog. Rows can be selectable (items the user
/// can highlight and choose) or non-selectable (headers, static text, blanks).
/// Each row expands to one or more terminal lines.
pub struct DialogRow<'a> {
    pub lines: Vec<Line<'a>>,
    pub selectable: bool,
}

impl<'a> DialogRow<'a> {
    /// A provider/section header — not selectable.
    pub fn header(line: Line<'a>) -> Self {
        Self {
            lines: vec![line],
            selectable: false,
        }
    }

    /// A selectable item, potentially spanning multiple display lines.
    pub fn item(lines: Vec<Line<'a>>) -> Self {
        Self {
            lines,
            selectable: true,
        }
    }

    /// A single-line selectable item.
    pub fn item1(line: Line<'a>) -> Self {
        Self {
            lines: vec![line],
            selectable: true,
        }
    }

    /// Non-selectable static text.
    pub fn text(line: Line<'a>) -> Self {
        Self {
            lines: vec![line],
            selectable: false,
        }
    }

    /// Blank separator.
    pub fn blank() -> Self {
        Self {
            lines: vec![Line::from("")],
            selectable: false,
        }
    }
}

// ── DialogState ─────────────────────────────────────────────────────────────────
/// Manages selection + scroll for any scrollable dialog.
///
/// `selected` is the index among **selectable** rows only (0-based).
/// `scroll` is the line offset into the flattened content for the viewport.
///
/// Scroll is automatically adjusted during rendering to keep the selected item
/// visible — no manual scroll tracking needed in event handlers.
pub struct DialogState {
    pub selected: usize,
    pub scroll: usize,
}

impl DialogState {
    pub fn new() -> Self {
        Self {
            selected: 0,
            scroll: 0,
        }
    }

    pub fn reset(&mut self) {
        self.selected = 0;
        self.scroll = 0;
    }

    /// Move selection up by one selectable item.
    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Move selection down by one selectable item (clamped to `selectable_count - 1`).
    pub fn move_down(&mut self, selectable_count: usize) {
        if selectable_count > 0 && self.selected + 1 < selectable_count {
            self.selected += 1;
        }
    }

    /// Set selection, clamping to valid range.
    pub fn select(&mut self, idx: usize, selectable_count: usize) {
        self.selected = idx.min(selectable_count.saturating_sub(1));
    }
}

// ── DialogConfig ────────────────────────────────────────────────────────────────
/// Visual configuration for a dialog popup.
pub struct DialogConfig<'a> {
    pub title: &'a str,
    pub width: u16,
    pub border_color: Color,
    pub title_color: Color,
}

impl<'a> DialogConfig<'a> {
    pub fn new(title: &'a str, border_color: Color, title_color: Color) -> Self {
        Self {
            title,
            width: 65,
            border_color,
            title_color,
        }
    }

    pub fn width(mut self, w: u16) -> Self {
        self.width = w;
        self
    }
}

// ── render_dialog ───────────────────────────────────────────────────────────────
/// Render a scrollable popup dialog, automatically adjusting `state.scroll` to
/// keep the selected item visible. Returns the 0-based index of the selected
/// selectable row (same as `state.selected`).
///
/// `rows` — the dialog content built by the caller.
/// `state` — mutable; scroll is auto-adjusted based on viewport size.
/// `config` — visual settings (title, width, colors).
pub fn render_dialog(
    frame: &mut Frame,
    area: Rect,
    config: &DialogConfig,
    rows: &[DialogRow],
    state: &mut DialogState,
) {
    // 1. Flatten rows → terminal lines, tracking selected item's line span.
    let mut all_lines: Vec<Line> = Vec::new();
    let mut selected_start: usize = 0;
    let mut selected_end: usize = 0;
    let mut selectable_idx: usize = 0;

    for row in rows {
        let line_start = all_lines.len();
        all_lines.extend(row.lines.iter().cloned());
        let line_end = all_lines.len();

        if row.selectable {
            if selectable_idx == state.selected {
                selected_start = line_start;
                selected_end = line_end;
            }
            selectable_idx += 1;
        }
    }

    // 2. Compute centered dialog size.
    let dialog_width = config.width.min(area.width.saturating_sub(4));
    let total_lines = all_lines.len() as u16;
    // +2 for top/bottom border
    let dialog_height = (total_lines + 2)
        .min(area.height.saturating_sub(2))
        .max(4); // minimum usable height
    let dialog_x = area.width.saturating_sub(dialog_width) / 2;
    let dialog_y = area.height.saturating_sub(dialog_height) / 2;
    let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(config.border_color))
        .title(Span::styled(
            config.title,
            Style::default()
                .fg(config.title_color)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);
    let visible = inner.height as usize;
    if visible == 0 {
        return;
    }

    // 3. Auto-scroll to keep the selected item visible.
    if selected_start < state.scroll {
        // Selected item is above the viewport — scroll up.
        state.scroll = selected_start;
    } else if selected_end > state.scroll + visible {
        // Selected item is below the viewport — scroll down.
        state.scroll = selected_end.saturating_sub(visible);
    }
    // Clamp scroll to valid range.
    let max_scroll = all_lines.len().saturating_sub(visible);
    state.scroll = state.scroll.min(max_scroll);

    // 4. Render the visible slice.
    let visible_lines: Vec<Line> = all_lines
        .into_iter()
        .skip(state.scroll)
        .take(visible)
        .collect();

    let paragraph = Paragraph::new(visible_lines);
    frame.render_widget(paragraph, inner);

    // 5. Scrollbar when content overflows.
    if max_scroll > 0 {
        let mut sb_state = ScrollbarState::new(max_scroll).position(state.scroll);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        frame.render_stateful_widget(scrollbar, inner, &mut sb_state);
    }
}
