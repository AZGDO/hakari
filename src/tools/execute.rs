use super::{ToolResult, ToolResultMetadata};
use crate::memory::kkm::Kkm;
use std::path::Path;
use std::time::Instant;
use tokio::io::AsyncBufReadExt;
use tokio::process::Command;
use tokio::sync::mpsc;

const BLOCKED_COMMANDS: &[&str] = &[
    "rm -rf /",
    "rm -rf ~",
    "sudo rm -rf",
    "mkfs",
    "format",
    ":(){:|:&};:",
];

const CONFIRM_COMMANDS: &[&str] = &["git push", "git commit", "npm publish", "cargo publish"];

pub struct ExecuteResult {
    pub tool_result: ToolResult,
    pub needs_confirmation: bool,
    pub confirmation_message: Option<String>,
}

pub async fn execute_command(
    project_dir: &Path,
    command: &str,
    kkm: &Kkm,
    stream_tx: Option<mpsc::UnboundedSender<String>>,
) -> ExecuteResult {
    // Safety check
    for blocked in BLOCKED_COMMANDS {
        if command.contains(blocked) {
            return ExecuteResult {
                tool_result: ToolResult {
                    success: false,
                    output: format!("Command blocked for safety: contains '{}'", blocked),
                    metadata: ToolResultMetadata::default(),
                },
                needs_confirmation: false,
                confirmation_message: None,
            };
        }
    }

    // Check if confirmation needed
    for confirm_cmd in CONFIRM_COMMANDS {
        if command.starts_with(confirm_cmd) {
            return ExecuteResult {
                tool_result: ToolResult {
                    success: false,
                    output: String::new(),
                    metadata: ToolResultMetadata::default(),
                },
                needs_confirmation: true,
                confirmation_message: Some(format!(
                    "Command '{}' requires confirmation. Approve?",
                    command
                )),
            };
        }
    }

    // Transform command based on KKM quirks
    let transformed = kkm.transform_command(command);

    // Determine timeout
    let timeout_secs = determine_timeout(&transformed);

    let start = Instant::now();

    let mut child = match Command::new("sh")
        .arg("-c")
        .arg(&transformed)
        .current_dir(project_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            return ExecuteResult {
                tool_result: ToolResult {
                    success: false,
                    output: format!("Failed to execute command: {}", e),
                    metadata: ToolResultMetadata::default(),
                },
                needs_confirmation: false,
                confirmation_message: None,
            };
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let stdout_task = tokio::spawn(read_stream(stdout, false, stream_tx.clone()));
    let stderr_task = tokio::spawn(read_stream(stderr, true, stream_tx.clone()));

    let wait_result =
        tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), child.wait()).await;

    let status = match wait_result {
        Ok(Ok(status)) => status,
        Ok(Err(error)) => {
            return ExecuteResult {
                tool_result: ToolResult {
                    success: false,
                    output: format!("Failed while waiting for command: {}", error),
                    metadata: ToolResultMetadata::default(),
                },
                needs_confirmation: false,
                confirmation_message: None,
            };
        }
        Err(_) => {
            let _ = child.kill().await;
            if let Some(stream_tx) = &stream_tx {
                let _ = stream_tx.send(format!(
                    "\n[hakari] command timed out after {}s\n",
                    timeout_secs
                ));
            }
            let stdout = stdout_task.await.unwrap_or_default();
            let stderr = stderr_task.await.unwrap_or_default();
            let elapsed = start.elapsed();
            let formatted = format_command_output(
                &transformed,
                -1,
                &stdout,
                &format!("{}\nTimed out after {}s", stderr, timeout_secs),
                elapsed.as_millis() as u64,
            );

            return ExecuteResult {
                tool_result: ToolResult {
                    success: false,
                    output: formatted,
                    metadata: ToolResultMetadata {
                        exit_code: Some(-1),
                        execution_time_ms: Some(elapsed.as_millis() as u64),
                        ..Default::default()
                    },
                },
                needs_confirmation: false,
                confirmation_message: None,
            };
        }
    };

    let stdout = stdout_task.await.unwrap_or_default();
    let stderr = stderr_task.await.unwrap_or_default();
    let elapsed = start.elapsed();
    let exit_code = status.code().unwrap_or(-1);

    let formatted = format_command_output(
        &transformed,
        exit_code,
        &stdout,
        &stderr,
        elapsed.as_millis() as u64,
    );

    ExecuteResult {
        tool_result: ToolResult {
            success: exit_code == 0,
            output: formatted,
            metadata: ToolResultMetadata {
                exit_code: Some(exit_code),
                execution_time_ms: Some(elapsed.as_millis() as u64),
                ..Default::default()
            },
        },
        needs_confirmation: false,
        confirmation_message: None,
    }
}

