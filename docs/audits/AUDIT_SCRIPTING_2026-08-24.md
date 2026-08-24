# Scripting Subsystem Audit — 2026-08-24

Fourteenth full pass over the M30/M47 Papyrus / `.pex` / ECS scripting domain
(prior reports: `AUDIT_SCRIPTING_2026-06-23.md` … `_08-16.md`, `_08-20.md`).
Run single-agent, no sub-agent fan-out, per this session's explicit
instruction (nested-agent scripting/audit fan-out has stalled or dropped
findings elsewhere today). Comprehensive — no `--focus` filter, all 8
dimensions covered directly via source reads, greps, `cargo check`, and
git archaeology.

**cargo WAS run this pass** (`cargo check -p byroredux-scripting --examples`),
unlike 2026-08-20's static-only pass — see the build-break cross-reference
below, which this session specifically asked this audit to verify.

**Dedup baseline**: `gh issue list --repo matiaszanolli/ByroRedux --limit 300
--json number,title,state,labels` (saved to `/tmp/audit/scripting/issues.json`
during the run, since cleaned up per Phase 4), `docs/audits/AUDIT_SCRIPTING_2026-08-20.md`,
and `git log --since=2026-08-20` over `crates/pex/`, `crates/papyrus/`,
`crates/scripting/`, `crates/hkx/`, and the engine-side attach/cinematic path.

## What changed since 2026-08-20

`crates/pex/` and `crates/papyrus/` have **zero commits** since 2026-08-19 —
Dims 1–4 (reader/opcode, CFG/lift, control-flow/boolean/lower, `.psc`
lexer/parser) are byte-for-byte unchanged from the 2026-08-20 pass. All churn
is in `crates/scripting/` (ten commits, 2026-08-23/24) plus one `crates/core`
lock-tracker change that surfaced a pre-existing scripting-side hazard:

| Commit | Effect on this domain |
|---|---|
| `27875a02` | Scene-lifecycle fragment dispatch: `SceneFragments`, `scene_fragment_dispatch_system`, mirroring quest-fragment dispatch for SCEN `Begin`/`End`/phase events |
| `7473a387` | Actor-specific trigger gating: `ActivatorGate::BaseForm`, `QuestTriggerApproachRegistry`, multi-triggerer `OnTriggerEnterEvent` (`Vec<EntityId>`), `TriggerVolume::intersects_sphere`, tethered-horse detection, `TriggerOccupancyState` |
| `5f38402e` | `ReferenceEnableState` + `Effect::Disable` |
| `cee35507` | `Effect::SetGlobalValue` (`Globals`, save-registered) + `Effect::Conditional` (narrow `GetStageDone`-guarded `If`/`Else`) + the multi-`Fragment_N`-per-stage merge fix + cascade-queue FIFO/ingress-vs-cascade rework |
| `25a0aabd` | Cascade-queue hardening follow-up |
| `eb2e2445` | `QuestAliasReadinessGate` — engine-authored alias-readiness-driven `SetStage` |
| `4e1afcbe`, `5428e872` | ECS lock-tracker changes (`crates/core`) — not scripting-owned, but surfaced ECS-2026-08-24-01 in `fragment.rs`, see below |

**Six of these ten commits landed the same day this audit runs** — this is
the newest, least-reviewed code in the domain, and where this pass
concentrated effort (Dims 5/6/7/8). Dims 1–4 got a spot-check of their
standing invariants (transmute guard, all four recursion caps, `catch_unwind`)
rather than a full re-read, since nothing in those files changed.

## Cross-cutting items this session specifically asked this audit to verify

### The `cargo test --workspace` build break — CONFIRMED, still broken, already filed elsewhere

Reproduced directly:

```
$ cargo check -p byroredux-scripting --examples
error[E0004]: non-exhaustive patterns: `&Effect::Conditional { .. }`,
  `&Effect::SetGlobalValue { .. }` and `&Effect::Disable { .. }` not covered
 --> crates/scripting/examples/fragment_coverage.rs:59:11
error: could not compile `byroredux-scripting` (example "fragment_coverage")
  due to 1 previous error
```

`crates/scripting/examples/fragment_coverage.rs:59`'s `match e { .. }` over
`Effect` (`crates/scripting/src/translate/effects.rs:68`) has not been
updated for the three variants `cee35507`/`5f38402e` added earlier today.
`cargo test --workspace` builds every workspace target — including
examples — before running any test binary, so this aborts the whole
invocation with zero tests executed, workspace-wide; `cargo test -p
byroredux-scripting` (no `--lib`, as `CLAUDE.md`'s own Quick Reference
documents) fails identically. `cargo test -p byroredux-scripting --lib`
still passes cleanly (isolated separately) — this is a build-target gap,
not a logic regression.

