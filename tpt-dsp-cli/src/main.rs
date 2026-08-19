//! `tpt-dsp-cli` — command-line WAV/IQ DSP pipeline.
//!
//! ```text
//! tpt-dsp-cli filter   --input in.wav --output out.wav --effect biquad:lowpass:1000 --effect delay:0.25
//! tpt-dsp-cli demod    --input iq.u8 --output audio.wav --format u8 --iq-rate 2400000
//! tpt-dsp-cli spectrum --input in.wav --fft-size 2048 --csv spectrum.csv --top 10
//! tpt-dsp-cli info     --input iq.u8 --format i16le
//! ```

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use tpt_dsp_cli::{analyze_complex, analyze_real, demod_file, filter_wav, read_iq, write_spectrum_csv, Effect, WavData};
use tpt_dsp_core::WindowType;
use tpt_dsp_io::IqFormat;

/// IQ sample byte layout.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum FormatArg {
    /// 8-bit unsigned, offset-binary.
    U8,
    /// 16-bit signed little-endian.
    I16Le,
    /// 16-bit signed big-endian.
    I16Be,
    /// 32-bit float little-endian.
    F32Le,
}

impl FormatArg {
    fn to_format(self) -> IqFormat {
        match self {
            FormatArg::U8 => IqFormat::U8,
            FormatArg::I16Le => IqFormat::I16Le,
            FormatArg::I16Be => IqFormat::I16Be,
            FormatArg::F32Le => IqFormat::F32Le,
        }
    }
}

/// Spectral window.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum WindowArg {
    /// Hann (raised cosine).
    Hann,
    /// Hamming.
    Hamming,
    /// Blackman.
    Blackman,
}

impl WindowArg {
    fn to_window(self) -> WindowType {
        match self {
            WindowArg::Hann => WindowType::Hann,
            WindowArg::Hamming => WindowType::Hamming,
            WindowArg::Blackman => WindowType::Blackman,
        }
    }
}

