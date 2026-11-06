#![no_main]

// Feeds arbitrary bytes to the WAV reader. Any input may be rejected, but none
// may panic, allocate without bound, or read past the declared chunk sizes.

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    if let Ok(wav) = tpt_dsp_io::read_wav_f32_reader(Cursor::new(data)) {
        // A successful decode must yield whole frames only.
        let channels = usize::from(wav.spec.channels.max(1));
        assert_eq!(wav.interleaved.len() % channels, 0);
    }
});
