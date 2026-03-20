use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Kkm {
    pub system: SystemInfo,
    pub tools: HashMap<String, ToolInfo>,
    pub quirks: Vec<Quirk>,
    pub ports: HashMap<u16, String>,
    pub global_preferences: GlobalPreferences,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub os: String,
    pub os_version: String,
    pub arch: String,
    pub shell: String,
    pub home_dir: String,
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub path: String,
    pub version: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quirk {
    pub trigger: String,
    pub fix: String,
    pub auto_apply: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalPreferences {
    pub default_branch: String,
    pub preferred_editor: String,
    pub custom: HashMap<String, String>,
}

impl Default for Kkm {
    fn default() -> Self {
        Self {
            system: SystemInfo {
                os: std::env::consts::OS.to_string(),
                os_version: String::new(),
                arch: std::env::consts::ARCH.to_string(),
                shell: std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()),
                home_dir: dirs::home_dir().unwrap_or_default().to_string_lossy().to_string(),
                username: std::env::var("USER").unwrap_or_else(|_| "unknown".to_string()),
            },
            tools: HashMap::new(),
            quirks: Vec::new(),
            ports: HashMap::new(),
            global_preferences: GlobalPreferences {
                default_branch: "main".to_string(),
                preferred_editor: String::new(),
                custom: HashMap::new(),
            },
        }
    }
}

impl Kkm {
    pub fn storage_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".hakari")
            .join("device_memory.json")
    }

    pub fn load() -> anyhow::Result<Self> {
        let path = Self::storage_path();
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            let mut kkm = Self::default();
            kkm.detect_system();
            kkm.save()?;
            Ok(kkm)
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::storage_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    pub fn detect_system(&mut self) {
        if let Ok(output) = std::process::Command::new("uname").arg("-r").output() {
            self.system.os_version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        }

        let tools_to_check = [
            ("python3", "python3"),
            ("python", "python"),
            ("node", "node"),
            ("npm", "npm"),
            ("pnpm", "pnpm"),
            ("yarn", "yarn"),
            ("cargo", "cargo"),
            ("rustc", "rustc"),
            ("git", "git"),
            ("docker", "docker"),
        ];

        for (name, cmd) in &tools_to_check {
            if let Ok(output) = std::process::Command::new("which").arg(cmd).output() {
                if output.status.success() {
                    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    let version = std::process::Command::new(cmd)
                        .arg("--version")
                        .output()
                        .ok()
                        .map(|o| String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("").to_string())
                        .unwrap_or_default();
                    self.tools.insert(name.to_string(), ToolInfo {
                        path,
                        version,
                        notes: String::new(),
                    });
                }
            }
        }

        if self.system.os == "macos" {
            self.quirks.push(Quirk {
                trigger: "sed -i ".to_string(),
                fix: "sed -i '' ".to_string(),
                auto_apply: true,
            });
        }

        if self.tools.contains_key("python3") && !self.tools.contains_key("python") {
            self.quirks.push(Quirk {
                trigger: "python ".to_string(),
                fix: "python3 ".to_string(),
                auto_apply: true,
            });
        }
    }

    pub fn transform_command(&self, command: &str) -> String {
        let mut result = command.to_string();
        for quirk in &self.quirks {
            if quirk.auto_apply && result.contains(&quirk.trigger) {
                result = result.replace(&quirk.trigger, &quirk.fix);
            }
        }
        result
    }

    pub fn add_quirk(&mut self, trigger: &str, fix: &str, auto_apply: bool) {
        if !self.quirks.iter().any(|q| q.trigger == trigger) {
            self.quirks.push(Quirk {
                trigger: trigger.to_string(),
                fix: fix.to_string(),
                auto_apply,
            });
        }
    }
}
