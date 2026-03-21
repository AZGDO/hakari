use super::{ToolResult, ToolResultMetadata};
use crate::memory::kpms::Kpms;
use crate::project::file_tree;
use std::path::Path;
use std::process::Command;

pub fn execute_grep(
    project_dir: &Path,
    query: &str,
    file_glob: Option<&str>,
    context_lines: Option<usize>,
    max_results: Option<usize>,
    kpms: &Kpms,
) -> ToolResult {
    let query = query.trim();
    if query.is_empty() {
        return ToolResult {
            success: false,
            output: "Error: query cannot be empty".to_string(),
            metadata: ToolResultMetadata::default(),
        };
    }

    let max_results = max_results.unwrap_or(40).clamp(1, 200);
    let context_lines = context_lines.unwrap_or(2).min(6);

    let mut command = Command::new("rg");
    command
        .current_dir(project_dir)
        .arg("--line-number")
        .arg("--with-filename")
        .arg("--color=never")
        .arg("--smart-case")
        .arg("--max-count")
        .arg(max_results.to_string())
        .arg("--context")
        .arg(context_lines.to_string());

    if let Some(file_glob) = file_glob.filter(|glob| !glob.trim().is_empty()) {
        command.arg("--glob").arg(file_glob);
    }

    if is_literal_query(query) {
        command.arg("--fixed-strings");
    }

    command.arg(query).arg(".");

    let output = match command.output() {
        Ok(output) => output,
        Err(error) => {
            return ToolResult {
                success: false,
                output: format!("Failed to run rg: {}", error),
                metadata: ToolResultMetadata::default(),
            };
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    if exit_code == 0 {
        let output = format_match_output(query, file_glob, &stdout, max_results, kpms);
        return ToolResult {
            success: true,
            output,
            metadata: ToolResultMetadata {
                exit_code: Some(exit_code),
                ..Default::default()
            },
        };
    }

    if exit_code == 1 {
        let suggestions = suggest_files(project_dir, query, file_glob, kpms);
        let mut output = format!("No matches for `{}`", query);
        if let Some(file_glob) = file_glob {
            output.push_str(&format!(" in `{}`", file_glob));
        }
        output.push('.');
        if !suggestions.is_empty() {
            output.push_str("\n\nClosest file candidates:\n");
            for suggestion in suggestions {
                output.push_str(&format!("  - {}\n", suggestion));
            }
        }
        return ToolResult {
            success: true,
            output,
            metadata: ToolResultMetadata {
                exit_code: Some(exit_code),
                ..Default::default()
            },
        };
    }

    ToolResult {
        success: false,
        output: format!(
            "rg failed (exit {}): {}",
            exit_code,
            if stderr.trim().is_empty() {
                stdout.trim()
            } else {
                stderr.trim()
            }
        ),
        metadata: ToolResultMetadata {
            exit_code: Some(exit_code),
            ..Default::default()
        },
    }
}

fn format_match_output(
    query: &str,
    file_glob: Option<&str>,
    stdout: &str,
    max_results: usize,
    kpms: &Kpms,
) -> String {
    let mut output = format!("Matches for `{}`", query);
    if let Some(file_glob) = file_glob {
        output.push_str(&format!(" in `{}`", file_glob));
    }
    output.push_str(":\n\n");

    let mut match_count = 0usize;
    for line in stdout.lines() {
        if !line.starts_with("--") && line.contains(':') {
            match_count += 1;
        }
        output.push_str(line);
        output.push('\n');
    }

    output.push_str(&format!(
        "\n{} match line(s) shown",
        match_count.min(max_results)
    ));

    let hints = relevant_hints(stdout, kpms);
    if !hints.is_empty() {
        output.push_str("\n\nRelated project hints:\n");
        for hint in hints {
            output.push_str(&format!("  - {}\n", hint));
        }
    }

    output
}

fn suggest_files(
    project_dir: &Path,
    query: &str,
    file_glob: Option<&str>,
    kpms: &Kpms,
) -> Vec<String> {
    let query_tokens = query_tokens(query);
    let mut candidates: Vec<(i32, String)> = Vec::new();

    for entry in file_tree::build_file_tree(project_dir, 1200) {
        if entry.is_dir {
            continue;
        }
        if let Some(glob) = file_glob {
            let ext = glob.trim_start_matches("*.");
            if !glob.is_empty() && glob.starts_with("*.") && !entry.path.ends_with(ext) {
                continue;
            }
        }

        let lower_path = entry.path.to_lowercase();
        let mut score = 0;
        for token in &query_tokens {
            if lower_path.contains(token) {
                score += 3;
            }
            if kpms
                .file_index
                .get(&entry.path)
                .map(|desc| desc.to_lowercase().contains(token))
                .unwrap_or(false)
            {
                score += 2;
            }
        }
        if score > 0 {
            candidates.push((score, entry.path));
        }
    }

    candidates.sort_by(|a, b| b.cmp(a));
    candidates
        .into_iter()
        .take(8)
        .map(|(_, path)| path)
        .collect()
}

fn relevant_hints(stdout: &str, kpms: &Kpms) -> Vec<String> {
    let mut hints = Vec::new();
    for line in stdout.lines().take(20) {
        let path = line.split(':').next().unwrap_or_default();
        if let Some(description) = kpms.file_index.get(path) {
            hints.push(format!("{} — {}", path, description));
        }
    }
    hints.sort();
    hints.dedup();
    hints.into_iter().take(4).collect()
}

fn query_tokens(query: &str) -> Vec<String> {
    query
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-')
        .map(|token| token.trim().to_lowercase())
        .filter(|token| token.len() >= 3)
        .collect()
}

fn is_literal_query(query: &str) -> bool {
    !query.chars().any(|ch| {
        matches!(
            ch,
            '.' | '*' | '+' | '?' | '[' | ']' | '(' | ')' | '{' | '}' | '|' | '^' | '$'
        )
    })
}
