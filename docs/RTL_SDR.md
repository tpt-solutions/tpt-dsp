# RTL-SDR: the supported path is `rtl_tcp`

The framework does not talk to an RTL-SDR dongle over USB. The `rtl-sdr`
feature of `tpt-dsp-io` is a stub that only changes the error a source reports.
The supported path is the `rtl_tcp` server from librtlsdr. It owns the USB
device and streams the I/Q samples over TCP, and `tpt-dsp` reads that stream.
See [DECISIONS.md](DECISIONS.md) for why a USB driver is not built in.

## Run the server

Install librtlsdr (it provides `rtl_tcp`), then start the server on the
machine with the dongle:

```sh
rtl_tcp -a 127.0.0.1 -p 1234
```

## Read it from Rust

`TcpIqSource::connect_rtl_tcp` connects, reads the 12-byte `RTL0` greeting
(magic, tuner type, gain-step count, all big-endian), and returns the samples
as unsigned 8-bit I/Q pairs:

```rust
use tpt_dsp_io::{IqSource, TcpIqSource};
use tpt_dsp_core::Complex32;

let (mut source, info) = TcpIqSource::connect_rtl_tcp("127.0.0.1:1234")?;
println!("tuner {}, {} gain steps", info.tuner, info.gain_count);

let mut block = vec![Complex32::new(0.0, 0.0); 16_384];
let n = source.recv(&mut block)?;
// block[..n] holds the next I/Q samples, scaled to about [-1, 1].
```

Connecting with plain `TcpIqSource::connect(addr, IqFormat::U8)` does not read
the greeting. The first six samples would then be the greeting bytes, so use
`connect_rtl_tcp` for `rtl_tcp`.

## Tune it

The same connection accepts control commands. Each is five bytes on the wire
(an opcode, then a big-endian `u32`), and `rtl_tcp` applies it before the
samples that follow:

```rust
let (mut source, _info) = TcpIqSource::connect_rtl_tcp("127.0.0.1:1234")?;
source.set_center_frequency(433_920_000)?; // 433.92 MHz
source.set_sample_rate(2_048_000)?;
source.set_tuner_gain_tenth_db(197)?;      // 19.7 dB; switches to manual gain
source.set_frequency_correction_ppm(0)?;
source.set_auto_gain()?;                   // back to AGC
```

Settings are not read back: `rtl_tcp` does not report them. Set what you need
after connecting.

## Not supported

- Bias tee, direct sampling, offset tuning, test mode and the other opcodes
  in `RtlTcpCommand`'s protocol. `RtlTcpCommand` is `#[non_exhaustive]`, so
  they can be added without breaking callers.
- A USB driver or `librtlsdr` binding. The `rtl-sdr` feature only changes the
  error that `RtlSdrSource` reports.

## Run the example

The `sdr_pipeline` example decimates the stream, FM-demodulates it, and writes
audio. Against a server:

```sh
cargo run -p tpt-dsp-io --example sdr_pipeline --features tcp -- --tcp 127.0.0.1:1234 --seconds 10 --out audio.wav
```

Without `--tcp` it uses a synthetic FM carrier, so it runs without hardware.

