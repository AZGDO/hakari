use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::agent::context::ContextController;
use crate::memory::kms::KMS;

const EXECUTE_TIMEOUT_SECS: u64 = 120;
const EXECUTE_MAX_OUTPUT_BYTES: usize = 64_000;

const BLOCKED_COMMANDS: &[&str] = &[
    "rm -rf /",
    "rm -rf ~",
    "rm -rf /*",
    "mkfs",
    "dd if=/dev/zero",
    ":(){:|:&};:",
    "chmod -R 777 /",
    "curl|bash",
    "wget|bash",
    "curl|sh",
    "wget|sh",
];

#[derive(Clone)]
pub struct FileKnowledge {
    pub content: String,
    pub compact_summary: String,
    pub annotations: String,
}

pub struct ToolInterceptor {
    pub file_cache: HashMap<String, FileKnowledge>,
    pub project_root: PathBuf,
    pub file_backups: HashMap<String, String>,
}

pub struct ToolResult {
    pub output: String,
    pub is_error: bool,
}

struct SyntaxError {
    message: String,
    line: Option<usize>,
}

impl ToolInterceptor {
    pub fn new(project_root: &str) -> Self {
        Self {
            file_cache: HashMap::new(),
            project_root: PathBuf::from(project_root),
            file_backups: HashMap::new(),
        }
    }

    pub fn populate_from_preparation(&mut self, files: &[(String, String, String, String)]) {
        // (path, content, summary, annotations)
        for (path, content, summary, annotations) in files {
            self.file_cache.insert(
                path.clone(),
                FileKnowledge {
                    content: content.clone(),
                    compact_summary: summary.clone(),
                    annotations: annotations.clone(),
                },
            );
        }
    }

    pub fn handle_read(
        &mut self,
        path: &str,
        start_line: Option<usize>,
        end_line: Option<usize>,
        context: &mut ContextController,
        kms: &mut KMS,
        shizuka_paths: &[String],
    ) -> ToolResult {
        let full_path = self.resolve_path(path);
        let path_str = full_path.display().to_string();
        let is_range_read = start_line.is_some() || end_line.is_some();

        // For full reads: check if already in active context and unchanged
        if !is_range_read
            && context.is_file_active(&path_str)
            && !self.file_was_modified_on_disk(&path_str)
        {
            let step = context.get_file_loaded_step(&path_str).unwrap_or(0);
            return ToolResult {
                output: format!(
                    "This file is already in your context (loaded at step {}). It has not changed. Reference it directly.",
                    step
                ),
                is_error: false,
            };
        }

        // Try to get content: from cache first, then disk
        let content = if let Some(knowledge) = self.file_cache.get(&path_str) {
            knowledge.content.clone()
        } else {
            match std::fs::read_to_string(&full_path) {
                Ok(content) => {
                    if !shizuka_paths.contains(&path_str) {
                        kms.record_preparation_miss(&path_str);
                    }
                    let summary = generate_compact_summary(&content, path);
                    self.file_cache.insert(
                        path_str.clone(),
                        FileKnowledge {
                            content: content.clone(),
                            compact_summary: summary,
                            annotations: String::new(),
                        },
                    );
                    content
                }
                Err(e) => {
                    return ToolResult {
                        output: format!("Error reading file: {}", e),
                        is_error: true,
                    };
                }
            }
        };

        // Line-range read
        if is_range_read {
            let lines: Vec<&str> = content.lines().collect();
            let total = lines.len();
            let s = start_line.unwrap_or(1).saturating_sub(1).min(total);
            let e = end_line.unwrap_or(total).min(total);
            if s >= e {
                return ToolResult {
                    output: format!(
                        "Invalid range: lines {}-{} (file has {} lines)",
                        s + 1,
                        e,
                        total
                    ),
                    is_error: true,
                };
            }

            let mut output = format!(
                "Lines {}-{} of {} ({} total lines):\n",
                s + 1,
                e,
                path,
                total
            );
            for (i, line) in lines[s..e].iter().enumerate() {
                output.push_str(&format!("{:>4} | {}\n", s + i + 1, line));
            }

            // Track with reduced token cost
            let region_content = lines[s..e].join("\n");
            let summary = self
                .file_cache
                .get(&path_str)
                .map(|k| k.compact_summary.clone())
                .unwrap_or_default();
            context.track_range_read(&path_str, &region_content, &summary, s + 1, e);

            return ToolResult {
                output,
                is_error: false,
            };
        }

        // Full read
        let mut output = String::new();
        if let Some(knowledge) = self.file_cache.get(&path_str) {
            if !knowledge.annotations.is_empty() {
                output.push_str(&format!("// Note: {}\n", knowledge.annotations));
            }
        }
        output.push_str(&content);

        let summary = self
            .file_cache
            .get(&path_str)
            .map(|k| k.compact_summary.clone())
            .unwrap_or_default();
        let was_compacted = context.was_compacted(&path_str);
        let ttl = if was_compacted { 4 } else { 6 };
        context.track_file(&path_str, &content, &summary, ttl);

        ToolResult {
            output,
            is_error: false,
        }
    }

