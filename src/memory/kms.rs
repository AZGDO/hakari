use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::collections::HashMap;
use std::path::PathBuf;

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct KMS {
    pub task_summary: String,
    pub task_type: String,
    pub sub_tasks: Vec<SubTask>,
    pub modified_files: Vec<String>,
    pub preparation_misses: Vec<String>,
    pub file_descriptions: HashMap<String, String>,
    pub failed_approaches: Vec<String>,
    pub successful_strategy: Option<String>,
    pub errors: Vec<String>,
    pub step_count: usize,
    pub goal: String,
}

/// JSON-serializable representation of a chat session for --resume
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistedSession {
    pub id: String,
    pub messages: Vec<PersistedMessage>,
    pub cwd: String,
    pub session_ts: u64, // unix seconds
    /// First user message for display in picker
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedMessage {
    pub role: String,
    pub content: Vec<PersistedContent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PersistedContent {
    Text {
        text: String,
    },
    ToolUse {
        name: String,
        args_summary: String,
        status: String,
        output: Option<String>,
    },
    ShizukaBlock {
        preloaded: Vec<String>,
        referenced: Vec<String>,
        task_summary: String,
        classification: String,
    },
    Other,
}

impl PersistedSession {
    fn sessions_dir(cwd: &str) -> PathBuf {
        let hash = format!("{:x}", sha2::Sha256::digest(cwd.as_bytes()))
            .chars()
            .take(12)
            .collect::<String>();
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".hakari")
            .join("projects")
            .join(hash)
            .join("sessions")
    }

    pub fn save(&self, cwd: &str) -> std::io::Result<()> {
        let dir = Self::sessions_dir(cwd);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.json", self.id));
        let content = serde_json::to_string(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, content)
    }

    pub fn load_by_id(cwd: &str, id: &str) -> Option<Self> {
        let path = Self::sessions_dir(cwd).join(format!("{}.json", id));
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Returns all sessions for this project, sorted newest-first.
    pub fn list_all(cwd: &str) -> Vec<Self> {
        let dir = Self::sessions_dir(cwd);
        let mut sessions = Vec::new();

        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => return sessions,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(session) = serde_json::from_str::<PersistedSession>(&content) {
                        sessions.push(session);
                    }
                }
            }
        }

        // Sort newest first
        sessions.sort_by(|a, b| b.session_ts.cmp(&a.session_ts));
        sessions
    }
}

#[derive(Debug, Clone)]
pub struct SubTask {
    pub description: String,
    pub status: SubTaskStatus,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum SubTaskStatus {
    Pending,
    InProgress,
    Done,
    Failed(String),
}

impl KMS {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_file_modification(&mut self, path: &str) {
        if !self.modified_files.contains(&path.to_string()) {
            self.modified_files.push(path.to_string());
        }
    }

    pub fn record_preparation_miss(&mut self, path: &str) {
        if !self.preparation_misses.contains(&path.to_string()) {
            self.preparation_misses.push(path.to_string());
        }
    }

    pub fn record_error(&mut self, error: String) {
        self.errors.push(error);
    }

    pub fn increment_step(&mut self) {
        self.step_count += 1;
    }

    pub fn to_context_string(&self) -> String {
        if self.task_summary.is_empty() {
            return "First prompt in session.".into();
        }

        let mut out = String::new();
        out.push_str(&format!("Previous task: {}\n", self.task_summary));

        if !self.modified_files.is_empty() {
            out.push_str(&format!(
                "Modified files: {}\n",
                self.modified_files.join(", ")
            ));
        }

        if !self.errors.is_empty() {
            out.push_str("Recent errors:\n");
            for e in self.errors.iter().rev().take(3) {
                out.push_str(&format!("- {}\n", e));
            }
        }

        if !self.sub_tasks.is_empty() {
            out.push_str("Sub-task progress:\n");
            for st in &self.sub_tasks {
                let status = match &st.status {
                    SubTaskStatus::Pending => "pending",
                    SubTaskStatus::InProgress => "in progress",
                    SubTaskStatus::Done => "done",
                    SubTaskStatus::Failed(e) => e.as_str(),
                };
                out.push_str(&format!("- [{}] {}\n", status, st.description));
            }
        }

        out
    }
}
