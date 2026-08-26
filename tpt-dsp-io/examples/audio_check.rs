//! Live validation of the built-in audio backends (no external crates).
//!
//! Enumerates devices, then:
//! 1. plays a 1-second 440 Hz sine to the default output,
//! 2. plays a 1-second 523 Hz sine on the first listed output device,
//! 3. captures ~1 second from the default input and reports RMS/peak,
//! 4. captures ~1 second from the first listed input device.
//!
//! Run with: `cargo run -p tpt-dsp-io --example audio_check --features audio`
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.

use std::sync::mpsc;
use std::time::Duration;

use tpt_dsp_io::{
    has_default_input, list_input_devices, list_output_devices, run_input, run_input_on_device,
    run_output, run_output_on_device, AudioError,
};

fn tone(freq: f32, rate: u32) -> impl FnMut(&mut [f32]) + Send + 'static {
    let mut phase = 0.0_f32;
    move |out| {
        let step = 2.0 * std::f32::consts::PI * freq / rate as f32;
        for sample in out.iter_mut() {
            *sample = 0.2 * phase.sin();
            phase += step;
            if phase > 2.0 * std::f32::consts::PI {
                phase -= 2.0 * std::f32::consts::PI;
            }
        }
    }
}

fn play(seconds: u64, freq: f32, device: Option<String>) -> Result<(), AudioError> {
    let (stop_tx, stop_rx) = mpsc::channel();
    let timer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(seconds));
        drop(stop_tx);
    });
    let result = match device {
        Some(name) => {
            println!("  playing on device {name:?}");
            run_output_on_device(&name, 48_000, 256, tone(freq, 48_000), &stop_rx)
        }
        None => run_output(48_000, 256, tone(freq, 48_000), &stop_rx),
    };
    let _ = timer.join();
    result
}

fn capture(label: &str, seconds: u64, device: Option<String>) {
    let (stop_tx, stop_rx) = mpsc::channel();
    let timer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(seconds));
        drop(stop_tx);
    });
    let stats = std::sync::Arc::new(std::sync::Mutex::new((0_usize, 0_usize, 0.0_f64, 0.0_f32)));
    let callback_stats = std::sync::Arc::clone(&stats);
    let callback = move |samples: &[f32], channels: usize| {
        let mut s = callback_stats.lock().expect("stats lock");
        s.0 += samples.len();
        s.1 = s.1.max(channels);
        for &sample in samples {
            s.2 += f64::from(sample) * f64::from(sample);
            s.3 = s.3.max(sample.abs());
        }
    };
    let result = match device {
        Some(name) => {
            println!("  capturing from device {name:?}");
            run_input_on_device(&name, callback, &stop_rx)
        }
        None => run_input(callback, &stop_rx),
    };
    let _ = timer.join();
    match result {
        Ok(()) => {
            let (samples_seen, channels_seen, sum_squares, peak) =
                *stats.lock().expect("stats lock");
            let rms = if samples_seen > 0 {
                (sum_squares / samples_seen as f64).sqrt()
            } else {
                0.0
            };
            println!(
                "{label}: ok - {samples_seen} samples ({channels_seen} ch), rms={rms:.6}, peak={peak:.6}"
            );
        }
        Err(err) => println!("{label}: failed - {err}"),
    }
}

fn main() {
    println!("has_default_input: {}", has_default_input());

    println!("output devices:");
    for name in list_output_devices().unwrap_or_default() {
        println!("  - {name}");
    }
    println!("input devices:");
    for name in list_input_devices().unwrap_or_default() {
        println!("  - {name}");
    }

    print!("default output tone: ");
    match play(1, 440.0, None) {
        Ok(()) => println!("ok"),
        Err(err) => println!("failed - {err}"),
    }

    print!("specific output tone: ");
    let first_output = list_output_devices().unwrap_or_default().into_iter().next();
    match play(1, 523.25, first_output.clone()) {
        Ok(()) => println!("ok"),
        Err(err) => println!("failed - {err}"),
    }

    capture("default input", 1, None);
    let first_input = list_input_devices().unwrap_or_default().into_iter().next();
    capture("specific input", 1, first_input);
}
