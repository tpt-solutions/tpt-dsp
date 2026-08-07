//! End-to-end SDR receive chain: IQ source → FIR decimation → FM demodulation.
//!
//! ```text
//! 2.4 MS/s IQ ──▶ FIRDecimator ÷10 ──▶ FmDemodulator ──▶ FIRDecimator ÷5 ──▶ 48 kS/s audio
//! ```
//!
//! Run with the built-in generator (an FM-modulated 1 kHz tone):
//!
//! ```text
//! cargo run -p tpt-dsp-io --example sdr_pipeline
//! cargo run -p tpt-dsp-io --example sdr_pipeline -- --seconds 1 --out audio.f32
//! ```
//!
//! Or against a live rtl_tcp-style server emitting unsigned 8-bit IQ:
//!
//! ```text
//! cargo run -p tpt-dsp-io --example sdr_pipeline -- --tcp 127.0.0.1:1234
//! ```
//!
//! The raw output file is mono 32-bit float, little endian, 48 kHz:
//! `ffplay -f f32le -ar 48000 -ch_layout mono audio.f32`.

use std::io::{BufWriter, Write};

use tpt_dsp_core::{Complex32, FIRDecimator, FmDemodulator};
use tpt_dsp_io::{IqFormat, IqSource, SyntheticIqSource, TcpIqSource};

const IQ_RATE_HZ: f64 = 2_400_000.0;
const IQ_DECIM: usize = 10;
const AUDIO_DECIM: usize = 5;
const IQ_TAPS: usize = 63;
const AUDIO_TAPS: usize = 31;
const TONE_HZ: f64 = 1_000.0;
const DEVIATION_HZ: f64 = 25_000.0;
const CHUNK: usize = 4096;

/// A complex baseband stream reduced to audio.
///
/// The anti-alias filter runs on I and Q separately — a real-tap low-pass
/// applied to both halves is a complex low-pass — then the discriminator turns
/// phase steps into audio, which a second decimator brings to 48 kHz.
struct FmReceiver {
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
}

impl FmReceiver {
    fn new(max_chunk: usize) -> Self {
        let decimated = max_chunk / IQ_DECIM + 1;
        Self {
            i_dec: FIRDecimator::design_default(IQ_DECIM, IQ_TAPS),
            q_dec: FIRDecimator::design_default(IQ_DECIM, IQ_TAPS),
            demod: FmDemodulator::with_deviation(
                (IQ_RATE_HZ / IQ_DECIM as f64) as f32,
                DEVIATION_HZ as f32,
            ),
            audio_dec: FIRDecimator::design_default(AUDIO_DECIM, AUDIO_TAPS),
            i_in: vec![0.0; max_chunk],
            q_in: vec![0.0; max_chunk],
            i_out: vec![0.0; decimated],
            q_out: vec![0.0; decimated],
            baseband: vec![Complex32::default(); decimated],
            discriminated: vec![0.0; decimated],
        }
    }

    /// Push one block of IQ, appending the audio it produced to `audio`.
    /// Filter state carries over, so block sizes may vary freely.
    fn process(&mut self, iq: &[Complex32], audio: &mut Vec<f32>) {
        for (sample, (i, q)) in iq
            .iter()
            .zip(self.i_in.iter_mut().zip(self.q_in.iter_mut()))
        {
            *i = sample.re;
            *q = sample.im;
        }

        let n = self.i_dec.process(&self.i_in[..iq.len()], &mut self.i_out);
        self.q_dec.process(&self.q_in[..iq.len()], &mut self.q_out);
        for (z, (&i, &q)) in self.baseband[..n]
            .iter_mut()
            .zip(self.i_out[..n].iter().zip(&self.q_out[..n]))
        {
            *z = Complex32::new(i, q);
        }

        self.demod
            .process(&self.baseband[..n], &mut self.discriminated);

        let start = audio.len();
        audio.resize(start + self.audio_dec.output_len(n), 0.0);
        self.audio_dec
            .process(&self.discriminated[..n], &mut audio[start..]);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut tcp = None;
    let mut out_path = None;
    let mut seconds = 0.25f64;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--tcp" => tcp = args.next(),
            "--out" => out_path = args.next(),
            "--seconds" => seconds = args.next().unwrap_or_default().parse()?,
            "--help" | "-h" => {
                println!("usage: sdr_pipeline [--tcp ADDR] [--out FILE] [--seconds N]");
                return Ok(());
            }
            other => return Err(format!("unknown argument: {other}").into()),
        }
    }

    let total = (IQ_RATE_HZ * seconds) as u64;
    let mut source: Box<dyn IqSource> = match &tcp {
        Some(addr) => {
            println!("source: rtl_tcp {addr} (u8 IQ)");
            Box::new(TcpIqSource::connect(addr.as_str(), IqFormat::U8)?)
        }
        None => {
            println!(
                "source: synthetic FM carrier, {TONE_HZ} Hz tone, {DEVIATION_HZ} Hz deviation"
            );
            Box::new(
                SyntheticIqSource::fm_tone(IQ_RATE_HZ, TONE_HZ, DEVIATION_HZ).with_limit(total),
            )
        }
    };

    let audio_rate = IQ_RATE_HZ / (IQ_DECIM * AUDIO_DECIM) as f64;
    println!(
        "chain: {:.1} MS/s IQ -> /{IQ_DECIM} -> FM demod -> /{AUDIO_DECIM} -> {:.0} Hz audio",
        IQ_RATE_HZ / 1e6,
        audio_rate
    );

    let mut receiver = FmReceiver::new(CHUNK);
    let mut iq = vec![Complex32::default(); CHUNK];
    let mut audio = Vec::new();
    let mut ingested = 0u64;

    let started = std::time::Instant::now();
    loop {
        let n = source.recv(&mut iq)?;
        if n == 0 {
            break;
        }
        ingested += n as u64;
        receiver.process(&iq[..n], &mut audio);
        if tcp.is_some() && ingested >= total {
            break;
        }
    }
    let elapsed = started.elapsed();

    // Statistics skip the filters' start-up transient.
    let warmup = (audio.len() / 10).min(1000);
    let steady = &audio[warmup.min(audio.len())..];
    let peak = steady.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let rms = (steady.iter().map(|s| s * s).sum::<f32>() / steady.len().max(1) as f32).sqrt();
    println!(
        "ingested {ingested} IQ samples -> {} audio samples in {:.3} s ({:.2}x real time)",
        audio.len(),
        elapsed.as_secs_f64(),
        (ingested as f64 / IQ_RATE_HZ) / elapsed.as_secs_f64().max(f64::EPSILON)
    );
    println!("audio peak {peak:.4}, rms {rms:.4} (after {warmup}-sample warm-up)");

    let preview: Vec<String> = audio
        .iter()
        .skip(audio.len() / 2)
        .take(8)
        .map(|s| format!("{s:+.4}"))
        .collect();
    println!("audio[mid..mid+8]: {}", preview.join(" "));

    if let Some(path) = out_path {
        let file = std::fs::File::create(&path)?;
        let mut writer = BufWriter::new(file);
        for sample in &audio {
            writer.write_all(&sample.to_le_bytes())?;
        }
        writer.flush()?;
        println!("wrote {} f32le samples to {path}", audio.len());
    }

    Ok(())
}
