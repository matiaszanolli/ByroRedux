//! Per-record dump of the WATR fields that decide river motion: editor ID,
//! NAM0 linear velocity, NAM1 angular velocity, the parsed wind/normal-layer
//! directions and speeds, and the Skyrim SE NAM5 flow-normal texture.
//!
//! Evidence harness for WATAL W2: cross-checks decoded directions against
//! direction-named records (`RiverWaterFlowNE`, `CreekWaterFlowSW`, …).
//!
//! Usage:
//!   cargo run --release -p byroredux-plugin --example watr_flow_dump -- <ESM> [EDID_SUBSTR]

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let esm_path = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: watr_flow_dump ESM [EDID_SUBSTR]"))?;
    let needle = args.next().unwrap_or_default().to_ascii_lowercase();

    let bytes = std::fs::read(&esm_path)?;
    let index = byroredux_plugin::esm::records::parse_esm(&bytes)?;

    let mut records: Vec<_> = index
        .waters
        .values()
        .filter(|w| needle.is_empty() || w.editor_id.to_ascii_lowercase().contains(&needle))
        .collect();
    records.sort_by(|a, b| a.editor_id.cmp(&b.editor_id));
    for w in records {
        let p = &w.params;
        println!(
            "{:08X} {:<28} nam0={:?} nam1={:?} wind_dir={:.3} wind_speed={:.3} layer_dirs={:?} layer_speeds={:?} nam5={:?}",
            w.form_id,
            w.editor_id,
            w.linear_velocity,
            p.angular_velocity,
            p.wind_direction,
            p.wind_speed,
            p.noise_wind_directions,
            p.noise_wind_speeds,
            w.flow_noise_texture_path,
        );
    }
    Ok(())
}
