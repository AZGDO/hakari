use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::sync::mpsc;

use crate::agent::llm::{build_tool_def, LlmClient, LlmMessage, ToolDef};
use crate::agent::AgentEvent;
use crate::copilot::CopilotRateLimits;

pub const SHIZUKA_EXPLORE_PROMPT: &str = r#"You are a preparation agent for an autonomous coding agent. Your job is to explore the codebase and gather everything needed to complete a coding task.

You have four tools:
- shizuka_read(path): read a file's full content — use only for small files (< 100 lines)
- shizuka_read_lines(path, start_line, end_line): read specific line range — preferred for large files
- shizuka_grep(pattern, path?): search with ripgrep — USE THIS FIRST to locate relevant code
- shizuka_list_dir(path): list directory contents

WORKFLOW:
1. Use shizuka_grep() FIRST to find where relevant code lives.
2. Use shizuka_read_lines() to inspect the specific regions found by grep.
3. Only use shizuka_read() for small files or when you need full context.
4. Track which line ranges are relevant for each file — you will report these as focus_regions.
5. Identify build/test/lint commands by checking for Makefile, package.json scripts, Cargo.toml, pyproject.toml, etc.
6. Note any configuration files, dependency files, or project conventions that are relevant.

STRICT RULES:
1. Read each file AT MOST ONCE. If you have already read a file, do not call shizuka_read on it again.
2. Stop exploring as soon as you know which files need modification and what their dependencies are.
3. Do not read files unrelated to the task.
4. After reading the key files, STOP calling tools immediately.
5. Always check for existing tests related to the files being modified.
6. Identify the project's language, build system, and test framework so the coding agent can verify its work.

When you stop calling tools, the system will ask you for a structured report."#;

pub const SHIZUKA_STRUCTURE_PROMPT: &str = r#"Based on your exploration above, produce a structured preparation report for the coding agent.

List the relevant files in context_files. Do NOT include file content — it is already cached.
For each file specify: path, role (modify/reference/context), compact_summary, annotations, and focus_regions.

focus_regions is critical for files with role "modify": list the specific line ranges the agent needs to change.
Each focus_region has start_line (1-based), end_line (1-based), and a description of what's there.
This allows the agent to read only the relevant parts instead of entire files.

In the "approach" field, include:
1. The step-by-step plan for making changes.
2. Build/test/lint commands the agent should run to verify changes (e.g. "cargo check", "npm test", "pytest").
3. Any conventions or patterns the agent must follow based on existing code.

In "warnings", include:
- Files that are tightly coupled and must be changed together.
- Any tricky edge cases or gotchas you noticed in the code.
- Dependencies or imports that may need updating.

In "learnings", include:
- Project language, build system, and test framework.
- Coding style conventions observed (naming, error handling patterns, etc.).
- Relevant configuration or environment details.

For direct questions (not code changes), put the answer in direct_answer and leave context_files empty."#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preparation {
    pub task_classification: String,
    pub task_summary: String,
    pub direct_answer: Option<String>,
    pub context_files: Vec<ContextFile>,
    pub approach: Option<String>,
    #[serde(default)]
    pub learnings: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub sub_tasks: Vec<SubTaskDef>,
    /// Optional diagnostic information when parsing was tolerant or repaired
    #[serde(default)]
    pub parsing_warnings: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusRegion {
    pub start_line: usize,
    pub end_line: usize,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextFile {
    pub path: String,
    pub role: String,
    #[serde(default)]
    pub content: String,
    pub compact_summary: String,
    #[serde(default)]
    pub annotations: String,
    #[serde(default)]
    pub focus_regions: Vec<FocusRegion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTaskDef {
    pub description: String,
}

fn preparation_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "task_classification": {
                "type": "string",
                "enum": ["trivial", "small", "medium", "large"]
            },
            "task_summary": { "type": "string" },
            "direct_answer": { "type": "string" },
            "context_files": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "path":            { "type": "string" },
                        "role":            { "type": "string", "enum": ["modify", "reference", "context"] },
                        "compact_summary": { "type": "string" },
                        "annotations":     { "type": "string" },
                        "focus_regions":   {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "start_line":   { "type": "integer" },
                                    "end_line":     { "type": "integer" },
                                    "description":  { "type": "string" }
                                },
                                "required": ["start_line", "end_line", "description"]
                            }
                        }
                    },
                    "required": ["path", "role", "compact_summary"]
                }
            },
            "approach":  { "type": "string" },
            "learnings": { "type": "array", "items": { "type": "string" } },
            "warnings":  { "type": "array", "items": { "type": "string" } },
            "sub_tasks": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": { "description": { "type": "string" } },
                    "required": ["description"]
                }
            }
        },
        "required": ["task_classification", "task_summary", "context_files"]
    })
}

