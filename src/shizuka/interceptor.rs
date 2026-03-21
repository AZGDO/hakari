use super::context_controller::ContextController;
use super::loop_detector::LoopDetector;
use super::scope_enforcer::ScopeEnforcer;
use crate::llm::messages::ToolCall;
use crate::memory::kkm::Kkm;
use crate::memory::kms::Kms;
use crate::memory::kpms::Kpms;
use crate::tools::execute::ExecuteResult;
use crate::tools::{self, ToolResult};
use std::path::Path;
use tokio::sync::mpsc;

pub struct Interceptor {
    pub loop_detector: LoopDetector,
    pub scope_enforcer: ScopeEnforcer,
    pub context_controller: ContextController,
}

pub struct InterceptResult {
    pub tool_result: ToolResult,
    pub needs_confirmation: bool,
    pub confirmation_message: Option<String>,
    pub injected_warnings: Vec<String>,
}

impl Interceptor {
    pub fn new(scope_files: Vec<String>, max_context_tokens: usize) -> Self {
        Self {
            loop_detector: LoopDetector::new(),
            scope_enforcer: ScopeEnforcer::new(scope_files),
            context_controller: ContextController::new(max_context_tokens),
        }
    }

    pub async fn intercept_tool_call(
        &mut self,
        tool_call: &ToolCall,
        project_dir: &Path,
        kms: &mut Kms,
        kpms: &Kpms,
        kkm: &Kkm,
        execute_stream_tx: Option<mpsc::UnboundedSender<String>>,
    ) -> InterceptResult {
        let mut warnings = Vec::new();

        // Pre-hook: loop detection
        let call_hash = format!("{}:{}", tool_call.name, tool_call.arguments);
        if let Some(loop_warning) = self.loop_detector.check(&call_hash, &tool_call.name, kms) {
            warnings.push(loop_warning.clone());
            if loop_warning.contains("blocked") {
                return InterceptResult {
                    tool_result: ToolResult {
                        success: false,
                        output: loop_warning,
                        metadata: tools::ToolResultMetadata::default(),
                    },
                    needs_confirmation: false,
                    confirmation_message: None,
                    injected_warnings: warnings,
                };
            }
        }

        // Pre-hook: scope check for writes
        if tool_call.name == "Write" {
            if let Some(path) = tool_call.arguments.get("path").and_then(|v| v.as_str()) {
                if let Some(scope_warning) = self.scope_enforcer.check_write(path) {
                    warnings.push(scope_warning);
                }
            }
        }

        // Execute the tool
        let (result, needs_confirmation, confirmation_message) = match tool_call.name.as_str() {
            "Read" => {
                let path = tool_call
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let result = tools::read::execute_read(project_dir, path, kms, kpms);
                (result, false, None)
            }
            "Grep" => {
                let query = tool_call
                    .arguments
                    .get("query")
                    .or_else(|| tool_call.arguments.get("pattern"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let file_glob = tool_call
                    .arguments
                    .get("file_glob")
                    .and_then(|v| v.as_str());
                let context_lines = tool_call
                    .arguments
                    .get("context_lines")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let max_results = tool_call
                    .arguments
                    .get("max_results")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let result = tools::grep::execute_grep(
                    project_dir,
                    query,
                    file_glob,
                    context_lines,
                    max_results,
                    kpms,
                );
                (result, false, None)
            }
            "Write" => {
                let path = tool_call
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let content = tool_call
                    .arguments
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let result = tools::write::execute_write(project_dir, path, content, kms, kpms);
                (result, false, None)
            }
            "Execute" => {
                let command = tool_call
                    .arguments
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let exec_result =
                    tools::execute::execute_command(project_dir, command, kkm, execute_stream_tx)
                        .await;
                (
                    exec_result.tool_result,
                    exec_result.needs_confirmation,
                    exec_result.confirmation_message,
                )
            }
            "SummonNano" => {
                // SummonNano is handled at the agent level since it requires spawning a new agent
                (
                    ToolResult {
                        success: false,
                        output: "SummonNano must be handled by the agent orchestrator".to_string(),
                        metadata: tools::ToolResultMetadata::default(),
                    },
                    false,
                    None,
                )
            }
            _ => (
                ToolResult {
                    success: false,
                    output: format!("Unknown tool: {}", tool_call.name),
                    metadata: tools::ToolResultMetadata::default(),
                },
                false,
                None,
            ),
        };

        // Post-hook: update KMS
        let params_summary = match tool_call.name.as_str() {
            "Read" => tool_call
                .arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            "Grep" => tool_call
                .arguments
                .get("query")
                .or_else(|| tool_call.arguments.get("pattern"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            "Write" => tool_call
                .arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            "Execute" => tool_call
                .arguments
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            _ => format!("{:?}", tool_call.arguments),
        };
        let result_summary = if result.output.len() > 200 {
            format!("{}...", &result.output[..200])
        } else {
            result.output.clone()
        };
        kms.record_step(
            &tool_call.name,
            &params_summary,
            &result_summary,
            result.success,
        );

        // Post-hook: update loop detector
        self.loop_detector.record(&call_hash, result.success);

        // Post-hook: context management
        self.context_controller.step(kms);

        // Post-hook: record improvement signals
        if tool_call.name == "Read" {
            if let Some(path) = tool_call.arguments.get("path").and_then(|v| v.as_str()) {
                self.scope_enforcer.record_read(path);
            }
        }

        InterceptResult {
            tool_result: result,
            needs_confirmation,
            confirmation_message,
            injected_warnings: warnings,
        }
    }
}
