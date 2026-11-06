//! Visualizer state, layout and drawing: waterfall spectrogram + live spectrum line.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::canvas::{Canvas, Rect, Rgb};
use crate::colormap::colormap;
use crate::font::{self, ADVANCE, GLYPH_H};
use crate::pipeline::SpectrumFrame;
use crossbeam_channel::Receiver;
use tpt_dsp_analysis::{peak_bin, Spectrogram, SpectrumConfig};

/// Number of rows retained in the waterfall (one per analysed frame).
const WATERFALL_ROWS: usize = 256;

/// Lower bound of the dB display range used for colour-mapping / the line plot.
const DISPLAY_FLOOR_DB: f32 = -120.0;
/// Upper bound of the dB display range.
const DISPLAY_CEIL_DB: f32 = 0.0;

/// Height of the top status bar in pixels.
const BAR_HEIGHT: usize = 22;
/// Horizontal padding inside the status bar and around the button.
const PAD: usize = 8;
/// Pause/resume button size in pixels.
const BUTTON_W: usize = 72;
const BUTTON_H: usize = GLYPH_H + 6;
/// Share of the body height given to the waterfall (the rest is the spectrum).
const WATERFALL_PERCENT: usize = 66;

const BACKGROUND: Rgb = Rgb::new(18, 18, 22);
const BAR_BG: Rgb = Rgb::new(34, 34, 40);
const BUTTON_BG: Rgb = Rgb::new(64, 64, 74);
const TEXT: Rgb = Rgb::new(224, 224, 224);
const MUTED: Rgb = Rgb::new(150, 150, 150);
const SPECTRUM_LINE: Rgb = Rgb::new(0, 255, 180);

/// Screen regions for one window size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Layout {
    pub(crate) bar: Rect,
    pub(crate) waterfall: Rect,
    pub(crate) spectrum: Rect,
    pub(crate) button: Rect,
}

/// Compute the regions for a `width x height` window.
pub(crate) fn layout(width: usize, height: usize) -> Layout {
    let bar_h = BAR_HEIGHT.min(height);
    let body_h = height - bar_h;
    let split = body_h * WATERFALL_PERCENT / 100;
    let button_w = BUTTON_W.min(width);
    Layout {
        bar: Rect::new(0, 0, width, bar_h),
        waterfall: Rect::new(0, bar_h, width, split),
        spectrum: Rect::new(0, bar_h + split, width, body_h - split),
        button: Rect::new(
            width.saturating_sub(button_w + PAD),
            bar_h.saturating_sub(BUTTON_H) / 2,
            button_w,
            BUTTON_H.min(bar_h),
        ),
    }
}

/// Normalise a dB value into `[0, 1]` over the display range.
fn normalize(db: f32) -> f32 {
    let span = (DISPLAY_CEIL_DB - DISPLAY_FLOOR_DB).max(1e-6);
    ((db - DISPLAY_FLOOR_DB) / span).clamp(0.0, 1.0)
}

