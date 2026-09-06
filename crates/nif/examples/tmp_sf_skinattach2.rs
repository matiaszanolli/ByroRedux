//! TEMPORARY audit scratch — per-SHAPE SkinAttach ↔ BSSkin::Instance pairing.
//!
//! Tighter than tmp_sf_skinattach: instead of matching any SkinAttach in the
//! scene, this walks each `BSGeometry`'s OWN `extra_data_refs` for a
//! `SkinAttach` and compares its bone-name count against the bone count of
//! the `BSSkin::BoneData` reached through that same shape's
//! `skin_instance_ref`. That is the structural link a fix would use.

use byroredux_bsa::Ba2Archive;
use byroredux_nif::blocks::extra_data::NiExtraData;
use byroredux_nif::blocks::skin::{BsSkinBoneData, BsSkinInstance};
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

    let mut shapes_with_skin = 0usize;
    let mut skin_allnull = 0usize;
    let mut allnull_own_attach = 0usize;
    let mut allnull_own_attach_match = 0usize;
    let mut allnull_own_attach_mismatch = 0usize;
    let mut nonnull_own_attach_match = 0usize;
    let mut nonnull_own_attach_disagree = 0usize;
    let mut disagree_samples: Vec<String> = Vec::new();

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

            for block in &scene.blocks {
                let Some(geom) = block
                    .as_any()
                    .downcast_ref::<byroredux_nif::blocks::bs_geometry::BSGeometry>()
                else {
                    continue;
                };
                let Some(si) = geom.skin_instance_ref.index() else {
                    continue;
                };
                let Some(inst) = scene
                    .blocks
                    .get(si)
                    .and_then(|b| b.as_any().downcast_ref::<BsSkinInstance>())
                else {
                    continue;
                };
                shapes_with_skin += 1;

                // This shape's OWN SkinAttach, via its own extra_data_refs.
                let mut own: Option<&Vec<String>> = None;
                for r in &geom.av.net.extra_data_refs {
                    let Some(i) = r.index() else { continue };
                    let Some(ed) = scene
                        .blocks
                        .get(i)
                        .and_then(|b| b.as_any().downcast_ref::<NiExtraData>())
                    else {
                        continue;
                    };
                    if ed.type_name == "SkinAttach" {
                        own = ed.skin_attach_bones.as_ref();
                        break;
                    }
                }

                // Authoritative bone count: BSSkin::BoneData through the instance.
                let bone_count = inst
                    .bone_data_ref
                    .index()
                    .and_then(|i| scene.blocks.get(i))
                    .and_then(|b| b.as_any().downcast_ref::<BsSkinBoneData>())
                    .map(|bd| bd.bones.len())
                    .unwrap_or(inst.bone_refs.len());

                let all_null = !inst.bone_refs.is_empty()
                    && inst.bone_refs.iter().all(|r| r.index().is_none());

                if all_null {
                    skin_allnull += 1;
                    if let Some(names) = own {
                        allnull_own_attach += 1;
                        if names.len() == bone_count {
                            allnull_own_attach_match += 1;
                        } else {
                            allnull_own_attach_mismatch += 1;
                            if disagree_samples.len() < 8 {
                                disagree_samples.push(format!(
                                    "COUNT_MISMATCH {f} attach={} bones={bone_count}",
                                    names.len()
                                ));
                            }
                        }
                    }
                } else if let Some(names) = own {
                    // Cross-check: where bone_refs DO resolve, do the resolved
                    // node names agree with the SkinAttach list, in order?
                    let resolved: Vec<Option<String>> = inst
                        .bone_refs
                        .iter()
                        .map(|r| {
                            r.index()
                                .and_then(|i| scene.blocks.get(i))
                                .and_then(|b| b.as_any().downcast_ref::<byroredux_nif::blocks::node::NiNode>())
                                .and_then(|n| n.av.net.name.as_deref().map(|s| s.to_string()))
                        })
                        .collect();
                    if names.len() == resolved.len()
                        && resolved
                            .iter()
                            .zip(names.iter())
                            .all(|(a, b)| a.as_deref() == Some(b.as_str()))
                    {
                        nonnull_own_attach_match += 1;
                    } else {
                        nonnull_own_attach_disagree += 1;
                        if disagree_samples.len() < 8 {
                            disagree_samples.push(format!(
                                "ORDER_DISAGREE {f} attach={:?} resolved={:?}",
                                &names[..names.len().min(5)],
                                &resolved[..resolved.len().min(5)]
                            ));
                        }
                    }
                }
            }
        }
        println!("[{name}] done");
    }

    println!("---");
    println!("BSGeometry shapes with a skin instance      = {shapes_with_skin}");
    println!("  skin has ALL-NULL bone_refs               = {skin_allnull}");
    println!("    shape's OWN extra_data has SkinAttach   = {allnull_own_attach}");
    println!("      names.len() == BoneData bone count    = {allnull_own_attach_match}");
    println!("      count mismatch                        = {allnull_own_attach_mismatch}");
    println!("  skin bone_refs resolve (control group)    = {}", shapes_with_skin - skin_allnull);
    println!("    OWN SkinAttach agrees in ORDER + name   = {nonnull_own_attach_match}");
    println!("    OWN SkinAttach disagrees                = {nonnull_own_attach_disagree}");
    for s in &disagree_samples {
        println!("SAMPLE {s}");
    }
}