pub fn shizuka_tools() -> Vec<ToolDef> {
    vec![
        build_tool_def(
            "shizuka_read",
            "Read a file from the project. Returns full content. Prefer shizuka_read_lines for large files.",
            json!({
                "path": {"type": "string", "description": "File path relative to project root"}
            }),
            vec!["path"],
        ),
        build_tool_def(
            "shizuka_read_lines",
            "Read specific line range from a file. Preferred over shizuka_read for large files. Use after grep to inspect matches.",
            json!({
                "path": {"type": "string", "description": "File path relative to project root"},
                "start_line": {"type": "integer", "description": "1-based start line"},
                "end_line": {"type": "integer", "description": "1-based end line (inclusive)"}
            }),
            vec!["path", "start_line", "end_line"],
        ),
        build_tool_def(
            "shizuka_grep",
            "Search the codebase for a pattern using ripgrep. USE THIS FIRST to locate relevant code before reading.",
            json!({
                "pattern": {"type": "string", "description": "Search pattern (regex supported)"},
                "path":    {"type": "string", "description": "Optional: directory to search in"}
            }),
            vec!["pattern"],
        ),
        build_tool_def(
            "shizuka_list_dir",
            "List directory contents with file sizes.",
            json!({
                "path": {"type": "string", "description": "Directory path relative to project root. Use '.' for root."}
            }),
            vec!["path"],
        ),
    ]
}

pub fn build_shizuka_user_message(
    user_request: &str,
    file_tree: &str,
    kpms_context: &str,
    kkm_context: &str,
    kms_context: &str,
) -> String {
    format!(
        "User request: {}\n\nProject file tree:\n{}\n\nProject memory (KPMS):\n{}\n\nDevice info (KKM):\n{}\n\nSession state (KMS):\n{}",
        user_request, file_tree, kpms_context, kkm_context, kms_context
    )
}

