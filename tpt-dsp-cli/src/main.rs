//! `tpt-dsp-cli` — command-line WAV/IQ DSP pipeline.
//!
//! ```text
//! tpt-dsp-cli gen      --output tone.wav --waveform sine --freq 1000 --seconds 2
//! tpt-dsp-cli resample --input in.wav --output out.wav --to-rate 44100
//! tpt-dsp-cli design   --kind cheby1 --order 4 --cutoff 1000 --ripple 1 --csv lp.csv
//! tpt-dsp-cli response --effect biquad:lowpass:1000 --rate 48000
//! tpt-dsp-cli filter   --input in.wav --output out.wav --effect biquad:lowpass:1000 --effect delay:0.25
//! tpt-dsp-cli demod    --input iq.u8 --output audio.wav --format u8 --iq-rate 2400000
//! tpt-dsp-cli spectrum --input in.wav --fft-size 2048 --csv spectrum.csv --top 10
//! tpt-dsp-cli info     --input iq.u8 --format i16le
//! tpt-dsp-cli completions bash > tpt-dsp-cli.bash
//! ```

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;
use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use tpt_dsp_cli::tools::{
    design_response, effect_response, generate, parse_pipeline_json, resample, DesignKind,
    DesignSpec, GenSpec, GenWaveform,
};
use tpt_dsp_cli::{
    analyze_complex, analyze_real, demod_file, filter_wav, read_iq, write_spectrum_csv, write_wav,
    Effect, WavData,
};
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

/// Waveform for `gen`.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum WaveArg {
    /// Sine.
    Sine,
    /// Band-limited sawtooth.
    Saw,
    /// Band-limited square.
    Square,
    /// White noise.
    Noise,
    /// Sweep from 20 Hz up to `--freq`.
    Chirp,
}

impl WaveArg {
    fn to_gen(self) -> GenWaveform {
        match self {
            WaveArg::Sine => GenWaveform::Sine,
            WaveArg::Saw => GenWaveform::Saw,
            WaveArg::Square => GenWaveform::Square,
            WaveArg::Noise => GenWaveform::Noise,
            WaveArg::Chirp => GenWaveform::Chirp,
        }
    }
}

/// Filter family for `design`.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum DesignArg {
    /// Butterworth.
    Butter,
    /// Chebyshev type I.
    Cheby1,
    /// Chebyshev type II.
    Cheby2,
}

impl DesignArg {
    fn to_kind(self) -> DesignKind {
        match self {
            DesignArg::Butter => DesignKind::Butterworth,
            DesignArg::Cheby1 => DesignKind::Chebyshev1,
            DesignArg::Cheby2 => DesignKind::Chebyshev2,
        }
    }
}

/// Shells for `completions`.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum ShellArg {
    /// Bash.
    Bash,
    /// Zsh.
    Zsh,
    /// Fish.
    Fish,
    /// PowerShell.
    Powershell,
    /// Elvish.
    Elvish,
}

impl ShellArg {
    fn to_shell(self) -> clap_complete::Shell {
        match self {
            ShellArg::Bash => clap_complete::Shell::Bash,
            ShellArg::Zsh => clap_complete::Shell::Zsh,
            ShellArg::Fish => clap_complete::Shell::Fish,
            ShellArg::Powershell => clap_complete::Shell::PowerShell,
            ShellArg::Elvish => clap_complete::Shell::Elvish,
        }
    }
}

