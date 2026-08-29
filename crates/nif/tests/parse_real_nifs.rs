//! Per-game NIF parse-rate integration tests.
//!
//! These tests walk **every** mesh-bearing archive a game ships (#3041 —
//! `Game::mesh_archives`, not just the primary one), parse every NIF entry
//! in each — `.nif`, plus the renamed distant-LOD `.bto`/`.btr` (#2587) —
//! and assert that at least `MIN_SUCCESS_RATE` of them parse without error.
//! A failure names the worst archive so a red build says which one regressed.
//! They are `#[ignore]`d by default because they require game data and
//! run for several seconds. Opt in with:
//!
//! ```sh
//! cargo test -p byroredux-nif --test parse_real_nifs -- --ignored
//! ```
//!
//! Point the `BYROREDUX_*_DATA` env vars at your `Data/` directories if
//! your install path differs from the defaults in `common::Game::default_path`.

mod common;

use common::{
    open_all_mesh_archives, open_ba2_by_name, open_mesh_archive, open_optional_mesh_archives,
    parse_all_nifs_in_archive, Game, ParseStats,
};

/// Acceptance threshold per N23.10 + ROADMAP. Gates on the
/// **recoverable** rate (clean + NiUnknown-recovered + truncated) so a
/// hard parse failure on any vanilla NIF is a regression. Every
/// supported game currently hits 100% recoverable — the recovery paths
/// (block_size seek, runtime size cache, `oblivion_skip_sizes` hint,
/// dispatch-level unknown-type fallback) absorb under-consuming parser
/// bugs by substituting `NiUnknown` placeholders and continuing.
///
/// The `clean` rate (fully parsed, no placeholders) is printed by
/// `ParseStats::print_summary` as a secondary metric. Pre-#568 it
/// masqueraded as the gate metric — the record_success path silently
/// absorbed `NiUnknown` recoveries, so Skyrim's ~55% placeholder rate
/// (from bhkRigidBody and friends) reported as "100% clean". Driving
/// `clean` upward is open work tracked on the individual parser-bug
/// issues (e.g. #546). This gate stays at recoverable so hard-failure
/// regressions still fail loud and clear.
///
/// If a future mod-content test tolerates partial coverage, define a
/// separate `MIN_SUCCESS_RATE_MOD` and use it there rather than
/// loosening the vanilla gate. See issue #487.
const MIN_RECOVERABLE_RATE: f64 = 1.0;