    pub fn handle_write(
        &mut self,
        path: &str,
        content: &str,
        context: &mut ContextController,
        kms: &mut KMS,
    ) -> ToolResult {
        let full_path = self.resolve_path(path);
        let path_str = full_path.display().to_string();

        // Syntax check
        if let Some(error) = check_syntax(&full_path, content) {
            let mut msg = format!("Write blocked -- syntax error");
            if let Some(line) = error.line {
                msg.push_str(&format!(" at line {}", line));
            }
            msg.push_str(&format!(":\n{}", error.message));

            // Show context around the error line
            if let Some(line) = error.line {
                let lines: Vec<&str> = content.lines().collect();
                let start = line.saturating_sub(4);
                let end = (line + 3).min(lines.len());
                msg.push_str("\nContext:\n");
                for i in start..end {
                    let marker = if i + 1 == line { ">>>" } else { "   " };
                    msg.push_str(&format!(
                        "{} {:>4} | {}\n",
                        marker,
                        i + 1,
                        lines.get(i).unwrap_or(&"")
                    ));
                }
            }

            msg.push_str("\nFix the syntax error and call write() again.");
            return ToolResult {
                output: msg,
                is_error: true,
            };
        }

        // Backup existing file
        let old_content = std::fs::read_to_string(&full_path).ok();
        if let Some(ref old) = old_content {
            self.file_backups.insert(path_str.clone(), old.clone());
        }

        // Create parent dirs and write
        if let Some(parent) = full_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return ToolResult {
                    output: format!("Error creating directories: {}", e),
                    is_error: true,
                };
            }
        }

        if let Err(e) = std::fs::write(&full_path, content) {
            return ToolResult {
                output: format!("Error writing file: {}", e),
                is_error: true,
            };
        }

        kms.record_file_modification(&path_str);
        context.promote_to_active(&path_str, content);

        // Update cache
        let summary = generate_compact_summary(content, path);
        self.file_cache.insert(
            path_str.clone(),
            FileKnowledge {
                content: content.to_string(),
                compact_summary: summary,
                annotations: String::new(),
            },
        );

        // Build result
        let line_count = content.lines().count();
        let mut result = format!("Written: {} ({} lines", path, line_count);

        if let Some(ref old) = old_content {
            let (added, removed) = diff_summary(old, content);
            result.push_str(&format!(", +{} -{} from original", added, removed));
        }
        result.push(')');

        // Post-write lint (async-ish but we block here for simplicity)
        if let Some(lint_output) = run_lint(&full_path, old_content.as_deref()) {
            result.push_str(&format!("\n{}", lint_output));
        }

        // Check for related tests
        if let Some(tests) = find_related_tests(&full_path) {
            result.push_str(&format!("\nRelated tests: {}", tests));
        }

        ToolResult {
            output: result,
            is_error: false,
        }
    }

    pub fn handle_edit(
        &mut self,
        path: &str,
        old_text: &str,
        new_text: &str,
        context: &mut ContextController,
        kms: &mut KMS,
    ) -> ToolResult {
        let full_path = self.resolve_path(path);
        let path_str = full_path.display().to_string();

        let current = match std::fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(e) => {
                return ToolResult {
                    output: format!("Error reading {}: {}", path, e),
                    is_error: true,
                }
            }
        };

        // 1. Try exact match first
        let count = current.matches(old_text).count();
        let new_content = if count == 1 {
            current.replacen(old_text, new_text, 1)
        } else if count > 1 {
            return ToolResult {
                output: format!(
                    "Edit failed: old_text found {} times in {}. Include more surrounding context to make it unique.",
                    count, path
                ),
                is_error: true,
            };
        } else {
            // 2. Try whitespace-normalized match (collapse runs of whitespace)
            if let Some(result) = try_whitespace_normalized_edit(&current, old_text, new_text) {
                result
            }
            // 3. Try trimmed-lines match (strip leading/trailing whitespace per line)
            else if let Some(result) = try_trimmed_lines_edit(&current, old_text, new_text) {
                result
            } else {
                // Build a helpful error with expanded context so agent doesn't need to re-read
                let content_lines: Vec<&str> = current.lines().collect();
                let hint = find_closest_match(&current, old_text);
                let mut msg = format!("Edit failed: old_text not found in {}.", path);
                if let Some(ref h) = hint {
                    let context_start = h.start_line.saturating_sub(6);
                    let context_end = (h.end_line + 5).min(content_lines.len());
                    let context_text: Vec<String> = content_lines[context_start..context_end]
                        .iter()
                        .enumerate()
                        .map(|(i, line)| format!("{:>4} | {}", context_start + i + 1, line))
                        .collect();
                    msg.push_str(&format!(
                        "\n\nClosest match at lines {}-{}. Here is the actual file content (lines {}-{}):\n```\n{}\n```\nCopy the exact text from above as old_text.",
                        h.start_line, h.end_line, context_start + 1, context_end, context_text.join("\n")
                    ));
                } else {
                    // No close match at all — show first 30 lines of file as reference
                    let preview_end = 30.min(content_lines.len());
                    let preview: Vec<String> = content_lines[..preview_end]
                        .iter()
                        .enumerate()
                        .map(|(i, line)| format!("{:>4} | {}", i + 1, line))
                        .collect();
                    msg.push_str(&format!(
                        "\nNo similar text found. File start (lines 1-{}):\n```\n{}\n```\nUse grep_file() to find the exact text, or read() with line range.",
                        preview_end, preview.join("\n")
                    ));
                }
                return ToolResult {
                    output: msg,
                    is_error: true,
                };
            }
        };

        if let Some(error) = check_syntax(&full_path, &new_content) {
            let mut msg = format!("Edit blocked -- syntax error");
            if let Some(line) = error.line {
                msg.push_str(&format!(" at line {}", line));
            }
            msg.push_str(&format!(":\n{}", error.message));

            if let Some(line) = error.line {
                let lines: Vec<&str> = new_content.lines().collect();
                let start = line.saturating_sub(4);
                let end = (line + 3).min(lines.len());
                msg.push_str("\nContext:\n");
                for i in start..end {
                    let marker = if i + 1 == line { ">>>" } else { "   " };
                    msg.push_str(&format!(
                        "{} {:>4} | {}\n",
                        marker,
                        i + 1,
                        lines.get(i).unwrap_or(&"")
                    ));
                }
            }

            msg.push_str("\nFix the syntax error and try again.");
            return ToolResult {
                output: msg,
                is_error: true,
            };
        }

        self.file_backups.insert(path_str.clone(), current);

        if let Err(e) = std::fs::write(&full_path, &new_content) {
            return ToolResult {
                output: format!("Error writing {}: {}", path, e),
                is_error: true,
            };
        }

        kms.record_file_modification(&path_str);
        context.promote_to_active(&path_str, &new_content);

        let summary = generate_compact_summary(&new_content, path);
        self.file_cache.insert(
            path_str.clone(),
            FileKnowledge {
                content: new_content.clone(),
                compact_summary: summary,
                annotations: String::new(),
            },
        );

        let old_lines = old_text.lines().count();
        let new_lines = new_text.lines().count();
        let mut result = format!(
            "Edited {} (replaced {} lines with {} lines)",
            path, old_lines, new_lines
        );

        if let Some(lint_output) = run_lint(&full_path, None) {
            result.push_str(&format!("\n{}", lint_output));
        }

        ToolResult {
            output: result,
            is_error: false,
        }
    }

    pub fn handle_create_file(
        &mut self,
        path: &str,
        content: &str,
        context: &mut ContextController,
        kms: &mut KMS,
    ) -> ToolResult {
        let full_path = self.resolve_path(path);
        let path_str = full_path.display().to_string();

        if full_path.exists() {
            return ToolResult {
                output: format!("File already exists: {}. Use edit() to modify it.", path),
                is_error: true,
            };
        }

        if let Some(parent) = full_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return ToolResult {
                    output: format!("Error creating directories: {}", e),
                    is_error: true,
                };
            }
        }

        if let Err(e) = std::fs::write(&full_path, content) {
            return ToolResult {
                output: format!("Error writing {}: {}", path, e),
                is_error: true,
            };
        }

        kms.record_file_modification(&path_str);
        context.promote_to_active(&path_str, content);

        let summary = generate_compact_summary(content, path);
        self.file_cache.insert(
            path_str,
            FileKnowledge {
                content: content.to_string(),
                compact_summary: summary,
                annotations: String::new(),
            },
        );

        let line_count = content.lines().count();
        ToolResult {
            output: format!("Created {} ({} lines)", path, line_count),
            is_error: false,
        }
    }

    pub fn handle_grep(&self, pattern: &str, search_path: Option<&str>) -> ToolResult {
        let search_dir = match search_path {
            Some(p) => self.resolve_path(p),
            None => self.project_root.clone(),
        };

        let mut cmd = Command::new("rg");
        cmd.arg("--line-number")
            .arg("--no-heading")
            .arg("--color=never")
            .arg("--max-count=30")
            .arg("-C")
            .arg("2")
            .arg(pattern)
            .arg(&search_dir);

        match cmd.output() {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.is_empty() {
                    return ToolResult {
                        output: format!("No matches found for \"{}\"", pattern),
                        is_error: false,
                    };
                }

                let formatted = format_grep_results(&stdout, &self.file_cache, &self.project_root);
                ToolResult {
                    output: formatted,
                    is_error: false,
                }
            }
            Err(e) => ToolResult {
                output: format!("Grep error: {}", e),
                is_error: true,
            },
        }
    }

    pub fn handle_grep_file(
        &self,
        path: &str,
        pattern: &str,
        context_lines: Option<usize>,
    ) -> ToolResult {
        let full_path = self.resolve_path(path);
        let ctx = context_lines.unwrap_or(3);

        if !full_path.exists() {
            return ToolResult {
                output: format!("File not found: {}", path),
                is_error: true,
            };
        }

        let mut cmd = Command::new("rg");
        cmd.arg("--line-number")
            .arg("--no-heading")
            .arg("--color=never")
            .arg("-C")
            .arg(ctx.to_string())
            .arg(pattern)
            .arg(&full_path);

        match cmd.output() {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.is_empty() {
                    return ToolResult {
                        output: format!("No matches for \"{}\" in {}", pattern, path),
                        is_error: false,
                    };
                }

                // Add total line count for orientation
                let total_lines = std::fs::read_to_string(&full_path)
                    .map(|c| c.lines().count())
                    .unwrap_or(0);
                let match_count = stdout.lines().filter(|l| !l.starts_with('-')).count();

                let mut result = format!(
                    "{} ({} total lines, {} matching lines):\n",
                    path, total_lines, match_count
                );
                result.push_str(&stdout);
                ToolResult {
                    output: result,
                    is_error: false,
                }
            }
            Err(e) => ToolResult {
                output: format!("Grep error: {}", e),
                is_error: true,
            },
        }
    }

    pub fn handle_execute(
        &self,
        command: &str,
        working_dir: Option<&str>,
        timeout_secs: Option<u64>,
        kms: &mut KMS,
    ) -> ToolResult {
        let cmd_trimmed = command.trim();
        if cmd_trimmed.is_empty() {
            return ToolResult {
                output: "Error: empty command.".into(),
                is_error: true,
            };
        }

        // Safety: block destructive patterns
        let cmd_lower = cmd_trimmed.to_lowercase();
        for blocked in BLOCKED_COMMANDS {
            let normalized = blocked.replace(' ', "");
            let cmd_normalized = cmd_lower.replace(' ', "");
            if cmd_normalized.contains(&normalized) {
                return ToolResult {
                    output: format!(
                        "Blocked: '{}' matches dangerous pattern '{}'.",
                        cmd_trimmed, blocked
                    ),
                    is_error: true,
                };
            }
        }

        // Block piping untrusted remote content to shell
        if (cmd_lower.contains("curl ") || cmd_lower.contains("wget "))
            && (cmd_lower.contains("| bash")
                || cmd_lower.contains("| sh")
                || cmd_lower.contains("|bash")
                || cmd_lower.contains("|sh"))
        {
            return ToolResult {
                output: "Blocked: piping remote content to a shell is not allowed.".into(),
                is_error: true,
            };
        }

        let cwd = match working_dir {
            Some(d) => {
                let p = self.resolve_path(d);
                if !p.is_dir() {
                    return ToolResult {
                        output: format!("Error: working directory '{}' does not exist.", d),
                        is_error: true,
                    };
                }
                p
            }
            None => self.project_root.clone(),
        };

        let timeout = timeout_secs.unwrap_or(EXECUTE_TIMEOUT_SECS);

        // Determine shell
        let (shell, shell_arg) = if cfg!(target_os = "windows") {
            (std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into()), "/C")
        } else {
            (std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()), "-c")
        };

        let mut child = match Command::new(&shell)
            .arg(shell_arg)
            .arg(cmd_trimmed)
            .current_dir(&cwd)
            .env("TERM", "dumb")
            .env("NO_COLOR", "1")
            .env("CI", "true")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                return ToolResult {
                    output: format!("Failed to spawn command: {}", e),
                    is_error: true,
                };
            }
        };

        // Drain stdout and stderr on background threads to prevent pipe buffer
        // deadlocks. Previously we waited for the process to exit before reading,
        // which caused commands like `cargo build` to hang when the OS pipe buffer
        // (typically 64 KB) filled up — the child blocks on write while we block
        // on wait, creating a deadlock.
        let stdout_handle = child.stdout.take().map(|pipe| {
            std::thread::spawn(move || {
                use std::io::Read;
                let mut buf = Vec::new();
                let mut reader = std::io::BufReader::new(pipe);
                let _ = reader.read_to_end(&mut buf);
                String::from_utf8_lossy(&buf).to_string()
            })
        });
        let stderr_handle = child.stderr.take().map(|pipe| {
            std::thread::spawn(move || {
                use std::io::Read;
                let mut buf = Vec::new();
                let mut reader = std::io::BufReader::new(pipe);
                let _ = reader.read_to_end(&mut buf);
                String::from_utf8_lossy(&buf).to_string()
            })
        });

        // Wait with timeout — pipes are being drained concurrently so no deadlock
        let start = std::time::Instant::now();
        let wait_result = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => {
                    if start.elapsed().as_secs() >= timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        break Err(format!("Command timed out after {}s.", timeout));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(e) => break Err(format!("Error waiting for command: {}", e)),
            }
        };

        // Collect output from reader threads
        let stdout = stdout_handle
            .and_then(|h| h.join().ok())
            .unwrap_or_default();
        let stderr = stderr_handle
            .and_then(|h| h.join().ok())
            .unwrap_or_default();

        match wait_result {
            Err(timeout_msg) => {
                kms.record_error(timeout_msg.clone());
                // Still include any partial output captured before the timeout
                let mut output = timeout_msg.clone();
                if !stdout.is_empty() || !stderr.is_empty() {
                    output.push_str("\n\nPartial output before timeout:\n");
                    if !stdout.is_empty() {
                        output.push_str(&stdout);
                    }
                    if !stderr.is_empty() {
                        if !stdout.is_empty() {
                            output.push('\n');
                        }
                        output.push_str("[stderr]\n");
                        output.push_str(&stderr);
                    }
                }
                ToolResult {
                    output,
                    is_error: true,
                }
            }
            Ok(status) => {
                let exit_code = status.code().unwrap_or(-1);
                let is_error = !status.success();

                let mut output = String::new();
                if !stdout.is_empty() {
                    output.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push_str("[stderr]\n");
                    output.push_str(&stderr);
                }

                if output.is_empty() {
                    output = if is_error {
                        format!("Command exited with code {} (no output).", exit_code)
                    } else {
                        "(no output)".into()
                    };
                } else if is_error {
                    output.push_str(&format!("\n[exit code: {}]", exit_code));
                }

                // Truncate very long output — keep tail for build errors which
                // typically appear at the end
                if output.len() > EXECUTE_MAX_OUTPUT_BYTES {
                    let total_len = output.len();
                    let total_lines = output.lines().count();
                    // Keep first 8KB for initial context + last portion up to limit
                    let head_budget = 8_000.min(EXECUTE_MAX_OUTPUT_BYTES / 4);
                    let tail_budget = EXECUTE_MAX_OUTPUT_BYTES - head_budget - 200;

                    let head_end = {
                        let mut end = head_budget;
                        while end > 0 && !output.is_char_boundary(end) {
                            end -= 1;
                        }
                        end
                    };
                    let tail_start = {
                        let mut start = total_len.saturating_sub(tail_budget);
                        while start < total_len && !output.is_char_boundary(start) {
                            start += 1;
                        }
                        start
                    };

                    let head_lines = output[..head_end].lines().count();
                    let tail_lines = output[tail_start..].lines().count();
                    let omitted = total_lines.saturating_sub(head_lines + tail_lines);

                    output = format!(
                        "{}\n\n... ({} lines omitted, {} total bytes) ...\n\n{}",
                        &output[..head_end],
                        omitted,
                        total_len,
                        &output[tail_start..]
                    );
                }

                if is_error {
                    kms.record_error(format!(
                        "Command '{}' failed (exit {})",
                        cmd_trimmed, exit_code
                    ));
                }

                ToolResult { output, is_error }
            }
        }
    }

    pub fn handle_list_dir(&self, path: &str) -> ToolResult {
        let full_path = self.resolve_path(path);

        if !full_path.exists() {
            return ToolResult {
                output: format!("Path does not exist: {}", path),
                is_error: true,
            };
        }

        if !full_path.is_dir() {
            return ToolResult {
                output: format!("Not a directory: {}", path),
                is_error: true,
            };
        }

        match std::fs::read_dir(&full_path) {
            Ok(entries) => {
                let mut items: Vec<String> = entries
                    .filter_map(|e| e.ok())
                    .map(|e| {
                        let meta = e.metadata().ok();
                        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                        let is_dir = meta.map(|m| m.is_dir()).unwrap_or(false);
                        let name = e.file_name().to_string_lossy().to_string();
                        if is_dir {
                            format!("{}/", name)
                        } else {
                            format!("{} ({})", name, format_file_size(size))
                        }
                    })
                    .collect();
                items.sort();
                if items.is_empty() {
                    ToolResult {
                        output: format!("{}: (empty directory)", path),
                        is_error: false,
                    }
                } else {
                    ToolResult {
                        output: items.join("\n"),
                        is_error: false,
                    }
                }
            }
            Err(e) => ToolResult {
                output: format!("Error listing {}: {}", path, e),
                is_error: true,
            },
        }
    }

    fn resolve_path(&self, path: &str) -> PathBuf {
        let p = Path::new(path);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.project_root.join(p)
        }
    }

    fn file_was_modified_on_disk(&self, path: &str) -> bool {
        if let Some(cached) = self.file_cache.get(path) {
            if let Ok(current) = std::fs::read_to_string(path) {
                return current != cached.content;
            }
        }
        true
    }
}

fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn try_whitespace_normalized_edit(content: &str, old_text: &str, new_text: &str) -> Option<String> {
    let norm_old = normalize_whitespace(old_text);
    if norm_old.is_empty() {
        return None;
    }

    let content_lines: Vec<&str> = content.lines().collect();
    let old_lines: Vec<&str> = old_text.lines().collect();
    let old_line_count = old_lines.len();
    if old_line_count == 0 {
        return None;
    }

    let mut match_start = None;
    let mut match_count = 0;

    for i in 0..content_lines.len() {
        if i + old_line_count > content_lines.len() {
            break;
        }

        let mut matches = true;
        for j in 0..old_line_count {
            if normalize_whitespace(content_lines[i + j]) != normalize_whitespace(old_lines[j]) {
                matches = false;
                break;
            }
        }
        if matches {
            match_start = Some(i);
            match_count += 1;
        }
    }

    if match_count != 1 {
        return None;
    }

    let start = match_start.unwrap();
    let mut result = String::new();
    for line in &content_lines[..start] {
        result.push_str(line);
        result.push('\n');
    }
    result.push_str(new_text);
    if !new_text.ends_with('\n') && start + old_line_count < content_lines.len() {
        result.push('\n');
    }
    for line in &content_lines[start + old_line_count..] {
        result.push_str(line);
        result.push('\n');
    }
    // Preserve original trailing newline behavior
    if !content.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }
    Some(result)
}

