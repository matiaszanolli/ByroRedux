//! Census the FO3/FNV `WEAP` `VATS` sub-record (#3324).
//!
//! `ItemRecord::ap_cost` was pinned to `0.0` for every FO3/FNV weapon because
//! the parser looked for it in `DNAM`. It is not there: FO3/FNV author a
//! dedicated `VATS` sub-record that no arm dispatches.
//!
//! This probe exists so the field order is established from the archive
//! rather than from a remembered layout. It decodes the candidate
//! `u32 + f32 + f32 + f32 + u8` shape and reports, per slot, the value
//! distribution and — for the leading FormID — what record type each payload
//! resolves to against a whole-file FormID→type map. A slot whose values are
//! integral and clustered in 14..=48 is AP cost; one clustered in 0..=100 on
//! multiples of 5 is a skill requirement; one clustered near 1.0 is a
//! multiplier. Those shapes are what name the fields, not a guess.
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example probe_weap_vats -- <ESM>

use std::collections::BTreeMap;

use byroredux_plugin::esm::reader::{EsmReader, RecordHeader, SubRecord};

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: probe_weap_vats <ESM>"))?;
    let bytes = std::fs::read(&path)?;

    // Pass 1 — whole-file FormID → record type, so the leading u32 can be
    // classified instead of assumed.
    let mut form_types: BTreeMap<u32, String> = BTreeMap::new();
    {
        let mut reader = EsmReader::new(&bytes);
        let _ = reader.read_file_header()?;
        walk(&mut reader, bytes.len(), &mut |header, _subs| {
            form_types.insert(
                header.form_id,
                String::from_utf8_lossy(&header.record_type).to_string(),
            );
        })?;
    }

    // Pass 2 — the WEAP census.
    let mut reader = EsmReader::new(&bytes);
    let _ = reader.read_file_header()?;

    let mut weap_total = 0usize;
    let mut with_vats = 0usize;
    let mut sizes: BTreeMap<usize, usize> = BTreeMap::new();
    let mut effect_types: BTreeMap<String, usize> = BTreeMap::new();
    let mut slot4: BTreeMap<String, usize> = BTreeMap::new();
    let mut slot8: BTreeMap<String, usize> = BTreeMap::new();
    let mut slot12: BTreeMap<String, usize> = BTreeMap::new();
    let mut slot16: BTreeMap<u8, usize> = BTreeMap::new();
    let mut samples: Vec<String> = Vec::new();

    walk(&mut reader, bytes.len(), &mut |header, subs| {
        if &header.record_type != b"WEAP" {
            return;
        }
        weap_total += 1;
        let mut edid = String::new();
        for sub in subs {
            if &sub.sub_type == b"EDID" {
                edid = String::from_utf8_lossy(&sub.data)
                    .trim_end_matches('\0')
                    .to_string();
            }
        }
        for sub in subs {
            if &sub.sub_type != b"VATS" {
                continue;
            }
            with_vats += 1;
            *sizes.entry(sub.data.len()).or_default() += 1;
            let d = &sub.data;
            let u32_at = |o: usize| -> Option<u32> {
                d.get(o..o + 4)
                    .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            };
            let f32_at = |o: usize| -> Option<f32> {
                d.get(o..o + 4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            };
            if let Some(effect) = u32_at(0) {
                let kind = if effect == 0 {
                    "<null>".to_string()
                } else {
                    form_types
                        .get(&effect)
                        .cloned()
                        .unwrap_or_else(|| "<unresolved>".to_string())
                };
                *effect_types.entry(kind).or_default() += 1;
            }
            for (offset, bucket) in [(4, &mut slot4), (8, &mut slot8), (12, &mut slot12)] {
                if let Some(v) = f32_at(offset) {
                    *bucket.entry(format!("{v:.2}")).or_default() += 1;
                }
            }
            if let Some(&v) = d.get(16) {
                *slot16.entry(v).or_default() += 1;
            }
            if samples.len() < 6 && f32_at(12).is_some_and(|v| v > 0.0) {
                samples.push(format!(
                    "{edid}: effect={:08X} @4={:.2} @8={:.2} @12={:.2} @16={} raw={}",
                    u32_at(0).unwrap_or(0),
                    f32_at(4).unwrap_or(0.0),
                    f32_at(8).unwrap_or(0.0),
                    f32_at(12).unwrap_or(0.0),
                    d.get(16).copied().unwrap_or(0),
                    d.iter()
                        .map(|b| format!("{b:02X}"))
                        .collect::<Vec<_>>()
                        .join("")
                ));
            }
        }
    })?;

    println!("{path}");
    println!("  WEAP={weap_total} with_VATS={with_vats} sizes={sizes:?}");
    println!("  slot 0 (u32) resolves to: {effect_types:?}");
    for (label, bucket) in [
        ("slot 4  (f32)", &slot4),
        ("slot 8  (f32)", &slot8),
        ("slot 12 (f32)", &slot12),
    ] {
        let mut rows: Vec<_> = bucket.iter().collect();
        rows.sort_by(|a, b| b.1.cmp(a.1));
        let head: Vec<String> = rows
            .iter()
            .take(14)
            .map(|(v, n)| format!("{v}×{n}"))
            .collect();
        println!("  {label}: {} distinct, {}", bucket.len(), head.join(" "));
    }
    println!("  slot 16 (u8): {slot16:?}");
    for s in &samples {
        println!("    {s}");
    }
    Ok(())
}

fn walk(
    reader: &mut EsmReader,
    end: usize,
    visit: &mut impl FnMut(&RecordHeader, &[SubRecord]),
) -> anyhow::Result<()> {
    while reader.position() < end {
        let Ok(header) = reader.read_record_header() else {
            break;
        };
        if &header.record_type == b"GRUP" {
            let group_end = reader.position() + header.data_size as usize - 24;
            walk(reader, group_end.min(end), visit)?;
            continue;
        }
        let subs = reader.read_sub_records(&header).unwrap_or_default();
        visit(&header, &subs);
    }
    Ok(())
}
