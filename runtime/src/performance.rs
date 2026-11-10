//! Opt-in diagnostics for a rendered surface. Wall-clock presentation cadence is independent
//! of simulation time; collecting or displaying it never requests another frame.
use std::{
    collections::VecDeque,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

pub const CAPACITY: usize = 240;
pub const BUDGET_MS: f64 = 1000.0 / 60.0;

pub(crate) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("OSVAULD_PERFORMANCE").is_ok_and(|v| v == "1"))
}

/// Plain observational data for a host. Work includes CPU time and waits, not GPU completion.
#[derive(Clone, Debug)]
pub struct FramePerformance {
    pub mode: &'static str,
    pub normal_frames: u64,
    pub presented_frames: u64,
    pub samples: usize,
    pub work_last_ms: f64,
    pub work_p50_ms: f64,
    pub work_p95_ms: f64,
    pub work_max_ms: f64,
    pub over_budget: usize,
    pub viewport: (f32, f32),
    pub scale: f32,
    pub adapter: Arc<str>,
    pub label: Arc<str>,
    fps: Option<f64>,
    measured_at: Instant,
}
impl FramePerformance {
    pub fn fps(&self) -> Option<f64> {
        self.fps_at(Instant::now())
    }
    pub fn age_ms(&self) -> f64 {
        self.measured_at.elapsed().as_secs_f64() * 1000.0
    }
    fn fps_at(&self, now: Instant) -> Option<f64> {
        if now.saturating_duration_since(self.measured_at) > Duration::from_secs(1) {
            None
        } else {
            self.fps
        }
    }
    pub fn overlay_rect(&self, viewport: (f32, f32)) -> [f32; 4] {
        let width = 320.0_f32.min((viewport.0 - 24.0).max(0.0));
        [
            (viewport.0 - width - 12.0).max(0.0),
            (viewport.1 - 72.0).max(0.0),
            width,
            60.0,
        ]
    }
}

pub(crate) struct Monitor {
    costs: VecDeque<f64>,
    presentations: VecDeque<Instant>,
    published: FramePerformance,
    last_publish: Option<Instant>,
}
impl Monitor {
    pub fn new(mode: &'static str, viewport: (f32, f32), scale: f32, adapter: Arc<str>) -> Self {
        Self {
            costs: VecDeque::with_capacity(CAPACITY),
            presentations: VecDeque::with_capacity(CAPACITY + 1),
            last_publish: None,
            published: FramePerformance {
                mode,
                viewport,
                scale,
                adapter,
                normal_frames: 0,
                presented_frames: 0,
                samples: 0,
                work_last_ms: 0.0,
                work_p50_ms: 0.0,
                work_p95_ms: 0.0,
                work_max_ms: 0.0,
                over_budget: 0,
                fps: None,
                measured_at: Instant::now(),
                label: "FPS — (warming up)\nwork —\ntarget 60 · CPU + waits".into(),
            },
        }
    }
    pub fn viewport(&mut self, viewport: (f32, f32), scale: f32) {
        self.published.viewport = viewport;
        self.published.scale = scale;
    }
    pub fn snapshot(&self) -> FramePerformance {
        self.published.clone()
    }
    pub fn label(&self) -> &str {
        &self.published.label
    }
    pub fn overlay_rect(&self, viewport: (f32, f32)) -> [f32; 4] {
        self.published.overlay_rect(viewport)
    }
    pub fn record(&mut self, now: Instant, work_ms: f64, presented: bool, capture: bool) -> bool {
        if capture {
            return false;
        }
        if self.costs.len() == CAPACITY {
            self.costs.pop_front();
        }
        self.costs.push_back(work_ms);
        self.published.normal_frames = self.published.normal_frames.saturating_add(1);
        if presented {
            if self.presentations.len() == CAPACITY + 1 {
                self.presentations.pop_front();
            }
            self.presentations.push_back(now);
            self.published.presented_frames = self.published.presented_frames.saturating_add(1);
        }
        while self
            .presentations
            .front()
            .is_some_and(|t| now.saturating_duration_since(*t) > Duration::from_secs(1))
        {
            self.presentations.pop_front();
        }
        // The panel changes at most four times a second. Sorting and formatting are absent
        // from the other frames; the ring itself only exists when diagnostics are enabled.
        if self
            .last_publish
            .is_some_and(|t| now.saturating_duration_since(t) < Duration::from_millis(250))
        {
            return false;
        }
        self.last_publish = Some(now);
        let count = self.costs.len();
        let mut values = [0.0; CAPACITY];
        for (to, from) in values.iter_mut().zip(&self.costs) {
            *to = *from;
        }
        values[..count].sort_by(f64::total_cmp);
        let percentile = |p: f64| values[(count as f64 * p).ceil() as usize - 1];
        let fps = if self.published.mode == "windowed" && self.presentations.len() > 1 {
            let seconds = self
                .presentations
                .back()
                .unwrap()
                .duration_since(*self.presentations.front().unwrap())
                .as_secs_f64();
            (seconds > 0.0).then(|| (self.presentations.len() - 1) as f64 / seconds)
        } else {
            None
        };
        let s = &mut self.published;
        s.samples = count;
        s.work_last_ms = work_ms;
        s.work_p50_ms = percentile(0.5);
        s.work_p95_ms = percentile(0.95);
        s.work_max_ms = values[count - 1];
        s.over_budget = values[..count].iter().filter(|v| **v > BUDGET_MS).count();
        s.fps = fps;
        s.measured_at = now;
        let cadence = fps
            .map(|v| format!("{v:.1} FPS recent · {:.1} ms", 1000.0 / v))
            .unwrap_or_else(|| {
                format!(
                    "FPS — ({})",
                    if s.mode == "offscreen" {
                        "offscreen"
                    } else {
                        "warming up / idle"
                    }
                )
            });
        s.label = format!(
            "{cadence}\nwork {:.1} ms · p95 {:.1} ms\ntarget 60 · {} · CPU + waits",
            s.work_last_ms,
            s.work_p95_ms,
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
        )
        .into();
        true
    }
}

#[cfg(test)]
mod tests;
