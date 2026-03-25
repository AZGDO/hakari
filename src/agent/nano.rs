use serde_json::json;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::agent::context::ContextController;
use crate::agent::llm::{build_tool_def, LlmClient, LlmMessage, StreamEvent, ToolDef};
use crate::agent::loop_detect::{LoopDetector, LoopIntervention};
use crate::agent::shizuka::Preparation;
use crate::agent::tools::ToolInterceptor;
use crate::agent::AgentEvent;
use crate::memory::kms::KMS;

fn safe_prefix(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

pub const NANO_SYSTEM_PROMPT: &str = r#"You are an expert autonomous coding agent. You have full access to the filesystem, shell, and codebase. Your job is to accomplish the user's task completely and correctly, verifying your work at each step.

CORE PRINCIPLES:
1. Be autonomous. Do not ask the user for help. Figure things out yourself.
2. Verify your work. After making changes, run the relevant build/lint/test commands to confirm correctness.
3. Fix what you break. If a build or test fails after your changes, debug and fix it immediately.
4. Be precise. Use grep/grep_file FIRST to locate code. Use read() with line ranges. Use edit() for targeted changes.
5. Be thorough. Handle edge cases, update tests, and ensure nothing is left in a broken state.

WORKFLOW:
1. LOCATE: Use grep() / grep_file() to find relevant code. Never guess file contents.
2. READ: Use read(path, start_line, end_line) to read ONLY what you need. Do not read entire large files.
3. CHANGE: Choose the right tool:
   - write(path, content): Use when you have the COMPLETE corrected file content and the file is small (< 200 lines). This is a single-call operation — no need to read first if the preparation already provided the full code. Ideal for small bug fixes where you know the exact final state.
   - edit(path, old_text, new_text): Use for targeted changes in larger files. Include enough context in old_text for uniqueness.
   - create_file(path, content): Use only for brand new files.
4. VERIFY: Use execute() to run build, lint, type-check, and test commands. Fix any errors.
5. REPEAT: If verification fails, read the error output, locate the problem, fix it, and verify again.

IMPORTANT — DIRECT WRITES:
When the preparation includes inline code for focus regions and you know the exact fix, you can call write() directly with the corrected file content. This saves a round-trip (no need to read first, then edit). Only do this for small files where the preparation gave you the full content.

If edit() fails, the error message includes the actual file content. Use that text directly — do NOT re-read.

EXECUTE TOOL GUIDELINES:
- Use execute() to run any shell command: build tools, test suites, linters, git, package managers, scripts, etc.
- IMPORTANT: Check the Device info (KKM) for the user's OS. On Windows, use cmd.exe syntax (e.g. `dir`, `type`, `del`). On Unix/macOS, use sh syntax (e.g. `ls`, `cat`, `rm`).
- Always check build/compilation after code changes: `cargo check`, `npm run build`, `go build`, `python -m py_compile`, etc.
- Run tests after changes: `cargo test`, `pytest`, `npm test`, etc.
- Use execute() for any task that requires shell access: installing deps, running migrations, checking git status, etc.
- Read command output carefully. If a command fails, analyze the error and fix the root cause.
- For long-running commands, set a reasonable timeout.
- Never run destructive system commands (rm -rf /, etc.). The tool blocks them automatically.

SELF-CHECKING:
- After every code change, verify it compiles/parses correctly.
- After completing all changes, run the project's test suite if one exists.
- If you created new functionality, check that existing tests still pass.
- If you see lint warnings or type errors, fix them before finishing.

RULES:
- edit() replaces old_text with new_text. Minor whitespace differences are tolerated but match indentation.
- Include enough surrounding context lines in old_text to be unique in the file.
- For new files, use create_file(). For existing files, use edit() or write().
- Never rewrite entire files when a targeted edit suffices (unless the file is small and the fix is clear).
- When completely done and verified, state what you changed concisely and stop calling tools.

TOOLS:
- read(path, start_line?, end_line?) — Read a file or specific line range (1-based)
- write(path, content) — Replace entire file with new content (best for small files with known fixes)
- edit(path, old_text, new_text) — Replace old_text with new_text in a file
- create_file(path, content) — Create a new file (fails if exists)
- grep(pattern, path?) — Search codebase for a pattern (regex)
- grep_file(path, pattern, context_lines?) — Search within a specific file
- execute(command, working_dir?, timeout?) — Run any shell command and return output
- list_dir(path) — List directory contents with file sizes"#;

pub fn nano_tools() -> Vec<ToolDef> {
    vec![
        build_tool_def(
            "read",
            "Read a file or a specific line range. Use start_line/end_line to read only what you need instead of the entire file.",
            json!({
                "path": {"type": "string", "description": "File path (relative to project root or absolute)"},
                "start_line": {"type": "integer", "description": "Optional: 1-based start line for range read"},
                "end_line": {"type": "integer", "description": "Optional: 1-based end line (inclusive) for range read"}
            }),
            vec!["path"],
        ),
        build_tool_def(
            "write",
            "Write the complete content of a file, replacing the entire file. Use when you have the full corrected file content and the file is small (< 200 lines). For larger files or partial changes, prefer edit(). Runs syntax check before writing.",
            json!({
                "path": {"type": "string", "description": "File path to write (relative to project root or absolute)"},
                "content": {"type": "string", "description": "Complete file content to write"}
            }),
            vec!["path", "content"],
        ),
        build_tool_def(
            "edit",
            "Edit an existing file by replacing old_text with new_text. Minor whitespace differences are tolerated. Include enough surrounding lines for uniqueness.",
            json!({
                "path": {"type": "string", "description": "File path to edit"},
                "old_text": {"type": "string", "description": "Text to find in the file (include surrounding context for uniqueness)"},
                "new_text": {"type": "string", "description": "Replacement text"}
            }),
            vec!["path", "old_text", "new_text"],
        ),
        build_tool_def(
            "create_file",
            "Create a new file. Fails if the file already exists. Creates parent directories if needed.",
            json!({
                "path": {"type": "string", "description": "File path to create"},
                "content": {"type": "string", "description": "Full file content"}
            }),
            vec!["path", "content"],
        ),
        build_tool_def(
            "grep",
            "Search the entire codebase for a pattern. Returns matching files and lines with context.",
            json!({
                "pattern": {"type": "string", "description": "Search pattern (regex supported)"},
                "path": {"type": "string", "description": "Optional: directory or file to search in"}
            }),
            vec!["pattern"],
        ),
        build_tool_def(
            "grep_file",
            "Search within a specific file for a pattern. Returns matches with surrounding context. More efficient than read() for finding edit targets.",
            json!({
                "path": {"type": "string", "description": "File path to search in"},
                "pattern": {"type": "string", "description": "Search pattern (regex supported)"},
                "context_lines": {"type": "integer", "description": "Lines of context around each match (default: 3)"}
            }),
            vec!["path", "pattern"],
        ),
        build_tool_def(
            "execute",
            "Run a shell command and return its stdout/stderr. Use for builds, tests, linting, git, package managers, scripts, and any other CLI task. Commands run in a fresh shell with the project root as working directory by default.",
            json!({
                "command": {"type": "string", "description": "Shell command to execute (e.g. 'cargo test', 'npm run build', 'git diff')"},
                "working_dir": {"type": "string", "description": "Optional: working directory (relative to project root or absolute). Defaults to project root."},
                "timeout": {"type": "integer", "description": "Optional: timeout in seconds (default: 120)"}
            }),
            vec!["command"],
        ),
        build_tool_def(
            "list_dir",
            "List directory contents with file sizes. Use to explore project structure.",
            json!({
                "path": {"type": "string", "description": "Directory path to list (relative to project root or absolute)"}
            }),
            vec!["path"],
        ),
    ]
}

pub fn build_nano_user_message(prep: &Preparation) -> String {
    let mut msg = String::new();

    msg.push_str(&prep.task_summary);
    msg.push('\n');

    if let Some(ref approach) = prep.approach {
        msg.push_str(&format!("\nApproach: {}\n", approach));
    }

    if !prep.learnings.is_empty() {
        msg.push_str("\nNotes:\n");
        for l in &prep.learnings {
            msg.push_str(&format!("- {}\n", l));
        }
    }

    if !prep.warnings.is_empty() {
        msg.push_str("\nWarnings:\n");
        for w in &prep.warnings {
            msg.push_str(&format!("- {}\n", w));
        }
    }

    if !prep.sub_tasks.is_empty() {
        msg.push_str("\nSteps:\n");
        for (i, st) in prep.sub_tasks.iter().enumerate() {
            msg.push_str(&format!("{}. {}\n", i + 1, st.description));
        }
    }

    if !prep.context_files.is_empty() {
        msg.push_str("\nFiles identified for this task:\n");
        for cf in &prep.context_files {
            let label = match cf.role.as_str() {
                "modify" => "MODIFY",
                "reference" => "REF",
                _ => "CTX",
            };
            msg.push_str(&format!(
                "  [{}] {} \u{2014} {}\n",
                label, cf.path, cf.compact_summary
            ));
            if !cf.annotations.is_empty() {
                msg.push_str(&format!("         Note: {}\n", cf.annotations));
            }

            if !cf.focus_regions.is_empty() {
                let lines: Vec<&str> = cf.content.lines().collect();
                for region in &cf.focus_regions {
                    msg.push_str(&format!(
                        "         Focus lines {}-{}: {}\n",
                        region.start_line, region.end_line, region.description
                    ));
                    if region.end_line.saturating_sub(region.start_line) < 40
                        && !cf.content.is_empty()
                    {
                        let s = region.start_line.saturating_sub(1).min(lines.len());
                        let e = region.end_line.min(lines.len());
                        if s < e {
                            msg.push_str("         ```\n");
                            for (i, line) in lines[s..e].iter().enumerate() {
                                msg.push_str(&format!("         {:>4} | {}\n", s + i + 1, line));
                            }
                            msg.push_str("         ```\n");
                        }
                    }
                }
            }
        }

        let has_inline = prep.context_files.iter().any(|cf| {
            cf.role == "modify"
                && !cf.focus_regions.is_empty()
                && cf
                    .focus_regions
                    .iter()
                    .any(|r| r.end_line.saturating_sub(r.start_line) < 40)
                && !cf.content.is_empty()
        });

        if has_inline {
            msg.push_str("\nThe focused code regions above contain the exact text you need. For small files (< 200 lines), call write(path, content) with the complete corrected file to apply the fix in one shot. For larger files, call edit() directly using the text shown. Only read() if you need additional context beyond what's shown.\n");
        } else {
            msg.push_str("\nUse read() with start_line/end_line to read the focused regions before editing. Use grep_file() if you need to find specific patterns.\n");
        }
    }

    // Append environment info
    let os_name = std::env::consts::OS;
    let shell = if os_name == "windows" {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into())
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into())
    };
    msg.push_str(&format!("\nEnvironment: OS={}, shell={}\n", os_name, shell));

    msg.push_str("\nIMPORTANT: After making all changes, use execute() to verify your work compiles and tests pass. Fix any errors before finishing.\n");

    msg
}

