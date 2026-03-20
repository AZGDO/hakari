use std::path::Path;
use crate::memory::kpms::ProjectInfo;

pub fn detect_project(project_dir: &Path) -> ProjectInfo {
    let name = project_dir
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let mut info = ProjectInfo {
        name,
        project_type: String::new(),
        language: String::new(),
        framework: String::new(),
        package_manager: String::new(),
        build_command: String::new(),
        test_command: String::new(),
        lint_command: String::new(),
        dev_command: String::new(),
    };

    // Detect by config files
    if project_dir.join("Cargo.toml").exists() {
        info.language = "rust".to_string();
        info.package_manager = "cargo".to_string();
        info.build_command = "cargo build".to_string();
        info.test_command = "cargo test".to_string();
        info.lint_command = "cargo clippy".to_string();
        info.project_type = "rust".to_string();
    }

    if project_dir.join("package.json").exists() {
        info.language = "javascript/typescript".to_string();

        if project_dir.join("pnpm-lock.yaml").exists() {
            info.package_manager = "pnpm".to_string();
        } else if project_dir.join("yarn.lock").exists() {
            info.package_manager = "yarn".to_string();
        } else if project_dir.join("bun.lockb").exists() {
            info.package_manager = "bun".to_string();
        } else {
            info.package_manager = "npm".to_string();
        }

        let pm = &info.package_manager;
        info.build_command = format!("{pm} run build");
        info.test_command = format!("{pm} test");
        info.lint_command = format!("{pm} run lint");
        info.dev_command = format!("{pm} run dev");

        if let Ok(content) = std::fs::read_to_string(project_dir.join("package.json")) {
            if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(deps) = pkg.get("dependencies") {
                    if deps.get("next").is_some() {
                        info.framework = "next.js".to_string();
                        info.project_type = "next.js app".to_string();
                    } else if deps.get("react").is_some() {
                        info.framework = "react".to_string();
                        info.project_type = "react app".to_string();
                    } else if deps.get("vue").is_some() {
                        info.framework = "vue".to_string();
                        info.project_type = "vue app".to_string();
                    } else if deps.get("express").is_some() {
                        info.framework = "express".to_string();
                        info.project_type = "express api".to_string();
                    }
                }
            }
        }

        if project_dir.join("tsconfig.json").exists() {
            info.language = "typescript".to_string();
        }
    }

    if project_dir.join("pyproject.toml").exists() || project_dir.join("setup.py").exists() {
        info.language = "python".to_string();
        info.package_manager = "pip".to_string();
        info.test_command = "pytest".to_string();
        info.lint_command = "ruff check .".to_string();
        info.project_type = "python".to_string();

        if project_dir.join("pyproject.toml").exists() {
            if let Ok(content) = std::fs::read_to_string(project_dir.join("pyproject.toml")) {
                if content.contains("poetry") {
                    info.package_manager = "poetry".to_string();
                } else if content.contains("[tool.uv]") || content.contains("uv") {
                    info.package_manager = "uv".to_string();
                }
            }
        }
    }

    if project_dir.join("go.mod").exists() {
        info.language = "go".to_string();
        info.package_manager = "go".to_string();
        info.build_command = "go build ./...".to_string();
        info.test_command = "go test ./...".to_string();
        info.lint_command = "golangci-lint run".to_string();
        info.project_type = "go".to_string();
    }

    info
}
