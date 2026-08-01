//! Dump one record's raw subrecords from an ESM for format diagnostics.
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example dump_record_subs -- \
//!     <file.esm> <RECORD_TYPE> <FORMID_HEX|=EDITOR_ID|EDID_SUBSTRING>

use byroredux_plugin::esm::reader::{EsmReader, RecordHeader, SubRecord};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or_else(|| {
        anyhow::anyhow!("usage: dump_record_subs <file.esm> <TYPE> <FORMID|EDID>")
    })?;
    let record_type: [u8; 4] = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing four-character record type"))?
        .as_bytes()
        .try_into()
        .map_err(|_| anyhow::anyhow!("record type must be exactly four bytes"))?;
    let selector = Selector::parse(
        &args
            .next()
            .ok_or_else(|| anyhow::anyhow!("missing FormID or editor-ID selector"))?,
    );

    let bytes = std::fs::read(path)?;
    let mut reader = EsmReader::new(&bytes);
    let _header = reader.read_file_header()?;
    walk_groups(&mut reader, bytes.len(), record_type, &selector)
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
        if let Ok(form_id) = u32::from_str_radix(raw.trim_start_matches("0x"), 16) {
            return Self::FormId(form_id);
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
    reader: &mut EsmReader<'_>,
    end: usize,
    record_type: [u8; 4],
    selector: &Selector,
) -> anyhow::Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let group = reader.read_group_header()?;
            let inner_end = reader.group_content_end(&group);
            walk_groups(reader, inner_end, record_type, selector)?;
            continue;
        }
        let header = reader.read_record_header()?;
        if header.record_type != record_type {
            reader.skip_record(&header);
            continue;
        }
        let subs = reader.read_sub_records(&header)?;
        let editor_id = subs
            .iter()
            .find(|sub| sub.sub_type == *b"EDID")
            .map(|sub| zstring(&sub.data))
            .unwrap_or_default();
        if selector.matches(header.form_id, &editor_id) {
            print_record(&header, &editor_id, &subs);
        }
    }
    Ok(())
}

fn print_record(header: &RecordHeader, editor_id: &str, subs: &[SubRecord]) {
    println!("== {editor_id} ({:08X}) ==", header.form_id);
    for (index, sub) in subs.iter().enumerate() {
        let kind = std::str::from_utf8(&sub.sub_type).unwrap_or("????");
        let preview = &sub.data[..sub.data.len().min(64)];
        let text = matches!(kind, "EDID" | "CIS1" | "CIS2" | "ANAM" | "BNAM" | "PNAM")
            .then(|| zstring(&sub.data));
        println!(
            "{index:>3} {kind} len={:>5} text={text:?} hex={preview:02x?}",
            sub.data.len()
        );
    }
}

fn zstring(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}
