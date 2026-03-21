use crate::tui::theme::Theme;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};

pub enum PopupType {
    Confirmation {
        title: String,
        message: String,
    },
    Help,
    Escalation {
        summary: String,
    },
    ModelSelector {
        models: Vec<ModelEntry>,
        selected: usize,
        loading: bool,
        error: Option<String>,
        scroll_offset: usize,
        target: ModelTarget,
    },
    Settings {
        entries: Vec<SettingEntry>,
        selected: usize,
        editing: Option<(usize, String)>,
    },
    ConnectFlow {
        state: ConnectState,
    },
    ConnectMenu {
        selected: usize,
    },
    ModelList {
        entries: Vec<ModelListDisplay>,
        selected: usize,
    },
    ReasoningSelector {
        levels: Vec<String>,
        selected: usize,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModelTarget {
    Nano,
    Shizuka,
}

#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub id: String,
    pub name: String,
    pub provider: Option<String>,
    pub release_status: Option<String>,
    pub reasoning: bool,
    pub context: usize,
    pub active: bool,
    pub input_rate: Option<f64>,
    pub output_rate: Option<f64>,
    pub premium_multiplier_paid_display: Option<String>,
    pub premium_multiplier_free_display: Option<String>,
    pub included_in_paid: bool,
    pub category: String,
}

#[derive(Debug, Clone)]
pub struct ModelListDisplay {
    pub demand: String,
    pub model_id: String,
    pub category: String,
    pub reasoning: String,
    pub rate: String,
}

#[derive(Debug, Clone)]
pub struct SettingEntry {
    pub key: String,
    pub label: String,
    pub value: String,
    pub editable: bool,
}

#[derive(Debug, Clone)]
pub enum ConnectState {
    Starting,
    WaitingForAuth { uri: String, code: String },
    Polling,
    Success,
    Error(String),
}

pub struct Popup {
    pub popup_type: PopupType,
    pub visible: bool,
}

pub enum PopupMouseAction {
    None,
    Submit,
    Edit,
    Close,
}

impl Popup {
    pub fn confirmation(title: &str, message: &str) -> Self {
        Self {
            popup_type: PopupType::Confirmation {
                title: title.to_string(),
                message: message.to_string(),
            },
            visible: true,
        }
    }

    pub fn help() -> Self {
        Self {
            popup_type: PopupType::Help,
            visible: true,
        }
    }

    pub fn escalation(summary: &str) -> Self {
        Self {
            popup_type: PopupType::Escalation {
                summary: summary.to_string(),
            },
            visible: true,
        }
    }

    pub fn model_selector(
        models: Vec<ModelEntry>,
        current_model: &str,
        target: ModelTarget,
    ) -> Self {
        let selected = models
            .iter()
            .position(|m| m.id == current_model)
            .unwrap_or(0);
        Self {
            popup_type: PopupType::ModelSelector {
                models,
                selected,
                loading: false,
                error: None,
                scroll_offset: 0,
                target,
            },
            visible: true,
        }
    }

    pub fn model_selector_loading(target: ModelTarget) -> Self {
        Self {
            popup_type: PopupType::ModelSelector {
                models: Vec::new(),
                selected: 0,
                loading: true,
                error: None,
                scroll_offset: 0,
                target,
            },
            visible: true,
        }
    }

    pub fn model_selector_error(message: &str, target: ModelTarget) -> Self {
        Self {
            popup_type: PopupType::ModelSelector {
                models: Vec::new(),
                selected: 0,
                loading: false,
                error: Some(message.to_string()),
                scroll_offset: 0,
                target,
            },
            visible: true,
        }
    }

    pub fn settings(entries: Vec<SettingEntry>) -> Self {
        Self {
            popup_type: PopupType::Settings {
                entries,
                selected: 0,
                editing: None,
            },
            visible: true,
        }
    }

    pub fn connect_flow() -> Self {
        Self {
            popup_type: PopupType::ConnectFlow {
                state: ConnectState::Starting,
            },
            visible: true,
        }
    }

    pub fn connect_menu() -> Self {
        Self {
            popup_type: PopupType::ConnectMenu { selected: 0 },
            visible: true,
        }
    }

