# Scripting Subsystem Audit — 2026-08-07

Tenth full pass over the M30/M47 Papyrus/`.pex`/ECS scripting domain (prior
reports: `AUDIT_SCRIPTING_2026-06-23.md`, `_06-27.md`, `_07-02.md`, `_07-03.md`,
`_07-06.md`, `_07-16.md`, `_07-21.md`, `_07-25.md`, `_08-03.md`). Run as 7
dimension agents (max 3 concurrent), covering `crates/pex/`, `crates/papyrus/`,
`crates/scripting/`, and the engine-side attach path
(`byroredux/src/cell_loader/references/`, `byroredux/src/asset_provider/`,
`crates/plugin/src/esm/records/`).

**Dedup baseline**: `gh issue list --repo matiaszanolli/ByroRedux --limit 300`
(94 open issues) + direct verification against `docs/audits/AUDIT_SCRIPTING_2026-08-03.md`'s
own findings.

**Test baseline** (aggregated across dimensions): `cargo test -p byroredux-pex`
— 49 passed, 0 failed, 1 ignored (game-data-gated). `cargo test -p
byroredux-papyrus` — 85 unit + 4 integration, all green. `cargo test -p
byroredux-scripting` — **276 passed, 0 failed, 3 ignored** (up from 259 at the
last pass). `cargo test -p byroredux cell_loader::references` — 24 passed, 0
failed. No regressions anywhere in the domain.

## What changed since 2026-08-03

`crates/pex/` and `crates/papyrus/` are **byte-for-byte unchanged** — three
consecutive audit passes (07-25 → 08-03 → 08-07) with zero functional edits
to either crate. `crates/scripting/src/` and the engine attach path absorbed
two feature commits:

- **`a844c26b`** ("complete lifecycle and alias runtime", M47.3 Phases 0–3) —
  the substantial change. Added six new quest-lifecycle `Effect` variants
  (`StartQuest`/`StopQuest`/`CompleteQuest`/`ResetQuest`/`SetQuestActive`/
  `FailAllObjectives`) and their recognizer primitives; widened
  `SetObjectiveDisplayed`/`Completed`/`Failed`'s objective-index field
  `u16`→`i32` (a genuine wire-format fix, verified correct); built out the
  M47.3 quest-alias-fill runtime in `crates/scripting/src/scene.rs`
  (`SceneActorBindings`, `apply_alias_injections`,
  `QuestAliasInjectionState`'s permanent-grant ledger, `ALIAS_FLAG_*`
  wiring, `NearAlias` relation matching); and added
  `stamp_quest_reference`/`spawn_logical_quest_reference`/
  `attach_quest_reference_script` plus an `is_primary_synth` gate to
  `byroredux/src/cell_loader/references/mod.rs`, unifying canonical
  reference-identity stamping across the NPC-actor, invisible-trigger,
  missing-mesh, and static-mesh spawn paths.
- **`0775df28`** ("add runtime observability") — read-only diagnostics
  (`quest_alias_diagnostics`), no behavioral change.
