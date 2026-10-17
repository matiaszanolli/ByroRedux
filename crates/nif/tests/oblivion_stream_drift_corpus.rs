//! #3711 (NIF-2026-08-30-D1-01) — real-corpus regression pin for the
//! `no_block_sizes` stream-drift detector (`drift_warning` /
//! `FIXED_SIZE_BLOCK_TYPES`, `crates/nif/src/lib.rs`).
//!
//! Pre-fix, the detector armed whenever a type's first couple of
//! same-file instances happened to agree within ±2 bytes — a heuristic
//! that gave a **100% false-positive rate** on a real 9,612-file Oblivion
//! corpus (base + all eight DLC archives): 4,280 warnings, 0 real drift.
//! The fix restricts firing to [`FIXED_SIZE_BLOCK_TYPES`], a list measured
//! (not guessed) against that same corpus. This test is what keeps that
//! measurement honest going forward: it re-parses the real corpus and
//! asserts the warning count is still zero.
//!
//! Unlike `per_block_baselines.rs`'s `Game::mesh_archives()` (which
//! deliberately restricts Oblivion to the base `Oblivion - Meshes.bsa`
//! under an all-or-nothing DLC gate — see #2334/#3712), this test opens
//! every archive it can find directly and accumulates across whichever
//! subset is present, since a drift-warning count is additive rather than
//! a corpus-size gate: running against just the base archive on a
//! non-GOTY install is still a meaningful (if smaller) regression check,
//! not a misleading one.
//!
//! #5439 — the data directory resolves through `common::game_data_dir` (so the
//! `BYROREDUX_REQUIRE_GAME_DATA` strict lane and the binding-override rule of
//! #3850 apply), an archive that is present but unopenable panics instead of
//! reading as absent, and a host without the base `Oblivion - Meshes.bsa` skips
//! (or fails, strict) rather than running the detector over a DLC alone.
//!
//! `#[ignore]` — needs real Oblivion game data on disk; run with:
//! `cargo test -p byroredux-nif --test oblivion_stream_drift_corpus -- --ignored --nocapture`

mod common;

use common::Game;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

static DRIFT_COUNT: AtomicUsize = AtomicUsize::new(0);
static FIRST_FEW: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Eight Tamriel terrain-LOD files have a correct root footer followed by
/// stale bytes left by an older, longer file at the same archive path.
const STALE_TAIL_LOD_FILES: &[&str] = &[
    r"meshes\landscape\lod\60.32.-32.32.nif",
    r"meshes\landscape\lod\60.-64.-64.32.nif",
    r"meshes\landscape\lod\60.00.-64.32.nif",
    r"meshes\landscape\lod\60.00.-32.32.nif",
    r"meshes\landscape\lod\60.32.-64.32.nif",
    r"meshes\landscape\lod\60.-32.00.32.nif",
    r"meshes\landscape\lod\60.-96.-64.32.nif",
    r"meshes\landscape\lod\60.-32.-32.32.nif",
];

#[test]
fn optional_oblivion_mesh_archives_skip_absent_and_reject_present_invalid_files() {
    let data_dir = std::env::temp_dir().join(format!(
        "byroredux-oblivion-optional-archives-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&data_dir).expect("create temporary Oblivion data dir");
    let absent = common::open_optional_mesh_archives_from(Game::Oblivion, &data_dir);
    assert!(absent.is_empty(), "absent optional DLC archives remain skipped");

    let invalid_archive = data_dir.join("DLCFrostcrag.bsa");
    std::fs::write(&invalid_archive, b"not a BSA archive").expect("write invalid BSA");
    let failed_open = std::panic::catch_unwind(|| {
        common::open_optional_mesh_archives_from(Game::Oblivion, &data_dir)
    });
    assert!(
        failed_open.is_err(),
        "a present but unopenable vanilla Oblivion DLC archive must fail the corpus gate"
    );

    std::fs::write(data_dir.join("_ResourcePack.bsa"), b"not a BSA archive")
        .expect("write invalid Skyrim SE archive");
    assert!(
        common::open_optional_mesh_archives_from(Game::SkyrimSE, &data_dir).is_empty(),
        "the account-varying Skyrim SE optional tier remains tolerant"
    );

    std::fs::remove_dir_all(&data_dir).expect("remove temporary Oblivion data dir");
}

struct CountingLogger;

impl log::Log for CountingLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        let msg = format!("{}", record.args());
        if msg.starts_with("Stream drift suspect") {
            DRIFT_COUNT.fetch_add(1, Ordering::Relaxed);
            let mut guard = FIRST_FEW.lock().unwrap();
            if guard.len() < 10 {
                guard.push(msg);
            }
        }
    }
    fn flush(&self) {}
}

