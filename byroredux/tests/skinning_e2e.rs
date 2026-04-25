//! M29 — End-to-end skinning chain verification on real game content.
//!
//! Two fixture paths exercise different parts of the skinning pipeline:
//!
//! - **FNV `meshes\characters\_male\upperbody.nif`** — `NiTriShape` +
//!   `NiSkinInstance` + `NiSkinData` (the legacy / Gamebryo-2.x skin
//!   layout). End-to-end: bones populate, names round-trip the
//!   `node_by_name` lookup, partition-local → global bone-index remap
//!   is correct, per-vertex `vertex_bone_indices` + `vertex_bone_weights`
//!   reach the importer in bounds, and the palette responds to bone
//!   transform mutations.
//!
//! - **SSE `meshes\actors\character\character assets\malebody_0.nif`** —
//!   `BSTriShape` + `BSSkinInstance` + `BSSkinBoneData` +
//!   `SseSkinGlobalBuffer` (the FO4+ / Skyrim SE skin layout). #638
//!   closed the global-buffer skin-payload gap: `decode_sse_packed_buffer`
//!   now decodes the 12-byte VF_SKINNED block instead of skipping it,
//!   and `extract_skin_bs_tri_shape` falls back to those decoded values
//!   when the inline arrays are empty. The four assertions here pin
//!   bones, names, palette logic, and per-vertex bounds — all live
//!   regressions, no soft flags.
//!
//! All tests are `#[ignore]` — fixtures live in proprietary BSAs that
//! can't ship in the repo. Opt in with
//! `cargo test -p byroredux --test skinning_e2e -- --ignored`.

use byroredux_bsa::BsaArchive;
use byroredux_core::ecs::components::skinned_mesh::{SkinnedMesh, MAX_BONES_PER_MESH};
use byroredux_core::math::{Mat4, Quat, Vec3};
use byroredux_nif::import::{import_nif_scene, ImportedSkin};
use byroredux_nif::parse_nif;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

const SSE_DEFAULT_DATA: &str =
    "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data";
const SSE_MESH_BSA: &str = "Skyrim - Meshes0.bsa";
const SSE_FIXTURE_NIF: &str = "meshes\\actors\\character\\character assets\\malebody_0.nif";

const FNV_DEFAULT_DATA: &str =
    "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data";
const FNV_MESH_BSA: &str = "Fallout - Meshes.bsa";
const FNV_FIXTURE_NIF: &str = "meshes\\characters\\_male\\upperbody.nif";

fn data_dir(env_var: &str, default: &str) -> Option<PathBuf> {
    if let Ok(val) = std::env::var(env_var) {
        let path = PathBuf::from(val);
        if path.is_dir() {
            return Some(path);
        }
    }
    let default_path = PathBuf::from(default);
    if default_path.is_dir() {
        Some(default_path)
    } else {
        None
    }
}

/// Captured node-name set + the largest skinned mesh's `ImportedSkin`.
/// The set comes from the imported scene's node hierarchy (the same
/// nodes scene assembly spawns as bone entities) — extracting just
/// the names avoids cloning `ImportedScene` (which is non-Clone).
struct Fixture {
    node_names: Vec<Arc<str>>,
    skin: ImportedSkin,
}

/// Open a BSA + extract a fixture NIF + run the importer. Picks the
/// *largest* skinned mesh (most positions). Sub-meshes with small bone
/// counts (e.g. 'meathead01' on FNV upperbody) are attached but the
/// body / torso carries the gameplay-relevant skin.
fn load_fixture(
    env_var: &str,
    default: &str,
    bsa_name: &str,
    nif_path: &str,
) -> Option<Fixture> {
    let data = data_dir(env_var, default).or_else(|| {
        eprintln!("[M29] skipping: no data dir (set {env_var} or install to {default})");
        None
    })?;
    let bsa_path = data.join(bsa_name);
    let bsa = BsaArchive::open(&bsa_path)
        .map_err(|e| eprintln!("[M29] skipping: failed to open {bsa_path:?}: {e}"))
        .ok()?;
    let bytes = bsa
        .extract(nif_path)
        .map_err(|e| eprintln!("[M29] skipping: extract {nif_path} failed: {e}"))
        .ok()?;
    let scene = parse_nif(&bytes).expect("parse_nif must succeed on canonical fixture");
    let imported = import_nif_scene(&scene);

    let node_names: Vec<Arc<str>> = imported
        .nodes
        .iter()
        .filter_map(|n| n.name.clone())
        .collect();

    let mut best: Option<(&ImportedSkin, usize)> = None;
    for mesh in imported.meshes.iter() {
        if let Some(skin) = mesh.skin.as_ref() {
            let pos = mesh.positions.len();
            if best.map_or(true, |(_, p)| pos > p) {
                best = Some((skin, pos));
            }
        }
    }
    let (skin, _pos) = best?;
    Some(Fixture {
        node_names,
        skin: skin.clone(),
    })
}