pub async fn run_shizuka(
    client: &dyn LlmClient,
    model: &str,
    user_message: &str,
    project_root: &str,
    tx: &mpsc::Sender<AgentEvent>,
) -> Result<Preparation, String> {
    let _ = tx.send(AgentEvent::PhaseChange("shizuka".into())).await;

    let tools = shizuka_tools();
    let mut messages: Vec<LlmMessage> = vec![
        LlmMessage::System(SHIZUKA_EXPLORE_PROMPT.to_string()),
        LlmMessage::User(user_message.to_string()),
    ];

    let mut file_cache: HashMap<String, String> = HashMap::new();
    let mut files_shown: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Phase 1: exploration loop
    let max_turns = 20;
    for _turn in 0..max_turns {
        let response = client.generate(model, &messages, &tools).await?;

        // Forward rate limits if present (Copilot provides these)
        if let Some(ref rl) = response.rate_limits {
            let _ = tx
                .send(AgentEvent::CopilotRateLimitUpdate(CopilotRateLimits {
                    total: rl.total,
                    remaining: rl.remaining,
                    reset_at: rl.reset_at,
                }))
                .await;
        }

        messages.push(LlmMessage::Assistant {
            text: response.text.clone(),
            tool_calls: response.tool_calls.clone(),
        });

        if response.tool_calls.is_empty() {
            break;
        }

        for tc in &response.tool_calls {
            let _ = tx
                .send(AgentEvent::ShizukaToolCall {
                    name: tc.name.clone(),
                    args: tc.args.to_string(),
                })
                .await;

            let result = if tc.name == "shizuka_read" || tc.name == "shizuka_read_lines" {
                let path = tc
                    .args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if tc.name == "shizuka_read" && files_shown.contains(&path) {
                    "(already read \u{2014} do not read again)".to_string()
                } else {
                    if tc.name == "shizuka_read" {
                        files_shown.insert(path.clone());
                    }
                    let content =
                        execute_shizuka_tool(&tc.name, &tc.args, project_root, &mut file_cache);
                    truncate_for_model(&content, 8192)
                }
            } else {
                execute_shizuka_tool(&tc.name, &tc.args, project_root, &mut file_cache)
            };

            messages.push(LlmMessage::ToolResult {
                tool_call_id: tc.id.clone(),
                name: tc.name.clone(),
                content: result,
            });
        }
    }

    // Phase 2: structured output — swap system prompt for structure instructions
    if let Some(LlmMessage::System(_)) = messages.first() {
        messages[0] = LlmMessage::System(SHIZUKA_STRUCTURE_PROMPT.to_string());
    }
    messages.push(LlmMessage::User(
        "Now produce the structured preparation report. Do not include file contents.".to_string(),
    ));

    let schema = preparation_schema();
    let json_text = client
        .generate_structured(model, &messages, &schema)
        .await?;

    // Tolerant parsing pipeline
    // 1) Strict parse
    let mut diagnostics: Vec<String> = Vec::new();
    match parse_strict(&json_text) {
        Ok(mut prep) => {
            // Attach file content from cache
            for cf in &mut prep.context_files {
                if cf.content.is_empty() {
                    if let Some(cached) = file_cache.get(&cf.path) {
                        cf.content = cached.clone();
                    } else {
                        cf.content = read_file_from_root(project_root, &cf.path);
                    }
                }
            }
            return Ok(prep);
        }
        Err(e_strict) => {
            diagnostics.push(format!("Strict parse failed: {}", e_strict));
        }
    }

    // 2) Parse as generic Value and map tolerantly
    if let Ok(val) = serde_json::from_str::<Value>(&json_text) {
        match parse_value_map_to_preparation(&val) {
            Ok(mut prep) => {
                diagnostics.push("Parsed from Value with tolerant mapping".into());
                prep.parsing_warnings = Some(diagnostics.join("\n"));
                // attach cached contents
                for cf in &mut prep.context_files {
                    if cf.content.is_empty() {
                        if let Some(cached) = file_cache.get(&cf.path) {
                            cf.content = cached.clone();
                        } else {
                            cf.content = read_file_from_root(project_root, &cf.path);
                        }
                    }
                }
                return Ok(prep);
            }
            Err(e_map) => {
                diagnostics.push(format!("Value mapping failed: {}", e_map));
            }
        }
    } else {
        diagnostics.push("Failed to parse as generic JSON Value".into());
    }

    // 3) Try to extract a JSON block and normalize
    if let Some(extracted) = try_extract_json_block(&json_text) {
        let normalized = normalize_json_text(&extracted);
        if let Ok(mut prep) = parse_strict(&normalized) {
            diagnostics.push("Recovered by extracting JSON block".into());
            prep.parsing_warnings = Some(diagnostics.join("\n"));
            for cf in &mut prep.context_files {
                if cf.content.is_empty() {
                    if let Some(cached) = file_cache.get(&cf.path) {
                        cf.content = cached.clone();
                    } else {
                        cf.content = read_file_from_root(project_root, &cf.path);
                    }
                }
            }
            return Ok(prep);
        }
        if let Ok(val) = serde_json::from_str::<Value>(&normalized) {
            if let Ok(mut prep) = parse_value_map_to_preparation(&val) {
                diagnostics.push("Recovered by extracting and normalizing JSON block".into());
                prep.parsing_warnings = Some(diagnostics.join("\n"));
                for cf in &mut prep.context_files {
                    if cf.content.is_empty() {
                        if let Some(cached) = file_cache.get(&cf.path) {
                            cf.content = cached.clone();
                        } else {
                            cf.content = read_file_from_root(project_root, &cf.path);
                        }
                    }
                }
                return Ok(prep);
            }
        }
    } else {
        diagnostics.push("No JSON block could be extracted".into());
    }

    // 4) LLM repair attempts (limited)
    let mut repair_attempts = 0u8;
    let max_repairs = 2u8;
    while repair_attempts < max_repairs {
        repair_attempts += 1;
        diagnostics.push(format!("Attempting LLM repair (attempt {})", repair_attempts));
        match request_json_repair_via_llm(client, model, &messages, &json_text).await {
            Ok(repaired_text) => {
                // try strict
                if let Ok(mut prep) = parse_strict(&repaired_text) {
                    diagnostics.push("Repaired by LLM (strict parse)".into());
                    prep.parsing_warnings = Some(diagnostics.join("\n"));
                    for cf in &mut prep.context_files {
                        if cf.content.is_empty() {
                            if let Some(cached) = file_cache.get(&cf.path) {
                                cf.content = cached.clone();
                            } else {
                                cf.content = read_file_from_root(project_root, &cf.path);
                            }
                        }
                    }
                    return Ok(prep);
                }
                if let Ok(val) = serde_json::from_str::<Value>(&repaired_text) {
                    if let Ok(mut prep) = parse_value_map_to_preparation(&val) {
                        diagnostics.push("Repaired by LLM (tolerant mapping)".into());
                        prep.parsing_warnings = Some(diagnostics.join("\n"));
                        for cf in &mut prep.context_files {
                            if cf.content.is_empty() {
                                if let Some(cached) = file_cache.get(&cf.path) {
                                    cf.content = cached.clone();
                                } else {
                                    cf.content = read_file_from_root(project_root, &cf.path);
                                }
                            }
                        }
                        return Ok(prep);
                    }
                }
                diagnostics.push("LLM repair did not yield valid Preparation".into());
            }
            Err(e) => {
                diagnostics.push(format!("LLM repair request failed: {}", e));
                break;
            }
        }
    }

    // Fallback: best-effort Preparation using cached files
    diagnostics.push("Falling back to best-effort Preparation".into());
    let mut context_files: Vec<ContextFile> = Vec::new();
    for (path, content) in file_cache.iter() {
        context_files.push(ContextFile {
            path: path.clone(),
            role: "context".into(),
            content: content.clone(),
            compact_summary: "(cached content)".into(),
            annotations: String::new(),
            focus_regions: Vec::new(),
        });
    }
    let prep = Preparation {
        task_classification: "small".into(),
        task_summary: "Partial preparation due to malformed model output".into(),
        direct_answer: None,
        context_files,
        approach: None,
        learnings: Vec::new(),
        warnings: vec!["Parsing failed; returned partial preparation".into()],
        sub_tasks: Vec::new(),
        parsing_warnings: Some(diagnostics.join("\n\nOriginal assistant output:\n") + &json_text),
    };

    Ok(prep)
}


