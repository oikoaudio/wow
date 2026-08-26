//! Host-independent fractional-delay primitives for clean wow/flutter.
//!
//! Reference primitives use `f64` so algorithmic error can be distinguished
//! from the `f32` rounding error of the plug-in audio path.

pub mod modulation;

use std::f64::consts::PI;
use std::sync::Arc;

/// A random-access interpolation kernel. `position` is in absolute samples.
pub trait Kernel {
    fn sample<S: SampleSource>(&self, source: &S, position: f64) -> f64;
    fn name(&self) -> &'static str;
}

/// Integer sample access used by both slices and circular buffers.
pub trait SampleSource {
    fn at(&self, index: i64) -> f64;
}

/// `f32` audio-path counterpart used by plugin hosts. Read positions remain
/// `f64` so long-running modulation does not lose sub-sample phase precision.
pub trait KernelF32 {
    fn sample_f32<S: SampleSourceF32>(&self, source: &S, position: f64) -> f32;
}

pub trait SampleSourceF32 {
    fn at_f32(&self, index: i64) -> f32;
}

/// Four-point, third-order Lagrange interpolation in Farrow-polynomial form.
#[derive(Clone, Copy, Default)]
pub struct CubicFarrow;

impl Kernel for CubicFarrow {
    fn sample<S: SampleSource>(&self, source: &S, position: f64) -> f64 {
        let i = position.floor() as i64;
        let mu = position - i as f64;
        let xm1 = source.at(i - 1);
        let x0 = source.at(i);
        let x1 = source.at(i + 1);
        let x2 = source.at(i + 2);

        // Lagrange basis for nodes -1, 0, 1, 2.
        let c0 = -mu * (mu - 1.0) * (mu - 2.0) / 6.0;
        let c1 = (mu + 1.0) * (mu - 1.0) * (mu - 2.0) / 2.0;
        let c2 = -(mu + 1.0) * mu * (mu - 2.0) / 2.0;
        let c3 = (mu + 1.0) * mu * (mu - 1.0) / 6.0;
        xm1 * c0 + x0 * c1 + x1 * c2 + x2 * c3
    }

    fn name(&self) -> &'static str {
        "cubic-farrow"
    }
}

/// Even-tap Lagrange fractional-delay filter, centered around the read point.
pub struct Lagrange<const TAPS: usize>;

impl<const TAPS: usize> Default for Lagrange<TAPS> {
    fn default() -> Self {
        Self
    }
}

impl<const TAPS: usize> Kernel for Lagrange<TAPS> {
    fn sample<S: SampleSource>(&self, source: &S, position: f64) -> f64 {
        assert!(TAPS >= 2 && TAPS.is_multiple_of(2));
        let base = position.floor() as i64 - (TAPS as i64 / 2 - 1);
        let local = position - base as f64;
        let mut sum = 0.0;
        for k in 0..TAPS {
            let mut weight = 1.0;
            for j in 0..TAPS {
                if j != k {
                    weight *= (local - j as f64) / (k as f64 - j as f64);
                }
            }
            sum += source.at(base + k as i64) * weight;
        }
        sum
    }

    fn name(&self) -> &'static str {
        "lagrange"
    }
}

/// Eight-point, seventh-degree Lagrange interpolation in fixed Farrow form.
///
/// This evaluates the same polynomial as `Lagrange<8>`, but all basis
/// coefficients are compile-time constants and the per-sample divisions and
/// nested basis-product loop are eliminated.
#[derive(Clone, Copy, Default)]
pub struct Farrow8;

