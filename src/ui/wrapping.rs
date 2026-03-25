use ratatui::text::Line;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Compute display width of a &str using unicode-width semantics.
pub fn visual_width_str(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Compute the number of terminal rows a list of Lines will occupy after word-wrap at `width`.
pub fn wrapped_height(lines: &[Line], width: usize) -> usize {
    if width == 0 {
        return lines.len();
    }
    lines
        .iter()
        .map(|line| {
            let display_width: usize = line
                .spans
                .iter()
                .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                .sum();
            if display_width == 0 {
                1
            } else {
                (display_width + width - 1) / width
            }
        })
        .sum()
}

/// Compute how many visual wrapped lines a logical line will occupy, given first-line and subsequent-line widths.
pub fn visual_lines_for_line(line: &str, first_w: usize, sub_w: usize, is_first: bool) -> usize {
    let total = visual_width_str(line);
    if total == 0 {
        1
    } else if is_first {
        if total <= first_w {
            1
        } else {
            1 + ((total.saturating_sub(first_w) + sub_w - 1) / sub_w)
        }
    } else {
        (total + sub_w - 1) / sub_w
    }
}

/// Given a logical column (in char indices) compute which wrapped visual line the cursor is on and its visual column.
/// Returns (wrap_index, visual_col).
pub fn compute_visual_cursor(
    line: &str,
    logical_col: usize,
    first_w: usize,
    sub_w: usize,
    is_first: bool,
) -> (usize, usize) {
    let mut curr_wrap = 0usize;
    let mut curr_w = 0usize;
    let mut limit = if is_first { first_w } else { sub_w };
    for (idx, ch) in line.chars().enumerate() {
        if idx >= logical_col {
            break;
        }
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if limit == 0 {
            limit = 1;
        }
        if curr_w + w > limit {
            curr_wrap += 1;
            curr_w = 0;
            limit = sub_w;
        }
        curr_w += w;
    }
    (curr_wrap, curr_w)
}
