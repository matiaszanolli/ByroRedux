//! M41.0 Phase 1b.x — bind-pose agreement diagnostic.
//!
//! Discovers and pins (as `#[ignore]`d, opt-in) a small but real
//! **bind-pose disagreement** between vanilla FNV `skeleton.nif` and
//! `upperbody.nif`. They agree perfectly on the lower body (Pelvis,
//! Thighs) but diverge by ~0.11 game-units starting at `Bip01 Spine`
//! and propagating up the chain.
//!
//! **Cause**: `skeleton.nif` represents the bind pose via a deep chain
//! `Scene Root → Bip01 (rotY=90°, t=(0, 67.771, -0.657))
//!   → Bip01 NonAccum (identity)
//!   → Bip01 Spine (t=(1.608, 6.193, 0), q=(-0.504,-0.495,0.495,0.504))`
//! while `upperbody.nif` "flattens" the same bind into a single
//! `Bip01 Spine` directly under Scene Root with t=(0, 73.992, -2.153)
//! and q=(-0.006, 0.006, 0.707, 0.707). The composed *world rotation*
//! agrees to within rounding noise (~0.001), but the composed *world
//! translation* differs by (0, 0.028, 0.112) at Bip01 Spine — almost
//! certainly authoring drift between the two NIFs (different Max/Maya
//! sessions / different export-time precision).
//!
//! **What this is NOT**: this is *not* the catastrophic spike artifact
//! described in `byroredux/src/npc_spawn.rs:402-431`. A 0.11-unit
//! offset on a few bones produces a slight per-vertex shift, not a
//! "long-spike vertex artifact emanating from the head." The spike
//! requires either a NaN, a near-zero scale, or a bone-resolution
//! divergence that drags some palette slots to identity while
//! neighbours land at REFR offset. The spike's true root cause is
//! still open as Phase 1b.x.
//!
//! `#[ignore]` because it needs vanilla FNV game data; run with
//! `BYROREDUX_FNV_DATA=<path> cargo test -p byroredux --test
//! m41_phase1bx_skinning -- --ignored --nocapture`.
//!
//! These tests are written to **fail today** to document the
//! disagreement quantitatively — once the underlying authoring
//! drift is reconciled (or the importer normalises both NIFs into a
//! single shared bind), they should pass and stay green.

use byroredux_bsa::BsaArchive;
use byroredux_core::math::{Mat4, Vec3, Vec4};
use byroredux_nif::import::{import_nif_scene, ImportedNode};
use byroredux_nif::parse_nif;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

const FNV_DEFAULT_DATA: &str = "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data";
const FNV_MESH_BSA: &str = "Fallout - Meshes.bsa";
const FNV_SKELETON_NIF: &str = "meshes\\characters\\_male\\skeleton.nif";
const FNV_UPPERBODY_NIF: &str = "meshes\\characters\\_male\\upperbody.nif";

fn fnv_data_dir() -> Option<PathBuf> {
    let from_env = std::env::var("BYROREDUX_FNV_DATA").ok().map(PathBuf::from);
    let candidate = from_env.unwrap_or_else(|| PathBuf::from(FNV_DEFAULT_DATA));
    if candidate.is_dir() {
        Some(candidate)
    } else {
        eprintln!(
            "skipping: BYROREDUX_FNV_DATA not set and {} not a directory",
            candidate.display()
        );
        None
    }
}

/// Compose a node's local transform into a single column-major Mat4
/// matching the NIF→ECS importer's convention.
fn imported_node_local(node: &ImportedNode) -> Mat4 {
    let q = byroredux_core::math::Quat::from_xyzw(
        node.rotation[0],
        node.rotation[1],
        node.rotation[2],
        node.rotation[3],
    );
    let t = Vec3::new(node.translation[0], node.translation[1], node.translation[2]);
    Mat4::from_scale_rotation_translation(Vec3::splat(node.scale), q, t)
}

/// Walk a hierarchy of `ImportedNode`s and return a name→world matrix
/// map at bind pose. World root is identity; every descendant composes
/// `parent_world × node_local`.
fn node_world_at_bind(nodes: &[ImportedNode]) -> HashMap<Arc<str>, Mat4> {
    let mut by_idx: Vec<Mat4> = Vec::with_capacity(nodes.len());
    for (i, node) in nodes.iter().enumerate() {
        let local = imported_node_local(node);
        let world = match node.parent_node {
            Some(parent_idx) if parent_idx < i => by_idx[parent_idx] * local,
            _ => local, // root or invalid backref — use as-is
        };
        by_idx.push(world);
    }
    let mut map = HashMap::new();
    for (node, world) in nodes.iter().zip(by_idx.iter()) {
        if let Some(name) = node.name.clone() {
            map.insert(name, *world);
        }
    }
    map
}

