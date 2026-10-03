use crate::{geometry::Rect, spring::Spring};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionProfile {
    Surface,
    Content,
    Micro,
}

impl MotionProfile {
    /// Time multipliers over the existing refresh-rate independent Spring.
    /// Surface remains grounded, content catches up a little sooner, and
    /// small controls settle quickly without introducing a second spring law.
    pub const fn time_scale(self) -> f32 {
        match self {
            Self::Surface => 0.88,
            Self::Content => 1.15,
            Self::Micro => 1.42,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionPolicy {
    pub reduced: bool,
    pub time_scale: f32,
}

impl Default for MotionPolicy {
    fn default() -> Self {
        Self {
            reduced: false,
            time_scale: 1.,
        }
    }
}

impl MotionPolicy {
    pub fn dt(self, dt: f32, profile: MotionProfile) -> f32 {
        if self.reduced {
            0.
        } else {
            (dt.max(0.) * self.time_scale.clamp(0.05, 8.) * profile.time_scale()).min(0.1)
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AnimatedRect {
    pub x: Spring,
    pub y: Spring,
    pub w: Spring,
    pub h: Spring,
}

impl Default for AnimatedRect {
    fn default() -> Self {
        Self::new(Rect::default())
    }
}

impl AnimatedRect {
    pub fn new(rect: Rect) -> Self {
        Self {
            x: Spring::new(rect.x),
            y: Spring::new(rect.y),
            w: Spring::new(rect.w),
            h: Spring::new(rect.h),
        }
    }

    pub fn rect(self) -> Rect {
        Rect {
            x: self.x.value,
            y: self.y.value,
            w: self.w.value.max(0.),
            h: self.h.value.max(0.),
        }
    }

    pub fn target(&self) -> Rect {
        Rect {
            x: self.x.target,
            y: self.y.target,
            w: self.w.target.max(0.),
            h: self.h.target.max(0.),
        }
    }

    pub fn set(&mut self, rect: Rect, immediate: bool) {
        self.x.set(rect.x, immediate);
        self.y.set(rect.y, immediate);
        self.w.set(rect.w.max(0.), immediate);
        self.h.set(rect.h.max(0.), immediate);
    }

    pub fn advance(&mut self, dt: f32, profile: MotionProfile, policy: MotionPolicy) {
        let dt = policy.dt(dt, profile);
        self.x.advance(dt);
        self.y.advance(dt);
        self.w.advance(dt);
        self.h.advance(dt);
    }

    pub fn active(&self) -> bool {
        self.x.active() || self.y.active() || self.w.active() || self.h.active()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct VisualElement {
    pub rect: AnimatedRect,
    pub opacity: Spring,
    pub radius: Spring,
    pub scale: Spring,
    pub profile: MotionProfile,
}

impl VisualElement {
    pub fn new(rect: Rect, radius: f32, profile: MotionProfile) -> Self {
        Self {
            rect: AnimatedRect::new(rect),
            opacity: Spring::new(1.),
            radius: Spring::new(radius),
            scale: Spring::new(1.),
            profile,
        }
    }

    pub fn set(&mut self, rect: Rect, radius: f32, opacity: f32, policy: MotionPolicy) {
        self.rect.set(rect, policy.reduced);
        self.radius.set(radius, policy.reduced);
        self.opacity.set(opacity.clamp(0., 1.), policy.reduced);
    }

    pub fn advance(&mut self, dt: f32, policy: MotionPolicy) {
        self.rect.advance(dt, self.profile, policy);
        let dt = policy.dt(dt, self.profile);
        self.opacity.advance(dt);
        self.radius.advance(dt);
        self.scale.advance(dt);
    }

    pub fn active(&self) -> bool {
        self.rect.active() || self.opacity.active() || self.radius.active() || self.scale.active()
    }
}
