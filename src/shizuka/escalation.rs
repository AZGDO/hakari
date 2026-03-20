use crate::memory::kms::Kms;

#[derive(Debug, Clone, PartialEq)]
pub enum EscalationLevel {
    None,
    SoftRedirection,
    HardConstraint,
    UserEscalation,
}

pub struct EscalationEngine {
    current_level: EscalationLevel,
    failed_approaches: Vec<String>,
}

impl EscalationEngine {
    pub fn new() -> Self {
        Self {
            current_level: EscalationLevel::None,
            failed_approaches: Vec::new(),
        }
    }

    pub fn evaluate(&mut self, kms: &Kms, max_tool_calls: usize) -> EscalationAction {
        let step = kms.steps.current;
        let error_count = kms.errors.iter()
            .filter(|e| e.resolution_status != "resolved")
            .count();
        let failed_attempts = kms.task.attempt_history.iter()
            .filter(|a| a.reason_for_failure.is_some())
            .count();

        // Check iteration budget
        if step >= max_tool_calls {
            self.current_level = EscalationLevel::UserEscalation;
            return EscalationAction::HardStop {
                message: format!(
                    "Iteration limit reached ({}/{}). Summarize your progress and remaining issues, then stop.",
                    step, max_tool_calls
                ),
            };
        }

        // Progressive escalation
        if (failed_attempts >= 3 || error_count >= 5)
            && self.current_level != EscalationLevel::UserEscalation
        {
            self.current_level = EscalationLevel::UserEscalation;
            return EscalationAction::UserEscalation {
                summary: build_escalation_summary(kms),
            };
        } else if (failed_attempts >= 2 || error_count >= 3)
            && self.current_level != EscalationLevel::HardConstraint
            && self.current_level != EscalationLevel::UserEscalation
        {
            self.current_level = EscalationLevel::HardConstraint;
            return EscalationAction::HardConstraint {
                message: "Multiple failed attempts detected. Consider reading related files for additional context before attempting another fix.".to_string(),
                restrict_writes: find_problematic_files(kms),
            };
        } else if (failed_attempts >= 1 || error_count >= 2)
            && self.current_level == EscalationLevel::None
        {
            self.current_level = EscalationLevel::SoftRedirection;
            let approaches: Vec<String> = kms.task.attempt_history.iter()
                .map(|a| a.approach_description.clone())
                .collect();
            return EscalationAction::SoftRedirection {
                message: format!(
                    "Previous approaches tried: [{}]. Consider an alternative strategy.",
                    approaches.join("; ")
                ),
            };
        }

        EscalationAction::Continue
    }

    pub fn record_failed_approach(&mut self, description: &str) {
        self.failed_approaches.push(description.to_string());
    }

    pub fn reset(&mut self) {
        self.current_level = EscalationLevel::None;
        self.failed_approaches.clear();
    }
}

#[derive(Debug)]
pub enum EscalationAction {
    Continue,
    SoftRedirection { message: String },
    HardConstraint { message: String, restrict_writes: Vec<String> },
    HardStop { message: String },
    UserEscalation { summary: String },
}

fn build_escalation_summary(kms: &Kms) -> String {
    let mut summary = String::new();
    summary.push_str(&format!("## Task Summary\n{}\n\n", kms.task.goal));

    summary.push_str("## Approaches Tried\n");
    for (i, attempt) in kms.task.attempt_history.iter().enumerate() {
        summary.push_str(&format!(
            "{}. {}\n   Result: {}\n",
            i + 1,
            attempt.approach_description,
            attempt.reason_for_failure.as_deref().unwrap_or("unknown")
        ));
    }

    summary.push_str("\n## Unresolved Errors\n");
    for error in &kms.errors {
        if error.resolution_status != "resolved" {
            summary.push_str(&format!(
                "- Step {}: {}\n",
                error.step, error.error_message
            ));
        }
    }

    summary.push_str("\n## Modified Files\n");
    for (path, info) in &kms.files.index {
        if info.is_modified {
            summary.push_str(&format!("- {}\n", path));
        }
    }

    summary
}

fn find_problematic_files(kms: &Kms) -> Vec<String> {
    kms.files.index.iter()
        .filter(|(_, info)| {
            info.is_modified && kms.errors.iter().any(|e| {
                e.file.as_deref() == Some(info.purpose.as_str()) && e.resolution_status != "resolved"
            })
        })
        .map(|(path, _)| path.clone())
        .collect()
}
