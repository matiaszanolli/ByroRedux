//! Real-data validation for M41.x Phase 2 — the FNV humanoid skeleton's
//! Havok ragdoll must thread end-to-end into `ImportedScene.ragdoll`:
//! 18 capsule bodies + 17 joints (3 bare `bhkRagdollConstraint` + 14
//! `bhkMalleableConstraint` wrapping a Ragdoll), all decoded as Ragdoll
//! joints, every body resolving a bone name + shape.
//!
//! `#[ignore]` because it needs vanilla FNV game data; run with
//! `cargo test -p byroredux-nif --test ragdoll_import -- --ignored --nocapture`.

mod common;

use common::{open_mesh_archive, Game};

use byroredux_core::string::StringPool;
use byroredux_nif::import::{import_nif_scene, ImportedJointKind};
use byroredux_nif::parse_nif;

const SKELETON_PATH: &str = r"meshes\characters\_male\skeleton.nif";

#[test]
#[ignore]
fn fnv_humanoid_skeleton_threads_ragdoll() {
    let Some(archive) = open_mesh_archive(Game::FalloutNV) else {
        return;
    };
    let bytes = archive
        .extract(SKELETON_PATH)
        .expect("FNV mesh archive must contain _male/skeleton.nif");
    let scene = parse_nif(&bytes).expect("skeleton.nif must parse");
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);

    let ragdoll = imported
        .ragdoll
        .expect("FNV humanoid skeleton must thread a ragdoll articulation");

    eprintln!(
        "FNV _male skeleton ragdoll: {} bodies, {} joints",
        ragdoll.bodies.len(),
        ragdoll.constraints.len(),
    );
    let names: Vec<&str> = ragdoll.bodies.iter().map(|b| b.bone_name.as_ref()).collect();
    eprintln!("ragdoll bones: {names:?}");

    // Vanilla FNV _male skeleton: 18 capsule bodies, 17 joints.
    assert_eq!(ragdoll.bodies.len(), 18, "expected 18 ragdoll bodies");
    assert_eq!(ragdoll.constraints.len(), 17, "expected 17 joints");

    for b in &ragdoll.bodies {
        assert!(!b.bone_name.is_empty(), "ragdoll body missing a bone name");
        assert!(b.mass >= 0.0, "negative mass on {}", b.bone_name);
    }
    for c in &ragdoll.constraints {
        assert!(
            c.body_a < ragdoll.bodies.len() && c.body_b < ragdoll.bodies.len(),
            "joint references an out-of-range body",
        );
        assert_ne!(c.body_a, c.body_b, "joint links a body to itself");
    }

    // Every joint decodes to a real kind — the `constraints` vec only holds
    // successfully-decoded Ragdoll/LimitedHinge (Other is dropped), so a
    // count of 17 already means none silently dropped. The FNV humanoid is
    // a mix: ball-joints for spine/neck/shoulders/hips (Ragdoll) and
    // angle-limited hinges for elbows/knees (LimitedHinge) — both reached
    // through the malleable wrapper.
    let ragdoll_joints = ragdoll
        .constraints
        .iter()
        .filter(|c| matches!(c.kind, ImportedJointKind::Ragdoll { .. }))
        .count();
    let hinge_joints = ragdoll
        .constraints
        .iter()
        .filter(|c| matches!(c.kind, ImportedJointKind::LimitedHinge { .. }))
        .count();
    eprintln!("joints: {ragdoll_joints} Ragdoll + {hinge_joints} LimitedHinge");
    assert_eq!(
        ragdoll_joints + hinge_joints,
        17,
        "every joint must decode to Ragdoll or LimitedHinge, none dropped",
    );
    assert!(
        hinge_joints > 0,
        "FNV humanoid elbows/knees should decode as LimitedHinge",
    );

    // Bone names round-trip (sanity that the host-NiNode mapping works).
    assert!(
        names.iter().any(|n| n.contains("Bip01") || n.contains("Spine")),
        "expected a Bip01/Spine bone among {names:?}",
    );
}
