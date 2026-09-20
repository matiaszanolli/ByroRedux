//! Survey which QUST stage fragments call `SetObjectiveDisplayed` /
//! `SetObjectiveCompleted` in their compiled `.pex` bodies.
//!
//! Playable-slice P3/P4 support: the objective HUD consumer
//! (`byroredux/src/objectives.rs`) draws what a fragment displayed, and the
//! P4 fixture needs a quest whose stage fragments drive objectives through
//! the canonical `quest.start` / `quest.setstage` path. This probe walks a
//! real master's QUST records, decodes each VMAD's stage→`Fragment_N` table
//! (`parse_quest_fragments` — the production path), pulls the fragment's
//! compiled bytecode out of the scripts archive, and substring-scans the
//! `.pex` string table for the objective calls.
//!
//! Usage:
//!   cargo run -p byroredux-plugin --release --example probe_objective_fragments -- \
//!     <Skyrim.esm> <Skyrim - Misc.bsa> [MAX_QUESTS]

use byroredux_bsa::BsaArchive;
use byroredux_plugin::esm::reader::EsmReader;
use byroredux_plugin::esm::records::script_instance::parse_quest_fragments;
use std::collections::HashMap;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let esm_path = args.next().expect("usage: <Skyrim.esm> <Misc.bsa> [MAX]");
    let bsa_path = args.next().expect("usage: <Skyrim.esm> <Misc.bsa> [MAX]");
    let max_quests: usize = args
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(usize::MAX);

    let bytes = std::fs::read(&esm_path)?;
    let mut reader = EsmReader::new(&bytes);
    let end = bytes.len();
    let mut quests: HashMap<u32, (String, String, Vec<(u16, String)>)> = HashMap::new();

    walk(&mut reader, end, b"QUST", &mut |form_id, subs| {
        let mut edid = String::new();
        let mut full = String::new();
        let vmad = subs.iter().find(|(t, _)| t == b"VMAD").map(|(_, d)| d);
        for (sub_type, data) in subs {
            match sub_type {
                b"EDID" => {
                    edid = String::from_utf8_lossy(&data[..data.len().min(511)]).to_string()
                }
                b"FULL" => {
                    full = String::from_utf8_lossy(&data[..data.len().min(511)]).to_string()
                }
                _ => {}
            }
        }
        let Some(vmad) = vmad else { return };
        let fragments = parse_quest_fragments(vmad);
        if fragments.is_empty() {
            return;
        }
        let stages = fragments
            .into_iter()
            .map(|fragment| (fragment.stage, fragment.script_name))
            .collect();
        quests.insert(form_id, (edid, full, stages));
    });

    let archive = BsaArchive::open(&bsa_path)?;
    let mut surveyed = 0usize;
    let mut missing_pex = 0usize;
    let mut printed = 0usize;
    let mut form_ids: Vec<_> = quests.keys().copied().collect();
    form_ids.sort();
    for form_id in form_ids {
        if surveyed >= max_quests {
            break;
        }
        surveyed += 1;
        let (edid, full, stages) = &quests[&form_id];
        for (stage, script_name) in stages {
            let path = format!("scripts\\{}.pex", script_name.to_ascii_lowercase());
            let Ok(pex) = archive.extract(&path) else {
                missing_pex += 1;
                continue;
            };
            let needle: &[u8] = b"SetObjectiveDisplayed";
            let displayed = pex.windows(needle.len()).any(|w| w == needle);
            let needle: &[u8] = b"SetObjectiveCompleted";
            let completed = pex.windows(needle.len()).any(|w| w == needle);
            if !displayed && !completed {
                continue;
            }
            printed += 1;
            println!(
                "quest 0x{form_id:08X} {edid}  full={full:?}  stage={stage}  {script_name}  \
                 displayed={displayed} completed={completed}"
            );
        }
    }
    eprintln!(
        "surveyed {surveyed} fragment-bearing quests: {printed} objective-driving stages, \
         {missing_pex} fragments absent from the archive"
    );
    Ok(())
}

/// A decompressed sub-record: its 4-byte type + owned data bytes.
type OwnedSub = ([u8; 4], Vec<u8>);

/// Recursively walk records, invoking `f` on every record of `want` type
/// with its (decompressed) sub-records. Same pattern as
/// `dump_qust_vmad_fragments`.
fn walk(reader: &mut EsmReader, end: usize, want: &[u8; 4], f: &mut dyn FnMut(u32, &[OwnedSub])) {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let Ok(g) = reader.read_group_header() else {
                return;
            };
            let sub_end = reader.group_content_end(&g);
            walk(reader, sub_end, want, f);
            continue;
        }
        let Ok(header) = reader.read_record_header() else {
            return;
        };
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
