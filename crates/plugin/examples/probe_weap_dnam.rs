//! Spike probe for FO3/FNV `WEAP` `DNAM` float slots (#6350, 1b).
//!
//! FO3 authors no `VATS` sub-record, so its AP cost and Base VATS
//! To-Hit Chance are read from nowhere today — the shared FO3/FNV
//! `DNAM` arm decodes `anim_type` (@0) and `min_spread` (@16) and
//! *discards* the @20 f32. This probe names the discarded slots from
//! the corpus instead of a remembered layout: per-offset f32 value
//! distributions plus samples, with FNV's `VATS` sub-record (AP cost
//! @12, #3324) printed alongside so shared-weapon correlation can
//! confirm an AP column when one exists.
//!
//! #6350 follow-up: this probe absorbed the deleted `probe_weap_align.rs`'s
//! watchlist (parsed damage/clip/crit beside the raw columns) — one
//! DNAM probe, not two.
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example probe_weap_dnam -- <ESM>

use std::collections::BTreeMap;

use byroredux_plugin::esm::parse_esm;
use byroredux_plugin::esm::reader::EsmReader;

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: probe_weap_dnam <ESM>"))?;
    let bytes = std::fs::read(&path)?;

    /// One DNAM sample row: weapon EDID, the raw DNAM bytes, and the FNV
    /// `VATS` sub-record bytes when the weapon authors one.
    type DnamRow = (String, Vec<u8>, Option<Vec<u8>>);

    let mut sizes: BTreeMap<usize, usize> = BTreeMap::new();
    let mut dnam_by_size: BTreeMap<usize, Vec<DnamRow>> = BTreeMap::new();

    let mut reader = EsmReader::new(&bytes);
    let _ = reader.read_file_header()?;
    reader.for_each_record(bytes.len(), Some(b"WEAP"), |_header, subs| {
        let zstring = |d: &[u8]| String::from_utf8_lossy(d).trim_end_matches('\0').to_string();
        let mut edid = String::new();
        let mut dnam: Option<Vec<u8>> = None;
        let mut vats: Option<Vec<u8>> = None;
        for sub in &subs {
            match &sub.sub_type {
                b"EDID" => edid = zstring(&sub.data),
                b"DNAM" => dnam = Some(sub.data.clone()),
                b"VATS" => vats = Some(sub.data.clone()),
                _ => {}
            }
        }
        if let Some(d) = dnam {
            *sizes.entry(d.len()).or_default() += 1;
            let bucket = dnam_by_size.entry(d.len()).or_default();
            if bucket.len() < 400 {
                bucket.push((edid, d, vats));
            }
        }
    })?;

    // Correlation table: for every weapon whose VATS sub-record authors
    // an AP override (#3324 layout: f32 @12), line it up against the
    // DNAM columns at @28 (base to-hit) and @68 (the standard AP cost)
    // plus the parser's own decode of the same record.
    let parsed = parse_esm(&bytes).expect("parse");
    println!("  correlation: formid | edid | parsed dmg | dnam@28 | dnam@68 | vats_ap_override");
    let mut reader = EsmReader::new(&bytes);
    let _ = reader.read_file_header()?;
    let mut printed = 0usize;
    reader.for_each_record(bytes.len(), Some(b"WEAP"), |header, subs| {
        if printed >= 30 {
            return;
        }
        let zstring = |d: &[u8]| String::from_utf8_lossy(d).trim_end_matches('\0').to_string();
        let mut edid = String::new();
        let mut dnam: Option<Vec<u8>> = None;
        let mut vats: Option<Vec<u8>> = None;
        for sub in subs {
            match &sub.sub_type {
                b"EDID" => edid = zstring(&sub.data),
                b"DNAM" => dnam = Some(sub.data.clone()),
                b"VATS" => vats = Some(sub.data.clone()),
                _ => {}
            }
        }
        let f32_at = |d: &[u8], o: usize| {
            d.get(o..o + 4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        };
        let vats_ap = vats.as_deref().and_then(|v| f32_at(v, 12)).unwrap_or(0.0);
        if vats_ap > 0.0 {
            if let Some(d) = dnam {
                let parsed_damage = parsed
                    .items
                    .get(&header.form_id)
                    .map(|item| match &item.kind {
                        byroredux_plugin::esm::records::ItemKind::Weapon { damage, .. } => {
                            format!("{damage:.0}")
                        }
                        _ => "-".to_string(),
                    })
                    .unwrap_or_else(|| "-".into());
                let at28 = f32_at(&d, 28);
                let at68 = f32_at(&d, 68);
                println!(
                    "    {:08X} {edid:<30} {parsed_damage:>6} {:>7} {:>7} {vats_ap:>14.0}",
                    header.form_id,
                    at28.map(|v| format!("{v:.0}")).unwrap_or("-".into()),
                    at68.map(|v| format!("{v:.0}")).unwrap_or("-".into()),
                );
                printed += 1;
            }
        }
    })?;

    println!("{path}");
    println!("  DNAM sizes: {sizes:?}");
    let mut classes: Vec<_> = dnam_by_size.iter().collect();
    classes.sort_by_key(|e| std::cmp::Reverse(e.1.len()));
    for class in classes.iter().take(2) {
        let (size, rows) = (class.0, class.1);
        println!("  DNAM size class {size}B ({} samples):", rows.len());
        for offset in (0..*size).step_by(4) {
            let mut f32s: BTreeMap<String, usize> = BTreeMap::new();
            let mut integral = 0usize;
            let mut total = 0usize;
            for (_, d, _) in rows.iter() {
                if let Some(b) = d.get(offset..offset + 4) {
                    let v = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                    if v.is_finite() && (v.abs() < 500.0 || v == 0.0) {
                        *f32s.entry(format!("{v:.2}")).or_default() += 1;
                        if v.fract() == 0.0 {
                            integral += 1;
                        }
                    }
                    total += 1;
                }
            }
            let mut ranked: Vec<_> = f32s.iter().collect();
            ranked.sort_by(|a, b| b.1.cmp(a.1));
            let head: Vec<String> = ranked
                .iter()
                .take(10)
                .map(|(v, n)| format!("{v}×{n}"))
                .collect();
            println!(
                "    @{offset:>2}: {integral}/{total} integral f32; top: {}",
                head.join(" ")
            );
        }
        println!("    samples (edid, dnam-hex, vats-hex if any):");
        for (edid, d, vats) in rows.iter().take(8) {
            let hex = |v: &[u8]| v.iter().map(|b| format!("{b:02X}")).collect::<String>();
            println!(
                "      {edid:<28} {}  {}",
                hex(d),
                vats.as_ref().map(|v| hex(v)).unwrap_or_default()
            );
        }
    }
    Ok(())
}