**This is squarely in this audit's crate scope, but it is already filed**:
`docs/audits/AUDIT_SAFETY_2026-08-24.md` (published earlier today) carries it
as **SAFE-BUILD-2026-08-24-01 (HIGH)**, with the identical repro and root
cause. Per the dedup protocol this is carried here as **Existing:
SAFE-BUILD-2026-08-24-01**, not re-filed as a second finding — but it is
listed in this report's tally line because it directly affects how much
weight this pass's "verified via `cargo check`" claims can carry (see each
finding's own verification note below).

**Suggested fix** (unchanged from the safety audit): add the three missing
match arms to `fragment_coverage.rs` — one line each, using the compiler's
own suggested arm signatures.

### ECS-2026-08-24-01 — double `Transform` read-guard in `fragment.rs` — CONFIRMED, already filed, cross-referenced not duplicated

`docs/audits/AUDIT_ECS_2026-08-24.md` files **ECS-2026-08-24-01 (MEDIUM)**:
`Effect::SetVehicle` (`crates/scripting/src/fragment.rs:916-917`) and
`Effect::TetherToHorse` (`:942-946`) each hold two `ComponentRef<Transform>`
read guards simultaneously (`world.get::<Transform>(actor)` then
`world.get::<Transform>(vehicle)` while the first guard is still live),
which the newly-added `#2386` recursive-read diagnostic
(`crates/core/src/ecs/lock_tracker.rs`, landed this session in `5428e872`)
now flags with `log::warn!` on every call. Re-verified directly against the
live code at both line ranges — the finding is accurate. The mitigating
fact ECS-2026-08-24-01 already documents also holds: both fragment-dispatch
systems that reach these arms are registered `add_exclusive`
(`byroredux/src/boot.rs`), so the pattern is a real-today log-spam +
diagnostic-noise issue and a latent (not currently reachable) deadlock
vector, not an active hang. **Carried here as Existing: ECS-2026-08-24-01,
not duplicated.**

## Decompiler Soundness Matrix (Dims 1–4, unchanged since 2026-08-20)

