#[derive(Clone, Copy, Debug)]
pub struct Spring {
    pub value: f32,
    pub velocity: f32,
    pub target: f32,
}
impl Spring {
    pub fn new(value: f32) -> Self {
        Self {
            value,
            velocity: 0.,
            target: value,
        }
    }
    pub fn set(&mut self, target: f32, immediate: bool) {
        if !target.is_finite() {
            return;
        }
        self.target = target;
        if immediate {
            self.value = target;
            self.velocity = 0.;
        }
    }
    pub fn active(&self) -> bool {
        (self.value - self.target).abs() > 0.01 || self.velocity.abs() > 0.01
    }
    // A fixed 240Hz internal integration step makes the response independent of
    // monitor refresh rate. Coefficients are calibrated separately from Svelte.
    pub fn advance(&mut self, dt: f32) {
        if !self.active() {
            self.value = self.target;
            self.velocity = 0.;
            return;
        }
        let dt = dt.clamp(0., 0.1);
        let steps = (dt * 240.).ceil().max(1.) as u32;
        let h = dt / steps as f32;
        for _ in 0..steps {
            self.velocity += ((self.target - self.value) * 620. - self.velocity * 40.) * h;
            self.value += self.velocity * h;
        }
        if !self.active() {
            self.value = self.target;
            self.velocity = 0.;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interrupted_motion_preserves_velocity_and_converges() {
        let mut s = Spring::new(80.);
        s.set(300., false);
        s.advance(0.05);
        let v = s.velocity;
        let x = s.value;
        s.set(90., false);
        assert_eq!(v, s.velocity);
        assert_eq!(x, s.value);
        for _ in 0..240 {
            s.advance(1. / 120.);
        }
        assert_eq!(s.value, 90.);
        assert!(!s.active());
    }
    #[test]
    fn refresh_rates_agree() {
        let mut a = Spring::new(0.);
        a.set(200., false);
        let mut b = a;
        for _ in 0..30 {
            a.advance(1. / 60.);
        }
        for _ in 0..60 {
            b.advance(1. / 120.);
        }
        assert!((a.value - b.value).abs() < 0.02);
    }
    #[test]
    fn immediate_and_invalid_targets() {
        let mut s = Spring::new(0.);
        s.set(30., true);
        assert!(!s.active());
        s.set(f32::NAN, false);
        assert_eq!(s.target, 30.);
    }
}
