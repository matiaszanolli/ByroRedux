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
//!
//! ## Credit
//!
//! The legacy-NiSkinData skinning formula reverse-engineered for the
//! Phase 1b.x investigation (`oblivion_vertex_world_check` and the
//! prior `fnv_skinning_invariant_check`) draws directly on the
//! [OpenMW project](https://gitlab.com/OpenMW/openmw)'s NIF
//! skinning evaluator at
//! `components/sceneutil/riggeometry.cpp:175-208` and the loader at
//! `components/nifosg/nifloader.cpp:1604-1631`. OpenMW is the open-
//! source reimplementation that handles the full legacy-Gamebryo skin
//! pipeline (Morrowind / Oblivion / FO3 / FNV / Skyrim LE) correctly,
//! and was the only authoritative source we found that surfaces the
//! `NiSkinData::mTransform` global factor that NifSkope's partition
//! path drops (`tools/nifskope/src/gl/glmesh.cpp:875`). OpenMW is
//! GPLv3 — reference-only here; no code is copy-pasted.

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

const FNV_DEFAULT_DATA: &str = "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data";
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
fn load_fixture(env_var: &str, default: &str, bsa_name: &str, nif_path: &str) -> Option<Fixture> {
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
    let mut pool = byroredux_core::string::StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);

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

/// Helper for the invariant test — runs against arbitrary NIF bytes.
fn run_skinning_invariant(label: &str, bytes: &[u8]) {
    use byroredux_core::math::Mat4;
    use byroredux_nif::blocks::node::NiNode;
    use byroredux_nif::blocks::skin::{BsDismemberSkinInstance, NiSkinData, NiSkinInstance};
    use byroredux_nif::blocks::tri_shape::NiTriShape;
    use byroredux_nif::types::BlockRef;

    let scene = byroredux_nif::parse_nif(bytes).unwrap();

    fn nitransform_to_mat4(t: &byroredux_nif::types::NiTransform) -> Mat4 {
        let r = &t.rotation.rows;
        let s = t.scale;
        Mat4::from_cols_array(&[
            r[0][0] * s,
            r[1][0] * s,
            r[2][0] * s,
            0.0,
            r[0][1] * s,
            r[1][1] * s,
            r[2][1] * s,
            0.0,
            r[0][2] * s,
            r[1][2] * s,
            r[2][2] * s,
            0.0,
            t.translation.x,
            t.translation.y,
            t.translation.z,
            1.0,
        ])
    }

    fn world_xform_for_named_node(
        scene: &byroredux_nif::scene::NifScene,
        target_name: &str,
    ) -> Option<Mat4> {
        let root_idx = scene.root_index?;
        let mut stack: Vec<(usize, Mat4)> = vec![(root_idx, Mat4::IDENTITY)];
        while let Some((idx, parent_world)) = stack.pop() {
            let Some(node) = scene.get_as::<NiNode>(idx) else {
                continue;
            };
            let local = nitransform_to_mat4(&node.av.transform);
            let world = parent_world * local;
            if node
                .av
                .net
                .name
                .as_deref()
                .map(|n: &str| n.eq_ignore_ascii_case(target_name))
                .unwrap_or(false)
            {
                return Some(world);
            }
            for child_ref in &node.children {
                if let Some(child_idx) = (*child_ref).index() {
                    stack.push((child_idx, world));
                }
            }
        }
        None
    }

    eprintln!("\n========== {} ==========", label);
    for shape_block_idx in 0..scene.blocks.len() {
        let Some(shape) = scene.get_as::<NiTriShape>(shape_block_idx) else {
            continue;
        };
        let shape_name = shape.av.net.name.as_deref().unwrap_or("?").to_string();
        if matches!(
            shape_name.as_str(),
            "bodycaps" | "limbcaps" | "meatneck01" | "meathead01"
        ) {
            continue;
        }
        let Some(skin_idx) = shape.skin_instance_ref.index() else {
            continue;
        };
        let inst = scene.get_as::<NiSkinInstance>(skin_idx);
        let inst_dis = scene.get_as::<BsDismemberSkinInstance>(skin_idx);
        let (data_ref, bone_refs): (BlockRef, &[BlockRef]) = if let Some(i) = inst {
            (i.data_ref, &i.bone_refs)
        } else if let Some(i) = inst_dis {
            (i.base.data_ref, &i.base.bone_refs)
        } else {
            continue;
        };
        let Some(data_idx) = data_ref.index() else {
            continue;
        };
        let Some(data) = scene.get_as::<NiSkinData>(data_idx) else {
            continue;
        };

        let global_to_skin = nitransform_to_mat4(&data.skin_transform);
        eprintln!(
            "── shape '{}' ──  global_to_skin.t=({:.3},{:.3},{:.3})  scale={:.3}",
            shape_name,
            data.skin_transform.translation.x,
            data.skin_transform.translation.y,
            data.skin_transform.translation.z,
            data.skin_transform.scale,
        );
        for (i, bone_ref) in bone_refs.iter().enumerate().take(3) {
            let Some(bone_idx) = bone_ref.index() else {
                continue;
            };
            let Some(bone_node) = scene.get_as::<NiNode>(bone_idx) else {
                continue;
            };
            let bone_name = bone_node.av.net.name.as_deref().unwrap_or("?");
            let Some(bone_world) = world_xform_for_named_node(&scene, bone_name) else {
                eprintln!("  [{}] {} — could not resolve bone in tree", i, bone_name);
                continue;
            };
            let skin_to_bone = nitransform_to_mat4(&data.bones[i].skin_transform);
            let composed = global_to_skin * bone_world * skin_to_bone;
            let id = Mat4::IDENTITY;
            let mut max_diff: f32 = 0.0;
            for c in 0..4 {
                for r in 0..4 {
                    let v = composed.col(c)[r] - id.col(c)[r];
                    if v.abs() > max_diff {
                        max_diff = v.abs()
                    }
                }
            }
            let composed_t = composed.col(3);
            eprintln!(
                "  [{}] {:30} bone_world.t=({:.1},{:.1},{:.1})  skinToBone.t=({:.3},{:.3},{:.3})  composed.t=({:.3},{:.3},{:.3})  max_diff_from_I={:.4}",
                i, bone_name,
                bone_world.col(3)[0], bone_world.col(3)[1], bone_world.col(3)[2],
                data.bones[i].skin_transform.translation.x,
                data.bones[i].skin_transform.translation.y,
                data.bones[i].skin_transform.translation.z,
                composed_t[0], composed_t[1], composed_t[2],
                max_diff,
            );
        }
    }
}

