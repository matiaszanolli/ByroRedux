# FNV-SD-04: Creature idle/walk clips use Oblivion's idle.kf / forward.kf; no FNV creature directory ships either

Labels: medium,animation,bug,game:fnv,legacy-compat

**Severity**: MEDIUM · **Dimension**: 4/5 (animation at spawn) · **Game**: FNV (FO3 and Oblivion SI share the resolver, see Related)

## Description
The creature clip paths are `{dir}idle.kf` and `{dir}forward.kf` (Oblivion's convention, built by closed #2567 and "verified over Oblivion - Meshes.bsa"). FO3/FNV creatures keep the move-type clips under different names, and nothing verifies the convention for them.

## Evidence
Location: `byroredux/src/npc_spawn.rs:577` (`creature_idle_kf_path`), `:943` (`creature_walk_kf_path`), `byroredux/src/npc_spawn/resumable/runtime.rs:472-473` (both derived for every creature), Finalize at `:~884-935` (idle None -> "spawning unanimated"; walk None -> no `WalkAnimation`). The only data check is Oblivion-only (`byroredux/src/npc_spawn/tests.rs:2360`, `installed_oblivion_creature_assets_resolve_from_their_records`). Re-confirmed present at HEAD.

`lsbsa` over `Fallout - Meshes.bsa` + `Update.bsa` + the four DLC mains (47,510 names), census of the 42 distinct creature directories referenced by the 1,060 modelled FNV `CREA` records: **`idle.kf` 0 / 42, `forward.kf` 0 / 42**; `mtidle.kf` 40 / 42; `locomotion\mtforward.kf` 23 / 42 (plus the shipped misspelling `mtfoward.kf` in `deathclaw` and `nightstalker`); the two without `mtidle.kf` are the human-skeleton directory and `failedfevsubject`. Disproof attempted: no second resolver supplies creature clips (`grep` for `locomotion\\`, `mtidle`, `kffz` in `byroredux/src` finds only humanoid paths).

## Impact
The 1,112 FNV creature placements that do spawn (and, after FNV-SD-01, the other 1,877) stand in bind pose and slide when an ambient package walks them. Not visible to the existing gates, which spawn humanoids.

## Related
FNV-SD-01, #5495, #2567. Variants of the same resolver: FO3-SD-05 (FO3: `idle.kf` 1 / 25, `mtidle.kf` 24 / 25, `forward.kf` 0 / 25); OBL-SD-04 (Oblivion Shivering Isles `grummite`/`gnarl`/`elytra` ship `forwardwalk.kf` instead of `forward.kf`; 196 `CREA` bases with no walk clip). A fix should resolve per game, and for Oblivion read the authored `KFFZ` list or try `forwardwalk.kf`.

## Suggested Fix
Resolve creature clips per game (`mtidle.kf` and `locomotion\mtforward.kf`, with the `mtfoward.kf` fallback for FO3/FNV; `forward.kf` then `forwardwalk.kf`, or the authored `KFFZ`, for Oblivion) and extend the installed-data test to FNV and FO3 the way it covers Oblivion.

Source: docs/audits/AUDIT_FNV_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling games' paths (FO3, FNV, Oblivion share this path)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser / spawn boundary, never pushed into shaders or the renderer
- [ ] **TESTS**: A regression test pins this specific fix (real-data `--ignored` test where the defect is a data census)