/// tpt-dsp command-line DSP pipeline.
#[derive(Parser)]
#[command(name = "tpt-dsp-cli", version, about = "Command-line WAV/IQ DSP pipeline built on tpt-dsp")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Apply a chain of effects to a WAV file (one chain per channel).
    Filter {
        /// Input WAV file.
        #[arg(long)]
        input: PathBuf,
        /// Output WAV file.
        #[arg(long)]
        output: PathBuf,
        /// Effect spec, repeatable. See `Effect::parse` in the library docs.
        #[arg(long = "effect", action = clap::ArgAction::Append)]
        effects: Vec<String>,
    },
    /// FM-demodulate a raw IQ file into a mono WAV file.
    Demod {
        /// Input raw IQ file.
        #[arg(long)]
        input: PathBuf,
        /// Output WAV file.
        #[arg(long)]
        output: PathBuf,
        /// IQ byte format.
        #[arg(long, value_enum, default_value_t = FormatArg::U8)]
        format: FormatArg,
        /// IQ sample rate in Hz.
        #[arg(long, default_value_t = 2_400_000.0)]
        iq_rate: f64,
        /// FM deviation in Hz.
        #[arg(long, default_value_t = 25_000.0)]
        deviation: f64,
        /// Optional integer decimation factor for the output rate.
        #[arg(long)]
        decimate: Option<usize>,
    },
    /// Analyze a WAV or IQ file: spectrum, peak and time-domain features.
    Spectrum {
        /// Input WAV or IQ file. IQ is assumed when `--format` is given.
        #[arg(long)]
        input: PathBuf,
        /// IQ byte format (selects IQ input instead of WAV).
        #[arg(long)]
        format: Option<FormatArg>,
        /// Sample rate for IQ input in Hz.
        #[arg(long, default_value_t = 2_400_000.0)]
        iq_rate: f64,
        /// FFT / transform size.
        #[arg(long, default_value_t = 1024)]
        fft_size: usize,
        /// Spectral window.
        #[arg(long, value_enum, default_value_t = WindowArg::Hann)]
        window: WindowArg,
        /// Optional CSV output of the averaged spectrum.
        #[arg(long)]
        csv: Option<PathBuf>,
        /// Number of strongest peaks to report.
        #[arg(long, default_value_t = 8)]
        top: usize,
    },
    /// Print file metadata (WAV header or IQ size/format).
    Info {
        /// Input WAV or IQ file.
        #[arg(long)]
        input: PathBuf,
        /// IQ byte format (selects IQ input instead of WAV).
        #[arg(long)]
        format: Option<FormatArg>,
        /// Assumed IQ sample rate in Hz (informational only).
        #[arg(long, default_value_t = 2_400_000.0)]
        iq_rate: f64,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Filter { input, output, effects } => {
            let specs: Vec<Effect> = effects
                .iter()
                .map(|s| Effect::parse(s))
                .collect::<Result<Vec<_>>>()?;
            filter_wav(&input, &output, &specs)?;
            println!("wrote `{}`", output.display());
        }
        Command::Demod { input, output, format, iq_rate, deviation, decimate } => {
            demod_file(&input, &output, format.to_format(), iq_rate, deviation, decimate)?;
            println!("wrote `{}`", output.display());
        }
        Command::Spectrum { input, format, iq_rate, fft_size, window, csv, top } => {
            let report = if let Some(fmt) = format {
                let iq = read_iq(&input, fmt.to_format())?;
                if iq.is_empty() {
                    anyhow::bail!("no IQ samples parsed from `{}`", input.display());
                }
                analyze_complex(&iq, iq_rate, fft_size, window.to_window(), top)
            } else {
                let data = read_wav_for_analysis(&input)?;
                let mono = downmix_mono(&data);
                analyze_real(&mono, data.sample_rate as f64, fft_size, window.to_window(), top)
            };
            print_spectrum(&report);
            if let Some(csv_path) = csv {
                write_spectrum_csv(&csv_path, &report.spectrum)?;
                println!("wrote spectrum CSV to `{}`", csv_path.display());
            }
        }
        Command::Info { input, format, iq_rate } => {
            if let Some(fmt) = format {
                let iq = read_iq(&input, fmt.to_format())?;
                let seconds = iq.len() as f64 / iq_rate;
                println!(
                    "IQ file `{}`: format {:?}, {} complex samples, {:.3} s @ {:.0} Hz",
                    input.display(),
                    fmt,
                    iq.len(),
                    seconds,
                    iq_rate
                );
            } else {
                let data = read_wav_for_analysis(&input)?;
                let frames = data.channels.iter().map(Vec::len).min().unwrap_or(0);
                println!(
                    "WAV file `{}`: {} Hz, {} channel(s), {} frames ({:.3} s)",
                    input.display(),
                    data.sample_rate,
                    data.channels.len(),
                    frames,
                    frames as f64 / data.sample_rate as f64
                );
            }
        }
    }
    Ok(())
}

fn read_wav_for_analysis(path: &PathBuf) -> Result<WavData> {
    let data = tpt_dsp_cli::read_wav(path)?;
    if data.channels.is_empty() {
        anyhow::bail!("wav `{}` has no channels", path.display());
    }
    Ok(data)
}

/// Average all channels of a WAV down to a single mono buffer for analysis.
fn downmix_mono(data: &WavData) -> Vec<f32> {
    let frames = data.channels.iter().map(Vec::len).min().unwrap_or(0);
    let channels = data.channels.len().max(1) as f32;
    (0..frames)
        .map(|i| data.channels.iter().map(|c| c[i]).sum::<f32>() / channels)
        .collect()
}

fn print_spectrum(report: &tpt_dsp_cli::SpectrumReport) {
    println!("sample rate : {:.0} Hz", report.sample_rate);
    println!("dominant     : {:.2} Hz", report.dominant_hz);
    println!("peak level   : {:.2} dB", report.peak_db);
    println!("rms          : {:.4}", report.rms);
    println!("zero-cross   : {:.4}", report.zero_crossing_rate);
    println!("centroid     : {:.2} Hz", report.spectral_centroid_hz);
    if report.top_peaks.is_empty() {
        println!("peaks        : none above threshold");
    } else {
        let listed: Vec<String> = report
            .top_peaks
            .iter()
            .map(|(bin, db)| format!("#{bin} {:.1} dB", db))
            .collect();
        println!("top peaks    : {}", listed.join(", "));
    }
}
