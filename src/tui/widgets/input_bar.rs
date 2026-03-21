use crate::tui::commands;
use crate::tui::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use std::path::Path;

pub struct InputBar {
    pub content: String,
    pub cursor_pos: usize,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub focused: bool,
    pub slash_suggestions: Vec<&'static commands::SlashCommand>,
    pub slash_selected: usize,
    pub file_suggestions: Vec<String>,
    pub file_selected: usize,
    pub pinned_files: Vec<String>,
    pub show_suggestions: bool,
}

impl InputBar {
    pub fn new() -> Self {
        Self {
            content: String::new(),
            cursor_pos: 0,
            history: Vec::new(),
            history_index: None,
            focused: true,
            slash_suggestions: Vec::new(),
            slash_selected: 0,
            file_suggestions: Vec::new(),
            file_selected: 0,
            pinned_files: Vec::new(),
            show_suggestions: false,
        }
    }

    pub fn insert_char(&mut self, ch: char) {
        self.content.insert(self.cursor_pos, ch);
        self.cursor_pos += ch.len_utf8();
        self.history_index = None;
        self.update_suggestions_state();
    }

    pub fn insert_str(&mut self, s: &str) {
        self.content.insert_str(self.cursor_pos, s);
        self.cursor_pos += s.len();
        self.history_index = None;
        self.update_suggestions_state();
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
            self.update_suggestions_state();
        }
    }

    pub fn delete_char_after(&mut self) {
        if self.cursor_pos < self.content.len() {
            self.content.remove(self.cursor_pos);
            self.update_suggestions_state();
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
        let before = &self.content[..self.cursor_pos];
        if let Some(pos) = before.rfind('\n') {
            self.cursor_pos = pos + 1;
        } else {
            self.cursor_pos = 0;
        }
    }

    pub fn move_cursor_end(&mut self) {
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
        self.content = format!(
            "{}{}",
            &self.content[..new_pos],
            &self.content[self.cursor_pos..]
        );
        self.cursor_pos = new_pos;
        self.update_suggestions_state();
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
        self.clear_suggestions();
        Some(text)
    }

    pub fn accept_suggestion(&mut self) -> bool {
        if !self.slash_suggestions.is_empty() && self.show_suggestions {
            let cmd = self.slash_suggestions[self.slash_selected].name;
            self.content = format!("{} ", cmd);
            self.cursor_pos = self.content.len();
            self.clear_suggestions();
            return true;
        }
        if !self.file_suggestions.is_empty() && self.show_suggestions {
            let file = self.file_suggestions[self.file_selected].clone();
            // Replace the @query with the file
            if let Some(at_pos) = self.content[..self.cursor_pos].rfind('@') {
                self.content = format!(
                    "{}@{} {}",
                    &self.content[..at_pos],
                    file,
                    &self.content[self.cursor_pos..]
                );
                self.cursor_pos = at_pos + 1 + file.len() + 1;
            }
            self.clear_suggestions();
            return true;
        }
        false
    }

    pub fn suggestion_up(&mut self) {
        if !self.slash_suggestions.is_empty() {
            self.slash_selected = self.slash_selected.saturating_sub(1);
        }
        if !self.file_suggestions.is_empty() {
            self.file_selected = self.file_selected.saturating_sub(1);
        }
    }

    pub fn suggestion_down(&mut self) {
        if !self.slash_suggestions.is_empty() {
            self.slash_selected =
                (self.slash_selected + 1).min(self.slash_suggestions.len().saturating_sub(1));
        }
        if !self.file_suggestions.is_empty() {
            self.file_selected =
                (self.file_selected + 1).min(self.file_suggestions.len().saturating_sub(1));
        }
    }

    pub fn has_suggestions(&self) -> bool {
        self.show_suggestions
            && (!self.slash_suggestions.is_empty() || !self.file_suggestions.is_empty())
    }

    fn update_suggestions_state(&mut self) {
        // Slash commands
        let trimmed = self.content.trim();
        if trimmed.starts_with('/') && !trimmed.contains(' ') {
            self.slash_suggestions = commands::match_commands(trimmed);
            self.slash_selected = 0;
            self.show_suggestions = !self.slash_suggestions.is_empty();
            self.file_suggestions.clear();
            return;
        }

        self.slash_suggestions.clear();

        // @ file mentions
        if let Some(query) = commands::get_current_at_query(&self.content, self.cursor_pos) {
            if !query.is_empty() {
                self.show_suggestions = true;
                self.file_selected = 0;
                // File matching is done externally via update_file_suggestions
            } else {
                self.show_suggestions = true;
                self.file_selected = 0;
            }
            return;
        }

        self.clear_suggestions();
    }

    fn clear_suggestions(&mut self) {
        self.slash_suggestions.clear();
        self.file_suggestions.clear();
        self.slash_selected = 0;
        self.file_selected = 0;
        self.show_suggestions = false;
    }

    pub fn update_file_suggestions(&mut self, project_dir: &Path) {
        if let Some(query) = commands::get_current_at_query(&self.content, self.cursor_pos) {
            self.file_suggestions = commands::match_files(&query, project_dir);
            if self.file_suggestions.is_empty() && query.is_empty() {
                self.file_suggestions = self.pinned_files.clone();
            }
        } else {
            self.file_suggestions.clear();
        }
    }

    pub fn pin_file(&mut self, path: &str) {
        if !self.pinned_files.contains(&path.to_string()) {
            self.pinned_files.push(path.to_string());
        }
    }

    pub fn unpin_file(&mut self, path: &str) {
        self.pinned_files.retain(|p| p != path);
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
        (self.line_count() as u16 + 2).min(10)
    }

    pub fn suggestion_popup_area(&self, input_area: Rect) -> Option<Rect> {
        if !self.has_suggestions() {
            return None;
        }

        let item_count = if !self.slash_suggestions.is_empty() {
            self.slash_suggestions.len()
        } else {
            self.file_suggestions.len()
        };

        let height = (item_count as u16 + 2).min(12);
        Some(Rect {
            x: input_area.x + 1,
            y: input_area.y.saturating_sub(height),
            width: input_area.width.saturating_sub(2).min(60),
            height,
        })
    }

    pub fn select_suggestion_at(&mut self, input_area: Rect, x: u16, y: u16) -> bool {
        let Some(popup_area) = self.suggestion_popup_area(input_area) else {
            return false;
        };

        if x < popup_area.x
            || x >= popup_area.x + popup_area.width
            || y < popup_area.y
            || y >= popup_area.y + popup_area.height
        {
            return false;
        }

        let row = y.saturating_sub(popup_area.y + 1) as usize;
        if !self.slash_suggestions.is_empty() {
            if row < self.slash_suggestions.len() {
                self.slash_selected = row;
                return self.accept_suggestion();
            }
        } else if row < self.file_suggestions.len() {
            self.file_selected = row;
            return self.accept_suggestion();
        }

        false
    }

    pub fn set_cursor_from_position(&mut self, inner_area: Rect, x: u16, y: u16) {
        if inner_area.width == 0 || inner_area.height == 0 {
            return;
        }

        let rel_x = x.saturating_sub(inner_area.x) as usize;
        let rel_y = y.saturating_sub(inner_area.y) as usize;
        self.cursor_pos = self.cursor_pos_from_xy(rel_x, rel_y, inner_area.width as usize);
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let border_style = if self.focused {
            Theme::input_border()
        } else {
            Style::default().fg(Theme::border())
        };

        // Pinned files indicator in title
        let title_spans = if self.pinned_files.is_empty() {
            vec![Span::styled(" > ", Theme::input_text())]
        } else {
            let mut spans = vec![Span::styled(" > ", Theme::input_text())];
            for (i, file) in self.pinned_files.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::styled(" ", Theme::label()));
                }
                let short = file.rsplit('/').next().unwrap_or(file);
                spans.push(Span::styled(
                    format!(" @{} ", short),
                    Style::default()
                        .fg(Theme::blue())
                        .bg(Theme::surface_bright()),
                ));
            }
            spans.push(Span::styled(" ", Theme::input_text()));
            spans
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title(Line::from(title_spans));

        let inner = block.inner(area);

        // Styled content: highlight @mentions and /commands
        let styled_content = self.build_styled_content();

        let paragraph = Paragraph::new(styled_content).wrap(Wrap { trim: false });

        frame.render_widget(block, area);
        frame.render_widget(paragraph, inner);

        // Cursor position
        if self.focused {
            let (cx, cy) = self.cursor_xy(inner.width as usize);
            let cursor_x = inner.x + cx as u16;
            let cursor_y = inner.y + cy as u16;
            if cursor_y < inner.y + inner.height {
                frame.set_cursor_position(Position {
                    x: cursor_x,
                    y: cursor_y,
                });
            }
        }

        // Render suggestions popup above the input
        if self.has_suggestions() {
            self.render_suggestions(frame, area);
        }
    }

    fn build_styled_content(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        for (line_index, line) in self.content.split('\n').enumerate() {
            let mut spans = Vec::new();
            let mut i = 0;
            let chars: Vec<char> = line.chars().collect();

            while i < chars.len() {
                if chars[i] == '/' && i == 0 && line_index == 0 {
                    let start = i;
                    let mut end = i;
                    while end < chars.len() && !chars[end].is_whitespace() {
                        end += 1;
                    }
                    let cmd_text: String = chars[start..end].iter().collect();
                    spans.push(Span::styled(
                        cmd_text,
                        Style::default()
                            .fg(Theme::mauve())
                            .add_modifier(Modifier::BOLD),
                    ));
                    i = end;
                } else if chars[i] == '@' {
                    let start = i;
                    let mut end = i + 1;
                    while end < chars.len() && !chars[end].is_whitespace() {
                        end += 1;
                    }
                    let mention: String = chars[start..end].iter().collect();
                    spans.push(Span::styled(
                        mention,
                        Style::default()
                            .fg(Theme::blue())
                            .add_modifier(Modifier::BOLD),
                    ));
                    i = end;
                } else {
                    let start = i;
                    while i < chars.len()
                        && chars[i] != '@'
                        && !(chars[i] == '/' && i == 0 && line_index == 0)
                    {
                        i += 1;
                    }
                    let text: String = chars[start..i].iter().collect();
                    spans.push(Span::styled(text, Theme::input_text()));
                }
            }

            if spans.is_empty() {
                spans.push(Span::styled("", Theme::input_text()));
            }

            lines.push(Line::from(spans));
        }

        if lines.is_empty() {
            lines.push(Line::from(Span::styled("", Theme::input_text())));
        }

        lines
    }

    fn render_suggestions(&self, frame: &mut Frame, input_area: Rect) {
        let items: Vec<(String, String, bool)> = if !self.slash_suggestions.is_empty() {
            self.slash_suggestions
                .iter()
                .enumerate()
                .map(|(i, cmd)| {
                    (
                        cmd.name.to_string(),
                        cmd.description.to_string(),
                        i == self.slash_selected,
                    )
                })
                .collect()
        } else if !self.file_suggestions.is_empty() {
            self.file_suggestions
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let short = f.rsplit('/').next().unwrap_or(f);
                    (format!("@{}", short), f.clone(), i == self.file_selected)
                })
                .collect()
        } else {
            return;
        };

        let popup_area = self.suggestion_popup_area(input_area).unwrap_or(input_area);

        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(Style::default().fg(Theme::border_focus()))
            .style(Style::default().bg(Theme::surface()));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let mut lines = Vec::new();
        for (name, desc, selected) in &items {
            let style = if *selected {
                Style::default()
                    .fg(Theme::text_bright())
                    .bg(Theme::surface_bright())
            } else {
                Style::default().fg(Theme::text())
            };
            let name_style = if *selected {
                Style::default()
                    .fg(Theme::mauve())
                    .bg(Theme::surface_bright())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Theme::mauve())
            };

            let padding = 16usize.saturating_sub(name.len());
            lines.push(Line::from(vec![
                Span::styled(format!(" {}", name), name_style),
                Span::styled(" ".repeat(padding), style),
                Span::styled(
                    desc.clone(),
                    Style::default().fg(Theme::text_dim()).bg(if *selected {
                        Theme::surface_bright()
                    } else {
                        Theme::surface()
                    }),
                ),
            ]));
        }

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);
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

    fn cursor_pos_from_xy(&self, target_x: usize, target_y: usize, width: usize) -> usize {
        let mut x = 0;
        let mut y = 0;
        let mut pos = 0;

        for ch in self.content.chars() {
            if y == target_y && x >= target_x {
                return pos;
            }

            pos += ch.len_utf8();
            if ch == '\n' {
                if y == target_y {
                    return pos.saturating_sub(1);
                }
                x = 0;
                y += 1;
                continue;
            }

            x += 1;
            if width > 0 && x >= width {
                if y == target_y {
                    return pos;
                }
                x = 0;
                y += 1;
            }
        }

        self.content.len()
    }
}
