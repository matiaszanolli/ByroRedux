//! Dump every authored `GMST` whose EditorID matches `fAVD`/Health from an
//! ESM — the sourced multiplier table for the FO3/FNV NPC Health curve
//! (#5238).
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example health_gmst_dump -- <file.esm> [...]

use byroredux_plugin::esm::reader::EsmReader;

fn main() -> anyhow::Result<()> {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path)?;
        let mut reader = EsmReader::new(&bytes);
        let _header = reader.read_file_header()?;
        let end = bytes.len();
        println!("== {path}");
        walk(&mut reader, end)?;
    }
    Ok(())
}

fn walk(reader: &mut EsmReader<'_>, end: usize) -> anyhow::Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let group = reader.read_group_header()?;
            let inner_end = reader.group_content_end(&group);
            walk(reader, inner_end)?;
            continue;
        }
        let header = reader.read_record_header()?;
        if header.record_type != *b"GMST" {
            reader.skip_record(&header);
            continue;
        }
        let subs = reader.read_sub_records(&header)?;
        let mut editor_id = String::new();
        let mut value: Option<f32> = None;
        for sub in &subs {
            match &sub.sub_type {
                b"EDID" => {
                    editor_id =
                        String::from_utf8_lossy(&sub.data).trim_end_matches('\0').to_owned();
                }
                b"DATA" | b"FLTV" if sub.data.len() >= 5 => {
                    // FNV/FO3 GMST DATA: 1 type byte + 4-byte value.
                    value = Some(f32::from_le_bytes([sub.data[1], sub.data[2], sub.data[3], sub.data[4]]));
                }
                b"DATA" | b"FLTV" if sub.data.len() >= 4 => {
                    value = Some(f32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]));
                }
                _ => {}
            }
        }
        if editor_id.starts_with("fAVD") && editor_id.to_ascii_lowercase().contains("health") {
            println!(
                "  {editor_id:42} form_id={:08X} value={value:?}",
                header.form_id
            );
        }
    }
    Ok(())
}
