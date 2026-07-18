# RT-1: TES-family (Oblivion, Skyrim) player rig never grounds at cell-load spawn — infinite freefall

**Labels**: bug, high, legacy-compat

**Severity**: HIGH
**Dimension**: runtime / physics (character controller)
**Games**: oblivion (`ICMarketDistrictTheGildedCarafe`), skyrim_se (`WhiterunDragonsreach`)
**Location**: `byroredux/src/systems/character.rs` (M28.5 grounding, logged as `M28.5 frame N: body Y a→b … grounded=false`); ground-probe/KCC result from `crates/physics/src/world.rs` (`grounded` field on the move-shape result) written into `crates/physics/src/components.rs::CharacterController.is_grounded`

## Description
On both TES-family cells, the player character controller free-falls from spawn and never sets `is_grounded=true` for the entire 240-frame bench window (and continues in `--bench-hold`). Contrast with the three Fallout-family cells, which all ground within 0–9 frames of spawn. This is the exact symptom #1832 described pre-fix, and its own closing comment already anticipated it would keep reproducing: "Even after the fix, the character still free-falls completely at the door-based spawn point in both Skyrim cells tested. This looks like a **separate** issue... Next step is a fresh investigation into the door-threshold spawn gap specifically... not yet filed as a separate issue."

Closest prior work is CLOSED #1832 ("RT-2: TES-family character rig never grounds -> infinite freefall (Oblivion, Skyrim)", `stateReason: COMPLETED`, closed 2026-07-05). Its own closing comment states a partial fix landed (`ae083d69`, reclassifying zero-mass `Dynamic`-per-enum Havok bodies as `Static`) and explicitly flags this exact door-threshold spawn symptom as still reproducing and "not yet filed as a separate issue." Reported as NEW here because the specific symptom was never given its own tracking issue — this is a continuation of a deferred, never-fixed symptom, not a regression of a fix that held.

Verified against current code (HEAD `c3e09bb5`, matching this repo's current HEAD exactly): the referenced grounding/logging code paths in `character.rs`/`world.rs`/`components.rs` are present as described.

## Evidence
- FNV (control): `M28.5 frame 0: body Y 13962.0→13962.0 … grounded=true` (grounds immediately).
- FO4 (control): `M28.5 frame 0: body Y 294.2→312.2 … grounded=true` (grounds immediately).
- FO3 (control): `grounded=false` frames 1–4 during the initial fall from spawn height, then `M28.5 frame 9: body Y 7494.0→7490.3 … grounded=true, rapier_bodies=845 [TRANSITION]` (settles, as expected).
- Oblivion: falls from `Y=414.8` to `Y≈324.0` by frame ~60, then **sticks** at `Y≈323.9–324.0` for the rest of the run (frames 120→900+, `Δ≈0.000`) while `v` stays pinned at the `-2000.0` terminal-velocity cap and `grounded` **never** flips to `true` — the KCC appears to be resting against *something* (Y stops changing) but the grounded flag is never set, a distinct sub-symptom from Skyrim's case below.
- Skyrim: falls continuously and never contacts anything — `Y` descends monotonically from `-232.3` (frame 0) through `-28824.8` (frame 900) at a steady `Δ≈-33.332`/tick once terminal velocity is reached, `grounded=false` throughout. True infinite fall into the void, matching #1832's original evidence table almost exactly in magnitude/shape.
- The RT-1/#1698 perf-collapse half of the original #1832 report (Skyrim 321→8.7/30 fps from the falling body sweeping 1575 rapier bodies every substep) does **not** reproduce here — this run's Skyrim `wall_fps` is 270.0, a normal ~16% advisory delta from baseline, not a collapse — so the `ae083d69` mass=0→Static reclassification fix is holding for the performance half even though the grounding half is still broken.

## Impact
The player character is not usable at spawn in either TES-family test cell — basic movement/standing is broken for the two games that exercise the `bhkRigidBody`/Havok-derived collision path (Oblivion, Skyrim), while all three Fallout-family games (which share the same character-controller code but different collision-authoring conventions) are unaffected. This blocks any manual playtesting or automated interaction testing that assumes the player starts grounded in a TES-family interior loaded via `--cell`. Does not corrupt telemetry, crash the renderer, or affect any structural runtime metrics (entities/textures/mesh-cache/skin-pool/draws all matched baseline exactly on both cells) — purely a physics-layer correctness gap.

## Related
#1832 (closed — the partial mass=0 fix is confirmed still intact; the door-threshold-spawn continuation was explicitly deferred by that issue's own closing comment). #1698 (closed — the associated Skyrim perf collapse does not reproduce this run, consistent with the `ae083d69` fix holding).

## Suggested Fix
Per #1832's own next-step note, investigate the door-threshold spawn specifically rather than the collision-classification angle again (already fixed and confirmed holding). Two candidate leads named in #1832: (1) the Bannered Mare/first-`DoorTeleport` spawn point in Skyrim leads to an *exterior* worldspace not loaded under an interior-only `--cell` invocation, so the landing spot may have no floor geometry on our side of the loaded content at all; (2) a pre-existing code comment in `crates/physics/src/world.rs` about floor-plank vertex gaps (~1-2 BU) at collision-triangle seams — a KCC tunneling issue independent of body classification. Oblivion's "sticks at Y≈324 but never grounds" sub-symptom suggests the KCC probe may also have a grounded-flag threshold/normal-facing bug distinct from the tunneling theory — worth checking whether the resting contact's surface normal is being computed post the Z-up→Y-up conversion (`crates/nif/src/import/coord.rs`) correctly for TES-derived collision meshes specifically.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (whether the same grounding-probe path is exercised identically on all 7 games' collision-authoring conventions, not just the 5 with committed baselines)
- [ ] **TESTS**: A regression test pins this specific fix (an automated `--bench-hold` + `byro-dbg` check that `is_grounded=true` within N frames of spawn on both TES-family baseline cells)

