//! Bounded, deterministic modulation trajectories for the variable-delay reader.
//!
//! The oscillators describe instantaneous playback speed. Their analytic
//! antiderivatives produce bounded delay offsets, avoiding the accumulated
//! numerical drift that results from integrating an arbitrary speed signal.

use std::f64::consts::{SQRT_2, TAU};

/// Greatest absolute value of any normalized waveform antiderivative below.
/// The rounded-square waveform reaches exactly 7/6 at phase zero.
pub const MAX_SHAPE_PRIMITIVE: f64 = 7.0 / 6.0;
const WOW_DRIFT_TARGETS_PER_CYCLE: f64 = 0.5;
const FLUTTER_DRIFT_TARGETS_PER_CYCLE: f64 = 0.1;

#[derive(Clone, Copy, Debug)]
pub struct ModulationParams {
    pub wow_rate_hz: f64,
    pub wow_depth_cents: f64,
    /// -1 = rounded square, 0 = sine, +1 = triangle.
    pub wow_shape: f64,
    pub flutter_rate_hz: f64,
    pub flutter_depth_cents: f64,
    /// -1 = rounded square, 0 = sine, +1 = triangle.
    pub flutter_shape: f64,
    /// Independent, rate-derived random variation of both oscillators, 0..1.
    pub drift_amount: f64,
    /// Symmetric L/R modulation phase displacement, 0..1 = 0..180 degrees.
    pub stereo_amount: f64,
}

