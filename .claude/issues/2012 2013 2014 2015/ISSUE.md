# Batch: 2012, 2013, 2014, 2015

## #2012 — LC0716-01: PACK schedule (PSDT) parsed with a single fixed byte layout; diverges on Skyrim+/FO4/FO76/Starfield
- Severity: MEDIUM · Dimension: per-game translation-survey gap
- Location: `crates/plugin/src/esm/records/misc/ai.rs:538-550` (`parse_pack`'s `PSDT` arm);
  `crates/plugin/src/esm/records/mod.rs:603-611` (`PACK` dispatch arm — no `GameKind` gate,
  unlike sibling `SCOL`/`PKIN`/`MOVS`/`MSWP` arms at lines 246-294)
- Old (pre-Skyrim) PSDT: `month,day,date,time:i8 + duration:i32` @ offset 4, 8 bytes total —
  matches Redux's current fixed layout.
- New (Skyrim+) PSDT: `month,day,date,hour,minute:i8 + unused:3B + duration:i32` @ offset 8,
  12 bytes total. Redux reads offset 4 unconditionally → on Skyrim+ this misreads
  `minute`+padding as `duration`.
- PKDT sub-record confirmed NOT affected (offset-compatible across eras).
- Impact: dormant today — `index.packages` only consumed by Oblivion/FO3NV spawn path
  (`GameKind::has_runtime_facegen_recipe()`); Skyrim+/FO4/FO76/Starfield spawn never reads it.
  Forward-looking correctness gap.
- Suggested fix: thread `GameKind` into `parse_pack`, branch PSDT decode on
  "post-Skyrim package format" predicate (offset 8 vs 4).
- Completeness: SIBLING (PKDT confirmed unaffected), TESTS (Skyrim+ PSDT fixture pinning
  offset 8 read).
- Domain: esm → `byroredux-plugin`

## #2013 — RT-1: TES-family (Oblivion, Skyrim) player rig never grounds at cell-load spawn — infinite freefall
- Severity: HIGH · Dimension: runtime/physics (character controller)
- Location: `byroredux/src/systems/character.rs` (M28.5 grounding);
  `crates/physics/src/world.rs` (ground-probe/KCC); `crates/physics/src/components.rs`
  (`CharacterController.is_grounded`)
- Continuation of CLOSED #1832 (mass=0 Dynamic→Static Havok reclassification fix landed and
  holds for the perf-collapse half; grounding half explicitly deferred as "not yet filed").
- Oblivion: falls then STICKS around Y≈324 (Δ≈0) but `grounded` never flips true — distinct
  sub-symptom from Skyrim.
- Skyrim: true infinite fall into the void, never contacts anything.
- Fallout-family (FNV/FO3/FO4) all ground correctly within 0-9 frames — same character
  controller code, different collision-authoring conventions.
- Candidate leads (from #1832's own deferred note): (1) Skyrim's first door-teleport spawn
  point may lead to an exterior worldspace with no floor loaded under an interior-only
  `--cell` invocation; (2) floor-plank vertex-gap KCC tunneling noted in `world.rs` comments;
  (3) Oblivion's "sticks but never grounds" suggests a grounded-flag threshold/normal-facing
  bug, possibly Z-up→Y-up conversion (`crates/nif/src/import/coord.rs`) related for
  TES-derived collision meshes specifically.
- Completeness: SIBLING (verify across all 7 games' collision-authoring conventions),
  TESTS (bench-hold + byro-dbg check that is_grounded=true within N frames on both
  TES-family baseline cells).
- Domain: physics/runtime — needs investigation across `byroredux-physics`, `byroredux-nif`
  (coord conversion), and `byroredux` (character system). No live Vulkan device available in
  this session — investigation will be code-review based; per project history
  (feedback_speculative_vulkan_fixes / no live-repro), avoid shipping speculative fixes
  without independently verifiable evidence.

## #2014 — SAVE-D1-NEW-01: Seven M42 AI-procedure runtime-state components are absent from the save registry
- Severity: HIGH · Dimension: Snapshot Completeness & Determinism
- Location: `crates/core/src/ecs/components/{wander,travel,follow,escort,guard,patrol,sandbox}.rs`;
  `byroredux/src/save_io.rs:162-208` (`build_save_registry`)
- None of `WanderState`, `PatrolState`, `GuardState`, `FollowState`, `EscortState`,
  `TravelState`, `Traveled`, `Escorted`, `Seated` are registered for save.
- Terminal one-shot markers (`Traveled`/`Escorted`/`Seated`) are the sharp edge: losing them
  on save→load makes a finished NPC silently redo its behavior.
- Suggested fix: register terminal markers + position/phase-only state (plain Vec3/enum/u32,
  no EntityId) in `build_save_registry`, add delta-safe ones to `MUTABLE_DELTA_COLUMNS`.
  Do NOT add `FollowState`/`EscortState`/`Seated` to `MUTABLE_DELTA_COLUMNS` — they carry
  `EntityId` fields (session-local-reference hazard, same as `#1696`'s
  `AnimationPlayer.root_entity` exclusion) — full `register_component` only for those three.
- Completeness: SAVE-REGISTRY (build_save_registry AND SAVE_TYPE_SOURCES), TESTS.
- Domain: binary → `byroredux` (save_io.rs is the fix site; component structs already exist
  in byroredux-core, no changes needed there)

## #2015 — SAVE-D2-03: SAVE_TYPE_SOURCES (the #1714 guard's file scan list) omits actor_values.rs
- Severity: HIGH · Dimension: Registry & (De)serialization Fidelity
- Location: `byroredux/src/save_io.rs:1196-1211` (`SAVE_TYPE_SOURCES`) vs `:191`
  (`register_component::<ActorValues>`)
- `db121f96` registered `ActorValues` but never added `actor_values.rs` to
  `SAVE_TYPE_SOURCES`, so the `#1714` guard's "scans every save-participating type" claim
  is now false while the test still reports green. No current data loss (zero
  `#[serde(default)]` fields on ActorValue today) but the safety net has a silent hole.
- Suggested fix: add `crates/core/src/ecs/components/actor_values.rs` to
  `SAVE_TYPE_SOURCES`. Optionally consider deriving the list more robustly so a future
  omission fails loudly.
- Completeness: SAVE-REGISTRY, TESTS.
- Domain: binary → `byroredux`

Note: #2014 and #2015 both touch `save_io.rs`'s `SAVE_TYPE_SOURCES`/`build_save_registry` —
will implement together carefully to avoid one fix masking the other's regression guard.
