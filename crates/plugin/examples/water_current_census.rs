//! Census every raw `XWCN` / `XWCS` / `XWCU` water-current block on `CELL`
//! and `REFR` records, decoding `XWCU` as its documented array of 16-byte
//! entries.
//!
//! Evidence harness for WATAL W2: the loader used to read the first three
//! floats of `XWCU` as a single current velocity. Vanilla Skyrim's
//! `EvergreenGroveExterior` instead authors `XWCN = 4` followed by a
//! 64-byte `XWCU`, so the layout and meaning need to be read off real data
//! across games before the decode is changed.
//!
//! Usage:
//!   cargo run --release -p byroredux-plugin --example water_current_census -- <ESM> [...]

use byroredux_plugin::esm::reader::{EsmReader, SubRecord};

fn main() -> anyhow::Result<()> {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path)?;
        let mut reader = EsmReader::new(&bytes);
        let _header = reader.read_file_header()?;
        let mut stats = Stats::default();
        println!("=== {path}");
        walk(&mut reader, bytes.len(), &mut stats)?;
        println!(
            "  totals: CELL with XWCU {} / REFR with XWCU {} / XWCN without XWCU {} / length mismatches {}",
            stats.cells, stats.refs, stats.count_only, stats.mismatches
        );
    }
    Ok(())
}

#[derive(Default)]
struct Stats {
    cells: usize,
    refs: usize,
    count_only: usize,
    mismatches: usize,
}

fn walk(reader: &mut EsmReader<'_>, end: usize, stats: &mut Stats) -> anyhow::Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let group = reader.read_group_header()?;
            let inner_end = reader.group_content_end(&group);
            walk(reader, inner_end, stats)?;
            continue;
        }
        let header = reader.read_record_header()?;
        if header.record_type != *b"CELL" && header.record_type != *b"REFR" {
            reader.skip_record(&header);
            continue;
        }
        let subs = reader.read_sub_records(&header)?;
        let find = |tag: &[u8; 4]| subs.iter().find(|s| &s.sub_type == tag);
        let count = find(b"XWCN")
            .or_else(|| find(b"XWCS"))
            .filter(|s| s.data.len() >= 4)
            .map(|s| u32::from_le_bytes(s.data[..4].try_into().unwrap()));
        let Some(xwcu) = find(b"XWCU") else {
            if count.is_some() {
                stats.count_only += 1;
            }
            continue;
        };
        let kind = std::str::from_utf8(&header.record_type).unwrap_or("????");
        if kind == "CELL" {
            stats.cells += 1;
        } else {
            stats.refs += 1;
        }
        let entries = xwcu.data.len() / 16;
        let consistent = xwcu.data.len() % 16 == 0 && count.is_none_or(|n| n as usize == entries);
        if !consistent {
            stats.mismatches += 1;
        }
        let base = find(b"NAME")
            .filter(|s| s.data.len() >= 4)
            .map(|s| format!("base={:08X}", u32::from_le_bytes(s.data[..4].try_into().unwrap())))
            .unwrap_or_default();
        println!(
            "  {kind} {:08X} {:<32} {base} count={count:?} xwcu_len={} entries={entries}{}",
            header.form_id,
            editor_id(&subs),
            xwcu.data.len(),
            if consistent { "" } else { "  <-- LENGTH MISMATCH" }
        );
        for (index, entry) in xwcu.data.chunks(16).enumerate() {
            let f = |o: usize| {
                entry
                    .get(o..o + 4)
                    .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            };
            let tail_u32 = entry
                .get(12..16)
                .map(|b| u32::from_le_bytes(b.try_into().unwrap()));
            println!(
                "      [{index}] vec=({:?}, {:?}, {:?}) tail_f32={:?} tail_u32={tail_u32:?}",
                f(0),
                f(4),
                f(8),
                f(12)
            );
        }
    }
    Ok(())
}

fn editor_id(subs: &[SubRecord]) -> String {
    subs.iter()
        .find(|s| s.sub_type == *b"EDID")
        .map(|s| {
            let end = s.data.iter().position(|b| *b == 0).unwrap_or(s.data.len());
            String::from_utf8_lossy(&s.data[..end]).into_owned()
        })
        .unwrap_or_default()
}
