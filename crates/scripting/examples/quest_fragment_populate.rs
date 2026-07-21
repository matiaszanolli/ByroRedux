//! End-to-end M47.2 keystone validation — headless, no Vulkan.
//!
//! Drives the *complete* real pipeline the cell loader runs, on real game
//! data: parse an ESM → read each QUST's `VMAD` fragment bindings
//! (stage→`Fragment_N`) → resolve + decompile the quest's compiled `.pex`
//! from a scripts archive → lower each bound fragment body to canonical
//! effects → register into [`QuestStageFragments`]. Reports how many
//! quests, bindings, and effects actually populate.
//!
//! This is the runtime analog of `fragment_coverage` (which measures the
//! lowerer over the *whole* corpus): here we only touch the fragments the
//! VMAD decoder actually binds to a stage, exactly as the engine does.
//!
//! ```bash
//! cargo run --release -p byroredux-scripting --example quest_fragment_populate -- \
//!   "<Skyrim SE>/Data/Skyrim.esm" \
//!   "<Skyrim SE>/Data/Skyrim - Misc.bsa"
//! ```

use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_scripting::{populate_quest_fragments_from_pex, QuestFormId, QuestStageFragments};

enum Archive {
    Bsa(BsaArchive),
    Ba2(Ba2Archive),
}

impl Archive {
    fn open(path: &str) -> std::io::Result<Self> {
        if path.to_ascii_lowercase().ends_with(".ba2") {
            Ok(Archive::Ba2(Ba2Archive::open(path)?))
        } else {
            Ok(Archive::Bsa(BsaArchive::open(path)?))
        }
    }
    fn extract(&self, path: &str) -> std::io::Result<Vec<u8>> {
        match self {
            Archive::Bsa(a) => a.extract(path),
            Archive::Ba2(a) => a.extract(path),
        }
    }
}

/// Mirror the engine's `pex_archive_path` normalisation.
fn pex_key(script_name: &str) -> String {
    let mut name = script_name.replace('/', "\\").to_ascii_lowercase();
    if !name.ends_with(".pex") {
        name.push_str(".pex");
    }
    if !name.starts_with("scripts\\") {
        name = format!("scripts\\{name}");
    }
    name
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let esm_path = args
        .next()
        .ok_or("usage: quest_fragment_populate ESM SCRIPTS_ARCHIVE")?;
    let scripts_path = args
        .next()
        .ok_or("usage: quest_fragment_populate ESM SCRIPTS_ARCHIVE")?;

    let bytes = std::fs::read(&esm_path)?;
    let index = byroredux_plugin::esm::parse_esm(&bytes).map_err(|e| e.to_string())?;
    let archive = Archive::open(&scripts_path)?;

    let mut frags = QuestStageFragments::default();
    let mut quests_with_bindings = 0usize;
    let mut total_bindings = 0usize;
    let mut pex_missing = 0usize;
    let mut inserted = 0usize;
    let mut vmads_registered = 0usize;
    let mut examples: Vec<String> = Vec::new();

    for (&form_id, quest) in index.quests.iter() {
        // Register the quest's own VMAD scripts-section (property table)
        // regardless of whether it also carries fragment bindings — a
        // cross-quest `Property`-targeted effect needs it to resolve.
        if let Some(vmad) = &quest.script_instance {
            let before = frags.vmad(QuestFormId(form_id)).is_some();
            frags.insert_vmad(QuestFormId(form_id), vmad.clone());
            if !before && frags.vmad(QuestFormId(form_id)).is_some() {
                vmads_registered += 1;
            }
        }
        if quest.fragments.is_empty() {
            continue;
        }
        quests_with_bindings += 1;
        total_bindings += quest.fragments.len();

        // All fragments share one QF_ script.
        let script_name = &quest.fragments[0].script_name;
        let Ok(pex) = archive.extract(&pex_key(script_name)) else {
            pex_missing += 1;
            continue;
        };
        let bindings: Vec<(u16, &str)> = quest
            .fragments
            .iter()
            .map(|f| (f.stage, f.fragment_name.as_str()))
            .collect();
        let before = frags.len();
        let n =
            populate_quest_fragments_from_pex(&mut frags, QuestFormId(form_id), &pex, &bindings);
        inserted += n;
        if n > 0 && examples.len() < 15 {
            examples.push(format!(
                "{form_id:08X} {script_name}: {n}/{} stage fragments lowered (map now {})",
                quest.fragments.len(),
                frags.len().max(before),
            ));
        }
    }

    println!("== M47.2 quest-fragment population (real pipeline) ==");
    println!("ESM:            {esm_path}");
    println!("scripts:        {scripts_path}");
    println!("scripted quests (VMAD fragment bindings): {quests_with_bindings}");
    println!("total stage→Fragment_N bindings:          {total_bindings}");
    println!("quests whose .pex was missing:            {pex_missing}");
    println!("stage fragments fully lowered + registered: {inserted}");
    println!("QuestStageFragments map size:             {}", frags.len());
    println!("quests with a registered VMAD (property table): {vmads_registered}");
    println!("-- examples --");
    for e in &examples {
        println!("  {e}");
    }
    Ok(())
}
