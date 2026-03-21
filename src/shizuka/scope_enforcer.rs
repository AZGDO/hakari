use std::collections::HashSet;

pub struct ScopeEnforcer {
    scope_files: HashSet<String>,
    out_of_scope_writes: Vec<String>,
    read_files: HashSet<String>,
}

impl ScopeEnforcer {
    pub fn new(scope_files: Vec<String>) -> Self {
        Self {
            scope_files: scope_files.into_iter().collect(),
            out_of_scope_writes: Vec::new(),
            read_files: HashSet::new(),
        }
    }

    pub fn check_write(&mut self, path: &str) -> Option<String> {
        if self.scope_files.is_empty() {
            return None;
        }

        if self.scope_files.contains(path) {
            return None;
        }

        // Check if it's a dependency of a scoped file (one hop)
        let is_one_hop = self.scope_files.iter().any(|sf| {
            let sf_parent = std::path::Path::new(sf).parent();
            let path_parent = std::path::Path::new(path).parent();
            sf_parent == path_parent
        });

        if is_one_hop {
            return Some(format!(
                "Note: modifying '{}' which is adjacent to the task scope.",
                path
            ));
        }

        self.out_of_scope_writes.push(path.to_string());

        if self.out_of_scope_writes.len() > 3 {
            Some(format!(
                "You appear to be drifting from the original task. Currently in scope: [{}]. You've made {} out-of-scope modifications. Please refocus or explain why these changes are necessary.",
                self.scope_files.iter().cloned().collect::<Vec<_>>().join(", "),
                self.out_of_scope_writes.len()
            ))
        } else {
            Some(format!(
                "Warning: '{}' is outside the initial task scope. Confirm this is necessary.",
                path
            ))
        }
    }

    pub fn record_read(&mut self, path: &str) {
        self.read_files.insert(path.to_string());
    }

    pub fn get_unpredicted_reads(&self) -> Vec<String> {
        self.read_files
            .iter()
            .filter(|p| !self.scope_files.contains(*p))
            .cloned()
            .collect()
    }
}
