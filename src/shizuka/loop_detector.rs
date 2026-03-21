use crate::memory::kms::Kms;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

const WINDOW_SIZE: usize = 20;

pub struct LoopDetector {
    recent_calls: Vec<String>,
    call_counts: HashMap<String, usize>,
    approach_hashes: HashMap<String, Vec<ApproachAttempt>>,
}

struct ApproachAttempt {
    step: usize,
    success: bool,
    description: String,
}

impl LoopDetector {
    pub fn new() -> Self {
        Self {
            recent_calls: Vec::new(),
            call_counts: HashMap::new(),
            approach_hashes: HashMap::new(),
        }
    }

    pub fn check(&self, call_hash: &str, tool_name: &str, kms: &Kms) -> Option<String> {
        // Check exact repetition
        if let Some(count) = self.call_counts.get(call_hash) {
            if *count >= 2 {
                let prev_step = kms.steps.history.iter().rev().find(|s| {
                    let h = format!("{}:{}", s.tool, s.params_summary);
                    h == *call_hash || call_hash.contains(&s.params_summary)
                });
                if let Some(prev) = prev_step {
                    return Some(format!(
                        "You already performed this exact action at step {}. The result was: {}",
                        prev.step,
                        if prev.result_summary.len() > 100 {
                            format!("{}...", &prev.result_summary[..100])
                        } else {
                            prev.result_summary.clone()
                        }
                    ));
                }
            }
        }

        // Check read-read cycle (same file read multiple times without write)
        if tool_name == "Read" {
            let path = call_hash.strip_prefix("Read:").unwrap_or(call_hash);
            let read_count = self
                .recent_calls
                .iter()
                .filter(|c| c.starts_with("Read:") && c.contains(path))
                .count();
            if read_count >= 2 {
                return Some(
                    "This file is now pinned in your context. You don't need to read it again."
                        .to_string(),
                );
            }
        }

        None
    }

    pub fn record(&mut self, call_hash: &str, _success: bool) {
        self.recent_calls.push(call_hash.to_string());
        if self.recent_calls.len() > WINDOW_SIZE {
            let removed = self.recent_calls.remove(0);
            if let Some(count) = self.call_counts.get_mut(&removed) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    self.call_counts.remove(&removed);
                }
            }
        }
        *self.call_counts.entry(call_hash.to_string()).or_insert(0) += 1;
    }

    pub fn check_approach_hash(
        &mut self,
        file_path: &str,
        diff_summary: &str,
        step: usize,
    ) -> Option<String> {
        let hash = compute_approach_hash(file_path, diff_summary);
        let attempts = self.approach_hashes.entry(hash.clone()).or_default();

        if attempts.len() >= 3 {
            let reasons: Vec<&str> = attempts
                .iter()
                .filter(|a| !a.success)
                .map(|a| a.description.as_str())
                .collect();
            return Some(format!(
                "You've attempted this approach 3 times. Previous outcomes: [{}]. You must try a fundamentally different approach, or escalate to the user.",
                reasons.join("; ")
            ));
        } else if attempts.len() >= 2 {
            let last = &attempts[attempts.len() - 1];
            return Some(format!(
                "This appears to be the same approach as step {} which resulted in [{}]. If you're intentionally refining it, proceed. If not, consider an alternative approach.",
                last.step, last.description
            ));
        }

        attempts.push(ApproachAttempt {
            step,
            success: false, // will be updated
            description: diff_summary.to_string(),
        });

        None
    }

    pub fn check_write_error_cycle(&self, file_path: &str, kms: &Kms) -> Option<String> {
        let recent_writes: Vec<&crate::memory::kms::StepRecord> = kms
            .steps
            .history
            .iter()
            .rev()
            .filter(|s| s.tool == "Write" && s.params_summary.contains(file_path))
            .take(6)
            .collect();

        let failed_count = recent_writes.iter().filter(|s| !s.success).count();
        if failed_count >= 3 {
            let approaches: Vec<String> = recent_writes
                .iter()
                .map(|s| {
                    format!(
                        "Step {}: {}",
                        s.step,
                        &s.result_summary[..s.result_summary.len().min(80)]
                    )
                })
                .collect();
            return Some(format!(
                "You have attempted to fix this file {} times. Previous approaches: [{}]. Consider a fundamentally different approach or ask the user for clarification.",
                failed_count,
                approaches.join("; ")
            ));
        }

        None
    }
}

fn compute_approach_hash(file_path: &str, diff_summary: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(file_path.as_bytes());
    hasher.update(diff_summary.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)[..16].to_string()
}
