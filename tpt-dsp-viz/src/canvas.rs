//! Software canvas: drawing primitives over a caller-owned pixel buffer.
//!
//! Pixels are `0x00RRGGBB` `u32` values, the format minifb presents directly.
//! Every primitive clips to the buffer, so callers never need to pre-check
//! bounds.

use crate::font::{self, ADVANCE, GLYPH_W};

/// An opaque 8-bit-per-channel colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
}

impl Rgb {
    /// Black, `#000000`.
    pub const BLACK: Rgb = Rgb::new(0, 0, 0);

    /// Build a colour from its red, green and blue channels.
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Packed `0x00RRGGBB` form used by the pixel buffer.
    pub(crate) const fn to_u32(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | self.b as u32
    }
}

/// An axis-aligned pixel rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rect {
    pub(crate) x: usize,
    pub(crate) y: usize,
    pub(crate) w: usize,
    pub(crate) h: usize,
}

impl Rect {
    pub(crate) const fn new(x: usize, y: usize, w: usize, h: usize) -> Self {
        Self { x, y, w, h }
    }

    /// One past the rightmost column.
    pub(crate) const fn right(&self) -> usize {
        self.x + self.w
    }

    /// One past the bottom row.
    pub(crate) const fn bottom(&self) -> usize {
        self.y + self.h
    }

    /// Whether the point `(px, py)` lies inside the rectangle.
    pub(crate) const fn contains(&self, px: usize, py: usize) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }
}

/// A drawing surface borrowing a `width * height` pixel buffer.
pub(crate) struct Canvas<'a> {
    pixels: &'a mut [u32],
    width: usize,
    height: usize,
}

impl<'a> Canvas<'a> {
    /// Wrap `pixels`, which must hold exactly `width * height` entries.
    pub(crate) fn new(pixels: &'a mut [u32], width: usize, height: usize) -> Self {
        debug_assert_eq!(pixels.len(), width * height);
        Self {
            pixels,
            width,
            height,
        }
    }

    /// Fill the whole surface with `c`.
    pub(crate) fn clear(&mut self, c: Rgb) {
        self.pixels.fill(c.to_u32());
    }

    /// Set one pixel; out-of-range coordinates are ignored.
    pub(crate) fn set(&mut self, x: usize, y: usize, c: Rgb) {
        if x < self.width && y < self.height {
            self.pixels[y * self.width + x] = c.to_u32();
        }
    }

    /// Fill `r`, clipped to the surface.
    pub(crate) fn fill_rect(&mut self, r: Rect, c: Rgb) {
        let v = c.to_u32();
        let x1 = r.right().min(self.width);
        let y1 = r.bottom().min(self.height);
        let x0 = r.x.min(x1);
        for y in r.y.min(y1)..y1 {
            let row = y * self.width;
            self.pixels[row + x0..row + x1].fill(v);
        }
    }

    /// Draw a line between two points (Bresenham), clipped to the surface.
    pub(crate) fn line(&mut self, x0: i64, y0: i64, x1: i64, y1: i64, c: Rgb) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let (mut x, mut y) = (x0, y0);
        let mut err = dx + dy;
        loop {
            if x >= 0 && y >= 0 {
                self.set(x as usize, y as usize, c);
            }
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Draw `s` with its top-left corner at `(x, y)`.
    pub(crate) fn text(&mut self, x: usize, y: usize, s: &str, c: Rgb) {
        self.text_clipped(x, y, s, c, usize::MAX);
    }

    /// Like [`Canvas::text`], but stops before any glyph that would reach `max_x`.
    pub(crate) fn text_clipped(&mut self, x: usize, y: usize, s: &str, c: Rgb, max_x: usize) {
        let mut cx = x;
        for ch in s.chars() {
            if cx + GLYPH_W > max_x {
                break;
            }
            let rows = font::glyph(ch);
            for (ry, row) in rows.iter().enumerate() {
                for (col, b) in row.bytes().enumerate().take(GLYPH_W) {
                    if b == b'#' {
                        self.set(cx + col, y + ry, c);
                    }
                }
            }
            cx += ADVANCE;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Rgb = Rgb::new(255, 0, 0);

    #[test]
    fn packs_channels_as_0rgb() {
        assert_eq!(Rgb::new(0x12, 0x34, 0x56).to_u32(), 0x0012_3456);
    }

    #[test]
    fn fill_rect_clips_to_surface() {
        let mut buf = vec![0u32; 4 * 3];
        let mut c = Canvas::new(&mut buf, 4, 3);
        c.fill_rect(Rect::new(2, 1, 10, 10), RED);
        let on: usize = buf.iter().filter(|&&p| p == RED.to_u32()).count();
        assert_eq!(on, 2 * 2); // columns 2..4, rows 1..3
    }

    #[test]
    fn fill_rect_fully_outside_is_a_no_op() {
        let mut buf = vec![0u32; 4 * 4];
        let mut c = Canvas::new(&mut buf, 4, 4);
        c.fill_rect(Rect::new(9, 9, 3, 3), RED);
        assert!(buf.iter().all(|&p| p == 0));
    }

    #[test]
    fn line_hits_both_endpoints() {
        let mut buf = vec![0u32; 8 * 8];
        let mut c = Canvas::new(&mut buf, 8, 8);
        c.line(0, 0, 7, 3, RED);
        assert_eq!(buf[0], RED.to_u32());
        assert_eq!(buf[3 * 8 + 7], RED.to_u32());
    }

    #[test]
    fn line_with_negative_endpoints_does_not_panic() {
        let mut buf = vec![0u32; 4 * 4];
        let mut c = Canvas::new(&mut buf, 4, 4);
        c.line(-5, -5, 10, 10, RED);
    }

    #[test]
    fn text_clipped_stops_at_limit() {
        let mut buf = vec![0u32; 40 * 10];
        let mut c = Canvas::new(&mut buf, 40, 10);
        c.text_clipped(0, 0, "IIIIIIII", RED, 12);
        // Only glyphs starting before x = 12 are drawn; none beyond column 12+.
        let max_x = (0..40)
            .filter(|&x| (0..10).any(|y| buf[y * 40 + x] == RED.to_u32()))
            .max()
            .unwrap_or(0);
        assert!(max_x < 12 + GLYPH_W);
    }
}
