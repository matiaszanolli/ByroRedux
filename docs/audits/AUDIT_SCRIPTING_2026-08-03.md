# Scripting Subsystem Audit — 2026-08-03

Ninth full pass over the M30/M47 Papyrus/`.pex`/ECS scripting domain (prior
reports: `AUDIT_SCRIPTING_2026-06-23.md`, `_06-27.md`, `_07-02.md`, `_07-03.md`,
`_07-06.md`, `_07-16.md`, `_07-21.md`, `_07-25.md`). Run as one leg of a
`comprehensive` audit-suite sweep, orchestrated across 7 dimension agents
(3 light re-verification + 2 deep dives, foreground/blocking so results
merged inline; a further batch of 3 ran first). All seven dimensions covered:
`crates/pex/`, `crates/papyrus/`, `crates/scripting/`, and the engine-side
attach path (`byroredux/src/cell_loader/references/`,
`byroredux/src/asset_provider/`, `crates/plugin/src/esm/records/`).

**Why this pass looked different from the last eight**: `crates/scripting/src/`
grew by **~10,000 lines** since the 2026-07-25 pass (`git diff --stat` against
that pass's baseline commit shows 9,998 insertions across 29 files) —
entirely new modules never previously audited: `package.rs` (1047 lines, PACK
execution runtime), `scene.rs` (1431 lines, SCEN record playback),
`cinematic.rs` (785 lines), `dialogue.rs` (757 lines), `vm_state.rs` (236
lines), `equipment.rs` (72 lines), `player_control.rs` (149 lines), plus a new
per-script recognizer `translate/recognizers/two_state_activator.rs` (119
lines) and a 1671-line real-data conformance probe
(`examples/mq101_conformance.rs`). Existing files grew substantially too:
`translate/effects.rs` 600→1528 lines, `fragment.rs` +710 lines,
`quest_stages.rs` +520 lines, `condition.rs` +282 lines. By contrast,
`crates/pex/` had **zero functional changes** (only a `Cargo.toml` dependency
line removed) and `crates/papyrus/` only carried the prior pass's own bugfix.
Effort was allocated accordingly: light re-verification for Dimensions 1–4
and 7, full deep-dive read-throughs for Dimensions 5 and 6 where the new
surface lives.

**Dedup baseline**: `gh issue list --repo matiaszanolli/ByroRedux` (47 open
issues) plus direct `gh issue view` on every prior-pass finding referenced
below.

**Test baseline**: `cargo test -p byroredux-pex -p byroredux-papyrus -p
byroredux-scripting` — 49 + 85(+4) + 259 unit tests, all green, 0 failed (up
from 80+4+49+187 at the last pass — scripting alone grew 187→259 tests). Real
Skyrim SE game data was available on disk this pass; `cargo run --release -p
byroredux-scripting --example mq101_conformance` was additionally run
end-to-end against it: **PASS (31 checks)** over the real MQ101 corpus (136
quest aliases, 159 stage bindings, 17 SCEN records, 363 phases, 730 actions,
218 package procedures) — no parser/decompiler/lowering regression.

## Executive Summary

**What shipped, re-confirmed live, no regressions**: M30.2 `.psc` parser;
M47.0 event hooks; M47.1 condition eval (now 19 catalog functions — 13
previously verified plus 6 new: `GetVMScriptVariable`, `GetDead`, `GetInCell`,
`GetEquipped`, `IsSceneActionComplete`, `HasLoaded3D`, all following the
established safe-default-sentinel discipline); M47.2 `.pex` reader + 5-phase
decompiler + recognizer chain + dynamic attach path + XPRM trigger volumes +
fragment lowerer + QUST VMAD property-table fix + `AddItem`/`MoveTo` effects.
**New this pass, fully wired and live-data-verified**: a large ECS-native
runtime for Bethesda's `PACK` (AI package execution), `SCEN` (scene/cinematic
playback), `DIAL`/`INFO` (dialogue), two-state activators, player-control-state
override, equipment/tether effects, and a VM-variable publication layer
(`vm_state.rs`) — collectively the MQ101 opening-cart-sequence slice. All of
it is reached from `byroredux_scripting::register()` +  seven new
`add_exclusive` systems in `boot.rs`, fed by `asset_provider/script.rs`'s
`install_*` family resolving PACK/SCEN/DIAL/IMAD data from the ESM index at
cell-load time (no `.pex` needed for this data — it's inline ESM, only the
VMAD fragment path needs `.pex`).

