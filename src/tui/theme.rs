use ratatui::prelude::*;
use ratatui::style::{Color, Modifier, Style};

pub struct Theme;

impl Theme {
    // Background
    pub fn bg() -> Color { Color::Rgb(22, 22, 30) }
    pub fn surface() -> Color { Color::Rgb(30, 30, 42) }
    pub fn surface_bright() -> Color { Color::Rgb(40, 40, 55) }

    // Text
    pub fn text() -> Color { Color::Rgb(205, 214, 244) }
    pub fn text_dim() -> Color { Color::Rgb(127, 132, 156) }
    pub fn text_bright() -> Color { Color::Rgb(245, 245, 255) }

    // Accent colors
    pub fn blue() -> Color { Color::Rgb(137, 180, 250) }
    pub fn green() -> Color { Color::Rgb(166, 227, 161) }
    pub fn red() -> Color { Color::Rgb(243, 139, 168) }
    pub fn yellow() -> Color { Color::Rgb(249, 226, 175) }
    pub fn cyan() -> Color { Color::Rgb(148, 226, 213) }
    pub fn mauve() -> Color { Color::Rgb(203, 166, 247) }
    pub fn peach() -> Color { Color::Rgb(250, 179, 135) }
    pub fn teal() -> Color { Color::Rgb(148, 226, 213) }

    // Borders
    pub fn border() -> Color { Color::Rgb(69, 71, 90) }
    pub fn border_focus() -> Color { Self::blue() }

    // Semantic styles
    pub fn user_message() -> Style {
        Style::default().fg(Self::text_bright()).add_modifier(Modifier::BOLD)
    }

    pub fn nano_message() -> Style {
        Style::default().fg(Self::green())
    }

    pub fn shizuka_message() -> Style {
        Style::default().fg(Self::cyan()).add_modifier(Modifier::DIM)
    }

    pub fn tool_header() -> Style {
        Style::default().fg(Self::yellow()).add_modifier(Modifier::BOLD)
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
        Style::default().fg(Self::red()).add_modifier(Modifier::BOLD)
    }

    pub fn status_bar() -> Style {
        Style::default().bg(Self::surface_bright()).fg(Self::text())
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
        Style::default().bg(Self::surface_bright()).fg(Self::text_bright())
    }

    pub fn spinner() -> Style {
        Style::default().fg(Self::mauve())
    }

    pub fn label() -> Style {
        Style::default().fg(Self::text_dim())
    }

    pub fn badge_trivial() -> Style {
        Style::default().fg(Self::green()).add_modifier(Modifier::BOLD)
    }

    pub fn badge_small() -> Style {
        Style::default().fg(Self::blue()).add_modifier(Modifier::BOLD)
    }

    pub fn badge_medium() -> Style {
        Style::default().fg(Self::yellow()).add_modifier(Modifier::BOLD)
    }

    pub fn badge_large() -> Style {
        Style::default().fg(Self::red()).add_modifier(Modifier::BOLD)
    }

    pub fn block_default() -> ratatui::widgets::Block<'static> {
        ratatui::widgets::Block::default()
            .border_style(Style::default().fg(Self::border()))
    }
}
