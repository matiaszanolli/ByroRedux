//! Survey Fallout 4 `VOLI` (volumetric-lighting) records directly from an ESM.
//!
//! This is intentionally a raw corpus probe rather than a parser: Bethesda
//! record layouts must be confirmed against shipped data before fields become
//! part of the engine contract.
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example dump_voli_subs -- <Fallout4.esm>

use byroredux_plugin::esm::reader::{EsmReader, RecordHeader, SubRecord};

const RECORD_TYPE: [u8; 4] = *b"VOLI";
const PREVIEW_RECORD_LIMIT: usize = 24;
const PREVIEW_BYTE_LIMIT: usize = 128;

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: <Fallout4.esm>"))?;
    let bytes = std::fs::read(&path)?;
    println!(
        "Surveying {} ({:.1} MiB)",
        path,
        bytes.len() as f64 / 1_048_576.0
    );

    let mut reader = EsmReader::new(&bytes);
    let _header = reader.read_file_header()?;
    let mut survey = Survey::default();
    walk_groups(&mut reader, bytes.len(), &mut survey)?;

    println!("VOLI records: {}", survey.record_count);
    println!("VOLI subrecord signatures:");
    let mut signatures: Vec<_> = survey.signatures.into_iter().collect();
    signatures.sort_by_key(|(signature, _)| signature.clone());
    for (signature, count) in signatures {
        println!("  {count:>5} × {signature}");
    }
    Ok(())
}

#[derive(Default)]
struct Survey {
    record_count: usize,
    signatures: std::collections::HashMap<String, usize>,
}

fn walk_groups(reader: &mut EsmReader, end: usize, survey: &mut Survey) -> anyhow::Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let group = reader.read_group_header()?;
            let inner_end = reader.group_content_end(&group);
            walk_groups(reader, inner_end, survey)?;
            continue;
        }

        let header = reader.read_record_header()?;
        if header.record_type == RECORD_TYPE {
            let subs = reader.read_sub_records(&header)?;
            survey.record_count += 1;
            let signature = subrecord_signature(&subs);
            *survey.signatures.entry(signature).or_default() += 1;
            if survey.record_count <= PREVIEW_RECORD_LIMIT {
                print_record(&header, &subs);
            }
        } else {
            reader.skip_record(&header);
        }
    }
    Ok(())
}

fn subrecord_signature(subs: &[SubRecord]) -> String {
    subs.iter()
        .map(|sub| {
            format!(
                "{}:{}",
                std::str::from_utf8(&sub.sub_type).unwrap_or("????"),
                sub.data.len()
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn print_record(header: &RecordHeader, subs: &[SubRecord]) {
    let editor_id = subs
        .iter()
        .find(|sub| sub.sub_type == *b"EDID")
        .map(|sub| read_zstring(&sub.data))
        .unwrap_or_default();
    println!("\n== {} (form {:08X}) ==", editor_id, header.form_id);
    for sub in subs {
        let kind = std::str::from_utf8(&sub.sub_type).unwrap_or("????");
        let preview = &sub.data[..sub.data.len().min(PREVIEW_BYTE_LIMIT)];
        println!("  {kind} len={:>5} hex={preview:02x?}", sub.data.len());
    }
}

fn read_zstring(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}
