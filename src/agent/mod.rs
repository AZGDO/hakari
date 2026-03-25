pub mod context;
pub mod llm;
pub mod llm_copilot;
pub mod llm_gemini;
pub mod loop_detect;
pub mod nano;
pub mod shizuka;
pub mod tools;

use std::sync::Arc;
use tokio::sync::mpsc;

use crate::agent::llm::LlmClient;
use crate::agent::llm_copilot::CopilotLlm;
use crate::agent::llm_gemini::GeminiLlm;
use crate::agent::nano::run_nano;
use crate::agent::shizuka::{build_shizuka_user_message, run_shizuka, scan_file_tree};
use crate::config::HakariConfig;
use crate::copilot::CopilotRateLimits;
use crate::memory::MemorySystem;

#[derive(Debug, Clone)]
pub enum AgentEvent {
    PhaseChange(String),
    ShizukaToolCall {
        name: String,
        args: String,
    },
    ShizukaReady {
        preloaded: Vec<String>,
        referenced: Vec<String>,
        task_summary: String,
        classification: String,
    },
    StreamChunk(String),
    ToolStart {
        name: String,
        args: String,
    },
    ToolComplete {
        name: String,
        result: String,
        is_error: bool,
    },
    TokenUpdate {
        input: u64,
        output: u64,
        cached: u64,
    },
    ContextUpdate(f64),
    DirectAnswer(String),
    CopilotRateLimitUpdate(CopilotRateLimits),
    CopilotRequestUsed,
    Done(String),
    Error(String),
}

// ── Conversational fast-path detection ──────────────────────────────────────

fn is_conversational(prompt: &str) -> bool {
    let trimmed = prompt.trim().to_lowercase();
    let word_count = trimmed.split_whitespace().count();

    if word_count <= 3 {
        let greetings = [
            "hi",
            "hello",
            "hey",
            "yo",
            "sup",
            "thanks",
            "thank you",
            "bye",
            "goodbye",
            "ok",
            "okay",
            "yes",
            "no",
            "sure",
            "what's up",
            "whats up",
            "good morning",
            "good evening",
        ];
        if greetings
            .iter()
            .any(|g| trimmed == *g || trimmed.starts_with(g))
        {
            return true;
        }
    }

    if word_count <= 10 && !trimmed.contains('@') && !trimmed.contains('/') {
        let non_code_starts = [
            "who are you",
            "what are you",
            "how are you",
            "what can you",
            "what do you",
            "tell me about yourself",
            "what is your",
            "what's your",
            "are you",
        ];
        if non_code_starts.iter().any(|s| trimmed.starts_with(s)) {
            return true;
        }
    }

    false
}

// ── Client construction ─────────────────────────────────────────────────────

fn build_client(config: &HakariConfig, provider_override: &str) -> Result<Arc<dyn LlmClient>, String> {
    let provider = if provider_override.is_empty() {
        config.active_provider().map(|(name, _)| name.to_string()).unwrap_or_default()
    } else {
        provider_override.to_string()
    };

    match provider.as_str() {
        "copilot" => {
            let key = config.providers.get("copilot")
                .map(|p| p.api_key.as_str())
                .unwrap_or("");
            if key.is_empty() {
                return Err("No Copilot OAuth token. Use /connect to authenticate with GitHub.".into());
            }
            Ok(Arc::new(CopilotLlm::new(key)))
        }
        "gemini" => {
            let key = config.providers.get("gemini")
                .map(|p| p.api_key.as_str())
                .unwrap_or("");
            if key.is_empty() {
                return Err("No Gemini API key configured. Use /connect to set up.".into());
            }
            Ok(Arc::new(GeminiLlm::new(key)))
        }
        _ => {
            let key = config.active_api_key().unwrap_or("");
            if key.is_empty() {
                return Err("No API key configured. Use /connect to set up a provider.".into());
            }
            if config.is_copilot() {
                Ok(Arc::new(CopilotLlm::new(key)))
            } else {
                Ok(Arc::new(GeminiLlm::new(key)))
            }
        }
    }
}

fn shizuka_model_for(config: &HakariConfig, provider_name: &str) -> String {
    let configured = &config.preferences.shizuka_model;
    match provider_name {
        "copilot" => {
            if configured.starts_with("gemini-") {
                crate::copilot::copilot_default_shizuka_model().to_string()
            } else {
                configured.clone()
            }
        }
        _ => configured.clone(),
    }
}

fn nano_model_for(config: &HakariConfig, classification: &str, provider_name: &str) -> String {
    let configured = config.model_for_classification(classification).to_string();
    match provider_name {
        "copilot" => {
            if configured.starts_with("gemini-") {
                "gpt-5-mini".to_string()
            } else {
                configured
            }
        }
        _ => configured,
    }
}

// ── Main entry point ────────────────────────────────────────────────────────

