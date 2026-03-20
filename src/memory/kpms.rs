use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Kpms {
    pub project: ProjectInfo,
    pub architecture: ArchitectureInfo,
    pub file_index: HashMap<String, String>,
    pub learnings: Vec<Learning>,
    pub strategies: Vec<Strategy>,
    pub user_preferences: UserPreferences,
    pub anti_patterns: Vec<AntiPattern>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub project_type: String,
    pub language: String,
    pub framework: String,
    pub package_manager: String,
    pub build_command: String,
    pub test_command: String,
    pub lint_command: String,
    pub dev_command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureInfo {
    pub patterns: Vec<String>,
    pub conventions: Vec<String>,
    pub structure_notes: String,
    pub important_files: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Learning {
    pub id: String,
    pub context: String,
    pub lesson: String,
    pub source: String,
    pub confidence: f64,
    pub last_confirmed: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Strategy {
    pub task_type: String,
    pub approach: String,
    pub success_count: usize,
    pub failure_count: usize,
    pub avg_iterations: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferences {
    pub coding_style: String,
    pub communication_style: String,
    pub auto_test: bool,
    pub custom: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AntiPattern {
    pub pattern: String,
    pub prevention: String,
    pub times_encountered: usize,
}

impl Default for Kpms {
    fn default() -> Self {
        Self {
            project: ProjectInfo {
                name: String::new(),
                project_type: String::new(),
                language: String::new(),
                framework: String::new(),
                package_manager: String::new(),
                build_command: String::new(),
                test_command: String::new(),
                lint_command: String::new(),
                dev_command: String::new(),
            },
            architecture: ArchitectureInfo {
                patterns: Vec::new(),
                conventions: Vec::new(),
                structure_notes: String::new(),
                important_files: HashMap::new(),
            },
            file_index: HashMap::new(),
            learnings: Vec::new(),
            strategies: Vec::new(),
            user_preferences: UserPreferences {
                coding_style: String::new(),
                communication_style: "concise".to_string(),
                auto_test: true,
                custom: HashMap::new(),
            },
            anti_patterns: Vec::new(),
        }
    }
}

impl Kpms {
    pub fn storage_path(project_dir: &Path) -> PathBuf {
        project_dir.join(".hakari").join("project_memory.json")
    }

    pub fn load(project_dir: &Path) -> anyhow::Result<Self> {
        let path = Self::storage_path(project_dir);
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self, project_dir: &Path) -> anyhow::Result<()> {
        let path = Self::storage_path(project_dir);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    pub fn add_learning(&mut self, context: &str, lesson: &str, source: &str) {
        let id = uuid::Uuid::new_v4().to_string();
        self.learnings.push(Learning {
            id,
            context: context.to_string(),
            lesson: lesson.to_string(),
            source: source.to_string(),
            confidence: 1.0,
            last_confirmed: chrono::Utc::now().to_rfc3339(),
        });
    }

    pub fn update_file_index(&mut self, path: &str, description: &str) {
        self.file_index.insert(path.to_string(), description.to_string());
    }

    pub fn get_relevant_learnings(&self, task_description: &str) -> Vec<&Learning> {
        let task_lower = task_description.to_lowercase();
        self.learnings.iter().filter(|l| {
            let context_lower = l.context.to_lowercase();
            let lesson_lower = l.lesson.to_lowercase();
            task_lower.split_whitespace().any(|word| {
                context_lower.contains(word) || lesson_lower.contains(word)
            })
        }).collect()
    }

    pub fn get_relevant_warnings(&self, task_description: &str) -> Vec<&AntiPattern> {
        let task_lower = task_description.to_lowercase();
        self.anti_patterns.iter().filter(|ap| {
            let pattern_lower = ap.pattern.to_lowercase();
            task_lower.split_whitespace().any(|word| pattern_lower.contains(word))
        }).collect()
    }
}