/// The waterfall and spectrum-line visualizer.
///
/// A producer thread feeds [`SpectrumFrame`]s into the receiver; each frame is
/// appended to the spectrogram (for the waterfall) and kept as the latest frame
/// (for the spectrum line). The display can be paused, which freezes both views
/// without stopping the producer. Drive it with [`crate::run_app`], or call
/// [`VizApp::update`], [`VizApp::on_click`] and [`VizApp::draw_into`] from your
/// own loop.
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
    /// Scratch: one spectrogram row in dB, reused every frame.
    row_scratch: Vec<f32>,
    /// Scratch: colour-mapped waterfall pixels (`filled * cols`), reused every frame.
    waterfall_pixels: Vec<Rgb>,
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
            row_scratch: vec![0.0; bins],
            waterfall_pixels: Vec::with_capacity(WATERFALL_ROWS * bins),
        }
    }

    /// Whether the display is currently paused.
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Toggle the paused state.
    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    /// Take in every pending frame. Call once per UI tick.
    ///
    /// While paused, frames are discarded so the bounded channel does not back
    /// up the producer.
    pub fn update(&mut self) {
        if self.paused {
            while self.receiver.try_recv().is_ok() {}
            return;
        }
        while let Ok(frame) = self.receiver.try_recv() {
            self.spectrogram.push_row(&frame.db);
            self.latest = Some(frame);
        }
    }

    /// Handle a primary-button press at `(x, y)` in a `width x height` window.
    ///
    /// Presses on the pause/resume button toggle the paused state.
    pub fn on_click(&mut self, x: usize, y: usize, width: usize, height: usize) {
        if layout(width, height).button.contains(x, y) {
            self.toggle_pause();
        }
    }

    /// Draw the whole display into `pixels`, a `0x00RRGGBB` buffer of
    /// `width * height` entries.
    ///
    /// # Panics
    ///
    /// Panics if `pixels.len() != width * height`.
    pub fn draw_into(&mut self, pixels: &mut [u32], width: usize, height: usize) {
        assert_eq!(
            pixels.len(),
            width * height,
            "pixel buffer must hold width * height entries"
        );
        let mut canvas = Canvas::new(pixels, width, height);
        canvas.clear(BACKGROUND);
        let lay = layout(width, height);
        self.draw_top_bar(&mut canvas, &lay);
        self.draw_waterfall(&mut canvas, lay.waterfall);
        self.draw_spectrum(&mut canvas, lay.spectrum);
    }

    /// (frequency Hz, dB) of the strongest bin in the latest frame.
    fn latest_peak(&self) -> Option<(f32, f32)> {
        let f = self.latest.as_ref()?;
        let bin = peak_bin(&f.db);
        let freq = bin as f32 * f.sample_rate / f.fft_size as f32;
        Some((freq, f.db[bin]))
    }

    fn draw_top_bar(&self, canvas: &mut Canvas<'_>, lay: &Layout) {
        canvas.fill_rect(lay.bar, BAR_BG);
        let text_y = lay.bar.y + lay.bar.h.saturating_sub(GLYPH_H) / 2;
        let limit = lay.button.x.saturating_sub(PAD);

        let mut x = lay.bar.x + PAD;
        let source = format!("Source: {}", self.source_label);
        canvas.text_clipped(x, text_y, &source, TEXT, limit);
        x += font::text_width(&source) + 3 * ADVANCE;

        if let Some((freq, db)) = self.latest_peak() {
            let peak = format!("Peak: {freq:6.1} Hz, {db:6.1} dB");
            canvas.text_clipped(x, text_y, &peak, TEXT, limit);
            x += font::text_width(&peak) + 3 * ADVANCE;
        }

        let rows = format!("Rows: {}", self.spectrogram.filled());
        canvas.text_clipped(x, text_y, &rows, MUTED, limit);

        canvas.fill_rect(lay.button, BUTTON_BG);
        let label = if self.paused { "Resume" } else { "Pause" };
        let label_x = lay.button.x + lay.button.w.saturating_sub(font::text_width(label)) / 2;
        let label_y = lay.button.y + lay.button.h.saturating_sub(GLYPH_H) / 2;
        canvas.text_clipped(label_x, label_y, label, TEXT, lay.button.right());
    }

    fn draw_waterfall(&mut self, canvas: &mut Canvas<'_>, rect: Rect) {
        canvas.fill_rect(rect, Rgb::BLACK);
        let filled = self.spectrogram.filled();
        if filled == 0 || rect.w == 0 || rect.h == 0 {
            return;
        }
        let cols = self.spectrogram.cols();

        // Colour-map every stored row once. Row 0 is the oldest, so it lands at
        // the top of the rectangle and the newest row sits at the bottom.
        self.waterfall_pixels.clear();
        for i in 0..filled {
            self.spectrogram.row(i, &mut self.row_scratch);
            self.waterfall_pixels
                .extend(self.row_scratch.iter().map(|&db| colormap(normalize(db))));
        }

        // Nearest-neighbour scaling from (cols x filled) onto the rectangle.
        for dy in 0..rect.h {
            let row = dy * filled / rect.h;
            let src = &self.waterfall_pixels[row * cols..(row + 1) * cols];
            for dx in 0..rect.w {
                canvas.set(rect.x + dx, rect.y + dy, src[dx * cols / rect.w]);
            }
        }
    }

    fn draw_spectrum(&self, canvas: &mut Canvas<'_>, rect: Rect) {
        canvas.fill_rect(rect, Rgb::BLACK);
        let Some(frame) = &self.latest else {
            return;
        };
        let n = frame.db.len();
        if n < 2 || rect.w < 2 || rect.h < 2 {
            return;
        }

        let mut prev: Option<(i64, i64)> = None;
        for (i, &db) in frame.db.iter().enumerate() {
            let x = rect.x + i * (rect.w - 1) / (n - 1);
            let lift = (normalize(db) * (rect.h - 1) as f32).round() as usize;
            let y = rect.bottom() - 1 - lift;
            let point = (x as i64, y as i64);
            if let Some((px, py)) = prev {
                canvas.line(px, py, point.0, point.1, SPECTRUM_LINE);
            }
            prev = Some(point);
        }

        let muted_text_h = GLYPH_H + 2;
        canvas.text(
            rect.x + PAD / 2,
            rect.y + 2,
            &format!("{DISPLAY_CEIL_DB:.0} dB"),
            MUTED,
        );
        canvas.text(
            rect.x + PAD / 2,
            rect.bottom().saturating_sub(muted_text_h),
            &format!("{DISPLAY_FLOOR_DB:.0} dB"),
            MUTED,
        );
    }
}

