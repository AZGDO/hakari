use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KKM {
    pub os: String,
    pub shell: String,
    #[serde(default)]
    pub tool_versions: Vec<ToolVersion>,
    #[serde(default)]
    pub quirks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolVersion {
    pub name: String,
    pub version: String,
}

impl Default for KKM {
    fn default() -> Self {
        let shell = if cfg!(target_os = "windows") {
            std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into())
        } else {
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into())
        };
        Self {
            os: std::env::consts::OS.to_string(),
            shell,
            tool_versions: Vec::new(),
            quirks: Vec::new(),
        }
    }
}

#[allow(dead_code)]
impl KKM {
    fn file_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".hakari")
            .join("kkm.toml")
    }

    pub fn load() -> Self {
        let path = Self::file_path();
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => toml::from_str(&content).unwrap_or_default(),
                Err(_) => Self::detect(),
            }
        } else {
            Self::detect()
        }
    }

    pub fn detect() -> Self {
        let mut kkm = Self::default();

        // Detect common tools
        let tools = [
            ("rg", "ripgrep"),
            ("cargo", "cargo"),
            ("node", "node"),
            ("python3", "python3"),
            ("go", "go"),
            ("git", "git"),
        ];

        for (cmd, name) in tools {
            if let Ok(output) = std::process::Command::new(cmd).arg("--version").output() {
                if output.status.success() {
                    let ver = String::from_utf8_lossy(&output.stdout);
                    let first_line = ver.lines().next().unwrap_or("").to_string();
                    kkm.tool_versions.push(ToolVersion {
                        name: name.to_string(),
                        version: first_line,
                    });
                }
            }
        }

        kkm
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::file_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, content)
    }

    pub fn add_quirk(&mut self, quirk: String) {
        if !self.quirks.contains(&quirk) {
            self.quirks.push(quirk);
        }
    }

    pub fn to_context_string(&self) -> String {
        let mut out = format!(
            "OS: {} ({})\nShell: {}\n",
            self.os,
            std::env::consts::ARCH,
            self.shell
        );

        if !self.tool_versions.is_empty() {
            out.push_str("Available tools:\n");
            for t in &self.tool_versions {
                out.push_str(&format!("- {}: {}\n", t.name, t.version));
            }
        }

        if !self.quirks.is_empty() {
            out.push_str("Known quirks:\n");
            for q in &self.quirks {
                out.push_str(&format!("- {}\n", q));
            }
        }

        out
    }
}
