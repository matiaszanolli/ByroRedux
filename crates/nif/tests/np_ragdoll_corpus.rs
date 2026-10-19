//! #5015 — real-corpus pin for the FO4 `bhkRagdollSystem` decode
//! ([`byroredux_nif::blocks::collision::decode_np_ragdoll`]).
//!
//! The layout was recovered from the vanilla corpus (there is no reflection
//! metadata in the blobs and no SDK is consulted), so the pins here are the
//! consistency checks it was derived from, run over every ragdoll skeleton:
//!
//! - every `bhkRagdollSystem` blob decodes;
//! - for every decoded joint, both of its bodies name the **same world point**
//!   for the joint's pivot at the bind pose (a mis-read offset, frame order or
//!   body index breaks this at once);
//! - the human skeleton imports as 18 named bodies and 17 joints, with the
//!   masses (pelvis 18 kg, thighs 4 kg, hands 2 kg) that equal capsule volume
//!   times the density stored beside the inverse mass (checked to 1e-7 when
//!   the layout was derived).
//!
//! `#[ignore]` — needs Fallout 4 on disk; run with:
//! `cargo test -p byroredux-nif --test np_ragdoll_corpus -- --ignored --nocapture`

mod common;

use byroredux_nif::blocks::collision::{
    decode_np_ragdoll, BhkSystemBinary, NpConstraintData, NpRagdoll,
};
use common::{open_all_mesh_archives, Game};

/// Rotate `v` by the quaternion `q = [x, y, z, w]`.
fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    let t = [
        2.0 * (y * v[2] - z * v[1]),
        2.0 * (z * v[0] - x * v[2]),
        2.0 * (x * v[1] - y * v[0]),
    ];
    [
        v[0] + w * t[0] + (y * t[2] - z * t[1]),
        v[1] + w * t[1] + (z * t[0] - x * t[2]),
        v[2] + w * t[2] + (x * t[1] - y * t[0]),
    ]
}

fn world_pivot(ragdoll: &NpRagdoll, body: usize, pivot: [f32; 4]) -> [f32; 3] {
    let b = &ragdoll.bodies[body];
    let r = rotate(b.rotation, [pivot[0], pivot[1], pivot[2]]);
    [
        b.position[0] + r[0],
        b.position[1] + r[1],
        b.position[2] + r[2],
    ]
}

/// Skeletons whose every joint must agree to a centimetre: the human rig and
/// the creatures an exterior spawns most. The machines and props that do not
/// (turrets, the vertibird, Liberty Prime's furniture, a few robot parts) author
/// joints whose pivots meet at a rest pose other than the stored bind pose —
/// 64 of the corpus's 847 joints, concentrated in those files.
const EXACT: &[&str] = &[
    "character\\characterassets\\skeleton.nif",
    "supermutant\\characterassets\\skeleton.nif",
    "feralghoul\\characterassets\\skeleton.nif",
    "brahmin\\characterassets\\skeleton.nif",
    "molerat\\characterassets\\skeleton.nif",
    "dogmeat\\characterassets\\skeleton.nif",
    "radscorpion\\characterassets\\skeleton.nif",
    "yaoguai\\characterassets\\skeleton.nif",
    "mirelurkking\\characterassets\\skeleton.nif",
    "radstag\\characterassets\\skeleton.nif",
];

