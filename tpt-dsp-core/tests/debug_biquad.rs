use tpt_dsp_core::{Biquad, BiquadType};
#[test]
fn debug() {
    let mut f = Biquad::<f64>::design(BiquadType::AllPass, 48_000.0, 500.0, 1.0, 0.0);
    let mut amp_all = 0.0f64;
    let mut amp_settled = 0.0f64;
    for i in 0..10_000 {
        let x = (i as f64 * 1_000.0 * std::f64::consts::TAU / 48_000.0).sin();
        let y = f.tick(x).abs();
        amp_all = amp_all.max(y);
        if i > 4_000 {
            amp_settled = amp_settled.max(y);
        }
    }
    eprintln!("amp over all: {amp_all}");
    eprintln!("amp after settle: {amp_settled}");
}
