use ratatui::style::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Theme {
    pub mode: ThemeMode,
    pub text: Color,
    pub inverse_text: Color,
    pub background: Color,
    pub claude: Color,
    pub claude_shimmer: Color,
    pub permission: Color,
    pub permission_shimmer: Color,
    pub prompt_border: Color,
    pub prompt_border_shimmer: Color,
    pub inactive: Color,
    pub subtle: Color,
    pub suggestion: Color,
    pub success: Color,
    pub error: Color,
    pub warning: Color,
    pub bash_border: Color,
    pub diff_added: Color,
    pub diff_removed: Color,
    pub diff_added_word: Color,
    pub diff_removed_word: Color,
    pub plan_mode: Color,
    pub auto_accept: Color,
    pub fast_mode: Color,
    pub remember: Color,
    pub professional_blue: Color,
    pub user_message_bg: Color,
    pub bash_message_bg: Color,
    pub memory_bg: Color,
    pub rate_limit_fill: Color,
    pub rate_limit_empty: Color,
    pub brief_label_you: Color,
    pub brief_label_claude: Color,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            mode: ThemeMode::Dark,
            text: Color::Rgb(255, 255, 255),
            inverse_text: Color::Rgb(0, 0, 0),
            background: Color::Reset,
            claude: Color::Rgb(215, 119, 87),
            claude_shimmer: Color::Rgb(235, 159, 127),
            permission: Color::Rgb(177, 185, 249),
            permission_shimmer: Color::Rgb(207, 215, 255),
            prompt_border: Color::Rgb(136, 136, 136),
            prompt_border_shimmer: Color::Rgb(166, 166, 166),
            inactive: Color::Rgb(153, 153, 153),
            subtle: Color::Rgb(80, 80, 80),
            suggestion: Color::Rgb(177, 185, 249),
            success: Color::Rgb(78, 186, 101),
            error: Color::Rgb(255, 107, 128),
            warning: Color::Rgb(255, 193, 7),
            bash_border: Color::Rgb(253, 93, 177),
            diff_added: Color::Rgb(34, 92, 43),
            diff_removed: Color::Rgb(122, 41, 54),
            diff_added_word: Color::Rgb(56, 166, 96),
            diff_removed_word: Color::Rgb(179, 89, 107),
            plan_mode: Color::Rgb(72, 150, 140),
            auto_accept: Color::Rgb(175, 135, 255),
            fast_mode: Color::Rgb(255, 120, 20),
            remember: Color::Rgb(177, 185, 249),
            professional_blue: Color::Rgb(106, 155, 204),
            user_message_bg: Color::Rgb(55, 55, 55),
            bash_message_bg: Color::Rgb(65, 60, 65),
            memory_bg: Color::Rgb(55, 65, 70),
            rate_limit_fill: Color::Rgb(177, 185, 249),
            rate_limit_empty: Color::Rgb(80, 83, 112),
            brief_label_you: Color::Rgb(122, 180, 232),
            brief_label_claude: Color::Rgb(215, 119, 87),
        }
    }

    pub fn light() -> Self {
        Self {
            mode: ThemeMode::Light,
            text: Color::Rgb(0, 0, 0),
            inverse_text: Color::Rgb(255, 255, 255),
            background: Color::Reset,
            claude: Color::Rgb(215, 119, 87),
            claude_shimmer: Color::Rgb(245, 149, 117),
            permission: Color::Rgb(87, 105, 247),
            permission_shimmer: Color::Rgb(137, 155, 255),
            prompt_border: Color::Rgb(153, 153, 153),
            prompt_border_shimmer: Color::Rgb(183, 183, 183),
            inactive: Color::Rgb(102, 102, 102),
            subtle: Color::Rgb(175, 175, 175),
            suggestion: Color::Rgb(87, 105, 247),
            success: Color::Rgb(44, 122, 57),
            error: Color::Rgb(171, 43, 63),
            warning: Color::Rgb(150, 108, 30),
            bash_border: Color::Rgb(255, 0, 135),
            diff_added: Color::Rgb(105, 219, 124),
            diff_removed: Color::Rgb(255, 168, 180),
            diff_added_word: Color::Rgb(47, 157, 68),
            diff_removed_word: Color::Rgb(209, 69, 75),
            plan_mode: Color::Rgb(0, 102, 102),
            auto_accept: Color::Rgb(135, 0, 255),
            fast_mode: Color::Rgb(255, 106, 0),
            remember: Color::Rgb(0, 0, 255),
            professional_blue: Color::Rgb(106, 155, 204),
            user_message_bg: Color::Rgb(240, 240, 240),
            bash_message_bg: Color::Rgb(250, 245, 250),
            memory_bg: Color::Rgb(230, 245, 250),
            rate_limit_fill: Color::Rgb(87, 105, 247),
            rate_limit_empty: Color::Rgb(39, 47, 111),
            brief_label_you: Color::Rgb(37, 99, 235),
            brief_label_claude: Color::Rgb(215, 119, 87),
        }
    }

    pub fn toggle(&self) -> Self {
        match self.mode {
            ThemeMode::Dark => Self::light(),
            ThemeMode::Light => Self::dark(),
        }
    }
}
