//! #3713 / #5960 — real-corpus regression pin: every decoded `bhk*Constraint`
//! must consume exactly its declared `block_size`. Zero stream drift, on every
//! game that ships constraints.
//!
//! History. `is_havok_constraint_stub` (since removed, #4626) once suppressed
//! drift telemetry for nine constraint type names, hiding the historic
//! `bhkHingeConstraint` +128 under-read (a whole missing parser, not a motor
//! tail) until #3330 found it by hand. #3713 replaced the suppression with an
//! assertable allowlist of by-design "motor left for `block_size` recovery"
//! residuals (+1 `MOTOR_NONE`, +18 / +19 / +26 for the spring / velocity /
//! position motors, and Malleable's +4 `Strength` stacked on those). That
//! allowlist was the *measured* shape of the gap, and it was a gap: 100% of
//! Skyrim's 1,575 Ragdoll / LimitedHinge blocks parsed one byte short, and
//! every one logged a realignment WARN.
//!
//! #5960 consumes the trailing `bhkConstraintMotorCInfo` (and Malleable's
//! `Strength`), so the allowlist is gone and the assertion is the strict one:
//! any residual on a decoded constraint is a parser regression.
//!
//! Covers Oblivion, Fallout 3, Fallout NV, Skyrim LE and Skyrim SE (FO4 /
//! FO76 / Starfield ship none).
//!
//! `#[ignore]` — needs real game data on disk; run with:
//! `cargo test -p byroredux-nif --test constraint_drift_corpus -- --ignored --nocapture`

mod common;

use common::{open_all_mesh_archives, Game};

/// The `bhk*Constraint` types with typed CInfo decoders (#3713, #4212).
///
/// The first five carry a trailing `bhkConstraintMotorCInfo` (consumed since
/// #5960); the last three have no motor field at all. Every one must now
/// drift by exactly 0.
const DECODED_CONSTRAINT_TYPES: &[&str] = &[
    "bhkRagdollConstraint",
    "bhkLimitedHingeConstraint",
    "bhkHingeConstraint",
    "bhkMalleableConstraint",
    "bhkPrismaticConstraint",
    "bhkBallAndSocketConstraint",
    "bhkStiffSpringConstraint",
    "bhkBallSocketConstraintChain",
];

#[test]
#[ignore = "needs real game data on disk"]
fn decoded_constraints_consume_their_whole_block() {
    let mut any_game_ran = false;
    let mut total_events = 0u64;
    let mut unexpected: Vec<String> = Vec::new();

    for game in [
        Game::Oblivion,
        Game::Fallout3,
        Game::FalloutNV,
        Game::SkyrimLE,
        Game::SkyrimSE,
    ] {
        let Some(archives) = open_all_mesh_archives(game) else {
            continue;
        };
        any_game_ran = true;

        for (archive_name, archive) in &archives {
            let files: Vec<String> = archive
                .list_files()
                .into_iter()
                .filter(|p| byroredux_nif::corpus::is_nif_entry(p))
                .collect();
            for f in &files {
                let Ok(bytes) = archive.extract(f) else {
                    continue;
                };
                let Ok(scene) = byroredux_nif::parse_nif(&bytes) else {
                    continue;
                };
                for &ty in DECODED_CONSTRAINT_TYPES {
                    let Some(buckets) = scene.drift_histogram.get(ty) else {
                        continue;
                    };
                    for (&drift, &count) in buckets {
                        total_events += count as u64;
                        if drift != 0 {
                            unexpected.push(format!(
                                "[{}] {archive_name}/{f}: {ty} drift={drift:+} (x{count})",
                                game.label(),
                            ));
                        }
                    }
                }
            }
        }
    }

    if !any_game_ran {
        eprintln!(
            "skipping: no game data found for Oblivion / Fallout 3 / Fallout NV / \
             Skyrim LE / Skyrim SE"
        );
        return;
    }

    eprintln!(
        "constraint_drift_corpus: {total_events} decoded-constraint drift event(s) observed \
         across {} type(s)",
        DECODED_CONSTRAINT_TYPES.len(),
    );
    assert!(
        unexpected.is_empty(),
        "{} decoded-constraint block(s) drifted from their declared size — the \
         trailing motor / Strength is no longer consumed (#5960), or a CInfo is \
         under-read (like the historic bhkHingeConstraint +128):\n{:#?}",
        unexpected.len(),
        unexpected,
    );
}
