//! Bounded DSP state; no audio samples leave the capture thread.
use rustfft::{num_complex::Complex, Fft, FftPlanner};
use std::sync::Arc;
pub const SIZE: usize = 2048;
pub const BANDS: usize = 6;
const RANGES: [(f32, f32); BANDS] = [
    (20., 250.),
    (250., 600.),
    (600., 2000.),
    (2000., 5000.),
    (5000., 10000.),
    (10000., 20000.),
];
const GAINS: [f32; BANDS] = [1.15, 1.65, 2.2, 3.4, 5.4, 8.];
pub struct Analyzer {
    fft: Arc<dyn Fft<f32>>,
    ring: Box<[f32; SIZE]>,
    window: Box<[f32; SIZE]>,
    buffer: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    position: usize,
    rate: f32,
    levels: [f32; BANDS],
}
impl Analyzer {
    pub fn new(rate: u32) -> Self {
        let fft = FftPlanner::new().plan_fft_forward(SIZE);
        let scratch = vec![Complex::default(); fft.get_inplace_scratch_len()];
        Self {
            fft,
            scratch,
            ring: Box::new([0.; SIZE]),
            window: Box::new(std::array::from_fn(|i| {
                0.5 * (1. - (2. * std::f32::consts::PI * i as f32 / (SIZE - 1) as f32).cos())
            })),
            buffer: vec![Complex::default(); SIZE],
            position: 0,
            rate: rate.max(1) as f32,
            levels: [0.; BANDS],
        }
    }
    pub fn push(&mut self, sample: f32) {
        self.ring[self.position] = if sample.is_finite() {
            sample.clamp(-1., 1.)
        } else {
            0.
        };
        self.position = (self.position + 1) % SIZE;
    }
    pub fn clear(&mut self) {
        self.ring.fill(0.);
        self.levels.fill(0.);
        self.position = 0;
    }
    pub fn analyze(&mut self, seconds: f32) -> [f32; BANDS] {
        for i in 0..SIZE {
            self.buffer[i] =
                Complex::new(self.ring[(self.position + i) % SIZE] * self.window[i], 0.);
        }
        self.fft
            .process_with_scratch(&mut self.buffer, &mut self.scratch);
        for (i, &(low, high)) in RANGES.iter().enumerate() {
            let lo = ((low * SIZE as f32 / self.rate) as usize).max(1);
            let hi = ((high * SIZE as f32 / self.rate) as usize).min(SIZE / 2);
            let target = if lo >= hi {
                0.
            } else {
                let power = self.buffer[lo..hi]
                    .iter()
                    .map(|v| v.norm_sqr())
                    .sum::<f32>()
                    / (hi - lo) as f32;
                let rms = power.sqrt() / SIZE as f32;
                ((20. * (rms * GAINS[i]).max(1e-10).log10() + 78.) / 66.).clamp(0., 1.)
            };
            // Time-based response, independent of endpoint sample rate or packet size.
            let tau = if target > self.levels[i] { 0.035 } else { 0.14 };
            self.levels[i] +=
                (target - self.levels[i]) * (1. - (-seconds.clamp(0., 0.25) / tau).exp());
            if self.levels[i] < 0.001 {
                self.levels[i] = 0.;
            }
        }
        self.levels
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tones_land_in_correct_bands_at_supported_rates() {
        for rate in [44100, 48000, 96000] {
            for (expected, freq) in [120., 400., 1000., 3000., 7000., 14000.]
                .into_iter()
                .enumerate()
            {
                let mut a = Analyzer::new(rate);
                for i in 0..SIZE {
                    a.push(
                        0.03 * (2. * std::f32::consts::PI * freq * i as f32 / rate as f32).sin(),
                    );
                }
                let bars = a.analyze(0.25);
                let peak = bars
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.total_cmp(b.1))
                    .unwrap()
                    .0;
                assert_eq!(peak, expected, "rate {rate} freq {freq}: {bars:?}");
                assert!(bars.iter().all(|v| v.is_finite() && (0. ..=1.).contains(v)));
            }
        }
    }
    #[test]
    fn silence_invalid_samples_and_reset_leave_no_residue() {
        let mut a = Analyzer::new(48000);
        for _ in 0..SIZE {
            a.push(f32::NAN);
            a.push(f32::INFINITY);
        }
        assert_eq!(a.analyze(0.05), [0.; BANDS]);
        for i in 0..SIZE {
            a.push((i as f32).sin());
        }
        assert!(a.analyze(0.05).iter().any(|v| *v > 0.));
        a.clear();
        assert_eq!(a.analyze(0.05), [0.; BANDS]);
    }
}
