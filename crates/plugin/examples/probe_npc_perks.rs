//! Census `NPC_` perk sub-records (`PRKZ` count + `PRKR` entries) in a
//! shipped master (#3158).
//!
//! `parse_npc_actor_values` — the only producer of `NpcRecord::perks` — is
//! gated on `GameKind::uses_actor_value_properties()`, i.e. FO4 / FO76 /
//! Starfield. Whether that gate is too narrow is a wire-format question about
//! each game's `NPC_`, and the answer has to come from the archive, not from
//! lineage reasoning. This probe answers it directly.
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example probe_npc_perks -- <ESM> [ESM ...]

use byroredux_plugin::esm::reader::{EsmReader, RecordHeader, SubRecord};

fn main() -> anyhow::Result<()> {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        anyhow::bail!("usage: probe_npc_perks <ESM> [ESM ...]");
    }

    for path in paths {
        let bytes = std::fs::read(&path)?;
        let mut reader = EsmReader::new(&bytes);
        let _header = reader.read_file_header()?;

        let mut npc_total = 0usize;
        let mut with_prkr = 0usize;
        let mut prkr_entries = 0usize;
        let mut with_prkz = 0usize;
        let mut prkr_lengths: Vec<usize> = Vec::new();
        let mut sample: Vec<String> = Vec::new();

        walk(&mut reader, bytes.len(), &mut |header, subs| {
            if &header.record_type != b"NPC_" {
                return;
            }
            npc_total += 1;
            let mut record_prkr = 0usize;
            let mut edid = String::new();
            for sub in subs {
                match &sub.sub_type {
                    b"EDID" => {
                        edid = String::from_utf8_lossy(&sub.data)
                            .trim_end_matches('\0')
                            .to_string();
                    }
                    b"PRKZ" => with_prkz += 1,
                    b"PRKR" => {
                        record_prkr += 1;
                        if !prkr_lengths.contains(&sub.data.len()) {
                            prkr_lengths.push(sub.data.len());
                        }
                    }
                    _ => {}
                }
            }
            if record_prkr > 0 {
                with_prkr += 1;
                prkr_entries += record_prkr;
                if sample.len() < 5 {
                    sample.push(format!("{edid}({record_prkr})"));
                }
            }
        })?;

        prkr_lengths.sort_unstable();
        println!(
            "{path}: NPC_={npc_total} with_PRKZ={with_prkz} with_PRKR={with_prkr} \
             PRKR_entries={prkr_entries} PRKR_sub_lengths={prkr_lengths:?}"
        );
        if !sample.is_empty() {
            println!("  sample: {}", sample.join(", "));
        }
    }
    Ok(())
}

/// Depth-first GRUP walk, invoking `visit` for every record with its
/// sub-record list.
fn walk(
    reader: &mut EsmReader,
    end: usize,
    visit: &mut impl FnMut(&RecordHeader, &[SubRecord]),
) -> anyhow::Result<()> {
    while reader.position() < end {
        let Ok(header) = reader.read_record_header() else {
            break;
        };
        if &header.record_type == b"GRUP" {
            let group_end = reader.position() + header.data_size as usize - 24;
            walk(reader, group_end.min(end), visit)?;
            continue;
        }
        let subs = reader.read_sub_records(&header).unwrap_or_default();
        visit(&header, &subs);
    }
    Ok(())
}
