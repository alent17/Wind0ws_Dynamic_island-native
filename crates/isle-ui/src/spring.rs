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
    // Fractional powers of the original Svelte 60 Hz recurrence:
    // v' = .2 v + .18 (target - x), x' = x + v'. This preserves
    // its sampled trajectory without stepping visually at 60 Hz on faster displays.
    pub fn advance(&mut self, dt: f32) {
        if !self.active() {
            self.value = self.target;
            self.velocity = 0.;
            return;
        }
        let dt = dt.clamp(0., 0.1);
        let discriminant = (1.02_f32 * 1.02 - 0.8).sqrt();
        let a = (1.02 + discriminant) / 2.;
        let b = (1.02 - discriminant) / 2.;
        let power = dt * 60.;
        let alpha = (a.powf(power) - b.powf(power)) / (a - b);
        let beta = (a * b.powf(power) - b * a.powf(power)) / (a - b);
        let displacement = self.value - self.target;
        let velocity = self.velocity / 60.;
        self.value = self.target + (alpha * 0.82 + beta) * displacement + alpha * 0.2 * velocity;
        self.velocity = (alpha * -0.18 * displacement + (alpha * 0.2 + beta) * velocity) * 60.;
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
    fn matches_original_svelte_sixty_hz_trajectory() {
        let mut s = Spring::new(80.);
        s.set(364., false);
        let (mut value, mut velocity) = (80_f32, 0_f32);
        for _ in 0..24 {
            velocity = velocity * 0.2 + (364. - value) * 0.18;
            value += velocity;
            s.advance(1. / 60.);
            assert!((s.value - value).abs() < 0.001);
        }
    }
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