// ── Local tool execution (shared, not provider-specific) ────────────────────

fn truncate_for_model(content: &str, max_bytes: usize) -> String {
    if content.len() <= max_bytes {
        return content.to_string();
    }
    let mut end = max_bytes;
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\n... (truncated \u{2014} full content cached)",
        &content[..end]
    )
}

// ---------------- Tolerant parsing helpers ----------------

fn parse_strict(text: &str) -> Result<Preparation, String> {
    serde_json::from_str::<Preparation>(text).map_err(|e| format!("strict parse error: {}", e))
}

fn parse_value_map_to_preparation(val: &Value) -> Result<Preparation, String> {
    let mut diag: Vec<String> = Vec::new();
    // Expect object
    let obj = match val.as_object() {
        Some(o) => o,
        None => {
            // If top-level array, assume it's context_files
            if let Some(arr) = val.as_array() {
                diag.push("Top-level array found; treating as context_files".into());
                let mut context_files = Vec::new();
                for v in arr {
                    if let Some(p) = v.get("path").and_then(|x| x.as_str()) {
                        let role = v
                            .get("role")
                            .and_then(|x| x.as_str())
                            .unwrap_or("context")
                            .to_string();
                        let compact = v
                            .get("compact_summary")
                            .and_then(|x| x.as_str())
                            .unwrap_or("(no summary)")
                            .to_string();
                        context_files.push(ContextFile {
                            path: p.to_string(),
                            role,
                            content: String::new(),
                            compact_summary: compact,
                            annotations: v
                                .get("annotations")
                                .and_then(|x| x.as_str())
                                .unwrap_or("")
                                .to_string(),
                            focus_regions: Vec::new(),
                        });
                    }
                }
                let prep = Preparation {
                    task_classification: "small".into(),
                    task_summary: "Partial prep from array output".into(),
                    direct_answer: None,
                    context_files,
                    approach: None,
                    learnings: Vec::new(),
                    warnings: Vec::new(),
                    sub_tasks: Vec::new(),
                    parsing_warnings: Some(diag.join("\n")),
                };
                return Ok(prep);
            }
            return Err("value is not an object or array".into());
        }
    };

    // helpers to coerce
    fn coerce_string(
        obj: &serde_json::Map<String, Value>,
        k: &str,
        diag: &mut Vec<String>,
    ) -> Option<String> {
        obj.get(k).and_then(|v| {
            if v.is_string() {
                v.as_str().map(|s| s.to_string())
            } else if v.is_array() {
                // join array elements
                let parts: Vec<String> = v
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|e| e.as_str().map(|s| s.to_string()))
                    .collect();
                if !parts.is_empty() {
                    diag.push(format!("Coerced array -> string for key {} by joining", k));
                    Some(parts.join(" "))
                } else {
                    None
                }
            } else if v.is_number() || v.is_boolean() {
                Some(v.to_string())
            } else {
                None
            }
        })
    }

    let task_classification = coerce_string(obj, "task_classification", &mut diag).unwrap_or_else(|| {
        diag.push("Missing task_classification; defaulting to 'small'".into());
        "small".into()
    });
    let task_summary = coerce_string(obj, "task_summary", &mut diag).unwrap_or_else(|| {
        diag.push("Missing task_summary; defaulting".into());
        "(no summary provided)".into()
    });
    let direct_answer = obj.get("direct_answer").and_then(|v| {
        if v.is_string() {
            v.as_str().map(|s| s.to_string())
        } else if v.is_array() {
            let parts: Vec<String> = v
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|e| e.as_str().map(|s| s.to_string()))
                .collect();
            if !parts.is_empty() {
                diag.push("Coerced direct_answer array -> string".into());
                Some(parts.join(" "))
            } else {
                None
            }
        } else {
            None
        }
    });

    // context_files
    let mut context_files: Vec<ContextFile> = Vec::new();
    if let Some(cf_val) = obj.get("context_files") {
        if let Some(arr) = cf_val.as_array() {
            for item in arr {
                if let Some(it) = item.as_object() {
                    let path = it
                        .get("path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("(unknown)")
                        .to_string();
                    let role = it
                        .get("role")
                        .and_then(|v| v.as_str())
                        .unwrap_or("context")
                        .to_string();
                    let compact_summary = it
                        .get("compact_summary")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let annotations = it
                        .get("annotations")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let mut focus_regions = Vec::new();
                    if let Some(fr_arr) = it.get("focus_regions").and_then(|v| v.as_array()) {
                        for fr in fr_arr {
                            if let Some(fr_obj) = fr.as_object() {
                                let start_line = fr_obj
                                    .get("start_line")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(1) as usize;
                                let end_line = fr_obj
                                    .get("end_line")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(start_line as u64)
                                    as usize;
                                let description = fr_obj
                                    .get("description")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                focus_regions.push(FocusRegion {
                                    start_line,
                                    end_line,
                                    description,
                                });
                            }
                        }
                    }
                    context_files.push(ContextFile {
                        path,
                        role,
                        content: String::new(),
                        compact_summary,
                        annotations,
                        focus_regions,
                    });
                }
            }
        }
    }

    let approach = obj.get("approach").and_then(|v| v.as_str().map(|s| s.to_string()));
    let learnings = obj
        .get("learnings")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_else(Vec::new);
    let warnings = obj
        .get("warnings")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_else(Vec::new);
    let sub_tasks = obj
        .get("sub_tasks")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.get("description").and_then(|d| d.as_str()).map(|s| {
                    SubTaskDef {
                        description: s.to_string(),
                    }
                }))
                .collect()
        })
        .unwrap_or_else(Vec::new);

    let prep = Preparation {
        task_classification,
        task_summary,
        direct_answer,
        context_files,
        approach,
        learnings,
        warnings,
        sub_tasks,
        parsing_warnings: Some(diag.join("\n")),
    };

    Ok(prep)
}

