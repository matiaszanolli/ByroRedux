//! Survey Skyrim `PACK` records directly from an ESM.
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example dump_pack_subs -- <Skyrim.esm> [EDID_FILTER]
//!
//! Prefix the filter with `=` for an exact editor-ID match, or pass an
//! eight-digit hexadecimal FormID.

use std::collections::BTreeMap;

use byroredux_plugin::esm::reader::{EsmReader, RecordHeader, SubRecord};

const RECORD_TYPE: [u8; 4] = *b"PACK";
const PREVIEW_BYTE_LIMIT: usize = 64;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: dump_pack_subs <Skyrim.esm> [EDID_FILTER]"))?;
    let filter = args.next().unwrap_or_else(|| "mq101".to_owned());
    let selector = Selector::parse(&filter);

    let bytes = std::fs::read(&path)?;
    let mut reader = EsmReader::new(&bytes);
    let _header = reader.read_file_header()?;
    let mut survey = Survey::default();
    walk_groups(&mut reader, bytes.len(), &selector, &mut survey)?;

    println!();
    println!("PACK records total: {}", survey.total);
    println!("PACK records matching {filter:?}: {}", survey.matched);
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

enum Selector {
    Contains(String),
    Exact(String),
    FormId(u32),
}

impl Selector {
    fn parse(raw: &str) -> Self {
        if let Some(exact) = raw.strip_prefix('=') {
            return Self::Exact(exact.to_ascii_lowercase());
        }
        if raw.len() == 8 && raw.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            if let Ok(form_id) = u32::from_str_radix(raw, 16) {
                return Self::FormId(form_id);
            }
        }
        Self::Contains(raw.to_ascii_lowercase())
    }

    fn matches(&self, form_id: u32, editor_id: &str) -> bool {
        let editor_id = editor_id.to_ascii_lowercase();
        match self {
            Self::Contains(needle) => editor_id.contains(needle),
            Self::Exact(expected) => editor_id == *expected,
            Self::FormId(expected) => form_id == *expected,
        }
    }
}

fn walk_groups(
    reader: &mut EsmReader,
    end: usize,
    selector: &Selector,
    survey: &mut Survey,
) -> anyhow::Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let group = reader.read_group_header()?;
            let inner_end = reader.group_content_end(&group);
            walk_groups(reader, inner_end, selector, survey)?;
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
        if !selector.matches(header.form_id, &editor_id) {
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
            "  {index:>3} {kind} len={:>5} {decoded:<36} hex={preview:02x?}",
            sub.data.len()
        );
    }
}

fn decode_preview(kind: &str, data: &[u8]) -> String {
    match kind {
        "EDID" | "ANAM" | "BNAM" | "PNAM" => format!("str={:?}", read_zstring(data)),
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