    pub fn model_list(entries: Vec<ModelListDisplay>) -> Self {
        Self {
            popup_type: PopupType::ModelList {
                entries,
                selected: 0,
            },
            visible: true,
        }
    }

    pub fn reasoning_selector(levels: Vec<String>, current: &str) -> Self {
        let selected = levels.iter().position(|l| l == current).unwrap_or(0);
        Self {
            popup_type: PopupType::ReasoningSelector { levels, selected },
            visible: true,
        }
    }

    pub fn select_up(&mut self) {
        match &mut self.popup_type {
            PopupType::ModelSelector {
                selected,
                scroll_offset,
                ..
            } => {
                let new_sel = selected.saturating_sub(1);
                *selected = new_sel;
                if new_sel < *scroll_offset {
                    *scroll_offset = new_sel;
                }
            }
            PopupType::Settings {
                selected, editing, ..
            } => {
                if editing.is_none() {
                    *selected = selected.saturating_sub(1);
                }
            }
            PopupType::ConnectMenu { selected } => {
                *selected = selected.saturating_sub(1);
            }
            PopupType::ModelList { selected, .. } => {
                *selected = selected.saturating_sub(1);
            }
            PopupType::ReasoningSelector { selected, .. } => {
                *selected = selected.saturating_sub(1);
            }
            _ => {}
        }
    }

    pub fn select_down(&mut self) {
        match &mut self.popup_type {
            PopupType::ModelSelector {
                selected,
                models,
                scroll_offset,
                ..
            } => {
                let new_sel = (*selected + 1).min(models.len().saturating_sub(1));
                *selected = new_sel;
                // Keep scroll_offset so selected is always visible (assume ~20 visible rows)
                let visible: usize = 20;
                if new_sel >= *scroll_offset + visible {
                    *scroll_offset = new_sel.saturating_sub(visible) + 1;
                }
            }
            PopupType::Settings {
                selected,
                entries,
                editing,
                ..
            } => {
                if editing.is_none() {
                    *selected = (*selected + 1).min(entries.len().saturating_sub(1));
                }
            }
            PopupType::ConnectMenu { selected } => {
                *selected = (*selected + 1).min(1);
            }
            PopupType::ModelList { selected, entries } => {
                *selected = (*selected + 1).min(entries.len().saturating_sub(1));
            }
            PopupType::ReasoningSelector { selected, levels } => {
                *selected = (*selected + 1).min(levels.len().saturating_sub(1));
            }
            _ => {}
        }
    }

    /// Returns true if the settings popup is currently in editing mode.
    pub fn settings_is_editing(&self) -> bool {
        matches!(
            &self.popup_type,
            PopupType::Settings {
                editing: Some(_),
                ..
            }
        )
    }

    /// Start editing the currently selected settings entry. Returns false if not editable.
    pub fn settings_start_edit(&mut self) -> bool {
        if let PopupType::Settings {
            entries,
            selected,
            editing,
        } = &mut self.popup_type
        {
            let idx = *selected;
            if let Some(entry) = entries.get(idx) {
                if entry.editable {
                    *editing = Some((idx, entry.value.clone()));
                    return true;
                }
            }
        }
        false
    }

    /// Cancel the in-progress edit without saving.
    pub fn settings_cancel_edit(&mut self) {
        if let PopupType::Settings { editing, .. } = &mut self.popup_type {
            *editing = None;
        }
    }

    /// Commit the in-progress edit, returning (key, new_value) if there was one.
    pub fn settings_commit_edit(&mut self) -> Option<(String, String)> {
        if let PopupType::Settings {
            entries, editing, ..
        } = &mut self.popup_type
        {
            if let Some((idx, ref val)) = editing.clone() {
                let key = entries.get(idx).map(|e| e.key.clone()).unwrap_or_default();
                let new_val = val.clone();
                if let Some(entry) = entries.get_mut(idx) {
                    entry.value = new_val.clone();
                }
                *editing = None;
                return Some((key, new_val));
            }
        }
        None
    }