/// Walk **every** mesh-bearing archive for `game`, not just the primary one.
///
/// #3041 — this gate used to open `game.mesh_archive()` alone. On FNV that is
/// `Fallout - Meshes.bsa`: 14,881 NIFs out of a corpus that also ships four
/// story DLC, four pre-order packs and the 1.4 `Update.bsa`. The gate that
/// certifies "FNV NIF parse rate 100% clean" was measuring a fraction of the
/// content it claimed, and a parser change breaking only DLC assets — entirely
/// plausible, since DLC ships later-authored content — would not have turned it
/// red. `Game::mesh_archives()` already enumerated the full set for the
/// baseline harnesses (#2334); only this gate had not been moved over.
///
/// Per-archive attribution is the point of the loop rather than one merged
/// walk: a failure has to name the archive that regressed, or the widening
/// just makes the red build harder to diagnose than the narrow one was.
///
/// #3369 — Skyrim SE left the same blind spot open one tier down: a stock
/// AE `Data/` also ships `_ResourcePack.bsa` and four Creation Club
/// archives carrying 715 NIFs that no gate opened. Those can't join
/// `mesh_archives()` (they vary per account, and the baseline harnesses
/// that share that list compare absolute counts), so this gate also sweeps
/// `Game::optional_mesh_archives()` — present-only, and safe here precisely
/// because the assertion below is a rate, not a count.
fn run_game(game: Game, limit: Option<usize>) {
    let Some(mut archives) = open_all_mesh_archives(game) else {
        return; // Skip if game data not available — common::open_all_mesh_archives prints the reason.
    };
    let required = archives.len();
    archives.extend(open_optional_mesh_archives(game));
    if archives.len() > required {
        eprintln!(
            "[{}] + {} optional archive(s) present on this install (#3369)",
            game.label(),
            archives.len() - required,
        );
    }

    let mut totals = ParseStats::default();
    let mut worst: Option<(&str, f64, usize)> = None;

    for (name, archive) in &archives {
        eprintln!(
            "[{}] opened {} ({} files)",
            game.label(),
            name,
            archive.file_count()
        );

        // `limit` is a per-archive cap: it exists so a smoke run can bound
        // work, and bounding it globally would silently stop walking the
        // later archives entirely — the exact blind spot this fix closes.
        let stats = parse_all_nifs_in_archive(archive, limit);
        eprintln!(
            "[{}/{}] {} NIFs, {} clean, {} truncated, {} failed ({:.2}% recoverable)",
            game.label(),
            name,
            stats.total,
            stats.clean,
            stats.truncated.len(),
            stats.failures.len(),
            stats.recoverable_rate() * 100.0,
        );

        // Track the worst archive so the assertion below can name it.
        if stats.total > 0 {
            let rate = stats.recoverable_rate();
            if worst.is_none_or(|(_, worst_rate, _)| rate < worst_rate) {
                worst = Some((name, rate, stats.failures.len()));
            }
        }

        totals.total += stats.total;
        totals.clean += stats.clean;
        totals.truncated.extend(stats.truncated);
        totals.failures.extend(stats.failures);
    }

    totals.print_summary(game.label());

    assert!(
        totals.total > 0,
        "[{}] expected at least one NIF across {} archive(s)",
        game.label(),
        archives.len(),
    );
    assert!(
        totals.recoverable_rate() >= MIN_RECOVERABLE_RATE,
        "[{}] parse recoverable rate {:.2}% across {} archive(s) is below the {:.0}% \
         threshold ({} hard failures); worst archive: {}",
        game.label(),
        totals.recoverable_rate() * 100.0,
        archives.len(),
        MIN_RECOVERABLE_RATE * 100.0,
        totals.failures.len(),
        match worst {
            Some((name, rate, fails)) =>
                format!("{name} at {:.2}% ({fails} hard failures)", rate * 100.0),
            None => "none".to_string(),
        },
    );
}

#[test]
#[ignore]
fn parse_rate_fallout_nv() {
    run_game(Game::FalloutNV, None);
}

#[test]
#[ignore]
fn parse_rate_fallout_3() {
    run_game(Game::Fallout3, None);
}

#[test]
#[ignore]
fn parse_rate_skyrim_se() {
    run_game(Game::SkyrimSE, None);
}

#[test]
#[ignore]
fn parse_rate_oblivion() {
    // Oblivion BSA v103 uses zlib compression (handled in
    // `crates/bsa/src/archive.rs:470-475`). Previous "decompression not
    // yet implemented" comment was stale after M26+.
    run_game(Game::Oblivion, None);
}

#[test]
#[ignore]
fn parse_rate_fallout_4() {
    run_game(Game::Fallout4, None);
}

#[test]
#[ignore]
fn parse_rate_fallout_76() {
    run_game(Game::Fallout76, None);
}

#[test]
#[ignore]
fn parse_rate_starfield() {
    // Starfield meshes use BA2 v2 GNRL with the 32-byte header extension.
    // Texture archives (BA2 v3 DX10) use a different chunk layout that's
    // not yet supported and is tracked separately.
    run_game(Game::Starfield, None);
}

