use crate::llm::client::LlmClient;
use crate::llm::messages::Message;
use crate::memory::kkm::Kkm;
use crate::memory::kms::{Kms, TaskClassification};
use crate::memory::kpms::Kpms;
use crate::project::file_tree;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparationResult {
    pub task_classification: TaskClassification,
    pub task_summary: String,
    pub files_to_preload: Vec<String>,
    pub files_to_reference: Vec<String>,
    pub suggested_approach: Option<String>,
    pub relevant_learnings: Vec<String>,
    pub relevant_warnings: Vec<String>,
    pub kms_updates: KmsUpdates,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KmsUpdates {
    pub goal: String,
    pub sub_tasks: Vec<String>,
}

pub fn try_fast_path(prompt: &str, project_dir: &Path) -> Option<PreparationResult> {
    let file_pattern = regex::Regex::new(r"(?:^|\s)(\S+\.\w{1,10})(?:\s|$|,|\.|\))").ok()?;
    let mut mentioned_files = Vec::new();

    for cap in file_pattern.captures_iter(prompt) {
        let file_ref = cap[1].to_string();
        let full_path = project_dir.join(&file_ref);
        if full_path.exists() {
            mentioned_files.push(file_ref);
        }
    }

    if mentioned_files.is_empty() {
        return None;
    }

    // Check for trivial task patterns
    let trivial_patterns = [
        "fix typo",
        "fix the typo",
        "rename",
        "change the name",
        "update the comment",
        "add a comment",
        "remove the comment",
        "fix whitespace",
        "fix formatting",
        "fix indent",
    ];

    let prompt_lower = prompt.to_lowercase();
    let is_trivial = trivial_patterns.iter().any(|p| prompt_lower.contains(p))
        || (mentioned_files.len() == 1 && prompt.len() < 100);

    let classification = if is_trivial {
        TaskClassification::Trivial
    } else if mentioned_files.len() <= 2 {
        TaskClassification::Small
    } else {
        return None; // Not suitable for fast path
    };

    Some(PreparationResult {
        task_classification: classification,
        task_summary: prompt.chars().take(200).collect(),
        files_to_preload: mentioned_files,
        files_to_reference: Vec::new(),
        suggested_approach: None,
        relevant_learnings: Vec::new(),
        relevant_warnings: Vec::new(),
        kms_updates: KmsUpdates {
            goal: prompt.to_string(),
            sub_tasks: Vec::new(),
        },
    })
}

pub async fn run_preparation(
    llm_client: &LlmClient,
    prompt: &str,
    kms: &Kms,
    kpms: &Kpms,
    kkm: &Kkm,
    project_dir: &Path,
) -> anyhow::Result<PreparationResult> {
    let file_tree_entries = file_tree::build_file_tree(project_dir, 500);
    let file_tree_str = file_tree::format_file_tree_plain(&file_tree_entries);

    let mut context = String::new();
    context.push_str(&format!("## User Task\n{}\n\n", prompt));

    context.push_str("## Project Info\n");
    context.push_str(&format!("Name: {}\n", kpms.project.name));
    context.push_str(&format!("Type: {}\n", kpms.project.project_type));
    context.push_str(&format!("Language: {}\n", kpms.project.language));
    context.push_str(&format!("Framework: {}\n", kpms.project.framework));
    context.push_str(&format!(
        "Package Manager: {}\n\n",
        kpms.project.package_manager
    ));

    context.push_str(&format!("## File Tree\n{}\n\n", file_tree_str));

    if !kpms.learnings.is_empty() {
        context.push_str("## Project Learnings\n");
        for l in &kpms.learnings {
            context.push_str(&format!("- {}: {}\n", l.context, l.lesson));
        }
        context.push('\n');
    }

    if !kpms.anti_patterns.is_empty() {
        context.push_str("## Warnings\n");
        for ap in &kpms.anti_patterns {
            context.push_str(&format!("- {}: {}\n", ap.pattern, ap.prevention));
        }
        context.push('\n');
    }

    if !kpms.file_index.is_empty() {
        context.push_str("## File Descriptions\n");
        for (path, desc) in &kpms.file_index {
            context.push_str(&format!("- {}: {}\n", path, desc));
        }
        context.push('\n');
    }

    context.push_str(&format!(
        "## Device\nOS: {}, Shell: {}\n\n",
        kkm.system.os, kkm.system.shell
    ));

    if !kms.task.original_prompt.is_empty() {
        context.push_str(&format!(
            "## Current Session State\nGoal: {}\nStep: {}\n\n",
            kms.task.goal, kms.steps.current
        ));
    }

    let system_prompt = r#"You are Shizuka, the preparation engine for a coding agent called Nano. Your job is to analyze the user's task and prepare the optimal context package.

Respond with ONLY a JSON object (no markdown, no code fences) with this exact schema:
{
  "task_classification": "trivial" | "small" | "medium" | "large",
  "task_summary": "one sentence describing the task",
  "files_to_preload": ["paths that Nano will definitely need"],
  "files_to_reference": ["paths Nano might need, load as summary"],
  "suggested_approach": "brief strategy or null if trivial",
  "relevant_learnings": ["pulled from project learnings"],
  "relevant_warnings": ["pulled from warnings — things that have failed before"],
  "kms_updates": {
    "goal": "what we're trying to accomplish",
    "sub_tasks": ["breakdown if not trivial"]
  }
}

Classification guide:
- trivial: single file, obvious fix (typo, rename, simple change)
- small: 1-2 files, clear scope (add validation, fix bug, add feature to one component)
- medium: 3-5+ files, requires understanding relationships (add error handling to module, refactor pattern)
- large: system-wide changes, many files, requires decomposition (migration, major refactor)"#;

    let messages = vec![Message::system(system_prompt), Message::user(&context)];

    let (response_text, _) = llm_client.shizuka_chat(&messages).await?;

    // Parse the JSON response
    let cleaned = response_text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    match serde_json::from_str::<PreparationResult>(cleaned) {
        Ok(result) => Ok(result),
        Err(e) => {
            tracing::warn!(
                "Failed to parse Shizuka preparation response: {}. Raw: {}",
                e,
                cleaned
            );
            // Fallback: create a basic preparation
            Ok(PreparationResult {
                task_classification: TaskClassification::Small,
                task_summary: prompt.chars().take(200).collect(),
                files_to_preload: Vec::new(),
                files_to_reference: Vec::new(),
                suggested_approach: None,
                relevant_learnings: Vec::new(),
                relevant_warnings: Vec::new(),
                kms_updates: KmsUpdates {
                    goal: prompt.to_string(),
                    sub_tasks: Vec::new(),
                },
            })
        }
    }
}
