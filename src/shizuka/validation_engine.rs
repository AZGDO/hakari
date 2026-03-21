use crate::memory::kpms::Kpms;
use std::path::Path;

pub struct ValidationEngine;

impl ValidationEngine {
    pub fn validate_write(file_path: &Path, content: &str) -> Vec<String> {
        let mut issues = Vec::new();

        // Step 1: Parse check based on file extension
        let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if let Some(parse_error) = Self::parse_check(ext, content) {
            issues.push(format!("Parse error: {}", parse_error));
        }

        issues
    }

    fn parse_check(ext: &str, content: &str) -> Option<String> {
        match ext {
            "json" => serde_json::from_str::<serde_json::Value>(content)
                .err()
                .map(|e| format!("Invalid JSON at line {}: {}", e.line(), e)),
            "rs" => Self::basic_bracket_check(content, '{', '}'),
            "ts" | "tsx" | "js" | "jsx" => Self::basic_bracket_check(content, '{', '}'),
            "py" => {
                // Basic Python syntax: check for obvious indent issues
                let mut prev_indent = 0;
                for (i, line) in content.lines().enumerate() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let indent = line.len() - line.trim_start().len();
                    if indent > prev_indent + 8 {
                        return Some(format!(
                            "Suspicious indent jump at line {} (from {} to {} spaces)",
                            i + 1,
                            prev_indent,
                            indent
                        ));
                    }
                    prev_indent = indent;
                }
                None
            }
            _ => None,
        }
    }

    fn basic_bracket_check(content: &str, open: char, close: char) -> Option<String> {
        let mut depth: i32 = 0;
        let mut in_string = false;
        let mut escape_next = false;
        let mut string_char = '"';
        let mut line_num: usize = 1;

        for ch in content.chars() {
            if ch == '\n' {
                line_num += 1;
            }
            if escape_next {
                escape_next = false;
                continue;
            }
            if ch == '\\' && in_string {
                escape_next = true;
                continue;
            }
            if !in_string && (ch == '"' || ch == '\'' || ch == '`') {
                in_string = true;
                string_char = ch;
                continue;
            }
            if in_string && ch == string_char {
                in_string = false;
                continue;
            }
            if !in_string {
                if ch == open {
                    depth += 1;
                } else if ch == close {
                    depth -= 1;
                    if depth < 0 {
                        return Some(format!("Unmatched '{}' at line {}", close, line_num));
                    }
                }
            }
        }

        if depth > 0 {
            Some(format!(
                "Unclosed '{}' — {} more '{}' needed",
                open, depth, close
            ))
        } else {
            None
        }
    }

    pub fn detect_tests(project_dir: &Path, file_path: &str) -> Vec<String> {
        let path = Path::new(file_path);
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let parent = path.parent().unwrap_or(Path::new(""));

        let mut tests = Vec::new();
        let candidates = [
            parent.join(format!("{}.test.{}", stem, ext)),
            parent.join(format!("{}.spec.{}", stem, ext)),
            parent.join(format!("{}_test.{}", stem, ext)),
            parent
                .join("__tests__")
                .join(format!("{}.test.{}", stem, ext)),
            parent.join("tests").join(format!("test_{}.{}", stem, ext)),
        ];

        for candidate in &candidates {
            let full = project_dir.join(candidate);
            if full.exists() {
                tests.push(candidate.to_string_lossy().to_string());
            }
        }

        tests
    }

    pub fn run_lint(project_dir: &Path, file_path: &Path, lint_command: &str) -> Vec<String> {
        if lint_command.is_empty() {
            return Vec::new();
        }

        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("{} {}", lint_command, file_path.display()))
            .current_dir(project_dir)
            .output();

        match output {
            Ok(output) => {
                if output.status.success() {
                    Vec::new()
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let combined = format!("{}\n{}", stdout, stderr);
                    combined
                        .lines()
                        .filter(|l| !l.trim().is_empty())
                        .take(10)
                        .map(|l| l.to_string())
                        .collect()
                }
            }
            Err(_) => Vec::new(),
        }
    }
}