/// Full Starfield mesh corpus — walks all 5 vanilla mesh archives so the
/// per-archive clean rates are each independently gated. The headline
/// `parse_rate_starfield` test only covers Meshes01 (~35% of total NIFs).
///
/// Per-archive minimums (clean %) — refreshed 2026-07-11 (#1900 / NIF-D3-02,
/// mirroring the FO4 #1457 treatment: measured minus ~0.5% margin, rounded
/// down to the nearest 0.5%). The 2026-04-27 (#759) floors had gone stale
/// by 2-3 points on every archive except MeshesPatch:
///   Meshes01.ba2        ≥ 99.5% (31 058 NIFs; 100.00% actual; was 97.0%)
///   Meshes02.ba2        ≥ 99.5% ( 7 552 NIFs; 100.00% actual; was 99.0%)
///   MeshesPatch.ba2     ≥ 99.5% (29 849 NIFs; 99.98% actual; was 98.0%)
///   LODMeshes.ba2       ≥ 99.5% (19 535 NIFs; 100.00% actual; unchanged)
///   FaceMeshes.ba2      ≥ 99.5% ( 1 282 NIFs; 100.00% actual; unchanged)
///
/// #3397 — MeshesPatch's row was refreshed 2026-08-27. Its `98.91% actual`
/// was the *pre-#2105* measurement (29 849 − 325 truncated); `b7e0318f` took
/// that tail 325 → 6, i.e. 99.98%, but the floor was never re-tightened after
/// the fix it predates. At 98.0% the gate tolerated 597 truncations against
/// an actual 6 — a full revert of #2105 would have left this test green,
/// which is precisely the regression it exists to catch (and the shape of
/// #2201, which only tripped because Meshes02's floor happened to sit at
/// 99.5%). Now on the same "measured minus ~0.5%" rule as its four siblings.
#[test]
#[ignore]
fn parse_rate_starfield_all_meshes() {
    struct ArchiveSpec {
        name: &'static str,
        // Minimum clean-parse rate (0.0–1.0). Recoverable is always gated
        // at 100% via the outer assertion.
        min_clean: f64,
    }
    let archives: &[ArchiveSpec] = &[
        ArchiveSpec {
            name: "Starfield - Meshes01.ba2",
            min_clean: 0.995,
        },
        ArchiveSpec {
            name: "Starfield - Meshes02.ba2",
            min_clean: 0.995,
        },
        ArchiveSpec {
            name: "Starfield - MeshesPatch.ba2",
            min_clean: 0.995,
        },
        ArchiveSpec {
            name: "Starfield - LODMeshes.ba2",
            min_clean: 0.995,
        },
        ArchiveSpec {
            name: "Starfield - FaceMeshes.ba2",
            min_clean: 0.995,
        },
    ];

    let Some(_data_dir) = common::game_data_dir(Game::Starfield) else {
        return; // skip cleanly when Starfield is not installed
    };

    for spec in archives {
        let Some(archive) = open_ba2_by_name(Game::Starfield, spec.name) else {
            eprintln!("[Starfield] skipping {}: not found", spec.name);
            continue;
        };
        let stats = parse_all_nifs_in_archive(&archive, None);
        stats.print_summary(&format!("Starfield/{}", spec.name));

        assert!(
            stats.total > 0,
            "[Starfield/{}] expected at least one NIF",
            spec.name
        );
        assert!(
            stats.recoverable_rate() >= MIN_RECOVERABLE_RATE,
            "[Starfield/{}] recoverable rate {:.2}% below 100% threshold ({} hard failures)",
            spec.name,
            stats.recoverable_rate() * 100.0,
            stats.failures.len()
        );
        assert!(
            stats.success_rate() >= spec.min_clean,
            "[Starfield/{}] clean rate {:.2}% below {:.1}% minimum ({} truncated)",
            spec.name,
            stats.success_rate() * 100.0,
            spec.min_clean * 100.0,
            stats.truncated.len()
        );
    }
}

