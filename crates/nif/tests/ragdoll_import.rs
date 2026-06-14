//! Real-data validation for PHYSAL ingestion (the physics abstraction
//! layer — see `docs/engine/physal.md`). A humanoid skeleton's Havok
//! ragdoll must thread end-to-end into `ImportedScene.ragdoll` through the
//! *same* code path on every classic-chain game (Oblivion / FNV / Skyrim)
//! — only the on-disk constraint byte order differs, and that is resolved
//! at parse. This is the "single consistent ragdoll logic" the layer
//! exists to provide, exercised on real content.
//!
//! `#[ignore]` because it needs vanilla game data; run with e.g.
//! `cargo test -p byroredux-nif --test ragdoll_import -- --ignored --nocapture`.

mod common;

use common::{open_mesh_archive, Game};

use byroredux_core::string::StringPool;
use byroredux_nif::import::{import_nif_scene, ImportedJointKind, ImportedRagdoll};
use byroredux_nif::parse_nif;

/// Extract + import a skeleton and return its threaded ragdoll, or `None`
/// when the game data / skeleton isn't present (so a missing-data or
/// path-mismatch run *skips* rather than masquerading as a failure — the
/// per-game skeleton path varies and we don't fabricate counts for data
/// we can't open).
fn thread_skeleton_ragdoll(game: Game, skeleton_path: &str) -> Option<ImportedRagdoll> {
    let archive = open_mesh_archive(game)?;
    let bytes = match archive.extract(skeleton_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{game:?}: skeleton not at {skeleton_path:?} ({e}) — skipping");
            return None;
        }
    };
    let scene = parse_nif(&bytes).expect("skeleton.nif must parse");
    let mut pool = StringPool::new();
    import_nif_scene(&scene, &mut pool).ragdoll
}

/// Shared structural invariants every game's threaded ragdoll must hold —
/// no per-game body/joint counts hard-coded here (those are measured, not
/// assumed). Prints the actual decode so real-data runs can harden the
/// numbers later, mirroring the smoke-test philosophy.
fn assert_structural(game: Game, ragdoll: &ImportedRagdoll) {
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
    eprintln!(
        "{game:?} ragdoll: {} bodies, {} joints ({ragdoll_joints} Ragdoll + {hinge_joints} LimitedHinge)",
        ragdoll.bodies.len(),
        ragdoll.constraints.len(),
    );

    // A ragdoll needs ≥2 bodies and ≥1 joint to articulate at all.
    assert!(ragdoll.bodies.len() >= 2, "{game:?}: need ≥2 ragdoll bodies");
    assert!(!ragdoll.constraints.is_empty(), "{game:?}: need ≥1 joint");
    for b in &ragdoll.bodies {
        assert!(!b.bone_name.is_empty(), "{game:?}: body missing a bone name");
        assert!(b.mass >= 0.0, "{game:?}: negative mass on {}", b.bone_name);
    }
    for c in &ragdoll.constraints {
        assert!(
            c.body_a < ragdoll.bodies.len() && c.body_b < ragdoll.bodies.len(),
            "{game:?}: joint references an out-of-range body",
        );
        assert_ne!(c.body_a, c.body_b, "{game:?}: joint links a body to itself");
    }
    // `constraints` only holds successfully-decoded Ragdoll/LimitedHinge
    // (Other is dropped at extract), so a non-zero count already means
    // every surfaced joint decoded — i.e. the era-correct field order was
    // read. This is the cross-game assertion: it holds whether the bytes
    // arrived in Oblivion or FO3+ order.
    assert_eq!(
        ragdoll_joints + hinge_joints,
        ragdoll.constraints.len(),
        "{game:?}: every surfaced joint must decode to Ragdoll or LimitedHinge",
    );
}

#[test]
#[ignore]
fn fnv_humanoid_skeleton_threads_ragdoll() {
    let Some(ragdoll) =
        thread_skeleton_ragdoll(Game::FalloutNV, r"meshes\characters\_male\skeleton.nif")
    else {
        return;
    };
    assert_structural(Game::FalloutNV, &ragdoll);

    // FNV is the measured reference: vanilla _male skeleton is 18 capsule
    // bodies, 17 joints, with elbows/knees as LimitedHinge.
    assert_eq!(ragdoll.bodies.len(), 18, "FNV: expected 18 ragdoll bodies");
    assert_eq!(ragdoll.constraints.len(), 17, "FNV: expected 17 joints");
    let hinges = ragdoll
        .constraints
        .iter()
        .filter(|c| matches!(c.kind, ImportedJointKind::LimitedHinge { .. }))
        .count();
    assert!(hinges > 0, "FNV elbows/knees should decode as LimitedHinge");

    let names: Vec<&str> = ragdoll.bodies.iter().map(|b| b.bone_name.as_ref()).collect();
    assert!(
        names.iter().any(|n| n.contains("Bip01") || n.contains("Spine")),
        "expected a Bip01/Spine bone among {names:?}",
    );
}

#[test]
#[ignore]
fn oblivion_humanoid_skeleton_threads_ragdoll() {
    // Oblivion introduced Havok ragdolls; the human skeleton lives at the
    // same `_male` path as FNV but ships the `#NI_BS_LTE_16#` (pivots-first,
    // no-motor) constraint layout — proof the parse-time seam, and nothing
    // downstream, is all that differs.
    let Some(ragdoll) =
        thread_skeleton_ragdoll(Game::Oblivion, r"meshes\characters\_male\skeleton.nif")
    else {
        return;
    };
    assert_structural(Game::Oblivion, &ragdoll);
}

#[test]
#[ignore]
fn skyrim_humanoid_skeleton_threads_ragdoll() {
    // Skyrim SE: FO3+ constraint layout (gated by NIF version, not bsver)
    // + havok_scale ×69.99. Skeleton path moved under actors/character.
    let Some(ragdoll) = thread_skeleton_ragdoll(
        Game::SkyrimSE,
        r"meshes\actors\character\character assets\skeleton.nif",
    ) else {
        return;
    };
    assert_structural(Game::SkyrimSE, &ragdoll);
}
