# 2013: RT-1: TES-family (Oblivion, Skyrim) player rig never grounds at cell-load spawn — infinite freefall

https://github.com/matiaszanolli/ByroRedux/issues/2013

Labels: high, legacy-compat, bug

**Severity**: HIGH · **Dimension**: runtime / physics (character controller)
**Games**: oblivion (`ICMarketDistrictTheGildedCarafe`), skyrim_se (`WhiterunDragonsreach`)
**Location**: `byroredux/src/systems/character.rs` (M28.5 grounding); `crates/physics/src/world.rs` (`grounded` field on move-shape result); `crates/physics/src/components.rs::CharacterController.is_grounded`
**Status**: NEW
**Audit**: docs/audits/AUDIT_RUNTIME_2026-07-16.md (RT-1)

## Description
On both TES-family cells, the player character controller free-falls from spawn and never sets `is_grounded=true` for the entire 240-frame bench window (and continues in `--bench-hold`). Contrast with the three Fallout-family cells, which all ground within 0–9 frames of spawn. This is the exact symptom CLOSED #1832 described pre-fix, and its own closing comment already anticipated it would keep reproducing: "the character still free-falls completely at the door-based spawn point in both Skyrim cells tested... not yet filed as a separate issue." Reported as NEW because the specific symptom was never given its own tracking issue — a continuation of a deferred, never-fixed symptom, not a regression of a fix that held.

## Evidence
- FNV/FO4/FO3 (controls): all ground within 0-9 frames of spawn.
- Oblivion: falls from `Y=414.8` to `Y≈324.0` by frame ~60, then sticks at `Y≈323.9-324.0` (Δ≈0.000) while `v` stays pinned at the -2000.0 terminal-velocity cap and `grounded` never flips true — resting against something but the grounded flag never sets.
- Skyrim: falls continuously and never contacts anything — `Y` descends monotonically to -28824.8 by frame 900, `grounded=false` throughout. True infinite fall.
- The original #1832 perf-collapse half (Skyrim 321→8.7/30 fps) does NOT reproduce — `wall_fps` 270.0, normal advisory delta — so the `ae083d69` mass=0→Static fix holds for the performance half even though the grounding half is still broken.

## Impact
Player character is not usable at spawn in either TES-family test cell — basic movement/standing broken for Oblivion/Skyrim (Havok-derived collision path), while Fallout-family games (same character-controller code, different collision-authoring conventions) are unaffected. Blocks manual/automated interaction testing assuming a grounded spawn. Does not corrupt telemetry or crash the renderer — purely a physics-layer correctness gap; all structural runtime metrics matched baseline exactly.

## Related
#1832 (closed — partial mass=0 fix confirmed still intact; door-threshold-spawn continuation explicitly deferred by that issue's own closing comment). #1698 (closed — associated Skyrim perf collapse does not reproduce, consistent with `ae083d69` holding).

## Suggested Fix
Investigate the door-threshold spawn specifically (not the already-fixed collision-classification angle). Candidate leads: (1) Skyrim's first-`DoorTeleport` spawn point may lead to an exterior worldspace not loaded under an interior-only `--cell` invocation, leaving no floor geometry loaded; (2) a pre-existing code comment in `crates/physics/src/world.rs` about floor-plank vertex gaps at collision-triangle seams (KCC tunneling, independent of body classification). Oblivion's "sticks but never grounds" sub-symptom suggests a grounded-flag threshold/normal-facing bug — worth checking whether the resting contact's surface normal is computed correctly post the Z-up→Y-up conversion (`crates/nif/src/import/coord.rs`) for TES-derived collision meshes specifically.

## Completeness Checks
- [ ] SIBLING: Same pattern checked in related files (whether the same grounding-probe path is exercised identically across all 7 games' collision-authoring conventions, not just the 5 with committed baselines)
- [ ] TESTS: A regression test pins this specific fix (automated `--bench-hold` + `byro-dbg` check that `is_grounded=true` within N frames of spawn on both TES-family baseline cells)

## Label gap note
No dedicated `runtime` or `physics` label exists in this repo; `legacy-compat` used as the closest fit (TES-family-specific vs. Fallout-family collision-authoring divergence) per audit-publish's mapping guidance for AUDIT_RUNTIME findings with no dedicated label.
