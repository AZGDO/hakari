use super::{ToolResult, ToolResultMetadata};
use crate::memory::kms::Kms;
use crate::memory::kpms::Kpms;
use similar::{ChangeTag, TextDiff};
use std::path::{Path, PathBuf};

pub fn execute_write(
    project_dir: &Path,
    path: &str,
    content: &str,
    kms: &mut Kms,
    kpms: &Kpms,
) -> ToolResult {
    let full_path = resolve_path(project_dir, path);

    if !full_path.starts_with(project_dir) {
        return ToolResult {
            success: false,
            output: format!("Error: path is outside project directory: {}", path),
            metadata: ToolResultMetadata::default(),
        };
    }

    // Syntax validation for known file types
    let ext = full_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if let Some(syntax_error) = check_syntax(ext, content) {
        return ToolResult {
            success: false,
            output: format!("Write blocked — syntax error: {}", syntax_error),
            metadata: ToolResultMetadata {
                file_path: Some(path.to_string()),
                ..Default::default()
            },
        };
    }

    // Read original content for diff
    let original = std::fs::read_to_string(&full_path).ok();

    // Create parent directories
    if let Some(parent) = full_path.parent() {
        if !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return ToolResult {
                    success: false,
                    output: format!("Error creating directories: {}", e),
                    metadata: ToolResultMetadata::default(),
                };
            }
        }
    }

    // Write file
    if let Err(e) = std::fs::write(&full_path, content) {
        return ToolResult {
            success: false,
            output: format!("Error writing file: {}", e),
            metadata: ToolResultMetadata::default(),
        };
    }

    // Record in KMS
    kms.record_file_write(path, original.clone());

    // Generate diff summary
    let diff_summary = if let Some(ref orig) = original {
        generate_diff_summary(orig, content)
    } else {
        format!("New file created ({} lines)", content.lines().count())
    };

    // Run lint checks
    let lint_warnings = run_lint_check(project_dir, &full_path, kpms);

    // Detect related tests
    let related_tests = detect_related_tests(project_dir, path);

    let line_count = content.lines().count();
    let mut output = format!("✓ Written: {} ({} lines)\n", path, line_count);
    output.push_str(&format!("  {}\n", diff_summary));

    if !lint_warnings.is_empty() {
        output.push_str(&format!(
            "  +{} lint warning(s) (non-blocking):\n",
            lint_warnings.len()
        ));
        for w in &lint_warnings {
            output.push_str(&format!("    {}\n", w));
        }
    }

    if !related_tests.is_empty() {
        output.push_str(&format!(
            "  Related tests: {}\n",
            related_tests.join(", ")
        ));
    }

    ToolResult {
        success: true,
        output,
        metadata: ToolResultMetadata {
            file_path: Some(path.to_string()),
            lines_changed: Some(diff_summary),
            lint_warnings,
            related_tests,
            ..Default::default()
        },
    }
}

fn resolve_path(project_dir: &Path, path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        project_dir.join(p)
    }
}

fn check_syntax(ext: &str, content: &str) -> Option<String> {
    match ext {
        "json" => {
            if let Err(e) = serde_json::from_str::<serde_json::Value>(content) {
                Some(format!("Invalid JSON: {}", e))
            } else {
                None
            }
        }
        "toml" => {
            // Basic TOML validation: check for obvious issues
            if content.contains("= =") || content.contains("[[") && !content.contains("]]") {
                Some("Possible TOML syntax error detected".to_string())
            } else {
                None
            }
        }
        _ => None, // For other types, we rely on linting
    }
}

fn generate_diff_summary(original: &str, new: &str) -> String {
    let diff = TextDiff::from_lines(original, new);
    let mut added = 0;
    let mut removed = 0;
    let mut changed_ranges = Vec::new();
    let mut current_start = None;
    let mut current_end = 0;

    for (idx, change) in diff.iter_all_changes().enumerate() {
        match change.tag() {
            ChangeTag::Insert => {
                added += 1;
                if current_start.is_none() {
                    current_start = Some(idx + 1);
                }
                current_end = idx + 1;
            }
            ChangeTag::Delete => {
                removed += 1;
                if current_start.is_none() {
                    current_start = Some(idx + 1);
                }
                current_end = idx + 1;
            }
            ChangeTag::Equal => {
                if let Some(start) = current_start.take() {
                    changed_ranges.push(format!("lines {}-{}", start, current_end));
                }
            }
        }
    }
    if let Some(start) = current_start {
        changed_ranges.push(format!("lines {}-{}", start, current_end));
    }

    let range_str = if changed_ranges.is_empty() {
        "no changes".to_string()
    } else if changed_ranges.len() > 3 {
        format!("{} regions changed", changed_ranges.len())
    } else {
        format!("Changed: {}", changed_ranges.join(", "))
    };

    format!("{} (+{} -{} lines)", range_str, added, removed)
}

fn run_lint_check(_project_dir: &Path, _file_path: &Path, kpms: &Kpms) -> Vec<String> {
    // We attempt a quick lint only if we know the lint command
    if kpms.project.lint_command.is_empty() {
        return Vec::new();
    }

    // For now, we don't block on lint — the validation engine handles this asynchronously
    // This is a placeholder for the fast-path lint integration
    Vec::new()
}

fn detect_related_tests(project_dir: &Path, file_path: &str) -> Vec<String> {
    let path = Path::new(file_path);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let parent = path.parent().unwrap_or(Path::new(""));

    let mut tests = Vec::new();

    // Check for .test.{ext} and .spec.{ext} patterns
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let test_patterns = [
        parent.join(format!("{}.test.{}", stem, ext)),
        parent.join(format!("{}.spec.{}", stem, ext)),
        parent.join("__tests__").join(format!("{}.test.{}", stem, ext)),
        parent.join("__tests__").join(format!("{}.spec.{}", stem, ext)),
        parent.join("tests").join(format!("{}.test.{}", stem, ext)),
    ];

    for pattern in &test_patterns {
        let full_path = project_dir.join(pattern);
        if full_path.exists() {
            tests.push(pattern.to_string_lossy().to_string());
        }
    }

    tests
}
