use crate::llm::messages::ConversationHistory;
use crate::memory::kms::Kms;

pub struct ContextController {
    max_tokens: usize,
    ttl_config: TtlConfig,
}

struct TtlConfig {
    active_file: Option<usize>, // None = infinity
    preloaded_file: usize,
    reference_file: usize,
    nano_read: usize,
    re_read: usize,
}

impl Default for TtlConfig {
    fn default() -> Self {
        Self {
            active_file: None,
            preloaded_file: 8,
            reference_file: 6,
            nano_read: 6,
            re_read: 4,
        }
    }
}

impl ContextController {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            ttl_config: TtlConfig::default(),
        }
    }

    pub fn step(&mut self, kms: &mut Kms) {
        // Decrement TTLs for all files in context
        let mut files_to_compact = Vec::new();

        for (path, info) in kms.files.index.iter_mut() {
            if info.is_active {
                // Active files never evict
                continue;
            }
            if let Some(ref mut ttl) = info.ttl_remaining {
                if *ttl > 0 {
                    *ttl -= 1;
                } else {
                    // TTL expired, mark for compaction
                    if info.in_context {
                        files_to_compact.push(path.clone());
                    }
                }
            }
        }

        // Compact expired files
        for path in &files_to_compact {
            if let Some(info) = kms.files.index.get_mut(path) {
                info.in_context = false;
                if !kms.context.compacted_files.contains(path) {
                    kms.context.compacted_files.push(path.clone());
                }
                kms.context.active_files.retain(|f| f != path);
            }
        }

        // Update token estimate
        self.update_token_estimate(kms);
    }

    pub fn compact_file_in_history(
        &self,
        history: &mut ConversationHistory,
        file_path: &str,
        compact_summary: &str,
    ) {
        let indices = history.find_tool_result_indices_for_file(file_path);
        for idx in indices {
            let compacted = format!(
                "[COMPACTED — {}]\n{}\nRe-read if you need the full implementation.",
                file_path, compact_summary
            );
            history.replace_content_at(idx, &compacted);
        }
    }

    pub fn apply_evictions(&self, history: &mut ConversationHistory, kms: &Kms) {
        for path in &kms.context.compacted_files {
            if let Some(info) = kms.files.index.get(path) {
                if let Some(ref summary) = info.compact_summary {
                    self.compact_file_in_history(history, path, summary);
                }
            }
        }
    }

    pub fn check_budget(&self, kms: &Kms) -> bool {
        let safety_margin = self.max_tokens / 10; // 10% safety
        let reasoning_reserve = 20_000;
        kms.context.total_tokens_estimate < (self.max_tokens - safety_margin - reasoning_reserve)
    }

    fn update_token_estimate(&self, kms: &mut Kms) {
        // Rough estimate: sum of file contents in context / 4 (tokens per char average)
        let mut total = 2000; // System prompt base
        for info in kms.files.index.values() {
            if info.in_context {
                if info.is_active {
                    total += 5000; // Estimate for active file content
                } else if info.compact_summary.is_some() {
                    total += 200; // Compacted
                } else {
                    total += 3000; // Full reference file
                }
            }
        }
        // Add step history
        total += kms.steps.history.len() * 100;
        kms.context.total_tokens_estimate = total;
    }

    pub fn pin_file(&self, kms: &mut Kms, path: &str) {
        if let Some(info) = kms.files.index.get_mut(path) {
            info.ttl_remaining = None;
            info.is_active = true;
            info.in_context = true;
            if !kms.context.active_files.contains(&path.to_string()) {
                kms.context.active_files.push(path.to_string());
            }
        }
    }
}
