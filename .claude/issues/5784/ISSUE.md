# OBL-D3-2026-10-14-02: the #5487 archive-presence pin skips green under `BYROREDUX_REQUIRE_GAME_DATA=1`, and none of Dim 3's real-data pins run in the strict CI lane

**Labels**: low,game:oblivion,legacy-compat,tech-debt,test-gap,bug
**Source**: docs/audits/AUDIT_OBLIVION_2026-10-14.md

Found by `/audit-oblivion` (docs/audits/AUDIT_OBLIVION_2026-10-14.md, HEAD `adbd468e2`). Finding ID: `OBL-D3-2026-10-14-02`. Re-verified against current code before filing.

- **Severity**: LOW
- **Dimension**: 3
- **Location**: `byroredux/src/npc_spawn/tests.rs` `installed_oblivion_body_paths_exist_in_the_meshes_archive` (the `let Ok(archive) = … else { eprintln!("skipping …"); return; }`); `.github/workflows/real-data-gates.yml`
- **Status**: NEW (same family as closed #3850 / #4660 / #5326 / #5011)
- **Description**: the same file already owns `require_game_data(env_var, tried)` (#3850) and other tests call it; this one does not. With `BYROREDUX_REQUIRE_GAME_DATA=1 BYROREDUX_OBLIVION_DATA=/nonexistent` it prints "skipping …" and reports `ok`. Separately, no CI lane executes it: the `corpus` matrix filters `-p byroredux-nif` on the substring `oblivion`, so `installed_oblivion_body_paths_exist_*` (bin crate), `dark_texture_role_population_matches_the_documented_census` and `real_archive_torch_meshes_surface_particle_emitters` (names carry no `oblivion`) are manual-only.
- **Evidence**: runs recorded above; the `body_paths_kf_era_*` unit test compares the function to the very literals it returns, so only the `--ignored` test can catch archive-name drift.
- **Impact**: a future edit that makes `humanoid_body_paths(Oblivion, …)` request a name the archive does not ship (the original #5487 defect) stays green in CI.
- **Related**: #3850, #5326, #5487, #5088.
- **Suggested Fix**: call `require_game_data("BYROREDUX_OBLIVION_DATA", &bsa_path)` in the skip branch; add the three Dim 3 real-data tests by exact name to a strict-lane step, as was done for the ObScript gate (#5088).

### Dim 4 — Exterior & lighting data

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' arms, sibling parsers/tests)
- [ ] **TESTS**: A regression test pins this specific fix
