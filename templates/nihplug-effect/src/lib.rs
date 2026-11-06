// SPDX-License-Identifier: MIT OR Apache-2.0

//! {{project-name}} — a CLAP and VST3 audio effect built with nice-plug.
//!
//! The plugin is a host of one effect per channel. The effect here is a
//! placeholder gain stage. To make your own plugin, replace [`GainEffect`] with
//! your effect and keep its shape:
//!
//! - allocate every buffer in `new`, and reuse it;
//! - process in place with no allocation, locking or syscalls;
//! - read parameters once per block, not per sample.
//!
//! Add parameters to [`PluginParams`] with a stable `#[id = "…"]`. Hosts store
//! automation by that id, so renaming one breaks saved projects.

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

use std::num::NonZeroU32;
use std::sync::Arc;

use nice_plug::prelude::*;

/// One channel's effect state. The placeholder has none; a real effect keeps
/// its filters, delay lines and scratch buffers here.
#[derive(Debug, Clone)]
pub struct GainEffect {
    /// Linear gain applied to every sample.
    gain: f32,
}

impl GainEffect {
    /// Create the effect. The only place this type allocates, if it ever does.
    pub fn new() -> Self {
        Self { gain: 1.0 }
    }

    /// Process one channel buffer in place. Allocation-free.
    pub fn process(&mut self, buf: &mut [f32]) {
        for x in buf.iter_mut() {
            *x *= self.gain;
        }
    }
}

impl Default for GainEffect {
    fn default() -> Self {
        Self::new()
    }
}

/// Host-automatable parameters.
#[derive(Params)]
pub struct PluginParams {
    /// Output gain, linear.
    #[id = "gain"]
    pub gain: FloatParam,
}

impl Default for PluginParams {
    fn default() -> Self {
        Self {
            gain: FloatParam::new("Gain", 1.0, FloatRange::Linear { min: 0.0, max: 2.0 }),
        }
    }
}

/// The plugin: one [`GainEffect`] per input channel.
pub struct PluginEffect {
    params: Arc<PluginParams>,
    channels: Vec<GainEffect>,
    max_block: usize,
}

impl Default for PluginEffect {
    fn default() -> Self {
        Self {
            params: Arc::new(PluginParams::default()),
            channels: Vec::new(),
            max_block: 1024,
        }
    }
}

impl Plugin for PluginEffect {
    const NAME: &'static str = "{{project-name}}";
    const VENDOR: &'static str = "{{authors}}";
    const URL: &'static str = "https://example.com";
    const EMAIL: &'static str = "you@example.com";
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
        // Everything that allocates happens here, off the audio thread.
        self.max_block = buffer_config.max_buffer_size as usize;
        let channels = layout.main_input_channels.map_or(0, |c| c.get() as usize);
        self.channels = (0..channels).map(|_| GainEffect::new()).collect();
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer<'_>,
        _aux: &mut AuxiliaryBuffers<'_>,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let gain = self.params.gain.value();
        for effect in &mut self.channels {
            effect.gain = gain;
        }
        for (_, mut block) in buffer.iter_blocks(self.max_block) {
            for c in 0..block.channels() {
                if let (Some(channel), Some(effect)) = (block.get_mut(c), self.channels.get_mut(c))
                {
                    effect.process(channel);
                }
            }
        }
        ProcessStatus::Normal
    }
}

impl ClapPlugin for PluginEffect {
    const CLAP_ID: &'static str = "com.example.{{crate_name}}";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("{{project-name}}");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect, ClapFeature::Stereo];
}

impl Vst3Plugin for PluginEffect {
    // Exactly 16 bytes, unique to your plugin. Changing it breaks saved projects.
    const VST3_CLASS_ID: [u8; 16] = *b"tptdspeffect0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Stereo];
}

nice_export_clap!(PluginEffect);
nice_export_vst3!(PluginEffect);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_scales_every_sample_without_allocating() {
        let mut fx = GainEffect::new();
        fx.gain = 0.5;
        let mut buf = [2.0f32; 64];
        fx.process(&mut buf);
        assert!(buf.iter().all(|&s| s == 1.0));
    }
}
