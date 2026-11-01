# tpt-dsp-io

> Pure Rust hardware I/O for the
> [tpt-dsp](https://github.com/tpt-solutions/tpt-dsp) framework.

`tpt-dsp-io` builds on [`tpt-dsp-core`](../tpt-dsp-core) to get raw samples off
real hardware and into your DSP pipeline. It provides allocation-free I/Q byte
parsing for SDR/RF streams, a driver-agnostic source abstraction, a blocking TCP
IQ source and an async TCP IQ server, a WAV reader and writer, and (behind
features) built-in audio I/O and a serial reader. It has no external
dependencies unless you enable the `serial` or `tcp` feature.

Everything that produces baseband implements the [`IqSource`] trait, so a receive
chain is written once against `recv` and re-targeted simply by swapping the
source.

## Modules

| Module     | Feature      | Description                                                                                       |
| ---------- | ------------ | ------------------------------------------------------------------------------------------------- |
| `iq`       | always       | Parse raw interleaved I/Q byte streams into [`Complex32`] samples. Allocation-free.               |
| `source`   | always       | The [`IqSource`] trait plus [`SyntheticIqSource`], an in-memory generator for tests/examples.     |
| `tcp`      | always (client) / `tcp` (server) | [`TcpIqSource`] (blocking source over any reader) and the async `serve_iq` server.      |
| `rtlsdr`   | always       | [`RtlSdrSource`] + [`RtlSdrConfig`]; a documented stub unless a driver is wired in behind `rtl-sdr`. |
| `wav`      | always       | RIFF/WAVE reader and writer, normalised to `f32`: `read_wav_f32_path`, `write_wav_f32_path`.     |
| `audio`    | `audio`      | Built-in real-time audio I/O: `run_output`, `run_input`, `list_output_devices`, `list_input_devices`. |
| `serial`   | `serial`     | `SerialReader` — a serial-port byte reader.                                                        |

## Features

| Feature   | Default | Description                                                              |
| --------- | ------- | ------------------------------------------------------------------------ |
| `audio`   | ✗       | Built-in audio I/O (WASAPI on Windows, ALSA on Linux, CoreAudio on macOS). |
| `serial`  | ✗       | Serial-port reader (uses `serialport`).                                  |
| `tcp`     | ✗       | Async TCP IQ server, `serve_iq` (uses `tokio`).                          |
| `rtl-sdr` | ✗       | Selects the RTL-SDR hardware backend (see [`RtlSdrSource`] docs).        |

The default build is the `iq`, `source`, `wav` and TCP-client core only, with no
audio or serial dependencies.

## Examples

### Parse an I/Q byte stream (RTL-SDR style, 8-bit offset binary)

```rust
use tpt_dsp_io::{parse_iq, IqFormat};

let mut samples = [tpt_dsp_core::Complex32::new(0.0, 0.0); 512];
let n = parse_iq(IqFormat::U8, &raw_bytes, &mut samples);
let baseband = &samples[..n];
```

### Carry a sample split across two reads

Byte-oriented transports run through [`IqReassembler`], which keeps a partial
sample from one read and completes it with the next, instead of discarding it.
`push` reports how many samples were written and how many input bytes were taken:

```rust
use tpt_dsp_io::{IqFormat, IqReassembler};

let mut reasm = IqReassembler::new(IqFormat::I16Le);
let mut out = [tpt_dsp_core::Complex32::new(0.0, 0.0); 256];
let mut chunk: &[u8] = &read_buf[..n_read];

while !chunk.is_empty() {
    let r = reasm.push(chunk, &mut out);
    handle(&out[..r.samples]);
    chunk = &chunk[r.consumed..]; // offer the rest again after draining `out`
}
```

### Swap the source, keep the pipeline

```rust
use tpt_dsp_io::{IqSource, SyntheticIqSource, TcpIqSource, IqFormat};

fn run(mut src: impl IqSource) {
    let mut buf = [tpt_dsp_core::Complex32::new(0.0, 0.0); 1024];
    while let Ok(n) = src.recv(&mut buf) {
        // ... your DSP ...
    }
}

run(SyntheticIqSource::fm_tone(48_000.0, 1_000.0, 5_000.0)); // tests
run(TcpIqSource::connect("127.0.0.1:1234", IqFormat::U8).unwrap()); // live
```

See `examples/sdr_pipeline.rs` for decimation and FM demodulation on top of a
source.

## License

Dual licensed under **MIT** and **Apache-2.0**. © TPT Solutions.
