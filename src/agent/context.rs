use std::collections::HashMap;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct TrackedFile {
    pub path: String,
    pub loaded_at_step: usize,
    pub ttl: i32, // -1 = never evict
    pub full_content: String,
    pub compact_summary: String,
    pub is_active: bool,
    pub message_index: usize,
    pub was_compacted: bool,
    pub is_range_read: bool,
    pub range_start: usize,
    pub range_end: usize,
}

pub struct ContextController {
    pub files: HashMap<String, TrackedFile>,
    pub current_step: usize,
    pub estimated_tokens: usize,
    pub context_window_limit: usize,
}

impl ContextController {
    pub fn new(context_window_limit: usize) -> Self {
        Self {
            files: HashMap::new(),
            current_step: 0,
            estimated_tokens: 0,
            context_window_limit,
        }
    }

    pub fn track_file(&mut self, path: &str, content: &str, summary: &str, ttl: i32) {
        let tokens = estimate_tokens(content);
        self.estimated_tokens += tokens;

        self.files.insert(
            path.to_string(),
            TrackedFile {
                path: path.to_string(),
                loaded_at_step: self.current_step,
                ttl,
                full_content: content.to_string(),
                compact_summary: summary.to_string(),
                is_active: false,
                message_index: 0,
                was_compacted: false,
                is_range_read: false,
                range_start: 0,
                range_end: 0,
            },
        );
    }

    pub fn track_range_read(
        &mut self,
        path: &str,
        region_content: &str,
        summary: &str,
        start: usize,
        end: usize,
    ) {
        let tokens = estimate_tokens(region_content);
        self.estimated_tokens += tokens;

        // If already tracked, just update the step (don't replace full content)
        if let Some(existing) = self.files.get_mut(path) {
            existing.loaded_at_step = self.current_step;
            existing.was_compacted = false;
            return;
        }

        self.files.insert(
            path.to_string(),
            TrackedFile {
                path: path.to_string(),
                loaded_at_step: self.current_step,
                ttl: 6,
                full_content: region_content.to_string(),
                compact_summary: summary.to_string(),
                is_active: false,
                message_index: 0,
                was_compacted: false,
                is_range_read: true,
                range_start: start,
                range_end: end,
            },
        );
    }

    pub fn track_initial_file(&mut self, path: &str, content: &str, summary: &str, role: &str) {
        let ttl = match role {
            "modify" => -1,
            "reference" => 8,
            "context" => 5,
            _ => 6,
        };
        self.track_file(path, content, summary, ttl);
    }

    pub fn promote_to_active(&mut self, path: &str, new_content: &str) {
        if let Some(file) = self.files.get_mut(path) {
            file.is_active = true;
            file.ttl = -1;
            file.full_content = new_content.to_string();
            file.was_compacted = false;
            file.is_range_read = false;
        } else {
            self.files.insert(
                path.to_string(),
                TrackedFile {
                    path: path.to_string(),
                    loaded_at_step: self.current_step,
                    ttl: -1,
                    full_content: new_content.to_string(),
                    compact_summary: String::new(),
                    is_active: true,
                    message_index: 0,
                    was_compacted: false,
                    is_range_read: false,
                    range_start: 0,
                    range_end: 0,
                },
            );
        }
    }

    pub fn is_file_active(&self, path: &str) -> bool {
        self.files
            .get(path)
            .map(|f| !f.was_compacted)
            .unwrap_or(false)
    }

    pub fn was_compacted(&self, path: &str) -> bool {
        self.files
            .get(path)
            .map(|f| f.was_compacted)
            .unwrap_or(false)
    }

    pub fn get_file_loaded_step(&self, path: &str) -> Option<usize> {
        self.files.get(path).map(|f| f.loaded_at_step)
    }

    pub fn reset_ttl_for_referenced(&mut self, assistant_text: &str) {
        for file in self.files.values_mut() {
            if !file.is_active && file.ttl != -1 {
                let filename = std::path::Path::new(&file.path)
                    .file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or("");
                if !filename.is_empty() && assistant_text.contains(filename) {
                    file.loaded_at_step = self.current_step;
                }
            }
        }
    }

    pub fn increment_step(&mut self) {
        self.current_step += 1;
    }

    /// Returns paths of files that should be compacted, along with their summaries
    pub fn get_compaction_targets(&mut self) -> Vec<(String, String)> {
        let step = self.current_step;
        let mut targets = Vec::new();

        for file in self.files.values_mut() {
            if file.ttl != -1
                && !file.is_active
                && !file.was_compacted
                && (step as i32 - file.loaded_at_step as i32) >= file.ttl
            {
                let summary = format!(
                    "[Compacted] {}\n[Call read(\"{}\") to see full content again.]",
                    file.compact_summary, file.path
                );
                targets.push((file.path.clone(), summary));
                file.was_compacted = true;
                self.estimated_tokens -= estimate_tokens(&file.full_content);
                self.estimated_tokens += estimate_tokens(&file.compact_summary);
            }
        }

        targets
    }

    /// Force compact everything non-active (safety net)
    pub fn force_compact_all(&mut self) -> Vec<(String, String)> {
        let mut targets = Vec::new();

        for file in self.files.values_mut() {
            if !file.is_active && !file.was_compacted {
                let summary = format!(
                    "[Compacted] {}\n[Call read(\"{}\") to see full content again.]",
                    file.compact_summary, file.path
                );
                targets.push((file.path.clone(), summary));
                file.was_compacted = true;
            }
        }

        targets
    }

    pub fn is_over_budget(&self) -> bool {
        self.estimated_tokens > (self.context_window_limit * 3 / 4)
    }

    pub fn context_percent(&self) -> f64 {
        if self.context_window_limit == 0 {
            return 0.0;
        }
        (self.estimated_tokens as f64 / self.context_window_limit as f64) * 100.0
    }
}

fn estimate_tokens(text: &str) -> usize {
    // Rough estimate: ~4 chars per token for English/code
    text.len() / 4
}