#[test]
#[ignore = "needs Fallout 4 game data on disk"]
fn every_fo4_ragdoll_blob_decodes_with_consistent_joint_pivots() {
    let Some(archives) = open_all_mesh_archives(Game::Fallout4) else {
        return;
    };
    let (mut blobs, mut joints, mut disagreeing) = (0usize, 0usize, 0usize);
    let mut failures: Vec<String> = Vec::new();
    for (_, archive) in &archives {
        for file in archive.list_files() {
            let lower = file.to_ascii_lowercase();
            if !byroredux_nif::corpus::is_nif_entry(&lower) || !lower.starts_with("meshes\\actors") {
                continue;
            }
            let Ok(bytes) = archive.extract(&file) else { continue };
            let Ok(scene) = byroredux_nif::parse_nif(&bytes) else { continue };
            for block in &scene.blocks {
                let Some(system) = block.as_any().downcast_ref::<BhkSystemBinary>() else {
                    continue;
                };
                if system.type_name != "bhkRagdollSystem" {
                    continue;
                }
                blobs += 1;
                let ragdoll = match decode_np_ragdoll(&system.data) {
                    Ok(r) => r,
                    Err(e) => {
                        failures.push(format!("{file}: does not decode: {e}"));
                        continue;
                    }
                };
                let must_agree = EXACT.iter().any(|name| lower.ends_with(name));
                for joint in &ragdoll.constraints {
                    let (a, b) = match &joint.data {
                        NpConstraintData::Ragdoll(r) => (r.pivot_a, r.pivot_b),
                        NpConstraintData::LimitedHinge(h) => (h.pivot_a, h.pivot_b),
                    };
                    let wa = world_pivot(&ragdoll, joint.body_a, a);
                    let wb = world_pivot(&ragdoll, joint.body_b, b);
                    let d = ((wa[0] - wb[0]).powi(2) + (wa[1] - wb[1]).powi(2) + (wa[2] - wb[2]).powi(2)).sqrt();
                    joints += 1;
                    // Metres; 1 cm.
                    if d > 0.01 {
                        disagreeing += 1;
                        if must_agree {
                            failures.push(format!(
                                "{file}: joint {} <-> {} pivots disagree by {d:.4} m at the bind pose",
                                joint.body_a, joint.body_b
                            ));
                        }
                    }
                }
            }
        }
    }
    eprintln!("[fo4] {blobs} ragdoll blobs, {joints} joints decoded, {disagreeing} disagree by more than 1 cm");
    assert!(blobs >= 40, "the vanilla corpus carries dozens of ragdoll skeletons, found {blobs}");
    assert!(failures.is_empty(), "{} failure(s):\n{}", failures.len(), failures.join("\n"));
    // Measured 2026-10-19: 783 of 847 agree (92.4 %). A decode that misreads
    // offsets collapses this far below the machines-and-props residue.
    assert!(
        disagreeing * 100 <= joints * 15,
        "{disagreeing} of {joints} joints disagree — the layout has drifted"
    );
}

#[test]
#[ignore = "needs Fallout 4 game data on disk"]
fn the_human_skeleton_imports_as_eighteen_named_bodies_and_seventeen_joints() {
    let Some(archives) = open_all_mesh_archives(Game::Fallout4) else {
        return;
    };
    let bytes = archives
        .iter()
        .find_map(|(_, a)| a.extract("meshes\\actors\\character\\characterassets\\skeleton.nif").ok())
        .expect("the human skeleton");
    let scene = byroredux_nif::parse_nif(&bytes).expect("parses");
    let ragdoll = byroredux_nif::import::collision::extract_ragdoll(&scene).expect("a ragdoll");
    assert_eq!(ragdoll.bodies.len(), 18);
    assert_eq!(ragdoll.constraints.len(), 17);
    let names: Vec<&str> = ragdoll.bodies.iter().map(|b| &*b.bone_name).collect();
    for expected in ["COM", "LLeg_Thigh", "RLeg_Calf", "SPINE2", "Chest", "Neck", "HEAD", "LArm_Hand", "RArm_ForeArm1"] {
        assert!(names.contains(&expected), "{expected} missing from {names:?}");
    }
    // Pelvis 18 kg, thighs 4 kg, hands 2 kg — the values the volume×density
    // check reproduced to 1e-7 when the layout was derived.
    let mass = |bone: &str| ragdoll.bodies.iter().find(|b| &*b.bone_name == bone).unwrap().mass;
    assert!((mass("COM") - 18.0).abs() < 1e-3);
    assert!((mass("LLeg_Thigh") - 4.0).abs() < 1e-3);
    assert!((mass("LArm_Hand") - 2.0).abs() < 1e-3);
    // Every body of this skeleton sits on its bone.
    assert!(ragdoll.bodies.iter().all(|b| !b.is_t));
}