impl Kernel for Farrow8 {
    #[inline]
    fn sample<S: SampleSource>(&self, source: &S, position: f64) -> f64 {
        const C: [[f64; 8]; 8] = [
            [
                0.0,
                -1.0 / 105.0,
                1.0 / 180.0,
                1.0 / 90.0,
                -1.0 / 144.0,
                -1.0 / 720.0,
                1.0 / 720.0,
                -1.0 / 5040.0,
            ],
            [
                0.0,
                1.0 / 10.0,
                -3.0 / 40.0,
                -71.0 / 720.0,
                1.0 / 12.0,
                -1.0 / 360.0,
                -1.0 / 120.0,
                1.0 / 720.0,
            ],
            [
                0.0,
                -3.0 / 5.0,
                3.0 / 4.0,
                1.0 / 15.0,
                -13.0 / 48.0,
                3.0 / 80.0,
                1.0 / 48.0,
                -1.0 / 240.0,
            ],
            [
                1.0,
                -1.0 / 4.0,
                -49.0 / 36.0,
                49.0 / 144.0,
                7.0 / 18.0,
                -7.0 / 72.0,
                -1.0 / 36.0,
                1.0 / 144.0,
            ],
            [
                0.0,
                1.0,
                3.0 / 4.0,
                -11.0 / 18.0,
                -13.0 / 48.0,
                17.0 / 144.0,
                1.0 / 48.0,
                -1.0 / 144.0,
            ],
            [
                0.0,
                -3.0 / 10.0,
                -3.0 / 40.0,
                89.0 / 240.0,
                1.0 / 12.0,
                -3.0 / 40.0,
                -1.0 / 120.0,
                1.0 / 240.0,
            ],
            [
                0.0,
                1.0 / 15.0,
                1.0 / 180.0,
                -4.0 / 45.0,
                -1.0 / 144.0,
                17.0 / 720.0,
                1.0 / 720.0,
                -1.0 / 720.0,
            ],
            [
                0.0,
                -1.0 / 140.0,
                0.0,
                7.0 / 720.0,
                0.0,
                -1.0 / 360.0,
                0.0,
                1.0 / 5040.0,
            ],
        ];

        let integer = position.floor() as i64;
        let mu = position - integer as f64;
        let x = [
            source.at(integer - 3),
            source.at(integer - 2),
            source.at(integer - 1),
            source.at(integer),
            source.at(integer + 1),
            source.at(integer + 2),
            source.at(integer + 3),
            source.at(integer + 4),
        ];

        macro_rules! dot {
            ($degree:expr) => {
                x[0] * C[0][$degree]
                    + x[1] * C[1][$degree]
                    + x[2] * C[2][$degree]
                    + x[3] * C[3][$degree]
                    + x[4] * C[4][$degree]
                    + x[5] * C[5][$degree]
                    + x[6] * C[6][$degree]
                    + x[7] * C[7][$degree]
            };
        }

        let p0 = x[3];
        let p1 = dot!(1);
        let p2 = dot!(2);
        let p3 = dot!(3);
        let p4 = dot!(4);
        let p5 = dot!(5);
        let p6 = dot!(6);
        let p7 = dot!(7);
        let value = p7.mul_add(mu, p6);
        let value = value.mul_add(mu, p5);
        let value = value.mul_add(mu, p4);
        let value = value.mul_add(mu, p3);
        let value = value.mul_add(mu, p2);
        let value = value.mul_add(mu, p1);
        value.mul_add(mu, p0)
    }

    fn name(&self) -> &'static str {
        "farrow-8"
    }
}

/// Dense phase-table, Kaiser-windowed sinc interpolator.
///
/// Construction is non-real-time. Processing performs no allocation.
pub struct WindowedSinc {
    taps: usize,
    phases: usize,
    cutoff: f64,
    table: Vec<f64>,
}

impl WindowedSinc {
    pub fn new(taps: usize, phases: usize, cutoff: f64, beta: f64) -> Self {
        assert!(taps >= 4 && taps.is_multiple_of(2));
        assert!(phases >= 2);
        assert!(cutoff > 0.0 && cutoff <= 1.0);
        let mut table = vec![0.0; (phases + 1) * taps];
        let denom = bessel_i0(beta);
        let center = taps as f64 / 2.0 - 1.0;
        for phase in 0..=phases {
            let frac = phase as f64 / phases as f64;
            let row = &mut table[phase * taps..(phase + 1) * taps];
            let mut gain = 0.0;
            for (k, coefficient) in row.iter_mut().enumerate() {
                let distance = k as f64 - center - frac;
                let normalized = (2.0 * k as f64 / (taps - 1) as f64) - 1.0;
                let window =
                    bessel_i0(beta * (1.0 - normalized * normalized).max(0.0).sqrt()) / denom;
                *coefficient = cutoff * sinc(cutoff * distance) * window;
                gain += *coefficient;
            }
            for coefficient in row {
                *coefficient /= gain;
            }
        }
        Self {
            taps,
            phases,
            cutoff,
            table,
        }
    }

    /// Builds a conservative anti-aliasing kernel for a known maximum playback
    /// rate. `passband` is the desired fraction of the safe Nyquist limit.
    ///
    /// When reading faster than 1x, an input component at normalized frequency
    /// `f` appears at `f * rate`. Limiting the reconstruction cutoff to
    /// `passband / max_rate` keeps that component below the output Nyquist limit.
    pub fn for_max_rate(
        taps: usize,
        phases: usize,
        max_rate: f64,
        passband: f64,
        beta: f64,
    ) -> Self {
        assert!(max_rate.is_finite() && max_rate >= 1.0);
        assert!(passband > 0.0 && passband <= 1.0);
        Self::new(taps, phases, passband / max_rate, beta)
    }

