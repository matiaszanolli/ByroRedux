//! Dump the VATS/AP GMST family's authored values from an ESM (#6350).
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example probe_gmst_vats -- <ESM>

use byroredux_plugin::esm::parse_esm;

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: probe_gmst_vats <ESM>"))?;
    let bytes = std::fs::read(&path)?;
    let index = parse_esm(&bytes).expect("parse");
    let mut names: Vec<String> = index
        .game_settings
        .values()
        .map(|s| s.editor_id.clone())
        .filter(|id| {
            id.starts_with("fActionPoints") || id.starts_with("fVATS") || id.starts_with("iVATS")
        })
        .collect();
    names.sort();
    for name in names {
        let value = index
            .game_setting_float(&name)
            .map(|v| format!("{v}"))
            .unwrap_or_else(|| "<non-numeric>".into());
        println!("{name} = {value}");
    }
    Ok(())
}
