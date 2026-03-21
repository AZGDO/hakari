use super::agent::{AgentEvent, NanoAgent};
use crate::config::HakariConfig;
use crate::llm::client::LlmClient;
use crate::memory::kkm::Kkm;
use crate::memory::kms::Kms;
use crate::memory::kpms::Kpms;
use crate::shizuka::preparation::PreparationResult;
use crate::tools::summon;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct SwarmManager {
    config: Arc<HakariConfig>,
    llm_client: Arc<LlmClient>,
    project_dir: PathBuf,
    active_agents: usize,
}

impl SwarmManager {
    pub fn new(
        config: Arc<HakariConfig>,
        llm_client: Arc<LlmClient>,
        project_dir: PathBuf,
    ) -> Self {
        Self {
            config,
            llm_client,
            project_dir,
            active_agents: 0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn spawn_sub_agent(
        &mut self,
        task: &str,
        files: Vec<String>,
        parent_kms: &mut Kms,
        kpms: &Kpms,
        kkm: &Kkm,
        parent_depth: usize,
        event_tx: mpsc::UnboundedSender<AgentEvent>,
    ) -> anyhow::Result<summon::SummonResult> {
        let request = summon::SummonRequest {
            task: task.to_string(),
            files: files.clone(),
        };

        // Validate
        if let Err(e) =
            summon::validate_summon(&request, parent_kms, parent_depth, self.active_agents)
        {
            return Ok(summon::SummonResult {
                tool_result: crate::tools::ToolResult {
                    success: false,
                    output: e,
                    metadata: crate::tools::ToolResultMetadata::default(),
                },
                modified_files: Vec::new(),
            });
        }

        // Acquire locks
        let agent_id = uuid::Uuid::new_v4().to_string();
        summon::acquire_file_locks(parent_kms, &files, &agent_id);
        self.active_agents += 1;

        // Build mini preparation for sub-agent
        let sub_prep = PreparationResult {
            task_classification: crate::memory::kms::TaskClassification::Small,
            task_summary: task.to_string(),
            files_to_preload: files.clone(),
            files_to_reference: Vec::new(),
            suggested_approach: None,
            relevant_learnings: Vec::new(),
            relevant_warnings: Vec::new(),
            kms_updates: crate::shizuka::preparation::KmsUpdates {
                goal: task.to_string(),
                sub_tasks: Vec::new(),
            },
        };

        let mut sub_kms = Kms::new(uuid::Uuid::new_v4().to_string());
        sub_kms.task.original_prompt = task.to_string();
        sub_kms.task.goal = task.to_string();

        let sub_agent = NanoAgent::new(
            self.config.clone(),
            self.llm_client.clone(),
            self.project_dir.clone(),
            parent_depth + 1,
        );

        let result = sub_agent
            .run(&sub_prep, &mut sub_kms, kpms, kkm, event_tx)
            .await;

        // Release locks
        summon::release_file_locks(parent_kms, &files);
        self.active_agents -= 1;

        // Collect modified files
        let modified_files: Vec<String> = sub_kms
            .files
            .index
            .iter()
            .filter(|(_, info)| info.is_modified)
            .map(|(path, _)| path.clone())
            .collect();

        match result {
            Ok(response) => {
                let output =
                    summon::format_summon_result(task, &modified_files, &response, true, "");
                Ok(summon::SummonResult {
                    tool_result: crate::tools::ToolResult {
                        success: true,
                        output,
                        metadata: crate::tools::ToolResultMetadata::default(),
                    },
                    modified_files,
                })
            }
            Err(e) => Ok(summon::SummonResult {
                tool_result: crate::tools::ToolResult {
                    success: false,
                    output: format!("Sub-agent failed: {}", e),
                    metadata: crate::tools::ToolResultMetadata::default(),
                },
                modified_files,
            }),
        }
    }
}