    /// Feed a character to the in-progress edit buffer.
    pub fn settings_edit_push(&mut self, ch: char) {
        if let PopupType::Settings {
            editing: Some((_, ref mut buf)),
            ..
        } = &mut self.popup_type
        {
            buf.push(ch);
        }
    }

    /// Delete last char from the in-progress edit buffer.
    pub fn settings_edit_backspace(&mut self) {
        if let PopupType::Settings {
            editing: Some((_, ref mut buf)),
            ..
        } = &mut self.popup_type
        {
            buf.pop();
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, animation_frame: u64) {
        if !self.visible {
            return;
        }

        match &self.popup_type {
            PopupType::Confirmation { title, message } => {
                self.render_confirmation(frame, area, title, message);
            }
            PopupType::Help => {
                self.render_help(frame, area);
            }
            PopupType::Escalation { summary } => {
                self.render_escalation(frame, area, summary);
            }
            PopupType::ModelSelector {
                models,
                selected,
                loading,
                error,
                scroll_offset,
                target,
            } => {
                self.render_model_selector(
                    frame,
                    area,
                    models,
                    *selected,
                    *loading,
                    error.as_deref(),
                    *scroll_offset,
                    target,
                    animation_frame,
                );
            }
            PopupType::Settings {
                entries,
                selected,
                editing,
            } => {
                self.render_settings(frame, area, entries, *selected, editing.as_ref());
            }
            PopupType::ConnectFlow { state } => {
                self.render_connect_flow(frame, area, state);
            }
            PopupType::ConnectMenu { selected } => {
                self.render_connect_menu(frame, area, *selected);
            }
            PopupType::ModelList { entries, selected } => {
                self.render_model_list(frame, area, entries, *selected);
            }
            PopupType::ReasoningSelector { levels, selected } => {
                self.render_reasoning_selector(frame, area, levels, *selected);
            }
        }
    }

    pub fn handle_mouse(&mut self, mouse: &MouseEvent, area: Rect) -> PopupMouseAction {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.select_up();
                PopupMouseAction::None
            }
            MouseEventKind::ScrollDown => {
                self.select_down();
                PopupMouseAction::None
            }
            MouseEventKind::Down(MouseButton::Left) => {
                self.handle_left_click(mouse.column, mouse.row, area)
            }
            _ => PopupMouseAction::None,
        }
    }

    fn handle_left_click(&mut self, x: u16, y: u16, area: Rect) -> PopupMouseAction {
        match &mut self.popup_type {
            PopupType::ModelSelector {
                models,
                selected,
                scroll_offset,
                ..
            } => {
                let popup_area = centered_rect(80, 80, area);
                let inner = popup_area.inner(Margin {
                    horizontal: 1,
                    vertical: 1,
                });
                let header_height = 4usize;
                let visible_height = (inner.height as usize).saturating_sub(header_height);
                let row = y.saturating_sub(inner.y) as usize;
                if x < inner.x
                    || x >= inner.x + inner.width
                    || y < inner.y
                    || y >= inner.y + inner.height
                {
                    return PopupMouseAction::Close;
                }
                if row < header_height {
                    return PopupMouseAction::None;
                }
                let visible_row = row - header_height;
                let index = scroll_offset
                    .saturating_add(visible_row)
                    .min(models.len().saturating_sub(1));
                if visible_row < visible_height && !models.is_empty() {
                    *selected = index;
                    return PopupMouseAction::Submit;
                }
                PopupMouseAction::None
            }
            PopupType::ConnectMenu { selected } => {
                let popup_area = centered_rect(55, 30, area);
                if !contains(popup_area, x, y) {
                    return PopupMouseAction::Close;
                }
                let option = y.saturating_sub(popup_area.y + 2) / 3;
                if option <= 1 {
                    *selected = option as usize;
                    PopupMouseAction::Submit
                } else {
                    PopupMouseAction::None
                }
            }
            PopupType::ReasoningSelector { selected, levels } => {
                let popup_area = centered_rect(40, 35, area);
                if !contains(popup_area, x, y) {
                    return PopupMouseAction::Close;
                }
                let option = y.saturating_sub(popup_area.y + 2) as usize;
                if option < levels.len() {
                    *selected = option;
                    PopupMouseAction::Submit
                } else {
                    PopupMouseAction::None
                }
            }
            PopupType::ModelList { selected, entries } => {
                let popup_area = centered_rect(70, 60, area);
                if !contains(popup_area, x, y) {
                    return PopupMouseAction::Close;
                }
                let option = y.saturating_sub(popup_area.y + 4) as usize;
                if option < entries.len() {
                    *selected = option;
                }
                PopupMouseAction::None
            }
            PopupType::Settings {
                selected,
                entries,
                editing,
            } => {
                let popup_area = centered_rect(65, 60, area);
                if !contains(popup_area, x, y) {
                    return PopupMouseAction::Close;
                }
                let option = y.saturating_sub(popup_area.y + 4) as usize;
                if option < entries.len() {
                    *selected = option;
                    if editing.is_none() && entries[option].editable {
                        return PopupMouseAction::Edit;
                    }
                }
                PopupMouseAction::None
            }
            PopupType::Help
            | PopupType::Escalation { .. }
            | PopupType::ConnectFlow { .. }
            | PopupType::Confirmation { .. } => PopupMouseAction::Close,
        }
    }

