//! Spike probe for the FO3/FNV `BPTD` body-part-data wire layout (#6350).
//!
//! `parse_bptd` is a count-only stub: it reads `BPTN` counts and the first
//! part name, and the upgrade note defers the "BPTN/BPNN/BPNT/BPND quartet"
//! decode to the dismemberment consumer. VATS is that consumer, and every
//! downstream phase (limb scan, per-part hit chance, limb routing) depends
//! on the `BPND` payload layout — which no reference in `../reference`
//! documents at byte level. So the layout is established from the corpus,
//! the same way `WEAP.VATS` was (#3324): value-shape clustering names the
//! fields, not a remembered layout.
//!
//! What this reports:
//! * the sub-record grammar (type sequence patterns) across all BPTDs;
//! * per-type size distributions — BPTN/BPNN/BPNT are zstrings, BPND is the
//!   typed payload, and its size classes group the layout variants;
//! * for the dominant BPND size class: per-offset u8 histograms, f32 and u32
//!   reads, and FormID resolution against a whole-file type map (an actor
//!   value slot that resolves to AVIF, or an explosion slot to EXPL, is
//!   strong naming evidence);
//! * sample BPTN/BPNN/BPNT strings (part name vs `Bip01` bone vs target
//!   bone roles confirm from content);
//! * a RACE / NPC_ / CREA sweep for 4-byte sub-records that resolve to BPTD
//!   FormIDs — the race→body-data link the parser drops today.
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example probe_bptd -- <ESM>

use std::collections::BTreeMap;