/// Mirror the resolution loop at `byroredux/src/scene.rs:1283-1322`:
/// build a `node_by_name` index from the captured node-name set and
/// resolve each bone name against it.
fn resolve_bones(fixture: &Fixture) -> (usize, usize) {
    let mut by_name: HashMap<&str, ()> = HashMap::new();
    for name in &fixture.node_names {
        by_name.insert(name.as_ref(), ());
    }
    let mut resolved = 0usize;
    let mut unresolved = 0usize;
    for bone in &fixture.skin.bones {
        if by_name.contains_key(bone.name.as_ref()) {
            resolved += 1;
        } else {
            unresolved += 1;
        }
    }
    (resolved, unresolved)
}

// ── FNV path: NiTriShape + NiSkinInstance + NiSkinData (working) ────

#[test]
#[ignore = "requires FNV BSA — opt in with --ignored"]
fn fnv_imports_skinned_mesh_with_resolved_bones() {
    let Some(fixture) = load_fixture(
        "BYROREDUX_FNV_DATA",
        FNV_DEFAULT_DATA,
        FNV_MESH_BSA,
        FNV_FIXTURE_NIF,
    ) else {
        return;
    };
    let skin = &fixture.skin;
    assert!(!skin.bones.is_empty(), "FNV upperbody must carry bones");
    let mut names: Vec<&str> = skin.bones.iter().map(|b| &*b.name).collect();
    names.sort();
    eprintln!(
        "[M29 FNV] {} bones, root={:?}, sample: {:?}",
        skin.bones.len(),
        skin.skeleton_root,
        &names[..names.len().min(6)]
    );
    let (resolved, unresolved) = resolve_bones(&fixture);
    let _ = unresolved;
    let rate = resolved as f64 / skin.bones.len() as f64;
    eprintln!(
        "[M29 FNV] resolution: {} / {} ({:.1}%)",
        resolved,
        skin.bones.len(),
        rate * 100.0
    );
    assert!(
        rate >= 0.80,
        "FNV bone resolution rate {:.1}% < 80% — likely a name-encoding regression",
        rate * 100.0
    );
}

#[test]
#[ignore = "requires FNV BSA — opt in with --ignored"]
fn fnv_vertex_indices_within_palette_bounds() {
    let Some(fixture) = load_fixture(
        "BYROREDUX_FNV_DATA",
        FNV_DEFAULT_DATA,
        FNV_MESH_BSA,
        FNV_FIXTURE_NIF,
    ) else {
        return;
    };
    let skin = &fixture.skin;
    assert!(
        !skin.vertex_bone_indices.is_empty(),
        "FNV NiTriShape skin path must populate vertex_bone_indices"
    );
    assert_eq!(
        skin.vertex_bone_indices.len(),
        skin.vertex_bone_weights.len()
    );
    let bone_count = skin.bones.len() as u16;
    assert!(bone_count <= MAX_BONES_PER_MESH as u16);
    let mut max_index = 0u16;
    for (vi, indices) in skin.vertex_bone_indices.iter().enumerate() {
        for (slot, &idx) in indices.iter().enumerate() {
            assert!(
                idx < bone_count || skin.vertex_bone_weights[vi][slot] == 0.0,
                "FNV vertex {} slot {} has bone {} >= bone_count {} (weight={})",
                vi,
                slot,
                idx,
                bone_count,
                skin.vertex_bone_weights[vi][slot]
            );
            if skin.vertex_bone_weights[vi][slot] > 0.0 {
                max_index = max_index.max(idx);
            }
        }
    }
    eprintln!(
        "[M29 FNV] {} vertices, max active bone index = {} (of {})",
        skin.vertex_bone_indices.len(),
        max_index,
        bone_count
    );
    assert!(max_index > 0, "FNV vertices all pinned to bone 0 — partition decode regression");
}

