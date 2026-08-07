//! Streaming verification for the IQ ingestion layer.
//!
//! Drives a synthetic 2.4 MS/s IQ stream through the real receive chain —
//! FIR decimation, FM demodulation, audio decimation — and checks that not a
//! single sample is dropped anywhere and that the loop keeps pace with the
//! sample rate.

use std::time::{Duration, Instant};

use tpt_dsp_core::{Complex32, FIRDecimator, FmDemodulator};
use tpt_dsp_io::{IqSource, SyntheticIqSource};

const IQ_RATE_HZ: f64 = 2_400_000.0;
const IQ_DECIM: usize = 10;
const AUDIO_DECIM: usize = 5;
const IQ_TAPS: usize = 63;
const AUDIO_TAPS: usize = 31;
const TONE_HZ: f64 = 1_000.0;
const DEVIATION_HZ: f64 = 25_000.0;

const STREAM_SECONDS: f64 = 1.0;
const TOTAL_IQ: usize = (IQ_RATE_HZ * STREAM_SECONDS) as usize;
const AUDIO_RATE_HZ: f64 = IQ_RATE_HZ / (IQ_DECIM * AUDIO_DECIM) as f64;

/// Deliberately ragged block sizes: none is a multiple of the decimation
/// factor, several are smaller than it, so every call lands the decimator on a
/// different phase and any lost carry-over shows up immediately.
const CHUNK_SIZES: [usize; 6] = [4096, 7, 1000, 337, 8191, 61];
const MAX_CHUNK: usize = 8191;

/// Wall-clock budget for the "keeps pace" assertion, as a multiple of the
/// stream's real-time duration: processing one second of 2.4 MS/s IQ must
/// finish within `REALTIME_MARGIN` seconds.
///
/// Optimized builds are held to strict real time (1.0). That is already a
/// ~36x cushion: the chain measures 0.027 s per second of signal on a
/// developer machine, so the assertion only fires if a change makes the
/// pipeline more than an order of magnitude slower, not if CI is busy.
///
/// `cargo test` builds without optimization, where the FIR inner loops are
/// far slower — 0.78 s per second of signal, so real time is still met but
/// with only a 1.3x cushion. Unoptimized runs therefore get 6.0, which leaves
/// a ~7.7x slack for a slower, loaded or throttled CI runner while still
/// catching a regression that costs several times the current budget. Both
/// profiles print the achieved real-time factor under `--nocapture`.
#[cfg(not(debug_assertions))]
const REALTIME_MARGIN: f64 = 1.0;
#[cfg(debug_assertions)]
const REALTIME_MARGIN: f64 = 6.0;

struct Chain {
    i_dec: FIRDecimator<f32>,
    q_dec: FIRDecimator<f32>,
    demod: FmDemodulator<f32>,
    audio_dec: FIRDecimator<f32>,
    i_in: Vec<f32>,
    q_in: Vec<f32>,
    i_out: Vec<f32>,
    q_out: Vec<f32>,
    baseband: Vec<Complex32>,
    discriminated: Vec<f32>,
    audio: Vec<f32>,
}

impl Chain {
    fn new() -> Self {
        let decimated = MAX_CHUNK / IQ_DECIM + 1;
        Self {
            i_dec: FIRDecimator::design_default(IQ_DECIM, IQ_TAPS),
            q_dec: FIRDecimator::design_default(IQ_DECIM, IQ_TAPS),
            demod: FmDemodulator::with_deviation(
                (IQ_RATE_HZ / IQ_DECIM as f64) as f32,
                DEVIATION_HZ as f32,
            ),
            audio_dec: FIRDecimator::design_default(AUDIO_DECIM, AUDIO_TAPS),
            i_in: vec![0.0; MAX_CHUNK],
            q_in: vec![0.0; MAX_CHUNK],
            i_out: vec![0.0; decimated],
            q_out: vec![0.0; decimated],
            baseband: vec![Complex32::default(); decimated],
            discriminated: vec![0.0; decimated],
            audio: vec![0.0; decimated / AUDIO_DECIM + 1],
        }
    }

    /// Returns `(decimated_iq, audio)` counts for this block; the audio lives
    /// in `self.audio[..audio]` until the next call.
    fn process(&mut self, iq: &[Complex32]) -> (usize, usize) {
        for (sample, (i, q)) in iq
            .iter()
            .zip(self.i_in.iter_mut().zip(self.q_in.iter_mut()))
        {
            *i = sample.re;
            *q = sample.im;
        }

        let expected = self.i_dec.output_len(iq.len());
        let n = self.i_dec.process(&self.i_in[..iq.len()], &mut self.i_out);
        let nq = self.q_dec.process(&self.q_in[..iq.len()], &mut self.q_out);
        assert_eq!(n, expected, "decimator discarded input samples");
        assert_eq!(n, nq, "I and Q decimators disagree");

        for (z, (&i, &q)) in self.baseband[..n]
            .iter_mut()
            .zip(self.i_out[..n].iter().zip(&self.q_out[..n]))
        {
            *z = Complex32::new(i, q);
        }

        self.demod
            .process(&self.baseband[..n], &mut self.discriminated);

        let expected_audio = self.audio_dec.output_len(n);
        let audio = self
            .audio_dec
            .process(&self.discriminated[..n], &mut self.audio);
        assert_eq!(audio, expected_audio, "audio decimator discarded samples");

        (n, audio)
    }
}

