//! Dump WTHR sub-records from a Bethesda ESM so we can ground-truth the
//! Skyrim schema before implementing `parse_wthr` for `GameKind::Skyrim`
//! (#539 / M33-04..07). Mirrors the `dump_npc_subs` pattern.
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example dump_wthr_subs -- <Skyrim.esm> [substr...]
//!
//! Substr filter: only WTHRs whose EDID contains *any* of the given
//! substrings are printed (default: a representative spread of clear /
//! cloudy / storm / snow / overcast weathers across Skyrim's vanilla
//! climates).

use byroredux_plugin::esm::reader::{EsmReader, RecordHeader, SubRecord};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: <ESM> [substr...]"))?;
    let filters: Vec<String> = {
        let rest: Vec<String> = args.collect();
        if rest.is_empty() {
            vec![
                "SkyrimClear".into(),
                "SkyrimCloudy".into(),
                "SkyrimStorm".into(),
                "SkyrimSnow".into(),
                "SkyrimOvercast".into(),
                "ClearMarsh".into(),
                "WeatherClear".into(),
                "WeatherCloudy".into(),
            ]
        } else {
            rest
        }
    };

    let bytes = std::fs::read(&path)?;
    println!(
        "Parsing {} ({:.1} MB)…",
        path,
        bytes.len() as f64 / 1_048_576.0
    );

    let mut reader = EsmReader::new(&bytes);
    let _hdr = reader.read_file_header()?;
    let total = bytes.len();
    walk_groups(&mut reader, total, &filters)?;
    Ok(())
}

fn walk_groups(reader: &mut EsmReader, end: usize, filters: &[String]) -> anyhow::Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let g = reader.read_group_header()?;
            let inner_end = reader.group_content_end(&g);
            if g.label == *b"WTHR" {
                walk_wthr_group(reader, inner_end, filters)?;
            } else {
                walk_groups(reader, inner_end, filters)?;
            }
        } else {
            let h = reader.read_record_header()?;
            reader.skip_record(&h);
        }
    }
    Ok(())
}

fn walk_wthr_group(reader: &mut EsmReader, end: usize, filters: &[String]) -> anyhow::Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let g = reader.read_group_header()?;
            let inner_end = reader.group_content_end(&g);
            walk_wthr_group(reader, inner_end, filters)?;
            continue;
        }
        let h = reader.read_record_header()?;
        if h.record_type == *b"WTHR" {
            let subs = reader.read_sub_records(&h)?;
            let edid = subs
                .iter()
                .find(|s| s.sub_type == *b"EDID")
                .map(|s| read_zstring(&s.data))
                .unwrap_or_default();
            if filters.is_empty() || filters.iter().any(|f| edid.contains(f.as_str())) {
                print_wthr(&h, &edid, &subs);
            }
        } else {
            reader.skip_record(&h);
        }
    }
    Ok(())
}

fn print_wthr(h: &RecordHeader, edid: &str, subs: &[SubRecord]) {
    println!("\n== {} (form {:08X}) ==", edid, h.form_id);
    // Per-subtype tally to spot stride drift across weathers (e.g.
    // NAM0 should be a fixed size per game; FNAM stride varies between
    // Oblivion / FNV / Skyrim).
    let mut counts: std::collections::HashMap<[u8; 4], (usize, usize)> =
        std::collections::HashMap::new();
    for s in subs {
        let entry = counts.entry(s.sub_type).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += s.data.len();
    }
    let mut keys: Vec<[u8; 4]> = counts.keys().copied().collect();
    keys.sort();
    for k in &keys {
        let (n, total_bytes) = counts[k];
        let kind = std::str::from_utf8(k).unwrap_or("????");
        println!("  {} ×{}  {} bytes total", kind, n, total_bytes);
    }
    println!("  ---");
    // Dump each sub_record's first 96 bytes hex so the parser
    // implementer can eyeball record-end alignment + field offsets.
    for s in subs {
        let kind = std::str::from_utf8(&s.sub_type).unwrap_or("????");
        let preview = &s.data[..s.data.len().min(96)];
        println!("  {} len={:>5}  hex={:02x?}", kind, s.data.len(), preview);
    }
}

fn read_zstring(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).to_string()
}
