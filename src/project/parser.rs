use std::path::Path;

pub fn generate_compact_summary(file_path: &Path, content: &str) -> String {
    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext {
        "rs" => generate_rust_summary(content),
        "ts" | "tsx" | "js" | "jsx" => generate_js_ts_summary(content),
        "py" => generate_python_summary(content),
        "go" => generate_go_summary(content),
        _ => generate_generic_summary(content),
    }
}

fn generate_rust_summary(content: &str) -> String {
    let mut summary = Vec::new();
    let mut imports = Vec::new();
    let mut exports = Vec::new();
    let mut structs = Vec::new();
    let mut functions = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("use ") {
            imports.push(trimmed.to_string());
        } else if trimmed.starts_with("pub fn ") || trimmed.starts_with("pub async fn ") {
            let sig = extract_until(trimmed, '{').trim_end().to_string();
            functions.push(sig);
        } else if trimmed.starts_with("pub struct ") || trimmed.starts_with("pub enum ") {
            let sig = extract_until(trimmed, '{').trim_end().to_string();
            structs.push(sig);
        } else if trimmed.starts_with("pub trait ") {
            let sig = extract_until(trimmed, '{').trim_end().to_string();
            exports.push(sig);
        } else if trimmed.starts_with("pub mod ") {
            exports.push(trimmed.to_string());
        }
    }

    if !imports.is_empty() {
        summary.push(format!("Imports: {}", imports.join(", ")));
    }
    if !structs.is_empty() {
        summary.push(format!("Types: {}", structs.join(", ")));
    }
    if !functions.is_empty() {
        summary.push(format!("Functions: {}", functions.join(", ")));
    }
    if !exports.is_empty() {
        summary.push(format!("Exports: {}", exports.join(", ")));
    }
    summary.push(format!("Lines: {}", content.lines().count()));

    summary.join("\n")
}

fn generate_js_ts_summary(content: &str) -> String {
    let mut summary = Vec::new();
    let mut imports = Vec::new();
    let mut exports = Vec::new();
    let mut functions = Vec::new();
    let mut types = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") {
            imports.push(trimmed.to_string());
        } else if trimmed.starts_with("export default ") || trimmed.starts_with("export ") {
            if trimmed.contains("function ") || trimmed.contains("const ") || trimmed.contains("class ") {
                exports.push(extract_until(trimmed, '{').trim_end().to_string());
            } else if trimmed.contains("interface ") || trimmed.contains("type ") {
                types.push(extract_until(trimmed, '{').trim_end().to_string());
            }
        } else if trimmed.starts_with("function ") {
            functions.push(extract_until(trimmed, '{').trim_end().to_string());
        } else if trimmed.starts_with("interface ") || (trimmed.starts_with("type ") && trimmed.contains('=')) {
            types.push(extract_until(trimmed, '{').trim_end().to_string());
        }
    }

    if !imports.is_empty() {
        summary.push(format!("Imports: {} items", imports.len()));
    }
    if !exports.is_empty() {
        summary.push(format!("Exports: {}", exports.join(", ")));
    }
    if !types.is_empty() {
        summary.push(format!("Types: {}", types.join(", ")));
    }
    if !functions.is_empty() {
        summary.push(format!("Internal: {}", functions.join(", ")));
    }
    summary.push(format!("Lines: {}", content.lines().count()));

    summary.join("\n")
}

fn generate_python_summary(content: &str) -> String {
    let mut summary = Vec::new();
    let mut imports = Vec::new();
    let mut classes = Vec::new();
    let mut functions = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
            imports.push(trimmed.to_string());
        } else if trimmed.starts_with("class ") {
            classes.push(extract_until(trimmed, ':').to_string());
        } else if trimmed.starts_with("def ") || trimmed.starts_with("async def ") {
            // Only top-level functions (no leading whitespace in original line)
            if !line.starts_with(' ') && !line.starts_with('\t') {
                functions.push(extract_until(trimmed, ':').to_string());
            }
        }
    }

    if !imports.is_empty() {
        summary.push(format!("Imports: {} items", imports.len()));
    }
    if !classes.is_empty() {
        summary.push(format!("Classes: {}", classes.join(", ")));
    }
    if !functions.is_empty() {
        summary.push(format!("Functions: {}", functions.join(", ")));
    }
    summary.push(format!("Lines: {}", content.lines().count()));

    summary.join("\n")
}

fn generate_go_summary(content: &str) -> String {
    let mut summary = Vec::new();
    let mut functions = Vec::new();
    let mut types = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("func ") {
            functions.push(extract_until(trimmed, '{').trim_end().to_string());
        } else if trimmed.starts_with("type ") {
            types.push(extract_until(trimmed, '{').trim_end().to_string());
        }
    }

    if !types.is_empty() {
        summary.push(format!("Types: {}", types.join(", ")));
    }
    if !functions.is_empty() {
        summary.push(format!("Functions: {}", functions.join(", ")));
    }
    summary.push(format!("Lines: {}", content.lines().count()));

    summary.join("\n")
}

fn generate_generic_summary(content: &str) -> String {
    let line_count = content.lines().count();
    let first_lines: Vec<&str> = content.lines().take(5).collect();
    format!("Lines: {}\nPreview: {}", line_count, first_lines.join(" | "))
}

fn extract_until(s: &str, ch: char) -> &str {
    s.find(ch).map(|i| &s[..i]).unwrap_or(s)
}
