use super::{ToolResult, ToolResultMetadata};
use crate::memory::kms::Kms;
use std::collections::HashSet;

const MAX_DEPTH: usize = 2;
const MAX_CONCURRENT: usize = 4;

pub struct SummonRequest {
    pub task: String,
    pub files: Vec<String>,
}

pub struct SummonResult {
    pub tool_result: ToolResult,
    pub modified_files: Vec<String>,
}

pub fn validate_summon(
    request: &SummonRequest,
    kms: &Kms,
    current_depth: usize,
    active_sub_agents: usize,
) -> Result<(), String> {
    if current_depth >= MAX_DEPTH {
        return Err(format!(
            "Maximum nesting depth ({}) reached. Sub-agents cannot spawn further sub-agents.",
            MAX_DEPTH
        ));
    }

    if active_sub_agents >= MAX_CONCURRENT {
        return Err(format!(
            "Maximum concurrent sub-agents ({}) reached. Wait for existing sub-agents to complete.",
            MAX_CONCURRENT
        ));
    }

    // Check file locks
    for file in &request.files {
        if let Some(locked_by) = kms.files.locks.get(file) {
            return Err(format!(
                "Cannot delegate — '{}' is currently locked by sub-agent '{}'.",
                file, locked_by
            ));
        }
    }

    Ok(())
}

pub fn acquire_file_locks(kms: &mut Kms, files: &[String], agent_id: &str) {
    for file in files {
        kms.files.locks.insert(file.clone(), agent_id.to_string());
    }
}

pub fn release_file_locks(kms: &mut Kms, files: &[String]) {
    for file in files {
        kms.files.locks.remove(file);
    }
}

pub fn check_file_overlap(request_files: &[String], parent_active_files: &[String]) -> Vec<String> {
    let parent_set: HashSet<&str> = parent_active_files.iter().map(|s| s.as_str()).collect();
    request_files
        .iter()
        .filter(|f| parent_set.contains(f.as_str()))
        .cloned()
        .collect()
}

pub fn format_summon_result(
    task: &str,
    modified_files: &[String],
    _approach: &str,
    tests_passing: bool,
    notes: &str,
) -> String {
    let mut output = format!("✓ Sub-agent completed: \"{}\"\n", task);
    if !modified_files.is_empty() {
        output.push_str("  Modified files:\n");
        for f in modified_files {
            output.push_str(&format!("    - {}\n", f));
        }
    }
    if tests_passing {
        output.push_str("  Tests: all passing\n");
    }
    if !notes.is_empty() {
        output.push_str(&format!("  Notes: {}\n", notes));
    }
    output
}