    pub fn taps(&self) -> usize {
        self.taps
    }
    pub fn phases(&self) -> usize {
        self.phases
    }
    pub fn cutoff(&self) -> f64 {
        self.cutoff
    }
}

/// Streaming integer-factor oversampling around a variable-delay kernel.
///
/// A matched pair of causal Kaiser-windowed FIR filters reconstructs the input
/// at the higher rate and removes time-warped content above the host Nyquist
/// limit before decimation. Delay values are linearly interpolated between host
/// samples. Construction allocates; `process_sample()` does not.
pub struct OversampledVariableDelay<K, const FACTOR: usize> {
    upsampler: PolyphaseUpsampler<FACTOR>,
    downsampler: DecimatingFir,
    reader: VariableDelay<K>,
    previous_delay: f64,
    initialized: bool,
}

impl<K: Kernel, const FACTOR: usize> OversampledVariableDelay<K, FACTOR> {
    pub fn new(
        max_delay_samples: usize,
        kernel_margin_high_rate: usize,
        filter_taps: usize,
        kernel: K,
    ) -> Self {
        assert!(FACTOR == 2 || FACTOR == 4);
        assert!(filter_taps >= 17 && filter_taps % 2 == 1);
        // Reserve ten percent of the host band for a realizable transition.
        let cutoff = 0.9 / FACTOR as f64;
        let coefficients = design_lowpass(filter_taps, cutoff, 10.0);
        Self {
            upsampler: PolyphaseUpsampler::new(&coefficients),
            downsampler: DecimatingFir::new(coefficients),
            reader: VariableDelay::new(max_delay_samples * FACTOR, kernel_margin_high_rate, kernel),
            previous_delay: 0.0,
            initialized: false,
        }
    }

    pub fn process_sample(&mut self, input: f64, delay_samples: f64) -> f64 {
        if !self.initialized {
            self.previous_delay = delay_samples;
            self.initialized = true;
        }

        self.upsampler.push(input);
        for phase in 0..FACTOR {
            let high_rate_input = self.upsampler.output(phase) * FACTOR as f64;
            let fraction = (phase + 1) as f64 / FACTOR as f64;
            let delay = self.previous_delay + fraction * (delay_samples - self.previous_delay);
            let warped = self
                .reader
                .process_sample(high_rate_input, delay * FACTOR as f64);
            self.downsampler.push(warped);
        }
        self.previous_delay = delay_samples;
        self.downsampler.output()
    }

    /// Nominal linear-phase filter latency in host-rate samples.
    pub fn latency_samples(&self) -> f64 {
        (self.downsampler.taps - 1) as f64 / FACTOR as f64
    }

    pub fn reset(&mut self) {
        self.upsampler.reset();
        self.downsampler.reset();
        self.reader.reset();
        self.previous_delay = 0.0;
        self.initialized = false;
    }
}

/// Four-times oversampling built as two sparse 2× halfband stages.
///
/// This is the production-oriented comparison to the direct 4× prototype. An
/// ideal halfband filter has alternating zero coefficients, so the sparse FIR
/// representation avoids nearly half of its multiplies.
pub struct CascadedOversampledVariableDelay4x<K> {
    up1: PolyphaseUpsampler<2>,
    up2: PolyphaseUpsampler<2>,
    down2: DecimatingFir,
    down1: DecimatingFir,
    reader: VariableDelay<K>,
    previous_delay: f64,
    initialized: bool,
}

impl<K: Kernel> CascadedOversampledVariableDelay4x<K> {
    pub fn new(max_delay_samples: usize, kernel_margin_high_rate: usize, kernel: K) -> Self {
        let halfband = design_lowpass(65, 0.5, 10.0);
        // The final 2× -> host-rate stage reserves the top ten percent of the
        // host band. A textbook halfband transition straddles Nyquist and does
        // not strongly reject components only just above it.
        let final_lowpass = design_lowpass(129, 0.45, 10.0);
        Self {
            up1: PolyphaseUpsampler::new(&halfband),
            up2: PolyphaseUpsampler::new(&halfband),
            down2: DecimatingFir::new(halfband.clone()),
            down1: DecimatingFir::new(final_lowpass),
            reader: VariableDelay::new(max_delay_samples * 4, kernel_margin_high_rate, kernel),
            previous_delay: 0.0,
            initialized: false,
        }
    }

