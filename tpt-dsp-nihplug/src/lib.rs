//! `tpt-dsp-nihplug` — a CLAP/VST3 audio plugin wrapping `tpt-dsp-audio`.
//!
//! The plugin is a small pedalboard chain built from the framework's own
//! real-time-safe effects, exactly like the WASM pedalboard and the CLI filter
//! command:
//!
//! ```text
//! Waveshaper (Tanh) → Delay → ConvolutionReverb → 3-band EQ
//! ```
//!
//! Every stage is driven by a host-automated parameter, and all processing
//! runs in place on pre-allocated buffers (no allocation on the audio thread).
//!
//! # Framework note
//!
//! The original `nih-plug` crate is no longer published on crates.io (it is in
//! maintenance mode). This crate uses [`nice-plug`](https://docs.rs/nice-plug),
//! the API-compatible community fork, and exports CLAP + VST3 builds via
//! `nice_export_clap!` / `nice_export_vst3!`.
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.

#![warn(rust_2018_idioms)]

use std::num::NonZeroU32;
use std::sync::Arc;

use nice_plug::prelude::*;
use tpt_dsp_audio::{generate_decay_ir, ConvolutionReverb, Curve, Delay, Waveshaper};
use tpt_dsp_core::{Biquad, BiquadCoeffs, BiquadType};

/// One voice: the full per-channel effect chain.
///
/// A separate instance is kept per input channel so channel state (delay lines,
/// reverb history) never mixes. All buffers are pre-allocated in
/// [`Voice::new`]; [`Voice::process`](Self::process) performs no allocation.
struct Voice {
    shaper: Waveshaper,
    delay: Delay,
    reverb: ConvolutionReverb,
    low: Biquad<f32>,
    peak: Biquad<f32>,
    high: Biquad<f32>,
    scratch: Vec<f32>,
}

impl Voice {
    fn new(sample_rate: f32, max_block: usize) -> Self {
        let ir = generate_decay_ir(4096, sample_rate, 0.3);
        let mut delay = Delay::new(((sample_rate).ceil() as usize).max(1) + 1);
        delay.set_delay_seconds(0.25, sample_rate);
        delay.set_feedback(0.3);
        delay.set_mix(0.3);
        Voice {
            shaper: Waveshaper::new(Curve::Tanh, 1.0, 1.0),
            delay,
            reverb: ConvolutionReverb::new(&ir, 256),
            low: Biquad::<f32>::design(BiquadType::LowShelf, sample_rate, 200.0, 0.707, 0.0),
            peak: Biquad::<f32>::design(BiquadType::Peaking, sample_rate, 1000.0, 1.0, 0.0),
            high: Biquad::<f32>::design(BiquadType::HighShelf, sample_rate, 8000.0, 0.707, 0.0),
            scratch: vec![0.0f32; max_block],
        }
    }

    /// Run the whole chain over one channel buffer in place.
    fn process(&mut self, buf: &mut [f32]) {
        self.shaper.process(buf);
        self.delay.process(buf);

        let len = buf.len();
        self.scratch[..len].copy_from_slice(buf);
        self.reverb.process(&self.scratch[..len], buf);

        self.low.process(buf, &mut self.scratch);
        buf.copy_from_slice(&self.scratch[..len]);
        self.peak.process(buf, &mut self.scratch);
        buf.copy_from_slice(&self.scratch[..len]);
        self.high.process(buf, &mut self.scratch);
        buf.copy_from_slice(&self.scratch[..len]);
    }
}

/// Plugin parameters.
#[derive(Params)]
struct PedalParams {
    /// Waveshaper pre-gain.
    #[id = "drive"]
    drive: FloatParam,
    /// Waveshaper wet/dry mix.
    #[id = "mix"]
    mix: FloatParam,
    /// Delay time in seconds.
    #[id = "delay_time"]
    delay_time: FloatParam,
    /// Delay feedback gain.
    #[id = "feedback"]
    feedback: FloatParam,
    /// Reverb wet amount.
    #[id = "reverb_wet"]
    reverb_wet: FloatParam,
    /// Low-shelf EQ gain in dB.
    #[id = "low_gain"]
    low_gain: FloatParam,
    /// Peaking EQ gain in dB.
    #[id = "peak_gain"]
    peak_gain: FloatParam,
    /// High-shelf EQ gain in dB.
    #[id = "high_gain"]
    high_gain: FloatParam,
}

