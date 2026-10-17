//! Integration tests exercising the FaceGen parsers against real
//! vanilla FNV / FO3 content. Gated `#[ignore]` because the
//! fixtures live in proprietary BSAs that can't ship in the repo.
//!
//! Opt in:
//!
//! ```bash
//! cargo test -p byroredux-facegen --test parse_real_facegen -- --ignored
//! ```
//!
//! `BYROREDUX_FNV_DATA` / `BYROREDUX_FO3_DATA` may override the default
//! Steam install paths; each game self-skips when its data dir isn't
//! present, so a CI environment without the game library doesn't fail the
//! run and a host with only one of the two still gates on that one.
//!
//! #2335 — the doc above used to claim FNV/FO3 coverage while `data_dir()`
//! read only `BYROREDUX_FNV_DATA` and the BSA path was hardcoded under the
//! FNV install, so **FO3 was never exercised**. It passed when pointed at an
//! FO3 install purely because FO3 and FNV ship byte-identical
//! `headhuman.{egm,egt,tri}` — asset reuse, not something the test
//! structurally guaranteed. Nothing would have caught a future FO3-only face
//! asset (ghoul / super-mutant / robot) diverging.
//!
//! The fix mirrors the `Game`-enum parametrization already used by
//! `crates/nif/tests/parse_real_nifs.rs` and `crates/spt/tests/parse_real_spt.rs`:
//! every case carries its own expectations, so the two games agreeing is an
//! *assertion* rather than an accident of shared assets.

use byroredux_bsa::BsaArchive;
use byroredux_facegen::{EgmFile, EgtFile, TriHeader};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug)]
enum Game {
    FalloutNV,
    Fallout3,
}

/// Per-game expectations for the shared `headhuman` head assets.
///
/// FNV values verified 2026-04-29 against unmodded FNV GOTY; FO3 values
/// measured 2026-08-26 against unmodded FO3 GOTY and found byte-identical.
/// They are listed per game **on purpose** — the day FO3 ships a different
/// head, this table is what notices.
struct Expected {
    egm_bytes: usize,
    egm_vertices: u32,
    egm_sym_morphs: usize,
    egm_asym_morphs: usize,
    egt_bytes: usize,
    egt_width: u32,
    egt_height: u32,
    egt_fgts_morphs: usize,
    tri_vertices: u32,
    tri_triangles: u32,
}

/// #3850 — the strict lane for real-data tests.
///
/// `BYROREDUX_REQUIRE_GAME_DATA=1` turns an absent corpus into a hard
/// failure instead of a silent libtest `ok`. Without it the `--ignored`
/// lane — the only lane these `#[ignore]`d tests ever execute in —
/// records a pass for a test that never touched a byte of game data, so
/// a green run is not evidence of anything. This is the Rust-side
/// counterpart of the shell gates' "explicit SKIP with exit code 77,
/// never a pass" rule (`docs/smoke-tests/README.md`, #3003).
#[track_caller]
fn require_game_data(env_var: &str, tried: &std::path::Path) {
    if std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok_and(|v| v != "0") {
        panic!(
            "BYROREDUX_REQUIRE_GAME_DATA is set, but no game data was found: \
             {env_var} is unset (or names a non-directory) and the default \
             {tried:?} is not a directory"
        );
    }
}

/// #4660 — the data directory resolved but lacks an archive the test needs.
/// Under the strict lane that is a failure naming the file, not a green skip.
#[track_caller]
fn require_archive(path: &std::path::Path) {
    if std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok_and(|v| v != "0") {
        panic!("BYROREDUX_REQUIRE_GAME_DATA is set, but {path:?} is not a file");
    }
    eprintln!("skipping: {path:?} not found");
}

impl Game {
    const ALL: &'static [Game] = &[Game::FalloutNV, Game::Fallout3];

