//! Real-time FFT frequency-band visualizer for audio playback.
//!
//! Computes windowed FFTs on audio PCM samples as they are streamed to the
//! playback device, updating an `Arc<Mutex<VisBands>>` for real-time TUI rendering.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rustfft::{num_complex::Complex, FftPlanner};

const FFT_SIZE: usize = 1024;
/// New samples consumed per FFT frame (overlap = FFT_SIZE - HOP_SIZE).
const HOP_SIZE: usize = 128;
pub const NUM_BANDS: usize = 128;

/// Per-frame decay for individual bands — snappy but not jittery.
const DECAY_FACTOR: f32 = 0.985;
/// Slower decay for the normalization envelope so quiet passages read quiet.
const DECAY_FACTOR_PEAK: f32 = 0.9985;

/// Shared frequency-band state written by the audio engine, read by the UI renderer.
pub struct VisBands {
    pub values: [f32; NUM_BANDS],
    pub updated_at: Instant,
    pub peak_envelope: f32,
    pub is_active: bool,
}

impl VisBands {
    pub fn new() -> Self {
        Self {
            values: [0.0; NUM_BANDS],
            updated_at: Instant::now(),
            peak_envelope: 1e-6,
            is_active: false,
        }
    }

    pub fn shared() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self::new()))
    }
}

impl Default for VisBands {
    fn default() -> Self {
        Self::new()
    }
}

/// Standalone FFT processor for PCM audio streams.
pub struct FftProcessor {
    sample_buf: VecDeque<f32>,
    bands: Arc<Mutex<VisBands>>,
    fft: Arc<dyn rustfft::Fft<f32>>,
    hann_window: Vec<f32>,
    fft_buf: Vec<Complex<f32>>,
    magnitudes: Vec<f32>,
    sample_rate: f32,
    band_ranges: Vec<(usize, usize)>,
    new_bands: [f32; NUM_BANDS],
    smooth_scratch: [f32; NUM_BANDS],
    channel_accumulator: Vec<f32>,
}

impl FftProcessor {
    pub fn new(bands: Arc<Mutex<VisBands>>, sample_rate: f32) -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        let hann_window: Vec<f32> = (0..FFT_SIZE)
            .map(|i| {
                0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (FFT_SIZE - 1) as f32).cos())
            })
            .collect();
        let band_ranges = precompute_band_ranges(FFT_SIZE / 2, NUM_BANDS);
        Self {
            sample_buf: VecDeque::with_capacity(FFT_SIZE * 2),
            bands,
            fft,
            hann_window,
            fft_buf: vec![Complex::new(0.0, 0.0); FFT_SIZE],
            magnitudes: vec![0.0; FFT_SIZE / 2],
            sample_rate,
            band_ranges,
            new_bands: [0.0; NUM_BANDS],
            smooth_scratch: [0.0; NUM_BANDS],
            channel_accumulator: Vec::with_capacity(2),
        }
    }

    pub fn process_sample(&mut self, sample: f32, channels: u16) {
        let num_ch = channels.max(1) as usize;
        self.channel_accumulator.push(sample);
        if self.channel_accumulator.len() >= num_ch {
            let mono_sample: f32 = self.channel_accumulator.iter().sum::<f32>() / (num_ch as f32);
            self.channel_accumulator.clear();
            self.sample_buf.push_back(mono_sample);
            self.run_fft_if_ready();
        }
    }

    fn run_fft_if_ready(&mut self) {
        while self.sample_buf.len() >= FFT_SIZE {
            {
                let (front, back) = self.sample_buf.as_slices();
                if front.len() >= FFT_SIZE {
                    for (dst, (&s, &w)) in self
                        .fft_buf
                        .iter_mut()
                        .zip(front.iter().zip(self.hann_window.iter()))
                    {
                        *dst = Complex::new(s * w, 0.0);
                    }
                } else {
                    let split = front.len();
                    for (dst, (&s, &w)) in self.fft_buf[..split]
                        .iter_mut()
                        .zip(front.iter().zip(self.hann_window[..split].iter()))
                    {
                        *dst = Complex::new(s * w, 0.0);
                    }
                    let remaining = FFT_SIZE - split;
                    for (dst, (&s, &w)) in self.fft_buf[split..].iter_mut().zip(
                        back[..remaining]
                            .iter()
                            .zip(self.hann_window[split..].iter()),
                    ) {
                        *dst = Complex::new(s * w, 0.0);
                    }
                }
            }

            self.fft.process(&mut self.fft_buf);

            for (mag, c) in self.magnitudes.iter_mut().zip(self.fft_buf.iter()) {
                *mag = c.norm();
            }

            fill_log_bands(&self.magnitudes, &self.band_ranges, &mut self.new_bands);
            smooth_bands(&mut self.new_bands, &mut self.smooth_scratch);

            if let Ok(mut g) = self.bands.lock() {
                let elapsed_hops =
                    g.updated_at.elapsed().as_secs_f32() * self.sample_rate / HOP_SIZE as f32;
                let decay = DECAY_FACTOR.powf(elapsed_hops);
                let peak_decay = DECAY_FACTOR_PEAK.powf(elapsed_hops);
                let frame_peak = self.new_bands.iter().copied().fold(0.0_f32, f32::max);
                for (stored, fresh) in g.values.iter_mut().zip(self.new_bands.iter()) {
                    *stored = (*stored * decay).max(*fresh);
                }
                g.peak_envelope = (g.peak_envelope * peak_decay).max(frame_peak);
                g.updated_at = Instant::now();
                g.is_active = true;
            }

            self.sample_buf.drain(..HOP_SIZE);
        }
    }
}