#[test]
#[ignore = "requires FNV BSA — opt in with --ignored"]
fn fnv_palette_responds_to_bone_transform() {
    let Some(fixture) = load_fixture(
        "BYROREDUX_FNV_DATA",
        FNV_DEFAULT_DATA,
        FNV_MESH_BSA,
        FNV_FIXTURE_NIF,
    ) else {
        return;
    };
    let skin = &fixture.skin;
    let bone_count = skin.bones.len();
    let bones: Vec<Option<u32>> = (0..bone_count as u32).map(Some).collect();
    let binds: Vec<Mat4> = skin
        .bones
        .iter()
        .map(|b| Mat4::from_cols_array_2d(&b.bind_inverse))
        .collect();
    let sm = SkinnedMesh::new(None, bones, binds);

    let baseline = sm.compute_palette(|_| Some(Mat4::IDENTITY));
    let target = 1u32;
    let mutated_world = Mat4::from_rotation_translation(
        Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
        Vec3::new(5.0, 0.0, 0.0),
    );
    let mutated = sm.compute_palette(|e| {
        if e == target {
            Some(mutated_world)
        } else {
            Some(Mat4::IDENTITY)
        }
    });
    let diff: f32 = baseline[target as usize]
        .to_cols_array()
        .iter()
        .zip(mutated[target as usize].to_cols_array().iter())
        .map(|(a, b)| (a - b).abs())
        .sum();
    assert!(diff > 1e-3, "FNV palette did not respond to bone Transform mutation");
}

#[test]
#[ignore = "requires FNV BSA — opt in with --ignored"]
fn fnv_kf_playback_drives_palette() {
    let Some(fixture) = load_fixture(
        "BYROREDUX_FNV_DATA",
        FNV_DEFAULT_DATA,
        FNV_MESH_BSA,
        FNV_FIXTURE_NIF,
    ) else {
        return;
    };
    let skin = &fixture.skin;
    let bone_count = skin.bones.len();
    let bones: Vec<Option<u32>> = (0..bone_count as u32).map(Some).collect();
    let binds: Vec<Mat4> = skin
        .bones
        .iter()
        .map(|b| Mat4::from_cols_array_2d(&b.bind_inverse))
        .collect();
    let sm = SkinnedMesh::new(None, bones, binds);

    let mut scratch = Vec::new();
    sm.compute_palette_into(&mut scratch, |_| Some(Mat4::IDENTITY));
    let frame_a: Vec<Mat4> = scratch.clone();
    let rot = Mat4::from_rotation_y(0.25_f32);
    sm.compute_palette_into(&mut scratch, |e| {
        if e % 2 == 1 {
            Some(rot)
        } else {
            Some(Mat4::IDENTITY)
        }
    });
    let frame_b: Vec<Mat4> = scratch.clone();
    let diff_slots = frame_a
        .iter()
        .zip(frame_b.iter())
        .filter(|(a, b)| {
            a.to_cols_array()
                .iter()
                .zip(b.to_cols_array().iter())
                .any(|(x, y)| (x - y).abs() > 1e-4)
        })
        .count();
    eprintln!("[M29 FNV] frame Δ: {} / {} palette slots changed", diff_slots, bone_count);
    assert!(diff_slots > 0, "FNV palette did not change across simulated KF tick");
}

// ── SSE path: BSTriShape + BSSkinInstance + SseSkinGlobalBuffer ─────
//
// Bones / names / palette logic work, and as of #638 (closeout in
// 2026-04-25) per-vertex skin extraction also recovers from the global
// buffer — `decode_sse_packed_buffer` now decodes the 12-byte VF_SKINNED
// payload instead of skipping it, and `extract_skin_bs_tri_shape` falls
// back to the decoded values when the inline arrays are empty. The
// assertion below is the live regression gate.

