//! Round-trip diagnostic for #591 / FO4-DIM6-06 — runs the full
//! `parse_esm` pipeline and reports which named FO4 NPCs ended up with
//! a populated `face_morphs` block.
//!
//! Usage: `cargo run -p byroredux-plugin --example check_face_morphs --release -- <Fallout4.esm>`

use byroredux_plugin::esm::records::parse_esm;

const NAMED: &[&str] = &[
    "Piper",
    "Hancock",
    "Cait",
    "Codsworth",
    "Curie",
    "Valentine",
    "Preston",
    "MQ101Kellogg",
    "DeaconBOS",
    "DanseBoS",
];

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: <ESM>"))?;
    let bytes = std::fs::read(&path)?;
    println!(
        "Parsing {} ({:.1} MB)…",
        path,
        bytes.len() as f64 / 1_048_576.0
    );

    let idx = parse_esm(&bytes)?;

    let mut named_total = 0usize;
    let mut named_with_morphs = 0usize;
    let all_total = idx.npcs.len();
    let mut all_with_morphs = 0usize;

    for (_, n) in idx.npcs.iter() {
        if n.face_morphs.is_some() {
            all_with_morphs += 1;
        }
        let edid = n.editor_id.as_str();
        if NAMED.iter().any(|p| edid.contains(p)) {
            named_total += 1;
            if let Some(fm) = &n.face_morphs {
                named_with_morphs += 1;
                println!(
                    "  {:08X} {:<40} morphs={:>3} sliders={:>3} hclf={} pnam={}",
                    n.form_id,
                    edid,
                    fm.morphs.len(),
                    fm.slider_keys.len(),
                    fm.hair_color
                        .map(|f| format!("{f:08X}"))
                        .unwrap_or_else(|| "-".to_string()),
                    fm.head_parts.len(),
                );
            }
        }
    }

    println!();
    println!(
        "named:  {named_with_morphs}/{named_total} hit (companion / Kellogg-prefixed records)"
    );
    println!("global: {all_with_morphs}/{all_total} NPC records carry face_morphs");
    Ok(())
}
