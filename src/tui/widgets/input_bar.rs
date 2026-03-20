use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use crate::tui::theme::Theme;

pub struct InputBar {
    pub content: String,
    pub cursor_pos: usize,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub focused: bool,
}

impl InputBar {
    pub fn new() -> Self {
        Self {
            content: String::new(),
            cursor_pos: 0,
            history: Vec::new(),
            history_index: None,
            focused: true,
        }
    }

    pub fn insert_char(&mut self, ch: char) {
        self.content.insert(self.cursor_pos, ch);
        self.cursor_pos += ch.len_utf8();
        self.history_index = None;
    }

    pub fn insert_str(&mut self, s: &str) {
        self.content.insert_str(self.cursor_pos, s);
        self.cursor_pos += s.len();
        self.history_index = None;
    }

    pub fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    pub fn delete_char_before(&mut self) {
        if self.cursor_pos > 0 {
            let prev = self.content[..self.cursor_pos]
                .chars()
                .last()
                .map(|c| c.len_utf8())
                .unwrap_or(0);
            self.cursor_pos -= prev;
            self.content.remove(self.cursor_pos);
        }
    }

    pub fn delete_char_after(&mut self) {
        if self.cursor_pos < self.content.len() {
            self.content.remove(self.cursor_pos);
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            let prev = self.content[..self.cursor_pos]
                .chars()
                .last()
                .map(|c| c.len_utf8())
                .unwrap_or(0);
            self.cursor_pos -= prev;
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor_pos < self.content.len() {
            let next = self.content[self.cursor_pos..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(0);
            self.cursor_pos += next;
        }
    }

    pub fn move_cursor_home(&mut self) {
        // Move to start of current line
        let before = &self.content[..self.cursor_pos];
        if let Some(pos) = before.rfind('\n') {
            self.cursor_pos = pos + 1;
        } else {
            self.cursor_pos = 0;
        }
    }

    pub fn move_cursor_end(&mut self) {
        // Move to end of current line
        let after = &self.content[self.cursor_pos..];
        if let Some(pos) = after.find('\n') {
            self.cursor_pos += pos;
        } else {
            self.cursor_pos = self.content.len();
        }
    }

    pub fn delete_word_before(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }
        let before = &self.content[..self.cursor_pos];
        let trimmed = before.trim_end();
        let new_pos = trimmed
            .rfind(|c: char| c.is_whitespace() || c == '/' || c == '.')
            .map(|p| p + 1)
            .unwrap_or(0);
        self.content = format!("{}{}", &self.content[..new_pos], &self.content[self.cursor_pos..]);
        self.cursor_pos = new_pos;
    }

    pub fn submit(&mut self) -> Option<String> {
        let text = self.content.trim().to_string();
        if text.is_empty() {
            return None;
        }
        self.history.push(text.clone());
        self.content.clear();
        self.cursor_pos = 0;
        self.history_index = None;
        Some(text)
    }

    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let idx = match self.history_index {
            Some(0) => 0,
            Some(i) => i - 1,
            None => self.history.len() - 1,
        };
        self.history_index = Some(idx);
        self.content = self.history[idx].clone();
        self.cursor_pos = self.content.len();
    }

    pub fn history_next(&mut self) {
        if let Some(idx) = self.history_index {
            if idx + 1 < self.history.len() {
                self.history_index = Some(idx + 1);
                self.content = self.history[idx + 1].clone();
                self.cursor_pos = self.content.len();
            } else {
                self.history_index = None;
                self.content.clear();
                self.cursor_pos = 0;
            }
        }
    }

    pub fn line_count(&self) -> usize {
        self.content.lines().count().max(1)
    }

    pub fn desired_height(&self) -> u16 {
        (self.line_count() as u16 + 2).min(10) // +2 for borders
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let border_style = if self.focused {
            Theme::input_border()
        } else {
            Style::default().fg(Theme::border())
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title(Line::from(vec![
                Span::styled(" > ", Theme::input_text()),
            ]));

        let inner = block.inner(area);

        let paragraph = Paragraph::new(self.content.as_str())
            .style(Theme::input_text())
            .wrap(Wrap { trim: false });

        frame.render_widget(block, area);
        frame.render_widget(paragraph, inner);

        // Cursor position
        if self.focused {
            let (cx, cy) = self.cursor_xy(inner.width as usize);
            let cursor_x = inner.x + cx as u16;
            let cursor_y = inner.y + cy as u16;
            if cursor_y < inner.y + inner.height {
                frame.set_cursor_position(Position { x: cursor_x, y: cursor_y });
            }
        }
    }

    fn cursor_xy(&self, width: usize) -> (usize, usize) {
        let text_before = &self.content[..self.cursor_pos];
        let mut x = 0;
        let mut y = 0;

        for ch in text_before.chars() {
            if ch == '\n' {
                x = 0;
                y += 1;
            } else {
                x += 1;
                if width > 0 && x >= width {
                    x = 0;
                    y += 1;
                }
            }
        }

        (x, y)
    }
}
