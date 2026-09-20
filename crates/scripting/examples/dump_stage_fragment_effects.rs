//! Dump the lowered fragment effects the production pipeline stores for one
//! quest's stages — the exact `QuestStageFragments` rows
//! `quest.setstage` dispatches. Playable-slice P4 fixture support: shows
//! what a stage fragment *actually does* (per-stage, not per-script string
//! scans) before freezing a fixture on it.
//!
//! Usage:
//!   cargo run -p byroredux-scripting --release --example dump_stage_fragment_effects -- \
//!     <Skyrim.esm> <Skyrim - Misc.bsa> <quest formid hex> [stage]

use byroredux_bsa::BsaArchive;
use byroredux_core::ecs::World;
use byroredux_scripting::{populate_quest_fragments_from_pex, QuestFormId, QuestStageFragments};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let esm_path = args.next().expect("usage: <ESM> <scripts bsa> <quest hex> [stage]");
    let bsa_path = args.next().expect("usage: <ESM> <scripts bsa> <quest hex> [stage]");
    let quest_raw = args.next().expect("usage: <ESM> <scripts bsa> <quest hex> [stage]");
    let quest_id = u32::from_str_radix(quest_raw.trim_start_matches("0x"), 16)?;
    let only_stage: Option<u16> = args
        .next()
        .and_then(|s| s.parse().ok());

    let bytes = std::fs::read(&esm_path)?;
    let index = byroredux_plugin::esm::parse_esm(&bytes)?;
    let quest = index
        .quests
        .get(&quest_id)
        .ok_or_else(|| format!("quest {quest_id:08X} not in index"))?;
    println!(
        "quest {quest_id:08X} ({}) — {} fragment bindings, script_instance={}",
        quest.editor_id,
        quest.fragments.len(),
        quest.script_instance.is_some(),
    );

    let mut by_script: std::collections::HashMap<&str, Vec<(u16, &str)>> =
        std::collections::HashMap::new();
    for f in &quest.fragments {
        by_script
            .entry(f.script_name.as_str())
            .or_default()
            .push((f.stage, f.fragment_name.as_str()));
    }

    let archive = BsaArchive::open(&bsa_path)?;
    let mut world = World::new();
    world.insert_resource(QuestStageFragments::default());
    for (script_name, bindings) in &by_script {
        let path = format!("scripts\\{}.pex", script_name.to_ascii_lowercase());
        let pex = archive
            .extract(&path)
            .map_err(|e| format!("{script_name}: {e}"))?;
        let bindings: Vec<(u16, &str)> = bindings
            .iter()
            .filter(|(stage, _)| only_stage.is_none_or(|want| *stage == want))
            .copied()
            .collect();
        let inserted = populate_quest_fragments_from_pex(
            &mut world.resource_mut::<QuestStageFragments>(),
            QuestFormId(quest_id),
            &pex,
            &bindings,
        );
        println!("  {script_name}: {inserted}/{} bindings lowered", bindings.len());
    }

    let frags = world.resource::<QuestStageFragments>();
    for fragment in &quest.fragments {
        let (stage, name) = (fragment.stage, fragment.fragment_name.as_str());
        if only_stage.is_some_and(|want| stage != want) {
            continue;
        }
        match frags.get(QuestFormId(quest_id), stage) {
            Some(effects) => {
                println!("stage {stage} ({}):", name);
                for effect in effects {
                    println!("    {effect:?}");
                }
            }
            None => println!("stage {stage} ({}): NOT LOWERED", name),
        }
    }
    Ok(())
}
