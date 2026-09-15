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
pub struct Spring {
    pub value: f32,
    pub velocity: f32,
    pub target: f32,
}

impl Spring {
    pub fn new(target: f32) -> Self {
        Self {
            value: 0.0,
            velocity: 0.0,
            target,
        }
    }

    pub fn tick(&mut self, dt: f32) {
        self.target = self.target.clamp(0.0, 1.0);
        let stiffness = 260.0;
        let damping = 30.0;
        let mut remaining = dt.min(0.1);
        while remaining > 0.0 {
            let step = remaining.min(1.0 / 120.0);
            let force = (self.target - self.value) * stiffness;
            self.velocity += (force - self.velocity * damping) * step;
            self.value += self.velocity * step;
            self.value = self.value.clamp(-0.1, 1.1);
            remaining -= step;
        }
        if self.velocity.abs() < 0.001 && (self.target - self.value).abs() < 0.001 {
            self.value = self.target;
            self.velocity = 0.0;
        }
    }

    pub fn in_flight(&self) -> bool {
        self.velocity != 0.0 || self.value != self.target
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
    Press,
    Value(f32),
}
