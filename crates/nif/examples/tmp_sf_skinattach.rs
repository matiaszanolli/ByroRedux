//! TEMPORARY audit scratch — does `SkinAttach` carry the bone names that
//! #3549's geometric solver has to recover? (delete after use)
//!
//! For every Starfield NIF: find `BSSkin::Instance` blocks, split them by
//! whether every `bone_refs` entry is NULL (the #3549 population), and check
//! whether the same scene carries a `SkinAttach` extra-data block whose
//! `skin_attach_bones` list is present and matches the skin's bone count.

use byroredux_bsa::Ba2Archive;
use byroredux_nif::parse_nif;

const ARCHIVES: &[&str] = &[
    "Starfield - Meshes01.ba2",
    "Starfield - MeshesPatch.ba2",
    "Starfield - FaceMeshes.ba2",
    "ShatteredSpace - Main01.ba2",
];

fn main() {
    let base = std::env::var("BYROREDUX_STARFIELD_DATA").unwrap_or_else(|_| {
        "/mnt/data/SteamLibrary/steamapps/common/Starfield/Data".to_string()
    });

    let mut skins = 0usize;
    let mut skins_all_null = 0usize;
    let mut all_null_with_attach = 0usize;
    let mut all_null_attach_count_match = 0usize;
    let mut nonnull_with_attach = 0usize;
    let mut attach_blocks = 0usize;
    let mut attach_with_names = 0usize;
    let mut bonetrans_blocks = 0usize;
    let mut bonetrans_with_payload = 0usize;
    let mut samples: Vec<String> = Vec::new();
    let mut multi_attach = 0usize;
    let mut multi_skin = 0usize;

    for name in ARCHIVES {
        let path = std::path::PathBuf::from(&base).join(name);
        let Ok(archive) = Ba2Archive::open(&path) else {
            eprintln!("[skip] {name}");
            continue;
        };
        let list: Vec<String> = archive
            .list_files()
            .into_iter()
            .filter(|f| byroredux_nif::corpus::is_nif_entry(f))
            .map(|s| s.to_string())
            .collect();
        for f in &list {
            let Ok(data) = archive.extract(f) else { continue };
            let Ok(scene) = parse_nif(&data) else { continue };

            // Collect this scene's SkinAttach bone lists.
            let mut attach_lists: Vec<usize> = Vec::new();
            let mut attach_names: Vec<Vec<String>> = Vec::new();
            for block in &scene.blocks {
                let Some(ed) = block
                    .as_any()
                    .downcast_ref::<byroredux_nif::blocks::extra_data::NiExtraData>()
                else {
                    continue;
                };
                if ed.type_name == "SkinAttach" {
                    attach_blocks += 1;
                    if let Some(b) = &ed.skin_attach_bones {
                        attach_with_names += 1;
                        attach_lists.push(b.len());
                        attach_names.push(b.clone());
                    }
                } else if ed.type_name == "BoneTranslations" {
                    bonetrans_blocks += 1;
                    if ed.bone_translations.as_ref().is_some_and(|v| !v.is_empty()) {
                        bonetrans_with_payload += 1;
                    }
                }
            }

            for block in &scene.blocks {
                let Some(inst) = block
                    .as_any()
                    .downcast_ref::<byroredux_nif::blocks::skin::BsSkinInstance>()
                else {
                    continue;
                };
                skins += 1;
                if scene.blocks.iter().filter(|b| b.as_any().downcast_ref::<byroredux_nif::blocks::skin::BsSkinInstance>().is_some()).count() > 1 { multi_skin += 1; }
                let all_null = !inst.bone_refs.is_empty()
                    && inst.bone_refs.iter().all(|r| r.index().is_none());
                if all_null {
                    skins_all_null += 1;
                    if !attach_lists.is_empty() {
                        all_null_with_attach += 1;
                        if attach_lists.iter().any(|n| *n == inst.bone_refs.len()) {
                            all_null_attach_count_match += 1;
                            if attach_lists.len() > 1 { multi_attach += 1; }
                            if samples.len() < 6 {
                                let nm = &attach_names[0];
                                samples.push(format!(
                                    "NAMES {f} n={} first8={:?}", nm.len(),
                                    &nm[..nm.len().min(8)]));
                            }
                        } else if samples.len() < 10 {
                            samples.push(format!(
                                "MISMATCH {name}|{f} bones={} attach_lens={:?}",
                                inst.bone_refs.len(),
                                attach_lists
                            ));
                        }
                    } else if samples.len() < 10 {
                        samples.push(format!(
                            "NO_ATTACH {name}|{f} bones={}",
                            inst.bone_refs.len()
                        ));
                    }
                } else if !attach_lists.is_empty() {
                    nonnull_with_attach += 1;
                }
            }
        }
        println!("[{name}] done");
    }

    println!("---");
    println!("BSSkin::Instance blocks           = {skins}");
    println!("  with ALL-NULL bone_refs         = {skins_all_null}");
    println!("    ...scene has a SkinAttach     = {all_null_with_attach}");
    println!("    ...and its len == bone count  = {all_null_attach_count_match}");
    println!("  with resolvable bone_refs       = {}", skins - skins_all_null);
    println!("    ...scene has a SkinAttach     = {nonnull_with_attach}");
    println!("SkinAttach blocks                 = {attach_blocks}");
    println!("  with a decoded bone-name list   = {attach_with_names}");
    println!("BoneTranslations blocks           = {bonetrans_blocks}");
    println!("  with a decoded payload          = {bonetrans_with_payload}");
    println!("scenes-with->1-attach (of matched)   = {multi_attach}");
    println!("skin instances in multi-skin scenes  = {multi_skin}");
    for s in &samples {
        println!("SAMPLE {s}");
    }
}
