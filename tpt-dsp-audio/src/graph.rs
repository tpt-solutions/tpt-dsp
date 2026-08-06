//! Audio graph: sources → effects → sinks.
//!
//! A small, real-time-safe graph abstraction. A [`Source`] generates blocks
//! of samples with no input; an [`AudioNode`] transforms a block in place or
//! from input to output; a [`Sink`] consumes a block. An [`AudioGraph`]
//! bundles one source, a chain of nodes, and one sink and drives them block
//! by block using pre-allocated scratch buffers (no allocation while
//! running).

/// A node that transforms an audio block.
///
/// Implementors read `input` and write `output` (which is the same length).
/// Implementations must be allocation-free on the hot path.
pub trait AudioNode {
    /// Process one block. `output.len()` must equal `input.len()`.
    fn process(&mut self, input: &[f32], output: &mut [f32]);
}

/// A block generator with no input (oscillators, file/stream readers, …).
pub trait Source {
    /// Fill `output` with the next block of samples.
    fn render(&mut self, output: &mut [f32]);
}

/// A block consumer (speakers, recorders, analyser, …).
pub trait Sink {
    /// Consume one block of samples.
    fn consume(&mut self, input: &[f32]);
}

/// A node that passes its input straight through (useful as a placeholder).
pub struct Passthrough;

impl AudioNode for Passthrough {
    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        output.copy_from_slice(input);
    }
}

/// A node wrapping a `FnMut(&[f32], &mut [f32])` closure.
pub struct ClosureNode<F>(pub F);

impl<F: FnMut(&[f32], &mut [f32])> AudioNode for ClosureNode<F> {
    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        (self.0)(input, output)
    }
}

/// A source wrapping a closure that fills blocks.
pub struct ClosureSource<F>(pub F);

impl<F: FnMut(&mut [f32])> Source for ClosureSource<F> {
    fn render(&mut self, output: &mut [f32]) {
        (self.0)(output);
    }
}

/// A sink wrapping a closure that consumes blocks.
pub struct ClosureSink<F>(pub F);

impl<F: FnMut(&[f32])> Sink for ClosureSink<F> {
    fn consume(&mut self, input: &[f32]) {
        (self.0)(input);
    }
}

/// A block-by-block audio processing graph.
///
/// Owns a source, a chain of nodes and a sink. Buffers are allocated once at
/// construction; [`run`](Self::run) and [`tick`](Self::tick) are
/// allocation-free.
pub struct AudioGraph {
    block_size: usize,
    source: Box<dyn Source>,
    nodes: Vec<Box<dyn AudioNode>>,
    sink: Box<dyn Sink>,
    scratch_a: Vec<f32>,
    scratch_b: Vec<f32>,
}

impl AudioGraph {
    /// Build a graph. `source` must produce exactly `block_size` samples per
    /// call.
    pub fn new(
        block_size: usize,
        source: Box<dyn Source>,
        nodes: Vec<Box<dyn AudioNode>>,
        sink: Box<dyn Sink>,
    ) -> Self {
        assert!(block_size > 0, "block size must be nonzero");
        Self {
            block_size,
            source,
            nodes,
            sink,
            scratch_a: vec![0.0; block_size],
            scratch_b: vec![0.0; block_size],
        }
    }

    /// Block size the graph was built for.
    pub fn block_size(&self) -> usize {
        self.block_size
    }

    /// Process a single block: render source → run chain → feed sink.
    ///
    /// Returns a borrowed view of the final block for inspection.
    pub fn tick(&mut self) -> &[f32] {
        self.source.render(&mut self.scratch_a);
        let mut current = &mut self.scratch_a[..];
        let mut next = &mut self.scratch_b[..];
        for node in self.nodes.iter_mut() {
            node.process(current, next);
            core::mem::swap(&mut current, &mut next);
        }
        self.sink.consume(current);
        current
    }

    /// Run the graph for `blocks` blocks.
    pub fn run(&mut self, blocks: usize) {
        for _ in 0..blocks {
            self.tick();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn graph_runs_source_through_gain_node_to_sink() {
        // A DC source at 0.25, a gain node ×2, collected by the sink.
        let collected = Arc::new(Mutex::new(Vec::new()));
        let sink_data = Arc::clone(&collected);
        let mut graph = AudioGraph::new(
            8,
            Box::new(ClosureSource(|out: &mut [f32]| {
                for s in out.iter_mut() {
                    *s = 0.25;
                }
            })),
            vec![Box::new(ClosureNode(|in_: &[f32], out: &mut [f32]| {
                for (o, x) in out.iter_mut().zip(in_.iter()) {
                    *o = x * 2.0;
                }
            }))],
            Box::new(ClosureSink(move |in_: &[f32]| sink_data.lock().unwrap().extend_from_slice(in_))),
        );
        graph.run(4);
        let collected = collected.lock().unwrap();
        assert_eq!(collected.len(), 32);
        assert!(collected.iter().all(|&x| (x - 0.5).abs() < 1e-6));
    }

    #[test]
    fn passthrough_chain_is_transparent() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen_data = Arc::clone(&seen);
        let mut graph = AudioGraph::new(
            4,
            Box::new(ClosureSource(|out: &mut [f32]| {
                for (i, s) in out.iter_mut().enumerate() {
                    *s = i as f32;
                }
            })),
            vec![Box::new(Passthrough), Box::new(Passthrough)],
            Box::new(ClosureSink(move |in_: &[f32]| seen_data.lock().unwrap().extend_from_slice(in_))),
        );
        graph.run(2);
        assert_eq!(
            *seen.lock().unwrap(),
            vec![0.0, 1.0, 2.0, 3.0, 0.0, 1.0, 2.0, 3.0]
        );
    }
}