fn try_trimmed_lines_edit(content: &str, old_text: &str, new_text: &str) -> Option<String> {
    let content_lines: Vec<&str> = content.lines().collect();
    let old_lines: Vec<&str> = old_text.lines().collect();
    let old_line_count = old_lines.len();
    if old_line_count == 0 {
        return None;
    }

    let mut match_start = None;
    let mut match_count = 0;

    for i in 0..content_lines.len() {
        if i + old_line_count > content_lines.len() {
            break;
        }

        let mut matches = true;
        for j in 0..old_line_count {
            if content_lines[i + j].trim() != old_lines[j].trim() {
                matches = false;
                break;
            }
        }
        if matches {
            match_start = Some(i);
            match_count += 1;
        }
    }

    if match_count != 1 {
        return None;
    }

    let start = match_start.unwrap();
    let mut result = String::new();
    for line in &content_lines[..start] {
        result.push_str(line);
        result.push('\n');
    }
    result.push_str(new_text);
    if !new_text.ends_with('\n') && start + old_line_count < content_lines.len() {
        result.push('\n');
    }
    for line in &content_lines[start + old_line_count..] {
        result.push_str(line);
        result.push('\n');
    }
    if !content.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }
    Some(result)
}

struct CloseMatch {
    start_line: usize,
    end_line: usize,
    #[allow(dead_code)]
    text: String,
}

