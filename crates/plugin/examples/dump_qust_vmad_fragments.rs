//! Derivation tool (M47.2 keystone): dump the trailing *fragment* section
//! of QUST `VMAD` sub-records from a real ESM so the stage→`Fragment_N`
//! table layout can be reverse-derived empirically — the same accepted
//! method the scripts-section decoder was built with (no public byte-spec
//! in-repo; no-guessing policy → derive from ground truth, don't fabricate).
//!
//! For each QUST that carries a VMAD, prints: the decoded script names
//! (from `ScriptInstanceData::parse_with_consumed`), the byte offset where
//! the scripts section ends, and a hexdump of everything after it (the
//! fragment section). Cross-validate the decoded `Fragment_N` / filename
//! strings against Champollion-decompiled quest `.pex`.
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example dump_qust_vmad_fragments -- <ESM> [MAX]

use byroredux_plugin::esm::reader::EsmReader;
use byroredux_plugin::esm::records::script_instance::{parse_quest_fragments, ScriptInstanceData};

fn hexdump(bytes: &[u8]) {
    for (i, chunk) in bytes.chunks(16).enumerate() {
        let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
        let ascii: String = chunk
            .iter()
            .map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { '.' })
            .collect();
        println!("    {:04x}  {:<47}  {}", i * 16, hex.join(" "), ascii);
    }
}

/// A decompressed sub-record: its 4-byte type + owned data bytes.
type OwnedSub = ([u8; 4], Vec<u8>);

/// Recursively walk records, invoking `f` on every record of `want` type
/// with its (decompressed) sub-records.
fn walk(reader: &mut EsmReader, end: usize, want: &[u8; 4], f: &mut dyn FnMut(u32, &[OwnedSub])) {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let Ok(g) = reader.read_group_header() else { return };
            let sub_end = reader.group_content_end(&g);
            walk(reader, sub_end, want, f);
            continue;
        }
        let Ok(header) = reader.read_record_header() else { return };
        if &header.record_type == want {
            match reader.read_sub_records(&header) {
                Ok(subs) => {
                    let owned: Vec<OwnedSub> =
                        subs.into_iter().map(|s| (s.sub_type, s.data)).collect();
                    f(header.form_id, &owned);
                }
                Err(_) => continue,
            }
        } else {
            reader.skip_record(&header);
        }
    }
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let esm_path = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: dump_qust_vmad_fragments ESM [MAX]"))?;
    let max: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(20);

    let bytes = std::fs::read(&esm_path)?;
    let mut reader = EsmReader::new(&bytes);
    let end = bytes.len();

    let mut shown = 0usize;
    let mut total_qust = 0usize;
    let mut with_vmad = 0usize;
    let mut ver_hist: std::collections::BTreeMap<u8, usize> = std::collections::BTreeMap::new();

    walk(&mut reader, end, b"QUST", &mut |form_id, subs| {
        total_qust += 1;
        let Some((_, vmad)) = subs.iter().find(|(t, _)| t == b"VMAD") else {
            return;
        };
        with_vmad += 1;

        let (scripts, consumed) = ScriptInstanceData::parse_with_consumed(vmad);
        let frag = &vmad[consumed.min(vmad.len())..];
        // Aggregate over EVERY qust-with-vmad: fragment-header version byte
        // distribution (hardening the decoder — confirm version 2 universal).
        if let Some(&ver) = frag.first() {
            *ver_hist.entry(ver).or_insert(0) += 1;
        }
        if shown >= max {
            return;
        }
        shown += 1;

        println!("========================================================");
        println!(
            "QUST {form_id:08X}  vmad={} B  ver={} objFmt={}  scripts={}  scripts_end@{}  fragBytes={}",
            vmad.len(),
            scripts.version,
            scripts.object_format,
            scripts.scripts.len(),
            consumed,
            frag.len()
        );
        for s in &scripts.scripts {
            println!("    script: {:?}  ({} props)", s.name, s.properties.len());
        }
        if frag.is_empty() {
            println!("    (no trailing fragment bytes)");
        } else {
            println!("    -- fragment section (post-scripts) --");
            hexdump(frag);
            println!("    -- decoded bindings --");
            for b in parse_quest_fragments(vmad) {
                println!(
                    "      stage {:>3} -> {}::{}",
                    b.stage, b.script_name, b.fragment_name
                );
            }
        }
    });

    eprintln!(
        "[dump_qust_vmad_fragments] {esm_path}: {total_qust} QUST, {with_vmad} with VMAD, {shown} shown"
    );
    eprintln!("[dump_qust_vmad_fragments] fragment-header version byte histogram: {ver_hist:?}");
    Ok(())
}
