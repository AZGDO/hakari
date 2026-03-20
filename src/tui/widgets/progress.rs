use ratatui::prelude::*;
use crate::tui::theme::Theme;

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub struct Spinner {
    frame: usize,
    pub label: String,
}

impl Spinner {
    pub fn new(label: &str) -> Self {
        Self {
            frame: 0,
            label: label.to_string(),
        }
    }

    pub fn tick(&mut self) {
        self.frame = (self.frame + 1) % SPINNER_FRAMES.len();
    }

    pub fn as_span(&self) -> Span<'_> {
        Span::styled(
            format!("{} {}", SPINNER_FRAMES[self.frame], self.label),
            Theme::spinner(),
        )
    }

    pub fn as_string(&self) -> String {
        format!("{} {}", SPINNER_FRAMES[self.frame], self.label)
    }
}