**Deferred, correctly, not flagged as defects**: Obscript/SCTX frontend
(Phase 5); the M47.1 condition resolvers' live-headless-cell re-verification;
M47.3 quest-alias-fill runtime.

**All prior-pass findings independently re-verified fixed in current code**,
not just closed on GitHub:
- **#2185** / SCR-D4-NEW4-01 (unterminated `State`/`Struct`/`Group` hangs the
  parser at EOF) — fixed via a shared `container_body_at_eof` guard called
  before the catch-all arm in all four container loops.
- **#2188** / SCR-D4-NEW4-02 (bad setter drops a valid getter) — fixed via a
  shared `place()` helper + per-accessor recovery.
- **#2186** / SCR-D5-NEW4-01 (`QuestRef::Property` ignored VMAD alias
  binding) — fixed: `ScriptInstance::object_form_id` now requires `alias ==
  -1`, matching its `ObjectRef::Property` sibling; both new HIGH-growth
  effect families (Dim 5's ~26 new primitives) consume this same fixed
  resolver, confirmed not to have drifted.
- **#2189** / SCR-D7-NEW4-01 (item-record VMAD family never decoded) —
  fixed: `CommonItemFields` now carries `script_instance`, `base_record_
  script_instance` has a working `items` arm.
- **#2191** / SCR-D6-NEW4-01 (hardcoded `ScriptRegistry` demo registration
  never retired) — fixed: the call site and the function itself are both
  gone from `boot.rs`/`papyrus_demo`.

**Two stale audit-trail corrections** (not new bugs, just status fixes so a
future pass doesn't keep re-flagging them): **#2130** (`quest_advance_system`'s
one-signal-per-entity assumption) was closed as fixed by commit `734a0f99`
the same day the 2026-07-25 report shipped — that report's "still open" line
was already stale the moment it was written. Verified directly: a
`HashSet<EntityId>` dedup now spans both `ActivateEvent` and
`OnTriggerEnterEvent` collection loops.

**A known concurrency finding, deliberately not re-derived here**: the
concurrency-audit pass that ran immediately before this one found a MEDIUM
lock-order inversion between `CinematicPresentationState` and
`QuestStageState` in this exact surface. Dimension 6's agent re-traced
`dispatch_player_cinematic_animation_event` and confirmed it is *not*
re-triggering that same site (the presentation-state borrow is dropped before
`QuestStageState` is acquired there) — consistent with the known finding
living elsewhere in the surface. Not re-filed.

**Findings this pass: 5 new (0 CRITICAL / 1 HIGH / 2 MEDIUM / 2 LOW)** — all
from the newly-grown surface (Dimensions 5 and 6); Dimensions 1–4 and 7
(pex reader/decompiler, Papyrus parser, engine attach) produced **zero new
findings**, consistent with those areas being either byte-for-byte unchanged
(`pex`) or having just received a targeted, well-tested fix (`papyrus`,
engine attach).

**Untrusted-input robustness verdict — CLEAN.** Re-verified independently
this pass (not inherited from prior reports): every `.pex` primitive read
funnels through `take()`; the `OpCode::from_u8` transmute guard is `>=` with
full 51-discriminant test coverage; hostile var-arg counts never feed
`Vec::with_capacity`; both decompiler recursion caps
(`MAX_REBUILD_DEPTH=1024`) and both Papyrus recursion caps
(`MAX_EXPR_DEPTH`/`MAX_STMT_DEPTH=256`) are present and tested; the
`container_body_at_eof` fix closes the one live DoS this domain has ever had.
No panic/OOB/unbounded-alloc path found, and no new one was introduced by the
~10,000-line growth (none of it touches untrusted-input parsing — PACK/SCEN
data flows in through the already-hardened ESM reader, and VMAD/`.pex`
resolution is unchanged).

**The 99.996% (26640/26641) decompile-rate claim — re-verified honest** a
second time: `pex_corpus_smoke.rs`'s `catch_unwind` wraps `decompile_script`
and both the panic arm and the `Err` arm feed the printed failure count.

