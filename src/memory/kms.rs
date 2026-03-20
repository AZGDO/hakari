use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Kms {
    pub session_id: String,
    pub task: TaskState,
    pub files: FileState,
    pub steps: StepState,
    pub context: ContextState,
    pub errors: Vec<ErrorRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub original_prompt: String,
    pub goal: String,
    pub classification: TaskClassification,
    pub sub_tasks: Vec<SubTask>,
    pub attempt_history: Vec<AttemptRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TaskClassification {
    Trivial,
    Small,
    Medium,
    Large,
}

impl std::fmt::Display for TaskClassification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Trivial => write!(f, "trivial"),
            Self::Small => write!(f, "small"),
            Self::Medium => write!(f, "medium"),
            Self::Large => write!(f, "large"),
        }
    }
}

impl TaskClassification {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Trivial => "trivial",
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTask {
    pub id: String,
    pub description: String,
    pub status: SubTaskStatus,
    pub assigned_to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SubTaskStatus {
    Pending,
    Active,
    Complete,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptRecord {
    pub approach_description: String,
    pub approach_hash: String,
    pub result: String,
    pub reason_for_failure: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileState {
    pub index: HashMap<String, FileInfo>,
    pub backups: HashMap<String, String>,
    pub locks: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub purpose: String,
    pub last_read_step: Option<usize>,
    pub last_write_step: Option<usize>,
    pub in_context: bool,
    pub ttl_remaining: Option<usize>,
    pub compact_summary: Option<String>,
    pub is_modified: bool,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepState {
    pub current: usize,
    pub history: Vec<StepRecord>,
    pub loop_detector_state: LoopDetectorState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRecord {
    pub step: usize,
    pub tool: String,
    pub params_summary: String,
    pub result_summary: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopDetectorState {
    pub recent_hashes: Vec<String>,
    pub cycle_counts: HashMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextState {
    pub total_tokens_estimate: usize,
    pub active_files: Vec<String>,
    pub compacted_files: Vec<String>,
    pub evicted_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorRecord {
    pub step: usize,
    pub file: Option<String>,
    pub error_message: String,
    pub resolution_status: String,
}

impl Kms {
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            task: TaskState {
                original_prompt: String::new(),
                goal: String::new(),
                classification: TaskClassification::Small,
                sub_tasks: Vec::new(),
                attempt_history: Vec::new(),
            },
            files: FileState {
                index: HashMap::new(),
                backups: HashMap::new(),
                locks: HashMap::new(),
            },
            steps: StepState {
                current: 0,
                history: Vec::new(),
                loop_detector_state: LoopDetectorState {
                    recent_hashes: Vec::new(),
                    cycle_counts: HashMap::new(),
                },
            },
            context: ContextState {
                total_tokens_estimate: 0,
                active_files: Vec::new(),
                compacted_files: Vec::new(),
                evicted_files: Vec::new(),
            },
            errors: Vec::new(),
        }
    }

    pub fn record_step(&mut self, tool: &str, params_summary: &str, result_summary: &str, success: bool) {
        let step = self.steps.current;
        self.steps.history.push(StepRecord {
            step,
            tool: tool.to_string(),
            params_summary: params_summary.to_string(),
            result_summary: result_summary.to_string(),
            success,
        });
        self.steps.current += 1;
    }

    pub fn record_file_read(&mut self, path: &str, summary: Option<String>) {
        let step = self.steps.current;
        let entry = self.files.index.entry(path.to_string()).or_insert_with(|| FileInfo {
            purpose: String::new(),
            last_read_step: None,
            last_write_step: None,
            in_context: false,
            ttl_remaining: Some(6),
            compact_summary: None,
            is_modified: false,
            is_active: false,
        });
        entry.last_read_step = Some(step);
        entry.in_context = true;
        if let Some(s) = summary {
            entry.compact_summary = Some(s);
        }
    }

    pub fn record_file_write(&mut self, path: &str, original_content: Option<String>) {
        let step = self.steps.current;
        let entry = self.files.index.entry(path.to_string()).or_insert_with(|| FileInfo {
            purpose: String::new(),
            last_read_step: None,
            last_write_step: None,
            in_context: false,
            ttl_remaining: None,
            compact_summary: None,
            is_modified: false,
            is_active: false,
        });
        entry.last_write_step = Some(step);
        entry.is_modified = true;
        entry.is_active = true;
        entry.ttl_remaining = None; // active files never evict
        entry.in_context = true;

        if !self.context.active_files.contains(&path.to_string()) {
            self.context.active_files.push(path.to_string());
        }

        if let Some(content) = original_content {
            self.files.backups.entry(path.to_string()).or_insert(content);
        }
    }

    pub fn record_error(&mut self, file: Option<&str>, error_message: &str) {
        self.errors.push(ErrorRecord {
            step: self.steps.current,
            file: file.map(|s| s.to_string()),
            error_message: error_message.to_string(),
            resolution_status: "unresolved".to_string(),
        });
    }

    pub fn get_write_count_for_file(&self, path: &str) -> usize {
        self.steps.history.iter()
            .filter(|s| s.tool == "Write" && s.params_summary.contains(path))
            .count()
    }
}