/// tpt-dsp command-line DSP pipeline.
#[derive(Parser)]
#[command(
    name = "tpt-dsp-cli",
    version,
    about = "Command-line WAV/IQ DSP pipeline built on tpt-dsp"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a test WAV file (sine, saw, square, noise or chirp).
    Gen {
        /// Output WAV file.
        #[arg(long)]
        output: PathBuf,
        /// Waveform.
        #[arg(long, value_enum, default_value_t = WaveArg::Sine)]
        waveform: WaveArg,
        /// Frequency in Hz (sweep end for chirp).
        #[arg(long, default_value_t = 440.0)]
        freq: f32,
        /// Duration in seconds.
        #[arg(long, default_value_t = 1.0)]
        seconds: f32,
        /// Sample rate in Hz.
        #[arg(long, default_value_t = 48_000)]
        rate: u32,
        /// Peak amplitude in [0, 1].
        #[arg(long, default_value_t = 0.5)]
        amplitude: f32,
        /// Number of channels.
        #[arg(long, default_value_t = 1)]
        channels: u16,
    },
    /// Resample a WAV file to another sample rate (polyphase, rational ratio).
    Resample {
        /// Input WAV file.
        #[arg(long)]
        input: PathBuf,
        /// Output WAV file.
        #[arg(long)]
        output: PathBuf,
        /// Target sample rate in Hz.
        #[arg(long)]
        to_rate: u32,
    },
    /// Print the magnitude response of a designed low-pass filter as CSV.
    Design {
        /// Filter family.
        #[arg(long, value_enum, default_value_t = DesignArg::Butter)]
        kind: DesignArg,
        /// Filter order.
        #[arg(long, default_value_t = 4)]
        order: usize,
        /// Sample rate in Hz.
        #[arg(long, default_value_t = 48_000.0)]
        rate: f32,
        /// Cutoff (or Chebyshev II stopband edge) in Hz.
        #[arg(long, default_value_t = 1_000.0)]
        cutoff: f32,
        /// Passband ripple in dB (Chebyshev I).
        #[arg(long, default_value_t = 1.0)]
        ripple: f32,
        /// Stopband attenuation in dB (Chebyshev II).
        #[arg(long, default_value_t = 40.0)]
        atten: f32,
        /// Number of frequency points from DC to Nyquist.
        #[arg(long, default_value_t = 512)]
        points: usize,
        /// Write CSV here instead of standard output.
        #[arg(long)]
        csv: Option<PathBuf>,
    },
    /// Measure the magnitude response of an effect chain as CSV.
    Response {
        /// Effect spec, repeatable (same syntax as `filter`).
        #[arg(long = "effect", action = clap::ArgAction::Append)]
        effects: Vec<String>,
        /// Sample rate in Hz.
        #[arg(long, default_value_t = 48_000.0)]
        rate: f32,
        /// Impulse-response length and FFT size (power of two).
        #[arg(long, default_value_t = 8192)]
        fft_size: usize,
        /// Write CSV here instead of standard output.
        #[arg(long)]
        csv: Option<PathBuf>,
    },
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
    /// Apply a JSON pipeline (an ordered list of effects) to a WAV file.
    Pipeline {
        /// JSON pipeline file: `{"effects": [ … ]}`.
        #[arg(long)]
        spec: PathBuf,
        /// Input WAV file.
        #[arg(long)]
        input: PathBuf,
        /// Output WAV file.
        #[arg(long)]
        output: PathBuf,
    },
    /// Print a shell completion script to standard output.
    Completions {
        /// Target shell.
        #[arg(value_enum)]
        shell: ShellArg,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Gen {
            output,
            waveform,
            freq,
            seconds,
            rate,
            amplitude,
            channels,
        } => {
            let data = generate(&GenSpec {
                waveform: waveform.to_gen(),
                freq_hz: freq,
                seconds,
                sample_rate: rate,
                amplitude,
                channels,
            })?;
            write_wav(&output, &data)?;
            println!("wrote `{}`", output.display());
        }
        Command::Resample {
            input,
            output,
            to_rate,
        } => {
            let data = tpt_dsp_cli::read_wav(&input)?;
            let out = resample(&data, to_rate)?;
            write_wav(&output, &out)?;
            println!(
                "wrote `{}` at {} Hz ({} frames)",
                output.display(),
                out.sample_rate,
                out.channels.first().map_or(0, Vec::len)
            );
        }
        Command::Design {
            kind,
            order,
            rate,
            cutoff,
            ripple,
            atten,
            points,
            csv,
        } => {
            let resp = design_response(&DesignSpec {
                kind: kind.to_kind(),
                order,
                sample_rate: rate,
                cutoff_hz: cutoff,
                ripple_db: ripple,
                atten_db: atten,
                points,
            })?;
            emit_response(&resp, csv.as_deref())?;
        }
        Command::Response {
            effects,
            rate,
            fft_size,
            csv,
        } => {
            let specs: Vec<Effect> = effects
                .iter()
                .map(|s| Effect::parse(s))
                .collect::<Result<Vec<_>>>()?;
            let resp = effect_response(&specs, rate, fft_size)?;
            emit_response(&resp, csv.as_deref())?;
        }
        Command::Filter {
            input,
            output,
            effects,
        } => {
            let specs: Vec<Effect> = effects
                .iter()
                .map(|s| Effect::parse(s))
                .collect::<Result<Vec<_>>>()?;
            filter_wav(&input, &output, &specs)?;
            println!("wrote `{}`", output.display());
        }
        Command::Demod {
            input,
            output,
            format,
            iq_rate,
            deviation,
            decimate,
        } => {
            demod_file(
                &input,
                &output,
                format.to_format(),
                iq_rate,
                deviation,
                decimate,
            )?;
            println!("wrote `{}`", output.display());
        }
        Command::Spectrum {
            input,
            format,
            iq_rate,
            fft_size,
            window,
            csv,
            top,
        } => {
            let report = if let Some(fmt) = format {
                let iq = read_iq(&input, fmt.to_format())?;
                if iq.is_empty() {
                    anyhow::bail!("no IQ samples parsed from `{}`", input.display());
                }
                analyze_complex(&iq, iq_rate, fft_size, window.to_window(), top)
            } else {
                let data = read_wav_for_analysis(&input)?;
                let mono = downmix_mono(&data);
                analyze_real(
                    &mono,
                    data.sample_rate as f64,
                    fft_size,
                    window.to_window(),
                    top,
                )
            };
            print_spectrum(&report);
            if let Some(csv_path) = csv {
                write_spectrum_csv(&csv_path, &report.spectrum)?;
                println!("wrote spectrum CSV to `{}`", csv_path.display());
            }
        }
        Command::Info {
            input,
            format,
            iq_rate,
        } => {
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
        Command::Pipeline {
            spec,
            input,
            output,
        } => {
            let text = std::fs::read_to_string(&spec)
                .with_context(|| format!("read pipeline `{}`", spec.display()))?;
            let effects = parse_pipeline_json(&text)?;
            filter_wav(&input, &output, &effects)?;
            println!(
                "applied {} effect(s); wrote `{}`",
                effects.len(),
                output.display()
            );
        }
        Command::Completions { shell } => {
            let mut cmd = Cli::command();
            clap_complete::generate(
                shell.to_shell(),
                &mut cmd,
                "tpt-dsp-cli",
                &mut std::io::stdout(),
            );
        }
    }
    Ok(())
}

/// Write a `(frequency_hz, dB)` response as CSV to a file, or to standard
/// output when no path is given.
fn emit_response(resp: &[(f64, f64)], csv: Option<&Path>) -> Result<()> {
    match csv {
        Some(path) => {
            write_spectrum_csv(path, resp)?;
            println!("wrote response CSV to `{}`", path.display());
        }
        None => {
            let mut out = std::io::stdout().lock();
            writeln!(out, "frequency_hz,magnitude_db")?;
            for (f, db) in resp {
                writeln!(out, "{f},{db}")?;
            }
        }
    }
    Ok(())
}

fn read_wav_for_analysis(path: &Path) -> Result<WavData> {
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
