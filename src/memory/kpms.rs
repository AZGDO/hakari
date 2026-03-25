use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KPMS {
    #[serde(default)]
    pub file_index: HashMap<String, String>,
    #[serde(default)]
    pub learnings: Vec<Learning>,
    #[serde(default)]
    pub strategies: HashMap<String, Vec<Strategy>>,
    #[serde(default)]
    pub anti_patterns: Vec<AntiPattern>,
    #[serde(default)]
    pub conventions: Vec<String>,
    #[serde(default)]
    pub session_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Learning {
    pub text: String,
    pub confirmations: u32,
    pub first_session: u32,
    pub last_session: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Strategy {
    pub description: String,
    pub success_count: u32,
    pub last_session: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AntiPattern {
    pub description: String,
    pub first_session: u32,
    pub last_encountered: u32,
}

impl KPMS {
    fn project_dir(project_path: &str) -> PathBuf {
        let hash = format!("{:x}", sha2::Sha256::digest(project_path.as_bytes()))
            .chars()
            .take(12)
            .collect::<String>();
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".hakari")
            .join("projects")
            .join(hash)
    }

    fn file_path(project_path: &str) -> PathBuf {
        Self::project_dir(project_path).join("kpms.toml")
    }

    pub fn load(project_path: &str) -> Self {
        let path = Self::file_path(project_path);
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => toml::from_str(&content).unwrap_or_default(),
                Err(_) => Self::default(),
            }
        } else {
            Self::default()
        }
    }

    pub fn save(&self, project_path: &str) -> std::io::Result<()> {
        let dir = Self::project_dir(project_path);
        std::fs::create_dir_all(&dir)?;
        let content = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(Self::file_path(project_path), content)
    }

    pub fn add_learning(&mut self, text: String) {
        if let Some(existing) = self.learnings.iter_mut().find(|l| l.text == text) {
            existing.confirmations += 1;
            existing.last_session = self.session_count;
        } else {
            self.learnings.push(Learning {
                text,
                confirmations: 1,
                first_session: self.session_count,
                last_session: self.session_count,
            });
        }
    }

    pub fn add_strategy(&mut self, task_type: &str, description: String) {
        let strategies = self.strategies.entry(task_type.to_string()).or_default();
        if let Some(existing) = strategies.iter_mut().find(|s| s.description == description) {
            existing.success_count += 1;
            existing.last_session = self.session_count;
        } else {
            strategies.push(Strategy {
                description,
                success_count: 1,
                last_session: self.session_count,
            });
        }
    }

    pub fn add_anti_pattern(&mut self, description: String) {
        if let Some(existing) = self
            .anti_patterns
            .iter_mut()
            .find(|a| a.description == description)
        {
            existing.last_encountered = self.session_count;
        } else {
            self.anti_patterns.push(AntiPattern {
                description,
                first_session: self.session_count,
                last_encountered: self.session_count,
            });
        }
    }

    pub fn prune(&mut self) {
        let sc = self.session_count;
        // Remove learnings confirmed < 2 times and older than 20 sessions
        self.learnings
            .retain(|l| l.confirmations >= 2 || sc - l.last_session < 20);
        // Keep only top 3 strategies per task type
        for strategies in self.strategies.values_mut() {
            strategies.sort_by(|a, b| b.success_count.cmp(&a.success_count));
            strategies.truncate(3);
        }
        // Remove anti-patterns older than 30 sessions if never re-encountered
        self.anti_patterns.retain(|a| sc - a.last_encountered < 30);
        // Remove file index entries for files that no longer exist
        self.file_index
            .retain(|path, _| std::path::Path::new(path).exists());
    }

    pub fn to_context_string(&self) -> String {
        let mut out = String::new();

        if !self.conventions.is_empty() {
            out.push_str("Conventions:\n");
            for c in &self.conventions {
                out.push_str(&format!("- {}\n", c));
            }
            out.push('\n');
        }

        if !self.learnings.is_empty() {
            out.push_str("Learnings:\n");
            for l in &self.learnings {
                out.push_str(&format!("- {} (confirmed {}x)\n", l.text, l.confirmations));
            }
            out.push('\n');
        }

        if !self.strategies.is_empty() {
            out.push_str("Strategies:\n");
            for (task_type, strats) in &self.strategies {
                out.push_str(&format!("  {}:\n", task_type));
                for s in strats {
                    out.push_str(&format!(
                        "  - {} ({}x success)\n",
                        s.description, s.success_count
                    ));
                }
            }
            out.push('\n');
        }

        if !self.anti_patterns.is_empty() {
            out.push_str("Anti-patterns (avoid):\n");
            for a in &self.anti_patterns {
                out.push_str(&format!("- {}\n", a.description));
            }
            out.push('\n');
        }

        if out.is_empty() {
            "No project memory yet.".into()
        } else {
            out
        }
    }
}

use sha2::Digest;