    fn label(self) -> &'static str {
        match self {
            Game::FalloutNV => "FNV",
            Game::Fallout3 => "FO3",
        }
    }

    fn env_var(self) -> &'static str {
        match self {
            Game::FalloutNV => "BYROREDUX_FNV_DATA",
            Game::Fallout3 => "BYROREDUX_FO3_DATA",
        }
    }

    /// Canonical Steam install path on the reference development machine —
    /// a fallback when the env var is unset, never a hard assumption.
    fn default_path(self) -> &'static str {
        match self {
            Game::FalloutNV => "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data",
            Game::Fallout3 => "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data",
        }
    }

    /// Both titles ship their head assets in an identically-named archive;
    /// spelled out per game anyway so a divergence is a one-line change.
    fn mesh_bsa(self) -> &'static str {
        match self {
            Game::FalloutNV | Game::Fallout3 => "Fallout - Meshes.bsa",
        }
    }

    fn expected(self) -> Expected {
        match self {
            Game::FalloutNV | Game::Fallout3 => Expected {
                egm_bytes: 695_904,
                egm_vertices: 1449,
                egm_sym_morphs: 50,
                egm_asym_morphs: 30,
                egt_bytes: 9_830_664,
                egt_width: 256,
                egt_height: 256,
                egt_fgts_morphs: 50,
                tri_vertices: 1211,
                tri_triangles: 2294,
            },
        }
    }

    fn data_dir(self) -> Option<PathBuf> {
        if let Some(val) = std::env::var(self.env_var()).ok().filter(|s| !s.is_empty()) {
            let p = PathBuf::from(&val);
            if p.is_dir() {
                return Some(p);
            }
            panic!(
                "{} points to {:?}, which is not a directory",
                self.env_var(),
                p
            );
        }
        let p = PathBuf::from(self.default_path());
        if p.is_dir() {
            return Some(p);
        }
        require_game_data(self.env_var(), &p);
        None
    }

    /// `None` only when this title's data is legitimately absent (and never
    /// under the strict lane). #4660 — a present install whose archive fails
    /// to open, or whose vanilla entry fails to extract, is a reader
    /// regression; both used to surface as "data not available; skipping".
    fn extract(self, inner: &str) -> Option<Vec<u8>> {
        let dir = self.data_dir()?;
        let bsa_path = dir.join(self.mesh_bsa());
        if !bsa_path.is_file() {
            require_archive(&bsa_path);
            return None;
        }
        let g = self.label();
        let archive = BsaArchive::open(&bsa_path)
            .unwrap_or_else(|e| panic!("[{g}] failed to open {bsa_path:?}: {e}"));
        let bytes = archive
            .extract(inner)
            .unwrap_or_else(|e| panic!("[{g}] failed to extract {inner} from {bsa_path:?}: {e}"));
        Some(bytes)
    }
}

/// Run `body` for every game whose data is on disk, and fail if *no* game
/// resolved — a run where both installs are absent is a legitimate skip, but
/// it must not look like a pass.
fn for_each_game(what: &str, body: impl Fn(Game, &Expected, Vec<u8>)) {
    let mut ran = 0;
    for &game in Game::ALL {
        let Some(bytes) = game.extract(what) else {
            eprintln!("[{}] data not available; skipping", game.label());
            continue;
        };
        eprintln!("[{}] {} — {} bytes", game.label(), what, bytes.len());
        body(game, &game.expected(), bytes);
        ran += 1;
    }
    if ran == 0 {
        eprintln!(
            "no game data available for any of {:?}; skipping",
            Game::ALL
        );
    }
}

// #4653 — the EGM deltas are scaled INT16s, so a correct decode yields
// ZERO non-finite components (the audit's 32,456-per-game NaN count was
// the f16 mis-decode's own artifact). Asserted in the test below.
const NV_EGM_NON_FINITE: usize = 0;
const FO3_EGM_NON_FINITE: usize = 0;

/// Count of non-finite delta components across every morph. #4653 — the EGM
/// deltas are scaled INT16s, so a correct decode yields none.
fn non_finite_components(egm: &EgmFile) -> usize {
    egm.fggs_morphs
        .iter()
        .chain(egm.fgga_morphs.iter())
        .map(|m| m.deltas.iter().flatten().filter(|c| !c.is_finite()).count())
        .sum()
}