    fn render_confirmation(&self, frame: &mut Frame, area: Rect, title: &str, message: &str) {
        let popup_area = centered_rect(60, 30, area);
        frame.render_widget(Clear, popup_area);
        let block = Block::default()
            .title(format!(" {} ", title))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Theme::yellow()))
            .style(Style::default().bg(Theme::surface()));

        let text = format!("{}\n\n  [Y] Yes   [N] No   [Esc] Cancel", message);
        let paragraph = Paragraph::new(text)
            .block(block)
            .style(Style::default().fg(Theme::text()))
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, popup_area);
    }

    fn render_help(&self, frame: &mut Frame, area: Rect) {
        let popup_area = centered_rect(72, 80, area);
        frame.render_widget(Clear, popup_area);
        let block = Block::default()
            .title(" Help ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Theme::border_focus()))
            .style(Style::default().bg(Theme::surface()));

        let help_text = vec![
            Line::default(),
            Line::from(Span::styled(
                "  Keybindings",
                Style::default()
                    .fg(Theme::mauve())
                    .add_modifier(Modifier::BOLD),
            )),
            Line::default(),
            help_row("Enter", "Submit message"),
            help_row("Shift+Enter", "New line in input"),
            help_row("Ctrl+C", "Quit"),
            help_row("Ctrl+Shift+C / Cmd+C", "Copy"),
            help_row("Ctrl+Shift+V / Cmd+V", "Paste"),
            help_row("Up/Down", "History / navigate suggestions"),
            help_row("Tab", "Accept suggestion"),
            help_row("PgUp/PgDn / Scroll", "Scroll messages"),
            help_row("Mouse", "Click traces, popups, suggestions"),
            help_row("Ctrl+W", "Delete word"),
            help_row("Esc", "Dismiss / scroll to bottom"),
            Line::default(),
            Line::from(Span::styled(
                "  Commands",
                Style::default()
                    .fg(Theme::mauve())
                    .add_modifier(Modifier::BOLD),
            )),
            Line::default(),
            help_row("/model", "Select nano AI model"),
            help_row("/shizuka", "Select shizuka (prep) model"),
            help_row("/reasoning", "Set reasoning level"),
            help_row("/modellist", "Show model assignments"),
            help_row("/connect", "Connect to provider"),
            help_row("/settings", "Open settings"),
            help_row("/clear", "Clear chat"),
            help_row("/compact", "Collapse traces"),
            help_row("/pin @file", "Pin file to context"),
            help_row("/unpin @file", "Remove pinned file"),
            help_row("/files", "List pinned files"),
            help_row("/status", "Session status"),
            help_row("/reset", "Reset session"),
            help_row("/undo", "Undo file changes"),
            help_row("/diff", "Show real session diffs"),
            help_row("/cost", "Token usage"),
            help_row("/export [path]", "Export chat to file"),
            help_row("/reinstall", "Reinstall binary"),
            help_row("/exit, /quit", "Exit HAKARI"),
            help_row("@filename", "Mention file (autocomplete)"),
            Line::default(),
            Line::from(Span::styled(
                "  Press Esc to close",
                Style::default().fg(Theme::text_muted()),
            )),
        ];

        let paragraph = Paragraph::new(help_text).block(block);
        frame.render_widget(paragraph, popup_area);
    }

    fn render_escalation(&self, frame: &mut Frame, area: Rect, summary: &str) {
        let popup_area = centered_rect(70, 60, area);
        frame.render_widget(Clear, popup_area);
        let block = Block::default()
            .title(" Agent Needs Help ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Theme::red()))
            .style(Style::default().bg(Theme::surface()));

        let text = format!("{}\n\n  [Enter] Provide guidance   [Esc] Dismiss", summary);
        let paragraph = Paragraph::new(text)
            .block(block)
            .style(Style::default().fg(Theme::text()))
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, popup_area);
    }

    #[allow(clippy::too_many_arguments)]
    fn render_model_selector(
        &self,
        frame: &mut Frame,
        area: Rect,
        models: &[ModelEntry],
        selected: usize,
        loading: bool,
        error: Option<&str>,
        scroll_offset: usize,
        target: &ModelTarget,
        animation_frame: u64,
    ) {
        let popup_area = centered_rect(80, 80, area);
        frame.render_widget(Clear, popup_area);

        let title = match target {
            ModelTarget::Nano => " Select Nano Model ",
            ModelTarget::Shizuka => " Select Shizuka Model ",
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Theme::border_focus()))
            .style(Style::default().bg(Theme::surface()));

        if loading {
            let dots = ".".repeat(((animation_frame / 3) % 4) as usize);
            let paragraph = Paragraph::new(format!("\n  Syncing Copilot catalog{}\n\n  Pulling live models, usage metadata, and pricing registry.", dots))
                .block(block)
                .style(Style::default().fg(Theme::text_dim()));
            frame.render_widget(paragraph, popup_area);
            return;
        }

        if let Some(error) = error {
            let paragraph = Paragraph::new(format!(
                "\n  Unable to load models.\n\n  {}\n\n  Press Esc to close.",
                error
            ))
            .block(block)
            .style(Style::default().fg(Theme::red()))
            .wrap(Wrap { trim: false });
            frame.render_widget(paragraph, popup_area);
            return;
        }

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let header_height: usize = 4;
        let visible_height = (inner.height as usize).saturating_sub(header_height);

        // Clamp scroll_offset so selected is always in view
        let scroll = if selected < scroll_offset {
            selected
        } else if selected >= scroll_offset + visible_height {
            selected.saturating_sub(visible_height) + 1
        } else {
            scroll_offset
        };

        let mut lines = Vec::new();
        lines.push(Line::default());
        lines.push(Line::from(vec![
            Span::styled("  j/k", Style::default().fg(Theme::text_dim())),
            Span::styled(" navigate  ", Style::default().fg(Theme::text_muted())),
            Span::styled("enter", Style::default().fg(Theme::text_dim())),
            Span::styled(" select  ", Style::default().fg(Theme::text_muted())),
            Span::styled("esc", Style::default().fg(Theme::text_dim())),
            Span::styled(" cancel", Style::default().fg(Theme::text_muted())),
        ]));
        lines.push(Line::default());

        // Table header
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {:<28}", "Model"),
                Style::default().fg(Theme::text_muted()),
            ),
            Span::styled(
                format!("{:>8}", "Rate"),
                Style::default().fg(Theme::text_muted()),
            ),
            Span::styled(
                format!(" {:<14}", "Release"),
                Style::default().fg(Theme::text_muted()),
            ),
            Span::styled(
                format!("{:>8}", "Ctx"),
                Style::default().fg(Theme::text_muted()),
            ),
            Span::styled("  Flags", Style::default().fg(Theme::text_muted())),
        ]));

        if models.is_empty() {
            lines.push(Line::default());
            lines.push(Line::from(Span::styled(
                "  No models available. Check Copilot auth and connectivity.",
                Style::default().fg(Theme::text_muted()),
            )));
        }

        let end = (scroll + visible_height).min(models.len());
        for (i, model) in models.iter().enumerate().take(end).skip(scroll) {
            let is_sel = i == selected;
            let bg = if is_sel {
                Theme::surface_bright()
            } else {
                Theme::surface()
            };

            let marker = if model.active { "\u{25cf} " } else { "  " };
            let marker_color = if model.active {
                Theme::green()
            } else {
                Theme::text_muted()
            };

            let name_style = if is_sel {
                Style::default()
                    .fg(Theme::text_bright())
                    .bg(bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Theme::text()).bg(bg)
            };

            let ctx_str = if model.context > 0 {
                format!("{}k", model.context / 1000)
            } else {
                "-".to_string()
            };

            let paid_rate = model
                .premium_multiplier_paid_display
                .clone()
                .unwrap_or_else(|| "n/a".to_string());
            let rate_color = if model.included_in_paid {
                Theme::green()
            } else if paid_rate == "n/a" {
                Theme::text_muted()
            } else {
                Theme::yellow()
            };

            let mut tags = Vec::new();
            if model.reasoning {
                tags.push("reason");
            }
            if model.included_in_paid {
                tags.push("included");
            }
            if !model.category.is_empty() {
                tags.push(&model.category);
            }
            if let Some(provider) = &model.provider {
                tags.push(provider);
            }
            let tag_str = if tags.is_empty() {
                String::new()
            } else {
                format!("  {}", tags.join(" "))
            };
            let tag_color = match model.category.as_str() {
                "Max" => Theme::red(),
                "High" => Theme::peach(),
                "Medium" => Theme::blue(),
                "Light" => Theme::green(),
                _ => Theme::text_muted(),
            };

            let release = model
                .release_status
                .clone()
                .unwrap_or_else(|| "-".to_string());

            lines.push(Line::from(vec![
                Span::styled(marker.to_string(), Style::default().fg(marker_color).bg(bg)),
                Span::styled(format!("{:<26}", model.name), name_style),
                Span::styled(
                    format!("{:>8}", paid_rate),
                    Style::default().fg(rate_color).bg(bg),
                ),
                Span::styled(
                    format!(" {:<14}", release),
                    Style::default().fg(Theme::text_dim()).bg(bg),
                ),
                Span::styled(
                    format!("{:>8}", ctx_str),
                    Style::default().fg(Theme::text_dim()).bg(bg),
                ),
                Span::styled(tag_str, Style::default().fg(tag_color).bg(bg)),
            ]));
        }

        if scroll > 0 {
            lines.insert(
                header_height,
                Line::from(Span::styled(
                    "  \u{2191} more above",
                    Style::default().fg(Theme::text_muted()),
                )),
            );
        }
        if end < models.len() {
            lines.push(Line::from(Span::styled(
                format!("  \u{2193} {} more below", models.len() - end),
                Style::default().fg(Theme::text_muted()),
            )));
        }

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);
    }

    fn render_settings(
        &self,
        frame: &mut Frame,
        area: Rect,
        entries: &[SettingEntry],
        selected: usize,
        editing: Option<&(usize, String)>,
    ) {
        let popup_area = centered_rect(65, 60, area);
        frame.render_widget(Clear, popup_area);
        let block = Block::default()
            .title(" Settings ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Theme::border_focus()))
            .style(Style::default().bg(Theme::surface()));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let mut lines = Vec::new();
        lines.push(Line::default());
        let hint = if editing.is_some() {
            "  Enter confirm  Esc cancel"
        } else {
            "  j/k navigate  Enter edit  Esc close"
        };
        lines.push(Line::from(Span::styled(
            hint,
            Style::default().fg(Theme::text_muted()),
        )));
        lines.push(Line::default());

        for (i, entry) in entries.iter().enumerate() {
            let is_sel = i == selected;
            let is_editing = editing.map(|(idx, _)| *idx == i).unwrap_or(false);
            let bg = if is_sel {
                Theme::surface_bright()
            } else {
                Theme::surface()
            };
            let label_style = Style::default().fg(Theme::text_dim()).bg(bg);

            let value_span = if is_editing {
                let buf = editing.map(|(_, b)| b.as_str()).unwrap_or("");
                Span::styled(
                    format!("{}|", buf),
                    Style::default()
                        .fg(Theme::text_bright())
                        .bg(bg)
                        .add_modifier(Modifier::BOLD),
                )
            } else if entry.editable {
                Span::styled(
                    entry.value.clone(),
                    Style::default().fg(Theme::text()).bg(bg),
                )
            } else {
                Span::styled(
                    entry.value.clone(),
                    Style::default().fg(Theme::text_muted()).bg(bg),
                )
            };

            let marker = if is_editing {
                Span::styled("  \u{25b8} ", Style::default().fg(Theme::yellow()).bg(bg))
            } else if is_sel {
                Span::styled("  \u{25b8} ", Style::default().fg(Theme::mauve()).bg(bg))
            } else {
                Span::styled("    ", Style::default().bg(bg))
            };

            lines.push(Line::from(vec![
                marker,
                Span::styled(format!("{:<24}", entry.label), label_style),
                value_span,
            ]));
        }

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);
    }

    fn render_connect_flow(&self, frame: &mut Frame, area: Rect, state: &ConnectState) {
        let popup_area = centered_rect(60, 40, area);
        frame.render_widget(Clear, popup_area);
        let block = Block::default()
            .title(" Connect GitHub Copilot ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Theme::green()))
            .style(Style::default().bg(Theme::surface()));

        let text = match state {
            ConnectState::Starting => "  Initiating device flow...".to_string(),
            ConnectState::WaitingForAuth { uri, code } => {
                format!(
                    "\n  1. Open: {}\n\n  2. Enter code:\n\n       {}\n\n  Waiting for authorization...\n\n  Press Esc to cancel",
                    uri, code,
                )
            }
            ConnectState::Polling => "  Checking authorization...".to_string(),
            ConnectState::Success => {
                "\n  Authentication successful!\n\n  Token saved. Press Esc to close.".to_string()
            }
            ConnectState::Error(e) => {
                format!("\n  Authentication failed: {}\n\n  Press Esc to close.", e)
            }
        };

        let paragraph = Paragraph::new(text)
            .block(block)
            .style(Style::default().fg(Theme::text()))
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, popup_area);
    }

    fn render_connect_menu(&self, frame: &mut Frame, area: Rect, selected: usize) {
        let popup_area = centered_rect(55, 30, area);
        frame.render_widget(Clear, popup_area);
        let block = Block::default()
            .title(" Connect Provider ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Theme::border_focus()))
            .style(Style::default().bg(Theme::surface()));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let options = [
            ("GitHub Copilot", "Authenticate via device flow"),
            ("OpenAI Compatible", "Set API key env variable"),
        ];

        let mut lines = Vec::new();
        lines.push(Line::default());

        for (i, (name, desc)) in options.iter().enumerate() {
            let is_sel = i == selected;
            let bg = if is_sel {
                Theme::surface_bright()
            } else {
                Theme::surface()
            };

            lines.push(Line::from(vec![
                Span::styled(
                    if is_sel { "  \u{25b8} " } else { "    " },
                    Style::default().fg(Theme::mauve()).bg(bg),
                ),
                Span::styled(
                    name.to_string(),
                    Style::default()
                        .fg(if is_sel {
                            Theme::text_bright()
                        } else {
                            Theme::text()
                        })
                        .bg(bg)
                        .add_modifier(if is_sel {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("      ", Style::default().bg(bg)),
                Span::styled(
                    desc.to_string(),
                    Style::default().fg(Theme::text_muted()).bg(bg),
                ),
            ]));
            lines.push(Line::default());
        }

        lines.push(Line::from(Span::styled(
            "  Esc to cancel",
            Style::default().fg(Theme::text_muted()),
        )));

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);
    }

    fn render_model_list(
        &self,
        frame: &mut Frame,
        area: Rect,
        entries: &[ModelListDisplay],
        selected: usize,
    ) {
        let popup_area = centered_rect(70, 60, area);
        frame.render_widget(Clear, popup_area);
        let block = Block::default()
            .title(" Model Assignments ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Theme::border_focus()))
            .style(Style::default().bg(Theme::surface()));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let mut lines = Vec::new();
        lines.push(Line::default());
        lines.push(Line::from(vec![
            Span::styled("  j/k", Style::default().fg(Theme::text_dim())),
            Span::styled(" navigate  ", Style::default().fg(Theme::text_muted())),
            Span::styled("esc", Style::default().fg(Theme::text_dim())),
            Span::styled(" close", Style::default().fg(Theme::text_muted())),
        ]));
        lines.push(Line::default());

        // Table header
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {:<16}", "Demand"),
                Style::default().fg(Theme::text_muted()),
            ),
            Span::styled(
                format!("{:<24}", "Model"),
                Style::default().fg(Theme::text_muted()),
            ),
            Span::styled(
                format!("{:<8}", "Rate"),
                Style::default().fg(Theme::text_muted()),
            ),
            Span::styled(
                format!("{:<10}", "Category"),
                Style::default().fg(Theme::text_muted()),
            ),
            Span::styled("Reasoning", Style::default().fg(Theme::text_muted())),
        ]));

        if entries.is_empty() {
            lines.push(Line::default());
            lines.push(Line::from(Span::styled(
                "  No model assignments configured.",
                Style::default().fg(Theme::text_muted()),
            )));
        } else {
            for (i, entry) in entries.iter().enumerate() {
                let is_sel = i == selected;
                let bg = if is_sel {
                    Theme::surface_bright()
                } else {
                    Theme::surface()
                };
                let style = if is_sel {
                    Style::default().fg(Theme::text_bright()).bg(bg)
                } else {
                    Style::default().fg(Theme::text()).bg(bg)
                };

                let cat_color = match entry.category.as_str() {
                    "Max" => Theme::red(),
                    "High" => Theme::peach(),
                    "Medium" => Theme::blue(),
                    "Light" => Theme::green(),
                    _ => Theme::text_muted(),
                };

                lines.push(Line::from(vec![
                    Span::styled(
                        if is_sel { "\u{25b8} " } else { "  " },
                        Style::default().fg(Theme::mauve()).bg(bg),
                    ),
                    Span::styled(format!("{:<16}", entry.demand), style),
                    Span::styled(format!("{:<24}", entry.model_id), style),
                    Span::styled(
                        format!("{:<8}", entry.rate),
                        Style::default().fg(Theme::yellow()).bg(bg),
                    ),
                    Span::styled(
                        format!("{:<10}", entry.category),
                        Style::default().fg(cat_color).bg(bg),
                    ),
                    Span::styled(
                        entry.reasoning.clone(),
                        Style::default().fg(Theme::cyan()).bg(bg),
                    ),
                ]));
            }
        }

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);
    }

    fn render_reasoning_selector(
        &self,
        frame: &mut Frame,
        area: Rect,
        levels: &[String],
        selected: usize,
    ) {
        let popup_area = centered_rect(40, 35, area);
        frame.render_widget(Clear, popup_area);
        let block = Block::default()
            .title(" Reasoning Level ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Theme::border_focus()))
            .style(Style::default().bg(Theme::surface()));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let mut lines = Vec::new();
        lines.push(Line::default());

        for (i, level) in levels.iter().enumerate() {
            let is_sel = i == selected;
            let bg = if is_sel {
                Theme::surface_bright()
            } else {
                Theme::surface()
            };

            lines.push(Line::from(vec![
                Span::styled(
                    if is_sel { "  \u{25b8} " } else { "    " },
                    Style::default().fg(Theme::mauve()).bg(bg),
                ),
                Span::styled(
                    level.clone(),
                    Style::default()
                        .fg(if is_sel {
                            Theme::text_bright()
                        } else {
                            Theme::text()
                        })
                        .bg(bg)
                        .add_modifier(if is_sel {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
            ]));
        }

        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "  Enter select, Esc cancel",
            Style::default().fg(Theme::text_muted()),
        )));

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);
    }
}

fn help_row<'a>(key: &'a str, desc: &'a str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {:<26}", key),
            Style::default().fg(Theme::text_dim()),
        ),
        Span::styled(desc.to_string(), Style::default().fg(Theme::text_muted())),
    ])
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}