fn try_extract_json_block(s: &str) -> Option<String> {
    // Remove markdown fences and look for first balanced {..} or [..]
    let cleaned = s
        .replace("```json", "")
        .replace("```", "")
        .replace("\r", "");

    // Try find '{' block
    if let Some(start) = cleaned.find('{') {
        let mut depth = 0i32;
        for (i, ch) in cleaned.chars().enumerate().skip(start) {
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    return Some(cleaned[start..=i].to_string());
                }
            }
        }
        // fallback: take from first '{' to last '}'
        if let Some(last) = cleaned.rfind('}') {
            if last > start {
                return Some(cleaned[start..=last].to_string());
            }
        }
    }
    // Try array
    if let Some(start) = cleaned.find('[') {
        let mut depth = 0i32;
        for (i, ch) in cleaned.chars().enumerate().skip(start) {
            if ch == '[' {
                depth += 1;
            } else if ch == ']' {
                depth -= 1;
                if depth == 0 {
                    return Some(cleaned[start..=i].to_string());
                }
            }
        }
        if let Some(last) = cleaned.rfind(']') {
            if last > start {
                return Some(cleaned[start..=last].to_string());
            }
        }
    }
    None
}

fn normalize_json_text(s: &str) -> String {
    let mut out = s.to_string();
    // Remove JS-style comments
    out = out
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//") && !t.starts_with("/*") && !t.starts_with("* ")
        })
        .collect::<Vec<&str>>()
        .join("\n");
    // Remove trailing commas before } or ]
    out = out.replace(",\n}", "\n}");
    out = out.replace(",\n]", "\n]");
    // Trim fences
    out = out.trim().to_string();
    out
}