impl Drop for VizApp {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::bounded;

    fn app() -> VizApp {
        let (_tx, rx) = bounded(1);
        VizApp::new(
            rx,
            &SpectrumConfig::default(),
            "test".to_string(),
            Arc::new(AtomicBool::new(false)),
        )
    }

    #[test]
    fn button_click_toggles_pause_and_other_clicks_do_not() {
        let (w, h) = (800, 600);
        let mut app = app();
        let b = layout(w, h).button;

        app.on_click(b.x + 1, b.y + 1, w, h);
        assert!(app.is_paused());
        app.on_click(b.x + 1, b.y + 1, w, h);
        assert!(!app.is_paused());

        // The waterfall and spectrum are not clickable.
        let lay = layout(w, h);
        app.on_click(lay.waterfall.x + 5, lay.waterfall.y + 5, w, h);
        app.on_click(lay.spectrum.x + 5, lay.spectrum.y + 5, w, h);
        assert!(!app.is_paused());
    }

    #[test]
    fn layout_regions_fit_inside_the_window() {
        for (w, h) in [(1024, 720), (800, 560), (40, 10), (1, 1), (0, 0)] {
            let lay = layout(w, h);
            for r in [lay.bar, lay.waterfall, lay.spectrum, lay.button] {
                assert!(r.right() <= w, "{r:?} overflows width {w}");
                assert!(r.bottom() <= h, "{r:?} overflows height {h}");
            }
            // The body regions tile the area under the bar.
            assert_eq!(lay.waterfall.h + lay.spectrum.h + lay.bar.h, h);
        }
    }

    #[test]
    fn draws_into_tiny_and_empty_buffers_without_panicking() {
        let mut app = app();
        for (w, h) in [(0, 0), (1, 1), (3, 2), (40, 10)] {
            let mut pixels = vec![0u32; w * h];
            app.draw_into(&mut pixels, w, h);
        }
    }

    #[test]
    #[should_panic(expected = "pixel buffer must hold")]
    fn mismatched_buffer_is_rejected() {
        let mut app = app();
        let mut pixels = vec![0u32; 10];
        app.draw_into(&mut pixels, 4, 4);
    }
}