/// #1075 / FO4-D5-005 — Full FO4 mesh corpus across both vanilla
/// archives (`Fallout4 - Meshes.ba2` + `Fallout4 - MeshesExtra.ba2`).
/// The headline `parse_rate_fallout_4` test only opens the first; the
/// `MeshesExtra` archive carries DLC mesh overrides, settlement
/// construction pieces, weapon mods, and power-armor variants whose
/// block-type coverage gaps are invisible to the existing test.
/// Mirrors the Starfield multi-archive pattern (#754 / #759).
///
/// Per-archive minimums (clean %) — calibrated from a full sweep on
/// 2026-06-14 (#1457): both archives parse 100.00% clean / 100%
/// recoverable (Meshes 34 995/34 995, MeshesExtra 124 871/124 871, 0
/// truncated). The FaceGen truncation tail the 2026-06-02 audit recorded
/// (1 238 truncated in Meshes.ba2) is gone. Floors set to 0.995 — within
/// 0.5% of the measured 100%, rounded down — so a real regression (>0.5%
/// of the corpus losing clean parse) trips the gate while a single new
/// FaceGen drift does not.
#[test]
#[ignore]
fn parse_rate_fo4_all_meshes() {
    struct ArchiveSpec {
        name: &'static str,
        min_clean: f64,
    }
    let archives: &[ArchiveSpec] = &[
        ArchiveSpec {
            name: "Fallout4 - Meshes.ba2",
            min_clean: 0.995, // 100.00% clean measured 2026-06-14 (#1457); -0.5% margin
        },
        ArchiveSpec {
            name: "Fallout4 - MeshesExtra.ba2",
            min_clean: 0.995, // 100.00% clean measured 2026-06-14 (#1457); -0.5% margin
        },
    ];

    let Some(_data_dir) = common::game_data_dir(Game::Fallout4) else {
        return; // skip cleanly when FO4 is not installed
    };

    for spec in archives {
        let Some(archive) = open_ba2_by_name(Game::Fallout4, spec.name) else {
            eprintln!("[Fallout 4] skipping {}: not found", spec.name);
            continue;
        };
        let stats = parse_all_nifs_in_archive(&archive, None);
        stats.print_summary(&format!("Fallout 4/{}", spec.name));

        assert!(
            stats.total > 0,
            "[Fallout 4/{}] expected at least one NIF",
            spec.name
        );
        assert!(
            stats.recoverable_rate() >= MIN_RECOVERABLE_RATE,
            "[Fallout 4/{}] recoverable rate {:.2}% below 100% threshold ({} hard failures)",
            spec.name,
            stats.recoverable_rate() * 100.0,
            stats.failures.len()
        );
        assert!(
            stats.success_rate() >= spec.min_clean,
            "[Fallout 4/{}] clean rate {:.2}% below {:.1}% minimum ({} truncated)",
            spec.name,
            stats.success_rate() * 100.0,
            spec.min_clean * 100.0,
            stats.truncated.len()
        );
    }
}

/// Smoke subset — runs the first 50 NIFs from each available game in one
/// test so `cargo test -- --ignored` gives a fast signal without waiting
/// for the full per-game sweep. Useful during parser refactors.
#[test]
#[ignore]
fn parse_rate_smoke_all_games() {
    for game in [
        Game::FalloutNV,
        Game::Fallout3,
        Game::SkyrimSE,
        Game::Oblivion,
        Game::Fallout4,
        Game::Fallout76,
        Game::Starfield,
    ] {
        let Some(archive) = open_mesh_archive(game) else {
            continue;
        };
        let stats = parse_all_nifs_in_archive(&archive, Some(50));
        stats.print_summary(&format!("{} (smoke)", game.label()));
        if stats.total > 0 {
            assert!(
                stats.recoverable_rate() >= MIN_RECOVERABLE_RATE,
                "[{} smoke] parse recoverable rate {:.2}% below threshold",
                game.label(),
                stats.recoverable_rate() * 100.0,
            );
        }
    }
}

