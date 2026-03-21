use super::context_builder;
use super::system_prompt;
use crate::config::HakariConfig;
use crate::llm::client::LlmClient;
use crate::llm::messages::{ConversationHistory, Message, ToolCall};
use crate::llm::providers::StreamEvent;
use crate::llm::tool_schema;
use crate::memory::kkm::Kkm;
use crate::memory::kms::Kms;
use crate::memory::kpms::Kpms;
use crate::shizuka::escalation::{EscalationAction, EscalationEngine};
use crate::shizuka::interceptor::Interceptor;
use crate::shizuka::preparation::PreparationResult;
use crate::tools::ToolResultMetadata;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum AgentEvent {
    PreparationStart,
    PreparationResult(String),
    ThinkingStart,
    TextDelta(String),
    Trace(String),
    ToolOutputDelta {
        name: String,
        chunk: String,
    },
    ToolCallStart {
        name: String,
        id: String,
    },
    ToolCallEnd {
        name: String,
        result: String,
        success: bool,
        metadata: ToolResultMetadata,
        step: usize,
        context_tokens: usize,
    },
    Warning(String),
    Escalation(String),
    Complete {
        final_response: String,
        kms: Box<Kms>,
    },
    Error(String),
}

pub struct NanoAgent {
    config: Arc<HakariConfig>,
    llm_client: Arc<LlmClient>,
    project_dir: std::path::PathBuf,
    depth: usize,
}

impl NanoAgent {
    pub fn new(
        config: Arc<HakariConfig>,
        llm_client: Arc<LlmClient>,
        project_dir: std::path::PathBuf,
        depth: usize,
    ) -> Self {
        Self {
            config,
            llm_client,
            project_dir,
            depth,
        }
    }

