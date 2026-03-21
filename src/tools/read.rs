use super::{ToolResult, ToolResultMetadata};
use crate::memory::kms::Kms;
use crate::memory::kpms::Kpms;
use crate::project::parser::generate_compact_summary;
use std::path::{Path, PathBuf};

pub fn execute_read(project_dir: &Path, path: &str, kms: &mut Kms, kpms: &Kpms) -> ToolResult {
    let full_path = resolve_path(project_dir, path);

    if !full_path.exists() {
        return ToolResult {
            success: false,
            output: format!("Error: file not found: {}", path),
            metadata: ToolResultMetadata::default(),
        };
    }

    if !full_path.starts_with(project_dir) {
        return ToolResult {
            success: false,
            output: format!("Error: path is outside project directory: {}", path),
            metadata: ToolResultMetadata::default(),
        };
    }

    let content = match std::fs::read_to_string(&full_path) {
        Ok(c) => c,
        Err(e) => {
            return ToolResult {
                success: false,
                output: format!("Error reading file: {}", e),
                metadata: ToolResultMetadata::default(),
            };
        }
    };

    let line_count = content.lines().count();
    let summary = generate_compact_summary(&full_path, &content);
    kms.record_file_read(path, Some(summary));

    let mut output = String::new();

    // Add KPMS annotations if available
    let annotations = get_file_annotations(path, kpms);
    if !annotations.is_empty() {
        output.push_str(&format!("[HAKARI context: {}]\n\n", annotations));
    }

    // For large files, add structural map
    if line_count > 2000 {
        let structure_map = generate_compact_summary(&full_path, &content);
        output.push_str(&format!(
            "[Structure map ({} lines)]\n{}\n\n",
            line_count, structure_map
        ));
    }

    output.push_str(&content);

    ToolResult {
        success: true,
        output,
        metadata: ToolResultMetadata {
            file_path: Some(path.to_string()),
            ..Default::default()
        },
    }
}

fn resolve_path(project_dir: &Path, path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        project_dir.join(p)
    }
}

fn get_file_annotations(path: &str, kpms: &Kpms) -> String {
    let mut annotations = Vec::new();

    if let Some(desc) = kpms.file_index.get(path) {
        annotations.push(desc.clone());
    }

    for learning in &kpms.learnings {
        if learning.context.contains(path) || learning.lesson.contains(path) {
            annotations.push(learning.lesson.clone());
        }
    }

    for ap in &kpms.anti_patterns {
        if ap.pattern.contains(path) {
            annotations.push(format!("Warning: {}", ap.prevention));
        }
    }

    annotations.join(". ")
}
