use crate::memory::kms::TaskClassification;
use crate::memory::kpms::Kpms;
use crate::shizuka::preparation::PreparationResult;
use std::path::Path;

pub fn build_nano_context(
    prep: &PreparationResult,
    project_dir: &Path,
    kpms: &Kpms,
) -> String {
    let mut context = String::new();

    // Task description
    context.push_str(&format!("Task: {}\n\n", prep.task_summary));

    // Project context for non-trivial tasks
    if prep.task_classification != TaskClassification::Trivial {
        context.push_str("Project context:\n");
        if !kpms.project.language.is_empty() {
            context.push_str(&format!("- Language: {}\n", kpms.project.language));
        }
        if !kpms.project.framework.is_empty() {
            context.push_str(&format!("- Framework: {}\n", kpms.project.framework));
        }
        if !kpms.project.package_manager.is_empty() {
            context.push_str(&format!("- Package manager: {}\n", kpms.project.package_manager));
        }
        for pattern in &kpms.architecture.patterns {
            context.push_str(&format!("- {}\n", pattern));
        }
        context.push('\n');
    }

    // Suggested approach
    if let Some(approach) = &prep.suggested_approach {
        context.push_str(&format!("Suggested approach: {}\n\n", approach));
    }

    // Relevant learnings
    if !prep.relevant_learnings.is_empty() {
        context.push_str("Relevant learnings from previous sessions:\n");
        for learning in &prep.relevant_learnings {
            context.push_str(&format!("- {}\n", learning));
        }
        context.push('\n');
    }

    // Warnings
    if !prep.relevant_warnings.is_empty() {
        context.push_str("Warnings:\n");
        for warning in &prep.relevant_warnings {
            context.push_str(&format!("⚠ {}\n", warning));
        }
        context.push('\n');
    }

    // Preloaded files
    for file_path in &prep.files_to_preload {
        let full_path = project_dir.join(file_path);
        match std::fs::read_to_string(&full_path) {
            Ok(content) => {
                context.push_str(&format!("File: {}\n", file_path));
                context.push_str(&content);
                context.push_str("\n\n");
            }
            Err(e) => {
                context.push_str(&format!("File: {} (could not read: {})\n\n", file_path, e));
            }
        }
    }

    // Reference files
    for file_path in &prep.files_to_reference {
        let full_path = project_dir.join(file_path);
        match std::fs::read_to_string(&full_path) {
            Ok(content) => {
                context.push_str(&format!("Reference: {}\n", file_path));
                context.push_str(&content);
                context.push_str("\n\n");
            }
            Err(e) => {
                context.push_str(&format!("Reference: {} (could not read: {})\n\n", file_path, e));
            }
        }
    }

    // Sub-tasks for medium/large
    if !prep.kms_updates.sub_tasks.is_empty() {
        context.push_str("Sub-tasks:\n");
        for (i, task) in prep.kms_updates.sub_tasks.iter().enumerate() {
            context.push_str(&format!("{}. {}\n", i + 1, task));
        }
        context.push('\n');
    }

    context
}
