//! Short-time Fourier transform (STFT), its inverse, and a phase-vocoder
//! time stretcher.
//!
//! Frames use a periodic Hann window at analysis hop `hop`. [`istft`] is the
//! weighted overlap-add inverse, which reconstructs the input exactly wherever
//! the window envelope is non-zero (away from the first and last `fft_size`
//! samples when the hop is large).
//!
//! [`time_stretch`] is a phase vocoder: it changes duration without changing
//! pitch by keeping each partial's instantaneous frequency and re-deriving its
//! phase at the new hop. These functions allocate their results; they are for
//! offline processing.

use tpt_dsp_core::{windowed, FftPlan, WindowType, C32};

fn hann(n: usize) -> Vec<f32> {
    let mut w = vec![0.0f32; n];
    windowed(WindowType::Hann, n, &mut w);
    w
}

fn wrap_pi(x: f32) -> f32 {
    let tau = core::f32::consts::TAU;
    let mut y = (x + core::f32::consts::PI).rem_euclid(tau);
    y -= core::f32::consts::PI;
    y
}

/// Forward STFT: one one-sided spectrum (`fft_size/2 + 1` bins) per frame.
///
/// Frames start at `0, hop, 2·hop, …` and continue while the start is inside
/// the input; samples past the end are zero.
///
/// # Panics
///
/// Panics if `fft_size` is not a power of two ≥ 4 or `hop` is zero.
pub fn stft(input: &[f32], fft_size: usize, hop: usize) -> Vec<Vec<C32>> {
    assert!(
        fft_size >= 4 && fft_size.is_power_of_two(),
        "fft_size must be a power of two"
    );
    assert!(hop >= 1, "hop must be at least 1");
    let w = hann(fft_size);
    let mut plan = FftPlan::new_forward(fft_size);
    let mut buf = vec![C32::new(0.0, 0.0); fft_size];
    let mut frames = Vec::new();
    let mut start = 0usize;
    while start < input.len() {
        for (i, slot) in buf.iter_mut().enumerate() {
            let x = input.get(start + i).copied().unwrap_or(0.0);
            *slot = C32::new(x * w[i], 0.0);
        }
        plan.process_inplace(&mut buf);
        frames.push(buf[..fft_size / 2 + 1].to_vec());
        start += hop;
    }
    frames
}

/// Inverse STFT by weighted overlap-add, returning `len` samples.
///
/// `frames` must come from [`stft`] with the same `fft_size` and `hop`.
pub fn istft(frames: &[Vec<C32>], fft_size: usize, hop: usize, len: usize) -> Vec<f32> {
    let w = hann(fft_size);
    let mut plan = FftPlan::new_inverse(fft_size);
    let mut buf = vec![C32::new(0.0, 0.0); fft_size];
    let total = frames.len().saturating_sub(1) * hop + fft_size;
    let mut out = vec![0.0f32; total.max(len)];
    let mut env = vec![0.0f32; total.max(len)];
    let scale = 1.0 / fft_size as f32;
    for (m, frame) in frames.iter().enumerate() {
        let half = fft_size / 2;
        for (k, slot) in buf.iter_mut().enumerate().take(half + 1) {
            *slot = frame.get(k).copied().unwrap_or(C32::new(0.0, 0.0));
        }
        // Hermitian symmetry gives a real signal from the one-sided spectrum.
        for k in 1..half {
            buf[fft_size - k] = buf[k].conj();
        }
        plan.process_inplace(&mut buf);
        let start = m * hop;
        for i in 0..fft_size {
            out[start + i] += buf[i].re * scale * w[i];
            env[start + i] += w[i] * w[i];
        }
    }
    (0..len)
        .map(|i| if env[i] > 1e-6 { out[i] / env[i] } else { 0.0 })
        .collect()
}

