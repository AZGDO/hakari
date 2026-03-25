use sha2::{Digest, Sha256};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RecentCall {
    pub step: usize,
    pub tool: String,
    pub params_hash: String,
    pub file_path: Option<String>,
    pub result_summary: String,
    pub is_error: bool,
}

pub struct LoopDetector {
    pub recent_calls: Vec<RecentCall>,
    pub write_counts: HashMap<String, Vec<usize>>, // path -> list of steps
    pub max_iterations: usize,
    pub current_step: usize,
    pub expected_modify_files: Vec<String>,
}

pub enum LoopIntervention {
    None,
    Message(String),
    BudgetExhausted(String),
}

impl LoopDetector {
    pub fn new(classification: &str, expected_files: Vec<String>) -> Self {
        let max = match classification {
            "trivial" => 10,
            "small" => 20,
            "medium" => 35,
            "large" => 50,
            _ => 25,
        };

        Self {
            recent_calls: Vec::new(),
            write_counts: HashMap::new(),
            max_iterations: max,
            current_step: 0,
            expected_modify_files: expected_files,
        }
    }

    pub fn record_call(
        &mut self,
        tool: &str,
        params: &str,
        file_path: Option<&str>,
        result_summary: &str,
    ) {
        self.current_step += 1;

        let params_hash = format!("{:x}", Sha256::digest(params.as_bytes()));
        let is_error = result_summary.contains("failed")
            || result_summary.contains("Error")
            || result_summary.contains("blocked");

        if tool == "write" || tool == "edit" {
            if let Some(path) = file_path {
                self.write_counts
                    .entry(path.to_string())
                    .or_default()
                    .push(self.current_step);
            }
        }

        self.recent_calls.push(RecentCall {
            step: self.current_step,
            tool: tool.to_string(),
            params_hash,
            file_path: file_path.map(|s| s.to_string()),
            result_summary: result_summary.to_string(),
            is_error,
        });

        // Keep last 15
        if self.recent_calls.len() > 15 {
            self.recent_calls.remove(0);
        }
    }

    pub fn check(&self) -> LoopIntervention {
        // Budget check
        if self.current_step >= self.max_iterations {
            return LoopIntervention::BudgetExhausted(
                format!(
                    "[System] You've reached the iteration limit ({}) for this task. Summarize what you've accomplished and what remains, then stop.",
                    self.max_iterations
                )
            );
        }

        // Exact repetition: same tool + same params within last 5 calls
        if self.recent_calls.len() >= 2 {
            let last = self.recent_calls.last().unwrap();
            let window = self.recent_calls.iter().rev().skip(1).take(4);
            for prev in window {
                if prev.tool == last.tool && prev.params_hash == last.params_hash {
                    return LoopIntervention::Message(format!(
                        "[System] You already did this at step {}. Result was: {}. Do not repeat.",
                        prev.step, prev.result_summary
                    ));
                }
            }
        }

        // Write cycling: same file written 3+ times
        for (path, steps) in &self.write_counts {
            if steps.len() >= 3 {
                let entries: Vec<String> = steps.iter().map(|s| format!("Step {}", s)).collect();
                return LoopIntervention::Message(
                    format!(
                        "[System] You have written to {} {} times this session (at: {}). If your approach isn't working, try something fundamentally different or explain the difficulty to the user.",
                        path, steps.len(), entries.join(", ")
                    )
                );
            }
        }

        // Read-after-read: same file read 2+ times without writes between
        if self.recent_calls.len() >= 2 {
            let last = self.recent_calls.last().unwrap();
            if last.tool == "read" {
                if let Some(ref path) = last.file_path {
                    let had_write_between = self
                        .recent_calls
                        .iter()
                        .rev()
                        .skip(1)
                        .take_while(|c| !(c.tool == "read" && c.file_path.as_deref() == Some(path)))
                        .any(|c| {
                            (c.tool == "write" || c.tool == "edit")
                                && c.file_path.as_deref() == Some(path)
                        });

                    if !had_write_between {
                        let prev_read = self
                            .recent_calls
                            .iter()
                            .rev()
                            .skip(1)
                            .find(|c| c.tool == "read" && c.file_path.as_deref() == Some(path));

                        if let Some(prev) = prev_read {
                            return LoopIntervention::Message(
                                format!(
                                    "[System] This file was already read at step {} and hasn't changed. It's in your context. Use grep_file() if you need to find specific text.",
                                    prev.step
                                )
                            );
                        }
                    }
                }
            }
        }

        // Edit-fail-read-edit-fail loop: agent failing to edit, re-reading, failing again
        if self.recent_calls.len() >= 3 {
            let calls: Vec<&RecentCall> = self.recent_calls.iter().rev().take(6).collect();
            let mut edit_fail_count = 0;
            let mut read_between = false;
            let mut target_path: Option<&str> = None;

            for call in &calls {
                if call.tool == "edit" && call.is_error {
                    if target_path.is_none() {
                        target_path = call.file_path.as_deref();
                    }
                    if call.file_path.as_deref() == target_path {
                        edit_fail_count += 1;
                    }
                } else if call.tool == "read" && call.file_path.as_deref() == target_path {
                    read_between = true;
                }
            }

            if edit_fail_count >= 2 && read_between {
                return LoopIntervention::Message(
                    format!(
                        "[System] You've failed to edit {} multiple times with reads in between. The edit error messages already contain the file content you need. Read the error output carefully and use the EXACT text shown. If the approach isn't working, try grep_file() to find the precise text, or break the edit into smaller pieces.",
                        target_path.unwrap_or("this file")
                    )
                );
            }
        }

        // Scope drift: modifying 3+ files not in expected list
        let unexpected_writes: Vec<&String> = self
            .write_counts
            .keys()
            .filter(|path| {
                !self
                    .expected_modify_files
                    .iter()
                    .any(|e| path.contains(e.as_str()))
            })
            .collect();

        if unexpected_writes.len() >= 3 {
            return LoopIntervention::Message(
                format!(
                    "[System] You're modifying files outside the expected scope.\nExpected files: {}\nYou're also modifying: {}\nMake sure these changes are necessary.",
                    self.expected_modify_files.join(", "),
                    unexpected_writes.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                )
            );
        }

        LoopIntervention::None
    }
}
