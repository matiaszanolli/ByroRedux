# FO3-2026-08-24-D2-01: FO3 particle-emitter decode is unverified because its own regression test structurally cannot reach FO3

State: OPEN
Labels: bug,nif-parser,low,legacy-compat,nif,game:fo3,test-gap

## Description
The test's `games_to_try` array is `[Game::FalloutNV, Game::Fallout3, Game::Oblivion, Game::SkyrimSE]`. The loop body `return`s on the **first** game whose sampled 200 candidate NIFs yield any emitters (`if total_emitters > 0 { … return; }`). Both FalloutNV and Fallout3 mesh archives are present on this machine, and FalloutNV always yields emitters from its first 200 fire/fx/smoke/magic/effects-folder candidates — so the loop returns before Fallout3 is ever tried. Confirmed live: running the test with `--nocapture` shows only `[Fallout New Vegas] 182 emitters across 5 meshes …` — no `[Fallout 3]` line at all, even though this same test has existed since #2317-era commits.

## Location
`crates/nif/tests/parse_real_nifs.rs:311-392` (`real_archive_torch_meshes_surface_particle_emitters`)

## Evidence
```
$ cargo test -p byroredux-nif --release --test parse_real_nifs \
      -- --ignored --nocapture real_archive_torch_meshes_surface_particle_emitters
[Fallout New Vegas] 182 emitters across 5 meshes (sampled 200 NIFs from candidate folders)
  example: meshes\dlcanch\effects\dlcanchsnowmeshtube02.nif
  ...
test real_archive_torch_meshes_surface_particle_emitters ... ok
```
No `[Fallout 3]` line is printed; the test exits successfully having never called `open_mesh_archive(Game::Fallout3)`.

## Impact
The skill's own Dimension 2 checklist explicitly flags FO3 typed-particle decode (`extract_emitter_params`/`extract_emitter_rate`, feeding `apply_emitter_params`) as an "UNVERIFIED gap" because "the NIFAL decode doc-comments verify only against FNV + Oblivion." This test is the one piece of real-archive infrastructure that could close that gap, and its early-return-on-first-success shape means it never has, on any machine where FNV data ships alongside FO3 data. The code path itself may well be correct — FO3 shares the exact same typed-block dispatch and `extract_emitter_params`/`extract_emitter_rate` functions as FNV, with no FO3 gate of any kind — but that is an inference from shared code, not a verified fact, and the test that exists specifically to verify it cannot do so as written.

## Related
Not a duplicate of #2610 (FO4 BGEM particle flags, unrelated) or #2766 (renderer dispatch-count, unrelated).

## Suggested Fix
Change the loop to try every game and accumulate results (or at minimum assert non-zero emitters were found for *every* game with a present archive, not just the first), so a future FO3-specific emitter regression can't hide behind FNV's earlier success. Low urgency — no evidence of an actual decode defect, only of unverified coverage.

## Completeness Checks
- [ ] **TESTS**: This IS the test fix — accumulate/assert-per-game instead of early-return-on-first-success

_Source: AUDIT_FO3_2026-08-24.md, finding FO3-2026-08-24-D2-01._