static LOGGER: CountingLogger = CountingLogger;

#[test]
#[ignore = "needs Oblivion game data on disk"]
fn no_block_sizes_drift_detector_has_zero_false_positives_on_real_corpus() {
    // `set_logger` can only be called once per process; a re-run within
    // the same test binary (unlikely for a single `#[ignore]`d test, but
    // cheap to guard) would otherwise panic instead of skipping cleanly.
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Warn);

    let Some(data_dir) = common::game_data_dir(Game::Oblivion) else {
        return;
    };
    if !data_dir.join(Game::Oblivion.mesh_archive()).is_file() {
        // A DLC-only directory is not an install: the drift count over it
        // would be a smaller, misleading sample (#5439).
        common::require_game_data(Game::Oblivion.env_var(), &data_dir.join(Game::Oblivion.mesh_archive()));
        eprintln!("skipping: no base {} under {data_dir:?}", Game::Oblivion.mesh_archive());
        return;
    }

    // Base game + all eight DLC archives that carry meshes. Each opened
    // independently — a DLC archive that is absent (non-GOTY install) just
    // contributes zero files rather than skipping the whole test; one that is
    // present but unopenable panics (#5439).
    const ARCHIVE_NAMES: &[&str] = &[
        "Oblivion - Meshes.bsa",
        "Knights.bsa",
        "DLCBattlehornCastle.bsa",
        "DLCFrostcrag.bsa",
        "DLCHorseArmor.bsa",
        "DLCOrrery.bsa",
        "DLCShiveringIsles - Meshes.bsa",
        "DLCThievesDen.bsa",
        "DLCVileLair.bsa",
    ];

    let mut total_files = 0usize;
    let mut archives_opened = 0usize;
    let mut footer_mismatches = Vec::new();
    let mut stale_tail_files_seen = BTreeSet::new();
    let mut footer_files_checked = 0usize;
    let mut extract_failures = 0usize;
    let mut parse_failures = 0usize;
    let mut failure_samples = Vec::new();

    for name in ARCHIVE_NAMES {
        let path = data_dir.join(name);
        if !path.is_file() {
            continue;
        }
        let archive = common::open_present_archive(Game::Oblivion.label(), &path, common::ArchiveKind::Bsa);
        archives_opened += 1;
        let files: Vec<String> = archive
            .list_files()
            .into_iter()
            .filter(|p| byroredux_nif::corpus::is_nif_entry(p))
            .map(|p| p.to_string())
            .collect();
        for f in &files {
            total_files += 1;
            let bytes = match archive.extract(f) {
                Ok(bytes) => bytes,
                Err(error) => {
                    extract_failures += 1;
                    if failure_samples.len() < 10 {
                        failure_samples.push(format!("{name}:{f} extract: {error}"));
                    }
                    continue;
                }
            };
            let (scene, consumed) = match byroredux_nif::parse_nif_with_options_and_consumed(
                &bytes,
                &byroredux_nif::ParseOptions::default(),
            ) {
                Ok(parsed) => parsed,
                Err(error) => {
                    parse_failures += 1;
                    if failure_samples.len() < 10 {
                        failure_samples.push(format!("{name}:{f} parse: {error}"));
                    }
                    continue;
                }
            };
            let Ok((header, block_data_offset)) = byroredux_nif::header::NifHeader::parse(&bytes)
            else {
                parse_failures += 1;
                if failure_samples.len() < 10 {
                    failure_samples.push(format!("{name}:{f} header parse failed after NIF parse"));
                }
                continue;
            };
            if header.version < byroredux_nif::version::NifVersion::V5_0_0_1
                || !header.block_sizes.is_empty()
                || header.num_blocks == 0
            {
                continue;
            }
            footer_files_checked += 1;
            if scene.truncated || scene.root_index != Some(0) {
                footer_mismatches.push(format!(
                    "{name}:{f} (truncated={}, root_index={:?}, expected first block)",
                    scene.truncated, scene.root_index,
                ));
            }
            let footer_start = block_data_offset + consumed;
            if footer_start + 4 > bytes.len() {
                footer_mismatches.push(format!("{name}:{f} (footer starts past EOF)"));
                continue;
            }
            let root_count = u32::from_le_bytes(
                bytes[footer_start..footer_start + 4]
                    .try_into()
                    .expect("four-byte root count"),
            ) as usize;
            let Some(expected_footer_bytes) = root_count
                .checked_mul(4)
                .and_then(|root_bytes| root_bytes.checked_add(4))
            else {
                footer_mismatches.push(format!("{name}:{f} (root footer size overflows)"));
                continue;
            };
            let remaining = bytes.len() - footer_start;
            let is_stale_tail = STALE_TAIL_LOD_FILES.contains(&f.to_ascii_lowercase().as_str());
            if remaining != expected_footer_bytes {
                if is_stale_tail && remaining > expected_footer_bytes {
                    stale_tail_files_seen.insert(f.to_ascii_lowercase());
                } else {
                    footer_mismatches.push(format!(
                        "{name}:{f} (remaining={remaining}, expected={expected_footer_bytes}, roots={root_count}, truncated={}, root_index={:?})",
                        scene.truncated,
                        scene.root_index,
                    ));
                }
            } else if is_stale_tail {
                footer_mismatches.push(format!(
                    "{name}:{f} is allowlisted but no longer has stale trailing bytes"
                ));
            }
        }
    }

    if archives_opened == 0 {
        eprintln!("skipping: no Oblivion mesh archive opened under {data_dir:?}");
        return;
    }

    let count = DRIFT_COUNT.load(Ordering::Relaxed);
    eprintln!(
        "oblivion_stream_drift_corpus: {archives_opened} archive(s), {total_files} NIF(s), \
         {count} drift warning(s)"
    );
    assert_eq!(
        count,
        0,
        "the no_block_sizes drift detector fired {count} time(s) on a real Oblivion \
         corpus — either FIXED_SIZE_BLOCK_TYPES now includes a type that isn't actually \
         fixed-size (re-measure and shrink the list), or there's real drift to \
         investigate. First few: {:#?}",
        FIRST_FEW.lock().unwrap(),
    );
    assert!(footer_files_checked > 0, "no sizeless Oblivion footer was checked");
    assert_eq!(
        extract_failures, 0,
        "failed to extract {extract_failures} of {total_files} NIF entries; first failures: {failure_samples:#?}",
    );
    assert_eq!(
        parse_failures, 0,
        "failed to parse {parse_failures} of {total_files} NIF entries; first failures: {failure_samples:#?}",
    );
    assert!(
        footer_mismatches.is_empty(),
        "sizeless Oblivion NIFs must end at the root footer, except for the eight known stale-tail terrain LOD files: {footer_mismatches:#?}",
    );
    assert_eq!(
        stale_tail_files_seen,
        STALE_TAIL_LOD_FILES
            .iter()
            .map(|path| path.to_string())
            .collect(),
        "the stale-tail allowlist must match the measured corpus exactly",
    );
}
