//! On-demand hit geometry from live app threads. Collection has one shared deadline; snapshots
//! are scoped to an account, app incarnation and tile size before their coordinates are returned.

use std::sync::{Arc, mpsc::Receiver};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scope {
    pub instance: u64,
    pub revision: u64,
    pub size: Option<(f32, f32)>,
}

#[derive(Clone)]
pub struct Snapshot {
    pub id: Arc<str>,
    pub scope: Scope,
    pub rects: Arc<[runtime::ElRect]>,
}

pub struct Request {
    pub id: Arc<str>,
    pub scope: Scope,
    pub reply: Receiver<Result<Snapshot, String>>,
}

pub fn collect(requests: Vec<Request>, timeout: Duration) -> Result<Vec<Snapshot>, String> {
    let deadline = std::time::Instant::now() + timeout;
    requests.into_iter().map(|request| {
        let snapshot = request.reply.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .map_err(|_| "app geometry inspection timed out or disconnected".to_string())??;
        if snapshot.id != request.id || snapshot.scope != request.scope {
            return Err("app layout changed before geometry was painted; retry".into());
        }
        Ok(snapshot)
    }).collect()
}

pub fn validate(generation: u64, current: u64, snapshots: &[Snapshot],
    lookup: impl Fn(&str) -> Option<Scope>) -> Result<(), String>
{
    if generation != current { return Err("account changed during geometry inspection".into()); }
    if snapshots.iter().any(|s| lookup(&s.id) != Some(s.scope)) {
        return Err("app layout changed during geometry inspection; retry".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