fn find_closest_match(content: &str, old_text: &str) -> Option<CloseMatch> {
    let content_lines: Vec<&str> = content.lines().collect();
    let old_lines: Vec<&str> = old_text.lines().collect();
    if old_lines.is_empty() || content_lines.is_empty() {
        return None;
    }

    let first_trimmed = old_lines[0].trim();
    if first_trimmed.is_empty() {
        return None;
    }

    // Find lines that match the first line of old_text (trimmed)
    let mut best: Option<(usize, usize)> = None; // (start_idx, matching_lines)

    for (i, line) in content_lines.iter().enumerate() {
        if line.trim().contains(first_trimmed) || first_trimmed.contains(line.trim()) {
            // Count how many subsequent lines also match
            let mut matching = 1;
            for j in 1..old_lines.len() {
                if i + j >= content_lines.len() {
                    break;
                }
                let a = content_lines[i + j].trim();
                let b = old_lines[j].trim();
                if a == b || a.contains(b) || b.contains(a) {
                    matching += 1;
                } else {
                    break;
                }
            }
            if let Some((_, best_count)) = best {
                if matching > best_count {
                    best = Some((i, matching));
                }
            } else {
                best = Some((i, matching));
            }
        }
    }

    let (start, _) = best?;
    let end = (start + old_lines.len()).min(content_lines.len());
    let text = content_lines[start..end].join("\n");
    Some(CloseMatch {
        start_line: start + 1,
        end_line: end,
        text,
    })
}