/// Phase-vocoder time stretch.
///
/// `rate` is the playback-speed factor: `2.0` halves the duration, `0.5`
/// doubles it. Pitch is preserved. `hop` is the analysis hop (typically
/// `fft_size / 4`).
///
/// # Panics
///
/// Panics if `fft_size` is not a power of two ≥ 4, `hop` is zero, or `rate` is
/// not finite and positive.
pub fn time_stretch(input: &[f32], fft_size: usize, hop: usize, rate: f32) -> Vec<f32> {
    assert!(
        fft_size >= 4 && fft_size.is_power_of_two(),
        "fft_size must be a power of two"
    );
    assert!(hop >= 1, "hop must be at least 1");
    assert!(
        rate.is_finite() && rate > 0.0,
        "rate must be finite and positive"
    );
    if input.is_empty() {
        return Vec::new();
    }
    let w = hann(fft_size);
    let mut fwd = FftPlan::new_forward(fft_size);
    let mut inv = FftPlan::new_inverse(fft_size);
    let bins = fft_size / 2 + 1;
    let tau = core::f32::consts::TAU;
    let n = fft_size as f32;
    let synth_hop = hop;
    let out_len = (input.len() as f32 / rate).ceil() as usize + fft_size;
    let mut out = vec![0.0f32; out_len];
    let mut env = vec![0.0f32; out_len];

    let mut buf = vec![C32::new(0.0, 0.0); fft_size];
    let mut prev_phase = vec![0.0f32; bins];
    let mut synth_phase = vec![0.0f32; bins];
    let mut prev_pos = 0.0f32;
    let mut m = 0usize;
    loop {
        let pos = m as f32 * hop as f32 * rate;
        if pos >= input.len() as f32 {
            break;
        }
        let p = pos.round() as usize;
        for (i, slot) in buf.iter_mut().enumerate() {
            let x = input.get(p + i).copied().unwrap_or(0.0);
            *slot = C32::new(x * w[i], 0.0);
        }
        fwd.process_inplace(&mut buf);

        let mut spec = vec![C32::new(0.0, 0.0); bins];
        if m == 0 {
            for k in 0..bins {
                synth_phase[k] = buf[k].arg();
                spec[k] = C32::from_polar(buf[k].norm(), synth_phase[k]);
            }
        } else {
            let advance = pos - prev_pos; // analysis hop actually used
            for k in 0..bins {
                let phi = buf[k].arg();
                // Expected phase advance of bin k over `advance` samples.
                let expected = tau * k as f32 * advance / n;
                let deviation = wrap_pi(phi - prev_phase[k] - expected);
                // True per-sample angular frequency, applied over the synthesis hop.
                let true_advance = expected + deviation;
                synth_phase[k] += true_advance / advance * synth_hop as f32;
                spec[k] = C32::from_polar(buf[k].norm(), synth_phase[k]);
            }
        }
        for k in 0..bins {
            prev_phase[k] = buf[k].arg();
        }
        prev_pos = pos;

        // Mirror to a full spectrum and transform back.
        buf[..bins].copy_from_slice(&spec[..bins]);
        for k in 1..fft_size / 2 {
            buf[fft_size - k] = spec[k].conj();
        }
        inv.process_inplace(&mut buf);
        let start = m * synth_hop;
        for i in 0..fft_size {
            if start + i < out_len {
                out[start + i] += buf[i].re / n * w[i];
                env[start + i] += w[i] * w[i];
            }
        }
        m += 1;
    }
    let len = (input.len() as f32 / rate).round() as usize;
    (0..len.min(out_len))
        .map(|i| if env[i] > 1e-6 { out[i] / env[i] } else { 0.0 })
        .collect()
}