#[test]
fn synthetic_stream_runs_frame_drop_free_at_full_rate() {
    let mut source =
        SyntheticIqSource::fm_tone(IQ_RATE_HZ, TONE_HZ, DEVIATION_HZ).with_limit(TOTAL_IQ as u64);
    let mut chain = Chain::new();
    let mut iq = vec![Complex32::default(); MAX_CHUNK];

    let mut ingested = 0usize;
    let mut decimated_total = 0usize;
    let mut audio_total = 0usize;
    let mut short_reads = 0usize;
    let mut recv_calls = 0usize;

    // Signal-level drop detector: a lost or duplicated block breaks the
    // demodulated tone's phase, which shows up as an impossible step between
    // consecutive audio samples.
    let mut previous = 0.0f32;
    let mut max_step = 0.0f32;
    let mut peak = 0.0f32;
    let mut energy = 0.0f64;
    let mut analysed = 0usize;
    // Skip the filters' start-up transient (~1000 audio samples ≈ 21 ms).
    const WARMUP_AUDIO: usize = 1000;

    let started = Instant::now();
    loop {
        let want = CHUNK_SIZES[recv_calls % CHUNK_SIZES.len()].min(TOTAL_IQ - ingested);
        if want == 0 {
            break;
        }
        let n = source.recv(&mut iq[..want]).expect("synthetic recv");
        recv_calls += 1;
        if n == 0 {
            break;
        }
        if n < want {
            short_reads += 1;
        }
        ingested += n;

        let (decimated, audio) = chain.process(&iq[..n]);
        decimated_total += decimated;
        audio_total += audio;

        for (k, &sample) in chain.audio[..audio].iter().enumerate() {
            if audio_total - audio + k >= WARMUP_AUDIO {
                max_step = max_step.max((sample - previous).abs());
                peak = peak.max(sample.abs());
                energy += (sample as f64) * (sample as f64);
                analysed += 1;
            }
            previous = sample;
        }
    }
    let elapsed = started.elapsed();

    // (a) nothing dropped: the source handed over every sample it promised,
    // each stage produced exactly the count its own arithmetic predicts, and
    // no block was silently truncated.
    assert_eq!(ingested, TOTAL_IQ, "source under-delivered");
    assert_eq!(source.delivered(), TOTAL_IQ as u64);
    assert_eq!(short_reads, 0, "a recv returned fewer samples than asked");
    assert_eq!(
        decimated_total,
        TOTAL_IQ / IQ_DECIM,
        "decimated sample count does not match the input"
    );
    assert_eq!(
        audio_total,
        TOTAL_IQ / (IQ_DECIM * AUDIO_DECIM),
        "audio sample count does not match the input"
    );
    assert_eq!(source.recv(&mut iq[..1]).unwrap(), 0, "stream must be done");

    // The recovered tone must be intact across every block boundary.
    let rms = (energy / analysed as f64).sqrt();
    let step_limit = 4.0 * (core::f64::consts::TAU * TONE_HZ / AUDIO_RATE_HZ) as f32;
    assert!(analysed > 0, "no audio analysed");
    assert!(
        max_step < step_limit,
        "audio discontinuity {max_step} exceeds {step_limit}: samples were lost at a block boundary"
    );
    assert!(
        (0.9..=1.1).contains(&peak),
        "demodulated peak {peak} is not the expected full-scale tone"
    );
    assert!(
        (0.6..=0.8).contains(&rms),
        "demodulated rms {rms} is not the expected sine rms"
    );

    // (b) the loop keeps pace with the sample rate.
    let budget = Duration::from_secs_f64(STREAM_SECONDS * REALTIME_MARGIN);
    let realtime_factor = STREAM_SECONDS / elapsed.as_secs_f64();
    println!(
        "processed {ingested} IQ samples ({STREAM_SECONDS:.2} s of signal) in {:.3} s = {realtime_factor:.2}x real time; \
         {recv_calls} recv calls, {audio_total} audio samples, max step {max_step:.4}, rms {rms:.4}",
        elapsed.as_secs_f64()
    );
    assert!(
        elapsed <= budget,
        "processing {STREAM_SECONDS} s of signal took {:.3} s, over the {:.3} s budget \
         (margin {REALTIME_MARGIN}x real time)",
        elapsed.as_secs_f64(),
        budget.as_secs_f64()
    );
}
