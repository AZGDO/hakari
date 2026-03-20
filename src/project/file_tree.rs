use ignore::WalkBuilder;
use std::path::Path;

pub struct FileTreeEntry {
    pub path: String,
    pub is_dir: bool,
    pub depth: usize,
}

pub fn build_file_tree(project_dir: &Path, max_entries: usize) -> Vec<FileTreeEntry> {
    let mut entries = Vec::new();
    let walker = WalkBuilder::new(project_dir)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .max_depth(Some(6))
        .build();

    for result in walker {
        if entries.len() >= max_entries {
            break;
        }
        if let Ok(entry) = result {
            let path = entry.path();
            if path == project_dir {
                continue;
            }
            // Skip .git and .hakari directories
            let relative = path.strip_prefix(project_dir).unwrap_or(path);
            let relative_str = relative.to_string_lossy().to_string();
            if relative_str.starts_with(".git/") || relative_str.starts_with(".hakari/") {
                continue;
            }
            entries.push(FileTreeEntry {
                path: relative_str,
                is_dir: path.is_dir(),
                depth: entry.depth(),
            });
        }
    }
    entries
}

pub fn format_file_tree(entries: &[FileTreeEntry]) -> String {
    let mut output = String::new();
    for entry in entries {
        let indent = "  ".repeat(entry.depth.saturating_sub(1));
        let prefix = if entry.is_dir { "📁 " } else { "  " };
        output.push_str(&format!("{}{}{}\n", indent, prefix, entry.path));
    }
    output
}

pub fn format_file_tree_plain(entries: &[FileTreeEntry]) -> String {
    let mut output = String::new();
    for entry in entries {
        if !entry.is_dir {
            output.push_str(&entry.path);
            output.push('\n');
        }
    }
    output
}