#[test]
#[ignore = "requires SSE BSA — opt in with --ignored"]
fn sse_imports_skinned_mesh_with_resolved_bones() {
    let Some(fixture) = load_fixture(
        "BYROREDUX_SKYRIMSE_DATA",
        SSE_DEFAULT_DATA,
        SSE_MESH_BSA,
        SSE_FIXTURE_NIF,
    ) else {
        return;
    };
    let skin = &fixture.skin;
    assert!(!skin.bones.is_empty(), "SSE malebody must carry bones");
    let mut names: Vec<&str> = skin.bones.iter().map(|b| &*b.name).collect();
    names.sort();
    eprintln!(
        "[M29 SSE] {} bones, root={:?}, sample: {:?}",
        skin.bones.len(),
        skin.skeleton_root,
        &names[..names.len().min(6)]
    );
    let (resolved, unresolved) = resolve_bones(&fixture);
    let _ = unresolved;
    let rate = resolved as f64 / skin.bones.len() as f64;
    eprintln!(
        "[M29 SSE] resolution: {} / {} ({:.1}%)",
        resolved,
        skin.bones.len(),
        rate * 100.0
    );
    assert!(rate >= 0.80, "SSE bone resolution rate {:.1}% < 80%", rate * 100.0);
}

#[test]
#[ignore = "requires SSE BSA — opt in with --ignored"]
fn sse_vertex_indices_within_palette_bounds() {
    let Some(fixture) = load_fixture(
        "BYROREDUX_SKYRIMSE_DATA",
        SSE_DEFAULT_DATA,
        SSE_MESH_BSA,
        SSE_FIXTURE_NIF,
    ) else {
        return;
    };
    let skin = &fixture.skin;
    // #638 — this used to be the soft-flag gap test that returned
    // early when `vertex_bone_indices` was empty. The fix surfaces
    // the global-buffer skin payload, so emptiness is now a hard
    // failure: any SSE BSTriShape + SseSkinGlobalBuffer mesh that
    // lands here MUST carry per-vertex skin data.
    assert!(
        !skin.vertex_bone_indices.is_empty(),
        "[M29 SSE] vertex_bone_indices empty — #638 fallback regressed; \
         every vertex would hit the rigid fallback at triangle.vert:151 \
         and render in bind pose"
    );
    assert!(
        !skin.vertex_bone_weights.is_empty(),
        "[M29 SSE] vertex_bone_weights empty — #638 fallback regressed"
    );
    assert_eq!(
        skin.vertex_bone_indices.len(),
        skin.vertex_bone_weights.len(),
        "[M29 SSE] per-vertex indices / weights count mismatch"
    );

    let bone_count = skin.bones.len() as u16;
    for (vi, indices) in skin.vertex_bone_indices.iter().enumerate() {
        for (slot, &idx) in indices.iter().enumerate() {
            assert!(
                idx < bone_count || skin.vertex_bone_weights[vi][slot] == 0.0,
                "SSE vertex {} slot {} has bone {} >= bone_count {}",
                vi,
                slot,
                idx,
                bone_count
            );
        }
    }
}

#[test]
#[ignore = "requires SSE BSA — opt in with --ignored"]
fn sse_palette_responds_to_bone_transform() {
    // Independent of #638's per-vertex recovery, the palette compute
    // path runs over the bones list — verify it's operational on the
    // SSE side.
    let Some(fixture) = load_fixture(
        "BYROREDUX_SKYRIMSE_DATA",
        SSE_DEFAULT_DATA,
        SSE_MESH_BSA,
        SSE_FIXTURE_NIF,
    ) else {
        return;
    };
    let skin = &fixture.skin;
    let bone_count = skin.bones.len();
    if bone_count < 2 {
        eprintln!("[M29 SSE] skipping palette test — fixture has < 2 bones");
        return;
    }
    let bones: Vec<Option<u32>> = (0..bone_count as u32).map(Some).collect();
    let binds: Vec<Mat4> = skin
        .bones
        .iter()
        .map(|b| Mat4::from_cols_array_2d(&b.bind_inverse))
        .collect();
    let sm = SkinnedMesh::new(None, bones, binds);

    let baseline = sm.compute_palette(|_| Some(Mat4::IDENTITY));
    let target = 1u32;
    let mutated_world = Mat4::from_rotation_translation(
        Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
        Vec3::new(5.0, 0.0, 0.0),
    );
    let mutated = sm.compute_palette(|e| {
        if e == target {
            Some(mutated_world)
        } else {
            Some(Mat4::IDENTITY)
        }
    });
    let diff: f32 = baseline[target as usize]
        .to_cols_array()
        .iter()
        .zip(mutated[target as usize].to_cols_array().iter())
        .map(|(a, b)| (a - b).abs())
        .sum();
    assert!(diff > 1e-3, "SSE palette did not respond to bone Transform mutation");
}
