use super::kms::Kms;
use super::kpms::Kpms;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparationMiss {
    pub task_type: String,
    pub target_files: Vec<String>,
    pub missed_file: String,
    pub reason_needed: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationRecord {
    pub task_type: String,
    pub iterations: usize,
    pub writes_per_file: std::collections::HashMap<String, usize>,
    pub success: bool,
    pub strategy: String,
}

pub fn collect_preparation_misses(
    kms: &Kms,
    preloaded_files: &[String],
    reference_files: &[String],
) -> Vec<PreparationMiss> {
    let mut misses = Vec::new();
    let predicted: std::collections::HashSet<&str> = preloaded_files
        .iter()
        .chain(reference_files.iter())
        .map(|s| s.as_str())
        .collect();

    for step in &kms.steps.history {
        if step.tool == "Read" {
            let path = &step.params_summary;
            if !predicted.contains(path.as_str()) {
                misses.push(PreparationMiss {
                    task_type: kms.task.classification.to_string(),
                    target_files: preloaded_files.to_vec(),
                    missed_file: path.clone(),
                    reason_needed: format!("Nano read this file at step {}", step.step),
                });
            }
        }
    }
    misses
}

pub fn collect_iteration_record(kms: &Kms) -> IterationRecord {
    let mut writes_per_file = std::collections::HashMap::new();
    for step in &kms.steps.history {
        if step.tool == "Write" {
            *writes_per_file
                .entry(step.params_summary.clone())
                .or_insert(0) += 1;
        }
    }

    let success = kms.errors.iter().all(|e| e.resolution_status == "resolved");

    IterationRecord {
        task_type: kms.task.classification.to_string(),
        iterations: kms.steps.current,
        writes_per_file,
        success,
        strategy: kms.task.goal.clone(),
    }
}

pub fn persist_improvements(
    kpms: &mut Kpms,
    kms: &Kms,
    misses: &[PreparationMiss],
    record: &IterationRecord,
    session_id: &str,
) {
    for miss in misses {
        kpms.add_learning(
            &format!(
                "When working on {} with files {:?}",
                miss.task_type, miss.target_files
            ),
            &format!(
                "Also need file: {} ({})",
                miss.missed_file, miss.reason_needed
            ),
            session_id,
        );
    }

    let existing_strategy = kpms
        .strategies
        .iter_mut()
        .find(|s| s.task_type == record.task_type);
    if let Some(strategy) = existing_strategy {
        if record.success {
            strategy.success_count += 1;
        } else {
            strategy.failure_count += 1;
        }
        let total = (strategy.success_count + strategy.failure_count) as f64;
        strategy.avg_iterations =
            (strategy.avg_iterations * (total - 1.0) + record.iterations as f64) / total;
    } else {
        kpms.strategies.push(super::kpms::Strategy {
            task_type: record.task_type.clone(),
            approach: record.strategy.clone(),
            success_count: if record.success { 1 } else { 0 },
            failure_count: if record.success { 0 } else { 1 },
            avg_iterations: record.iterations as f64,
        });
    }

    for path in kms.files.index.keys() {
        if let Some(info) = kms.files.index.get(path) {
            if !info.purpose.is_empty() {
                kpms.update_file_index(path, &info.purpose);
            }
        }
    }
}
