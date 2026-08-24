# 3286: FO3-2026-08-24-D2-01: FO3 particle-emitter decode is unverified because its own regression test structurally cannot reach FO3

**Severity**: LOW · **Report**: `docs/audits/AUDIT_FO3_2026-08-24.md` (FO3-2026-08-24-D2-01)

## Description

The test's `games_to_try` array is `[FalloutNV, Fallout3, Oblivion, SkyrimSE]`. The loop body returns on the *first* game whose sampled 200 candidate NIFs yield any emitters. FalloutNV always yields emitters first, so Fallout3 is never tried. Confirmed live: `--nocapture` shows only a `[Fallout New Vegas]` line, no `[Fallout 3]` line, despite this test existing since #2317-era commits.

## Location

`crates/nif/tests/parse_real_nifs.rs:311-392` (`real_archive_torch_meshes_surface_particle_emitters`)

## Evidence

```
$ cargo test -p byroredux-nif --release --test parse_real_nifs \
      -- --ignored --nocapture real_archive_torch_meshes_surface_particle_emitters
[Fallout New Vegas] 182 emitters across 5 meshes ...
test real_archive_torch_meshes_surface_particle_emitters ... ok
```

## Impact

The skill's own Dimension 2 checklist flags FO3 typed-particle decode as an "UNVERIFIED gap." This test is the one real-archive infrastructure that could close it, and its early-return shape means it never has. The code path is likely correct (FO3 shares the exact dispatch/extraction functions with FNV, no FO3 gate exists) but that's an inference, not a verified fact.

## Suggested Fix

Change the loop to try every game and accumulate results, or assert non-zero emitters for every game with a present archive, not just the first.

## Completeness Checks
- [ ] **TESTS**: This IS the test fix — accumulate/assert-per-game instead of early-return-on-first-success
