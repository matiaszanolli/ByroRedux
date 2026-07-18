# Issue batch: 2009, 2010, 2011, 2013

## #2009 — MAT-D1-01: classify_pbr_keyword's unbounded substring match misfires the glass arm on common English words
- Severity: HIGH (bug, renderer/material)
- Location: `crates/core/src/ecs/components/material.rs:519-524` (glass arm), `:719-728` (`contains_any_ci`, shared matcher)
- `contains_any_ci` is a raw ASCII case-insensitive substring match with zero word-boundary logic. `"ice"` is a substring of many ordinary words (`office`, `notice`, `device`, `justice`, `invoice`, `spice`, `voice`, `twice`, `advice`, `entice`, `artifice`, `sacrifice`, `practice`, `police`, `juice`, `dice`, `slice`). Any of these in a diffuse texture path routes the surface through the glass arm (`roughness=0.1, metalness=0.0`), forcing spurious mirror-reflective ("wet floor") rendering.
- Secondary, lower-impact: `"fur"` (fabric arm, material.rs:540) is a literal prefix of `"furniture"`.
- Related: #1819 (CLOSED) fixed a different manifestation (SpeedTree bypass) without root-causing the shared classifier.
- Suggested fix: add a word-boundary check to `contains_any_ci` so a match only counts when the preceding/following byte is not ASCII-alphanumeric. Add regression tests: `office*.dds`/`notice*.dds`/`device*.dds`/`furniture*.dds` must NOT reach glass/fabric arms.

## #2010 — NIFAL-D4-01: Canonical FurnitureMarker.heading_z_radians Option is re-resolved by a per-era gameplay heuristic
- Severity: MEDIUM (bug, ecs, no-leak violation)
- Location: `crates/core/src/ecs/components/furniture.rs:41` (canonical field); consumer `byroredux/src/systems/sandbox.rs:69-71` (`is_sit_marker`) and `:97-104` (`seat_world_transform`)
- Canonical `FurnitureMarker.heading_z_radians: Option<f32>` is a legitimate "genuinely missing legacy data" representation, but the M42 sandbox-seating system re-resolves the era discriminant (`heading_z_radians.is_none()` as proxy for "legacy content, assume sit") at the gameplay layer instead of at the translate boundary — the no-leak NIFAL invariant violation.
- Impact: self-acknowledged v0 over-match; whole M42 system is opt-in (`BYRO_SANDBOX_SIT` unset by default) so no default-behavior impact today. Architectural concern only.
- Suggested fix: NOT URGENT per the issue itself — "when the seating feature matures past v0," resolve the era discriminant once at the translate boundary into an explicit `pub kind: FurnitureMarkerKind` (Sit/Sleep/Lean/Unknown) rather than leaving `heading_z_radians.is_none()` as an implicit flag.

## #2011 — ECS-2026-07-16-01: GuardState doc comment overstates its write frequency
- Severity: LOW (documentation only)
- Location: `crates/core/src/ecs/components/guard.rs:55-62`
- Doc comment claims "this state is read *and* written every tick, the same shape WanderState has" — but `guard_system` only ever writes `GuardState` once (first sight), then freezes it (mirrors `TravelState::destination`, NOT `WanderState`'s continuous mutation). The doc's own preceding sentence already says this correctly; the final sentence contradicts it.
- Impact: documentation-only; no runtime behavior affected.
- Suggested fix: reword the last sentence to match actual discipline (write-once-then-frozen, like Travel, not continuous like Wander).

## #2013 — RT-1: TES-family (Oblivion, Skyrim) player rig never grounds at cell-load spawn — infinite freefall
- Severity: HIGH (runtime/physics, character controller)
- Games: oblivion (`ICMarketDistrictTheGildedCarafe`), skyrim_se (`WhiterunDragonsreach`)
- Location: `byroredux/src/systems/character.rs` (M28.5 grounding); `crates/physics/src/world.rs` (ground-probe/KCC); `crates/physics/src/components.rs::CharacterController.is_grounded`
- Continuation of deferred symptom from CLOSED #1832 (partial fix `ae083d69` reclassified zero-mass Dynamic Havok bodies as Static — confirmed still holding, perf collapse does not reproduce). The door-threshold-spawn grounding symptom was explicitly deferred by #1832's own closing comment as "a separate issue... not yet filed."
- Two distinct sub-symptoms: Oblivion sticks at a resting Y position but `grounded` never flips true (KCC probe threshold/normal-facing bug?); Skyrim free-falls into the void indefinitely (never contacts anything — possible door-teleport spawn into unloaded exterior worldspace, or floor-plank vertex-gap tunneling per an existing code comment in `world.rs`).
- Suggested investigation leads: (1) Skyrim's first-DoorTeleport spawn point may lead to an exterior worldspace not loaded under interior-only `--cell`; (2) known floor-plank vertex-gap KCC tunneling issue; (3) Oblivion's resting-but-not-grounded case suggests a surface-normal computation bug post Z-up→Y-up conversion (`crates/nif/src/import/coord.rs`) specific to TES-derived collision meshes.
- Requires actual runtime reproduction (`--bench-hold` + `byro-dbg`, real game data + Vulkan device) to verify — same class of issue flagged by project convention as unsafe to fix speculatively without RenderDoc/telemetry verification.
