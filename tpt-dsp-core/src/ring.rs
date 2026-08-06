//! Lock-free, pre-allocated ring buffers.
//!
//! [`RingBuffer`] is a fixed-capacity FIFO over a caller-supplied buffer.
//! Read/write cursors are tracked with atomics — no locks, no allocation,
//! no `unsafe`. It works in `no_std` contexts and can be moved between
//! threads, or handed to another thread wholesale, for lock-free data
//! transfer. For fine-grained cross-thread SPSC streaming see
//! [`crate::SpscQueue`] (requires `std`).
//!
//! The single "slack" slot scheme means a buffer of length `N` stores at
//! most `N - 1` items, so a full ring is unambiguous (`read == write`).

use core::sync::atomic::{AtomicUsize, Ordering};

/// Read side operations available on a [`RingBuffer`].
pub trait RingRead<F: Copy> {
    /// Number of items currently buffered.
    fn len(&self) -> usize;
    /// `true` when the ring is empty.
    fn is_empty(&self) -> bool;
    /// Remove and return the oldest item, or `None` if empty.
    fn pop(&mut self) -> Option<F>;
    /// Peek the oldest item without removing it.
    fn front(&self) -> Option<F>;
}

/// Write side operations available on a [`RingBuffer`].
pub trait RingWrite<F> {
    /// `true` when the ring has no free slots.
    fn is_full(&self) -> bool;
    /// Append an item, returning it if the ring is full.
    fn push(&mut self, value: F) -> Result<(), F>;
    /// Clear all buffered items.
    fn clear(&mut self);
}

/// A fixed-capacity FIFO ring buffer.
///
/// # Example
///
/// ```
/// use tpt_dsp_core::{RingBuffer, RingWrite, RingRead};
/// let mut storage = [0f32; 4];
/// let mut ring = RingBuffer::new(&mut storage);
/// assert!(ring.push(1.0).is_ok());
/// assert_eq!(ring.pop(), Some(1.0));
/// ```
pub struct RingBuffer<'a, F> {
    data: &'a mut [F],
    capacity: usize,
    read: AtomicUsize,
    write: AtomicUsize,
}

impl<'a, F> RingBuffer<'a, F> {
    /// Wrap a storage slice. Usable capacity is `storage.len() - 1` (one
    /// slot is reserved to distinguish "full" from "empty").
    pub fn new(storage: &'a mut [F]) -> Self {
        assert!(
            storage.len() > 1,
            "ring buffer needs at least 2 slots of storage"
        );
        let capacity = storage.len() - 1;
        Self {
            data: storage,
            capacity,
            read: AtomicUsize::new(0),
            write: AtomicUsize::new(0),
        }
    }

    /// Number of items the ring can hold.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Number of items currently buffered.
    pub fn len(&self) -> usize {
        let write = self.write.load(Ordering::Relaxed);
        let read = self.read.load(Ordering::Relaxed);
        if write >= read {
            write - read
        } else {
            write + self.capacity - read + 1
        }
    }

    /// `true` when the ring is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// `true` when the ring has no free slots.
    #[inline]
    pub fn is_full(&self) -> bool {
        self.len() == self.capacity
    }

    /// Append an item. Returns `Err(value)` (unmodified) when full.
    pub fn push(&mut self, value: F) -> Result<(), F> {
        let write = self.write.load(Ordering::Relaxed);
        let read = self.read.load(Ordering::Relaxed);
        let next = (write + 1) % self.data.len();
        if next == read {
            return Err(value);
        }
        self.data[write] = value;
        self.write.store(next, Ordering::Release);
        Ok(())
    }

    /// Remove and return the oldest item, or `None` if empty.
    pub fn pop(&mut self) -> Option<F>
    where
        F: Copy,
    {
        let read = self.read.load(Ordering::Relaxed);
        let write = self.write.load(Ordering::Acquire);
        if read == write {
            return None;
        }
        let value = self.data[read];
        self.read
            .store((read + 1) % self.data.len(), Ordering::Release);
        Some(value)
    }

    /// Peek the oldest item without removing it.
    pub fn front(&self) -> Option<F>
    where
        F: Copy,
    {
        let read = self.read.load(Ordering::Relaxed);
        let write = self.write.load(Ordering::Acquire);
        if read == write {
            None
        } else {
            Some(self.data[read])
        }
    }

    /// Discard all buffered items.
    pub fn clear(&mut self) {
        self.read
            .store(self.write.load(Ordering::Relaxed), Ordering::Relaxed);
    }
}

impl<'a, F: Copy> RingRead<F> for RingBuffer<'a, F> {
    fn len(&self) -> usize {
        self.len()
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn pop(&mut self) -> Option<F> {
        self.pop()
    }

    fn front(&self) -> Option<F> {
        self.front()
    }
}

impl<'a, F> RingWrite<F> for RingBuffer<'a, F> {
    fn is_full(&self) -> bool {
        self.is_full()
    }

    fn push(&mut self, value: F) -> Result<(), F> {
        self.push(value)
    }

    fn clear(&mut self) {
        self.clear()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pop_roundtrip() {
        let mut storage = [0f32; 4];
        let mut ring = RingBuffer::new(&mut storage);
        assert!(ring.is_empty());
        assert!(ring.push(1.0).is_ok());
        assert!(ring.push(2.0).is_ok());
        assert_eq!(ring.pop(), Some(1.0));
        assert_eq!(ring.pop(), Some(2.0));
        assert_eq!(ring.pop(), None);
        assert!(ring.is_empty());
    }

    #[test]
    fn wraps_around() {
        let mut storage = [0u32; 4];
        let mut ring = RingBuffer::new(&mut storage);
        for i in 0..3 {
            assert!(ring.push(i).is_ok());
        }
        assert!(ring.push(99).is_err(), "ring should be full");
        for i in 0..3 {
            assert_eq!(ring.pop(), Some(i));
        }
        assert!(ring.is_empty());
        // Now write more — exercises index wraparound.
        for i in 10..13 {
            assert!(ring.push(i).is_ok());
        }
        for i in 10..13 {
            assert_eq!(ring.pop(), Some(i));
        }
    }

    #[test]
    fn capacity_is_len_minus_one() {
        let mut storage = [0f64; 5];
        let ring = RingBuffer::new(&mut storage);
        assert_eq!(ring.capacity(), 4);
    }

    #[test]
    fn front_peeks_without_removing() {
        let mut storage = [0f32; 4];
        let mut ring = RingBuffer::new(&mut storage);
        let _ = ring.push(7.0);
        assert_eq!(ring.front(), Some(7.0));
        assert_eq!(ring.len(), 1);
    }

    #[test]
    fn clear_empties() {
        let mut storage = [0f32; 4];
        let mut ring = RingBuffer::new(&mut storage);
        let _ = ring.push(1.0);
        let _ = ring.push(2.0);
        ring.clear();
        assert!(ring.is_empty());
        assert_eq!(ring.pop(), None);
    }
}