fn determine_timeout(command: &str) -> u64 {
    let cmd_lower = command.to_lowercase();
    if cmd_lower.contains("build") || cmd_lower.contains("compile") || cmd_lower.contains("test") {
        120
    } else if cmd_lower.starts_with("ls")
        || cmd_lower.starts_with("cat")
        || cmd_lower.starts_with("echo")
        || cmd_lower.starts_with("pwd")
    {
        10
    } else {
        30
    }
}

fn format_command_output(
    command: &str,
    exit_code: i32,
    stdout: &str,
    stderr: &str,
    elapsed_ms: u64,
) -> String {
    let status_icon = if exit_code == 0 { "✓" } else { "✗" };
    let elapsed_str = if elapsed_ms > 1000 {
        format!("{:.1}s", elapsed_ms as f64 / 1000.0)
    } else {
        format!("{}ms", elapsed_ms)
    };

    let mut output = format!(
        "{} Command: {} (exit {}, {})\n",
        status_icon, command, exit_code, elapsed_str
    );

    // Try to parse test output
    if command.contains("test") {
        if let Some(parsed) = try_parse_test_output(stdout, stderr) {
            output.push_str(&parsed);
            return output;
        }
    }

    // Truncate long output
    let combined = if !stderr.is_empty() && exit_code != 0 {
        format!("{}\n{}", stdout, stderr)
    } else {
        stdout.to_string()
    };

    let lines: Vec<&str> = combined.lines().collect();
    if lines.len() > 70 {
        let head: Vec<&str> = lines[..20].to_vec();
        let tail: Vec<&str> = lines[lines.len() - 50..].to_vec();
        output.push_str(&head.join("\n"));
        output.push_str(&format!("\n[... {} lines omitted ...]\n", lines.len() - 70));
        output.push_str(&tail.join("\n"));
    } else if !combined.trim().is_empty() {
        output.push_str(&combined);
    }

    output
}

fn try_parse_test_output(stdout: &str, stderr: &str) -> Option<String> {
    let combined = format!("{}\n{}", stdout, stderr);

    // Try to detect common test runner patterns
    // Jest/Vitest
    if combined.contains("Tests:") && (combined.contains("passed") || combined.contains("failed")) {
        let mut result = String::new();
        for line in combined.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("Tests:")
                || trimmed.starts_with("Test Suites:")
                || trimmed.starts_with("PASS")
                || trimmed.starts_with("FAIL")
                || trimmed.contains("Expected")
                || trimmed.contains("Received")
                || trimmed.starts_with("Coverage:")
            {
                result.push_str(&format!("  {}\n", trimmed));
            }
        }
        if !result.is_empty() {
            return Some(result);
        }
    }

    // Rust test output
    if combined.contains("test result:") {
        let mut result = String::new();
        for line in combined.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("test result:")
                || trimmed.starts_with("test ")
                || trimmed.contains("FAILED")
                || trimmed.contains("failures:")
            {
                result.push_str(&format!("  {}\n", trimmed));
            }
        }
        if !result.is_empty() {
            return Some(result);
        }
    }

    // Pytest output
    if combined.contains("passed") && (combined.contains("pytest") || combined.contains("===")) {
        let mut result = String::new();
        for line in combined.lines() {
            let trimmed = line.trim();
            if trimmed.contains("passed")
                || trimmed.contains("failed")
                || trimmed.contains("error")
                || trimmed.starts_with("FAILED")
                || trimmed.starts_with("E ")
                || trimmed.starts_with(">")
            {
                result.push_str(&format!("  {}\n", trimmed));
            }
        }
        if !result.is_empty() {
            return Some(result);
        }
    }

    None
}

async fn read_stream(
    stream: Option<impl tokio::io::AsyncRead + Unpin>,
    is_stderr: bool,
    stream_tx: Option<mpsc::UnboundedSender<String>>,
) -> String {
    let Some(stream) = stream else {
        return String::new();
    };

    let mut reader = tokio::io::BufReader::new(stream);
    let mut buffer = String::new();
    let mut collected = String::new();

    loop {
        buffer.clear();
        match reader.read_line(&mut buffer).await {
            Ok(0) => break,
            Ok(_) => {
                collected.push_str(&buffer);
                if let Some(stream_tx) = &stream_tx {
                    let chunk = if is_stderr {
                        format!("[stderr] {}", buffer)
                    } else {
                        buffer.clone()
                    };
                    let _ = stream_tx.send(chunk);
                }
            }
            Err(error) => {
                let line = format!("[hakari] failed to read process output: {}\n", error);
                collected.push_str(&line);
                if let Some(stream_tx) = &stream_tx {
                    let _ = stream_tx.send(line);
                }
                break;
            }
        }
    }

    collected
}