impl Default for PedalParams {
    fn default() -> Self {
        let gain = |name: &str, default: f32| {
            FloatParam::new(
                name,
                default,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            )
        };
        Self {
            drive: FloatParam::new(
                "Drive",
                1.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 10.0,
                },
            ),
            mix: FloatParam::new(
                "Waveshaper Mix",
                1.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            ),
            delay_time: FloatParam::new(
                "Delay Time",
                0.25,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            ),
            feedback: FloatParam::new(
                "Feedback",
                0.3,
                FloatRange::Linear {
                    min: 0.0,
                    max: 0.95,
                },
            ),
            reverb_wet: FloatParam::new(
                "Reverb Mix",
                0.2,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            ),
            low_gain: gain("Low Shelf", 0.0),
            peak_gain: gain("Mid Peak", 0.0),
            high_gain: gain("High Shelf", 0.0),
        }
    }
}

/// The tpt-dsp pedalboard plugin.
struct TptDspPedalboard {
    params: Arc<PedalParams>,
    voices: Vec<Voice>,
    sample_rate: f32,
    max_block: usize,
}

impl Default for TptDspPedalboard {
    fn default() -> Self {
        Self {
            params: Arc::new(PedalParams::default()),
            voices: Vec::new(),
            sample_rate: 48_000.0,
            max_block: 1024,
        }
    }
}

impl Plugin for TptDspPedalboard {
    const NAME: &'static str = "tpt-dsp Pedalboard";
    const VENDOR: &'static str = "TPT Solutions";
    const URL: &'static str = "https://github.com/tpt-solutions/tpt-dsp";
    const EMAIL: &'static str = "info@tpt-solutions.example";
    const VERSION: &'static str = "0.1.0";

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    type SysExMessage = ();
    type BackgroundTask = ();
    type Editor = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn activate(
        &mut self,
        layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;
        self.max_block = buffer_config.max_buffer_size as usize;
        self.voices = (0..layout.main_input_channels.unwrap().get() as usize)
            .map(|_| Voice::new(self.sample_rate, self.max_block))
            .collect();
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer<'_>,
        _aux: &mut AuxiliaryBuffers<'_>,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let sr = self.sample_rate;
        let p = &self.params;
        let drive = p.drive.value();
        let mix = p.mix.value();
        let delay_time = p.delay_time.value();
        let feedback = p.feedback.value();
        let reverb_wet = p.reverb_wet.value();
        let low_gain = p.low_gain.value();
        let peak_gain = p.peak_gain.value();
        let high_gain = p.high_gain.value();

        for voice in &mut self.voices {
            voice.shaper.set_drive(drive);
            voice.shaper.set_mix(mix);
            voice.delay.set_delay_seconds(delay_time, sr);
            voice.delay.set_feedback(feedback);
            voice.delay.set_mix(mix);
            voice.reverb.set_wet(reverb_wet);
            voice.low.set_coeffs(BiquadCoeffs::<f32>::design(
                BiquadType::LowShelf,
                sr,
                200.0,
                0.707,
                low_gain,
            ));
            voice.peak.set_coeffs(BiquadCoeffs::<f32>::design(
                BiquadType::Peaking,
                sr,
                1000.0,
                1.0,
                peak_gain,
            ));
            voice.high.set_coeffs(BiquadCoeffs::<f32>::design(
                BiquadType::HighShelf,
                sr,
                8000.0,
                0.707,
                high_gain,
            ));
        }

        for (_, mut block) in buffer.iter_blocks(self.max_block) {
            let channels = block.channels();
            for c in 0..channels {
                if let Some(channel) = block.get_mut(c) {
                    if let Some(voice) = self.voices.get_mut(c) {
                        voice.process(channel);
                    }
                }
            }
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for TptDspPedalboard {
    const CLAP_ID: &'static str = "com.tpt-solutions.pedalboard";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("tpt-dsp Waveshaper → Delay → Reverb → EQ");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect, ClapFeature::Stereo];
}

impl Vst3Plugin for TptDspPedalboard {
    const VST3_CLASS_ID: [u8; 16] = *b"tptdsppdlbrd0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Stereo];
}

nice_export_clap!(TptDspPedalboard);
nice_export_vst3!(TptDspPedalboard);
