//! Ephemeral creation feedback and job tokens; accepted writes belong to their original account.
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_JOB: AtomicU64 = AtomicU64::new(1);

#[derive(Default)]
pub(crate) struct Creation {
    pub pending: Option<u64>,
    pub generation: u64,
    pub error: Option<String>,
}

impl Creation {
    pub fn begin(&mut self) -> Option<u64> {
        if self.pending.is_some() {
            return None;
        }
        let job = NEXT_JOB.fetch_add(1, Ordering::Relaxed);
        self.pending = Some(job);
        self.error = None;
        Some(job)
    }

    pub fn finish(&mut self, job: u64, error: Option<String>) -> bool {
        if self.pending != Some(job) {
            return false;
        }
        self.pending = None;
        self.generation += 1;
        self.error = error;
        true
    }
}

pub(crate) fn name(value: &str) -> Result<String, String> {
    if value.len() > 256 {
        return Err("workspace name must be at most 256 bytes".into());
    }
    let name = value.trim();
    if name.is_empty() {
        return Err("enter a workspace name".into());
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests;
