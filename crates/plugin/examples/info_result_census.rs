//! M48 Phase 0 (#6643, GUI-PAGE-002 / GUI-REG-006) — grep the INFO result scripts of an ObScript-era master.
//!
//! The `obscript_tools dump grep:` probe reads `SCPT` records only, and the vanilla menu calls
//! (`ShowRaceMenu`, `MenuMode 1004`, ...) mostly live in dialogue result scripts. This walks every `DIAL`'s
//! `INFO` and prints each whose Begin/End result script source contains the needle, with its topic, owning
//! quest and `INFO` FormID, plus a per-needle count.
//!
//! ```bash
//! cargo run --release -p byroredux-plugin --example info_result_census -- <ESM> <needle> [needle ...]
//! ```
//! Needles match case-insensitively. Typical: `ShowRaceMenu`, `ShowTraitMenu`, `MenuMode`.

use byroredux_plugin::esm::parse_esm;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or_else(|| anyhow::anyhow!("usage: info_result_census <ESM> <needle> ..."))?;
    let needles: Vec<String> = args.map(|n| n.to_ascii_lowercase()).collect();
    anyhow::ensure!(!needles.is_empty(), "give at least one needle");
    let index = parse_esm(&std::fs::read(&path)?)?;
    let mut counts = vec![0usize; needles.len()];
    let mut topics: Vec<_> = index.dialogues.values().collect();
    topics.sort_by_key(|d| d.form_id);
    let (mut infos, mut with_script, mut with_source) = (0usize, 0usize, 0usize);
    for dial in topics {
        for info in &dial.infos {
            infos += 1;
            for (which, script) in [("begin", &info.result_script_begin), ("end", &info.result_script_end)] {
                if script.is_some() {
                    with_script += 1;
                }
                let Some(source) = script.as_ref().and_then(|s| s.source.as_deref()) else { continue };
                with_source += 1;
                let lower = source.to_ascii_lowercase();
                for (i, needle) in needles.iter().enumerate() {
                    if lower.contains(needle) {
                        counts[i] += 1;
                        let line = source.lines().find(|l| l.to_ascii_lowercase().contains(needle)).unwrap_or("").trim();
                        println!(
                            "{needle}: INFO {:08X} ({which}) topic {:08X} {:?} quests {:08X?}: {line}",
                            info.form_id, dial.form_id, dial.editor_id, dial.quest_refs
                        );
                    }
                }
            }
        }
    }
    println!("-- {infos} INFOs scanned; {with_script} result scripts, {with_source} with SCTX source");
    for (needle, n) in needles.iter().zip(counts) {
        println!("-- {needle}: {n} result scripts");
    }
    Ok(())
}
