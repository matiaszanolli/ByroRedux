//! Throwaway research probe (M42 seat assignment): dump raw PACK subrecords
//! (PKDT/PSDT/PLDT/PTDT/CTDA) for named packages so the PSDT schedule + PLDT
//! location byte layout can be confirmed against real FNV bytes before a parser
//! lands. Usage: cargo run -p byroredux-plugin --example probe_pack_subs -- <ESM> [edid-substr...]
use byroredux_plugin::esm::reader::{EsmReader, RecordHeader, SubRecord};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or_else(|| anyhow::anyhow!("usage: <ESM> [substr...]"))?;
    let filters: Vec<String> = {
        let r: Vec<String> = args.collect();
        if r.is_empty() { vec!["GSPGSaloonSandbox".into(), "GSTrudyAtBar".into(), "GSTrudyEvening".into(), "GSRingoAtSaloon".into()] } else { r }
    };
    let bytes = std::fs::read(&path)?;
    let mut reader = EsmReader::new(&bytes);
    let _ = reader.read_file_header()?;
    let total = bytes.len();
    walk(&mut reader, total, &filters)?;
    Ok(())
}

fn walk(reader: &mut EsmReader, end: usize, filters: &[String]) -> anyhow::Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let g = reader.read_group_header()?;
            let inner = reader.group_content_end(&g);
            walk(reader, inner, filters)?;
        } else {
            let h = reader.read_record_header()?;
            if h.record_type == *b"PACK" {
                let subs = reader.read_sub_records(&h)?;
                let edid = subs.iter().find(|s| s.sub_type == *b"EDID")
                    .map(|s| { let e = s.data.iter().position(|&c| c==0).unwrap_or(s.data.len()); String::from_utf8_lossy(&s.data[..e]).to_string() })
                    .unwrap_or_default();
                if filters.iter().any(|f| edid.contains(f.as_str())) { dump(&h, &edid, &subs); }
            } else {
                reader.skip_record(&h);
            }
        }
    }
    Ok(())
}

fn dump(h: &RecordHeader, edid: &str, subs: &[SubRecord]) {
    println!("\n== {} (form {:08X}) ==", edid, h.form_id);
    for s in subs {
        let k = std::str::from_utf8(&s.sub_type).unwrap_or("????");
        if matches!(&s.sub_type, b"PKDT" | b"PSDT" | b"PLDT" | b"PTDT" | b"CTDA" | b"IDLA" | b"PKPT") {
            println!("  {} len={:>3} hex={:02x?}", k, s.data.len(), &s.data[..s.data.len().min(24)]);
        }
    }
}
