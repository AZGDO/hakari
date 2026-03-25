pub mod kkm;
pub mod kms;
pub mod kpms;

use crate::memory::kkm::KKM;
use crate::memory::kms::KMS;
use crate::memory::kpms::KPMS;

pub struct MemorySystem {
    pub kpms: KPMS,
    pub kms: KMS,
    pub kkm: KKM,
}

impl MemorySystem {
    pub fn load(project_dir: &str) -> Self {
        Self {
            kpms: KPMS::load(project_dir),
            kms: KMS::new(),
            kkm: KKM::load(),
        }
    }

    pub fn post_session_update(&mut self, project_dir: &str) {
        // Extract learnings from KMS into KPMS
        for miss in &self.kms.preparation_misses {
            self.kpms.add_learning(format!(
                "When working on similar tasks, also include file: {}",
                miss
            ));
        }

        for (path, desc) in &self.kms.file_descriptions {
            self.kpms.file_index.insert(path.clone(), desc.clone());
        }

        for anti in &self.kms.failed_approaches {
            self.kpms.add_anti_pattern(anti.clone());
        }

        if let Some(ref strategy) = self.kms.successful_strategy {
            self.kpms
                .add_strategy(&self.kms.task_type, strategy.clone());
        }

        self.kpms.prune();
        self.kpms.session_count += 1;
        let _ = self.kpms.save(project_dir);
        let _ = self.kkm.save();
    }
}
