//! Bounded fixed-step planning on absolute native time. Captures must not call this clock.
const STEP: f64 = 1.0 / crate::physics3d::HZ as f64;
const MAX_STEPS: u8 = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AdvanceReport {
    pub steps: u8,
    pub dropped_seconds: f64,
}

#[derive(Debug, thiserror::Error, PartialEq)]
#[error("3D clock requires finite, nonnegative, monotonic elapsed time")]
pub struct ClockError;

#[derive(Default)]
pub(super) struct FixedClock {
    last: Option<f64>,
    running: bool,
    carry: f64,
    pub dropped_seconds: f64,
}

impl FixedClock {
    pub fn pause(&mut self) {
        self.running = false;
        self.carry = 0.0;
    }

    pub fn advance(&mut self, elapsed: f64, running: bool) -> Result<AdvanceReport, ClockError> {
        if !elapsed.is_finite() || elapsed < 0.0 || self.last.is_some_and(|last| elapsed < last) {
            return Err(ClockError);
        }
        let previous = self.last.replace(elapsed);
        if !running || !self.running {
            self.pause();
            self.running = running;
            return Ok(AdvanceReport::default());
        }
        let available = self.carry + (elapsed - previous.expect("running clock has a baseline"));
        let window = available.min(STEP * f64::from(MAX_STEPS));
        let steps = ((window / STEP + 1e-9).floor() as u8).min(MAX_STEPS);
        self.carry = (window - f64::from(steps) * STEP).max(0.0);
        let dropped_seconds = (available - window).max(0.0);
        self.dropped_seconds += dropped_seconds;
        Ok(AdvanceReport {
            steps,
            dropped_seconds,
        })
    }
}

#[cfg(test)]
mod tests;