fn generate_compact_summary(content: &str, path: &str) -> String {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();
    let mut items = Vec::new();

    match ext {
        "rs" => {
            for line in &lines {
                let trimmed = line.trim();
                if trimmed.starts_with("pub fn ") || trimmed.starts_with("fn ") {
                    if let Some(name) = extract_fn_name(trimmed) {
                        items.push(name);
                    }
                } else if trimmed.starts_with("pub struct ") || trimmed.starts_with("struct ") {
                    if let Some(name) = extract_type_name(trimmed, "struct") {
                        items.push(name);
                    }
                } else if trimmed.starts_with("pub enum ") || trimmed.starts_with("enum ") {
                    if let Some(name) = extract_type_name(trimmed, "enum") {
                        items.push(name);
                    }
                } else if trimmed.starts_with("pub trait ") || trimmed.starts_with("trait ") {
                    if let Some(name) = extract_type_name(trimmed, "trait") {
                        items.push(name);
                    }
                } else if trimmed.starts_with("impl ") {
                    let rest = &trimmed[5..];
                    if let Some(end) = rest.find(|c: char| c == '{' || c == ' ') {
                        items.push(format!("impl {}", &rest[..end]));
                    }
                }
            }
        }
        "py" => {
            for line in &lines {
                let trimmed = line.trim();
                if trimmed.starts_with("def ") {
                    if let Some(paren) = trimmed.find('(') {
                        items.push(format!("{}()", &trimmed[4..paren]));
                    }
                } else if trimmed.starts_with("class ") {
                    if let Some(end) = trimmed.find(|c: char| c == '(' || c == ':') {
                        items.push(trimmed[6..end].to_string());
                    }
                }
            }
        }
        "js" | "ts" | "jsx" | "tsx" => {
            for line in &lines {
                let trimmed = line.trim();
                if trimmed.contains("function ") {
                    if let Some(start) = trimmed.find("function ") {
                        let rest = &trimmed[start + 9..];
                        if let Some(paren) = rest.find('(') {
                            let name = rest[..paren].trim();
                            if !name.is_empty() {
                                items.push(format!("{}()", name));
                            }
                        }
                    }
                } else if trimmed.starts_with("export ") || trimmed.starts_with("class ") {
                    items.push(trimmed.chars().take(60).collect::<String>());
                }
            }
        }
        "go" => {
            for line in &lines {
                let trimmed = line.trim();
                if trimmed.starts_with("func ") {
                    if let Some(paren) = trimmed.find('(') {
                        let name = &trimmed[5..paren];
                        items.push(format!("{}()", name.trim()));
                    }
                } else if trimmed.starts_with("type ") {
                    if let Some(space) = trimmed[5..].find(' ') {
                        items.push(trimmed[5..5 + space].to_string());
                    }
                }
            }
        }
        _ => {
            // Generic: first doc comment + line count
            let first_comment = lines
                .iter()
                .take(5)
                .find(|l| l.starts_with("//") || l.starts_with("#") || l.starts_with("/*"))
                .map(|l| l.trim().to_string());
            if let Some(c) = first_comment {
                items.push(c);
            }
        }
    }

    if items.is_empty() {
        format!("{} lines", total)
    } else {
        let joined = items.iter().take(8).cloned().collect::<Vec<_>>().join(", ");
        format!("{}. {} lines total.", joined, total)
    }
}