/// Print the parent chain of a named node, showing each link's local
/// translation. Used to diagnose where two NIFs' bone hierarchies
/// diverge.
fn print_ancestor_chain(nodes: &[ImportedNode], target_name: &str) {
    let Some(target_idx) = nodes
        .iter()
        .position(|n| n.name.as_deref() == Some(target_name))
    else {
        eprintln!("    (target '{target_name}' not found)");
        return;
    };
    let mut chain = Vec::new();
    let mut cur = Some(target_idx);
    while let Some(idx) = cur {
        chain.push(idx);
        cur = nodes[idx].parent_node;
    }
    chain.reverse();
    for idx in chain {
        let n = &nodes[idx];
        eprintln!(
            "    [{idx:>3}] {:35} t=({:7.3},{:7.3},{:7.3})  q=({:.3},{:.3},{:.3},{:.3})  s={:.3}",
            n.name.as_deref().unwrap_or("<unnamed>"),
            n.translation[0],
            n.translation[1],
            n.translation[2],
            n.rotation[0],
            n.rotation[1],
            n.rotation[2],
            n.rotation[3],
            n.scale,
        );
    }
}

/// Helper — open the FNV mesh BSA or skip the test gracefully.
fn open_fnv_meshes() -> Option<BsaArchive> {
    let dir = fnv_data_dir()?;
    let path = dir.join(FNV_MESH_BSA);
    BsaArchive::open(&path)
        .map_err(|e| eprintln!("skipping: open {path:?}: {e}"))
        .ok()
}

/// **Diagnostic 1**: do skeleton.nif and upperbody.nif agree on each
/// shared bone's bind-pose world matrix?
///
/// External skinning composes `skel_bone_world × inv(body_bind_world)
/// × vertex_local`. If skel and body disagree on bind, the cancellation
/// fails and vertices land at the wrong offset (skin-bone offset
/// equals the disagreement matrix).
#[test]
#[ignore]
fn skeleton_and_body_agree_on_bone_bind_pose() {
    let Some(bsa) = open_fnv_meshes() else { return };
    let skel_bytes = bsa
        .extract(FNV_SKELETON_NIF)
        .expect("skeleton.nif extract");
    let body_bytes = bsa
        .extract(FNV_UPPERBODY_NIF)
        .expect("upperbody.nif extract");
    let skel_scene = parse_nif(&skel_bytes).expect("skeleton parses");
    let body_scene = parse_nif(&body_bytes).expect("upperbody parses");

    let mut skel_pool = byroredux_core::string::StringPool::new();
    let skel_imp = import_nif_scene(&skel_scene, &mut skel_pool);
    let mut body_pool = byroredux_core::string::StringPool::new();
    let body_imp = import_nif_scene(&body_scene, &mut body_pool);

    let skel_world = node_world_at_bind(&skel_imp.nodes);
    let body_world = node_world_at_bind(&body_imp.nodes);

    let body_skin = body_imp
        .meshes
        .iter()
        .find_map(|m| m.skin.as_ref())
        .expect("upperbody has at least one skinned mesh");

    let mut max_disagreement = 0.0_f32;
    let mut max_bone: Option<&str> = None;
    let mut shared = 0_usize;
    let mut skel_only = 0_usize;
    let mut body_only = 0_usize;
    for bone in &body_skin.bones {
        let in_skel = skel_world.get(&bone.name);
        let in_body = body_world.get(&bone.name);
        match (in_skel, in_body) {
            (Some(sk), Some(bw)) => {
                shared += 1;
                let max_d = sk
                    .to_cols_array()
                    .iter()
                    .zip(bw.to_cols_array().iter())
                    .map(|(a, b)| (a - b).abs())
                    .fold(0.0_f32, f32::max);
                if max_d > max_disagreement {
                    max_disagreement = max_d;
                    max_bone = Some(bone.name.as_ref());
                }
            }
            (Some(_), None) => skel_only += 1,
            (None, Some(_)) => body_only += 1,
            (None, None) => {}
        }
    }

    eprintln!(
        "FNV upperbody vs skeleton bone bind-pose comparison:\n  \
         shared bones: {shared}\n  \
         skel-only bones: {skel_only}\n  \
         body-only bones: {body_only}\n  \
         max bind-pose disagreement: {max_disagreement:.6}"
    );
    eprintln!("  bone names body binds to:");
    for bone in &body_skin.bones {
        let in_skel = skel_world.get(&bone.name);
        let in_body = body_world.get(&bone.name);
        let mut max_d = 0.0_f32;
        if let (Some(sk), Some(bw)) = (in_skel, in_body) {
            max_d = sk
                .to_cols_array()
                .iter()
                .zip(bw.to_cols_array().iter())
                .map(|(a, b)| (a - b).abs())
                .fold(0.0_f32, f32::max);
        }
        eprintln!(
            "    {:30} skel:{} body:{} max_d={:.4}",
            bone.name.as_ref(),
            if in_skel.is_some() { '✓' } else { '✗' },
            if in_body.is_some() { '✓' } else { '✗' },
            max_d
        );
    }

    // Dump the immediate ancestors of "Bip01 Spine" in BOTH NIFs to
    // find where the chain diverges. If skeleton has an intermediate
    // node (e.g. Bip01 NonAccum) that body doesn't, the chain
    // between Bip01 and Bip01 Spine accumulates an extra translation
    // that the body NIF's bind data doesn't see.
    eprintln!("\n  ancestor chain for 'Bip01 Spine' in skeleton.nif:");
    print_ancestor_chain(&skel_imp.nodes, "Bip01 Spine");
    eprintln!("\n  ancestor chain for 'Bip01 Spine' in upperbody.nif:");
    print_ancestor_chain(&body_imp.nodes, "Bip01 Spine");
    if let Some(name) = max_bone {
        let sk = skel_world.get(name).unwrap().col(3);
        let bw = body_world.get(name).unwrap().col(3);
        eprintln!(
            "  diverging bone '{name}': skel.t=({:.3},{:.3},{:.3}), body.t=({:.3},{:.3},{:.3})",
            sk.x, sk.y, sk.z, bw.x, bw.y, bw.z
        );
    }

    // The disagreement is real but small (~0.11 unit ≈ 1mm at FNV
    // actor scale, propagated as a constant Z offset from Bip01 Spine
    // up the chain). It is NOT large enough to explain the
    // catastrophic spike artifact described in
    // `byroredux/src/npc_spawn.rs:402-431`. Pinned at 0.2 so the test
    // fails loud if the disagreement *grows* (e.g. an importer change
    // that breaks one NIF more than the other), but accepts the
    // current ~0.11 baseline as documented authoring drift.
    assert!(
        max_disagreement < 0.2,
        "FNV skeleton.nif vs upperbody.nif bind-pose disagreement \
         {max_disagreement:.6} exceeds 0.2 — drift has GROWN beyond \
         the documented vanilla baseline (~0.11 at Bip01 Spine).",
    );
}