pub async fn run_nano(
    client: Arc<dyn LlmClient>,
    model: &str,
    prep: &Preparation,
    project_root: &str,
    tx: &mpsc::Sender<AgentEvent>,
    cancel_rx: &mut mpsc::Receiver<()>,
) -> Result<(String, KMS), String> {
    let _ = tx.send(AgentEvent::PhaseChange("nano".into())).await;

    let tools = nano_tools();
    let user_message = build_nano_user_message(prep);

    // Pre-populate interceptor file cache from Shizuka's exploration
    let mut interceptor = ToolInterceptor::new(project_root);
    let file_tuples: Vec<(String, String, String, String)> = prep
        .context_files
        .iter()
        .filter(|cf| !cf.content.is_empty())
        .map(|cf| {
            (
                cf.path.clone(),
                cf.content.clone(),
                cf.compact_summary.clone(),
                cf.annotations.clone(),
            )
        })
        .collect();
    interceptor.populate_from_preparation(&file_tuples);

    let context_window = 1_000_000;
    let mut context = ContextController::new(context_window);

    let expected_modify: Vec<String> = prep
        .context_files
        .iter()
        .filter(|cf| cf.role == "modify")
        .map(|cf| cf.path.clone())
        .collect();
    let mut loop_detector = LoopDetector::new(&prep.task_classification, expected_modify);

    let mut kms = KMS::new();
    kms.task_summary = prep.task_summary.clone();
    kms.task_type = prep.task_classification.clone();
    if let Some(ref approach) = prep.approach {
        kms.successful_strategy = Some(approach.clone());
    }
    for st in &prep.sub_tasks {
        kms.sub_tasks.push(crate::memory::kms::SubTask {
            description: st.description.clone(),
            status: crate::memory::kms::SubTaskStatus::Pending,
        });
    }

    let shizuka_paths: Vec<String> = prep
        .context_files
        .iter()
        .map(|cf| cf.path.clone())
        .collect();

    let mut messages: Vec<LlmMessage> = vec![
        LlmMessage::System(NANO_SYSTEM_PROMPT.to_string()),
        LlmMessage::User(user_message),
    ];

    let mut full_response_text = String::new();

    let max_turns = loop_detector.max_iterations + 5;
    for _turn in 0..max_turns {
        if cancel_rx.try_recv().is_ok() {
            return Ok(("Task cancelled by user.".into(), kms));
        }

        // Stream the response via channel
        let (stream_tx, mut stream_rx) = mpsc::channel::<StreamEvent>(32);
        let client_clone = Arc::clone(&client);
        let model_clone = model.to_string();
        let messages_clone = messages.clone();
        let tools_clone = tools.clone();

        let stream_handle = tokio::spawn(async move {
            let _ = client_clone
                .generate_stream(&model_clone, &messages_clone, &tools_clone, stream_tx)
                .await;
        });

        let mut turn_text = String::new();
        let mut turn_tool_calls: Vec<crate::agent::llm::ToolCall> = Vec::new();
        let mut malformed_fc = false;

        while let Some(event) = stream_rx.recv().await {
            match event {
                StreamEvent::TextDelta(delta) => {
                    turn_text.push_str(&delta);
                    let _ = tx.send(AgentEvent::StreamChunk(delta)).await;
                }
                StreamEvent::ToolCall(tc) => {
                    turn_tool_calls.push(tc);
                }
                StreamEvent::Usage(usage) => {
                    let _ = tx
                        .send(AgentEvent::TokenUpdate {
                            input: usage.input_tokens,
                            output: usage.output_tokens,
                            cached: usage.cached_tokens,
                        })
                        .await;
                }
                StreamEvent::Error(e) => {
                    if e.contains("MALFORMED_FUNCTION_CALL") {
                        malformed_fc = true;
                        break;
                    }
                    kms.record_error(e.clone());
                    return Err(e);
                }
                StreamEvent::Done => {
                    break;
                }
            }
        }

        let _ = stream_handle.await;

        // Handle MALFORMED_FUNCTION_CALL
        if malformed_fc {
            messages.push(LlmMessage::Assistant {
                text: "(My previous tool call was too large and failed.)".into(),
                tool_calls: vec![],
            });
            messages.push(LlmMessage::User(
                "Your tool call was too large and failed. Use edit(path, old_text, new_text) to make small, targeted changes instead of rewriting the whole file. Break the change into multiple smaller edit() calls if needed.".into(),
            ));
            continue;
        }

        messages.push(LlmMessage::Assistant {
            text: turn_text.clone(),
            tool_calls: turn_tool_calls.clone(),
        });

        // Publish pending stream to UI so the input can display what will be sent when model is ready
        if !turn_text.is_empty() {
            let _ = tx
                .send(AgentEvent::PendingStreamSet {
                    source: "nano".into(),
                    text: turn_text.clone(),
                    meta: None,
                })
                .await;
            full_response_text.push_str(&turn_text);
        }

        // No tool calls — model is done (or needs nudging)
        if turn_tool_calls.is_empty() {
            if _turn < 2 && kms.modified_files.is_empty() && !turn_text.is_empty() {
                messages.push(LlmMessage::User(
                    "Do not just describe the changes. Actually make them using read() and edit(). Start by reading the file, then call edit() with the exact old_text and new_text. After making changes, use execute() to verify they compile/work.".into(),
                ));
                continue;
            }
            break;
        }

        // Process tool calls
        for tc in &turn_tool_calls {
            let _ = tx
                .send(AgentEvent::ToolStart {
                    name: tc.name.clone(),
                    args: tc.args.to_string(),
                })
                .await;

            context.increment_step();
            kms.increment_step();

            let result = match tc.name.as_str() {
                "read" => {
                    let path = tc.args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                    let start_line = tc
                        .args
                        .get("start_line")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as usize);
                    let end_line = tc
                        .args
                        .get("end_line")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as usize);
                    interceptor.handle_read(
                        path,
                        start_line,
                        end_line,
                        &mut context,
                        &mut kms,
                        &shizuka_paths,
                    )
                }
                "edit" => {
                    let path = tc.args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                    let old_text = tc
                        .args
                        .get("old_text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let new_text = tc
                        .args
                        .get("new_text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    interceptor.handle_edit(path, old_text, new_text, &mut context, &mut kms)
                }
                "create_file" => {
                    let path = tc.args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                    let content = tc
                        .args
                        .get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    interceptor.handle_create_file(path, content, &mut context, &mut kms)
                }
                "write" => {
                    let path = tc.args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                    let content = tc
                        .args
                        .get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    interceptor.handle_write(path, content, &mut context, &mut kms)
                }
                "grep" => {
                    let pattern = tc
                        .args
                        .get("pattern")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let path = tc.args.get("path").and_then(|v| v.as_str());
                    interceptor.handle_grep(pattern, path)
                }
                "grep_file" => {
                    let path = tc.args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                    let pattern = tc
                        .args
                        .get("pattern")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let context_lines = tc
                        .args
                        .get("context_lines")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as usize);
                    interceptor.handle_grep_file(path, pattern, context_lines)
                }
                "execute" => {
                    let command = tc
                        .args
                        .get("command")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let working_dir = tc.args.get("working_dir").and_then(|v| v.as_str());
                    let timeout = tc.args.get("timeout").and_then(|v| v.as_u64());
                    interceptor.handle_execute(command, working_dir, timeout, &mut kms)
                }
                "list_dir" => {
                    let path = tc.args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                    interceptor.handle_list_dir(path)
                }
                "search_web" => {
                    let query = tc.args.get("query").and_then(|v| v.as_str()).unwrap_or("");
                    crate::agent::tools::ToolResult {
                        output: format!("Web search for \"{}\": This feature requires internet access. Try rephrasing your approach using local files.", query),
                        is_error: false,
                    }
                }
                _ => crate::agent::tools::ToolResult {
                    output: format!("Unknown tool: {}", tc.name),
                    is_error: true,
                },
            };

            loop_detector.record_call(
                &tc.name,
                &tc.args.to_string(),
                tc.args.get("path").and_then(|v| v.as_str()),
                safe_prefix(&result.output, 100),
            );

            let _ = tx
                .send(AgentEvent::ToolComplete {
                    name: tc.name.clone(),
                    result: result.output.clone(),
                    is_error: result.is_error,
                })
                .await;

            messages.push(LlmMessage::ToolResult {
                tool_call_id: tc.id.clone(),
                name: tc.name.clone(),
                content: result.output,
            });
        }

        // Context compaction
        if !turn_text.is_empty() {
            context.reset_ttl_for_referenced(&turn_text);
        }

        let compaction_targets = context.get_compaction_targets();
        compact_messages(&mut messages, &compaction_targets, &interceptor);

        if context.is_over_budget() {
            let force_targets = context.force_compact_all();
            compact_messages(&mut messages, &force_targets, &interceptor);
        }

        // Forward rate limits if present
        // (Streaming provides Usage events above; non-streaming response.rate_limits handled in shizuka)

        // Loop detection
        match loop_detector.check() {
            LoopIntervention::None => {}
            LoopIntervention::Message(msg) => {
                messages.push(LlmMessage::User(msg));
            }
            LoopIntervention::BudgetExhausted(msg) => {
                messages.push(LlmMessage::User(msg));
            }
        }

        let _ = tx
            .send(AgentEvent::ContextUpdate(context.context_percent()))
            .await;
    }

    for (path, knowledge) in &interceptor.file_cache {
        if !knowledge.compact_summary.is_empty() {
            kms.file_descriptions
                .insert(path.clone(), knowledge.compact_summary.clone());
        }
    }

    Ok((full_response_text, kms))
}

/// Replace tool result content with compact summaries for files that need compaction.
fn compact_messages(
    messages: &mut [LlmMessage],
    targets: &[(String, String)],
    interceptor: &ToolInterceptor,
) {
    for (path, summary) in targets {
        if let Some(cached) = interceptor.file_cache.get(path) {
            let prefix = safe_prefix(&cached.content, 100);
            for msg in messages.iter_mut() {
                if let LlmMessage::ToolResult { content, .. } = msg {
                    if content.contains(prefix) {
                        *content = summary.clone();
                    }
                }
            }
        }
    }
}
