//! Diagnostic: what cpal reports for the default output, and whether a Player
//! can actually open a stream. Run on desktop: `cargo run --example audio_probe`.
use cpal::traits::{DeviceTrait, HostTrait};

fn main() {
    let host = cpal::default_host();
    println!("host: {:?}", host.id());

    let dev = match host.default_output_device() {
        Some(d) => d,
        None => {
            println!("NO DEFAULT OUTPUT DEVICE");
            return;
        }
    };
    println!("device: {:?}", dev.name());

    match dev.default_output_config() {
        Ok(c) => println!(
            "default_output_config: {:?} {}ch {}Hz  buffer={:?}",
            c.sample_format(),
            c.channels(),
            c.sample_rate().0,
            c.buffer_size()
        ),
        Err(e) => println!("default_output_config ERR: {e}"),
    }

    println!("supported output configs:");
    if let Ok(cfgs) = dev.supported_output_configs() {
        for c in cfgs {
            println!(
                "  {:?} {}ch {}-{}Hz",
                c.sample_format(),
                c.channels(),
                c.min_sample_rate().0,
                c.max_sample_rate().0
            );
        }
    }

    match composer_core::engine::Player::new() {
        Ok(p) => println!("Player::new() OK  (stream running at {}Hz)", p.sample_rate),
        Err(e) => println!("Player::new() ERR: {e}"),
    }
}