pub async fn run_agent(
    user_prompt: String,
    project_root: String,
    config: HakariConfig,
    tx: mpsc::Sender<AgentEvent>,
    mut cancel_rx: mpsc::Receiver<()>,
) {
    // Determine providers for each phase (hybrid support)
    let shizuka_provider = config.preferences.shizuka_provider.clone();
    let nano_provider = config.preferences.nano_provider.clone();

    let shizuka_client = match build_client(&config, &shizuka_provider) {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(AgentEvent::Error(e)).await;
            return;
        }
    };

    // Fast path: conversational/trivial prompts
    if is_conversational(&user_prompt) {
        let _ = tx.send(AgentEvent::PhaseChange("shizuka".into())).await;

        let system = "You are Hakari, a friendly and helpful coding assistant. Answer conversationally and concisely.";
        let messages = vec![
            llm::LlmMessage::System(system.to_string()),
            llm::LlmMessage::User(user_prompt),
        ];
        let model = shizuka_model_for(&config, shizuka_client.provider_name());

        match shizuka_client.generate(&model, &messages, &[]).await {
            Ok(resp) => {
                if let Some(ref rl) = resp.rate_limits {
                    let _ = tx.send(AgentEvent::CopilotRateLimitUpdate(CopilotRateLimits {
                        total: rl.total, remaining: rl.remaining, reset_at: rl.reset_at,
                    })).await;
                }
                if shizuka_client.provider_name() == "copilot" {
                    let _ = tx.send(AgentEvent::CopilotRequestUsed).await;
                }
                let answer = if resp.text.is_empty() { "Hey!".to_string() } else { resp.text };
                let _ = tx.send(AgentEvent::ShizukaReady {
                    preloaded: Vec::new(), referenced: Vec::new(),
                    task_summary: "Conversational".into(), classification: "trivial".into(),
                }).await;
                let _ = tx.send(AgentEvent::DirectAnswer(answer.clone())).await;
                let _ = tx.send(AgentEvent::Done(answer)).await;
            }
            Err(e) => { let _ = tx.send(AgentEvent::Error(e)).await; }
        }
        return;
    }

    // Load memory system
    let mut memory = MemorySystem::load(&project_root);

    let file_tree = scan_file_tree(&project_root);
    let kpms_context = memory.kpms.to_context_string();
    let kkm_context = memory.kkm.to_context_string();
    let kms_context = memory.kms.to_context_string();

    let shizuka_message = build_shizuka_user_message(
        &user_prompt,
        &file_tree,
        &kpms_context,
        &kkm_context,
        &kms_context,
    );

    let shizuka_model = shizuka_model_for(&config, shizuka_client.provider_name());

    // Request 1: Shizuka
    let preparation = match run_shizuka(
        shizuka_client.as_ref(),
        &shizuka_model,
        &shizuka_message,
        &project_root,
        &tx,
    )
    .await
    {
        Ok(prep) => prep,
        Err(e) => {
            let _ = tx
                .send(AgentEvent::Error(format!("Shizuka failed: {}", e)))
                .await;
            return;
        }
    };
    if shizuka_client.provider_name() == "copilot" {
        let _ = tx.send(AgentEvent::CopilotRequestUsed).await;
    }

    // Direct answer fast-path
    if let Some(ref answer) = preparation.direct_answer {
        if !answer.is_empty() {
            let _ = tx
                .send(AgentEvent::ShizukaReady {
                    preloaded: Vec::new(),
                    referenced: Vec::new(),
                    task_summary: preparation.task_summary.clone(),
                    classification: preparation.task_classification.clone(),
                })
                .await;
            let _ = tx.send(AgentEvent::DirectAnswer(answer.clone())).await;
            let _ = tx.send(AgentEvent::Done(answer.clone())).await;
            return;
        }
    }

    let preloaded: Vec<String> = preparation
        .context_files
        .iter()
        .filter(|cf| cf.role == "modify")
        .map(|cf| cf.path.clone())
        .collect();
    let referenced: Vec<String> = preparation
        .context_files
        .iter()
        .filter(|cf| cf.role == "reference" || cf.role == "context")
        .map(|cf| cf.path.clone())
        .collect();
    let _ = tx
        .send(AgentEvent::ShizukaReady {
            preloaded,
            referenced,
            task_summary: preparation.task_summary.clone(),
            classification: preparation.task_classification.clone(),
        })
        .await;

    // Build nano client (may be different provider for hybrid mode)
    let nano_client = if nano_provider.is_empty() || nano_provider == shizuka_provider {
        Arc::clone(&shizuka_client)
    } else {
        match build_client(&config, &nano_provider) {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(AgentEvent::Error(format!("Nano client error: {}", e))).await;
                return;
            }
        }
    };

    let nano_model = nano_model_for(
        &config,
        &preparation.task_classification,
        nano_client.provider_name(),
    );

    // Request 2: Nano
    match run_nano(
        nano_client.clone(),
        &nano_model,
        &preparation,
        &project_root,
        &tx,
        &mut cancel_rx,
    )
    .await
    {
        Ok((response_text, kms)) => {
            if nano_client.provider_name() == "copilot" {
                let _ = tx.send(AgentEvent::CopilotRequestUsed).await;
            }
            memory.kms = kms;
            memory.post_session_update(&project_root);
            let _ = tx.send(AgentEvent::Done(response_text)).await;
        }
        Err(e) => {
            let _ = tx
                .send(AgentEvent::Error(format!("Nano failed: {}", e)))
                .await;
        }
    }
}
