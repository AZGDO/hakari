use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::state::AppState;
use crate::types::PermissionMode;
use crate::ui::helpers::mode_color;
use crate::ui::wrapping::{compute_visual_cursor, visual_lines_for_line};

pub fn render_input_with_rules(
    frame: &mut Frame,
    state: &AppState,
    sep_top: Rect,
    input_area: Rect,
    sep_bottom: Rect,
) {
    let theme = &state.theme;

    // Top separator
    let rule = "─".repeat(sep_top.width as usize);
    let top_rule = Paragraph::new(Span::styled(
        rule.clone(),
        Style::default().fg(theme.subtle),
    ));
    frame.render_widget(top_rule, sep_top);

    let is_busy = state.is_loading || state.streaming.is_some() || state.pending_stream.is_some();
    let prompt_char = if is_busy { "↯" } else { "❯" };
    let prompt_color = if is_busy { theme.claude } else { theme.text };
    let _prompt_width: u16 = 2;

    // Mode indicator on the right
    let mode_indicator = match &state.permission_mode {
        PermissionMode::Default => None,
        mode => Some((mode.symbol(), mode.short_title(), mode_color(mode, theme))),
    };
    let indicator_width = mode_indicator
        .as_ref()
        .map(|(sym, title, _)| format!("{} {}  ", sym, title).len() as u16)
        .unwrap_or(0);

    // Choose displayed text: pending stream takes precedence for display
    let displayed_input = if let Some(ref p) = state.pending_stream {
        p.text.as_str()
    } else {
        state.input.as_str()
    };

    // Split input into lines
    let input_lines: Vec<&str> = displayed_input.split('\n').collect();

    // Compute (cursor_row, cursor_col) from flat cursor_pos (logical coordinates)
    let (cursor_row, cursor_col) = {
        let mut rem = state.cursor_pos;
        // If pending_stream is showing, clamp cursor to displayed content length
        if state.pending_stream.is_some() {
            let total_chars: usize = displayed_input.chars().count();
            if rem > total_chars {
                rem = total_chars;
            }
        }
        let mut row = 0usize;
        let mut col = 0usize;
        for (i, ln) in input_lines.iter().enumerate() {
            let len = ln.chars().count();
            if rem <= len || i == input_lines.len() - 1 {
                row = i;
                col = rem.min(len);
                break;
            }
            rem -= len + 1; // +1 for '\n'
        }
        (row, col)
    };

    // Build ratatui Lines: first gets prompt prefix, rest get indent
    let mut rendered: Vec<Line> = input_lines
        .iter()
        .enumerate()
        .map(|(i, ln)| {
            if i == 0 {
                Line::from(vec![
                    Span::styled(
                        format!("{} ", prompt_char),
                        Style::default()
                            .fg(prompt_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(ln.to_string(), Style::default().fg(theme.text)),
                ])
            } else {
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(ln.to_string(), Style::default().fg(theme.text)),
                ])
            }
        })
        .collect();

    // If empty, still show one blank line with just the prompt
    if rendered.is_empty() {
        rendered.push(Line::from(Span::styled(
            format!("{} ", prompt_char),
            Style::default()
                .fg(prompt_color)
                .add_modifier(Modifier::BOLD),
        )));
    }

    // Render the input paragraph with wrapping
    let input_paragraph = Paragraph::new(rendered).wrap(Wrap::default());
    frame.render_widget(input_paragraph, input_area);

    // Mode indicator right-aligned on first row
    if let Some((symbol, title, color)) = mode_indicator {
        let indicator_text = format!("{} {} ", symbol, title);
        let iw = indicator_text.len() as u16;
        if input_area.width > iw + 4 {
            let ind_area = Rect::new(input_area.x + input_area.width - iw, input_area.y, iw, 1);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    indicator_text,
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ))),
                ind_area,
            );
        }
    }

    // Cursor: compute visual coordinates accounting for wrapping and prompt/indent
    if !is_busy {
        let area_w = input_area.width as usize;
        // effective width respects the mode indicator on the right
        let eff_w = area_w.saturating_sub(indicator_width as usize).max(1);
        // compute actual prompt width (prompt + trailing space)
        let prompt_prefix = format!("{} ", prompt_char);
        let pw = UnicodeWidthStr::width(prompt_prefix.as_str());
        // width available for text on the first logical line (after prompt)
        let first_w = eff_w.saturating_sub(pw).max(1);
        // width available for subsequent logical lines (after indent "  ")
        let indent_w = UnicodeWidthStr::width("  ");
        let sub_w = eff_w.saturating_sub(indent_w).max(1);

        let mut visual_row: usize = 0;
        let mut cursor_visual_col: usize = 0;
        let mut cursor_wrap_index: usize = 0; // which wrapped visual line within the logical line

        for (i, ln) in input_lines.iter().enumerate() {
            if i < cursor_row {
                visual_row += visual_lines_for_line(ln, first_w, sub_w, i == 0);
            } else if i == cursor_row {
                let (wrap_index, vis_col) =
                    compute_visual_cursor(ln, cursor_col, first_w, sub_w, i == 0);
                cursor_wrap_index = wrap_index;
                cursor_visual_col = vis_col;
                visual_row += cursor_wrap_index;
                break;
            }
        }

        // Determine absolute cursor X based on whether it's on the first visual line of its logical line
        let on_first_vis_of_line = cursor_wrap_index == 0;
        let x_base = if cursor_row == 0 && on_first_vis_of_line {
            input_area.x + pw as u16
        } else {
            input_area.x + indent_w as u16 // indent for wrapped/subsequent lines
        };
        let cx = x_base + cursor_visual_col as u16;
        // Respect input_scroll: compute visible row relative to input_area
        let scroll = state.input_scroll as isize;
        let vis_row = visual_row as isize - scroll;
        if vis_row >= 0 && (vis_row as u16) < input_area.height {
            let cy = input_area.y + vis_row as u16;
            // Use new API frame.set_cursor which takes (u16,u16)
            if cx < input_area.x + input_area.width.saturating_sub(indicator_width)
                && cy < input_area.y + input_area.height
            {
                frame.set_cursor_position((cx, cy));
            }
        }
    }

    // Bottom separator
    let bottom_rule = Paragraph::new(Span::styled(rule, Style::default().fg(theme.subtle)));
    frame.render_widget(bottom_rule, sep_bottom);
}