/// #401 — particle emitters must surface from real game content. Walks
/// the first up-to-200 NIFs in any candidate folder (`fire`, `smoke`,
/// `fx`, etc.) of every available reference archive, parses each, and
/// asserts that each one produces at least one
/// [`ImportedParticleEmitterFlat`]. Pre-fix the importer dropped every
/// NiPSysBlock and every torch rendered as an invisible node — this test
/// would have caught it.
///
/// #3286 — this used to `return` on the **first** game that yielded any
/// emitters. FNV is first in the list and always yields them, so on any
/// machine with FNV data installed the loop never reached Fallout 3,
/// Oblivion or Skyrim SE — confirmed by `--nocapture`, which printed only
/// the `[Fallout New Vegas]` line. That made this test useless as the
/// coverage it was cited for: FO3's typed-particle decode
/// (`extract_emitter_params` / `extract_emitter_rate`) shares FNV's
/// dispatch with no FO3 gate, but "shares the code path" is an inference,
/// and this is the one piece of real-archive infrastructure that could
/// turn it into a verified fact. Every present archive is now swept and
/// asserted independently.
#[test]
#[ignore]
fn real_archive_torch_meshes_surface_particle_emitters() {
    use byroredux_nif::import::import_nif_particle_emitters;

    let candidate_folders = ["fire", "fx", "smoke", "fxsmoke", "magic", "effects"];
    let games_to_try = [
        Game::FalloutNV,
        Game::Fallout3,
        Game::Oblivion,
        Game::SkyrimSE,
    ];

    let mut swept: Vec<(Game, usize, usize)> = Vec::new();
    let mut barren: Vec<Game> = Vec::new();
    // (game, emitters, with_params, with_rate, with_budget) — see #3343.
    let mut magnitudes: Vec<(Game, usize, usize, usize, usize)> = Vec::new();
    for game in games_to_try {
        let Some(archive) = open_mesh_archive(game) else {
            continue;
        };
        let all_files = archive.list_files();
        let mut total_emitters = 0usize;
        let mut paths_with_emitters: Vec<String> = Vec::new();
        // #3343 — presence alone is not coverage. Pre-fix the loop asserted
        // only `!emitters.is_empty()`, so a regression that zeroed every
        // authored rate, radius, life or base_scale left this test green: it
        // could detect the emitter *blocks* vanishing and nothing else, which
        // is precisely what the typed-emitter decode (`5708b5b9` / `9db60714`)
        // needed pinned. Accumulate decoded magnitudes alongside the count.
        let mut with_params = 0usize;
        let mut with_rate = 0usize;
        let mut with_budget = 0usize;
        // Walk up to 200 candidate NIFs per game so the test stays
        // fast (a few seconds) but has enough samples to find at least
        // one emitter in any reasonable mesh archive.
        let candidates: Vec<&String> = all_files
            .iter()
            .filter(|f| {
                let lower = f.to_ascii_lowercase();
                lower.ends_with(".nif") && candidate_folders.iter().any(|c| lower.contains(c))
            })
            .take(200)
            .collect();

        for path in &candidates {
            let bytes = match archive.extract(path) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let scene = match byroredux_nif::parse_nif(&bytes) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let emitters = import_nif_particle_emitters(&scene);
            if !emitters.is_empty() {
                total_emitters += emitters.len();
                for em in &emitters {
                    // A decoded param block must carry finite, positive spawn
                    // magnitudes — a zeroed or NaN decode is the regression
                    // this counts against, not a legitimately absent block.
                    if let Some(p) = &em.emitter_params {
                        if p.initial_radius.is_finite()
                            && p.initial_radius > 0.0
                            && p.life_span.is_finite()
                            && p.life_span > 0.0
                        {
                            with_params += 1;
                        }
                    }
                    if em.emitter_rate.is_some_and(|r| r.is_finite() && r > 0.0) {
                        with_rate += 1;
                    }
                    if em.max_particles.is_some_and(|m| m > 0) {
                        with_budget += 1;
                    }
                }
                if paths_with_emitters.len() < 5 {
                    paths_with_emitters.push((*path).clone());
                }
            }
        }

        if total_emitters > 0 {
            eprintln!(
                "[{}] {} emitters across {} meshes (params={} rate={} budget={}) \
                 (sampled {} NIFs from candidate folders)",
                game.label(),
                total_emitters,
                paths_with_emitters.len(),
                with_params,
                with_rate,
                with_budget,
                candidates.len(),
            );
            for p in &paths_with_emitters {
                eprintln!("  example: {}", p);
            }
        } else {
            eprintln!(
                "[{}] NO emitters in {} candidate NIFs",
                game.label(),
                candidates.len(),
            );
            barren.push(game);
        }
        swept.push((game, candidates.len(), total_emitters));
        magnitudes.push((game, total_emitters, with_params, with_rate, with_budget));
    }

    if swept.is_empty() {
        eprintln!("no reference game data available — skipping (set BYROREDUX_*_DATA env vars)");
        return;
    }
    assert!(
        barren.is_empty(),
        "these installed archives yielded zero particle emitters across their \
         candidate folders: {:?} — the importer regressed for them (the audit's \
         invisible-torch failure mode is back). Swept: {:?}",
        barren.iter().map(|g| g.label()).collect::<Vec<_>>(),
        swept
            .iter()
            .map(|(g, c, e)| (g.label(), *c, *e))
            .collect::<Vec<_>>(),
    );
    eprintln!(
        "[emitters] {} archive(s) swept, all non-zero: {:?}",
        swept.len(),
        swept
            .iter()
            .map(|(g, _, e)| (g.label(), *e))
            .collect::<Vec<_>>(),
    );

    // #3343 — magnitude floors. Presence-only assertions above prove the
    // emitter blocks are still found; these prove the typed decode still
    // produces usable numbers out of them.
    //
    // Floors are fractions, not the absolute counts the audit quoted. This
    // test samples the first 200 candidate NIFs per game, and `list_files()`
    // does not return a stable order — two consecutive runs over the same FNV
    // archive sampled 346 and then 439 emitters. An absolute pin would be
    // flaky by construction. A fraction is invariant to which 200 files the
    // sample happens to draw.
    //
    // Measured at fix time, all four installed games decoded params and budget
    // for **100%** of the emitters they sampled — the absolute sample size
    // moved run to run (FNV drew 220, 346 and 439 across three runs) but the
    // ratio did not budge. A 50% floor therefore leaves wide headroom for a
    // content-scope change while a decode that zeroes or NaNs the authored
    // magnitudes drops straight through it. Live counts print on every run.
    //
    // `params` and `budget` are near-universal on real content: essentially
    // every authored emitter carries a `NiPSysEmitter` base block and a
    // `NiPSysData` budget. `rate` is genuinely sparser — it needs a
    // `NiPSysEmitterCtlr` chain, and legacy / constant-rate emitters have
    // none — so it only has to be non-zero somewhere in the corpus.
    const MIN_PARAMS_FRACTION: f64 = 0.50;
    const MIN_BUDGET_FRACTION: f64 = 0.50;
    let mut magnitude_failures: Vec<String> = Vec::new();
    let mut total_rate = 0usize;
    for (game, emitters, params, rate, budget) in &magnitudes {
        total_rate += *rate;
        let n = *emitters as f64;
        if (*params as f64) < n * MIN_PARAMS_FRACTION {
            magnitude_failures.push(format!(
                "{}: only {}/{} emitters decoded finite positive                  initial_radius+life_span (floor {:.0}%)",
                game.label(),
                params,
                emitters,
                MIN_PARAMS_FRACTION * 100.0,
            ));
        }
        if (*budget as f64) < n * MIN_BUDGET_FRACTION {
            magnitude_failures.push(format!(
                "{}: only {}/{} emitters decoded a non-zero BS Max Vertices                  budget (floor {:.0}%)",
                game.label(),
                budget,
                emitters,
                MIN_BUDGET_FRACTION * 100.0,
            ));
        }
    }
    assert!(
        magnitude_failures.is_empty(),
        "authored particle magnitudes regressed — the emitter blocks are still \
         found but their decoded values are not usable: {:?}",
        magnitude_failures,
    );
    assert!(
        total_rate > 0,
        "no emitter in any swept archive decoded a finite positive birth rate — \
         the NiPSysEmitterCtlr -> interpolator chain regressed (#3343). Swept: {:?}",
        magnitudes
            .iter()
            .map(|(g, e, p, r, b)| (g.label(), *e, *p, *r, *b))
            .collect::<Vec<_>>(),
    );
    eprintln!(
        "[emitters] magnitude floors OK: {:?}",
        magnitudes
            .iter()
            .map(|(g, e, p, r, b)| (g.label(), *e, *p, *r, *b))
            .collect::<Vec<_>>(),
    );
}