/// A wrapper around a `rodio::Source` that computes real-time FFT bands as audio plays.
pub struct FftSource<S> {
    inner: S,
    processor: FftProcessor,
    channels: u16,
}

impl<S> FftSource<S>
where
    S: rodio::Source<Item = f32>,
{
    pub fn new(inner: S, bands: Arc<Mutex<VisBands>>) -> Self {
        let channels = inner.channels();
        let sample_rate = inner.sample_rate() as f32;
        let processor = FftProcessor::new(bands, sample_rate);
        Self {
            inner,
            processor,
            channels,
        }
    }
}

impl<S> Iterator for FftSource<S>
where
    S: rodio::Source<Item = f32>,
{
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.inner.next()?;
        self.processor.process_sample(sample, self.channels);
        Some(sample)
    }
}

impl<S> rodio::Source for FftSource<S>
where
    S: rodio::Source<Item = f32>,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.inner.channels()
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

fn precompute_band_ranges(num_bins: usize, num_bands: usize) -> Vec<(usize, usize)> {
    let log_min = 1.0_f64;
    let log_max = num_bins as f64;
    let mut used_up_to: usize = 1;
    let mut ranges = Vec::with_capacity(num_bands);
    for band in 0..num_bands {
        if used_up_to >= num_bins {
            ranges.push((num_bins - 1, num_bins));
            continue;
        }
        let t_start = band as f64 / num_bands as f64;
        let t_end = (band + 1) as f64 / num_bands as f64;
        let natural_start = (log_min * (log_max / log_min).powf(t_start)) as usize;
        let natural_end = (log_min * (log_max / log_min).powf(t_end)) as usize;
        let start = natural_start.max(used_up_to).min(num_bins - 1);
        let end = natural_end.max(start + 1).min(num_bins);
        used_up_to = end;
        ranges.push((start, end));
    }
    ranges
}

fn fill_log_bands(magnitudes: &[f32], band_ranges: &[(usize, usize)], out: &mut [f32]) {
    for (band_val, &(start, end)) in out.iter_mut().zip(band_ranges.iter()) {
        let len = (end - start) as f32;
        let sum_sq: f32 = magnitudes[start..end].iter().map(|&v| v * v).sum();
        *band_val = (sum_sq / len).sqrt();
    }
}

fn smooth_bands(bands: &mut [f32], scratch: &mut [f32]) {
    let n = bands.len();
    if n < 3 {
        return;
    }
    scratch[..n].copy_from_slice(&bands[..n]);
    for i in 0..n {
        let prev = scratch[if i > 0 { i - 1 } else { 0 }];
        let next = scratch[if i + 1 < n { i + 1 } else { n - 1 }];
        bands[i] = prev * 0.25 + scratch[i] * 0.5 + next * 0.25;
    }
}