/// Shift the pitch of `input` by `ratio` without changing its duration.
///
/// `ratio` is a frequency factor: `2.0` raises the pitch by an octave, `0.5`
/// lowers it by one, and `1.0` returns the input (up to the vocoder's
/// rounding). The output has the same length as the input.
///
/// The method is a phase vocoder followed by resampling. [`time_stretch`]
/// first changes the duration to `len / (1 / ratio)` with the pitch kept, and
/// the result is then read back over the original length. That read speed is
/// `ratio`, which is the pitch change. Interpolation is cubic (Catmull-Rom).
///
/// # Panics
///
/// Panics under the same conditions as [`time_stretch`], and if `ratio` is not
/// finite and positive.
pub fn pitch_shift(input: &[f32], fft_size: usize, hop: usize, ratio: f32) -> Vec<f32> {
    assert!(
        ratio.is_finite() && ratio > 0.0,
        "ratio must be finite and positive"
    );
    if input.is_empty() {
        return Vec::new();
    }
    // A stretch by 1/ratio gives a signal of length input.len() * ratio.
    let stretched = time_stretch(input, fft_size, hop, 1.0 / ratio);
    let n = input.len();
    let m = stretched.len();
    if m < 2 {
        return vec![0.0; n];
    }
    let step = (m - 1) as f64 / (n - 1).max(1) as f64;
    (0..n)
        .map(|i| {
            let pos = i as f64 * step;
            let k = pos.floor() as usize;
            let t = (pos - k as f64) as f32;
            let at = |j: isize| -> f32 {
                let j = j.clamp(0, m as isize - 1) as usize;
                stretched[j]
            };
            let (p0, p1, p2, p3) = (
                at(k as isize - 1),
                at(k as isize),
                at(k as isize + 1),
                at(k as isize + 2),
            );
            // Catmull-Rom spline through the four neighbouring samples.
            let a = -0.5 * p0 + 1.5 * p1 - 1.5 * p2 + 0.5 * p3;
            let b = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
            let c = -0.5 * p0 + 0.5 * p2;
            ((a * t + b) * t + c) * t + p1
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 16_000.0;

    fn sine(f: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (i as f32 * f * core::f32::consts::TAU / SR).sin())
            .collect()
    }

    fn peak_bin(x: &[f32], fft: usize) -> usize {
        let frames = stft(&x[..fft], fft, fft);
        let mags: Vec<f32> = frames[0].iter().map(|c| c.norm()).collect();
        mags.iter()
            .enumerate()
            .skip(1)
            .fold(
                (0usize, 0.0f32),
                |best, (k, &m)| if m > best.1 { (k, m) } else { best },
            )
            .0
    }

    #[test]
    fn stft_istft_reconstructs_the_interior() {
        let x = sine(440.0, 4096);
        let frames = stft(&x, 512, 128);
        let y = istft(&frames, 512, 128, x.len());
        for i in 512..(x.len() - 512) {
            assert!(
                (x[i] - y[i]).abs() < 1e-3,
                "sample {i}: {} vs {}",
                x[i],
                y[i]
            );
        }
    }

    #[test]
    fn stft_peak_is_at_the_tone_bin() {
        let fft = 512;
        let x = sine(1000.0, fft);
        let expected = (1000.0 * fft as f32 / SR).round() as usize;
        assert_eq!(peak_bin(&x, fft), expected);
    }

    #[test]
    fn time_stretch_changes_length_and_keeps_pitch() {
        let fft = 1024;
        let x = sine(1000.0, 16_000);
        let slow = time_stretch(&x, fft, fft / 4, 0.5);
        let expected_len = 32_000;
        assert!(
            (slow.len() as i64 - expected_len as i64).abs() < 64,
            "length {}",
            slow.len()
        );
        // Measure pitch in the steady middle of the stretched output.
        let mid = &slow[8000..8000 + fft];
        let bin = peak_bin(mid, fft);
        let f = bin as f32 * SR / fft as f32;
        assert!((f - 1000.0).abs() < 20.0, "pitch drifted to {f} Hz");
    }

    #[test]
    fn time_stretch_output_is_finite() {
        let x = sine(300.0, 8000);
        let y = time_stretch(&x, 512, 128, 1.7);
        assert!(y.iter().all(|v| v.is_finite()));
        assert!(!y.is_empty());
    }
}

#[cfg(test)]
mod pitch_shift_tests {
    use super::*;

    const SR: f32 = 16_000.0;

    fn sine(f: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (i as f32 * f * core::f32::consts::TAU / SR).sin())
            .collect()
    }

    /// Frequency of the strongest bin in a long Hann-windowed analysis.
    fn dominant_hz(x: &[f32]) -> f32 {
        let fft = 8192;
        let frames = stft(&x[..fft], fft, fft);
        let mags: Vec<f32> = frames[0].iter().map(|c| c.norm()).collect();
        let (k, _) = mags
            .iter()
            .enumerate()
            .skip(1)
            .fold(
                (0usize, 0.0f32),
                |best, (k, &m)| {
                    if m > best.1 {
                        (k, m)
                    } else {
                        best
                    }
                },
            );
        k as f32 * SR / fft as f32
    }

    #[test]
    fn octave_up_doubles_the_frequency_and_keeps_the_length() {
        let x = sine(440.0, 32_000);
        let y = pitch_shift(&x, 2048, 512, 2.0);
        assert_eq!(y.len(), x.len());
        let f = dominant_hz(&y);
        assert!((f - 880.0).abs() < 10.0, "got {f} Hz");
    }

    #[test]
    fn octave_down_halves_the_frequency() {
        let x = sine(440.0, 32_000);
        let y = pitch_shift(&x, 2048, 512, 0.5);
        assert_eq!(y.len(), x.len());
        let f = dominant_hz(&y);
        assert!((f - 220.0).abs() < 10.0, "got {f} Hz");
    }

    #[test]
    fn unit_ratio_keeps_the_pitch() {
        let x = sine(440.0, 32_000);
        let y = pitch_shift(&x, 2048, 512, 1.0);
        assert!((dominant_hz(&y) - 440.0).abs() < 10.0);
    }

    #[test]
    fn empty_input_gives_empty_output() {
        assert!(pitch_shift(&[], 512, 128, 2.0).is_empty());
    }

    #[test]
    #[should_panic(expected = "ratio must be finite and positive")]
    fn zero_ratio_panics() {
        let _ = pitch_shift(&[0.0; 64], 64, 16, 0.0);
    }
}