/// #4653 — int16 deltas are always finite, and the SDK's full-range
/// quantisation means every morph peaks at |m| near 32767 (relative to the
/// morph's own scale). Shared by every game's EGM case (#5011).
#[track_caller]
fn assert_egm_int16_quantised(g: &str, egm: &EgmFile) {
    let mut max_raw = 0.0f32;
    for morph in egm.fggs_morphs.iter().chain(egm.fgga_morphs.iter()) {
        for d in morph.deltas.iter().flatten() {
            max_raw = max_raw.max(d.abs());
        }
    }
    assert!(
        (30_000.0..=32_768.0).contains(&max_raw),
        "[{g}] every morph should quantise to near-full-range int16 \
         (expected peak ~32767, got {max_raw})"
    );
}

#[test]
#[ignore = "needs FNV/FO3 game data on disk"]
fn parse_vanilla_headhuman_egm() {
    for_each_game(
        r"meshes\characters\head\headhuman.egm",
        |game, exp, bytes| {
            let g = game.label();
            assert_eq!(
                bytes.len(),
                exp.egm_bytes,
                "[{g}] vanilla headhuman.egm baseline byte count drifted",
            );
            let egm = EgmFile::parse(&bytes).unwrap_or_else(|e| panic!("[{g}] parse: {e:?}"));
            assert_eq!(egm.num_vertices, exp.egm_vertices, "[{g}] egm vertices");
            assert_eq!(
                egm.fggs_morphs.len(),
                exp.egm_sym_morphs,
                "[{g}] egm symmetric morphs"
            );
            assert_eq!(
                egm.fgga_morphs.len(),
                exp.egm_asym_morphs,
                "[{g}] egm asymmetric morphs"
            );
            for morph in egm.fggs_morphs.iter().chain(egm.fgga_morphs.iter()) {
                assert_eq!(
                    morph.deltas.len(),
                    exp.egm_vertices as usize,
                    "[{g}] every morph must carry one delta per vertex"
                );
            }
            let nan_count = non_finite_components(&egm);
            assert_egm_int16_quantised(g, &egm);
            // #4665 (PAR-D4-2026-09-21-03) — this used to be an
            // eprintln, which could never catch a decode regression; the
            // PAR-D5-2026-09-21-01 EGM bug is exactly what it existed to
            // catch. The count is a property of the on-disk bytes, so it
            // must be pinned per game.
            eprintln!("[{g}] vanilla headhuman.egm: {nan_count} non-finite delta components");
            let expected_nan = match game {
                Game::FalloutNV => NV_EGM_NON_FINITE,
                Game::Fallout3 => FO3_EGM_NON_FINITE,
            };
            assert_eq!(
                nan_count, expected_nan,
                "[{g}] non-finite delta component count drifted — a half-\
                 float decode change"
            );
        },
    );
}

