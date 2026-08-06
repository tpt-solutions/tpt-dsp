//! Single-producer / single-consumer message queue (crossbeam-backed).
//!
//! Requires the `std` feature. This is the recommended way to hand audio
//! blocks or control messages between an audio thread and a UI/control
//! thread: crossbeam's channels are lock-free for the SPSC case and never
//! allocate in `try_*` paths.

use crossbeam_channel::{bounded, unbounded, Receiver, RecvError, SendError, Sender, TryRecvError, TrySendError};

/// A single-producer, single-consumer FIFO channel.
///
/// Cheaply cloneable for both ends; the channel itself is `Send + Sync`.
#[derive(Debug, Clone)]
pub struct SpscQueue<T> {
    sender: Sender<T>,
    receiver: Receiver<T>,
}

impl<T> SpscQueue<T> {
    /// Create a channel with a fixed capacity. `send` blocks when full,
    /// `try_send` returns the value instead.
    pub fn bounded(capacity: usize) -> Self {
        let (sender, receiver) = bounded(capacity);
        Self { sender, receiver }
    }

    /// Create an unbounded channel (prefer bounded for real-time use).
    pub fn unbounded() -> Self {
        let (sender, receiver) = unbounded();
        Self { sender, receiver }
    }

    /// Send a message, blocking until space is available.
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        self.sender.send(value)
    }

    /// Send a message only if capacity is available right now.
    /// This is the real-time-safe call (never blocks, never allocates).
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        self.sender.try_send(value)
    }

    /// Receive a message, blocking until one arrives.
    pub fn recv(&self) -> Result<T, RecvError> {
        self.receiver.recv()
    }

    /// Receive a message without blocking. Real-time-safe.
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        self.receiver.try_recv()
    }

    /// Split into independent producer and consumer halves.
    pub fn split(&self) -> (Producer<T>, Consumer<T>) {
        (Producer { sender: self.sender.clone() }, Consumer { receiver: self.receiver.clone() })
    }
}

/// Producer half of an [`SpscQueue`].
#[derive(Debug, Clone)]
pub struct Producer<T> {
    sender: Sender<T>,
}

impl<T> Producer<T> {
    /// Blocking send.
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        self.sender.send(value)
    }
    /// Non-blocking send (real-time-safe).
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        self.sender.try_send(value)
    }
}

/// Consumer half of an [`SpscQueue`].
#[derive(Debug, Clone)]
pub struct Consumer<T> {
    receiver: Receiver<T>,
}

impl<T> Consumer<T> {
    /// Blocking receive.
    pub fn recv(&self) -> Result<T, RecvError> {
        self.receiver.recv()
    }
    /// Non-blocking receive (real-time-safe).
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        self.receiver.try_recv()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_send_recv() {
        let q = SpscQueue::<u32>::bounded(4);
        assert!(q.try_send(1).is_ok());
        assert!(q.try_send(2).is_ok());
        assert_eq!(q.try_recv(), Ok(1));
        assert_eq!(q.try_recv(), Ok(2));
        assert!(matches!(q.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn bounded_overflow_rejects() {
        let q = SpscQueue::<u32>::bounded(2);
        q.try_send(1).unwrap();
        q.try_send(2).unwrap();
        assert!(matches!(q.try_send(3), Err(TrySendError::Full(3))));
    }

    #[test]
    fn split_halves_work() {
        let q = SpscQueue::<u32>::unbounded();
        let (p, c) = q.split();
        p.send(42).unwrap();
        assert_eq!(c.recv(), Ok(42));
    }

    #[test]
    fn recv_after_drop_is_disconnected() {
        let q = SpscQueue::<u32>::unbounded();
        drop(q);
        // The halves were not split, so both ends were dropped together —
        // nothing to assert beyond that constructing/splitting works.
        let q = SpscQueue::<u32>::unbounded();
        let (p, c) = q.split();
        drop(p);
        assert!(c.recv().is_err());
    }
}
