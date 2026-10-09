//! Dump Starfield WTHS records and their decoded BGS reflection
//! containers (#5364) — the ground-truth probe used to pin the chunk
//! grammar before `parse_bgs_reflection` landed. Kept for the
//! instance-value follow-up (OBJT/LIST/USER): it prints the schema
//! half through the parser and the instance half as raw hex.
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example dump_wths_refl -- <Starfield.esm> [substr...]

use byroredux_plugin::esm::reader::{EsmReader, RecordHeader, SubRecord};
use byroredux_plugin::esm::records::parse_bgs_reflection;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: <Starfield.esm> [substr...]"))?;
    let filters: Vec<String> = args.collect();

    let bytes = std::fs::read(&path)?;
    println!("Parsing {} ({:.1} MB)\u{2026}", path, bytes.len() as f64 / 1_048_576.0);

    let mut reader = EsmReader::new(&bytes);
    let _hdr = reader.read_file_header()?;
    let total = bytes.len();
    let mut found = 0;
    walk_groups(&mut reader, total, &filters, &mut found)?;
    println!("\ntotal WTHS records: {found}");
    Ok(())
}

fn walk_groups(
    reader: &mut EsmReader,
    end: usize,
    filters: &[String],
    found: &mut usize,
) -> anyhow::Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let g = reader.read_group_header()?;
            let inner_end = reader.group_content_end(&g);
            if g.label == *b"WTHS" {
                walk_wths_group(reader, inner_end, filters, found)?;
            } else {
                walk_groups(reader, inner_end, filters, found)?;
            }
        } else {
            let h = reader.read_record_header()?;
            reader.skip_record(&h);
        }
    }
    Ok(())
}

fn walk_wths_group(
    reader: &mut EsmReader,
    end: usize,
    filters: &[String],
    found: &mut usize,
) -> anyhow::Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let g = reader.read_group_header()?;
            let inner_end = reader.group_content_end(&g);
            walk_wths_group(reader, inner_end, filters, found)?;
            continue;
        }
        let h = reader.read_record_header()?;
        if h.record_type == *b"WTHS" {
            let subs = reader.read_sub_records(&h)?;
            *found += 1;
            let edid = subs
                .iter()
                .find(|s| s.sub_type == *b"EDID")
                .map(|s| read_zstring(&s.data))
                .unwrap_or_default();
            if filters.is_empty()
                || filters.iter().any(|f| edid.to_lowercase().contains(&f.to_lowercase()))
            {
                print_wths(&h, &edid, &subs);
            }
        } else {
            reader.skip_record(&h);
        }
    }
    Ok(())
}

fn read_zstring(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn print_wths(h: &RecordHeader, edid: &str, subs: &[SubRecord]) {
    println!("\n== {edid} (form {:08X}) ==", h.form_id);
    for s in subs {
        println!("  {} ({} B)", String::from_utf8_lossy(&s.sub_type), s.data.len());
    }
    let Some(blob) = subs
        .iter()
        .find(|s| s.sub_type == *b"REFL")
        .or_else(|| subs.iter().find(|s| s.sub_type == *b"RDIF"))
    else {
        return;
    };
    let refl = parse_bgs_reflection(&blob.data);
    println!(
        "  reflection: version={} chunks={} classes={} strings={} diff={:?} instance={:?} unknown={:?}",
        refl.version,
        refl.chunk_count,
        refl.classes.len(),
        refl.strings.len(),
        refl.diff_field_indices.as_ref().map(Vec::len),
        refl.instance_chunks,
        refl.unknown_chunks
    );
    for class in &refl.classes {
        println!(
            "  class {} (flags {:#06x}, {} fields)",
            class.name,
            class.flags,
            class.fields.len()
        );
        for f in &class.fields {
            println!(
                "    {} : {} (off={}, size={})",
                f.name, f.type_name, f.offset, f.size
            );
        }
    }
    // The instance half, raw — the follow-up decoder's input.
    let mut off = 0usize;
    while off + 8 <= blob.data.len() {
        let sig: [u8; 4] = blob.data[off..off + 4].try_into().unwrap();
        let size = u32::from_le_bytes(blob.data[off + 4..off + 8].try_into().unwrap()) as usize;
        if matches!(&sig, b"OBJT" | b"LIST" | b"USER") {
            println!(
                "  instance chunk {} ({} B):",
                String::from_utf8_lossy(&sig),
                size
            );
            let payload = &blob.data[off + 8..(off + 8 + size).min(blob.data.len())];
            for row in payload.chunks(16).take(8) {
                let hex: Vec<String> = row.iter().map(|b| format!("{b:02x}")).collect();
                println!("    {}", hex.join(" "));
            }
        }
        off += 8 + size;
    }
}
