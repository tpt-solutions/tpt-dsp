//! `tpt-dsp-control` tour: a PID controller holds a simulated DC motor at a
//! 100 rpm speed setpoint, with output limiting and back-calculation
//! anti-windup. The plant is integrated inline.
//!
//! ```text
//! cargo run -p tpt-dsp-control --example pid_speed_hold
//! ```

use tpt_dsp_control::{AntiWindup, Pid};

fn main() {
    let dt = 0.001f32; // 1 kHz control loop
    let mut pid = Pid::new(2.0, 5.0, 0.05, dt);
    pid.set_setpoint(100.0); // target speed, "rpm"
    pid.set_output_limits(-12.0, 12.0); // drive voltage
    pid.set_anti_windup(AntiWindup::BackCalculation(0.5));

    let plant_gain = 10.0f32; // rpm per volt (100 rpm -> 10 V, within ±12 V)
    let mut speed = 0.0f32;
    let mut max_error = 0.0f32;
    let mut settled = 0usize;

    for step in 0..2_000 {
        let drive = pid.update(speed);
        // First-order plant: speed chases drive * gain with lag.
        speed += (drive * plant_gain - speed) * dt * 10.0;
        let error = (100.0 - speed).abs();
        max_error = max_error.max(error);
        if error < 1.0 {
            settled += 1;
        }
        if step % 500 == 0 {
            println!("step {step}: speed {speed:.2} rpm, drive {drive:.2} V");
        }
    }
    println!("peak error {max_error:.2} rpm; {settled}/2000 samples within 1 rpm of setpoint");
}
