//! #3713 (NIF-2026-08-30-D5-01) — real-corpus regression pin for the
//! decoded `bhk*Constraint` motor-tail drift assertion
//! (`byroredux_nif::corpus::is_known_constraint_motor_tail_drift`).
//!
//! `is_havok_constraint_stub` (`crates/nif/src/lib.rs`) used to suppress
//! stream-drift telemetry for nine constraint type names, including four
//! (`bhkRagdollConstraint`, `bhkLimitedHingeConstraint`,
//! `bhkHingeConstraint`, `bhkMalleableConstraint`) that had since grown
//! typed CInfo decoders — routing their by-design motor-tail residual into
//! `stubbed_drift_histogram`, invisible to every drift-based audit. That
//! suppression is exactly what hid the historic `bhkHingeConstraint` +128
//! under-read (a whole missing parser, not a motor tail) until #3330 found
//! it by hand.
//!
//! The fix narrowed the stub list to the genuinely name-only types and
//! replaced the suppression with an assertable known-good residual set.
//! This test is what keeps that measurement honest going forward: it
//! re-parses a real corpus across every game with `bhk*Constraint`
//! content (Oblivion, Fallout 3, Fallout NV, Skyrim SE — FO4/FO76/Starfield
//! ship none) and asserts every observed drift for a decoded type is a
//! known motor-tail value.
//!
//! `#[ignore]` — needs real game data on disk; run with:
//! `cargo test -p byroredux-nif --test constraint_drift_corpus -- --ignored --nocapture`

mod common;

use byroredux_nif::corpus::is_known_constraint_motor_tail_drift;
use common::{open_all_mesh_archives, Game};

/// The five `bhk*Constraint` types with typed CInfo decoders (see
/// `is_havok_constraint_stub`'s #3713 note in `lib.rs`).
const DECODED_CONSTRAINT_TYPES: &[&str] = &[
    "bhkRagdollConstraint",
    "bhkLimitedHingeConstraint",
    "bhkHingeConstraint",
    "bhkMalleableConstraint",
    "bhkPrismaticConstraint",
];

#[test]
#[ignore = "needs real game data on disk"]
fn decoded_constraint_drift_is_always_a_known_motor_tail_value() {
    let mut any_game_ran = false;
    let mut total_events = 0u64;
    let mut unexpected: Vec<String> = Vec::new();

    for game in [
        Game::Oblivion,
        Game::Fallout3,
        Game::FalloutNV,
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
                        if !is_known_constraint_motor_tail_drift(ty, drift) {
                            unexpected.push(format!(
                                "[{}] {archive_name}/{f}: {ty} drift={drift:+} \
                                 (x{count}) is not a known motor-tail value",
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
             Skyrim SE"
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
        "{} decoded-constraint drift value(s) fell outside the known motor-tail \
         set — this is what a real parser regression (like the historic \
         bhkHingeConstraint +128, a whole undecoded CInfo) looks like:\n{:#?}",
        unexpected.len(),
        unexpected,
    );
}
