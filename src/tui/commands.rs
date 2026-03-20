use crate::project::file_tree;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SlashCommand {
    pub name: &'static str,
    pub description: &'static str,
    pub args: &'static str,
}

pub const COMMANDS: &[SlashCommand] = &[
    SlashCommand { name: "/model", description: "Select nano AI model", args: "[model-name]" },
    SlashCommand { name: "/shizuka", description: "Select shizuka model", args: "[model-name]" },
    SlashCommand { name: "/reasoning", description: "Set reasoning level", args: "[level]" },
    SlashCommand { name: "/models", description: "List available models", args: "" },
    SlashCommand { name: "/modellist", description: "Show model assignments", args: "" },
    SlashCommand { name: "/connect", description: "Connect to provider", args: "" },
    SlashCommand { name: "/settings", description: "Open settings", args: "" },
    SlashCommand { name: "/clear", description: "Clear chat", args: "" },
    SlashCommand { name: "/compact", description: "Collapse traces", args: "" },
    SlashCommand { name: "/help", description: "Show help", args: "" },
    SlashCommand { name: "/status", description: "Session status", args: "" },
    SlashCommand { name: "/reset", description: "Reset session", args: "" },
    SlashCommand { name: "/undo", description: "Undo file changes", args: "" },
    SlashCommand { name: "/diff", description: "Show changes", args: "" },
    SlashCommand { name: "/export", description: "Export chat", args: "[path]" },
    SlashCommand { name: "/pin", description: "Pin file to context", args: "<file>" },
    SlashCommand { name: "/unpin", description: "Unpin file", args: "<file>" },
    SlashCommand { name: "/files", description: "List pinned files", args: "" },
    SlashCommand { name: "/cost", description: "Token usage", args: "" },
    SlashCommand { name: "/reinstall", description: "Reinstall binary", args: "" },
    SlashCommand { name: "/exit", description: "Exit HAKARI", args: "" },
    SlashCommand { name: "/quit", description: "Exit HAKARI", args: "" },
];

pub fn match_commands(input: &str) -> Vec<&'static SlashCommand> {
    if !input.starts_with('/') {
        return Vec::new();
    }
    let query = input.to_lowercase();
    COMMANDS.iter().filter(|cmd| cmd.name.starts_with(&query)).collect()
}

pub fn is_command(input: &str) -> bool {
    let trimmed = input.trim();
    COMMANDS.iter().any(|cmd| {
        trimmed == cmd.name || trimmed.starts_with(&format!("{} ", cmd.name))
    })
}

pub fn parse_command(input: &str) -> Option<(&str, &str)> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
    let cmd = parts[0];
    let args = if parts.len() > 1 { parts[1].trim() } else { "" };
    Some((cmd, args))
}

pub fn match_files(query: &str, project_dir: &Path) -> Vec<String> {
    if query.is_empty() {
        return Vec::new();
    }
    let entries = file_tree::build_file_tree(project_dir, 1000);
    let query_lower = query.to_lowercase();
    entries
        .iter()
        .filter(|e| !e.is_dir)
        .filter(|e| e.path.to_lowercase().contains(&query_lower))
        .take(12)
        .map(|e| e.path.clone())
        .collect()
}

pub fn extract_at_mentions(input: &str) -> Vec<String> {
    let mut mentions = Vec::new();
    let mut i = 0;
    let chars: Vec<char> = input.chars().collect();
    while i < chars.len() {
        if chars[i] == '@' {
            let start = i + 1;
            let mut end = start;
            while end < chars.len() && !chars[end].is_whitespace() {
                end += 1;
            }
            if end > start {
                mentions.push(chars[start..end].iter().collect::<String>());
            }
            i = end;
        } else {
            i += 1;
        }
    }
    mentions
}

pub fn get_current_at_query(input: &str, cursor_pos: usize) -> Option<String> {
    let before = if cursor_pos <= input.len() { &input[..cursor_pos] } else { input };
    if let Some(at_pos) = before.rfind('@') {
        let after_at = &before[at_pos + 1..];
        if !after_at.contains(' ') {
            return Some(after_at.to_string());
        }
    }
    None
}