| Pass | Bounds-safe | Terminates | Total (no panic) | Fidelity-tested |
|------|:---:|:---:|:---:|:---:|
| Reader (`reader.rs`) | Yes | Yes | Yes | Yes |
| CFG (`cfg.rs`) | Yes | Yes | Yes | Yes |
| Lift + copy-prop (`lift.rs`) | Yes | Yes (#2024 linear chain) | Yes (#2666 fail-closed) | Yes |
| Boolean (`boolean.rs`) | Yes | Yes (`MAX_REBUILD_DEPTH=1024`) | Yes (#2667) | Partly (per 08-20) |
| Control-flow (`control_flow.rs`) | Yes | Yes, same cap | Yes (#1732 fail-closed) | Partly (per 08-20) |
| Lower (`lower.rs`) | Yes | Yes | Yes | Yes for straight-line/property/event shape |

Re-verified this pass by direct grep, not full re-read (no source changed):
`MAX_OPCODE = 51`, contiguous `#[repr(u8)]` 0..=50, `transmute` guarded
`byte >= MAX_OPCODE`; the var-arg vec still `Vec::new()` + `push` (#1710,
not `with_capacity(n)`); `MAX_REBUILD_DEPTH = 1024` present in **both**
`control_flow.rs` and `boolean.rs`; `translate_pex` still wraps
`decompile_script` in `catch_unwind`
(`crates/scripting/src/translate/mod.rs:112`); `MAX_EXPR_DEPTH` /
`MAX_STMT_DEPTH = 256` present in `crates/papyrus/src/parser/{expr,stmt}.rs`.
No regression found. **Untrusted-input robustness verdict for Dims 1–4:
unchanged, CLEAN.**

## Decline-Invariant Audit

| Decline point | Verdict |
|---|---|
| `classify_guard_atom` `?` in `classify_if_condition`'s per-atom loop | Conservative (unchanged) |
| `split_and` refusing to split `\|\|` | Conservative, deliberate (unchanged) |
| `lower_statements`'s `_ => return None` statement arm | Conservative; two narrowed exceptions confirmed exactly as documented — `Stmt::While` only through `lower_3d_loaded_wait`, `Stmt::If` only through the `Effect::Conditional` shape (empty `elseif_clauses`, every guard atom an exact `StageDone{0.0\|1.0}`, no latent effect in either branch, cloned `then_scope`/`else_scope`) |
| `apply_effect`'s `Effect::Conditional { .. } => unreachable!()` / `apply_quest_scoped_effect`'s mirrored arm | Both confirmed present as defense-in-depth; `apply_effects` intercepts `Conditional` before either is reached |
| `Effect::Conditional` guard-fail-closed | Confirmed: `guards.iter().all(|g| resolve_quest_logged(..).is_some_and(|q| stages.get_stage_done(q, g.stage) == g.done))` — an unresolvable guard quest makes `is_some_and` return `false`, so `.all()` fails and the `else` branch runs; a wrong-default-to-true bug would require a very different shape than what's here |
| Multi-`Fragment_N`-per-stage merge (`populate_quest_fragments_from_script`) | Confirmed correct: per-binding decline is independent (each `Fragment_N` is a genuinely separate script-method invocation in the real VM, not a sequential statement of one program), authoring order preserved via `stage_order`, installed once at the end, function-local state so repeated calls don't accumulate. **This is the right decline granularity, not a violation of decline-on-unmodeled** — see "Considered and disproved" below for the reasoning that ruled out a false-positive I initially suspected here |
| Cascade-queue FIFO / `is_cascade` gating (`quest_fragment_dispatch_system`) | Confirmed correct: `cascade_steps` increments only on `is_cascade == true`; WARN fires on overflow; `adv.previous_stage != adv.new_stage` gate skips no-op re-sets before requeuing |
| `Disable`'s receiver resolution vs. its sibling `AddItem`/`MoveTo` | **Inconsistent — see SCR-D5-2026-08-24-01** |
| `SceneActorBindings::resolve` on an unfilled alias | Returns `None`, never fabricates an entity (unchanged) |
| `QuestRef::Property` on an alias-bound entry | Still declines (unchanged, correct) |

## Runtime Lifecycle Invariant Matrix

| Invariant | Verdict |
|---|---|
| Marker drain coverage (`event_cleanup_system`) | `SceneFragmentInvocationBatch` correctly added to the Pattern-A drain list alongside the other batch markers; no marker found emitted-but-undrained |
| `scene_fragment_dispatch_system` scheduling (`boot.rs`) | Confirmed `add_exclusive`, and confirmed ordered `scene_playback_system` (:889) → `scene_fragment_dispatch_system` (:895) → … → `quest_fragment_dispatch` (:920), matching the in-source claim that a SCEN fragment's `SetStage` is visible to quest-fragment dispatch the same frame |
| Scene fragment `SetStage` reaching quest-fragment dispatch same-frame | Confirmed via the journal-mirror dedup mechanism: a scene fragment's `stages.set_stage()` call publishes to the same quest-event journal `quest_fragment_dispatch_system`'s own `poll_quest_events()` reads later the same frame, so the transition is picked up and cascaded correctly even though the two systems never share a direct call |
| `QuestStageAdvancedBatch(player_entity)` final-state consistency across the now-five same-frame writers | **Violated — see SCR-D6-2026-08-24-01** |
| `QuestAliasReadinessGate` guard triple (`is_running`, `< only_below_stage`, `!get_stage_done`) | Confirmed all three present and in the stated order |
| `cinematic_retained_entities` transitive `Children` walk | Confirmed genuinely transitive (stack-based DFS, `retained.insert(child)` as the visited-check, no depth limit needed since the ECS hierarchy is finite and acyclic by construction) |
| `actor_quest_trigger_is_in_sequence` (`trigger.rs`) vs. `scene_trigger_actor_approach_system` (`cinematic.rs`) agreement | Investigated in full — see "Considered and disproved" below. The between-scenes case is provably equivalent (both reduce to "global min not-done target_stage across all `BaseForm` advances for the quest"). The running-scene case has an asymmetry (gate: `target_stage <= awaited_stage`; router: `target_stage == awaited_stage`) but it is one-directional and safe: every trigger the router sends an actor toward is provably also allowed by the gate (equality is a subcase of `<=`), so "routed then refused" — the hazard the skill's checklist named — cannot happen. Not filed |
| `TriggerOccupancyState` pruning (`occupancy.inside.retain(|key,_| observed.contains(key))`) | Confirmed present and correctly scoped to the per-tick `observed` set built in the same loop |
| Two-phase lock-drop (`trigger_detection_system`, `timer_tick_system`, `recurring_update_tick_system`) | Unchanged, still correct |
| `Globals` save registration | Confirmed: `#[cfg_attr(feature = "save", derive(Serialize, Deserialize))]` on `Globals`, and `byroredux/src/save_io.rs:380` registers it — not a #1862-class gap |

## Findings

### MEDIUM

#### SCR-D6-2026-08-24-01: `quest_fragment_dispatch_system`'s tail `QuestStageAdvancedBatch` write is the one non-defensive producer among five same-frame writers to the same component

- **Severity**: MEDIUM
- **Dimension**: Scripting Runtime Systems (Dimension 6)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/fragment.rs:1928-1931` (the defect);
  contrast with the correct pattern at
  `crates/scripting/src/papyrus_demo/quest_advance.rs:467-473`,
  `crates/scripting/src/quest_stages.rs:947-953` (`quest_alias_readiness_stage_system`),
  `crates/scripting/src/quest_stages.rs:1129-1137` (`install_start_game_quests`),
  and `crates/scripting/src/fragment.rs:1441-1446`
  (`fragment_continuation_system`) — all five of which check
  `batches.get_mut(player)` and `extend()` before falling back to `insert()`.
- **Status**: NEW
- **Description**: `quest_advance.rs:463-466`'s own comment states the
  invariant every other writer to this component follows: *"append the whole
  producer batch while holding the storage write lock. Another same-frame
  producer may already have populated the compatibility sink; replacing it
  would lose its events."* `quest_fragment_dispatch_system`'s own tail
  (`fragment.rs:1928-1931`) does not follow it — it calls
  `q.insert(player_entity, QuestStageAdvancedBatch(chained))` unconditionally,
  with no `get_mut`-and-extend check first. This code is not new (it dates to
  `6fcae7ab`, 2026-07-03, Fix #1864), but at that time `quest_fragment_dispatch`
  was effectively the *last* writer registered before end-of-frame cleanup, so
  the omission was harmless. Two same-frame producers now run **immediately
  before** it in the schedule and were added specifically this session
  (`boot.rs:883` `quest_alias_readiness_stage_system`, `boot.rs:895`
  `scene_fragment_dispatch_system`) — both correctly defensive on their own
  side, both landed `2026-08-23`. If either one populates
  `QuestStageAdvancedBatch(player_entity)` in a frame where
  `quest_fragment_dispatch_system` *also* produces a non-empty `chained`
  (i.e. a fragment's own effects cause a further `SetStage`), the earlier
  producer's events are silently discarded from the component's final state
  for the frame — not lost from processing (both `quest_alias_readiness_stage_system`'s
  and `scene_fragment_dispatch_system`'s own `SetStage`s are separately picked
  up via the quest-event journal, so no quest transition is functionally
  dropped today), but discarded from the one place downstream same-frame
  consumers are meant to look. There is currently no such consumer in the
  tree (`grep` found only `event_cleanup_system`'s drain and the systems
  above — no journal-UI or save-notification reader exists yet), which is
  exactly why this is MEDIUM and not HIGH: it is a real, silent violation
  of a documented cross-system contract, sitting dormant only because
  nothing yet depends on the marker's late-frame contents. The very next
  same-frame consumer the code's own comments anticipate ("journal UI,
  further-frame dispatch") would silently see an incomplete batch.
- **Evidence**:
  ```rust
  // fragment.rs:1928-1931 — the one non-defensive writer
  let player_entity = world.resource::<crate::papyrus_demo::PlayerEntity>().0;
  if let Some(mut q) = world.query_mut::<QuestStageAdvancedBatch>() {
      q.insert(player_entity, QuestStageAdvancedBatch(chained));
  }
  ```
  ```rust
  // quest_advance.rs:463-473 — the documented, followed-everywhere-else pattern
  // "Another same-frame producer may already have populated the
  // compatibility sink; replacing it would lose its events."
  let Some(mut q) = world.query_mut::<QuestStageAdvancedBatch>() else { return; };
  if let Some(batch) = q.get_mut(player_entity) {
      batch.0.extend(advances_emitted);
  } else {
      q.insert(player_entity, QuestStageAdvancedBatch(advances_emitted));
  }
  ```
- **Impact**: None observable today (no consumer reads the post-dispatch
  marker state). Becomes a real, silent data-loss bug the moment a
  same-frame consumer is added after `quest_fragment_dispatch` in the
  schedule — exactly the shape #1864 was originally filed to prevent, now
  reopened one producer at a time as the scene/alias-readiness runtime grew
  around it.
- **Related**: Not a duplicate of #1864 (CLOSED, fixed the intra-call
  looping-insert case; this is a *cross-system* instance of the same class
  the fix commit's own comment already warns about). No existing open issue
  covers this exact site.
- **Suggested Fix**: Apply the same `get_mut`-then-`extend`-else-`insert`
  three-liner already used by the other five writers. One-line-shape fix,
  matching an established local pattern.

#### SCR-D5-2026-08-24-01: `Effect::Disable` has no production consumer, and its receiver resolution is narrower than its sibling object-targeting effects for no documented reason

- **Severity**: MEDIUM
- **Dimension**: Recognizer-Chain Soundness (Dimension 5)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/translate/effects.rs:803-810`
  (`prim_disable` — the lowering, shares `receiver_object` with `AddItem`/`MoveTo`);
  `crates/scripting/src/fragment.rs:741-748` (the dispatch — uses the
  strict `resolve_property_form_id`, not the alias-aware `resolve_object`);
  `crates/scripting/src/fragment.rs:65-73` (`ReferenceEnableState::is_enabled`)
- **Status**: NEW
- **Description**: Two independent defects in the same 2026-08-24 addition
  (`5f38402e`).

  **(a) No runtime consumer.** `ReferenceEnableState::is_enabled` is called
  from exactly one place in the whole tree —
  `crates/scripting/src/fragment/tests.rs:1412` — and nowhere in
  `byroredux/`. `grep -rln "ReferenceEnableState" --include="*.rs" .`
  (excluding tests) returns only `save_io.rs` (registration) and
  `fragment.rs` (definition + the write side). Nothing in cell loading,
  streaming, or rendering consults it, so a `Disable()` effect records
  intent but currently has zero observable runtime effect — a reference a
  script disables stays fully visible, collidable, and interactive.

  **(b) Receiver-resolution asymmetry.** At lowering time, `prim_disable`
  classifies its receiver through the exact same `receiver_object` function
  `AddItem`/`MoveTo`/`EquipItem` use (`effects.rs:807`:
  `object: receiver_object(object, scope)?`), which is what lets those
  siblings bind to a quest-alias-filled `ObjectReference Property` (an
  `ObjectRef::Property` with `alias >= 0`). But at dispatch time,
  `Effect::Disable`'s arm resolves that same `ObjectRef` through
  `resolve_property_form_id(vmad, object.property_name())` — the strict,
  non-alias-aware resolver `QuestRef::Property`/`SetGlobalValue`/`StartScene`/
  `PlayIdle` correctly use for genuinely non-alias-fillable target types
  (globals, scenes, idles, quests). `AddItem`/`MoveTo`/`EquipItem`/`Activate`/
  `SetOpen`, by contrast, resolve their `ObjectRef` receivers through
  `resolve_object`, which branches on `alias >= 0` and resolves live through
  `SceneActorBindings`. `Disable`'s receiver is, per the skill's own framing
  of this domain, "in authored content, frequently the same kind of
  scene-marker `ObjectReference Property`" the alias-aware siblings resolve —
  so `<AliasBoundMarker>.Disable()` silently declines (form-id lookup fails
  on an alias-bound property, which by construction has no static form id)
  in exactly the cases where the equivalent `AddItem`/`MoveTo`/`SetOpen` on
  the same alias-bound reference would resolve and apply. Nothing in the
  commit or any doc documents this as a deliberate narrower scope for
  `Disable` specifically.
- **Evidence**:
  ```rust
  // effects.rs:803-810 — lowering shares receiver_object with AddItem/MoveTo
  fn prim_disable(e: &Expr, scope: &Scope) -> Option<Effect> {
      let (object, args) = method_call(e, "Disable")?;
      if args.len() > 1 { return None; }
      Some(Effect::Disable {
          object: receiver_object(object, scope)?,   // same fn as AddItem/MoveTo
          fade_out: bool_arg(args, 0)?.unwrap_or(false),
      })
  }
  ```
  ```rust
  // fragment.rs:741-748 — dispatch uses the strict, non-alias-aware resolver
  Effect::Disable { object, fade_out: _ } => {
      let form_id = resolve_property_form_id(vmad, object.property_name())?;
      deferred.reference_enable_changes.push((form_id, false));
      None
  }
  ```
  ```rust
  // fragment.rs:710-712 — MoveTo's sibling receiver resolves via resolve_object (alias-aware)
  let moved_entity =
      resolve_object(vmad, world, context, moved, &deferred.scene_actor_bindings)?;
  ```
  ```
  $ grep -rln "ReferenceEnableState" --include="*.rs" . | grep -v test
  byroredux/src/save_io.rs
  crates/scripting/src/lib.rs
  crates/scripting/src/fragment.rs
  ```
- **Impact**: Two independent, additive gaps in one new effect. Even once
  (a) is fixed with a real runtime consumer, (b) means every alias-bound
  `Disable()` call — plausibly the majority of authored uses, since a
  script typically disables a reference it reached through an
  `ObjectReference Property` filled by the same quest-alias mechanism
  `AddItem`/`MoveTo` rely on — will continue to silently decline at
  dispatch, contributing nothing, while its sibling effects on the same
  receiver succeed.
- **Related**: Same root commit as the confirmed-open `ReferenceEnableState`
  gap the skill file itself already documents as a known future-phase item;
  this finding adds the receiver-resolution asymmetry, which the skill's
  checklist explicitly asked this pass to determine ("verify whether this is
  a deliberate narrower scope... or an inconsistency") — determined: it is
  an inconsistency, not a documented deliberate narrowing.
- **Suggested Fix**: (a) Give `Disable`/`Enable` (and whatever future effect
  reads `ReferenceEnableState`) a real consumer — the natural site is
  wherever cell-loader/streaming decides per-REFR visibility/collidability
  at spawn and on state change. (b) Route `Effect::Disable`'s receiver
  through `resolve_object` (alias-aware) instead of
  `resolve_property_form_id`, matching its sibling object-targeting effects,
  unless a reason emerges to keep it narrower — in which case document that
  reason at the call site so the next reader doesn't have to re-derive it.

### LOW

#### SCR-D5-2026-08-24-02: `Effect::Conditional`'s `lower_statements` recursion has no explicit depth cap, unlike every sibling recursive pass in this domain

- **Severity**: LOW
- **Dimension**: Recognizer-Chain Soundness (Dimension 5)
- **Untrusted-Input**: Yes (in the sense that a `.pex`-sourced nested-`If`
  chain reaches this code; see the bound analysis below for why this is not
  rated higher)
- **Location**: `crates/scripting/src/translate/effects.rs:301-387`
  (`lower_statements`, the `Stmt::If` arm's `then_effects`/`else_effects`
  recursive calls at `:358`/`:361`)
- **Status**: NEW
- **Description**: The new (2026-08-24) `Effect::Conditional` lowering path
  recurses into `lower_statements` once per level of nested `If` the source
  AST contains, with no local depth counter or cap of its own — unlike
  every other recursive pass in this domain, each of which carries an
  explicit, tested cap: `crates/pex/src/decompile/control_flow.rs` and
  `boolean.rs` both thread `MAX_REBUILD_DEPTH = 1024`;
  `crates/papyrus/src/parser/expr.rs`/`stmt.rs` carry
  `MAX_EXPR_DEPTH`/`MAX_STMT_DEPTH = 256`. This recursion is **not**
  independently unbounded in practice: for `.psc`-sourced input the AST it
  walks was itself built by the parser under `MAX_STMT_DEPTH = 256`; for
  `.pex`-sourced input (the path this new feature actually targets — a
  fragment lowered from a decompiled quest script), the nested-`If` shape
  of the AST `lower_statements` walks was itself produced by
  `control_flow.rs`'s reconstruction, which is capped at
  `MAX_REBUILD_DEPTH = 1024`. So this recursion is transitively bounded by
  two already-tested upstream caps in a different crate, in both of its
  reachable input paths — it is not rated MEDIUM/HIGH because a stack
  overflow here would require a `.pex` file that already maximizes a cap
  the decompiler itself enforces and tests, not an independent unbounded
  input.
- **Evidence**: `grep -n "depth" crates/scripting/src/translate/effects.rs`
  returns nothing in `lower_statements` or the functions it calls; no
  `MAX_*_DEPTH` constant exists in this file.
- **Impact**: A defense-in-depth gap, not an independently exploitable one.
  If either upstream cap is ever loosened without re-deriving whether
  `lower_statements`'s own stack budget still holds, this becomes the
  first place that finds out — silently, via a crash, rather than via a
  bounds-checked `Err`.
- **Related**: No existing issue. Distinct from the `MAX_CASCADE=64`
  cascade-queue bound (a different, already-guarded recursion class — see
  the Runtime Lifecycle matrix above).
- **Suggested Fix**: Thread an explicit `depth: u32` parameter through
  `lower_statements` (mirroring `stmt_depth`'s pattern in
  `crates/papyrus/src/parser/stmt.rs`) capped at a value at or below the
  smaller of the two upstream caps, returning `None` (decline) past it,
  plus a regression test analogous to
  `stmt_depth_cap_rejects_pathological_nested_if`. Low effort, and it
  converts an implicit, cross-crate invariant into a local, self-documenting
  one.

## Existing / correctly-tracked — NOT re-filed

Verified still open and still accurate against current code (no commit
since 2026-08-20 touches any of these):

- **SCR-D6-2026-08-20-01** (`HasPerk` reads a component the player never
  gets and only FO4+ NPCs ever get) — re-verified: `grep -n "Perks"
  byroredux/src/scene.rs` finds nothing on the player-entity construction
  path; `NpcRecord::perks` is still gated behind
  `uses_actor_value_properties()` (FO4/FO76/Starfield only).
- **SCR-D5-2026-08-20-01** (no `Lock`/`Unlock`/`SetLockLevel` effect
  primitive; `1e9723ab`'s `Locked` marker has no clearing path) —
  re-verified: `grep -n "prim_lock\|SetLockLevel\|Effect::.*Lock"
  crates/scripting/src/translate/effects.rs` returns nothing.
- **SCR-D7-2026-08-20-01** (`m47-triggers.sh`'s recognition/trigger counts
  are SOFT-only, so a script-attach regression cannot fail the gate) —
  `docs/smoke-tests/m47-triggers.sh` has no commits since 2026-08-20;
  re-read lines 130-168, the WARN-only structure is unchanged.
- **SCR-D7-2026-08-20-02** (`#3010`'s fix added a second
  `populate_quest_fragments` call site rather than consolidating, with no
  source pin) — `byroredux/src/cell_loader/exterior.rs` has no commits
  touching this call site since 2026-08-20.
- **#3014** (`crates/hkx`'s asset test passes vacuously via a bare `return`)
  — `crates/hkx/` has zero commits since 2026-08-19.
- **#3019** (`decompile/mod.rs`'s pipeline docstring names the wrong pass
  order) — file unchanged.
- **#2671, #2672, #2289, #2290, #2540, #2541, #2542, #2668, #2669, #2670,
  #2267, #2153, #2270** — all carried from 2026-08-20 on the same basis
  (no commit touches their sites).

## Considered and disproved / dropped

- **"The multi-`Fragment_N`-per-stage merge (`cee35507`) silently drops
  effects when one of several bindings for a stage fails to lower, which
  is a decline-invariant violation."** Investigated and disproved on
  reflection: each `Fragment_N` binding is an independently-invoked script
  method in the real Bethesda VM (multiple `QSDT` log entries genuinely
  fire independently when a stage is set), not sequential statements of one
  program — so a per-binding decline that still runs the siblings that
  *do* fully lower is the correct decline granularity, matching real
  runtime semantics more closely than declining the whole stage over one
  unmodeled fragment would. The invariant this domain protects is "don't
  partially lower one program's statement sequence," not "require every
  independent script bound to one event to be modeled." Not filed.
- **"`actor_quest_trigger_is_in_sequence` and
  `scene_trigger_actor_approach_system` disagree on the running-scene
  case, so a horse can be routed toward a trigger the gate then refuses."**
  Partially true (the gate's `target_stage <= awaited_stage` is more
  permissive than the router's `target_stage == awaited_stage`) but the
  disagreement is one-directional and safe: every trigger the router
  selects satisfies the gate's condition (equality implies `<=`), so
  "routed then refused" cannot occur from this asymmetry. The reverse
  (gate permits a trigger the router wouldn't route to) is harmless — it
  only matters for triggers reached by means other than this specific
  routing system, and the gate's job is to govern firing regardless of how
  an actor arrived. Not filed.
- **"A cinematic-retained cart/horse loses `CellRoot` and therefore
  silently drops out of `CellRootRefIndex`/`PersistentRefIndex` FormID
  lookups after crossing a cell boundary."** Investigated:
  `byroredux/src/cell_loader/form_id_root_index.rs`'s `rebuild()` does
  exclude any entity without a `CellRoot` matching the queried root — but
  both index types are explicitly root-*scoped* by their own module
  docstring, so an entity that has genuinely left every root's ownership
  (which is exactly what `cinematic_retained_entities` + the `CellRoot`
  strip is for) correctly falling outside every root's index is consistent
  with the documented contract, not a violation of it. Whether some
  *other*, not-yet-found consumer wrongly assumes universal `CellRoot`
  presence remains an open question this pass did not have budget to chase
  further — flagged here for a future pass rather than filed speculatively.

## Future-Phase Readiness

- **The `Disable`/`ReferenceEnableState`/`SetGlobalValue`/`Conditional`
  slice landed with solid decline discipline but a thinner cross-check
  than the domain's older primitives** — `AddItem`/`MoveTo`'s alias-aware
  resolution and `SetStage`'s cascade/lock discipline were each stress-
  tested against several adjacent invariants before landing; `Disable`
  shipped without its receiver-resolution choice being checked against its
  own siblings. This is the same class of gap SCR-D5-2026-08-20-01 named
  for the missing `Lock` primitive — new effects are landing faster than
  the cross-effect consistency sweep is keeping up.
- **`QuestStageAdvancedBatch` now has five same-frame writers to one
  entity** (`quest_advance_system`, `quest_alias_readiness_stage_system`,
  `scene_fragment_dispatch_system`, `quest_fragment_dispatch_system`,
  `fragment_continuation_system`), four of which independently reinvented
  the same "check-then-extend" defensive pattern by convention rather than
  by a shared helper. SCR-D6-2026-08-24-01 is the one that didn't; the
  next new producer is equally likely to miss it. A single
  `push_quest_stage_advances(world, player, advances)` helper in
  `quest_stages.rs` used by all five call sites would make the invariant
  structural instead of conventional.
- **The build-break and the recursive-read-guard finding (both
  cross-referenced, not duplicated, from today's safety and ECS audits)
  are worth reading together**: both are symptoms of the same session
  landing six scripting-domain commits in one day without the workspace
  compiling clean or the new ECS lock diagnostic being consulted before
  commit. Neither is a logic defect in the new runtime itself — `cargo
  test -p byroredux-scripting --lib` passes 311/311 — but both mean
  today's session-close should not report "clean" without naming them.

## Findings Count

**3 new: 0 CRITICAL / 0 HIGH / 2 MEDIUM / 1 LOW.**

By dimension — **Dim 1** (`.pex` reader & opcode decode): 0 (unchanged,
verified clean). **Dim 2** (decompiler CFG & lift): 0 (unchanged). **Dim 3**
(control-flow / boolean / lower): 0 (unchanged). **Dim 4** (`.psc` lexer &
Pratt parser): 0 (unchanged). **Dim 5** (recognizer-chain soundness): 1
MEDIUM + 1 LOW. **Dim 6** (scripting runtime systems): 1 MEDIUM. **Dim 7**
(engine attach & trigger wiring): 0 new (2 existing carried forward,
unchanged, still open). **Dim 8** (Havok idle / cinematic slice): 0 new
(`scene_trigger_actor_approach_system` and `cinematic_retained_entities`
both verified correct against their checklists).

Cross-referenced, not counted in the tally above (filed today by sibling
audits, verified accurate, not duplicated): the `cargo test --workspace`
build break (**SAFE-BUILD-2026-08-24-01**, HIGH, `AUDIT_SAFETY_2026-08-24.md`)
and the double-`Transform`-guard recursive read in `fragment.rs`
(**ECS-2026-08-24-01**, MEDIUM, `AUDIT_ECS_2026-08-24.md`).

**Untrusted-input robustness verdict**: unchanged, CLEAN for `.pex`, `.psc`
and `.hkx` — no source in the untrusted-input-facing crates changed this
pass. The one genuinely new untrusted-input-adjacent item
(SCR-D5-2026-08-24-02, the `Effect::Conditional` recursion) is transitively
bounded by two already-tested upstream caps and rated LOW, not a robustness
regression.

TALLY: CRITICAL=0 HIGH=0 MEDIUM=2 LOW=1
(cross-referenced elsewhere, not double-counted: HIGH=1 [SAFE-BUILD-2026-08-24-01], MEDIUM=1 [ECS-2026-08-24-01])
