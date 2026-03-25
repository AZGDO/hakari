use crate::theme::Theme;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

pub const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub const GRADIENT_COLORS: &[(u8, u8, u8)] = &[
    (235, 95, 87),
    (245, 139, 87),
    (250, 195, 95),
    (145, 200, 130),
    (130, 170, 220),
    (155, 130, 200),
    (200, 130, 180),
];

pub const CLAWD_ART: &[&str] = &[
    "████████████▓▒░░░░░░░░░▒▓███████████████",
    "███████▓▒░░░░░░░▒▒░░░▒▒░░░▒█████████████",
    "█████▒░░░░░░░░░░░░░░░░░░░░░░████████████",
    "███▓░░░░░░░░░░░░░░░░░░░▒░░░▒░▒▒▒████████",
    "██▒░░▒░░░░░░░░░░▒▒▒░░▒▒▒▒░░░▒░░░░░░░▒▓██",
    "██▒░▒░░░░░░░░░░░▒▒▒▒▒▒▒▒▒▒▒▒░░░░░░░░░░░░",
    "██▒▒▒░░░▒▒▒▒▒▒▒▒▒▓▒▒▒▓▒▒░▒▒▒░░░░░░░░░░░░",
    "██▒▒▒▒▒▒▒▒▒▒▒▒▒▓▒░▒▒▒▒▒▒░░░▒▒░▒▒██▓░░░░░",
    "██▒▒▒▒▒▒▒▒▒▒▒▒▒▒░░▒▒▒▒░▒░▒▒▒▒▒▒▒██▓▓▒▒▓▓",
    "██▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒░░░░░░░░░░▒▒▒███▓▓▓▓▓",
    "██▒▒▒▒▒▒▒▒▒▒▒▒▓░░░░░░░░░░░░░▒▒▒▒▓████▓▓▓",
    "██▒░░▒▒▒▒▒▒▒▒░░░░░░░░░░░░░░░▒▒▒▒▓█████▓▓",
    "██░░░░░░▒▒▒▒▓▒░░░░░░░░░░░░░░▓▒▒▒▒█████▓▓",
    "█▒░░░░░▒▒▒▒▒▓▓▒░░░░░░░░░░░▒▓▓▒▒▒▓▓▓▓▓███",
    "██▒░░░░░▒▒▒▒▓▓▓▓▒▒▒░░░░░▒▓▓▓▒▒▒▒▓▓▓▓▓▓▓▓",
    "███▒▓▓▓▓▒▒▒▒▒▓▓▓▒▒▒▒▒▒▒▒▒▒▓▓▒▒▒▒▓▓▓▓▓▓▓▓",
    "████▓█▓█▓▒▒▒▒▓▓▒▒▒▒▒▒▒▒▒▒▒▓▒▒▒▒▓▓▓▓▓▓███",
    "██████▓▓▓▓▒▒▒▒▓▒▒▒▒▒▒▒▒▒▒░▒▒▒▒▓▓▓▓▓▓▓▓▓▓",
    "█████▓▓▓▓▓▓▓▒▒▒▒░░░▒▒▒▓░░░▒▒▓▓▓▓▓▓▓▓▓▓▓▓",
    "█████▓▓██▓▓▓▓▓▓▓▒▒░░▒▓▓▒░░░▓▓▓▓▓▓▓▓█▓▓██",
];

pub fn gradient_char(ch: char, pos: usize, total: usize, offset: f64) -> Span<'static> {
    let t = (pos as f64 / total.max(1) as f64 + offset) % 1.0;
    let idx = t * (GRADIENT_COLORS.len() - 1) as f64;
    let lower = idx.floor() as usize;
    let upper = (lower + 1).min(GRADIENT_COLORS.len() - 1);
    let frac = idx - lower as f64;

    let (r1, g1, b1) = GRADIENT_COLORS[lower];
    let (r2, g2, b2) = GRADIENT_COLORS[upper];
    let r = (r1 as f64 * (1.0 - frac) + r2 as f64 * frac) as u8;
    let g = (g1 as f64 * (1.0 - frac) + g2 as f64 * frac) as u8;
    let b = (b1 as f64 * (1.0 - frac) + b2 as f64 * frac) as u8;

    Span::styled(
        ch.to_string(),
        Style::default()
            .fg(Color::Rgb(r, g, b))
            .add_modifier(Modifier::BOLD),
    )
}

pub fn shorten_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        return path.to_string();
    }

    if let Ok(home) = std::env::var("HOME") {
        if let Some(stripped) = path.strip_prefix(&home) {
            let shortened = format!("~{}", stripped);
            if shortened.len() <= max_len {
                return shortened;
            }
        }
    }

    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() <= 3 {
        return path.to_string();
    }

    format!(".../{}", parts.last().unwrap_or(&""))
}

pub fn safe_truncate_chars(s: &str, max_chars: usize) -> &str {
    let mut end = 0;
    let mut count = 0;
    for (i, _) in s.char_indices() {
        if count == max_chars {
            break;
        }
        end = i;
        count += 1;
    }
    if count < max_chars {
        s
    } else {
        &s[..end]
    }
}

pub fn mode_color(mode: &crate::types::PermissionMode, theme: &Theme) -> Color {
    match mode {
        crate::types::PermissionMode::Default => theme.text,
        crate::types::PermissionMode::PlanMode => theme.plan_mode,
        crate::types::PermissionMode::AcceptEdits => theme.auto_accept,
        crate::types::PermissionMode::BypassPermissions => theme.error,
        crate::types::PermissionMode::DontAsk => theme.error,
        crate::types::PermissionMode::Auto => theme.warning,
    }
}