/// #689 — verify the empirical absence of `NiSequenceStreamHelper` in
/// vanilla content. The 2026-04-25 audit (and the 2026-04-17 audit
/// before it) inferred a missing-importer-path bug from the comment in
/// `crates/nif/src/blocks/controller.rs` that says we don't consume the
/// block. This test walks Oblivion + FNV + Skyrim SE meshes archives
/// (`.nif` + `.kf`) and asserts zero `NiSequenceStreamHelper` blocks
/// across the whole corpus — pinning the audit's stale premise so a
/// future audit doesn't spend cycles on it.
///
/// If this assertion ever fires, vanilla content has appeared that
/// uses the legacy chain and the importer needs the Path-3 arm sketched
/// in `.claude/issues/689/INVESTIGATION.md`.
#[test]
#[ignore]
fn vanilla_archives_have_zero_nisequencestreamhelper() {
    let games = [Game::Oblivion, Game::FalloutNV, Game::SkyrimSE];
    let mut total_scanned = 0usize;
    let mut total_ssh = 0usize;
    let mut ssh_examples: Vec<(String, String)> = Vec::new();
    let mut tried_any_archive = false;

    for game in games {
        let Some(archive) = open_mesh_archive(game) else {
            continue;
        };
        tried_any_archive = true;
        for path in archive.list_files() {
            let lower = path.to_ascii_lowercase();
            if !(lower.ends_with(".nif") || lower.ends_with(".kf")) {
                continue;
            }
            total_scanned += 1;
            let Ok(bytes) = archive.extract(&path) else {
                continue;
            };
            let Ok(scene) = byroredux_nif::parse_nif(&bytes) else {
                continue;
            };
            for block in &scene.blocks {
                if block.block_type_name() == "NiSequenceStreamHelper" {
                    total_ssh += 1;
                    if ssh_examples.len() < 8 {
                        ssh_examples.push((game.label().to_string(), path.clone()));
                    }
                    break;
                }
            }
        }
    }

    if !tried_any_archive {
        eprintln!("no reference game data available — skipping (set BYROREDUX_*_DATA env vars)");
        return;
    }

    eprintln!(
        "scanned {} files across vanilla Oblivion + FNV + Skyrim SE meshes BSAs",
        total_scanned
    );
    eprintln!("NiSequenceStreamHelper occurrences: {}", total_ssh);

    assert_eq!(
        total_ssh,
        0,
        "expected zero NiSequenceStreamHelper blocks in vanilla content; found {} \
         (first {} examples: {:?}). The importer's Path 3 arm is now needed — \
         see .claude/issues/689/INVESTIGATION.md for the fix sketch.",
        total_ssh,
        ssh_examples.len(),
        ssh_examples
    );
}