- Eight further commits (`7beb7add`, `31613843`, `30905d4d`, `c5202627`,
  `84a6bea8`, `464ed88a`, `ee45f848`, `32ebfdec`, `19971e77`) are save/load
  registrations, mostly out of this audit's scope — but `84a6bea8`
  ("give up on a MoveTo action once its actor is unresolvable", #2287) and
  `464ed88a` ("cap WaitForActors3DLoaded retries with a wait-time ceiling",
  #2288) are the fixes for last pass's two MEDIUM findings, both
  independently re-verified fixed below.
- `16039d97` (Fix #2277, SCOL/PKIN placement-expansion caching) and
  `30d421cd` (exterior streaming teardown perf) touched the engine attach
  path but are perf-only, verified to introduce no synth-index drift.

## Executive Summary

**What shipped, re-confirmed live, no regressions**: M30.2 `.psc` parser;
M47.0 event hooks; M47.1 condition eval (19 catalog functions); M47.2 `.pex`
reader + 5-phase decompiler + recognizer chain + dynamic attach path + XPRM
trigger volumes + fragment lowerer + QUST VMAD property-table fix +
`AddItem`/`MoveTo` object-targeting effects; the MQ101 PACK/SCEN/DIAL/
two-state-activator/player-control/equipment/`vm_state` runtime (all
2026-07-21/08-03). **New this pass, fully wired and live-verified**: M47.3
quest-lifecycle effects (`StartQuest`/`StopQuest`/`CompleteQuest`/
`ResetQuest`/`SetQuestActive`/`FailAllObjectives`) and the M47.3 quest-alias
fill-and-apply runtime (`SceneActorBindings`, alias-injected faction/
inventory application, the unified canonical reference-identity stamping
across all synthetic-child spawn paths).

**Deferred, correctly, not flagged as defects**: Obscript/SCTX frontend
(Phase 5); the M47.1 condition resolvers' live-headless-cell re-verification;
M47.3 Phase 4+ (Created Object alias spawn, Story Manager event fills, true
`LCTN` traversal, reference-collection aliases, unloaded-world Find-Matching
search, injected packages/spells/keywords overlay families — all documented
bounded follow-ups per `docs/engine/m47-3-quest-alias-design.md`'s "Remaining
subsystem boundary").

**All prior-pass findings independently re-verified fixed in current code**:
- **#2286** (`SetMotionType` literal-integer mis-mapping, HIGH) — still fixed,
  byte-identical canonical table, no new code touches the function.
- **#2287** (`ScenePackagePlayback`'s `MoveTo` never completes on actor
  despawn, MEDIUM) — verified fixed correctly: a `stall_seconds` timeout
  (`MOVE_STALL_TIMEOUT_SECONDS = 5.0`) now gives up and logs a warning
  instead of stalling forever, with no "gives up too eagerly" regression
  (`Transform` is always present once an entity is spawned — a resolve-miss
  only ever means genuine despawn in this engine).
- **#2288** (`FragmentExecutionQueue`'s `WaitForActors3DLoaded` unbounded
  retry, MEDIUM) — verified fixed correctly: an `elapsed_seconds` ceiling
  (`MAX_ACTORS_3D_LOADED_WAIT_SECONDS = 30.0`) drops the entry with a warning
  rather than polling forever, generous enough headroom not to fire on a
  legitimately-slow load.

**Findings this pass: 5 new (0 CRITICAL / 1 HIGH / 1 MEDIUM / 3 LOW), plus 1
finding (SCR-D6-NEW6-01) that independently corroborates an already-filed
cross-domain finding** (`AUDIT_SAVE_2026-08-07.md`'s **SAVE-D6-01, HIGH**) —
not double-counted as a new issue, see below. All 5 counted findings come
from the newly-grown M47.3 surface (Dimensions 5, 6, 7); Dimensions 1–4 (pex
reader/decompiler, Papyrus parser) produced **zero new findings** for the
third consecutive pass, consistent with those crates being byte-for-byte
unchanged.

**Untrusted-input robustness verdict — CLEAN.** Re-verified independently
this pass: every `.pex` primitive read funnels through `take()`; the
`OpCode::from_u8` transmute guard is `>=` with full 51-discriminant test
coverage; hostile var-arg counts never feed `Vec::with_capacity`; both
decompiler recursion caps (`MAX_REBUILD_DEPTH=1024`) and both Papyrus
recursion caps (`MAX_EXPR_DEPTH`/`MAX_STMT_DEPTH=256`) are present and
tested. No panic/OOB/unbounded-alloc path found. None of this pass's new
findings are untrusted-input-reachable — every one is a real-VMAD/real-data
correctness or lifecycle gap in engine-authored logic, not a parser hazard.

**The 99.996% (26640/26641) decompile-rate claim — re-verified honest** a
third time: `pex_corpus_smoke.rs`'s `catch_unwind` wraps `decompile_script`
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
| Boolean (`boolean.rs`) | Yes | Yes (`MAX_REBUILD_DEPTH=1024`, #1815) | Yes | Yes |
| Control-flow (`control_flow.rs`) | Yes | Yes, same cap (#1729) | Yes | Yes |
| Lower (`lower.rs`) | Yes | Yes | Yes | Yes |

`crates/pex/` is functionally unchanged since 2026-07-25 — this matrix is a
third consecutive re-confirmation, not a re-derivation. Pass order in
`decompile_body` (`cfg → lift → boolean → control-flow → lower`) and both
documented deliberate Champollion departures (no debug-line guard in the
boolean pass; the fail-closed `||`-skip in `control_flow.rs`, #1732) remain
correct and adjudicated benign. One doc-rot finding: `docs/feature-matrix.md:157`'s
pipeline-order parenthetical still lists the phases in the wrong order
(see LOW findings).

## Decline-Invariant Audit

The recognizer-chain decline invariant (`crates/scripting/src/translate/`)
**held under the new M47.3 quest-lifecycle and alias-fill surface, with one
real exception (SCR-D5-NEW10-01 below)**. `lower_fragment`'s flat-sequence
model still declines on every `Stmt` shape outside
`VarDecl`/`Assign`/`Return(None)`/`ExprStmt`/the one narrow
`lower_3d_loaded_wait` `While` exception — the six new quest-lifecycle
primitives are all `Stmt::ExprStmt` shapes routed through the existing
`classify_effect` → `EFFECT_PRIMITIVES` table, no new `Stmt::If`/general
`Stmt::While` acceptance path was introduced. The ~967-line M47.3 alias-fill
runtime in `scene.rs` is structurally sound against the invariant: every new
fill-type/flag/injection path declines cleanly on an unresolved alias, an
ineligible candidate, or a documented-boundary fill type, and correctly
rolls back previously-injected state (faction rank, overlay components)
rather than leaking it forward when an alias drops out of resolution.

The one defect found (SCR-D5-NEW10-01) is not a classic "accepts a shape it
shouldn't" leak — it's a **table-order collision between two separately
correct primitives**: `Quest.Start()`/`.Stop()` and `Scene.Start()`/`.Stop()`
share an identical AST shape, and the new, correctly-narrow
`prim_start_quest`/`prim_stop_quest` guard falls through to the
pre-existing, permissive `prim_start_scene`/`prim_stop_scene` on decline
instead of terminating the fragment — silently mis-lowering a quest-start
request into a scene-start request. The domain's own escalation table rates
this HIGH ("recognizer emits a component on an unmodeled condition/term
instead of declining" — here, an unmodeled *ambiguity between two modeled
shapes*, not merely an unmodeled shape).

## Runtime Lifecycle Invariant Matrix

| Invariant | Status |
|---|---|
| Marker drain coverage | CLEAN — no new marker types introduced this session; the new M47.3 structs (`QuestAliasInjectedOverlays`, `QuestAliasRuntimeOverlays`, `QuestDefinitionRegistry`, `QuestAliasInjectionState`) are persistent overwritten-in-place state, correctly outside `cleanup.rs`'s remit |
| Two-phase lock-drop | CLEAN — `timer_tick_system`/`trigger_detection_system`/`recurring_update_tick_system` unchanged; `apply_alias_injections` scopes `FactionRanks`/`QuestAliasRuntimeOverlays`/`Inventory`/`QuestAliasInjectedOverlays` each in their own block with no cross-nesting |
| Cascade / re-entrant dispatch bound | `MAX_CASCADE=64` unchanged and correct. **#2287/#2288 (two unbounded-wait gaps from last pass) both VERIFIED FIXED, no new bug introduced** |
| Quest-stage / VM-state lifecycle | CLEAN — `stages_done` retention, ledger/overlay reconciliation all verified correct by direct test, except SCR-D6-NEW6-01 (a data-correctness bug in the new alias-inventory ledger, cross-referenced with `AUDIT_SAVE_2026-08-07.md`'s SAVE-D6-01, not a lock-ordering defect) |
| Lock-nesting surface (`#2269`-class) | **GREW**: the six new quest-lifecycle effect arms add two more nested resource acquisitions (`QuestDefinitionRegistry`, `SceneActorBindings`) inside `quest_fragment_dispatch_system`'s existing `(QuestStageState, QuestObjectiveState)` hold scope — no live reverse-order caller found for either (consistent with `#2269`'s own "no live deadlock today, becomes real the moment either system parallelizes" risk profile), filed as SCR-D6-NEW6-02, cross-referenced against the still-open `#2269` |
| CTDA OR-precedence | CLEAN, unchanged |
| Edge-trigger seed | CLEAN, unchanged |
| Condition resolver safe-defaults | CLEAN, unchanged (19 catalog functions) |
| Canonical reference-identity stamping (`is_primary_synth`) | CLEAN by direct inspection at all 8+1 call sites — correctly gates every `stamp_quest_reference`/`spawn_logical_quest_reference` site so a SCOL/PKIN one-REFR-to-N-children fan-out registers exactly one `SceneAliasCandidate`; zero test coverage of the invariant itself (SCR-D7-NEW10-01, LOW) |

## Findings

### HIGH

#### SCR-D5-NEW10-01: `MyQuest.Start()` / `MyQuest.Stop()` on a direct (non-locally-rebound) VMAD `Quest` property silently mis-lowers to `StartScene`/`StopScene` instead of `StartQuest`/`StopQuest` or a clean decline

- **Severity**: HIGH
- **Dimension**: Recognizer-Chain Soundness (Dimension 5)
- **Untrusted-Input**: No — a real-VMAD/real-`.pex`-data correctness gap, same class as the closed #2286
- **Location**: `crates/scripting/src/translate/effects.rs:460-473`
  (`explicit_quest_receiver`, new this session), consumed by
  `prim_start_quest`/`prim_stop_quest` (`:475-493`); collides with the
  pre-existing `prim_start_scene`/`prim_stop_scene` (`:629-647`, landed in
  `583a349a`, unchanged this session). `EFFECT_PRIMITIVES` table order
  (`:354-386`): `prim_start_quest`/`prim_stop_quest` are listed *before*
  `prim_start_scene`/`prim_stop_scene` ("first match wins").
- **Status**: NEW (introduced this session by `a844c26b`)
- **Description**: Papyrus's `Quest.Start()`/`Quest.Stop()` and
  `Scene.Start()`/`Scene.Stop()` share the identical zero-arg AST shape
  `<ident>.Start()` / `<ident>.Stop()` — nothing in the AST alone
  distinguishes a `Quest Property` from a scene-form property; that
  information only lives in VMAD property-type metadata, which the
  translate-time recognizer chain does not consult. Before this session,
  only `prim_start_scene`/`prim_stop_scene` existed for this shape, using
  the permissive `receiver_object` fallback (any bare `Ident` not otherwise
  classified → `ObjectRef::Property(name)`). This session added
  `prim_start_quest`/`prim_stop_quest` *ahead* of them in the table, guarded
  by a new, deliberately narrower resolver (`explicit_quest_receiver`) that
  only accepts `Self`, `GetOwningQuest()`, or a local already bound via an
  explicit `Quest k = …` declaration — it declines every bare, unbound
  `Quest`-typed VMAD property reference (the single most common real-world
  shape: a controller script calling `SomeQuestProperty.Start()` on a quest
  it doesn't own, without first copying it to a local). That decline is safe
  *in isolation*, but because `prim_start_quest` returning `None` simply
  falls through to the next table entry rather than terminating the
  fragment, the same bare identifier is then picked up by the unmodified,
  fully-permissive `prim_start_scene`/`prim_stop_scene` and silently
  accepted as a scene reference. The chain never re-considers "maybe this
  bare identifier is actually a Quest and I should decline the whole
  statement" — it commits to whichever primitive matches first, and for
  this one shape the newly-added guard just hands the ambiguous case to the
  wrong sibling instead of removing the ambiguity.
- **Evidence**: Empirically reproduced (temporary test added, run, then
  reverted — working tree confirmed clean):
  ```rust
  let body = first_fn_body(
      "ScriptName QF extends Quest\n\
       Quest Property MQ101 Auto\n\
       Function Fragment_99()\n\
       MQ101.Start()\n EndFunction\n",
  );
  lower_fragment(&body)
  // => Some([StartScene { scene: Property("MQ101") }])
  ```
  A genuine `Quest Property MQ101 Auto` called with `.Start()` — the
  MQ101-quest-controller idiom this audit's evidence base has repeatedly
  cited as real corpus content — lowers to `Effect::StartScene`, not
  `Effect::StartQuest` and not `None`. The crate's own pre-existing test
  `lowers_scene_start_and_stop_requests` (`effects.rs:1361-1379`, unchanged
  this session) pins the *exact* AST shape (`IntroScene.Start()` /
  `OldScene.Stop()` — a bare property identifier) that a real
  `Quest.Start()` call also produces; nothing in the recognizer
  syntactically distinguishes the two. `cargo test -p byroredux-scripting
  translate::` passes 65/65 — this gap has no test coverage in either
  direction.
- **Impact**: Any vanilla or modded quest-controller script that calls
  `.Start()`/`.Stop()` on a directly-referenced (not locally-rebound)
  `Quest Property` — the ordinary way one script starts another quest it
  doesn't own — silently mis-lowers to a scene-start/stop request. At
  effect-application time this very likely *looks* harmless (the "scene"
  lookup for a form that is actually a `QUST` record has no `SceneRegistry`
  entry and silently no-ops), so the practical symptom is **the quest
  silently never starts/stops** — no crash, no log-visible contradiction,
  just a quest that should be running and isn't. This is precisely the
  "silently corrupts game logic" failure mode the dimension's invariant
  exists to catch.
- **Related**: Same conceptual defect family as the closed #2286 (a
  hand-authored assumption instead of declining) but a different mechanism
  — table-order collision between two separately correct primitives rather
  than a wrong literal mapping inside one. Not a duplicate of any currently
  open issue.
- **Suggested Fix**: Make the ambiguity mutual instead of one-sided. Either
  (a) have `prim_start_scene`/`prim_stop_scene`'s object resolver decline
  when the same bare identifier could plausibly be a Quest (track which
  property names appear as a `Quest`-typed VMAD property, already knowable
  from `script_instance`'s property table) and decline both
  `prim_start_quest` *and* `prim_start_scene` when receiver identity can't
  be disambiguated at translate time; or (b) resolve both candidates lazily
  at effect-application time (a single ambiguous `Effect::StartQuestOrScene
  { name }` variant, VMAD-typed resolution at apply time picks the correct
  one, declining only if neither matches) rather than committing to one
  interpretation during translation. Add a regression test pinning the
  exact repro above asserting it does **not** silently become `StartScene`.

### MEDIUM

#### SCR-D6-NEW6-01: `QuestAliasInjectionState`'s permanent-grant ledger is keyed by raw `EntityId`, unconditionally restored on the live in-session cell-reload path where entity ids are never stable — re-grants CNTO items on every live reload of an alias-injecting cell

- **Severity**: MEDIUM as independently rated by this dimension in
  isolation, but this is the **same underlying bug** already filed by
  `docs/audits/AUDIT_SAVE_2026-08-07.md` as **SAVE-D6-01 (HIGH)** —
  discovered independently by both audits on the same day, from two
  different angles (this dimension via runtime-lifecycle tracing of the
  live-reload path; the save audit via M45.1 Live Load-Apply + Validation
  Gates cross-analysis). **Do not file as a second, separate issue** — this
  entry exists for this report's completeness and to record that Dimension
  6's independent trace corroborates SAVE-D6-01's root cause, mechanism,
  and fix recommendation exactly. Treat **SAVE-D6-01 (HIGH)** as the
  canonical severity and the single tracking issue for this bug.
- **Dimension**: Scripting Runtime Systems (Dimension 6)
- **Untrusted-Input**: No — a save/load lifecycle correctness gap in
  engine-authored runtime state, not attacker-controlled data
- **Location**: `crates/scripting/src/scene.rs:162-174`
  (`QuestAliasInjectionState`, `inventory_grants: HashSet<(QuestFormId, i32,
  EntityId, u32, u32)>`), `:668-689` (`apply_alias_injections`'s
  grant-check/insert), `byroredux/src/save_io.rs:328-332`
  (`register_resource::<QuestAliasInjectionState>`), `:920`
  (`restore_resources` call inside `execute_pending_save_loads`)
- **Status**: NEW (introduced this session by `a844c26b`, both the ledger
  and its save registration)
- **Description**: `inventory_grants` dedups CNTO (container-item) grants by
  the tuple `(quest, alias_id, entity, item, count)` — `entity` is a raw
  `EntityId` with no generation tag, and `World::spawn` never reclaims ids
  within a session. The struct is registered via `register_resource`, which
  is restored unconditionally on **both** the full from-menu load
  (`restore_world`, genuinely safe — entity ids preserved verbatim) **and**
  the live in-session reload path (`execute_pending_save_loads`, which
  tears down the current cell and reloads it into the *same, still-running*
  `World`). Because entity-id allocation is monotonic, every actor spawned
  by the reloaded cell gets a strictly-greater `EntityId` than any id used
  before the reload — including the ids embedded in the just-restored
  ledger. Those ledger entries can never match a post-reload alias binding
  again, so the next `quest_alias_refresh_system` tick finds no dedup match
  and grants the CNTO item a second time — on top of whatever `Inventory`'s
  own FormId-keyed delta overlay already restored. Net effect: item
  duplication on every live in-session reload of a cell with a
  quest-alias-injected inventory grant.
- **Evidence**: `World::spawn`'s doc comment confirms monotonic,
  never-reclaimed ids (specifically to prevent stale-reference aliasing,
  #372/#36). `execute_pending_save_loads` confirmed to operate on the same
  `&mut World`, calling `restore_resources` (unconditional) before
  `apply_deltas` (FormId-remapped). The only existing regression test,
  `quest_alias_inventory_grant_ledger_survives_snapshot_round_trip`,
  exercises exclusively the full-restore-into-a-fresh-World path (where ids
  coincidentally line up because it's the first spawn) — no test drives the
  actual `execute_pending_save_loads`-shaped live-reload case.
- **Impact**: any real content using QUST alias-injected inventory
  (`AliasInjectedData::inventory`) that is live-reloaded in-session
  (quicksave/quickload while playing) gets those items duplicated on every
  live reload of the alias-owning cell — a real, repeatable item-duplication
  exploit, not cosmetic.
- **Related**: See `AUDIT_SAVE_2026-08-07.md`'s **SAVE-D6-01** for the
  canonical writeup, full mechanism trace, and suggested fix (key
  `inventory_grants` by `(QuestFormId, alias_id, item, count)` without the
  entity, or give resources a "rebuild fresh on live-reload" lever
  analogous to `MUTABLE_DELTA_COLUMNS` exclusion for components). Also
  related to the same general `EntityId`-in-persisted-state hazard class as
  `#1696` (`AnimationPlayer.root_entity`) and `#2380`
  (`ActorCinematicState`/`HorseTetherState`) — this is the first instance
  landing inside a *resource* rather than a *component*, where the
  component-side mitigation (`MUTABLE_DELTA_COLUMNS` exclusion) has no
  equivalent lever.
- **Suggested Fix**: See SAVE-D6-01. Do not open a second GitHub issue —
  reference this cross-domain corroboration on the existing tracking issue
  for SAVE-D6-01 if useful for fix-verification breadth.

#### SCR-D6-NEW6-02: `a844c26b`'s six new quest-lifecycle `Effect` arms add two more nested resource acquisitions inside the exact hold-scope `#2269` (concurrency audit, open) already flagged as fragile — new, unchecked instances of that issue's own "SIBLING" completeness item

- **Severity**: MEDIUM (rated to match the established sibling finding,
  `#2269`, which the concurrency audit rated MEDIUM for the identical
  mechanism against a different resource pair)
- **Dimension**: Scripting Runtime Systems (Dimension 6)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/fragment.rs:1218-1220`
  (`quest_fragment_dispatch_system`'s `resource_2_mut::<QuestStageState,
  QuestObjectiveState>()` hold scope, spanning the whole cascade loop),
  nested acquisitions at `:459` (`QuestDefinitionRegistry`,
  `StartScene`/`StopScene`), `:750-751`/`:769-771`/`:780-782`/`:849-851`/
  `:862-863` (`QuestDefinitionRegistry`, the new `SetStage`/`StartQuest`/
  `StopQuest`/`CompleteAllObjectives`/`FailAllObjectives` arms), and every
  `crate::scene::mark_scene_actor_bindings_dirty(world)` call inside those
  same arms (`:473`, `:774`, `:797`, `:804`, `:812` — nested-acquires
  `SceneActorBindings` write)
- **Status**: NEW (all six call sites are new in `a844c26b`; the surrounding
  hold-scope and nesting *pattern* is pre-existing and already tracked as
  `#2269`)
- **Description**: `#2269` (open, owned by the concurrency audit, correctly
  not re-derived here) documents that `quest_fragment_dispatch_system` holds
  `(QuestStageState, QuestObjectiveState)` write guards across its entire
  cascade loop, and that two *pre-existing* `apply_effect` arms nested-acquire
  `CinematicPresentationState` from inside that scope — a lock order a
  *different* `add_exclusive` system acquires in reverse. `#2269`'s own
  "Completeness Checks" section lists an explicitly **unchecked** action
  item: "SIBLING: Other `apply_effect` arms checked for the same
  nested-resource-acquisition pattern." `a844c26b`'s six new lifecycle-effect
  arms are exactly that unchecked sibling check, materialized as new code:
  `StartScene`/`StopScene`'s ambiguous-property resolution path and
  `SetStage`/`StartQuest`/`StopQuest`/`CompleteAllObjectives`/
  `FailAllObjectives` all nested-acquire `QuestDefinitionRegistry` (read)
  from inside the same `stages`+`objectives`-held scope, and several
  additionally nested-acquire `SceneActorBindings` (write, via
  `mark_scene_actor_bindings_dirty`). Traced every other acquisition site of
  both resources looking for a live reverse-order caller: **none found** —
  both of `QuestDefinitionRegistry`'s write sites take `&mut World`
  (load-time-only), and `SceneActorBindings`'s consumer
  (`refresh_scene_actor_bindings`) drops its own `QuestStageState` borrow
  before touching `SceneActorBindings`. Consistent with `#2269`'s own stated
  risk profile ("no live deadlock today... becomes a real cross-thread ABBA
  risk the moment either system is promoted to the parallel lane"), not an
  escalation beyond it.
- **Evidence**: `apply_effect`'s and `apply_quest_scoped_effect`'s signatures
  changed this exact commit specifically to add a `world: &World` parameter
  enabling these nested lookups (`apply_quest_scoped_effect(effect, context,
  vmad, stages, objectives)` → `(effect, context, vmad, world, stages,
  objectives)`).
- **Impact**: none live today (same "exclusive-scheduling-only" caveat
  `#2269` already states) — but the surface `#2269`'s eventual fix needs to
  sweep is now larger: two more resources (`QuestDefinitionRegistry`,
  `SceneActorBindings`) join `CinematicPresentationState` as things nested
  inside `quest_fragment_dispatch_system`'s hold scope, landing in the same
  fast-growing function on the same day as the original finding.
- **Related**: `#2269` (open, concurrency-audit-owned) — this finding
  directly answers that issue's own open "SIBLING" completeness checkbox
  with concrete new instances. Recommend appending this evidence to `#2269`
  rather than tracking as a separate issue.
- **Suggested Fix**: same fix shape `#2269` already proposes — resolve
  `QuestDefinitionRegistry`-derived values and the `SceneActorBindings`-dirty
  signal without a nested acquisition while `QuestStageState`/
  `QuestObjectiveState` are held (clone/snapshot `QuestDefinitionRegistry`
  the same way `QuestStageFragments` is already cloned before the hold scope
  begins), and queue the dirty-bindings signal as a post-loop batch flush.

### LOW

#### SCR-D5-NEW10-02: Widened `SetObjective{Displayed,Completed,Failed}` `i32` field has no regression test pinning the new range

- **Severity**: LOW (test-coverage gap; the widen itself is verified correct)
- **Dimension**: Recognizer-Chain Soundness (Dimension 5)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/translate/effects.rs:529,541,552`
  (`i32::try_from(int_arg(args, 0)?).ok()?`); field type widen in the
  `Effect` enum at `:76,83,90`.
- **Status**: NEW (widen itself confirmed correct, coverage gap is new)
- **Description**: The `u16`→`i32` widen for the objective-index field is a
  **genuine bug fix, not a loosened range check** — confirmed against
  `crates/plugin/src/esm/records/misc/quest.rs:77-81`'s
  `QuestObjective::index` doc comment ("signed 32-bit on FO3/FNV, u16 on
  Skyrim+/FO4", `i32` as the documented common representation). No test in
  `effects.rs` exercises a value outside the old `u16` range (0..=65535) —
  neither a negative index (legal per FO3/FNV) nor an `i32`-overflowing
  literal (which must still decline via `.ok()?`).
- **Impact**: None today (the guard reads correctly by inspection, matching
  the pattern #2286's fix also used). Fold into the existing #2289 tracking
  (test-coverage gaps on this file's newer primitives) rather than a new issue.
- **Suggested Fix**: Add one test per `SetObjective*` primitive asserting a
  negative index lowers correctly and one asserting an `i32`-overflowing
  literal declines. Fold into #2289.

#### SCR-D7-NEW10-01: No regression test pins the `is_primary_synth` gate on `stamp_quest_reference`/`spawn_logical_quest_reference`

- **Severity**: LOW (test-coverage gap; every one of the 8 gated call sites
  reads correctly by direct inspection)
- **Dimension**: Engine Attach & Trigger Wiring (Dimension 7)
- **Untrusted-Input**: No
- **Location**: `byroredux/src/cell_loader/references/mod.rs` — the
  `stamp_quest_reference`/`spawn_logical_quest_reference`/
  `attach_quest_reference_script` functions (added `a844c26b`) and their 8
  call sites inside `spawn_synth_child`, plus the standalone `synth_idx ==
  0` gate in `load_references_budgeted`'s NPC-actor path.
- **Status**: NEW
- **Description**: A SCOL/PKIN-expanded REFR fanning into N synthetic
  children, only the first of which should register a `SceneAliasCandidate`,
  is correctly implemented at all 8+1 sites (verified by direct read — see
  the full table in `/tmp/audit/scripting/dim_7.md`, retained in the merge
  evidence below), but no test in this file's `mod tests` spawns a
  multi-child SCOL/PKIN expansion and asserts exactly one
  `SceneAliasCandidate` is registered for the whole REFR.
- **Evidence**: `grep -n "SceneAliasCandidate\|stamp_quest_reference\|spawn_logical_quest_reference\|is_primary_synth" byroredux/src/cell_loader/references/mod.rs | grep -i test`
  returns nothing.
- **Impact**: None today — verified correct by direct reading of every call
  site. But this is exactly the kind of invariant (a boolean gate repeated
  across 8 near-identical branches in a 500-line dispatch function) a future
  9th branch or a collapsing refactor could silently drop without any test
  catching it. A dropped gate would register N `SceneAliasCandidate`s for
  one authored alias-fillable reference, corrupting `SceneActorBindings`'s
  alias-fill resolution for that REFR.
- **Suggested Fix**: Add one regression test exercising `spawn_synth_child`
  against a REFR whose `base_form_id` is a SCOL/PKIN with ≥2 child
  placements, asserting `world.query::<SceneAliasCandidate>().iter().count()
  == 1`. If a full spawn fixture is too heavy, a source-scan test (mirroring
  `scol_expansion_is_cached_across_a_budget_yield`'s technique) asserting
  every `stamp_quest_reference(`/`spawn_logical_quest_reference(` call site
  is preceded by an `is_primary_synth` guard would close the gap at zero
  runtime cost.

#### SCR-D3-NEW10-01: `feature-matrix.md`'s M47.2 row states an incorrect decompiler pass order

- **Severity**: LOW (doc-only; the correct ordering lives in `lower.rs`'s
  and `control_flow.rs`'s own module docs, so an engineer reading source
  would not be misled — only a reader of the feature matrix alone would be)
- **Dimension**: Decompiler Control-Flow/Boolean/Lower (Dimension 3)
- **Untrusted-Input**: No — documentation only
- **Location**: `docs/feature-matrix.md:157`
- **Status**: NEW (not previously filed; confirmed absent from
  `/tmp/audit/scripting/issues.json`'s 94 open issues)
- **Description**: The parenthetical lists the decompiler pipeline as
  `CFG→lift→control-flow→lower→short-circuit`. The real order, verified
  against `decompile_body` in `lower.rs`, is `cfg → lift →
  rebuild_boolean_operators (short-circuit) → reconstruct (control-flow) →
  lower_body`. Two swaps: short-circuit collapse is third, not last;
  control-flow reconstruction is fourth, not third.
- **Evidence**: `crates/pex/src/decompile/lower.rs:230-236`:
  ```rust
  let mut cfg = build_cfg(func)?;
  let mut scopes = lift_function(object, func, &cfg)?;
  // Collapse `&&`/`||` short-circuits before control-flow reconstruction
  // so compound conditions surface as one expression, not nested ifs.
  rebuild_boolean_operators(&mut cfg, &mut scopes, &func.name)?;
  let nodes = reconstruct(cfg, scopes, &func.name)?;
  Ok(lower_body(&nodes))
  ```
- **Impact**: Cosmetic only. A reader relying solely on the feature matrix
  could form an incorrect mental model of pipeline structure.
- **Suggested Fix**: Update `docs/feature-matrix.md:157` to read
  `CFG→lift→short-circuit→control-flow→lower` (matching module names).

## Existing / correctly-tracked (NOT re-filed — dedup)

- **#2286, #2287, #2288** — all independently re-verified fixed in current
  code this pass (see Executive Summary), no regressions.
- **#2289** (test-coverage gap, several effect primitives) — still OPEN;
  SCR-D5-NEW10-02 above folds into this tracking rather than opening a new
  issue.
- **#2290** (`translate/source.rs` doc-rot re: "no `.pex` parser exists") —
  still OPEN, unchanged, not re-derived.
- **#2269** (`CinematicPresentationState`↔`QuestStageState` lock-order
  inversion, concurrency-audit-owned) — still OPEN. SCR-D6-NEW6-02 above
  documents new sibling instances of the same pattern and recommends
  appending to this issue rather than opening a new one.
- **#2270** (scripting's "snapshot before iterate" house rule undocumented)
  — still OPEN, concurrency-audit-owned, not re-derived.
- **SAVE-D6-01** (`AUDIT_SAVE_2026-08-07.md`, HIGH) — the canonical tracking
  for the `QuestAliasInjectionState.inventory_grants` `EntityId` hazard;
  SCR-D6-NEW6-01 above is this dimension's independent corroboration of the
  identical bug, not a separate filing.

## Findings Count

**5 new findings this pass: 0 CRITICAL / 1 HIGH / 1 MEDIUM / 3 LOW**, plus
1 independently-corroborated cross-domain finding (SCR-D6-NEW6-01) matching
the already-filed `AUDIT_SAVE_2026-08-07.md` SAVE-D6-01 (HIGH) — tracked
there, not double-counted here.

By dimension: Dim 1 (pex reader) — 0. Dim 2 (decompiler CFG/lift) — 0.
Dim 3 (control-flow/boolean/lower) — 1 LOW (doc-rot). Dim 4 (Papyrus
lexer/parser) — 0. Dim 5 (recognizer-chain) — 1 HIGH + 1 LOW. Dim 6
(runtime systems) — 2 MEDIUM (1 of which cross-references SAVE-D6-01).
Dim 7 (engine attach) — 1 LOW.

## Future-Phase Readiness

- **SCR-D5-NEW10-01 (Quest/Scene Start/Stop ambiguity)**: needs either a
  translate-time disambiguation (VMAD property-type lookup) or a deferred-
  resolution `Effect` variant — see Suggested Fix. Should be prioritized
  given the HIGH severity and the concrete "quest silently never starts"
  symptom.
- **SCR-D6-NEW6-01 / SAVE-D6-01 (alias-inventory-ledger EntityId hazard)**:
  cross-domain HIGH, canonical tracking in the save audit. The fix (key by
  alias identity, not entity) is cheap and mechanical.
- **SCR-D6-NEW6-02 (#2269 sibling instances)**: append evidence to #2269
  rather than opening a new issue; the eventual `#2269` fix should sweep
  `QuestDefinitionRegistry` and `SceneActorBindings`, not just
  `CinematicPresentationState`.
- **Test-coverage gaps (SCR-D5-NEW10-02, SCR-D7-NEW10-01, and the
  pre-existing #2289)**: mechanical, cheap, worth doing opportunistically.
- **`feature-matrix.md` doc-rot (SCR-D3-NEW10-01)**: one-line fix.
- **Condition resolvers, live-cell re-verification**: unchanged guidance —
  unit-test-clean (19 catalog functions), still not re-verified against a
  live headless cell with real CTDA data.
- **M47.3 Phase 4+**: unchanged — Created Object alias spawn, Story Manager
  event fills, true `LCTN` traversal, reference-collection aliases,
  unloaded-world Find-Matching search, and the injected
  packages/spells/keywords overlay families remain documented bounded
  follow-ups, correctly out of scope.
- **Obscript/SCTX frontend (Phase 5)**: unchanged, not built, correctly out
  of scope.
- **General observation for the next pass**: `crates/pex/` and
  `crates/papyrus/` have now gone three consecutive passes with zero
  functional change and zero new findings — future passes can continue
  light re-verification on those two crates unless a commit actually touches
  them, freeing more session budget for the fast-growing
  `crates/scripting/src/translate/` and `scene.rs` surface, which absorbed
  the domain's only HIGH and both MEDIUM findings this pass. The
  cross-domain duplicate (SCR-D6-NEW6-01 / SAVE-D6-01) discovered
  independently by two different audit skills on the same underlying bug is
  a healthy signal — it means the bug is real and visible from multiple
  angles, not a false positive — but is worth noting as a dedup pattern:
  future orchestration could check the same-day sibling audit reports
  before finalizing, as was done here.

---
*Tenth pass over this domain, run 2026-08-07 across 7 dimension agents
(max 3 concurrent). Dedup baseline: `gh issue list --repo
matiaszanolli/ByroRedux` (94 open issues) + direct verification against
`docs/audits/AUDIT_SCRIPTING_2026-08-03.md`'s findings, all confirmed fixed
(#2286/#2287/#2288) or still correctly open (#2269/#2270/#2289/#2290).
Cross-referenced against `docs/audits/AUDIT_SAVE_2026-08-07.md` for the
SCR-D6-NEW6-01/SAVE-D6-01 duplicate.*
