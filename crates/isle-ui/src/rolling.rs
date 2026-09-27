//! RollingDigit.svelte's shortest decimal path and 280 ms easing.
#[derive(Clone, Debug)]
pub struct Digit {
    digit: u32,
    from: f32,
    target: f32,
    started: f64,
}
#[derive(Clone, Debug)]
pub struct Tween {
    from: f32,
    target: f32,
    started: f64,
    duration: f64,
}
impl Tween {
    pub fn new(value: f32) -> Self {
        Self {
            from: value,
            target: value,
            started: -1.,
            duration: 0.,
        }
    }
    pub fn value(&self, now: f64) -> f32 {
        if !self.active(now) {
            self.target
        } else {
            self.from
                + (self.target - self.from)
                    * ease(((now - self.started) / self.duration).clamp(0., 1.) as f32)
        }
    }
    pub fn update(&mut self, target: f32, now: f64, duration: f64, immediate: bool) -> f32 {
        if immediate {
            *self = Self::new(target);
        } else if target != self.target {
            self.from = self.value(now);
            self.target = target;
            self.started = now;
            self.duration = duration;
        }
        self.value(now)
    }
    pub fn active(&self, now: f64) -> bool {
        self.from != self.target && now - self.started < self.duration
    }
}
fn ease(t: f32) -> f32 {
    if t <= 0. {
        return 0.;
    }
    // Invert cubic-bezier(.22, 1, .36, 1)'s x coordinate.
    let (mut lo, mut hi) = (0., 1.);
    for _ in 0..16 {
        let u = (lo + hi) / 2.;
        let x = 3. * (1. - u) * (1. - u) * u * 0.22 + 3. * (1. - u) * u * u * 0.36 + u * u * u;
        if x < t {
            lo = u;
        } else {
            hi = u;
        }
    }
    1. - (1. - (lo + hi) / 2.).powi(3)
}
impl Digit {
    pub fn new(digit: u32) -> Self {
        Self {
            digit,
            from: digit as f32,
            target: digit as f32,
            started: -1.,
        }
    }
    pub fn position(&self, now: f64) -> f32 {
        let t = ((now - self.started) / 0.28).clamp(0., 1.) as f32;
        if t >= 1. {
            self.target
        } else {
            self.from + (self.target - self.from) * ease(t)
        }
    }
    pub fn update(&mut self, digit: u32, now: f64, reduced: bool) -> f32 {
        if reduced {
            *self = Self::new(digit);
        } else if digit != self.digit {
            let position = self.position(now);
            let step = (digit as i32 - self.digit as i32 + 15) % 10 - 5;
            // Recentering prevents long-running clocks from accumulating floats.
            let offset = self.target - self.digit as f32;
            self.from = position - offset;
            self.target = self.digit as f32 + step as f32;
            self.digit = digit;
            self.started = now;
        }
        self.position(now)
    }
    pub fn active(&self, now: f64) -> bool {
        self.from != self.target && now - self.started < 0.28
    }
}

pub fn page_ease(t: f32) -> f32 {
    if t <= 0. {
        return 0.;
    }
    if t >= 1. {
        return 1.;
    }
    let (mut lo, mut hi) = (0., 1.);
    for _ in 0..16 {
        let u = (lo + hi) / 2.;
        let x = 3. * (1. - u) * u * 0.25 + u * u * u;
        if x < t {
            lo = u;
        } else {
            hi = u;
        }
    }
    let u = (lo + hi) / 2.;
    3. * (1. - u) * (1. - u) * u * 0.1 + 3. * (1. - u) * u * u + u * u * u
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ruler_retargets_continuously_and_dragging_is_immediate() {
        let mut t = Tween::new(20.);
        t.update(90., 0., 0.26, false);
        let before = t.value(0.1);
        assert_eq!(t.update(35., 0.1, 0.26, false), before);
        assert_eq!(t.update(40., 0.12, 0.26, true), 40.);
        assert!(!t.active(0.12));
    }
    #[test]
    fn wraps_and_interruption_starts_from_current_position() {
        let mut d = Digit::new(9);
        assert_eq!(d.update(0, 1., false), 9.);
        assert_eq!(d.position(1.3), 10.);
        d.update(9, 2., false);
        let before = d.position(2.08);
        let after = d.update(8, 2.08, false);
        assert!((before.rem_euclid(10.) - after.rem_euclid(10.)).abs() < 0.001);
        assert_eq!(d.position(3.).rem_euclid(10.), 8.);
        assert!(!d.active(3.));
    }
    #[test]
    fn reduced_motion_snaps_and_releases_frames() {
        let mut d = Digit::new(3);
        d.update(7, 1., false);
        assert!(d.active(1.1));
        assert_eq!(d.update(7, 1.1, true), 7.);
        assert!(!d.active(1.1));
    }
}
