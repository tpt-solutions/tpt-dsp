# tpt-dsp-io

> Pure Rust hardware I/O for the
> [tpt-dsp](https://github.com/tpt-solutions/tpt-dsp) framework.

`tpt-dsp-io` builds on [`tpt-dsp-core`](../tpt-dsp-core) to get raw samples off
real hardware and into your DSP pipeline. It provides allocation-free I/Q byte
parsing for SDR/RF streams, a driver-agnostic source abstraction, a blocking and
an async TCP IQ server, and (behind features) cpal audio output and serial
readers.

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
| `audio`    | `audio`      | cpal-based real-time output stream: `run_output`, `list_output_devices`.                         |
| `serial`   | `serial`     | `SerialReader` — a serial-port byte reader.                                                        |

## Features

| Feature   | Default | Description                                                              |
| --------- | ------- | ------------------------------------------------------------------------ |
| `audio`   | ✗       | cpal audio output.                                                       |
| `serial`  | ✗       | serial-port reader (serialport).                                         |
| `tcp`     | ✗       | async TCP IQ server (tokio).                                             |
| `rtl-sdr` | ✗       | selects the RTL-SDR hardware backend (see [`RtlSdrSource`] docs).        |

The default build is the `iq` + `source` + `tcp` (client) core only — no audio or
serial dependencies.

## Examples

### Parse an I/Q byte stream (RTL-SDR style, 8-bit offset binary)

```rust
use tpt_dsp_io::{iq::{parse_iq, IqFormat}, IqSource};

let mut samples = [tpt_dsp_core::Complex32::new(0.0, 0.0); 512];
let n = parse_iq(IqFormat::U8, &raw_bytes, &mut samples);
let baseband = &samples[..n];
```

### Carry a sample split across two reads

Byte-oriented transports run through [`IqReassembler`], which buffers a partial
sample split across reads instead of discarding it:

```rust
use tpt_dsp_io::iq::{IqReassembler, IqFormat};

let mut reasm = IqReassembler::new(IqFormat::I16Le);
// feed arbitrary byte chunks; `reassemble` returns full Complex32 samples
```

### Swap the source, keep the pipeline

```rust
use tpt_dsp_io::{IqSource, SyntheticIqSource, TcpIqSource};

fn run(mut src: impl IqSource) {
    let mut buf = [tpt_dsp_core::Complex32::new(0.0, 0.0); 1024];
    while let Ok(n) = src.recv(&mut buf) {
        // ... your DSP ...
    }
}

run(SyntheticIqSource::tone(48_000.0, 1_000.0)); // tests
run(TcpIqSource::connect("127.0.0.1:1234").unwrap()); // live
```

See `examples/sdr_pipeline.rs` for decimation and FM demodulation on top of a
source.

## License

Dual licensed under **MIT** and **Apache-2.0**. © TPT Solutions.
