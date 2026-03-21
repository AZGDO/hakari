pub mod execute;
pub mod grep;
pub mod read;
pub mod summon;
pub mod write;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub metadata: ToolResultMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolResultMetadata {
    pub file_path: Option<String>,
    pub lines_changed: Option<String>,
    pub diff: Option<String>,
    pub lint_warnings: Vec<String>,
    pub related_tests: Vec<String>,
    pub exit_code: Option<i32>,
    pub execution_time_ms: Option<u64>,
}
