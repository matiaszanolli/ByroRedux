//! One-off diagnostic: dump every raw sub-record of a single `QUST`
//! record by FormID, so an unexpected/unimplemented sub-type inside an
//! alias block can be spotted directly (companion to `qust_alias_survey`,
//! which only sees what the parser already decodes).
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example qust_alias_rawdump -- <ESM> <hex form_id>

use byroredux_plugin::esm::reader::EsmReader;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let esm_path = args.next().expect("usage: qust_alias_rawdump <ESM> <hex form_id>");
    let target = u32::from_str_radix(
        args.next().expect("need a hex form_id").trim_start_matches("0x"),
        16,
    )?;

    let bytes = std::fs::read(&esm_path)?;
    let mut reader = EsmReader::new(&bytes);
    let end = bytes.len();
    walk(&mut reader, end, target);
    Ok(())
}

fn walk(reader: &mut EsmReader, end: usize, target: u32) {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let Ok(g) = reader.read_group_header() else { return };
            let sub_end = reader.group_content_end(&g);
            walk(reader, sub_end, target);
            continue;
        }
        let Ok(header) = reader.read_record_header() else { return };
        if &header.record_type == b"QUST" && header.form_id == target {
            if let Ok(subs) = reader.read_sub_records(&header) {
                println!("QUST {:08X} — {} sub-records", header.form_id, subs.len());
                for s in &subs {
                    let name = std::str::from_utf8(&s.sub_type).unwrap_or("????");
                    let hex: Vec<String> = s.data.iter().take(24).map(|b| format!("{b:02x}")).collect();
                    println!("  {name:>4}  len={:<4}  {}", s.data.len(), hex.join(" "));
                }
            }
            return;
        }
        reader.skip_record(&header);
    }
}