    pub async fn run(
        &self,
        prep: &PreparationResult,
        kms: &mut Kms,
        kpms: &Kpms,
        kkm: &Kkm,
        event_tx: mpsc::UnboundedSender<AgentEvent>,
    ) -> anyhow::Result<String> {
        let system_prompt =
            if prep.task_classification == crate::memory::kms::TaskClassification::Large {
                system_prompt::build_orchestrator_system_prompt()
            } else {
                system_prompt::build_system_prompt()
            };

        let initial_context = context_builder::build_nano_context(prep, &self.project_dir, kpms);

        let mut history = ConversationHistory::new();
        history.add(Message::system(&system_prompt));
        history.add(Message::user(&initial_context));

        let tools = if self.config.nano_provider == crate::config::LlmProvider::Anthropic {
            tool_schema::get_tool_definitions_anthropic()
        } else {
            tool_schema::get_tool_definitions_openai()
        };

        let scope_files: Vec<String> = prep
            .files_to_preload
            .iter()
            .chain(prep.files_to_reference.iter())
            .cloned()
            .collect();

        let mut interceptor = Interceptor::new(scope_files, self.config.max_context_tokens);
        let mut escalation = EscalationEngine::new();

        let (max_tool_calls, _, _) = self.config.nano_budget(prep.task_classification.as_str());

        let mut final_response = String::new();

        let _ = event_tx.send(AgentEvent::ThinkingStart);

        // Apply context evictions before sending
        interceptor
            .context_controller
            .apply_evictions(&mut history, kms);

        // Single nano request — stream response
        let (stream_tx, mut stream_rx) = mpsc::unbounded_channel::<StreamEvent>();

        let messages_clone: Vec<Message> = history.messages.clone();
        let tools_clone = tools.clone();
        let llm = self.llm_client.clone();

        let llm_task = tokio::spawn(async move {
            llm.nano_chat(&messages_clone, &tools_clone, Some(stream_tx))
                .await
        });

        // Forward stream events (text only — skip argument delta noise)
        let event_tx_clone = event_tx.clone();
        while let Some(event) = stream_rx.recv().await {
            match &event {
                StreamEvent::TextDelta(text) => {
                    let _ = event_tx_clone.send(AgentEvent::TextDelta(text.clone()));
                }
                StreamEvent::ToolCallStart { id, name } => {
                    let _ = event_tx_clone.send(AgentEvent::ToolCallStart {
                        name: name.clone(),
                        id: id.clone(),
                    });
                }
                StreamEvent::Done => break,
                StreamEvent::Error(e) => {
                    let _ = event_tx_clone.send(AgentEvent::Error(e.clone()));
                    break;
                }
                _ => {}
            }
        }

        let (text, tool_calls) = llm_task.await??;

        if tool_calls.is_empty() {
            final_response = text.clone();
            history.add(Message::assistant(&text));
            let _ = event_tx.send(AgentEvent::Complete {
                final_response: text,
                kms: Box::new(kms.clone()),
            });
        } else {
            history.add(Message::assistant_with_tool_calls(
                &text,
                tool_calls.clone(),
            ));

            // Execute all tool calls (in order)
            for tc in &tool_calls {
                // Check escalation per tool call
                let escalation_action = escalation.evaluate(kms, max_tool_calls);
                match escalation_action {
                    EscalationAction::HardStop { message }
                    | EscalationAction::UserEscalation { summary: message } => {
                        let _ = event_tx.send(AgentEvent::Escalation(message.clone()));
                        final_response = message;
                        break;
                    }
                    EscalationAction::SoftRedirection { message }
                    | EscalationAction::HardConstraint { message, .. } => {
                        let _ = event_tx.send(AgentEvent::Warning(message));
                    }
                    EscalationAction::Continue => {}
                }

                let intercept_result = {
                    let (tool_stream_tx, mut tool_stream_rx) = mpsc::unbounded_channel::<String>();
                    let intercept_fut = interceptor.intercept_tool_call(
                        tc,
                        &self.project_dir,
                        kms,
                        kpms,
                        kkm,
                        if tc.name == "Execute" {
                            Some(tool_stream_tx)
                        } else {
                            None
                        },
                    );
                    tokio::pin!(intercept_fut);

                    loop {
                        tokio::select! {
                            maybe_chunk = tool_stream_rx.recv() => {
                                match maybe_chunk {
                                    Some(chunk) => {
                                        let _ = event_tx.send(AgentEvent::ToolOutputDelta {
                                            name: tc.name.clone(),
                                            chunk,
                                        });
                                    }
                                    None => continue,
                                }
                            }
                            result = &mut intercept_fut => {
                                break result;
                            }
                        }
                    }
                };

                let step = kms.steps.current;
                let context_tokens = kms.context.total_tokens_estimate;

                for warning in &intercept_result.injected_warnings {
                    let _ = event_tx.send(AgentEvent::Warning(warning.clone()));
                }

                let _ = event_tx.send(AgentEvent::ToolCallEnd {
                    name: tc.name.clone(),
                    result: intercept_result.tool_result.output.clone(),
                    success: intercept_result.tool_result.success,
                    metadata: intercept_result.tool_result.metadata.clone(),
                    step,
                    context_tokens,
                });

                if intercept_result.needs_confirmation {
                    let confirm_msg = intercept_result.confirmation_message.unwrap_or_default();
                    let _ = event_tx.send(AgentEvent::Warning(format!(
                        "Confirmation needed: {}",
                        confirm_msg
                    )));
                    history.add(Message::tool_result(
                        &tc.id,
                        &format!("Action requires user confirmation: {}", confirm_msg),
                    ));
                    continue;
                }

                let mut result_text = intercept_result.tool_result.output.clone();
                for warning in &intercept_result.injected_warnings {
                    result_text.push_str(&format!("\n[Warning] {}", warning));
                }

                history.add(Message::tool_result(&tc.id, &result_text));

                if !intercept_result.tool_result.success {
                    kms.record_error(
                        intercept_result.tool_result.metadata.file_path.as_deref(),
                        &intercept_result.tool_result.output,
                    );
                }
            }

            let _ = event_tx.send(AgentEvent::Complete {
                final_response: final_response.clone(),
                kms: Box::new(kms.clone()),
            });
        }

        Ok(final_response)
    }
}
