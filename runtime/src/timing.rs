use std::{sync::OnceLock, time::Instant};

static ENABLED: OnceLock<bool> = OnceLock::new();

/// CPU wall time only: present includes surface wait/encoding/submission, not GPU completion
/// or display scanout. Disabled tracing does not read clocks or allocate per frame.
pub(crate) struct FrameTiming {
    start: Option<Instant>,
    last: Option<Instant>,
    phases: [f64; 6],
}

impl FrameTiming {
    pub fn start() -> Self {
        let enabled =
            *ENABLED.get_or_init(|| std::env::var("OSVAULD_FRAME_TIMINGS").is_ok_and(|v| v == "1"));
        Self::at(enabled.then(Instant::now))
    }

    fn at(start: Option<Instant>) -> Self {
        Self {
            start,
            last: start,
            phases: [0.0; 6],
        }
    }

    pub fn mark(&mut self, phase: usize) {
        if self.last.is_some() {
            self.mark_at(phase, Instant::now());
        }
    }

    fn mark_at(&mut self, phase: usize, now: Instant) {
        if let Some(last) = self.last {
            self.phases[phase] = now.duration_since(last).as_secs_f64() * 1000.0;
            self.last = Some(now);
        }
    }

    pub fn finish(self, capture: bool) {
        if let Some(start) = self.start {
            let [prepare, view, layout, reconcile, paint, present] = self.phases;
            eprintln!(
                "OSVAULD_FRAME prepare_ms={prepare:.3} view_ms={view:.3} layout_ms={layout:.3} reconcile_ms={reconcile:.3} paint_ms={paint:.3} present_capture_ms={present:.3} total_ms={:.3} capture={capture}",
                start.elapsed().as_secs_f64() * 1000.0
            );
        }
    }
}

#[cfg(test)]
mod tests;
