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

/// Data-backed count floor for the vanilla humanoid skeleton. Every
/// classic-chain game ships an 18-body / 17-joint `_male`/character
/// articulation — measured 2026-07-05 across Oblivion (10 Ragdoll +
/// 7 LimitedHinge), FNV (9 + 8), and Skyrim SE (9 + 8). Assert AT the
/// measured count with `>=` (not `==`) so:
///   - a silent joint-drop regression (the #1850 breakable-constraint path,
///     an `Other`-decode regression, or a finite-guard slip) shrinks
///     `constraints.len()` below the floor and trips the test — the exact
///     protection FNV-D7-03 (#1851) asks for, now extended to Oblivion +
///     Skyrim (the sibling arms that previously had no count pin at all);
///   - a future parser improvement that surfaces MORE joints (e.g. rebuilding
///     the breakable-wrapped inner joint per #1850) does NOT false-fail, which
///     a brittle `==` pin would.
/// A floor with slack *below* the measured value (e.g. `>= 16`) would let a
/// single-joint drop pass, defeating the purpose — so the floor sits exactly
/// at the measured count.
fn assert_reference_counts(
    game: Game,
    ragdoll: &ImportedRagdoll,
    min_bodies: usize,
    min_joints: usize,
) {
    assert!(
        ragdoll.bodies.len() >= min_bodies,
        "{game:?}: expected >= {min_bodies} ragdoll bodies (measured reference), got {}",
        ragdoll.bodies.len(),
    );
    assert!(
        ragdoll.constraints.len() >= min_joints,
        "{game:?}: expected >= {min_joints} joints (measured reference), got {} — \
         a shrink below the measured floor signals a silent joint drop",
        ragdoll.constraints.len(),
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
    assert_reference_counts(Game::FalloutNV, &ragdoll, 18, 17);
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
    // Measured reference (2026-07-05): 18 bodies, 17 joints (10 Ragdoll +
    // 7 LimitedHinge). Pins the Oblivion arm against a silent joint drop —
    // the sibling gap FNV-D7-03 (#1851) flagged.
    assert_reference_counts(Game::Oblivion, &ragdoll, 18, 17);
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
    // Measured reference (2026-07-05): 18 bodies, 17 joints (9 Ragdoll +
    // 8 LimitedHinge). Pins the Skyrim arm against a silent joint drop —
    // the sibling gap FNV-D7-03 (#1851) flagged.
    assert_reference_counts(Game::SkyrimSE, &ragdoll, 18, 17);
}

// ── #1851 — CI-runnable pins for `assert_reference_counts` itself ──────
// These need no game data: they prove the measured-count floor actually
// trips on a dropped joint (and doesn't false-fail on a healthy or larger
// graph), so the real-data arms above are guarding what they claim to.

use byroredux_core::ecs::components::collision::CollisionShape;
use byroredux_core::math::{Quat, Vec3};
use byroredux_nif::import::{ImportedRagdollBody, ImportedRagdollConstraint};

/// Synthetic ragdoll with `n_bodies` trivial bodies + `n_joints` trivial
/// LimitedHinge joints — only the `.len()`s matter to the floor.
fn synthetic_ragdoll(n_bodies: usize, n_joints: usize) -> ImportedRagdoll {
    let bodies = (0..n_bodies)
        .map(|i| ImportedRagdollBody {
            bone_name: format!("Bone{i}").into(),
            mass: 1.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            friction: 0.0,
            restitution: 0.0,
            shape: CollisionShape::Ball { radius: 1.0 },
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
        })
        .collect();
    let constraints = (0..n_joints)
        .map(|i| ImportedRagdollConstraint {
            body_a: i,
            body_b: i + 1,
            kind: ImportedJointKind::LimitedHinge {
                axis_a: Vec3::ZERO,
                pivot_a: Vec3::ZERO,
                axis_b: Vec3::ZERO,
                pivot_b: Vec3::ZERO,
                min_angle: 0.0,
                max_angle: 0.0,
            },
        })
        .collect();
    ImportedRagdoll { bodies, constraints }
}

/// The floor passes exactly at the measured count and on any larger graph
/// (e.g. a future #1850 improvement surfacing an extra joint).
#[test]
fn reference_floor_passes_at_and_above_measured_count() {
    assert_reference_counts(Game::FalloutNV, &synthetic_ragdoll(18, 17), 18, 17);
    assert_reference_counts(Game::FalloutNV, &synthetic_ragdoll(19, 18), 18, 17);
}

/// The floor trips the moment a joint is dropped below the measured count —
/// the exact silent-regression FNV-D7-03 (#1851) exists to catch.
#[test]
#[should_panic(expected = "silent joint drop")]
fn reference_floor_trips_on_a_dropped_joint() {
    // 18 bodies but only 16 joints (one silently dropped) → below the floor.
    assert_reference_counts(Game::FalloutNV, &synthetic_ragdoll(18, 16), 18, 17);
}
