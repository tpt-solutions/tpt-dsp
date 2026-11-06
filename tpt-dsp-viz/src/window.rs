//! Window and event loop: presents [`VizApp`] frames through minifb.
//!
//! minifb owns the native window, input and presentation (GDI on Windows,
//! X11/Wayland elsewhere). All drawing happens in this crate's own software
//! canvas, so the only per-frame work here is copying input into the app and
//! handing the finished pixel buffer to the window.

use std::fmt;

use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};

use crate::app::VizApp;

/// Errors from opening or driving the visualizer window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The native window could not be created or presented to.
    Window(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Window(msg) => write!(f, "window error: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

/// Result type returned by the window entry points.
pub type Result<T = ()> = std::result::Result<T, Error>;

/// Open a `size.0 x size.1` window titled `title` and drive `app` until it is
/// closed (window close button or Escape).
///
/// Left-clicking the pause button, or pressing `P` or Space, toggles pause.
/// Dropping `app` on return signals its producer thread to stop.
///
/// # Errors
///
/// Returns [`Error::Window`] if the window cannot be created or a frame cannot
/// be presented.
pub fn run_app(mut app: VizApp, title: &str, size: (usize, usize)) -> Result {
    let (w0, h0) = size;
    let mut window = Window::new(
        title,
        w0.max(1),
        h0.max(1),
        WindowOptions {
            resize: true,
            ..WindowOptions::default()
        },
    )
    .map_err(|e| Error::Window(e.to_string()))?;
    window.set_target_fps(60);

    let mut pixels: Vec<u32> = Vec::new();
    let mut prev_button_down = false;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let (w, h) = window.get_size();
        if w == 0 || h == 0 {
            // Minimised: nothing to draw, keep pumping events.
            window.update();
            continue;
        }
        if pixels.len() != w * h {
            pixels.resize(w * h, 0);
        }

        app.update();

        let button_down = window.get_mouse_down(MouseButton::Left);
        if button_down && !prev_button_down {
            if let Some((mx, my)) = window.get_mouse_pos(MouseMode::Clamp) {
                app.on_click(mx.max(0.0) as usize, my.max(0.0) as usize, w, h);
            }
        }
        prev_button_down = button_down;

        for key in window.get_keys_pressed(KeyRepeat::No) {
            if matches!(key, Key::P | Key::Space) {
                app.toggle_pause();
            }
        }

        app.draw_into(&mut pixels, w, h);
        window
            .update_with_buffer(&pixels, w, h)
            .map_err(|e| Error::Window(e.to_string()))?;
    }
    Ok(())
}