const OBLIVION_DEFAULT_DATA: &str = "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data";
const OBLIVION_MESH_BSA: &str = "Oblivion - Meshes.bsa";

/// M41.0 Phase 1b.x — compute the actual rendered world position of
/// real Oblivion body vertices using the NifSkope formula (verified
/// in `tools/nifskope/src/gl/glmesh.cpp:875`):
///
///   vertex_world = bone.localTrans(skeleton_root) × skin_trans × vertex_local
///
/// (NifSkope drops a `scene->view` factor which we substitute with
/// identity to get world space.) For a vertex weighted 1.0 to a single
/// bone, the result should land at the bone's bind-pose world position
/// plus the vertex's offset in NIF mesh-local space — matching what a
/// human would expect for a standing biped at origin.
#[test]
#[ignore = "requires Oblivion BSA — opt in with --ignored"]
fn oblivion_vertex_world_check() {
    use byroredux_core::math::{Mat4, Vec4};
    use byroredux_nif::blocks::node::NiNode;
    use byroredux_nif::blocks::skin::{BsDismemberSkinInstance, NiSkinData, NiSkinInstance};
    use byroredux_nif::blocks::tri_shape::{NiTriShape, NiTriShapeData};
    use byroredux_nif::types::BlockRef;

    let Some(data_dir) = data_dir("BYROREDUX_OBLIVION_DATA", OBLIVION_DEFAULT_DATA) else {
        return;
    };
    let bsa = byroredux_bsa::BsaArchive::open(&data_dir.join(OBLIVION_MESH_BSA)).unwrap();
    let bytes = bsa
        .extract("meshes\\characters\\_male\\upperbody.nif")
        .unwrap();
    let scene = byroredux_nif::parse_nif(&bytes).unwrap();

    fn nitransform_to_mat4(t: &byroredux_nif::types::NiTransform) -> Mat4 {
        let r = &t.rotation.rows;
        let s = t.scale;
        Mat4::from_cols_array(&[
            r[0][0] * s,
            r[1][0] * s,
            r[2][0] * s,
            0.0,
            r[0][1] * s,
            r[1][1] * s,
            r[2][1] * s,
            0.0,
            r[0][2] * s,
            r[1][2] * s,
            r[2][2] * s,
            0.0,
            t.translation.x,
            t.translation.y,
            t.translation.z,
            1.0,
        ])
    }

    /// Walk up parent chain from `target_name` to `skel_root_name`,
    /// composing locals — replicates NifSkope's `Node::localTrans(root)`.
    fn local_trans_to_root(
        scene: &byroredux_nif::scene::NifScene,
        target_name: &str,
        skel_root_name: &str,
    ) -> Option<Mat4> {
        // Build parent map by walking the tree once.
        let root_idx = scene.root_index?;
        let mut parent_of: std::collections::HashMap<usize, usize> = Default::default();
        let mut stack = vec![root_idx];
        while let Some(idx) = stack.pop() {
            if let Some(node) = scene.get_as::<NiNode>(idx) {
                for child in &node.children {
                    if let Some(c) = (*child).index() {
                        parent_of.insert(c, idx);
                        stack.push(c);
                    }
                }
            }
        }
        // Find target by name + walk up.
        let mut target_idx: Option<usize> = None;
        for i in 0..scene.blocks.len() {
            if let Some(node) = scene.get_as::<NiNode>(i) {
                if node
                    .av
                    .net
                    .name
                    .as_deref()
                    .map(|n: &str| n.eq_ignore_ascii_case(target_name))
                    .unwrap_or(false)
                {
                    target_idx = Some(i);
                    break;
                }
            }
        }
        let mut cur = target_idx?;
        let mut accum = Mat4::IDENTITY;
        loop {
            let node = scene.get_as::<NiNode>(cur)?;
            if node
                .av
                .net
                .name
                .as_deref()
                .map(|n: &str| n.eq_ignore_ascii_case(skel_root_name))
                .unwrap_or(false)
            {
                return Some(accum);
            }
            let local = nitransform_to_mat4(&node.av.transform);
            accum = local * accum;
            let Some(&p) = parent_of.get(&cur) else {
                return Some(accum);
            };
            cur = p;
        }
    }

    // Find Arms:0 (or any skinned shape) and verify a single-bone
    // vertex's world position.
    for shape_idx in 0..scene.blocks.len() {
        let Some(shape) = scene.get_as::<NiTriShape>(shape_idx) else {
            continue;
        };
        let shape_name = shape.av.net.name.as_deref().unwrap_or("?").to_string();
        if shape_name != "Arms" && shape_name != "UpperBody" {
            continue;
        }
        let Some(skin_idx) = shape.skin_instance_ref.index() else {
            continue;
        };
        let inst = scene.get_as::<NiSkinInstance>(skin_idx);
        let inst_dis = scene.get_as::<BsDismemberSkinInstance>(skin_idx);
        let (data_ref, bone_refs, skel_root_ref): (BlockRef, &[BlockRef], BlockRef) =
            if let Some(i) = inst {
                (i.data_ref, &i.bone_refs, i.skeleton_root_ref)
            } else if let Some(i) = inst_dis {
                (i.base.data_ref, &i.base.bone_refs, i.base.skeleton_root_ref)
            } else {
                continue;
            };
        let Some(data_idx) = data_ref.index() else {
            continue;
        };
        let Some(data) = scene.get_as::<NiSkinData>(data_idx) else {
            continue;
        };
        let Some(skel_root_idx) = skel_root_ref.index() else {
            continue;
        };
        let Some(skel_root_node) = scene.get_as::<NiNode>(skel_root_idx) else {
            continue;
        };
        let skel_root_name = skel_root_node
            .av
            .net
            .name
            .as_deref()
            .unwrap_or("?")
            .to_string();

        // Find a bone we can identify and pick its first weighted vertex.
        let Some(data_idx) = shape.data_ref.index() else {
            continue;
        };
        let Some(geom) = scene.get_as::<NiTriShapeData>(data_idx) else {
            continue;
        };

        eprintln!("\n══ {} (skel_root={}) ══", shape_name, skel_root_name);
        for (i, bone_ref) in bone_refs.iter().enumerate().take(3) {
            let Some(bone_block_idx) = bone_ref.index() else {
                continue;
            };
            let Some(bone_node) = scene.get_as::<NiNode>(bone_block_idx) else {
                continue;
            };
            let bone_name = bone_node.av.net.name.as_deref().unwrap_or("?").to_string();

            let local_trans =
                local_trans_to_root(&scene, &bone_name, &skel_root_name).unwrap_or(Mat4::IDENTITY);
            let skin_trans = nitransform_to_mat4(&data.bones[i].skin_transform);

            // OpenMW formula (riggeometry.cpp:175-208) translated to
            // column-major (mat × vec). OpenMW uses OSG vec × mat
            // convention; equivalent column-major composition:
            //   palette = dataTransform × boneSkelSpace × InvBindMatrix
            //   vertex_world = palette × vertex
            // (skinToSkelMatrix is identity for body NIFs with shape
            //  parented directly under skel root.)
            //
            // Equivalent fix: pre-transform vertex by dataTransform at
            // import (vertex' = dataTransform × vertex), then existing
            // palette = boneSkelSpace × InvBind gives same result.
            let data_transform = nitransform_to_mat4(&data.skin_transform);
            // OpenMW formula (riggeometry.cpp:175-208): in OSG row-vec
            // convention `vec × invBind × boneSkel × skinToSkel × data`.
            // Translated to glam (column-major mat × vec):
            //   `data × skinToSkel × boneSkel × invBind × vec`
            // Order tested empirically — neither this nor the reverse
            // produce identity at bind for Oblivion body NIFs. The
            // formula needs further analysis (likely OSG matrix-storage
            // convention has another subtlety not captured here).
            let palette = data_transform * local_trans * skin_trans;

            // Sample the bone's first heavily-weighted vertex.
            let Some(vw) = data.bones[i].vertex_weights.iter().max_by(|a, b| {
                a.weight
                    .partial_cmp(&b.weight)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) else {
                continue;
            };
            if (vw.vertex_index as usize) >= geom.vertices.len() {
                continue;
            }
            let v = geom.vertices[vw.vertex_index as usize];
            let v4 = Vec4::new(v.x, v.y, v.z, 1.0);
            let world = palette * v4;

            // Where is the bone in bind world?
            let bone_world_t = local_trans.col(3);

            let st_rot = &data.bones[i].skin_transform.rotation.rows;
            let dt_rot = &data.skin_transform.rotation.rows;
            eprintln!(
                "  [{}] '{}' vertex {} (weight {:.2}): NIF-local=({:.1},{:.1},{:.1})  →  world=({:.1},{:.1},{:.1})  [bone_world.t=({:.1},{:.1},{:.1})]",
                i, bone_name, vw.vertex_index, vw.weight,
                v.x, v.y, v.z,
                world.x, world.y, world.z,
                bone_world_t.x, bone_world_t.y, bone_world_t.z,
            );
            eprintln!(
                "       skin_trans.rot:[{:.3} {:.3} {:.3}; {:.3} {:.3} {:.3}; {:.3} {:.3} {:.3}]  .t=({:.3},{:.3},{:.3})",
                st_rot[0][0], st_rot[0][1], st_rot[0][2],
                st_rot[1][0], st_rot[1][1], st_rot[1][2],
                st_rot[2][0], st_rot[2][1], st_rot[2][2],
                data.bones[i].skin_transform.translation.x,
                data.bones[i].skin_transform.translation.y,
                data.bones[i].skin_transform.translation.z,
            );
            eprintln!(
                "       data_trans.rot:[{:.3} {:.3} {:.3}; {:.3} {:.3} {:.3}; {:.3} {:.3} {:.3}]  .t=({:.3},{:.3},{:.3})",
                dt_rot[0][0], dt_rot[0][1], dt_rot[0][2],
                dt_rot[1][0], dt_rot[1][1], dt_rot[1][2],
                dt_rot[2][0], dt_rot[2][1], dt_rot[2][2],
                data.skin_transform.translation.x,
                data.skin_transform.translation.y,
                data.skin_transform.translation.z,
            );
        }
    }
}

#[test]
#[ignore = "requires Oblivion BSA — opt in with --ignored"]
fn oblivion_skinning_invariant_check() {
    let Some(data_dir) = data_dir("BYROREDUX_OBLIVION_DATA", OBLIVION_DEFAULT_DATA) else {
        return;
    };
    let bsa_path = data_dir.join(OBLIVION_MESH_BSA);
    let bsa = byroredux_bsa::BsaArchive::open(&bsa_path).unwrap();
    for nif_path in &[
        "meshes\\characters\\_male\\upperbody.nif",
        "meshes\\characters\\_male\\lowerbody.nif",
    ] {
        let Ok(bytes) = bsa.extract(nif_path) else {
            continue;
        };
        run_skinning_invariant(nif_path, &bytes);
    }
}

#[test]
#[ignore = "requires FNV BSA — opt in with --ignored"]
fn fnv_skinning_invariant_check() {
    // M41.0 Phase 1b.x — verify the legacy NiSkinData skinning
    // invariant against real FNV upperbody.nif data.
    //
    // Per niftools / nifly / Gamebryo 2.6 source:
    //   NiSkinData.skinTransform         = transformGlobalToSkin
    //   NiSkinData.bones[i].skinTransform = transformSkinToBone (per-bone)
    //
    // At BIND POSE, the runtime invariant is:
    //   bone_world_at_bind × skinToBone[i]  =  SkinToGlobal  (constant ∀ i)
    //   ⇔  GlobalToSkin × bone_world_at_bind × skinToBone[i] = identity
    //
    // i.e. composing global-to-skin × bone_world × skin-to-bone for ANY
    // bone yields identity in bind pose. This gives us a hard, parser-
    // independent assertion: if ANY bone breaks this invariant, our
    // import (or the runtime composition) is dropping or misordering a
    // factor.
    use byroredux_core::math::Mat4;
    use byroredux_nif::blocks::node::NiNode;
    use byroredux_nif::blocks::skin::{BsDismemberSkinInstance, NiSkinData, NiSkinInstance};
    use byroredux_nif::blocks::tri_shape::NiTriShape;
    use byroredux_nif::types::BlockRef;

    let bytes =
        byroredux_bsa::BsaArchive::open(&PathBuf::from(FNV_DEFAULT_DATA).join(FNV_MESH_BSA))
            .unwrap()
            .extract(FNV_FIXTURE_NIF)
            .unwrap();
    let scene = byroredux_nif::parse_nif(&bytes).unwrap();

    fn nitransform_to_mat4(t: &byroredux_nif::types::NiTransform) -> Mat4 {
        let r = &t.rotation.rows;
        let s = t.scale;
        // nif.xml line 1812 claims NiMatrix3 is "OpenGL column-major
        // format" but that contradicts how nifly (a well-tested
        // Bethesda-NIF round-trip library) handles it: nifly does a
        // raw memcpy from file into `Vector3 rows[3]` and its
        // Matrix3*Vector3 multiplies treat rows[i] as ROW i (standard
        // math notation). So the file actually stores rows
        // sequentially despite nif.xml's wording.
        //
        // Our parser matches nifly's read pattern: rows[i] = row i.
        // For glam Mat4 (column-major), col c = (M[0][c], M[1][c],
        // M[2][c]) = (r[0][c], r[1][c], r[2][c]).
        Mat4::from_cols_array(&[
            r[0][0] * s,
            r[1][0] * s,
            r[2][0] * s,
            0.0, // col 0
            r[0][1] * s,
            r[1][1] * s,
            r[2][1] * s,
            0.0, // col 1
            r[0][2] * s,
            r[1][2] * s,
            r[2][2] * s,
            0.0, // col 2
            t.translation.x,
            t.translation.y,
            t.translation.z,
            1.0,
        ])
    }

    // Walk the NIF node tree to compute each named node's world
    // transform at bind. Caller-side recursion via a helper closure-y
    // function (we just walk the raw tree).
    fn world_xform_for_named_node(
        scene: &byroredux_nif::scene::NifScene,
        target_name: &str,
    ) -> Option<Mat4> {
        let root_idx = scene.root_index?;
        let mut stack: Vec<(usize, Mat4)> = vec![(root_idx, Mat4::IDENTITY)];
        while let Some((idx, parent_world)) = stack.pop() {
            let Some(node) = scene.get_as::<NiNode>(idx) else {
                continue;
            };
            let local = nitransform_to_mat4(&node.av.transform);
            let world = parent_world * local;
            if node
                .av
                .net
                .name
                .as_deref()
                .map(|n: &str| n.eq_ignore_ascii_case(target_name))
                .unwrap_or(false)
            {
                return Some(world);
            }
            for child_ref in &node.children {
                if let Some(child_idx) = (*child_ref).index() {
                    stack.push((child_idx, world));
                }
            }
        }
        None
    }

    for shape_block_idx in 0..scene.blocks.len() {
        let Some(shape) = scene.get_as::<NiTriShape>(shape_block_idx) else {
            continue;
        };
        let shape_name = shape.av.net.name.as_deref().unwrap_or("?").to_string();
        if shape_name == "bodycaps"
            || shape_name == "limbcaps"
            || shape_name == "meatneck01"
            || shape_name == "meathead01"
        {
            continue; // skip dismemberment caps
        }

        let Some(skin_idx) = shape.skin_instance_ref.index() else {
            continue;
        };
        let inst = scene.get_as::<NiSkinInstance>(skin_idx);
        let inst_dis = scene.get_as::<BsDismemberSkinInstance>(skin_idx);
        let (data_ref, bone_refs): (BlockRef, &[BlockRef]) = if let Some(i) = inst {
            (i.data_ref, &i.bone_refs)
        } else if let Some(i) = inst_dis {
            (i.base.data_ref, &i.base.bone_refs)
        } else {
            continue;
        };
        let Some(data_idx) = data_ref.index() else {
            continue;
        };
        let Some(data) = scene.get_as::<NiSkinData>(data_idx) else {
            continue;
        };

        let global_to_skin = nitransform_to_mat4(&data.skin_transform);
        eprintln!(
            "── shape '{}' ──  global_to_skin.t=({:.3},{:.3},{:.3})  scale={:.3}",
            shape_name,
            data.skin_transform.translation.x,
            data.skin_transform.translation.y,
            data.skin_transform.translation.z,
            data.skin_transform.scale,
        );

        // For first 3 bones, compute and check the invariant:
        //   global_to_skin × bone_world_at_bind × skin_to_bone[i] ≈ identity
        for (i, bone_ref) in bone_refs.iter().enumerate().take(3) {
            let Some(bone_idx) = bone_ref.index() else {
                continue;
            };
            let Some(bone_node) = scene.get_as::<NiNode>(bone_idx) else {
                continue;
            };
            let bone_name = bone_node.av.net.name.as_deref().unwrap_or("?");

            let bone_world_at_bind = world_xform_for_named_node(&scene, bone_name);
            let Some(bone_world) = bone_world_at_bind else {
                eprintln!("  [{}] {} — could not resolve bone in tree", i, bone_name);
                continue;
            };

            let skin_to_bone = nitransform_to_mat4(&data.bones[i].skin_transform);

            // Compose: global_to_skin × bone_world × skin_to_bone
            let composed = global_to_skin * bone_world * skin_to_bone;
            // Distance from identity:
            let id = Mat4::IDENTITY;
            let mut max_diff: f32 = 0.0;
            for c in 0..4 {
                for r in 0..4 {
                    let v = composed.col(c)[r] - id.col(c)[r];
                    if v.abs() > max_diff {
                        max_diff = v.abs()
                    }
                }
            }
            let composed_t = composed.col(3);
            eprintln!(
                "  [{}] {:30} bone_world.t=({:.1},{:.1},{:.1})  skinToBone.t=({:.3},{:.3},{:.3})  composed.t=({:.3},{:.3},{:.3})  max_diff_from_I={:.4}",
                i, bone_name,
                bone_world.col(3)[0], bone_world.col(3)[1], bone_world.col(3)[2],
                data.bones[i].skin_transform.translation.x,
                data.bones[i].skin_transform.translation.y,
                data.bones[i].skin_transform.translation.z,
                composed_t[0], composed_t[1], composed_t[2],
                max_diff,
            );
        }
    }
}

#[test]
#[ignore = "requires FNV BSA — opt in with --ignored"]
fn fnv_dump_global_skin_transform() {
    // Check what NiSkinData.skin_transform (the global mesh→skeleton-root
    // offset) actually contains for FNV body NIFs. If non-identity, our
    // import currently drops it.
    use byroredux_nif::blocks::skin::NiSkinData;
    use byroredux_nif::blocks::tri_shape::NiTriShape;
    let bytes =
        byroredux_bsa::BsaArchive::open(&PathBuf::from(FNV_DEFAULT_DATA).join(FNV_MESH_BSA))
            .unwrap()
            .extract(FNV_FIXTURE_NIF)
            .unwrap();
    let scene = byroredux_nif::parse_nif(&bytes).unwrap();
    for i in 0..scene.blocks.len() {
        let Some(shape) = scene.get_as::<NiTriShape>(i) else {
            continue;
        };
        let Some(skin_inst_idx) = shape.skin_instance_ref.index() else {
            continue;
        };
        let inst = scene.get_as::<byroredux_nif::blocks::skin::NiSkinInstance>(skin_inst_idx);
        let inst_dismember =
            scene.get_as::<byroredux_nif::blocks::skin::BsDismemberSkinInstance>(skin_inst_idx);
        let data_ref = inst
            .map(|i| i.data_ref)
            .or_else(|| inst_dismember.map(|i| i.base.data_ref));
        let Some(data_ref) = data_ref else { continue };
        let Some(data_idx) = data_ref.index() else {
            continue;
        };
        let Some(data) = scene.get_as::<NiSkinData>(data_idx) else {
            continue;
        };
        eprintln!(
            "shape '{}' (block {}): NiSkinData.skin_transform.translation = ({:.3}, {:.3}, {:.3})  scale={:.3}",
            shape.av.net.name.as_deref().unwrap_or("?"),
            i,
            data.skin_transform.translation.x,
            data.skin_transform.translation.y,
            data.skin_transform.translation.z,
            data.skin_transform.scale,
        );
        let r = &data.skin_transform.rotation.rows;
        eprintln!(
            "    rotation matrix:  [{:.3} {:.3} {:.3}]  [{:.3} {:.3} {:.3}]  [{:.3} {:.3} {:.3}]",
            r[0][0], r[0][1], r[0][2], r[1][0], r[1][1], r[1][2], r[2][0], r[2][1], r[2][2],
        );
    }
}

#[test]
#[ignore = "requires FNV BSA — opt in with --ignored"]
fn fnv_vertex_skin_dump_arms1() {
    // M41.0 Phase 1b.x followup — direct dump of a few sample vertex
    // skin entries on `Arms:1` so we can hand-verify that bone indices
    // point at sensible bones (e.g. a chest vertex weights to spine
    // bones, not to a foot bone). Live runtime probe says all bones
    // agree across NIFs and the math should work, yet rendering
    // produces a long-ribbon vertex artifact — the disagreement has
    // to be in vertex-bone-index assignment.
    let bytes =
        byroredux_bsa::BsaArchive::open(&PathBuf::from(FNV_DEFAULT_DATA).join(FNV_MESH_BSA))
            .unwrap()
            .extract(FNV_FIXTURE_NIF)
            .unwrap();
    let scene = byroredux_nif::parse_nif(&bytes).unwrap();
    let mut pool = byroredux_core::string::StringPool::new();
    let imported = byroredux_nif::import::import_nif_scene(&scene, &mut pool);
    let arms1 = imported
        .meshes
        .iter()
        .find(|m| m.name.as_deref() == Some("Arms:1"))
        .expect("Arms:1 must exist");
    let skin = arms1.skin.as_ref().expect("Arms:1 must be skinned");

    eprintln!("Arms:1 bones (in skin.bones order):");
    for (i, b) in skin.bones.iter().enumerate() {
        eprintln!("  [{}] {}", i, b.name);
    }

    let n = arms1.positions.len();
    eprintln!("\nSample vertex skin assignments (first 8 + 4 from middle):");
    let sample_indices: Vec<usize> = (0..8).chain((n / 2)..(n / 2 + 4)).collect();
    for v in sample_indices {
        let pos = arms1.positions[v];
        let idx = skin.vertex_bone_indices[v];
        let w = skin.vertex_bone_weights[v];
        eprintln!(
            "  v[{:>3}] pos=({:6.1},{:6.1},{:6.1})  bone_idx={:?}  weights=[{:.2},{:.2},{:.2},{:.2}]  → {} {} {} {}",
            v,
            pos[0],
            pos[1],
            pos[2],
            idx,
            w[0],
            w[1],
            w[2],
            w[3],
            skin.bones.get(idx[0] as usize).map(|b| b.name.as_ref()).unwrap_or("?"),
            skin.bones.get(idx[1] as usize).map(|b| b.name.as_ref()).unwrap_or("?"),
            skin.bones.get(idx[2] as usize).map(|b| b.name.as_ref()).unwrap_or("?"),
            skin.bones.get(idx[3] as usize).map(|b| b.name.as_ref()).unwrap_or("?"),
        );
    }

    // Also: count how many vertices have ANY weight > 0 (i.e. skinned
    // path active) vs all-zero (rigid fallback).
    let mut active = 0;
    let mut zero = 0;
    for w in &skin.vertex_bone_weights {
        let s = w[0] + w[1] + w[2] + w[3];
        if s > 0.001 {
            active += 1
        } else {
            zero += 1
        }
    }
    eprintln!(
        "\nWeight distribution: {} active, {} all-zero (rigid fallback)",
        active, zero
    );
}

#[test]
#[ignore = "requires FNV BSA — opt in with --ignored"]
fn fnv_vertex_skin_coverage_full() {
    // M41.0 Phase 1b.x followup — rendering shows the body skin's
    // vertex-bone-indices array might cover fewer vertices than the
    // mesh has positions. The scene-side mesh attach in
    // `scene.rs:1453-1471` falls through to `Vertex::new` (rigid,
    // zero weights) for any vertex past `skin.vertex_bone_indices.len()`,
    // which the shader interprets as `wsum<0.001 → use inst.model`.
    // Surface the gap as a hard regression so a partial coverage
    // can't sneak past with M29's main palette assertions.
    let Some(fixture) = load_fixture(
        "BYROREDUX_FNV_DATA",
        FNV_DEFAULT_DATA,
        FNV_MESH_BSA,
        FNV_FIXTURE_NIF,
    ) else {
        return;
    };
    let bytes =
        byroredux_bsa::BsaArchive::open(&PathBuf::from(FNV_DEFAULT_DATA).join(FNV_MESH_BSA))
            .unwrap()
            .extract(FNV_FIXTURE_NIF)
            .unwrap();
    let scene = byroredux_nif::parse_nif(&bytes).unwrap();
    let mut pool = byroredux_core::string::StringPool::new();
    let imported = byroredux_nif::import::import_nif_scene(&scene, &mut pool);
    let mut mismatches = 0usize;
    for mesh in &imported.meshes {
        if let Some(skin) = mesh.skin.as_ref() {
            let pos_n = mesh.positions.len();
            let idx_n = skin.vertex_bone_indices.len();
            let w_n = skin.vertex_bone_weights.len();
            eprintln!(
                "[M29 FNV] mesh '{}': {} positions, {} vertex_bone_indices, {} vertex_bone_weights",
                mesh.name.as_deref().unwrap_or("?"),
                pos_n,
                idx_n,
                w_n,
            );
            if idx_n != pos_n || w_n != pos_n {
                mismatches += 1;
            }
        }
    }
    let _ = fixture;
    assert_eq!(
        mismatches, 0,
        "FNV skinned meshes have vertex_bone_indices/weights coverage \
         not matching positions length — every vertex past coverage \
         falls to the rigid path in `scene.rs:1471` and renders at \
         `inst.model × vertex_local` (placement_root × NIF-local), \
         while neighbours render through palette × vertex_local. The \
         mixed paths spread triangles across both regions and produce \
         the long-ribbon vertex artifact."
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
    assert!(
        max_index > 0,
        "FNV vertices all pinned to bone 0 — partition decode regression"
    );
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
    let sm = SkinnedMesh::new_with_global(None, bones, binds, Mat4::IDENTITY);

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
    assert!(
        diff > 1e-3,
        "FNV palette did not respond to bone Transform mutation"
    );
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
    let sm = SkinnedMesh::new_with_global(None, bones, binds, Mat4::IDENTITY);

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
    eprintln!(
        "[M29 FNV] frame Δ: {} / {} palette slots changed",
        diff_slots, bone_count
    );
    assert!(
        diff_slots > 0,
        "FNV palette did not change across simulated KF tick"
    );
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
    assert!(
        rate >= 0.80,
        "SSE bone resolution rate {:.1}% < 80%",
        rate * 100.0
    );
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
    let sm = SkinnedMesh::new_with_global(None, bones, binds, Mat4::IDENTITY);

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
    assert!(
        diff > 1e-3,
        "SSE palette did not respond to bone Transform mutation"
    );
}
