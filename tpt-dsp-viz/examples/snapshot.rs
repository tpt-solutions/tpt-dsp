// SPDX-License-Identifier: MIT OR Apache-2.0

//! Render one visualizer frame to a PNG file, without opening a window.
//!
//! Useful for checking the software renderer and font on a headless machine:
//!
//! ```text
//! cargo run -p tpt-dsp-viz --example snapshot -- out.png 1024 720
//! ```

use std::fs::File;
use std::io::BufWriter;

use crossbeam_channel::bounded;
use tpt_dsp_analysis::{RealtimeSpectrumAnalyzer, SpectrumConfig};
use tpt_dsp_viz::{analyze_block, SyntheticGenerator, VizApp};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).map_or("snapshot.png", String::as_str);
    let width: usize = args.get(2).map_or(Ok(1024), |s| s.parse())?;
    let height: usize = args.get(3).map_or(Ok(720), |s| s.parse())?;

    let config = SpectrumConfig::default();
    let (tx, rx) = bounded(512);
    let mut analyzer = RealtimeSpectrumAnalyzer::new(config);
    let mut generator = SyntheticGenerator::new(config.sample_rate, config.fft_size);
    for _ in 0..300 {
        let block = generator.next_block();
        tx.send(analyze_block(&mut analyzer, &block))?;
    }

    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let mut app = VizApp::new(rx, &config, "snapshot (synthetic)".to_string(), stop);
    app.update();

    let mut pixels = vec![0u32; width * height];
    app.draw_into(&mut pixels, width, height);

    let mut rgb = Vec::with_capacity(width * height * 3);
    for p in &pixels {
        rgb.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
    }
    let mut encoder = png::Encoder::new(
        BufWriter::new(File::create(path)?),
        width as u32,
        height as u32,
    );
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&rgb)?;
    println!("wrote {path} ({width}x{height})");
    Ok(())
}