    pub fn process_sample(&mut self, input: f64, delay_samples: f64) -> f64 {
        if !self.initialized {
            self.previous_delay = delay_samples;
            self.initialized = true;
        }

        self.up1.push(input);
        for phase1 in 0..2 {
            let sample_2x = self.up1.output(phase1) * 2.0;
            self.up2.push(sample_2x);
            for phase2 in 0..2 {
                let phase4 = phase1 * 2 + phase2;
                let sample_4x = self.up2.output(phase2) * 2.0;
                let fraction = (phase4 + 1) as f64 / 4.0;
                let delay = self.previous_delay + fraction * (delay_samples - self.previous_delay);
                let warped = self.reader.process_sample(sample_4x, delay * 4.0);
                self.down2.push(warped);
            }
            self.down1.push(self.down2.output());
        }
        self.previous_delay = delay_samples;
        self.down1.output()
    }

    pub fn latency_samples(&self) -> f64 {
        // 16 (up1) + 8 (up2) + 8 (down2) + 32 (final low-pass).
        64.0
    }

    pub fn reset(&mut self) {
        self.up1.reset();
        self.up2.reset();
        self.down2.reset();
        self.down1.reset();
        self.reader.reset();
        self.previous_delay = 0.0;
        self.initialized = false;
    }
}

struct PolyphaseUpsampler<const FACTOR: usize> {
    coefficients: Vec<Vec<(usize, f64)>>,
    history: Vec<f64>,
    head: usize,
}

impl<const FACTOR: usize> PolyphaseUpsampler<FACTOR> {
    fn new(prototype: &[f64]) -> Self {
        let mut coefficients = vec![Vec::new(); FACTOR];
        for (index, &coefficient) in prototype.iter().enumerate() {
            if coefficient.abs() > 1.0e-15 {
                coefficients[index % FACTOR].push((index / FACTOR, coefficient));
            }
        }
        let history_len = prototype.len().div_ceil(FACTOR);
        Self {
            coefficients,
            history: vec![0.0; history_len],
            head: 0,
        }
    }

    #[inline]
    fn push(&mut self, input: f64) {
        self.head += 1;
        if self.head == self.history.len() {
            self.head = 0;
        }
        self.history[self.head] = input;
    }

    #[inline]
    fn output(&self, phase: usize) -> f64 {
        let mut output = 0.0;
        for &(lag, coefficient) in &self.coefficients[phase] {
            let index = (self.head + self.history.len() - lag) % self.history.len();
            output += coefficient * self.history[index];
        }
        output
    }

    fn reset(&mut self) {
        self.history.fill(0.0);
        self.head = 0;
    }
}

struct DecimatingFir {
    coefficients: Vec<(usize, f64)>,
    taps: usize,
    history: Vec<f64>,
    head: usize,
}

impl DecimatingFir {
    fn new(coefficients: Vec<f64>) -> Self {
        let taps = coefficients.len();
        Self {
            history: vec![0.0; taps],
            coefficients: coefficients
                .into_iter()
                .enumerate()
                .filter(|(_, coefficient)| coefficient.abs() > 1.0e-15)
                .collect(),
            taps,
            head: 0,
        }
    }

    #[inline]
    fn push(&mut self, input: f64) {
        self.head += 1;
        if self.head == self.history.len() {
            self.head = 0;
        }
        self.history[self.head] = input;
    }

    #[inline]
    fn output(&self) -> f64 {
        let mut output = 0.0;
        for &(lag, coefficient) in &self.coefficients {
            let index = (self.head + self.history.len() - lag) % self.history.len();
            output += coefficient * self.history[index];
        }
        output
    }

    fn reset(&mut self) {
        self.history.fill(0.0);
        self.head = 0;
    }
}

fn design_lowpass(taps: usize, cutoff: f64, beta: f64) -> Vec<f64> {
    assert!(taps % 2 == 1);
    assert!(cutoff > 0.0 && cutoff < 1.0);
    let center = (taps - 1) as f64 / 2.0;
    let denom = bessel_i0(beta);
    let mut coefficients = Vec::with_capacity(taps);
    for k in 0..taps {
        let distance = k as f64 - center;
        let normalized = distance / center;
        let window = bessel_i0(beta * (1.0 - normalized * normalized).max(0.0).sqrt()) / denom;
        coefficients.push(cutoff * sinc(cutoff * distance) * window);
    }
    let gain: f64 = coefficients.iter().sum();
    for coefficient in &mut coefficients {
        *coefficient /= gain;
    }
    coefficients
}