**The `.psc`-vs-`.pex` fidelity gate — verified present, unchanged.**
`recognizes_da10_and_reproduces_hand_builder` passed in this pass's `cargo
test` run; `da10_pex_reproduces_hand_builder_byte_for_byte` remains
`#[ignore]`-gated on Skyrim SE game data as before.

## Decompiler Soundness Matrix

| Pass | Bounds-safe | Terminates | Total (no panic) | Fidelity-tested |
|------|:---:|:---:|:---:|:---:|
| Reader (`reader.rs`) | Yes | Yes | Yes | Yes — unchanged since 07-25, re-confirmed |
| CFG (`cfg.rs`) | Yes | Yes | Yes | Yes — unchanged since 07-25, re-confirmed |
| Lift + copy-prop (`lift.rs`) | Yes | Yes (#2024 O(n) fix intact) | Yes | Yes |
| Boolean (`boolean.rs`) | Yes | Yes (`MAX_REBUILD_DEPTH=1024`) | Yes | Yes |
| Control-flow (`control_flow.rs`) | Yes | Yes, same cap | Yes | Yes |
| Lower (`lower.rs`) | Yes | Yes | Yes | Yes |

`crates/pex/` is functionally unchanged since 2026-07-25 (`git diff --stat`
against that pass's baseline shows only a `Cargo.toml` dependency-line
removal), so this matrix is a re-confirmation, not a re-derivation. Pass
order in `decompile_body` (`cfg → lift → boolean → control-flow → lower`) and
both documented deliberate Champollion departures (no debug-line guard in the
boolean pass; the fail-closed `||`-skip in `control_flow.rs`, #1732) remain
correct and adjudicated benign.

## Decline-Invariant Audit

The recognizer-chain decline invariant (`crates/scripting/src/translate/`)
**held under the ~929-line growth in `effects.rs` and the new
`two_state_activator` recognizer, with one real exception (SCR-D5-NEW5-01
below)**. Re-verified directly against the new code: `lower_fragment`'s
flat-sequence model still declines on every `Stmt` shape outside
`VarDecl`/`Assign`/`Return(None)`/`ExprStmt`/the one narrow
`lower_3d_loaded_wait` `While` exception — no new `Stmt::If` or general
`Stmt::While` acceptance path was introduced despite ~26 new effect
primitives. `RECOGNIZERS` chain ordering in `mod.rs` correctly puts the new
per-script `two_state_activator` before `rumble` before the generic
`quest_stage_gate`. The already-fixed #2186 alias-decline pattern
(`ScriptInstance::object_form_id`'s `alias == -1` check) is consumed
correctly by every new resolver path touching VMAD `Object` properties.

The one defect found is not a classic "accepts a shape it shouldn't" leak —
it's an **accepted value silently bound to the wrong canonical variant**
(see SCR-D5-NEW5-01), which the domain's own escalation table places at HIGH
("recognizer emits a component on an unmodeled condition/term instead of
declining" — here, an unmodeled/mis-modeled *literal value*, not merely its
absence).

## Runtime Lifecycle Invariant Matrix

| Invariant | Status |
|---|---|
| Marker drain coverage | CLEAN — every new transient marker type (`ScenePackageEventBatch`, `EvaluatePackageRequest`, `SceneEventBatch`, `SceneFragmentInvocationBatch`, `DialoguePresentationEventBatch`, `DialogueLineCompletionBatch`, `TwoStateTransitionBatch`, `MotionTypeChangeRequest`, …) is drained either via `cleanup.rs`'s global list or a verified self-drain pattern made sound by `boot.rs`'s fixed `add_exclusive` registration order (`scene_playback_system` → `scene_package_system` → `two_state_activator_system` → `scene_dialogue_system`, confirmed against the scheduler's strictly-sequential exclusive-system guarantee) |
| Two-phase lock-drop | CLEAN across all new collect-then-mutate systems in `package.rs`/`scene.rs`/`dialogue.rs`/`cinematic.rs` — registry snapshots are cloned out of the borrow inline; no new nested-lock site found beyond the already-known, already-logged `CinematicPresentationState`/`QuestStageState` pair |
| Cascade / re-entrant dispatch bound | `quest_fragment_dispatch_system`'s `MAX_CASCADE=64` unchanged and correct. **Two NEW unbounded-wait gaps found this pass** (SCR-D6-NEW5-01, SCR-D6-NEW5-02) — neither is a per-frame cascade, both are cross-frame "never gives up" latent waits keyed on live-entity resolution |
| Quest-stage / VM-state lifecycle | CLEAN — `stages_done` retention, the `QUEST_EVENT_RETENTION=16_384` ring buffer, and the new `vm_state.rs` `TwoStateActivator` do-once gating all verified correct by direct test |
| CTDA OR-precedence | CLEAN, unchanged |
| Edge-trigger seed | CLEAN, unchanged |
| Condition resolver safe-defaults | CLEAN — all 6 new condition functions decline to `0.0` on unresolved data, matching the 13 previously-verified catalog functions |

## Findings

### HIGH

#### SCR-D5-NEW5-01: `SetMotionType`'s literal-integer branch reintroduces the exact `hkpMotion::MotionType` mis-mapping already fixed once (#1652) in the NIF collision importer

- **Severity**: HIGH
- **Dimension**: Recognizer-Chain Soundness (Dimension 5)
- **Untrusted-Input**: No — a real-VMAD/real-`.pex`-data correctness gap
- **Location**: `crates/scripting/src/translate/effects.rs:698-709` (`motion_type_arg`, the literal-`IntLit` branch)
- **Status**: NEW
- **Description**: `motion_type_arg` decodes `<object>.SetMotionType(...)`'s
  first argument two ways: a `MemberAccess` naming a `Motion_*` Papyrus
  constant (resolved correctly, by name, independent of the constant's
  underlying value), or a raw integer literal, hardcoded as `1 =>
  Dynamic, 4 => Keyframed, 5 => Static, 7 => CharacterKinematic, _ => None`.
  This does not match the canonical `hkpMotion::MotionType` enum this exact
  codebase already documents authoritatively in two other places
  (`docs/legacy/nif.xml`'s `hkMotionType` enum and the in-repo Havok 2007
  source's literal enum-to-string table): `4` is BOX_INERTIA (Dynamic), `5`
  is BOX_STABILIZED (Dynamic), `6` is the real KEYFRAMED, `7` is FIXED
  (Static) — CHARACTER is `9`, not `7`. `crates/nif/src/import/collision/mod.rs`
  (`havok_motion_type`) already implements and tests the *correct* table,
  with a doc comment explicitly citing this exact bug's prior instance
  (#1652: "every KEYFRAMED (6) door/platform into immovable Static"). The new
  `effects.rs` branch is an independently-introduced recurrence of the same
  conceptual mistake in an unrelated module — with an even-further-off
  mapping than the original bug had (it invents `5 => Static` and `7 =>
  CharacterKinematic`, neither present in the pre-#1652 error).
- **Evidence**: `gh issue view 1652` (closed) confirms the identical
  canonical table and the identical "4 => Keyframed" error signature in the
  sibling bug. No test in `effects.rs` exercises the literal-`IntLit` branch
  of `motion_type_arg` at all — every existing `SetMotionType` test uses the
  named `MemberAccess` form parsed from hand-written `.psc` source, which is
  immune to this bug because that branch resolves by name.
- **Impact**: `Motion_*` Papyrus properties are `AutoReadOnly` — this
  codebase's own established convention (documented elsewhere in the same
  file) is that Bethesda's vanilla compiler folds `AutoReadOnly` constant
  reads into literal pushes at each call site. Since M47.2 decompiles
  vanilla-shipped `.pex` (not hand-written `.psc`), a real
  `SetMotionType(Motion_Keyframed, ...)` call very likely decompiles as
  `SetMotionType(6, ...)` — a bare literal, not the `MemberAccess` shape the
  tests exercise. Against the canonical table: literal `6` (real KEYFRAMED,
  the single most common explicit motion-type request in scripted content,
  including MQ101's own cart) isn't matched by `{1,4,5,7}` at all → the
  fragment declines where it should succeed; literal `4` (real
  BOX_INERTIA/Dynamic) is wrongly lowered as Keyframed — a freely-dynamic
  object gets frozen/transform-driven instead; literal `7` (real
  FIXED/Static) is wrongly lowered as CharacterKinematic — a static prop or
  door gets misclassified as a character-kinematic body.
- **Related**: Sibling to the closed #1652 (`crates/nif/src/import/collision/mod.rs`)
  — same conceptual bug, independently reintroduced in a different module.
  Verified via `gh issue list` this is not a duplicate of any currently open
  issue.
- **Suggested Fix**: Replace the ad-hoc `{1,4,5,7}` match with the same
  canonical table already implemented and tested in
  `havok_motion_type` (ideally by calling that function directly, or
  extracting it to a shared location, per the project's existing
  "single canonical boundary" convention) — `1..=5|8 => Dynamic, 6 =>
  Keyframed, 7 => Static, 9 => CharacterKinematic`. Add a regression test
  running `lower_fragment` against the raw-literal form
  (`SetMotionType(6, true)` — the shape a decompiled `.pex` actually
  produces for an `AutoReadOnly` constant), asserting `MotionType::Keyframed`,
  plus one each for 4/5/7/9 pinning the corrected values.

### MEDIUM

#### SCR-D6-NEW5-01: `ScenePackagePlayback`'s `MoveTo` action never completes once its actor entity is despawned — permanently stalls any SCEN phase gated on it

- **Severity**: MEDIUM
- **Dimension**: Scripting Runtime Systems (Dimension 6)
- **Location**: `crates/scripting/src/package.rs:523-545` (`tick_command`'s `ScenePackageCommand::MoveTo` arm)
- **Status**: NEW, empirically verified
- **Description**: `tick_command`'s `MoveTo` arm resolves the acting actor's
  live `Transform` every tick and returns `false` ("not yet complete")
  whenever that lookup misses. If the actor entity is despawned mid-travel
  (e.g. exterior cell-streaming unloading the actor's cell while the player
  is elsewhere), `ActiveScenePackageAction` is never removed from
  `ScenePackagePlayback.active_actions` — it is retried every frame forever,
  with no fallback timeout analogous to `MAX_CASCADE` in `fragment.rs` or
  the interaction leaf's `INTERACTION_FALLBACK_SECONDS`. Since scene phase
  advancement requires all `ending_actions` to be `completed` whenever the
  phase has no independent `completion_conditions`, a phase whose only exit
  gate is this stuck action stalls the whole `SCEN` permanently.
- **Evidence**: reproduced directly (built, ran, reverted — tree confirmed
  clean): spawned an actor with a `Transform`, started a `MoveTo` package
  action, despawned the actor, then ran `scene_package_system` 100 times at
  dt=1000.0 (large enough to cover the destination many times over under
  normal conditions) — the action remained active after all 100 ticks. Real
  MQ101 corpus data makes this reachable, not just theoretical: the
  conformance probe's own inventory shows 217 package action stacks and only
  151 phases with authored completion CTDAs — the remaining ~66 rely purely
  on `ending_actions_complete`.
- **Impact**: any real-world despawn of a scene-bound actor mid-`MoveTo`
  silently and permanently stalls that `SCEN`'s progression — no log line,
  no recovery path, no timeout. The sibling `TimedInteraction` variant does
  not share this defect (its completion is a pure countdown, independent of
  entity liveness).
- **Related**: same root-cause shape as SCR-D6-NEW5-02 (a latent wait keyed
  on live-entity resolution with no give-up bound), different file.
- **Suggested Fix**: give the `MoveTo` arm the same kind of fallback bound
  the interaction leaf already has, or explicitly detect actor-entity loss
  and treat it as immediate completion/stop rather than "still traveling."
  Add a regression test alongside the existing
  `resolves_template_and_moves_actor_to_authored_marker` covering the
  despawn-mid-travel case.

#### SCR-D6-NEW5-02: `FragmentExecutionQueue`'s `WaitForActors3DLoaded` continuation has no retry cap or eviction path

- **Severity**: MEDIUM
- **Dimension**: Scripting Runtime Systems (Dimension 6)
- **Location**: `crates/scripting/src/fragment.rs:130-147` (`FragmentExecutionQueue`), `:248-258` (`actors_3d_loaded`), `:808-883` (`fragment_continuation_system`)
- **Status**: NEW
- **Description**: `apply_effects` suspends a fragment tail into
  `FragmentExecutionQueue` on `Effect::Wait` or an unresolved
  `Effect::WaitForActors3DLoaded`. For the `Actors3DLoaded` resume condition,
  if still unresolved, `fragment_continuation_system` simply re-arms and
  re-queues the entry — indefinitely, with no maximum retry count, no
  elapsed-time ceiling, and no eviction hook tied to `QuestStageState::reset`
  or the referenced actor's despawn. Unlike the sibling cascade in
  `quest_fragment_dispatch_system` (`MAX_CASCADE=64`, logged and broken on
  overflow), there is no analogous backstop here.
- **Evidence**: the crate's own existing test
  `actor_3d_load_gate_polls_without_blocking_then_resumes` directly
  demonstrates the unbounded-poll half of this — after one tick with the
  actor unresolved, the entry is retained (`len() == 1`), not dropped. No
  cleanup path clears an entry on `QuestStageState::reset` or actor despawn
  anywhere in the crate.
- **Impact**: bounded in practice today — real MQ101 corpus data shows
  exactly 1 such effect — so not a per-frame-exploitable growth vector
  currently, but a genuine structural gap: a quest reset or a
  permanently-unloadable alias target has no backstop against a queue entry
  living for the rest of the process's life.
- **Related**: SCR-D6-NEW5-01 (same "unbounded wait on live-entity
  resolution" shape, different subsystem) — a shared fix (a generic
  "give up after N attempts / M seconds" wrapper) would address both.
- **Suggested Fix**: add a maximum total wait duration or poll-retry count
  to the `Actors3DLoaded` arm, logging a `warn!` and dropping the entry
  (declining the tail, matching the crate's existing "skip, never guess"
  contract) once exceeded — mirroring `MAX_CASCADE`'s shape in the same
  file. Consider draining pending entries whose context quest is reset.

### LOW

#### SCR-D5-NEW5-02: Several new effect primitives ship with zero decline-path test coverage

- **Severity**: LOW (test-coverage gap; every primitive checked is structurally sound by inspection)
- **Dimension**: Recognizer-Chain Soundness (Dimension 5)
- **Location**: `crates/scripting/src/translate/effects.rs` test module (lines 959-1527)
- **Status**: NEW
- **Description**: Of the ~26 new effect primitives, roughly half have an
  explicit decline-path regression test alongside their positive-path test.
  The rest (`SetOpen`, `SetPlayerRestrained`, `SetPlayerControls`/
  `DisablePlayerControls`/`EnablePlayerControls`, `SetPlayerAiDriven`,
  `SetHudCartMode`, `PlayIdle`, `SetVehicle`, `TetherToHorse`,
  `SetMotionType`'s own arg-count/unrecognized-member decline path,
  `SetSittingRotation`, `ExitCart`, `PlayerImodAnimation`/
  `PlayerFurnitureAnimation`, `EvaluatePackage`, `Wait`,
  `StartScene`/`StopScene`) have a positive-path test but no test pinning
  their `?`/arg-count/arg-type guard.
- **Impact**: None today (every guard read correctly by inspection). But an
  unpinned guard is exactly what a future "simplifying" refactor can
  silently loosen without any test catching the regression.
- **Suggested Fix**: Add one decline-path test per untested primitive
  (a single `assert_eq!(lower_fragment(&body), None)` each). Prioritize
  `SetMotionType`'s arg-count/unrecognized-member decline and `ExitCart`'s
  seat-range boundary as the most structurally intricate of the untested
  set.

#### SCR-D5-NEW5-03: `translate/source.rs`'s module doc still claims "no `.pex` parser exists"

- **Severity**: LOW (doc rot only)
- **Dimension**: Recognizer-Chain Soundness (Dimension 5)
- **Location**: `crates/scripting/src/translate/source.rs:17-20`
- **Status**: NEW (pre-existing drift, not introduced by this session's growth, surfaced by this pass's full-file re-read)
- **Description**: The doc comment says a `.pex` frontend "is intentionally
  NOT a variant yet because no `.pex` parser exists." `translate_pex` has
  parsed and decompiled `.pex` bytes through this exact boundary since
  commit `c5293ef7` (2026-06-22); `source.rs` itself hasn't been touched
  since 2026-05-29.
- **Impact**: Cosmetic only — a reader of `source.rs` in isolation would
  wrongly conclude `.pex` support doesn't exist.
- **Suggested Fix**: Update the paragraph to state `.pex` is supported via
  `translate_pex`'s decompile-then-`PapyrusSource` path.

## Existing / correctly-tracked (NOT re-filed — dedup)

- **#2130** — `quest_advance_system`'s one-signal-per-entity assumption.
  Was already CLOSED (commit `734a0f99`, 2026-07-25) before the last report
  shipped; that report's "still open" line was stale on arrival. Verified
  fixed directly this pass via the `HashSet<EntityId>` dedup spanning both
  event-collection loops. No action needed.
- **The `CinematicPresentationState`/`QuestStageState` lock-order
  inversion** — owned by the concurrency audit that ran immediately before
  this one. Confirmed (Dimension 6) that the specific site re-traced this
  pass is not a re-trigger of that same finding. Not re-derived or re-filed
  here.

## Future-Phase Readiness

- **SCR-D5-NEW5-01 (motion-type mapping)**: cheap, mechanical fix — reuse
  the existing canonical table from `crates/nif/src/import/collision/mod.rs`
  rather than inventing a second one. Worth extracting to a shared location
  both crates can depend on, consistent with the project's single-canonical-
  boundary convention (NIFAL's `translate_material` precedent).
- **SCR-D6-NEW5-01 / SCR-D6-NEW5-02 (unbounded latent waits)**: both share
  one root cause (a wait keyed on live-entity/live-condition resolution with
  no give-up bound) — a single shared "bounded latent wait" helper could
  close both at once, and would proactively guard the next latent-wait
  effect this fast-growing surface adds.
- **Test-coverage gaps (SCR-D5-NEW5-02)**: mechanical, cheap, worth doing
  opportunistically alongside any other touch of `effects.rs`.
- **Condition resolvers, live-cell re-verification**: unchanged guidance
  from all prior passes — unit-test-clean (now 19 catalog functions), still
  not re-verified against a live headless cell with real CTDA data.
- **M47.3 quest-alias-fill runtime**: unchanged — out of this skill's crate
  scope.
- **Obscript/SCTX frontend (Phase 5)**: unchanged, not built, correctly out
  of scope.
- **General observation for the next pass**: this domain just absorbed its
  largest single-pass growth to date (+10k LOC in under two weeks, driven by
  the MQ101 cinematic slice). The decline invariant and untrusted-input
  hardening both held up well under that pressure — the one real defect
  found (motion-type mapping) was a hardcoded numeric table where the
  codebase already had a canonical, tested source of truth elsewhere; the
  two MEDIUM findings were both "no give-up bound" gaps in brand-new latent-
  wait machinery. None of this suggests the invariant itself is eroding,
  but the surface is now large enough (`crates/scripting/src` ~18.4k LOC
  incl. tests) that a future pass should consider splitting Dimension 6 into
  two (core runtime lifecycle vs. the SCEN/PACK/cinematic/dialogue
  subsystem) the way Dimension 5's per-primitive checklist already implies,
  to keep single-session context budgets from being strained the way this
  pass's two deep-dive agents both ran 40-50+ tool calls each.

---
*Ninth pass over this domain, run 2026-08-03 as part of a comprehensive
audit-suite sweep. Orchestrated across 7 dimension agents (light
re-verification for Dims 1-4 and 7; full deep-dive reads for Dims 5-6 given
the ~10,000-line growth since the last pass), all run in the foreground
(blocking) per this session's structural constraint against background
sub-agent delegation. Dedup baseline: `gh issue list --repo
matiaszanolli/ByroRedux` (47 open issues) + `docs/audits/AUDIT_SCRIPTING_2026-07-25.md`
+ direct `gh issue view` confirmation that #2185/#2186/#2188/#2189/#2191/#2130
are closed and their fixes hold in current code. The already-known
concurrency-audit finding (`CinematicPresentationState`/`QuestStageState`
lock inversion) was deliberately not re-derived or re-filed.*
