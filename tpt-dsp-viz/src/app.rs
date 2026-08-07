//! egui application: waterfall spectrogram + live spectrum line.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::colormap::colormap;
use crate::pipeline::SpectrumFrame;
use crossbeam_channel::Receiver;
use egui::Panel;
use egui::{
    Align2, CentralPanel, Color32, FontId, Image, Painter, Pos2, Rect, Sense, Stroke,
    TextureHandle, TextureOptions, Vec2,
};
use tpt_dsp_analysis::{peak_bin, Spectrogram, SpectrumConfig};

/// Number of rows retained in the waterfall (one per analysed frame).
const WATERFALL_ROWS: usize = 256;

/// Lower bound of the dB display range used for colour-mapping / the line plot.
const DISPLAY_FLOOR_DB: f32 = -120.0;
/// Upper bound of the dB display range.
const DISPLAY_CEIL_DB: f32 = 0.0;

/// egui application rendering the waterfall and live spectrum line.
///
/// A producer thread feeds [`SpectrumFrame`]s into [`Self::receiver`]; each
/// frame is appended to [`Self::spectrogram`] (for the waterfall) and kept as
/// [`Self::latest`] (for the spectrum line). The window can be paused, which
/// freezes both displays without stopping the producer.
pub struct VizApp {
    /// Receives analysed frames from the producer thread.
    receiver: Receiver<SpectrumFrame>,
    /// Ring of recent magnitude frames forming the waterfall.
    spectrogram: Spectrogram,
    /// Most recent frame, drawn as the live spectrum line.
    latest: Option<SpectrumFrame>,
    /// When true, incoming frames are dropped and the display is frozen.
    paused: bool,
    /// Human-readable description of the active signal source.
    source_label: String,
    /// Signalled when the app is dropped so the producer thread can exit.
    stop: Arc<AtomicBool>,
    /// Cached waterfall texture, re-coloured in place each frame.
    texture: Option<TextureHandle>,
}

impl VizApp {
    /// Build the app for `config`, receiving frames over `receiver`.
    pub fn new(
        receiver: Receiver<SpectrumFrame>,
        config: &SpectrumConfig,
        source_label: String,
        stop: Arc<AtomicBool>,
    ) -> Self {
        let bins = config.fft_size / 2 + 1;
        Self {
            receiver,
            spectrogram: Spectrogram::new(WATERFALL_ROWS, bins),
            latest: None,
            paused: false,
            source_label,
            stop,
            texture: None,
        }
    }

    /// Drain every pending frame, advancing the displays unless paused.
    fn drain_frames(&mut self) {
        if self.paused {
            // Keep the bounded channel from backing up the producer, but drop.
            while self.receiver.try_recv().is_ok() {}
            return;
        }
        while let Ok(frame) = self.receiver.try_recv() {
            self.spectrogram.push_row(&frame.db);
            self.latest = Some(frame);
        }
    }

    /// (frequency Hz, dB) of the strongest bin in the latest frame.
    fn latest_peak(&self) -> Option<(f32, f32)> {
        let f = self.latest.as_ref()?;
        let bin = peak_bin(&f.db);
        let freq = bin as f32 * f.sample_rate / f.fft_size as f32;
        Some((freq, f.db[bin]))
    }

    /// Normalise a dB value into `[0, 1]` over the display range.
    fn normalize(&self, db: f32) -> f32 {
        let span = (DISPLAY_CEIL_DB - DISPLAY_FLOOR_DB).max(1e-6);
        ((db - DISPLAY_FLOOR_DB) / span).clamp(0.0, 1.0)
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(format!("Source: {}", self.source_label));
            if let Some((freq, db)) = self.latest_peak() {
                ui.label(format!("Peak: {freq:6.1} Hz, {db:6.1} dB"));
            }
            if ui
                .button(if self.paused { "Resume" } else { "Pause" })
                .clicked()
            {
                self.paused = !self.paused;
            }
            ui.label(format!("Rows: {}", self.spectrogram.filled()));
        });
    }

    fn central(&mut self, ui: &mut egui::Ui) {
        let rect = ui.available_rect_before_wrap();
        let (waterfall, spectrum) = split_vertical(rect, 0.66);
        self.draw_waterfall(ui, waterfall);
        self.draw_spectrum_line(ui, spectrum);
    }

    fn draw_waterfall(&mut self, ui: &mut egui::Ui, rect: Rect) {
        ui.allocate_rect(rect, Sense::hover());
        let filled = self.spectrogram.filled();
        if filled == 0 {
            return;
        }
        let cols = self.spectrogram.cols();
        let mut pixels = Vec::with_capacity(filled * cols);
        let mut row = vec![0.0f32; cols];
        for i in 0..filled {
            self.spectrogram.row(i, &mut row);
            for &db in &row {
                pixels.push(colormap(self.normalize(db)));
            }
        }
        let image = egui::ColorImage::new([cols, filled], pixels);
        let options = TextureOptions::NEAREST;
        let tex = match &mut self.texture {
            Some(tex) => tex,
            None => self
                .texture
                .insert(ui.ctx().load_texture("waterfall", image.clone(), options)),
        };
        tex.set(image, options);
        if let Some(tex) = &self.texture {
            let img = Image::new(tex).fit_to_exact_size(rect.size());
            ui.put(rect, img);
        }
    }

    fn draw_spectrum_line(&mut self, ui: &mut egui::Ui, rect: Rect) {
        ui.allocate_rect(rect, Sense::hover());
        let Some(frame) = &self.latest else {
            return;
        };
        let n = frame.db.len();
        if n < 2 {
            return;
        }
        let painter: &Painter = ui.painter();
        let points: Vec<Pos2> = (0..n)
            .map(|i| {
                let x = rect.min.x + (i as f32 / (n - 1) as f32) * rect.width();
                let y = rect.max.y - self.normalize(frame.db[i]) * rect.height();
                Pos2::new(x, y)
            })
            .collect();
        painter.line(points, Stroke::new(1.5, Color32::from_rgb(0, 255, 180)));

        painter.text(
            rect.min,
            Align2::LEFT_BOTTOM,
            format!("{DISPLAY_CEIL_DB:.0} dB"),
            FontId::proportional(10.0),
            Color32::GRAY,
        );
        painter.text(
            Pos2::new(rect.min.x, rect.max.y),
            Align2::LEFT_TOP,
            format!("{DISPLAY_FLOOR_DB:.0} dB"),
            FontId::proportional(10.0),
            Color32::GRAY,
        );
    }
}

impl eframe::App for VizApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.drain_frames();
        Panel::top("top").show(ui, |ui| self.top_bar(ui));
        CentralPanel::default().show(ui, |ui| self.central(ui));
        ui.ctx().request_repaint();
    }
}

impl Drop for VizApp {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// Split `rect` into a top region (`top_fraction` of the height) and the rest.
fn split_vertical(rect: Rect, top_fraction: f32) -> (Rect, Rect) {
    let top_height = rect.height() * top_fraction.clamp(0.0, 1.0);
    let top = Rect::from_min_size(rect.min, Vec2::new(rect.width(), top_height));
    let bottom = Rect::from_min_size(
        Pos2::new(rect.min.x, rect.min.y + top_height),
        Vec2::new(rect.width(), rect.height() - top_height),
    );
    (top, bottom)
}