impl Kernel for WindowedSinc {
    fn sample<S: SampleSource>(&self, source: &S, position: f64) -> f64 {
        let integer = position.floor() as i64;
        let phase_position = (position - integer as f64) * self.phases as f64;
        let phase0 = (phase_position.floor() as usize).min(self.phases - 1);
        let blend = phase_position - phase0 as f64;
        let start = integer - (self.taps as i64 / 2 - 1);
        let row0 = &self.table[phase0 * self.taps..(phase0 + 1) * self.taps];
        let row1 = &self.table[(phase0 + 1) * self.taps..(phase0 + 2) * self.taps];
        let mut sum = 0.0;
        for k in 0..self.taps {
            let coefficient = row0[k] + blend * (row1[k] - row0[k]);
            sum += source.at(start + k as i64) * coefficient;
        }
        sum
    }

    fn name(&self) -> &'static str {
        "windowed-sinc"
    }
}

/// Quantized version of the windowed-sinc table for the host's `f32` audio
/// path. Table construction remains off the audio thread and uses `f64` before
/// coefficients are rounded once to `f32`.
pub struct WindowedSincF32 {
    taps: usize,
    phases: usize,
    table: Vec<f32>,
}

impl WindowedSincF32 {
    pub fn new(taps: usize, phases: usize, cutoff: f64, beta: f64) -> Self {
        let reference = WindowedSinc::new(taps, phases, cutoff, beta);
        Self {
            taps,
            phases,
            table: reference
                .table
                .into_iter()
                .map(|value| value as f32)
                .collect(),
        }
    }

    pub fn for_max_rate(
        taps: usize,
        phases: usize,
        max_rate: f64,
        passband: f64,
        beta: f64,
    ) -> Self {
        assert!(max_rate.is_finite() && max_rate >= 1.0);
        assert!(passband > 0.0 && passband <= 1.0);
        Self::new(taps, phases, passband / max_rate, beta)
    }
}

impl KernelF32 for WindowedSincF32 {
    #[inline]
    fn sample_f32<S: SampleSourceF32>(&self, source: &S, position: f64) -> f32 {
        let integer = position.floor() as i64;
        let phase_position = (position - integer as f64) * self.phases as f64;
        let phase0 = (phase_position.floor() as usize).min(self.phases - 1);
        let blend = (phase_position - phase0 as f64) as f32;
        let start = integer - (self.taps as i64 / 2 - 1);
        let row0 = &self.table[phase0 * self.taps..(phase0 + 1) * self.taps];
        let row1 = &self.table[(phase0 + 1) * self.taps..(phase0 + 2) * self.taps];
        let mut sum = 0.0_f32;
        for k in 0..self.taps {
            let coefficient = row0[k] + blend * (row1[k] - row0[k]);
            sum = source.at_f32(start + k as i64).mul_add(coefficient, sum);
        }
        sum
    }
}

/// Runtime quality choices shared by the DSP core and plugin wrapper.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum QualityMode {
    Draft,
    Normal,
    #[default]
    Hq,
    Ultra,
}

/// Prebuilt interpolation tables for allocation-free quality switching.
pub struct SincBankF32 {
    sets: Vec<RateSincSetF32>,
    max_rate: f64,
}

struct RateSincSetF32 {
    rate: f64,
    normal: WindowedSincF32,
    hq: WindowedSincF32,
    ultra: WindowedSincF32,
}

impl SincBankF32 {
    /// `max_rate` is the greatest instantaneous playback speed the modulation
    /// layer can request. The tables reserve the unsafe input band once during
    /// activation rather than redesigning filters on the audio thread.
    pub fn for_max_rate(max_rate: f64) -> Self {
        assert!(max_rate.is_finite() && max_rate >= 1.0);
        const RATE_STEP: f64 = 0.02;
        let steps = ((max_rate - 1.0) / RATE_STEP).ceil().max(1.0) as usize;
        let sets = (1..=steps)
            .map(|index| {
                let rate = 1.0 + (max_rate - 1.0) * index as f64 / steps as f64;
                RateSincSetF32 {
                    rate,
                    normal: WindowedSincF32::for_max_rate(80, 2048, rate, 0.995, 12.0),
                    hq: WindowedSincF32::for_max_rate(96, 4096, rate, 0.995, 14.0),
                    ultra: WindowedSincF32::for_max_rate(128, 4096, rate, 0.995, 16.0),
                }
            })
            .collect();
        Self { sets, max_rate }
    }

    #[inline]
    fn max_rate(&self) -> f64 {
        self.max_rate
    }