use byroredux_plugin::esm::reader::EsmReader;

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: probe_bptd <ESM>"))?;
    let bytes = std::fs::read(&path)?;

    // Pass 1 — whole-file FormID → record type, so candidate u32 slots can
    // be classified against what they actually reference.
    let mut form_types: BTreeMap<u32, String> = BTreeMap::new();
    {
        let mut reader = EsmReader::new(&bytes);
        let _ = reader.read_file_header()?;
        reader.for_each_record(bytes.len(), None, |header, _subs| {
            form_types.insert(
                header.form_id,
                String::from_utf8_lossy(&header.record_type).to_string(),
            );
        })?;
    }

    let mut bptd_ids: Vec<u32> = Vec::new();
    {
        let mut reader = EsmReader::new(&bytes);
        let _ = reader.read_file_header()?;
        reader.for_each_record(bytes.len(), Some(b"BPTD"), |header, _subs| {
            bptd_ids.push(header.form_id);
        })?;
    }

    // Pass 2 — the BPTD grammar + BPND payload census.
    let mut total = 0usize;
    let mut grammar: BTreeMap<String, usize> = BTreeMap::new();
    let mut type_sizes: BTreeMap<String, BTreeMap<usize, usize>> = BTreeMap::new();
    let mut bpnd_by_size: BTreeMap<usize, Vec<Vec<u8>>> = BTreeMap::new();
    let mut string_samples: Vec<String> = Vec::new();

    let mut reader = EsmReader::new(&bytes);
    let _ = reader.read_file_header()?;
    reader.for_each_record(bytes.len(), Some(b"BPTD"), |header, subs| {
        total += 1;
        let zstring = |d: &[u8]| String::from_utf8_lossy(d).trim_end_matches('\0').to_string();
        let mut edid = String::new();
        for sub in &subs {
            if &sub.sub_type == b"EDID" {
                edid = zstring(&sub.data);
            }
        }
        let seq = subs
            .iter()
            .map(|s| String::from_utf8_lossy(&s.sub_type).to_string())
            .collect::<Vec<_>>()
            .join(",");
        *grammar.entry(seq).or_default() += 1;
        for sub in &subs {
            let t = String::from_utf8_lossy(&sub.sub_type).to_string();
            *type_sizes
                .entry(t)
                .or_default()
                .entry(sub.data.len())
                .or_default() += 1;
            match &sub.sub_type {
                b"BPTN" | b"BPNN" | b"BPNT" => {
                    if string_samples.len() < 12 {
                        string_samples.push(format!(
                            "{edid} [{:.6X}] {}: {}",
                            header.form_id,
                            String::from_utf8_lossy(&sub.sub_type),
                            zstring(&sub.data)
                        ));
                    }
                }
                b"BPND" => {
                    let bucket = bpnd_by_size.entry(sub.data.len()).or_default();
                    if bucket.len() < 200 {
                        bucket.push(sub.data.clone());
                    }
                }
                _ => {}
            }
        }
    })?;

    println!("{path}");
    println!("  BPTD records: {total} ({} unique form ids)", bptd_ids.len());
    println!("  grammar patterns (top 6):");
    let mut grammars: Vec<_> = grammar.iter().collect();
    grammars.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
    for (seq, n) in grammars.iter().take(6) {
        println!("    {n:>4}× {seq}");
    }
    println!("  per-type sizes:");
    let mut types: Vec<_> = type_sizes.iter().collect();
    types.sort_by(|a, b| a.0.cmp(b.0));
    for (t, sizes) in &types {
        let mut rows: Vec<String> = sizes
            .iter()
            .map(|(sz, n)| format!("{sz}B×{n}"))
            .collect();
        rows.sort();
        println!("    {t}: {}", rows.join(" "));
    }
    println!("  string samples (name vs bone vs target bone):");
    for s in &string_samples {
        println!("    {s}");
    }

    // BPND payload — analyse the dominant size class, plus any minority ones.
    let mut size_classes: Vec<_> = bpnd_by_size.iter().collect();
    size_classes.sort_by_key(|entry| std::cmp::Reverse(entry.1.len()));
    for &(size, payloads) in size_classes.iter().take(3) {
        println!("  BPND size class {size}B ({} samples):", payloads.len());
        for offset in 0..*size {
            let mut u8s: BTreeMap<u8, usize> = BTreeMap::new();
            let mut f32s: BTreeMap<String, usize> = BTreeMap::new();
            let mut u32_kinds: BTreeMap<String, usize> = BTreeMap::new();
            for d in payloads.iter() {
                if let Some(&b) = d.get(offset) {
                    *u8s.entry(b).or_default() += 1;
                }
                if let Some(b) = d.get(offset..offset + 4) {
                    let v = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                    if v.is_finite() && v.abs() < 1000.0 {
                        *f32s.entry(format!("{v:.3}")).or_default() += 1;
                    }
                    let u = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                    let kind = if u == 0 {
                        "<zero>".to_string()
                    } else {
                        form_types
                            .get(&u)
                            .cloned()
                            .unwrap_or_else(|| "<unresolved>".to_string())
                    };
                    *u32_kinds.entry(kind).or_default() += 1;
                }
            }
            let top_u8: Vec<String> = {
                let mut rows: Vec<_> = u8s.iter().collect();
                rows.sort_by(|a, b| b.1.cmp(a.1));
                rows.iter()
                    .take(8)
                    .map(|(v, n)| format!("{v}×{n}"))
                    .collect()
            };
            let top_f32: Vec<String> = {
                let mut rows: Vec<_> = f32s.iter().collect();
                rows.sort_by(|a, b| b.1.cmp(a.1));
                rows.iter()
                    .take(6)
                    .map(|(v, n)| format!("{v}×{n}"))
                    .collect()
            };
            let resolved: Vec<String> = {
                let mut rows: Vec<_> = u32_kinds.iter().collect();
                rows.sort_by(|a, b| b.1.cmp(a.1));
                rows.iter()
                    .take(4)
                    .map(|(k, n)| format!("{k}×{n}"))
                    .collect()
            };
            println!(
                "    @{offset:>2}: u8[{}] f32[{}] u32→[{}]",
                top_u8.join(" "),
                top_f32.join(" "),
                resolved.join(" ")
            );
        }
        println!("    raw samples:");
        for d in payloads.iter().take(2) {
            println!(
                "      {}",
                d.iter().map(|b| format!("{b:02X}")).collect::<String>()
            );
        }
    }

    // Pass 2b — correlate the candidate scalar offsets with the owning
    // part's BPTN name. Chance/health/type/flags are disambiguated by the
    // roles vanilla authors give named parts (Head carries a distinct
    // profile from Torso/Legs), not by marginal distributions alone.
    {
        let mut by_part: BTreeMap<String, BTreeMap<usize, BTreeMap<u8, usize>>> =
            BTreeMap::new();
        let mut reader = EsmReader::new(&bytes);
        let _ = reader.read_file_header()?;
        reader.for_each_record(bytes.len(), Some(b"BPTD"), |_header, subs| {
            let zstring = |d: &[u8]| String::from_utf8_lossy(d).trim_end_matches('\0').to_string();
            let mut part_name = String::new();
            for sub in &subs {
                if &sub.sub_type == b"BPTN" {
                    part_name = zstring(&sub.data);
                } else if &sub.sub_type == b"BPND" && sub.data.len() == 84 {
                    let bucket = by_part.entry(part_name.clone()).or_default();
                    for offset in 4..=10 {
                        if let Some(&b) = sub.data.get(offset) {
                            *bucket.entry(offset).or_default().entry(b).or_default() += 1;
                        }
                    }
                }
            }
        })?;
        println!("  per-part-name scalar profile (offsets 4..=10, u8):");
        let mut names: Vec<_> = by_part.iter().collect();
        names.sort_by(|a, b| a.0.cmp(b.0));
        for (name, offsets) in names.iter().take(24) {
            let mut cols: Vec<String> = Vec::new();
            for offset in 4..=10 {
                let mut rows: Vec<_> = offsets
                    .get(&offset)
                    .map(|m| m.iter().collect())
                    .unwrap_or_default();
                rows.sort_by(|a, b| b.1.cmp(a.1));
                let head: Vec<String> = rows
                    .iter()
                    .take(4)
                    .map(|(v, n)| format!("{v}×{n}"))
                    .collect();
                cols.push(format!("@{offset}:{}", head.join("/")));
            }
            println!("    {name:<12} {}", cols.join("  "));
        }
    }

    // Pass 2c — DOBJ default objects: where the humanoid default BPTD
    // (races carry no BPTD ref; only CREA.PNAM does) is assigned.
    {
        let mut reader = EsmReader::new(&bytes);
        let _ = reader.read_file_header()?;
        reader.for_each_record(bytes.len(), Some(b"DOBJ"), |header, subs| {
            println!(
                "  DOBJ [{:.8X}] subs:",
                header.form_id
            );
            for sub in &subs {
                let entries: Vec<String> = sub
                    .data
                    .chunks(8)
                    .filter(|c| c.len() == 8)
                    .map(|c| {
                        let idx = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                        let form = u32::from_le_bytes([c[4], c[5], c[6], c[7]]);
                        let kind = form_types
                            .get(&form)
                            .cloned()
                            .unwrap_or_else(|| "<unresolved>".to_string());
                        format!("idx {idx} → {kind} {:.8X}", form)
                    })
                    .collect();
                println!(
                    "    {} ({}B): {}",
                    String::from_utf8_lossy(&sub.sub_type),
                    sub.data.len(),
                    entries.join(", ")
                );
            }
        })?;
    }

    // Pass 3 — who references BPTD? Sweep RACE / NPC_ / CREA for 4-byte
    // sub-records resolving to a BPTD FormID, and inventory their
    // sub-record types so the carrier arm is unambiguous.
    let bptd_set: std::collections::HashSet<u32> = bptd_ids.iter().copied().collect();
    for record_type in [b"RACE", b"NPC_", b"CREA"] {
        let mut carriers: BTreeMap<String, usize> = BTreeMap::new();
        let mut records_with_ref = 0usize;
        let mut records_total = 0usize;
        let mut reader = EsmReader::new(&bytes);
        let _ = reader.read_file_header()?;
        reader.for_each_record(bytes.len(), Some(record_type), |_header, subs| {
            records_total += 1;
            let mut hit = false;
            for sub in &subs {
                for w in sub.data.windows(4) {
                    let u = u32::from_le_bytes([w[0], w[1], w[2], w[3]]);
                    if bptd_set.contains(&u) {
                        *carriers
                            .entry(String::from_utf8_lossy(&sub.sub_type).to_string())
                            .or_default() += 1;
                        hit = true;
                    }
                }
            }
            if hit {
                records_with_ref += 1;
            }
        })?;
        println!(
            "  {type}: {records_total} records, {records_with_ref} carry a BPTD ref (incl. embedded); carriers: {carriers:?}",
            type = String::from_utf8_lossy(record_type),
        );
    }

    // Pass 4 — every BPTD EDID, so the humanoid default record is
    // identifiable by name when its carrier turns up.
    {
        let mut reader = EsmReader::new(&bytes);
        let _ = reader.read_file_header()?;
        reader.for_each_record(bytes.len(), Some(b"BPTD"), |header, subs| {
            for sub in &subs {
                if &sub.sub_type == b"EDID" {
                    println!(
                        "  BPTD {:.8X} = {}",
                        header.form_id,
                        String::from_utf8_lossy(&sub.data).trim_end_matches('\0')
                    );
                }
            }
        })?;
    }
    Ok(())
}