async fn request_json_repair_via_llm(
    client: &dyn LlmClient,
    model: &str,
    previous_messages: &[LlmMessage],
    previous_output: &str,
) -> Result<String, String> {
    let repair_prompt = "You previously returned invalid JSON. Using the Preparation schema, output only valid JSON matching the schema. Do not include any explanations.";
    let mut msgs: Vec<LlmMessage> = vec![LlmMessage::System(repair_prompt.to_string())];
    // include previous assistant content for context
    msgs.push(LlmMessage::User(format!(
        "Previous assistant output:\n\n{}",
        previous_output
    )));
    // Also include a compact trace of earlier messages (user asks) if available
    if let Some(LlmMessage::User(u)) = previous_messages.iter().find(|m| matches!(m, LlmMessage::User(_))) {
        msgs.push(LlmMessage::User(format!("User request context:\n{}", u)));
    }
    let resp = client.generate(model, &msgs, &[]).await?;
    Ok(resp.text)
}

fn read_file_from_root(project_root: &str, path: &str) -> String {
    let full = std::path::Path::new(project_root).join(path);
    std::fs::read_to_string(&full).unwrap_or_else(|e| format!("(could not read {}: {})", path, e))
}

fn execute_shizuka_tool(
    name: &str,
    args: &Value,
    project_root: &str,
    file_cache: &mut HashMap<String, String>,
) -> String {
    let root = std::path::Path::new(project_root);

    match name {
        "shizuka_read" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(cached) = file_cache.get(path) {
                return cached.clone();
            }
            let full_path = root.join(path);
            let content = match std::fs::read_to_string(&full_path) {
                Ok(c) => c,
                Err(e) => format!("Error reading {}: {}", path, e),
            };
            if !content.starts_with("Error reading") {
                file_cache.insert(path.to_string(), content.clone());
            }
            content
        }
        "shizuka_read_lines" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let start = args.get("start_line").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
            let end = args
                .get("end_line")
                .and_then(|v| v.as_u64())
                .unwrap_or(start as u64 + 50) as usize;
            let full_path = root.join(path);

            let content = if let Some(cached) = file_cache.get(path) {
                cached.clone()
            } else {
                match std::fs::read_to_string(&full_path) {
                    Ok(c) => {
                        file_cache.insert(path.to_string(), c.clone());
                        c
                    }
                    Err(e) => return format!("Error reading {}: {}", path, e),
                }
            };

            let lines: Vec<&str> = content.lines().collect();
            let total = lines.len();
            let s = start.saturating_sub(1).min(total);
            let e = end.min(total);
            if s >= e {
                return format!(
                    "Invalid range: lines {}-{} (file has {} lines)",
                    start, end, total
                );
            }

            let mut output = format!(
                "Lines {}-{} of {} ({} total lines):\n",
                start, end, path, total
            );
            for (i, line) in lines[s..e].iter().enumerate() {
                output.push_str(&format!("{:>4} | {}\n", s + i + 1, line));
            }
            output
        }
        "shizuka_grep" => {
            let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
            let search_path = args.get("path").and_then(|v| v.as_str());
            let search_dir = match search_path {
                Some(p) if !p.is_empty() => root.join(p),
                _ => root.to_path_buf(),
            };
            match std::process::Command::new("rg")
                .args([
                    "--line-number",
                    "--no-heading",
                    "--color=never",
                    "--max-count=50",
                ])
                .arg(pattern)
                .arg(&search_dir)
                .output()
            {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    if stdout.is_empty() {
                        format!("No matches for \"{}\"", pattern)
                    } else {
                        stdout.to_string()
                    }
                }
                Err(e) => format!("Grep error: {}", e),
            }
        }
        "shizuka_list_dir" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let full_path = root.join(path);
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
                                format!("{} ({})", name, format_size(size))
                            }
                        })
                        .collect();
                    items.sort();
                    items.join("\n")
                }
                Err(e) => format!("Error listing {}: {}", path, e),
            }
        }
        _ => format!("Unknown tool: {}", name),
    }
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{}B", bytes)
    }
}

pub fn scan_file_tree(project_root: &str) -> String {
    let root = std::path::Path::new(project_root);
    let mut tree = String::new();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .max_depth(Some(5))
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        if let Ok(rel) = path.strip_prefix(root) {
            let rel_str = rel.display().to_string();
            if rel_str.is_empty() {
                continue;
            }
            if rel_str.starts_with("target/") || rel_str.starts_with("node_modules/") {
                continue;
            }
            let depth = rel.components().count();
            let indent = "  ".repeat(depth.saturating_sub(1));
            let name = path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default();
            if path.is_dir() {
                tree.push_str(&format!("{}{}/\n", indent, name));
            } else {
                tree.push_str(&format!("{}{}\n", indent, name));
            }
        }
    }

    if tree.is_empty() {
        "(empty or unreadable project)".into()
    } else {
        tree
    }
}