/// #3369 regression — the two archive tiers must stay disjoint and the
/// optional one must actually name Skyrim's Creation Club / Anniversary
/// corpus.
///
/// Needs no game data on purpose: the defect #3369 filed was a *list*
/// omission, so the guard belongs on the list, where it also runs on the
/// CI machine that has no `Data/` at all. The two halves it pins:
///
/// * Disjointness — an entry in both tiers would be walked twice by this
///   gate, double-counting its NIFs into the rate.
/// * Non-empty for Skyrim SE — the whole point of #3369. If someone
///   "simplifies" the arm back to `_ => &[]`, the 715 NIFs silently fall
///   out of the gate again with nothing turning red.
#[test]
fn archive_tiers_are_disjoint_and_skyrim_optional_is_populated() {
    for game in Game::ALL {
        let required = game.mesh_archives();
        for opt in game.optional_mesh_archives() {
            assert!(
                !required.contains(opt),
                "[{}] {opt:?} is in both mesh_archives() and optional_mesh_archives(); \
                 run_game would walk it twice",
                game.label(),
            );
        }
    }

    let skyrim = Game::SkyrimSE.optional_mesh_archives();
    for expected in [
        "_ResourcePack.bsa",
        "ccBGSSSE001-Fish.bsa",
        "ccBGSSSE025-AdvDSGS.bsa",
        "ccBGSSSE037-Curios.bsa",
        "ccQDRSSE001-SurvivalMode.bsa",
    ] {
        assert!(
            skyrim.contains(&expected),
            "#3369: {expected:?} dropped out of Skyrim SE's optional mesh archives — \
             the 715 NIFs it guards are unreachable from the parse-rate gate again",
        );
    }
}