    #[inline]
    fn sample_set<S: SampleSourceF32>(
        set: &RateSincSetF32,
        source: &S,
        position: f64,
        quality: QualityMode,
    ) -> f32 {
        match quality {
            QualityMode::Draft => cubic_sample_f32(source, position),
            QualityMode::Normal => set.normal.sample_f32(source, position),
            QualityMode::Hq => set.hq.sample_f32(source, position),
            QualityMode::Ultra => set.ultra.sample_f32(source, position),
        }
    }

    #[inline]
    fn sample<S: SampleSourceF32>(
        &self,
        source: &S,
        position: f64,
        quality: QualityMode,
        requested_rate: f64,
    ) -> f32 {
        if quality == QualityMode::Draft {
            return cubic_sample_f32(source, position);
        }
        let rate = requested_rate.clamp(1.0, self.max_rate);
        let safe = self
            .sets
            .partition_point(|set| set.rate < rate)
            .min(self.sets.len() - 1);
        let tighter = (safe + 1).min(self.sets.len() - 1);
        let lower_rate = if safe == 0 {
            1.0
        } else {
            self.sets[safe - 1].rate
        };
        let safe_set = &self.sets[safe];
        let tighter_set = &self.sets[tighter];
        if (safe_set.rate - lower_rate).abs() <= f64::EPSILON {
            return Self::sample_set(safe_set, source, position, quality);
        }
        let blend = ((rate - lower_rate) / (safe_set.rate - lower_rate)) as f32;
        if blend <= f32::EPSILON {
            return Self::sample_set(safe_set, source, position, quality);
        }
        if blend >= 1.0 - f32::EPSILON {
            return Self::sample_set(tighter_set, source, position, quality);
        }
        let a = Self::sample_set(safe_set, source, position, quality);
        let b = Self::sample_set(tighter_set, source, position, quality);
        a + (b - a) * blend.clamp(0.0, 1.0)
    }
}

#[inline]
fn cubic_sample_f32<S: SampleSourceF32>(source: &S, position: f64) -> f32 {
    let i = position.floor() as i64;
    let mu = (position - i as f64) as f32;
    let xm1 = source.at_f32(i - 1);
    let x0 = source.at_f32(i);
    let x1 = source.at_f32(i + 1);
    let x2 = source.at_f32(i + 2);
    let c0 = -mu * (mu - 1.0) * (mu - 2.0) / 6.0;
    let c1 = (mu + 1.0) * (mu - 1.0) * (mu - 2.0) / 2.0;
    let c2 = -(mu + 1.0) * mu * (mu - 2.0) / 2.0;
    let c3 = (mu + 1.0) * mu * (mu - 1.0) / 6.0;
    xm1 * c0 + x0 * c1 + x1 * c2 + x2 * c3
}

/// Circular-buffer variable-delay reader. Allocate only during construction.
pub struct VariableDelay<K> {
    buffer: Vec<f64>,
    write_index: i64,
    kernel: K,
}

/// Allocation-free `f32` audio buffer with a high-precision read position.
pub struct VariableDelayF32<K> {
    buffer: Vec<f32>,
    write_index: i64,
    kernel: K,
}

impl<K: KernelF32> VariableDelayF32<K> {
    pub fn new(max_delay_samples: usize, kernel_margin: usize, kernel: K) -> Self {
        let requested = max_delay_samples + 2 * kernel_margin + 8;
        let capacity = requested.next_power_of_two();
        Self {
            buffer: vec![0.0; capacity],
            write_index: 0,
            kernel,
        }
    }

    #[inline]
    pub fn process_sample(&mut self, input: f32, delay_samples: f64) -> f32 {
        if !input.is_finite() || !delay_samples.is_finite() || delay_samples < 0.0 {
            return 0.0;
        }
        let slot = self.write_index as usize & (self.buffer.len() - 1);
        self.buffer[slot] = input;
        let position = self.write_index as f64 - delay_samples;
        let output = self.kernel.sample_f32(
            &RingViewF32 {
                buffer: &self.buffer,
                newest: self.write_index,
            },
            position,
        );
        self.write_index += 1;
        output
    }

    pub fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_index = 0;
    }
}

/// Shared-table variable delay with click-free, allocation-free quality
/// changes. Both kernels read the same history during a crossfade, so the new
/// mode never starts with an empty delay buffer.
pub struct QualityVariableDelayF32 {
    buffer: Vec<f32>,
    write_index: i64,
    kernels: Arc<SincBankF32>,
    current_quality: QualityMode,
    target_quality: QualityMode,
    crossfade_position: u32,
    crossfade_samples: u32,
}