/// #4665 — Oblivion ships 141 EGMs and had no test at all. #5011 — it used
/// to resolve its data dir by hand and `return` on a miss, so it was green
/// in the strict lane having touched nothing, and it pinned only `> 0`
/// counts. It now goes through the same `require_*` gates as the FNV/FO3
/// cases and pins the measured baseline plus the #4653 int16 checks.
///
/// Oblivion keeps the head sidecars under the race folder, not
/// `characters\head` like FNV/FO3, and its data dir is not a `Game` variant
/// (its other head assets differ), so the resolution is spelled out here.
#[test]
#[ignore = "needs Oblivion game data on disk"]
fn parse_vanilla_headhuman_egm_oblivion() {
    const ENV_VAR: &str = "BYROREDUX_OBLIVION_DATA";
    const DEFAULT: &str = "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data";
    // Measured 2026-10-14 against unmodded Oblivion GOTY.
    const EXPECT_BYTES: usize = 833_664;
    const EXPECT_VERTICES: u32 = 1736;
    const EXPECT_SYM_MORPHS: usize = 50;
    const EXPECT_ASYM_MORPHS: usize = 30;

    let data = if let Some(v) = std::env::var(ENV_VAR).ok().filter(|s| !s.is_empty()) {
        // An explicitly-set override is binding (#3850).
        let p = PathBuf::from(v);
        assert!(p.is_dir(), "{ENV_VAR} points to {p:?}, which is not a directory");
        p
    } else {
        let p = PathBuf::from(DEFAULT);
        if !p.is_dir() {
            require_game_data(ENV_VAR, &p);
            eprintln!("skipping: Oblivion data not found");
            return;
        }
        p
    };
    let archive_path = data.join("Oblivion - Meshes.bsa");
    if !archive_path.is_file() {
        require_archive(&archive_path);
        return;
    }
    let archive = BsaArchive::open(&archive_path)
        .unwrap_or_else(|e| panic!("open Oblivion - Meshes.bsa: {e:?}"));
    let entry = archive
        .list_files()
        .into_iter()
        .find(|p| p.to_lowercase() == "meshes\\characters\\imperial\\headhuman.egm")
        .unwrap_or_else(|| panic!("headhuman.egm not in Oblivion - Meshes.bsa"));
    let bytes = archive
        .extract(entry)
        .unwrap_or_else(|e| panic!("extract headhuman.egm: {e:?}"));
    let egm = EgmFile::parse(&bytes).unwrap_or_else(|e| panic!("parse: {e:?}"));
    let g = "Oblivion";
    eprintln!(
        "[{g}] headhuman.egm: {} bytes, {} verts, {} sym + {} asym morphs",
        bytes.len(),
        egm.num_vertices,
        egm.fggs_morphs.len(),
        egm.fgga_morphs.len()
    );
    assert_eq!(bytes.len(), EXPECT_BYTES, "[{g}] vanilla headhuman.egm baseline byte count drifted");
    assert_eq!(egm.num_vertices, EXPECT_VERTICES, "[{g}] egm vertices");
    assert_eq!(egm.fggs_morphs.len(), EXPECT_SYM_MORPHS, "[{g}] egm symmetric morphs");
    assert_eq!(egm.fgga_morphs.len(), EXPECT_ASYM_MORPHS, "[{g}] egm asymmetric morphs");
    for morph in egm.fggs_morphs.iter().chain(egm.fgga_morphs.iter()) {
        assert_eq!(
            morph.deltas.len(),
            egm.num_vertices as usize,
            "[{g}] every morph must carry one delta per vertex"
        );
    }
    assert_eq!(
        non_finite_components(&egm),
        0,
        "[{g}] non-finite delta component count drifted — an int16 decode change"
    );
    assert_egm_int16_quantised(g, &egm);
}

#[test]
#[ignore = "needs FNV/FO3 game data on disk"]
fn parse_vanilla_headhuman_egt() {
    for_each_game(
        r"meshes\characters\head\headhuman.egt",
        |game, exp, bytes| {
            let g = game.label();
            assert_eq!(
                bytes.len(),
                exp.egt_bytes,
                "[{g}] vanilla headhuman.egt baseline byte count drifted",
            );
            let egt = EgtFile::parse(&bytes).unwrap_or_else(|e| panic!("[{g}] parse: {e:?}"));
            assert_eq!(egt.width, exp.egt_width, "[{g}] egt width");
            assert_eq!(egt.height, exp.egt_height, "[{g}] egt height");
            assert_eq!(
                egt.fgts_morphs.len(),
                exp.egt_fgts_morphs,
                "[{g}] egt texture morphs"
            );
            let expected_pixels = (exp.egt_width * exp.egt_height) as usize;
            for morph in &egt.fgts_morphs {
                assert_eq!(
                    morph.pixels.len(),
                    expected_pixels,
                    "[{g}] every texture morph must cover the full {}x{} tile",
                    exp.egt_width,
                    exp.egt_height
                );
            }
        },
    );
}