fn extract_fn_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let start = if trimmed.starts_with("pub fn ") {
        7
    } else if trimmed.starts_with("pub async fn ") {
        13
    } else if trimmed.starts_with("async fn ") {
        9
    } else if trimmed.starts_with("fn ") {
        3
    } else {
        return None;
    };
    let rest = &trimmed[start..];
    rest.find('(').map(|p| format!("{}()", &rest[..p]))
}

fn extract_type_name(line: &str, keyword: &str) -> Option<String> {
    let trimmed = line.trim();
    if let Some(pos) = trimmed.find(keyword) {
        let after = &trimmed[pos + keyword.len()..].trim();
        let end = after
            .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '<' && c != '>')
            .unwrap_or(after.len());
        Some(after[..end].to_string())
    } else {
        None
    }
}

fn check_syntax(path: &Path, content: &str) -> Option<SyntaxError> {
    let ext = path.extension().and_then(|e| e.to_str())?;
    match ext {
        "py" => {
            let tmp = std::env::temp_dir().join("hakari_syntax_check.py");
            std::fs::write(&tmp, content).ok()?;
            let output = Command::new("python3")
                .arg("-m")
                .arg("py_compile")
                .arg(&tmp)
                .output()
                .ok()?;
            let _ = std::fs::remove_file(&tmp);
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let line = parse_error_line(&stderr);
                return Some(SyntaxError {
                    message: stderr,
                    line,
                });
            }
            None
        }
        "js" | "mjs" => {
            let tmp = std::env::temp_dir().join("hakari_syntax_check.js");
            std::fs::write(&tmp, content).ok()?;
            let output = Command::new("node")
                .arg("--check")
                .arg(&tmp)
                .output()
                .ok()?;
            let _ = std::fs::remove_file(&tmp);
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let line = parse_error_line(&stderr);
                return Some(SyntaxError {
                    message: stderr,
                    line,
                });
            }
            None
        }
        _ => None,
    }
}