impl QualityVariableDelayF32 {
    pub fn new(
        max_delay_samples: usize,
        kernel_margin: usize,
        kernels: Arc<SincBankF32>,
        quality: QualityMode,
        crossfade_samples: u32,
    ) -> Self {
        let requested = max_delay_samples + 2 * kernel_margin + 8;
        Self {
            buffer: vec![0.0; requested.next_power_of_two()],
            write_index: 0,
            kernels,
            current_quality: quality,
            target_quality: quality,
            crossfade_position: crossfade_samples,
            crossfade_samples,
        }
    }

    #[inline]
    pub fn process_sample(
        &mut self,
        input: f32,
        delay_samples: f64,
        requested_quality: QualityMode,
    ) -> f32 {
        self.process_sample_rate_aware(
            input,
            delay_samples,
            requested_quality,
            self.kernels.max_rate(),
        )
    }

    #[inline]
    pub fn process_sample_rate_aware(
        &mut self,
        input: f32,
        delay_samples: f64,
        requested_quality: QualityMode,
        playback_rate: f64,
    ) -> f32 {
        if !input.is_finite() || !delay_samples.is_finite() || delay_samples < 0.0 {
            return 0.0;
        }
        if requested_quality != self.target_quality {
            self.target_quality = requested_quality;
            self.crossfade_position = 0;
        }
        let slot = self.write_index as usize & (self.buffer.len() - 1);
        self.buffer[slot] = input;
        let position = self.write_index as f64 - delay_samples;
        let source = RingViewF32 {
            buffer: &self.buffer,
            newest: self.write_index,
        };
        let output = if self.current_quality == self.target_quality || self.crossfade_samples == 0 {
            self.current_quality = self.target_quality;
            self.kernels
                .sample(&source, position, self.current_quality, playback_rate)
        } else {
            let from = self
                .kernels
                .sample(&source, position, self.current_quality, playback_rate);
            let to = self
                .kernels
                .sample(&source, position, self.target_quality, playback_rate);
            let mix = (self.crossfade_position + 1) as f32 / self.crossfade_samples as f32;
            self.crossfade_position += 1;
            if self.crossfade_position >= self.crossfade_samples {
                self.current_quality = self.target_quality;
            }
            from + (to - from) * mix.min(1.0)
        };
        self.write_index += 1;
        output
    }

    pub fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_index = 0;
        self.current_quality = self.target_quality;
        self.crossfade_position = self.crossfade_samples;
    }
}

struct RingViewF32<'a> {
    buffer: &'a [f32],
    newest: i64,
}

impl SampleSourceF32 for RingViewF32<'_> {
    #[inline]
    fn at_f32(&self, index: i64) -> f32 {
        if index < 0 || index > self.newest || self.newest - index >= self.buffer.len() as i64 {
            0.0
        } else {
            self.buffer[index as usize & (self.buffer.len() - 1)]
        }
    }
}

impl<K: Kernel> VariableDelay<K> {
    pub fn new(max_delay_samples: usize, kernel_margin: usize, kernel: K) -> Self {
        let requested = max_delay_samples + 2 * kernel_margin + 8;
        let capacity = requested.next_power_of_two();
        Self {
            buffer: vec![0.0; capacity],
            write_index: 0,
            kernel,
        }
    }

    /// Writes one input and reads `delay_samples` behind it.
    ///
    /// The caller must ensure the delay is positive enough for the kernel's
    /// look-ahead and changes smoothly. Returns silence for unwritten history.
    pub fn process_sample(&mut self, input: f64, delay_samples: f64) -> f64 {
        debug_assert!(delay_samples.is_finite() && delay_samples >= 0.0);
        if !input.is_finite() || !delay_samples.is_finite() || delay_samples < 0.0 {
            return 0.0;
        }
        let slot = self.write_index as usize & (self.buffer.len() - 1);
        self.buffer[slot] = input;
        let position = self.write_index as f64 - delay_samples;
        let output = self.kernel.sample(
            &RingView {
                buffer: &self.buffer,
                newest: self.write_index,
            },
            position,
        );
        self.write_index += 1;
        output
    }

    pub fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_index = 0;
    }
}

struct RingView<'a> {
    buffer: &'a [f64],
    newest: i64,
}

impl SampleSource for RingView<'_> {
    fn at(&self, index: i64) -> f64 {
        if index < 0 || index > self.newest || self.newest - index >= self.buffer.len() as i64 {
            0.0
        } else {
            self.buffer[index as usize & (self.buffer.len() - 1)]
        }
    }
}

pub struct SliceSource<'a>(pub &'a [f64]);

