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
        if let Ok(val) = std::env::var(self.env_var()) {
            let p = PathBuf::from(val);
            if p.is_dir() {
                return Some(p);
            }
        }
        let p = PathBuf::from(self.default_path());
        if p.is_dir() {
            Some(p)
        } else {
            None
        }
    }

    fn extract(self, inner: &str) -> Option<Vec<u8>> {
        let dir = self.data_dir()?;
        let bsa_path = dir.join(self.mesh_bsa());
        let archive = BsaArchive::open(&bsa_path).ok()?;
        archive.extract(inner).ok()
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
            // NaN sentinel deltas DO appear in vanilla `headhuman.egm` —
            // FaceGen's authoring pipeline stores "no displacement" as
            // a half-float NaN bit-pattern on some entries (verified by
            // dumping non-finite indices on 2026-04-29). The Phase 3b
            // morph evaluator must guard against NaN propagation when
            // it sums `weight * delta` per vertex; the parser layer
            // preserves the on-disk bytes verbatim.
            let nan_count: usize = egm
                .fggs_morphs
                .iter()
                .chain(egm.fgga_morphs.iter())
                .map(|m| m.deltas.iter().flatten().filter(|c| !c.is_finite()).count())
                .sum();
            eprintln!("[{g}] vanilla headhuman.egm: {nan_count} non-finite delta components");
        },
    );
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
