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
//! `#[ignore]` — needs real Oblivion game data on disk; run with:
//! `cargo test -p byroredux-nif --test oblivion_stream_drift_corpus -- --ignored --nocapture`

use byroredux_bsa::BsaArchive;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

static DRIFT_COUNT: AtomicUsize = AtomicUsize::new(0);
static FIRST_FEW: Mutex<Vec<String>> = Mutex::new(Vec::new());

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

    let data_dir = std::env::var("BYROREDUX_OBLIVION_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from("/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data")
        });
    if !data_dir.is_dir() {
        eprintln!("skipping: Oblivion data dir not found at {data_dir:?}");
        return;
    }

    // Base game + all eight DLC archives that carry meshes. Each opened
    // independently — a missing DLC archive (non-GOTY install) just
    // contributes zero files rather than skipping the whole test.
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

    for name in ARCHIVE_NAMES {
        let path = data_dir.join(name);
        let archive = match BsaArchive::open(&path) {
            Ok(a) => a,
            Err(_) => continue,
        };
        archives_opened += 1;
        let files: Vec<String> = archive
            .list_files()
            .into_iter()
            .filter(|p| byroredux_nif::corpus::is_nif_entry(p))
            .map(|p| p.to_string())
            .collect();
        for f in &files {
            total_files += 1;
            let Ok(bytes) = archive.extract(f) else {
                continue;
            };
            let _ = byroredux_nif::parse_nif(&bytes);
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
}