impl SampleSource for SliceSource<'_> {
    fn at(&self, index: i64) -> f64 {
        if index < 0 {
            0.0
        } else {
            self.0.get(index as usize).copied().unwrap_or(0.0)
        }
    }
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1.0e-12 {
        1.0
    } else {
        (PI * x).sin() / (PI * x)
    }
}

fn bessel_i0(x: f64) -> f64 {
    let y = x * x / 4.0;
    let mut sum = 1.0;
    let mut term = 1.0;
    for k in 1..=24 {
        term *= y / (k as f64 * k as f64);
        sum += term;
        if term < sum * 1.0e-16 {
            break;
        }
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolation_is_exact_at_integer_positions() {
        let data = [0.0, 1.0, -2.0, 3.5, 0.25, -1.0, 2.0, 0.0];
        let source = SliceSource(&data);
        assert!((CubicFarrow.sample(&source, 3.0) - 3.5).abs() < 1e-14);
        assert!((Lagrange::<8>.sample(&source, 3.0) - 3.5).abs() < 1e-12);
        assert!((Farrow8.sample(&source, 3.0) - 3.5).abs() < 1e-12);
    }

    #[test]
    fn fixed_farrow_matches_generic_lagrange() {
        let data: Vec<_> = (0..128)
            .map(|n| ((n as f64 * 0.731).sin() + (n as f64 * 0.173).cos()) * 0.5)
            .collect();
        let source = SliceSource(&data);
        for &position in &[16.01, 23.125, 47.5, 79.9, 110.999] {
            let expected = Lagrange::<8>.sample(&source, position);
            let actual = Farrow8.sample(&source, position);
            assert!(
                (actual - expected).abs() < 2.0e-13,
                "position={position} actual={actual} expected={expected} error={}",
                (actual - expected).abs()
            );
        }
    }

    #[test]
    fn sinc_table_has_unity_dc_gain() {
        let kernel = WindowedSinc::new(64, 1024, 0.94, 10.0);
        let ones = vec![1.0; 256];
        let source = SliceSource(&ones);
        for &fraction in &[0.01, 0.25, 0.5, 0.9, 0.99] {
            assert!((kernel.sample(&source, 128.0 + fraction) - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn ring_wrap_preserves_constant_signal() {
        let mut delay = VariableDelay::new(128, 40, WindowedSinc::new(32, 128, 0.94, 9.0));
        let mut last = 0.0;
        for _ in 0..2000 {
            last = delay.process_sample(1.0, 64.25);
        }
        assert!((last - 1.0).abs() < 1e-10);
    }

    #[test]
    fn rate_aware_sinc_reserves_the_safe_band() {
        let kernel = WindowedSinc::for_max_rate(64, 256, 1.25, 0.95, 10.0);
        assert!((kernel.cutoff() - 0.76).abs() < 1e-12);
    }

    #[test]
    fn oversampled_reader_preserves_dc_after_settling() {
        let mut reader = OversampledVariableDelay::<_, 2>::new(256, 8, 129, CubicFarrow);
        let mut output = 0.0;
        for _ in 0..4000 {
            output = reader.process_sample(1.0, 128.25);
        }
        assert!((output - 1.0).abs() < 1e-10);
        assert_eq!(reader.latency_samples(), 64.0);
    }

    #[test]
    fn cascaded_halfband_reader_preserves_dc_after_settling() {
        let mut reader = CascadedOversampledVariableDelay4x::new(256, 8, CubicFarrow);
        let mut output = 0.0;
        for _ in 0..4000 {
            output = reader.process_sample(1.0, 128.25);
        }
        assert!((output - 1.0).abs() < 1e-10);
        assert_eq!(reader.latency_samples(), 64.0);
    }

    #[test]
    fn f32_sinc_reader_preserves_dc_after_settling() {
        let mut reader = VariableDelayF32::new(256, 52, WindowedSincF32::new(96, 4096, 0.98, 14.0));
        let mut output = 0.0;
        for _ in 0..4000 {
            output = reader.process_sample(1.0, 128.25);
        }
        assert!((output - 1.0).abs() < 2.0e-6);
    }

    #[test]
    fn quality_switch_uses_shared_history() {
        let bank = Arc::new(SincBankF32::for_max_rate(1.02));
        let mut reader = QualityVariableDelayF32::new(256, 68, bank, QualityMode::Normal, 32);
        let mut output = 0.0;
        for index in 0..4000 {
            let quality = if index < 2000 {
                QualityMode::Normal
            } else {
                QualityMode::Ultra
            };
            output = reader.process_sample(1.0, 128.25, quality);
        }
        assert!((output - 1.0).abs() < 2.0e-6);
    }
}
