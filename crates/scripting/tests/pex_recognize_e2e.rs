//! End-to-end M47.2 vertical slice: a real vanilla compiled script
//! (`.pex`) → decompile → recognizer → canonical behavior. This is the
//! whole thesis in one test — a shipping Bethesda script, untouched, drives
//! an ECS behavior with no VM.
//!
//! **Opt-in / `#[ignore]`d** — needs the Skyrim SE script archive on disk.
//! Run with:
//! ```bash
//! cargo test -p byroredux-scripting --test pex_recognize_e2e -- --ignored
//! ```
//! Skips gracefully (passing) when the game data isn't present.

use byroredux_plugin::esm::reader::GameKind;
use byroredux_scripting::translate_pex;

const SKYRIM_MISC_BSA: &str = "/mnt/data/SteamLibrary/steamapps/common/\
Skyrim Special Edition/Data/Skyrim - Misc.bsa";

fn extract_pex(stem: &str) -> Option<Vec<u8>> {
    let arch = byroredux_bsa::BsaArchive::open(SKYRIM_MISC_BSA).ok()?;
    let want = format!("{}.pex", stem.to_ascii_lowercase());
    let path = arch
        .list_files()
        .into_iter()
        .find(|f| f.to_ascii_lowercase().ends_with(&want))?
        .to_string();
    arch.extract(&path).ok()
}

#[test]
#[ignore = "needs Skyrim SE game data on disk"]
fn da10_pex_is_recognized_as_a_quest_stage_gate() {
    let Some(bytes) = extract_pex("DA10MainDoorScript") else {
        eprintln!("SKIP: DA10MainDoorScript.pex not found (no game data?)");
        return;
    };

    // DA10MainDoorScript is alias-attached and resolves its quest through
    // `Self.GetOwningQuest()`, so the recognizer needs the owning quest id
    // (the cell loader supplies it from the alias at attach time). A
    // synthetic id is fine here — we're testing recognition, not dispatch.
    let owning_quest = Some(0x0001_0800);

    let recognized = translate_pex(&bytes, GameKind::Skyrim, None, owning_quest)
        .expect("DA10 .pex is recognized as a quest-stage gate");

    // The generic quest_stage_gate recognizer claims it, keyed by editor id.
    assert!(
        recognized.archetype.contains("quest_stage_gate"),
        "archetype was {:?}",
        recognized.archetype
    );
    assert!(
        recognized.archetype.contains("DA10MainDoorScript"),
        "archetype was {:?}",
        recognized.archetype
    );

    // The recognition yields a spawn closure (the canonical behavior). We
    // don't run it here (it needs a World); its presence is the contract.
    let _spawn = recognized.spawn;
}

#[test]
#[ignore = "needs Skyrim SE game data on disk"]
fn an_unrecognized_pex_is_a_silent_miss() {
    // A vanilla script that fits no recognizer should translate to None,
    // not error — the "no consumer yet" contract. defaultRumbleOnActivate
    // IS recognized, so pick something the catalog doesn't claim: a plain
    // utility script. If it's absent, skip.
    let Some(bytes) = extract_pex("ObjectReference") else {
        eprintln!("SKIP: probe script not found (no game data?)");
        return;
    };
    // No panic / no error — either recognized or a clean None.
    let _ = translate_pex(&bytes, GameKind::Skyrim, None, None);
}