fn parse_error_line(error_text: &str) -> Option<usize> {
    // Try patterns: "line X", ":X:", ":X,", "Line X"
    for line in error_text.lines() {
        // Pattern: ":N:" or ":N,"
        let parts: Vec<&str> = line.split(':').collect();
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                if let Ok(n) = part.trim().parse::<usize>() {
                    if n > 0 && n < 100_000 {
                        return Some(n);
                    }
                }
            }
        }
    }
    None
}

fn run_lint(path: &Path, _old_content: Option<&str>) -> Option<String> {
    let ext = path.extension().and_then(|e| e.to_str())?;
    let project_root = find_project_root(path)?;

    match ext {
        "rs" => {
            let output = Command::new("cargo")
                .arg("check")
                .arg("--message-format=short")
                .current_dir(&project_root)
                .output()
                .ok()?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let relevant: Vec<&str> = stderr
                    .lines()
                    .filter(|l| l.contains("error") || l.contains("warning"))
                    .take(5)
                    .collect();
                if !relevant.is_empty() {
                    return Some(relevant.join("\n"));
                }
            }
            None
        }
        "py" => {
            // Check for ruff
            let output = Command::new("ruff").arg("check").arg(path).output().ok()?;
            if !output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let lines: Vec<&str> = stdout.lines().take(5).collect();
                if !lines.is_empty() {
                    return Some(lines.join("\n"));
                }
            }
            None
        }
        _ => None,
    }
}

fn find_project_root(path: &Path) -> Option<PathBuf> {
    let mut dir = path.parent()?;
    loop {
        if dir.join("Cargo.toml").exists()
            || dir.join("package.json").exists()
            || dir.join("pyproject.toml").exists()
            || dir.join(".git").exists()
        {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

fn find_related_tests(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let parent = path.parent()?;

    let test_patterns = [
        format!("{}_test", stem),
        format!("{}.test", stem),
        format!("{}.spec", stem),
        format!("test_{}", stem),
    ];

    let mut found = Vec::new();
    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            for pattern in &test_patterns {
                if name.contains(pattern.as_str()) {
                    found.push(entry.path().display().to_string());
                }
            }
        }
    }

    // Also check tests/ directory
    let project_root = find_project_root(path)?;
    let tests_dir = project_root.join("tests");
    if tests_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&tests_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.contains(stem) {
                    found.push(entry.path().display().to_string());
                }
            }
        }
    }

    if found.is_empty() {
        None
    } else {
        Some(found.join(", "))
    }
}

fn diff_summary(old: &str, new: &str) -> (usize, usize) {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    let mut added = 0;
    let mut removed = 0;

    // Simple line-by-line comparison
    let max = old_lines.len().max(new_lines.len());
    for i in 0..max {
        match (old_lines.get(i), new_lines.get(i)) {
            (Some(a), Some(b)) if a != b => {
                added += 1;
                removed += 1;
            }
            (None, Some(_)) => {
                added += 1;
            }
            (Some(_), None) => {
                removed += 1;
            }
            _ => {}
        }
    }

    (added, removed)
}

fn format_file_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{}B", bytes)
    }
}

fn format_grep_results(
    raw_output: &str,
    file_cache: &HashMap<String, FileKnowledge>,
    project_root: &Path,
) -> String {
    let mut grouped: HashMap<String, Vec<String>> = HashMap::new();
    let mut total = 0;

    for line in raw_output.lines() {
        if let Some((file, rest)) = line.split_once(':') {
            let entry = grouped.entry(file.to_string()).or_default();
            entry.push(format!("  L{}", rest));
            total += 1;
        }
    }

    let mut output = String::new();
    for (file, matches) in &grouped {
        let rel = Path::new(file)
            .strip_prefix(project_root)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| file.clone());

        output.push_str(&rel);

        // Enrich with cached summary
        if let Some(knowledge) = file_cache.get(file) {
            if !knowledge.compact_summary.is_empty() {
                output.push_str(&format!(": ({})", knowledge.compact_summary));
            }
        }
        output.push('\n');

        for m in matches {
            output.push_str(m);
            output.push('\n');
        }
        output.push('\n');
    }

    output.push_str(&format!("{} matches in {} files", total, grouped.len()));
    output
}
