//! Spectrogram / waterfall generation.
//!
//! Stores the most recent `rows` magnitude frames (each `cols` bins) in a
//! pre-allocated ring. The newest frame is [`push_row`](Self::push_row)ed;
//! [`row`](Self::row) / [`as_image`](Self::as_image) expose the buffer for
//! rendering (e.g. as a column-major image where column 0 is the oldest
//! frame).

/// A ring buffer of magnitude spectra forming a waterfall display.
#[derive(Debug, Clone)]
pub struct Spectrogram {
    rows: usize,
    cols: usize,
    data: Vec<f32>,
    head: usize,
    filled: usize,
}

impl Spectrogram {
    /// Create a spectrogram with room for `rows` frames of `cols` bins.
    ///
    /// # Panics
    ///
    /// Panics if either dimension is zero.
    pub fn new(rows: usize, cols: usize) -> Self {
        assert!(
            rows > 0 && cols > 0,
            "spectrogram dimensions must be positive"
        );
        Self {
            rows,
            cols,
            data: vec![0.0; rows * cols],
            head: 0,
            filled: 0,
        }
    }

    /// Number of frames retained.
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Bins per frame.
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Frames currently stored (`min(rows, frames seen)`).
    pub fn filled(&self) -> usize {
        self.filled
    }

    /// Append one magnitude frame (length must equal `cols`).
    pub fn push_row(&mut self, frame: &[f32]) {
        assert_eq!(frame.len(), self.cols, "frame length mismatch");
        let base = self.head * self.cols;
        self.data[base..base + self.cols].copy_from_slice(frame);
        self.head = (self.head + 1) % self.rows;
        if self.filled < self.rows {
            self.filled += 1;
        }
    }

    /// Read the `i`-th oldest frame (`0` = oldest, `filled-1` = newest) into
    /// `out` (length `cols`).
    pub fn row(&self, i: usize, out: &mut [f32]) {
        assert!(i < self.filled, "row index out of range");
        assert_eq!(out.len(), self.cols, "output length mismatch");
        // The oldest row is one beyond head (mod rows), unless not yet full.
        let oldest = if self.filled < self.rows {
            0
        } else {
            self.head
        };
        let idx = (oldest + i) % self.rows;
        let base = idx * self.cols;
        out.copy_from_slice(&self.data[base..base + self.cols]);
    }

    /// Flatten the spectrogram into a `rows × cols` image where each row is a
    /// frame in chronological order (oldest first). Allocation-free: `out`
    /// must be exactly `rows * cols` long (or `filled * cols` if you pass a
    /// smaller buffer — see [`row`](Self::row)).
    pub fn as_image(&self, out: &mut [f32]) {
        assert!(
            out.len() >= self.filled * self.cols,
            "image buffer too small"
        );
        for i in 0..self.filled {
            let base = i * self.cols;
            self.row(i, &mut out[base..base + self.cols]);
        }
    }

    /// The newest frame.
    pub fn latest(&self, out: &mut [f32]) {
        assert!(self.filled > 0, "no frames stored yet");
        let idx = if self.filled < self.rows {
            self.head - 1
        } else {
            (self.head + self.rows - 1) % self.rows
        };
        let base = idx * self.cols;
        out.copy_from_slice(&self.data[base..base + self.cols]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_chrono_ordered() {
        let mut s = Spectrogram::new(3, 2);
        s.push_row(&[1.0, 2.0]);
        s.push_row(&[3.0, 4.0]);
        s.push_row(&[5.0, 6.0]);
        let mut out = [0.0f32; 2];
        s.row(0, &mut out);
        assert_eq!(out, [1.0, 2.0]);
        s.row(2, &mut out);
        assert_eq!(out, [5.0, 6.0]);
    }

    #[test]
    fn ring_overwrites_oldest() {
        let mut s = Spectrogram::new(2, 1);
        s.push_row(&[1.0]);
        s.push_row(&[2.0]);
        s.push_row(&[3.0]); // overwrites the first
        assert_eq!(s.filled(), 2);
        let mut out = [0.0f32; 1];
        s.row(0, &mut out);
        assert_eq!(out[0], 2.0);
        s.latest(&mut out);
        assert_eq!(out[0], 3.0);
    }

    #[test]
    fn as_image_flattens_in_order() {
        let mut s = Spectrogram::new(2, 2);
        s.push_row(&[1.0, 2.0]);
        s.push_row(&[3.0, 4.0]);
        let mut img = vec![0.0f32; 4];
        s.as_image(&mut img);
        assert_eq!(img, vec![1.0, 2.0, 3.0, 4.0]);
    }
}
