pub struct Transition {
    pub progress: f32, // linear 0..1=> field that moves
    pub target: f32,   //  where its heading 0 or 1
    pub duration: f32, // seconds for full 0-1
}

impl Transition {
    pub fn tick(&mut self, dt: f32) {
        if self.duration <= 0.0 {
            self.progress = self.target;
            return;
        }
        let step = dt / self.duration;
        if self.progress < self.target {
            self.progress = (self.progress + step).min(self.target);
        } else {
            self.progress = (self.progress - step).max(self.target)
        }
    }

    pub fn new(target: f32, duration: f32) -> Self {
        Self {
            progress: 0.0,
            target,
            duration: duration,
        }
    }

    pub fn in_flight(&self) -> bool {
        self.target != self.progress
    }
}
pub enum Easing {
    Linear,
    EaseOut,
    EaseInOut,
}
impl Easing {
    pub fn apply(&self, t: f32) -> f32 {
        match self {
            Easing::Linear => t,
            Easing::EaseOut => 1.0 - (1.0 - t).powi(2),
            Easing::EaseInOut => t * t * (3.0 - 2.0 * t),
        }
    }
}

pub enum Driver {
    Hover,
    Value(f32),
}