#[test]
#[ignore = "needs FNV/FO3 game data on disk"]
fn parse_vanilla_headhuman_tri_header() {
    for_each_game(
        r"meshes\characters\head\headhuman.tri",
        |game, exp, bytes| {
            let g = game.label();
            let hdr = TriHeader::parse(&bytes).unwrap_or_else(|e| panic!("[{g}] parse: {e:?}"));
            // Vanilla headhuman.nif has 1211 verts / 2294 tris — the .tri
            // header must agree.
            assert_eq!(hdr.num_vertices, exp.tri_vertices, "[{g}] tri vertices");
            assert_eq!(hdr.num_triangles, exp.tri_triangles, "[{g}] tri triangles");
        },
    );
}

/// #5903 — pin the complete vanilla sidecar census, not only `headhuman`.
/// The counts were measured across every BSA in each unmodded Data folder;
/// each listed sidecar is extracted and passed through its production parser.
#[test]
#[ignore = "needs Oblivion, FO3 and FNV game data on disk"]
fn vanilla_facegen_sidecar_census_is_exact() {
    let games = [
        (
            "Oblivion",
            "BYROREDUX_OBLIVION_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data",
            (154, 20, 86),
        ),
        (
            "FO3",
            "BYROREDUX_FO3_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data",
            (250, 36, 49),
        ),
        (
            "FNV",
            "BYROREDUX_FNV_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data",
            (275, 36, 49),
        ),
    ];

    for (game, env_var, fallback, expected) in games {
        let data = if let Some(value) = std::env::var(env_var).ok().filter(|s| !s.is_empty()) {
            let path = PathBuf::from(&value);
            assert!(path.is_dir(), "{env_var} points to {path:?}, not a directory");
            path
        } else {
            let path = PathBuf::from(fallback);
            if !path.is_dir() {
                require_game_data(env_var, &path);
                eprintln!("[{game}] data unavailable; skipping corpus census");
                continue;
            }
            path
        };

        let mut archives: Vec<_> = std::fs::read_dir(&data)
            .unwrap_or_else(|e| panic!("[{game}] read {data:?}: {e}"))
            .map(|entry| entry.unwrap_or_else(|e| panic!("[{game}] directory entry: {e}")).path())
            .filter(|path| {
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("bsa"))
            })
            .collect();
        archives.sort();

        let mut counts = [0usize; 3];
        let mut failures = Vec::new();
        for archive_path in &archives {
            let archive = BsaArchive::open(archive_path)
                .unwrap_or_else(|e| panic!("[{game}] open {archive_path:?}: {e}"));
            for entry in archive.list_files() {
                let kind = match entry.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
                    "egm" => 0,
                    "egt" => 1,
                    "tri" => 2,
                    _ => continue,
                };
                counts[kind] += 1;
                match archive.extract(entry) {
                    Ok(bytes) => {
                        let parsed = match kind {
                            0 => EgmFile::parse(&bytes).map(|_| ()),
                            1 => EgtFile::parse(&bytes).map(|_| ()),
                            _ => TriHeader::parse(&bytes).map(|_| ()),
                        };
                        if let Err(error) = parsed {
                            failures.push((archive_path.clone(), entry.to_string(), error.to_string()));
                        }
                    }
                    Err(error) => failures.push((archive_path.clone(), entry.to_string(), error.to_string())),
                }
                if failures.len() >= 16 {
                    break;
                }
            }
            if failures.len() >= 16 {
                break;
            }
        }

        eprintln!(
            "[{game}] FaceGen corpus: {} archives, {} EGM / {} EGT / {} TRI, {} errors",
            archives.len(), counts[0], counts[1], counts[2], failures.len()
        );
        assert_eq!(
            archives.len(),
            if game == "Oblivion" { 17 } else if game == "FO3" { 16 } else { 21 },
            "[{game}] BSA archive census changed"
        );
        assert_eq!((counts[0], counts[1], counts[2]), expected, "[{game}] FaceGen sidecar census changed");
        assert!(failures.is_empty(), "[{game}] FaceGen parse/extract failures: {failures:?}");
    }
}
