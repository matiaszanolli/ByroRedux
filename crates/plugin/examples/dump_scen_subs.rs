//! Survey Skyrim `SCEN` records directly from an ESM.
//!
//! `SCEN` is a marker-delimited record: the meaning of repeated `NAM0`,
//! `ALID`, `FNAM`, `SNAM`, and `ANAM` subrecords depends on which phase,
//! actor, or action is currently open.  This raw probe keeps the byte-level
//! evidence reproducible while the typed parser evolves.
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example dump_scen_subs -- <Skyrim.esm> [EDID_FILTER]

use std::collections::BTreeMap;

use byroredux_plugin::esm::reader::{EsmReader, RecordHeader, SubRecord};

const RECORD_TYPE: [u8; 4] = *b"SCEN";
const PREVIEW_BYTE_LIMIT: usize = 48;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: dump_scen_subs <Skyrim.esm> [EDID_FILTER]"))?;
    let filter = args.next().unwrap_or_else(|| "mq101".to_owned());
    let filter = filter.to_ascii_lowercase();

    let bytes = std::fs::read(&path)?;
    let mut reader = EsmReader::new(&bytes);
    let _header = reader.read_file_header()?;
    let mut survey = Survey::default();
    walk_groups(&mut reader, bytes.len(), &filter, &mut survey)?;

    println!();
    println!("SCEN records total: {}", survey.total);
    println!("SCEN records matching {filter:?}: {}", survey.matched);
    println!("matching subrecord histogram:");
    for (kind, count) in survey.subrecord_histogram {
        println!("  {kind} {count}");
    }
    Ok(())
}

#[derive(Default)]
struct Survey {
    total: usize,
    matched: usize,
    subrecord_histogram: BTreeMap<String, usize>,
}

fn walk_groups(
    reader: &mut EsmReader,
    end: usize,
    filter: &str,
    survey: &mut Survey,
) -> anyhow::Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let group = reader.read_group_header()?;
            let inner_end = reader.group_content_end(&group);
            walk_groups(reader, inner_end, filter, survey)?;
            continue;
        }

        let header = reader.read_record_header()?;
        if header.record_type != RECORD_TYPE {
            reader.skip_record(&header);
            continue;
        }

        let subs = reader.read_sub_records(&header)?;
        survey.total += 1;
        let editor_id = subs
            .iter()
            .find(|sub| sub.sub_type == *b"EDID")
            .map(|sub| read_zstring(&sub.data))
            .unwrap_or_default();
        if !editor_id.to_ascii_lowercase().contains(filter) {
            continue;
        }

        survey.matched += 1;
        print_record(&header, &editor_id, &subs, survey);
    }
    Ok(())
}

fn print_record(header: &RecordHeader, editor_id: &str, subs: &[SubRecord], survey: &mut Survey) {
    println!();
    println!("== {editor_id} (form {:08X}) ==", header.form_id);
    for (index, sub) in subs.iter().enumerate() {
        let kind = std::str::from_utf8(&sub.sub_type).unwrap_or("????");
        *survey
            .subrecord_histogram
            .entry(kind.to_owned())
            .or_default() += 1;
        let preview = &sub.data[..sub.data.len().min(PREVIEW_BYTE_LIMIT)];
        let decoded = decode_preview(kind, &sub.data);
        println!(
            "  {index:>3} {kind} len={:>5} {decoded:<28} hex={preview:02x?}",
            sub.data.len()
        );
    }
}

fn decode_preview(kind: &str, data: &[u8]) -> String {
    match kind {
        "EDID" | "NAM0" => format!("str={:?}", read_zstring(data)),
        _ if data.len() == 4 => {
            let raw = u32::from_le_bytes(data.try_into().expect("four-byte slice"));
            let float = f32::from_bits(raw);
            format!("u32={raw} fid={raw:08X} f32={float:.3}")
        }
        _ => String::new(),
    }
}

fn read_zstring(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}
