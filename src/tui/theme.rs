use ratatui::prelude::*;
use ratatui::style::{Color, Modifier, Style};

pub struct Theme;

impl Theme {
    // Background
    pub fn bg() -> Color {
        Color::Rgb(17, 17, 27)
    }
    pub fn surface() -> Color {
        Color::Rgb(24, 24, 37)
    }
    pub fn surface_bright() -> Color {
        Color::Rgb(35, 35, 50)
    }
    pub fn surface_elevated() -> Color {
        Color::Rgb(42, 42, 58)
    }

    // Text
    pub fn text() -> Color {
        Color::Rgb(205, 214, 244)
    }
    pub fn text_dim() -> Color {
        Color::Rgb(108, 112, 134)
    }
    pub fn text_bright() -> Color {
        Color::Rgb(245, 245, 255)
    }
    pub fn text_muted() -> Color {
        Color::Rgb(80, 84, 104)
    }

    // Accent colors
    pub fn blue() -> Color {
        Color::Rgb(137, 180, 250)
    }
    pub fn green() -> Color {
        Color::Rgb(166, 227, 161)
    }
    pub fn red() -> Color {
        Color::Rgb(243, 139, 168)
    }
    pub fn yellow() -> Color {
        Color::Rgb(249, 226, 175)
    }
    pub fn cyan() -> Color {
        Color::Rgb(148, 226, 213)
    }
    pub fn mauve() -> Color {
        Color::Rgb(203, 166, 247)
    }
    pub fn peach() -> Color {
        Color::Rgb(250, 179, 135)
    }
    pub fn teal() -> Color {
        Color::Rgb(148, 226, 213)
    }
    pub fn pink() -> Color {
        Color::Rgb(245, 194, 231)
    }
    pub fn lavender() -> Color {
        Color::Rgb(180, 190, 254)
    }
    pub fn sapphire() -> Color {
        Color::Rgb(116, 199, 236)
    }

    // Borders
    pub fn border() -> Color {
        Color::Rgb(49, 50, 68)
    }
    pub fn border_focus() -> Color {
        Color::Rgb(88, 91, 112)
    }
    pub fn border_accent() -> Color {
        Self::mauve()
    }

    // Semantic styles
    pub fn user_message() -> Style {
        Style::default().fg(Self::text_bright())
    }

    pub fn nano_message() -> Style {
        Style::default().fg(Self::text())
    }

    pub fn shizuka_message() -> Style {
        Style::default()
            .fg(Self::cyan())
            .add_modifier(Modifier::DIM)
    }

    pub fn tool_header() -> Style {
        Style::default().fg(Self::text_muted())
    }

    pub fn tool_success() -> Style {
        Style::default().fg(Self::green())
    }

    pub fn tool_error() -> Style {
        Style::default().fg(Self::red())
    }

    pub fn warning() -> Style {
        Style::default().fg(Self::yellow())
    }

    pub fn error() -> Style {
        Style::default()
            .fg(Self::red())
            .add_modifier(Modifier::BOLD)
    }

    pub fn status_bar() -> Style {
        Style::default().bg(Self::surface()).fg(Self::text_dim())
    }

    pub fn header() -> Style {
        Style::default().bg(Self::surface()).fg(Self::text())
    }

    pub fn input_border() -> Style {
        Style::default().fg(Self::border_focus())
    }

    pub fn input_text() -> Style {
        Style::default().fg(Self::text_bright())
    }

    pub fn selected() -> Style {
        Style::default()
            .bg(Self::surface_bright())
            .fg(Self::text_bright())
    }

    pub fn spinner() -> Style {
        Style::default().fg(Self::mauve())
    }

    pub fn label() -> Style {
        Style::default().fg(Self::text_dim())
    }

    pub fn badge_trivial() -> Style {
        Style::default().fg(Self::green())
    }

    pub fn badge_small() -> Style {
        Style::default().fg(Self::blue())
    }

    pub fn badge_medium() -> Style {
        Style::default().fg(Self::yellow())
    }

    pub fn badge_large() -> Style {
        Style::default().fg(Self::red())
    }

    pub fn accent() -> Style {
        Style::default().fg(Self::mauve())
    }

    pub fn block_default() -> ratatui::widgets::Block<'static> {
        ratatui::widgets::Block::default().border_style(Style::default().fg(Self::border()))
    }
}