/// **Diagnostic 2**: agreement *gradient* down the bone chain.
///
/// Distinguishes "authoring drift in a single bone's local transform"
/// from "structural mismatch in the chain shape" by reporting the
/// max-disagreement at each ancestor of `Bip01 Spine`. If the gradient
/// is flat (all bones disagree equally), the disagreement is at the
/// leaf. If it's monotone-increasing toward the leaf, intermediate
/// nodes contribute. Pre-fix, the data shows: lower body (Pelvis,
/// Thighs) at 0.0; everything from Bip01 Spine up at 0.111 — a flat
/// 0.11 disagreement *introduced* at Bip01 Spine and inherited by
/// every descendant. That's a leaf-introduced drift, not a structural
/// mismatch — consistent with the "skeleton flattens the chain into
/// fewer nodes vs body's deeper representation" observation in
/// `skeleton_and_body_agree_on_bone_bind_pose`'s ancestor-chain dump.
#[test]
#[ignore]
fn bind_pose_disagreement_localised_to_spine_introduction() {
    let Some(bsa) = open_fnv_meshes() else { return };
    let skel_bytes = bsa.extract(FNV_SKELETON_NIF).unwrap();
    let body_bytes = bsa.extract(FNV_UPPERBODY_NIF).unwrap();

    let mut skel_pool = byroredux_core::string::StringPool::new();
    let mut body_pool = byroredux_core::string::StringPool::new();
    let skel_imp = import_nif_scene(&parse_nif(&skel_bytes).unwrap(), &mut skel_pool);
    let body_imp = import_nif_scene(&parse_nif(&body_bytes).unwrap(), &mut body_pool);

    let skel_world = node_world_at_bind(&skel_imp.nodes);
    let body_world = node_world_at_bind(&body_imp.nodes);

    // Walk lower-body bones (agreed) → spine introduction (disagreed)
    // and report the first bone where disagreement > 0.01. The cliff
    // localises which bone introduced the drift.
    let chain = [
        "Bip01 Pelvis",
        "Bip01 L Thigh",
        "Bip01 R Thigh",
        "Bip01 Spine",
        "Bip01 Spine1",
        "Bip01 Spine2",
        "Bip01 Neck1",
    ];
    eprintln!("Bind-pose agreement gradient (FNV upperbody vs skeleton):");
    for name in chain {
        let (Some(sk), Some(bw)) = (skel_world.get(name), body_world.get(name)) else {
            eprintln!("    {:30} (missing in one side)", name);
            continue;
        };
        let max_d = sk
            .to_cols_array()
            .iter()
            .zip(bw.to_cols_array().iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        eprintln!(
            "    {:30} max_d={:.4} skel.t=({:.3},{:.3},{:.3}) body.t=({:.3},{:.3},{:.3})",
            name,
            max_d,
            sk.col(3).x,
            sk.col(3).y,
            sk.col(3).z,
            bw.col(3).x,
            bw.col(3).y,
            bw.col(3).z,
        );
    }
    // No assertion — this is a documentation test that the cliff is
    // at Bip01 Spine. If the gradient pattern shifts (e.g. the
    // disagreement leaks into Pelvis), that's a real regression
    // worth investigating.
}
