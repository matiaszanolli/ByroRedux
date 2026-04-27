//! Dump NPC_ sub-records for named FO4 actors so the audit's #591
//! face-morph layout claims (FMRI / FMRS / NAM9 / MSDK / MSDV / QNAM /
//! NAMA / FTSM / BCLF) can be ground-truthed against real bytes before
//! a parser lands.
//!
//! Usage: `cargo run -p byroredux-plugin --example dump_npc_subs -- <Fallout4.esm> [substr...]`
//!
//! Substr filter: only NPCs whose EDID contains *any* of the given
//! substrings are printed (default: Piper / Cait / Valentine / Curie /
//! Preston / Codsworth / Hancock).

use byroredux_plugin::esm::reader::{EsmReader, RecordHeader, SubRecord};

const FACE_SUBS: &[&[u8; 4]] = &[
    b"FMRI", b"FMRS", b"MSDK", b"MSDV", b"NAM9", b"QNAM", b"NAMA", b"FTSM", b"BCLF", b"PNAM",
    b"HCLF", b"BNAM", b"WNAM", b"OBND",
];

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: <ESM> [substr...]"))?;
    let filters: Vec<String> = {
        let rest: Vec<String> = args.collect();
        if rest.is_empty() {
            vec![
                "Piper".into(),
                "Cait".into(),
                "Valentine".into(),
                "Curie".into(),
                "Preston".into(),
                "Codsworth".into(),
                "Hancock".into(),
            ]
        } else {
            rest
        }
    };

    let bytes = std::fs::read(&path)?;
    println!("Parsing {} ({:.1} MB)…", path, bytes.len() as f64 / 1_048_576.0);

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
            if g.label == *b"NPC_" {
                walk_npc_group(reader, inner_end, filters)?;
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

fn walk_npc_group(
    reader: &mut EsmReader,
    end: usize,
    filters: &[String],
) -> anyhow::Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let g = reader.read_group_header()?;
            let inner_end = reader.group_content_end(&g);
            walk_npc_group(reader, inner_end, filters)?;
            continue;
        }
        let h = reader.read_record_header()?;
        if h.record_type == *b"NPC_" {
            let subs = reader.read_sub_records(&h)?;
            let edid = subs
                .iter()
                .find(|s| s.sub_type == *b"EDID")
                .map(|s| read_zstring(&s.data))
                .unwrap_or_default();
            if filters.iter().any(|f| edid.contains(f.as_str())) {
                print_npc(&h, &edid, &subs);
            }
        } else {
            reader.skip_record(&h);
        }
    }
    Ok(())
}

fn print_npc(h: &RecordHeader, edid: &str, subs: &[SubRecord]) {
    println!("\n== {} (form {:08X}) ==", edid, h.form_id);
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
        println!(
            "  {} ×{}  {} bytes total",
            kind, n, total_bytes
        );
    }
    println!("  ---");
    for s in subs {
        if FACE_SUBS.iter().any(|t| t == &&s.sub_type) {
            let kind = std::str::from_utf8(&s.sub_type).unwrap_or("????");
            let preview = &s.data[..s.data.len().min(32)];
            println!(
                "  {}  len={:>5}  hex={:02x?}",
                kind,
                s.data.len(),
                preview,
            );
        }
    }
}

fn read_zstring(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).to_string()
}