impl Default for ModulationParams {
    fn default() -> Self {
        Self {
            wow_rate_hz: 0.55,
            wow_depth_cents: 8.0,
            wow_shape: 0.0,
            flutter_rate_hz: 12.0,
            flutter_depth_cents: 0.0,
            flutter_shape: 0.0,
            drift_amount: 0.0,
            stereo_amount: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StereoDelayOffset {
    pub left: f64,
    pub right: f64,
}

/// Two independent speed LFOs with seeded, rate-derived drift sources.
/// Processing is allocation-free and deterministic after reset.
pub struct ModulationEngine {
    sample_rate: f64,
    wow_phase: f64,
    flutter_phase: f64,
    wow_drift: SmoothRandom,
    flutter_drift: SmoothRandom,
}

impl ModulationEngine {
    pub fn new(sample_rate: f64, seed: u64) -> Self {
        assert!(sample_rate.is_finite() && sample_rate > 0.0);
        Self {
            sample_rate,
            wow_phase: 0.0,
            flutter_phase: 0.0,
            wow_drift: SmoothRandom::new(seed),
            flutter_drift: SmoothRandom::new(seed ^ 0xD1B5_4A32_D192_ED03),
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        if sample_rate.is_finite() && sample_rate > 0.0 {
            self.sample_rate = sample_rate;
        }
    }

    pub fn set_seed(&mut self, seed: u64) {
        self.wow_drift.set_seed(seed);
        self.flutter_drift.set_seed(seed ^ 0xD1B5_4A32_D192_ED03);
    }

    pub fn reset(&mut self) {
        self.wow_phase = 0.0;
        self.flutter_phase = 0.0;
        self.wow_drift.reset();
        self.flutter_drift.reset();
    }

    /// Return left/right delay offsets in samples, then advance by one sample.
    pub fn next(&mut self, params: ModulationParams) -> StereoDelayOffset {
        let drift = params.drift_amount.clamp(0.0, 1.0);
        let wow_drift = self.wow_drift.next(
            (params.wow_rate_hz * WOW_DRIFT_TARGETS_PER_CYCLE).clamp(0.025, 2.0),
            self.sample_rate,
        );
        let flutter_drift = self.flutter_drift.next(
            (params.flutter_rate_hz * FLUTTER_DRIFT_TARGETS_PER_CYCLE).clamp(0.5, 3.0),
            self.sample_rate,
        );
        // Full Drift permits each oscillator to wander independently by half an
        // octave while remaining positive and bounded by sqrt(2).
        let wow_rate_multiplier = 2.0_f64.powf(0.5 * drift * wow_drift);
        let flutter_rate_multiplier = 2.0_f64.powf(0.5 * drift * flutter_drift);

        let side_phase = params.stereo_amount.clamp(0.0, 1.0) * std::f64::consts::FRAC_PI_2;
        let render = |wow_side: f64, flutter_side: f64| {
            oscillator_delay(
                self.wow_phase + wow_side,
                params.wow_rate_hz,
                params.wow_depth_cents,
                params.wow_shape,
                self.sample_rate,
            ) + oscillator_delay(
                self.flutter_phase + flutter_side,
                params.flutter_rate_hz,
                params.flutter_depth_cents,
                params.flutter_shape,
                self.sample_rate,
            )
        };
        let output = StereoDelayOffset {
            left: render(-side_phase, -side_phase),
            right: render(side_phase, side_phase),
        };

        self.wow_phase = advance_phase(
            self.wow_phase,
            params.wow_rate_hz * wow_rate_multiplier,
            self.sample_rate,
        );
        self.flutter_phase = advance_phase(
            self.flutter_phase,
            params.flutter_rate_hz * flutter_rate_multiplier,
            self.sample_rate,
        );
        output
    }
}

/// Conservative delay excursion for one oscillator across every shape.
pub fn maximum_delay_excursion_samples(
    max_depth_cents: f64,
    min_rate_hz: f64,
    sample_rate: f64,
) -> f64 {
    speed_delta(max_depth_cents) * sample_rate * MAX_SHAPE_PRIMITIVE / (TAU * min_rate_hz)
}

/// Conservative maximum positive playback-rate delta for the constant-power
/// LFO blend at the maximum Drift rate multiplier.
pub fn maximum_rate_delta(depths_cents: &[f64]) -> f64 {
    depths_cents
        .iter()
        .copied()
        .map(speed_delta)
        .map(|delta| delta * delta)
        .sum::<f64>()
        .sqrt()
        * SQRT_2
}

#[inline]
fn oscillator_delay(
    phase: f64,
    rate_hz: f64,
    depth_cents: f64,
    shape: f64,
    sample_rate: f64,
) -> f64 {
    let rate_hz = rate_hz.max(0.001);
    let amplitude = speed_delta(depth_cents.max(0.0)) * sample_rate / (TAU * rate_hz);
    amplitude * shape_primitive(phase, shape)
}

#[inline]
fn speed_delta(depth_cents: f64) -> f64 {
    2.0_f64.powf(depth_cents / 1200.0) - 1.0
}

#[inline]
fn advance_phase(phase: f64, rate_hz: f64, sample_rate: f64) -> f64 {
    (phase + TAU * rate_hz.max(0.0) / sample_rate).rem_euclid(TAU)
}

/// Analytic antiderivatives of low-order, explicitly bandlimited speed waves.
/// Differentiating the returned value by phase yields minus the requested
/// speed waveform. The morph is a convex blend, so it cannot exceed the
/// endpoint excursion bound.
#[inline]
pub fn shape_primitive(phase: f64, shape: f64) -> f64 {
    let sine = phase.cos();
    let shape = shape.clamp(-1.0, 1.0);
    if shape < 0.0 {
        // A rounded square speed wave using only the first and third
        // harmonics. Its delay trajectory is correspondingly triangle-like.
        let rounded_square = 1.125 * phase.cos() + (0.125 / 3.0) * (3.0 * phase).cos();
        sine + (-shape) * (rounded_square - sine)
    } else {
        // Three-term triangle speed series, normalized to unit peak.
        const TRIANGLE_NORMALIZATION: f64 = 259.0 / 225.0;
        let triangle = (phase.cos() - (3.0 * phase).cos() / 27.0 + (5.0 * phase).cos() / 125.0)
            / TRIANGLE_NORMALIZATION;
        sine + shape * (triangle - sine)
    }
}

struct SmoothRandom {
    initial_seed: u64,
    state: u64,
    current: f64,
    next: f64,
    position: f64,
    segment_rate_scale: f64,
}

impl SmoothRandom {
    fn new(seed: u64) -> Self {
        let mut result = Self {
            initial_seed: normalized_seed(seed),
            state: normalized_seed(seed),
            current: 0.0,
            next: 0.0,
            position: 0.0,
            segment_rate_scale: 1.0,
        };
        result.reset();
        result
    }

    fn set_seed(&mut self, seed: u64) {
        let seed = normalized_seed(seed);
        if seed != self.initial_seed {
            self.initial_seed = seed;
            self.reset();
        }
    }

    fn reset(&mut self) {
        self.state = self.initial_seed;
        self.current = self.random_bipolar();
        self.next = self.random_bipolar();
        self.segment_rate_scale = self.random_segment_rate_scale();
        self.position = 0.0;
    }

    fn next(&mut self, rate_hz: f64, sample_rate: f64) -> f64 {
        let t = self.position.clamp(0.0, 1.0);
        let quintic = t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
        let value = self.current + quintic * (self.next - self.current);
        self.position += rate_hz * self.segment_rate_scale / sample_rate;
        while self.position >= 1.0 {
            self.position -= 1.0;
            self.current = self.next;
            self.next = self.random_bipolar();
            self.segment_rate_scale = self.random_segment_rate_scale();
        }
        value
    }

    fn random_segment_rate_scale(&mut self) -> f64 {
        // Avoid turning the succession of random targets into another audible
        // periodic LFO while keeping the average target cadence predictable.
        1.0 + 0.35 * self.random_bipolar()
    }

    fn random_bipolar(&mut self) -> f64 {
        // xorshift64*: tiny, deterministic, and sufficient for modulation.
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        let value = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
        let unit = (value >> 11) as f64 * (1.0 / ((1_u64 << 53) as f64));
        unit * 2.0 - 1.0
    }
}

#[inline]
fn normalized_seed(seed: u64) -> u64 {
    if seed == 0 {
        0xA076_1D64_78BD_642F
    } else {
        seed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shape_is_periodic_and_bounded() {
        for shape in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            let start = shape_primitive(0.37, shape);
            let end = shape_primitive(0.37 + TAU, shape);
            assert!((start - end).abs() < 1.0e-12);
            for index in 0..4096 {
                let phase = TAU * index as f64 / 4096.0;
                assert!(shape_primitive(phase, shape).abs() <= MAX_SHAPE_PRIMITIVE + 1.0e-12);
            }
        }
    }

    #[test]
    fn sine_depth_maps_to_requested_speed_excursion() {
        let sample_rate = 48_000.0;
        let rate = 0.55;
        let depth = 24.0;
        let phase = 0.731;
        let step = TAU * rate / sample_rate;
        let d0 = oscillator_delay(phase, rate, depth, 0.0, sample_rate);
        let d1 = oscillator_delay(phase + step, rate, depth, 0.0, sample_rate);
        let measured = -(d1 - d0);
        let expected = speed_delta(depth) * phase.sin();
        assert!((measured - expected).abs() < 2.0e-6);
    }

    #[test]
    fn smooth_random_repeats_after_reset() {
        let mut random = SmoothRandom::new(42);
        let first: Vec<_> = (0..2000).map(|_| random.next(100.0, 1000.0)).collect();
        random.reset();
        let second: Vec<_> = (0..2000).map(|_| random.next(100.0, 1000.0)).collect();
        assert_eq!(first, second);
        assert!(first.iter().all(|value| (-1.0..=1.0).contains(value)));
    }

    #[test]
    fn stereo_is_linked_at_zero_strength() {
        let mut engine = ModulationEngine::new(48_000.0, 7);
        let params = ModulationParams {
            flutter_depth_cents: 12.0,
            drift_amount: 1.0,
            ..ModulationParams::default()
        };
        for _ in 0..10_000 {
            let offset = engine.next(params);
            assert_eq!(offset.left, offset.right);
        }
    }

    #[test]
    fn stereo_phase_reaches_audible_channel_separation() {
        let mut engine = ModulationEngine::new(48_000.0, 7);
        let params = ModulationParams {
            wow_depth_cents: 20.0,
            stereo_amount: 1.0,
            ..ModulationParams::default()
        };
        let maximum_difference = (0..48_000)
            .map(|_| {
                let offset = engine.next(params);
                (offset.left - offset.right).abs()
            })
            .fold(0.0_f64, f64::max);
        assert!(maximum_difference > 100.0);
    }

    #[test]
    fn drift_is_seeded_and_repeatable() {
        let params = ModulationParams {
            wow_depth_cents: 20.0,
            flutter_depth_cents: 8.0,
            drift_amount: 1.0,
            ..ModulationParams::default()
        };
        let mut first = ModulationEngine::new(48_000.0, 42);
        let mut second = ModulationEngine::new(48_000.0, 42);
        for _ in 0..48_000 {
            let a = first.next(params);
            let b = second.next(params);
            assert_eq!(a.left, b.left);
            assert_eq!(a.right, b.right);
        }
    }

    #[test]
    fn drift_sources_are_independent() {
        let mut wow = SmoothRandom::new(19);
        let mut flutter = SmoothRandom::new(19 ^ 0xD1B5_4A32_D192_ED03);
        assert!((0..1000).any(|_| wow.next(1.0, 1000.0) != flutter.next(1.0, 1000.0)));
    }

    #[test]
    fn combined_rate_bound_covers_each_endpoint() {
        let combined = maximum_rate_delta(&[60.0, 20.0]);
        assert!(combined >= speed_delta(60.0) * SQRT_2);
        assert!(combined >= speed_delta(20.0) * SQRT_2);
    }
}
