# Concurrency and Synchronization Audit — 2026-08-27b

**Scope**: Full comprehensive run, all 7 dimensions, `--depth deep` (default).
No `--focus` filter. Run as part of the `comprehensive` audit-suite preset.

**Audited at**: `HEAD = 969d81c8`.

**Filename note**: `docs/audits/AUDIT_CONCURRENCY_2026-08-27.md` already exists —
a **Dimension-7-only** report from the earlier same-day `streaming-deep` preset
run. This report uses the repo's same-day `b`-suffix sibling convention
(cf. `AUDIT_RENDERER_2026-08-12b.md`) so that report is preserved intact. Its
three findings are reconciled below rather than re-filed.

**Method**: static analysis only. No engine process was launched (the user may
have a live instance — see the "no parallel engine launch" standing rule). One
build-side check was run: `cargo test -p byroredux --bin byroredux
scheduler_access` (15/15 pass — Dimension 4's regression gate).

**Dedup baseline**: `gh` could not reach `api.github.com` from this environment
(`error connecting to api.github.com`). Dedup ran against the cached
`/tmp/audit/issues.json` written at 23:23 today (400 issues, 99 OPEN / 301
CLOSED) plus every `docs/audits/AUDIT_CONCURRENCY_*.md` and today's sibling
`AUDIT_ECS_2026-08-27.md`. The cache is same-day, so staleness risk is low, but
issue states asserted below were additionally cross-checked against `git log`.

---

## Executive Summary

**Total: 4 findings — 0 CRITICAL, 1 HIGH, 2 MEDIUM, 1 LOW** (all NEW).

| # | ID | Severity | Dimension | Status | One-line |
|---|----|----|----|----|----|
| 1 | CONC-D3-2026-08-27b-01 | HIGH | ECS Lock Ordering | NEW | Live `ActorValues ↔ CharacterRuleset` lock-order cycle: `condition.rs`'s `GetActorValue` arm holds an `ActorValues` read guard across `try_resource::<CharacterRuleset>()`, the exact reverse of `pool_regen_tick_system` (whose `ActorValues` side is a **write**) and `melee_damage_charal_bonus` |
| 2 | CONC-D3-2026-08-27b-02 | MEDIUM | ECS Lock Ordering | NEW | #2153's hold-stack reduction is inert — `let config = *config;` **shadows but does not drop** the `PoolRegenConfig` guard, so `pool_regen_tick_system`'s stack is still 3-deep and its own comment asserts a drop Rust never performs |
| 3 | CONC-D3-2026-08-27b-03 | MEDIUM | ECS Lock Ordering | NEW | `studio_host::snapshot` inverts the canonical order's `Name → StringPool` tail — it takes `StringPool` first and then `Transform` / `Name` / `Material` per entity, closing a 2-cycle against `resolve_entity_name` and the debug evaluator |
| 4 | CONC-D3-2026-08-27b-04 | LOW | ECS Lock Ordering | NEW | `cinematic_animation_event_system` is a second `StringPool`-before-storage site (no reverse edge in-tree, so latent) — together with #3 it turns `StringPool` from a graph sink into a mid-graph node |

**Dimensions 1, 2, 4, 5, 6 and 7 came back clean** (0 new findings). Their
checklist-verification trails are below; each was re-derived from current
line-numbered code rather than trusted from the 2026-08-24 report.

No Vulkan-sync (GPU-side `sync`) findings were produced, so the
speculative-fix guardrail did not need to be exercised. All four findings are
CPU-side (`concurrency`) and provable from source alone.

**Notable correction to a same-day sibling report**: `AUDIT_ECS_2026-08-27.md`
(finding ECS-2026-08-27-04) states of the `CharacterRuleset → ActorValues`
hold order that *"no reverse edge exists in-tree today"*. Finding #1 below
shows that premise is false — the reverse edge has been in
`crates/scripting/src/condition.rs` since `2b9147ae` (2026-07-01), predating
the combat site that report examined. This is the audit-finding-hygiene rule
(verify the premise against current code) working in the direction it was
written for.

---

## Prior-report reconciliation

### `AUDIT_CONCURRENCY_2026-08-27.md` (Dimension 7 only, earlier today)

| Prior finding | Issue | State now |
|---|---|---|
| CONC-D7-2026-08-27-01 — preserved persistent-CELL root abandons its in-flight `PersistentCellApplyJob` | #3376 | **CLOSED / fix verified.** `40eb5d3a` added the `state.persistent_apply.is_some()` argument at `byroredux/src/app_step.rs:827-836` and the matching `in_flight` parameter + fail-safe in `persistent_root_survives_crossing` (`byroredux/src/cell_loader/exterior.rs:466-495`). |
| CONC-D7-2026-08-27-02 — `PersistentCellApplyJob` has no `cancel` | #3377 | **STILL OPEN, re-verified.** `cancel_active_streaming_apply` (`byroredux/src/streaming_helpers.rs:528-537`) still takes only `state.active_apply`; `drain_streaming_state` (`:387-397`) calls it and then drops the moved-out `state`, taking `persistent_apply` with it uncancelled. Not re-filed. |
| CONC-D7-2026-08-27-03 — `build_stream_parse_pool`'s "reserving half" rationale is false | #3378 | **STILL OPEN.** Doc-only. Not re-filed. |

### `AUDIT_CONCURRENCY_2026-08-24.md` (full sweep, prior comprehensive run)

| Prior finding | Issue | State now |
|---|---|---|
| CONC-D2-2026-08-24-01 — `MorphSlot::weight_buffer` host write races the previous frame's `skin_vertices.comp` read | #3244 (**still OPEN on tracker**) | **FIX HAS LANDED** — see Process Notes. `MorphSlot` now stages weights CPU-side (`crates/renderer/src/vulkan/morph_compute.rs:51-53, 173-202`) and `draw_frame` publishes them via `flush_pending_morph_weights` (`context/draw.rs:1506-1511`) called at `:1640`, i.e. *after* the dual-fence wait at `:1624-1636`. `morph_compute.rs:220-229` asserts that ordering against the `draw_frame` source. |
| CONC-D3-2026-08-24-01 — live 3-edge `Transform→GlobalTransform→CharacterController→Transform` cycle | #3260 | **CLOSED / fix verified.** `camera_follow_system` (`byroredux/src/systems/character.rs:541-561`) now scopes `GlobalTransform` and `CharacterController` in separate blocks; regression test `camera_follow_does_not_close_character_lock_cycle` at `:1124`. |
| CONC-D3-2026-08-24-02 — canonical order omits `CharacterController`/`RapierHandles` | #3261 | **CLOSED.** `docs/engine/ecs.md:602-626` now heads the order with `CharacterController → RapierHandles → Transform → …` and documents the `pull_dynamic` / `camera_follow` reasoning. |
| CONC-D3-2026-08-24-03 — `wander_system_inner` / `patrol_system_inner` skipped #2134's snapshot-before-`PhysicsWorld` | #3262 | **CLOSED / fix verified.** `byroredux/src/systems/wander.rs:249-357` is now Pass 1a (five storage reads, scoped) → Pass 1b (`PhysicsWorld` alone) → Pass 2 (scoped single-type writes). |
| CONC-D3-2026-08-24-05 — `weather_system`'s `WeatherDataRes`→`WeatherTransitionRes` hold undocumented | #3263 | **CLOSED.** |
| CONC-D3-2026-08-24-04 — resource accessors defuse the tracker scope before constructing the guard | #3264 | **CLOSED / fix verified.** All six accessors in `crates/core/src/ecs/world.rs` now bind `let resource = ResourceRead/Write::new(...)` *before* `scope.defuse()` (`:709-713`, `:739-743`, `:796-803`, `:810-817`, `:836-841`, `:857-862`). |
| CONC-D5-2026-08-24-01 — `pull_dynamic`'s stale lock-drop comment | #3130 | **CLOSED.** Comment at `crates/physics/src/sync.rs:1133-1140` now describes the drops that actually happen there. |
| CONC-D5-2026-08-24-02 — `player_water_state` re-locks `TotalTime`+`WindField` per plane | #3265 | **CLOSED / fix verified.** `byroredux/src/systems/character.rs:901-908` samples both once before any water storage guard; pinned by `player_water_state_samples_weather_once_before_water_queries` at `:1164`. |
| CONC-D5-2026-08-24-03 — physics diagnostics hold storage guards across `FormIdPool` | #3266 | **CLOSED / fix verified.** `dump_awake_fallers` drops `layer_q`/`form_q`/`physics_source_q` at `crates/physics/src/sync.rs:383-385` before `try_resource::<FormIdPool>()` at `:387`; same shape at `:672-676`. Pinned by `physics_diagnostics_resolve_forms_after_storage_guards_drop` (`:1999`). |
| CONC-D5-2026-08-24-04 — `physics_sync_system` re-entrant from 3 non-scheduler sites | #3267 | **CLOSED.** |
| (post-24 follow-on) `pull_dynamic` held `GlobalTransform`+`Transform` together | #3303 | **CLOSED / fix verified.** Split into two sequential passes at `crates/physics/src/sync.rs:1159-1229`; regression test `pull_dynamic_does_not_close_transform_global_transform_lock_cycle` (`:1476`). |
| ECS-2026-08-24-02 — #2386 recursive-read warning is unbounded / carries no call site | #3249 | **STILL OPEN.** `crates/core/src/ecs/lock_tracker.rs:98-106` still logs a bare type name once per 1→2 transition. Not re-filed. |
| CONC-2026-08-16-02 — a cancelled screenshot makes `DebugDrainSystem` skip that frame's whole drain | #3090 | **STILL OPEN, re-verified.** `crates/debug-server/src/system.rs:72-77` still `return`s out of `run()` on the cancel path, before the command drain. Not re-filed. |

---

## Findings

### CONC-D3-2026-08-27b-01: live `ActorValues ↔ CharacterRuleset` lock-order cycle — the CTDA `GetActorValue` arm is the reverse edge that `pool_regen_tick_system`'s and `melee_damage_charal_bonus`'s safety argument assumes does not exist

- **Severity**: HIGH
- **Dimension**: ECS Lock Ordering & Deadlock
- **Location**: forward edge `crates/scripting/src/condition.rs:470-509`; reverse edges `crates/core/src/character/regen.rs:176-180` and `byroredux/src/combat.rs:349-360`
- **Status**: NEW
- **Trigger Conditions**: Any session that evaluates a `GetActorValue` CTDA for an actor-value the subject does not carry (the arm falls through to the ruleset branch) **and** either runs `pool_regen_tick_system` with a live `PoolRegenConfig` (Oblivion) or resolves one melee swing through `melee_damage_charal_bonus` (FNV/FO3, `MeleeDamageConfig` + `CharacterRuleset` present). Both halves are ordinary gameplay: CTDA `GetActorValue` gates AI packages (`byroredux/src/npc_spawn/ai_package.rs:450`), quest stages, triggers and scenes; melee is the P2 vertical slice's core loop.
- **Verification Path**: Reproducible without a GPU. Run any FNV/FO3 cell with `BYRO_LOCK_ORDER_CHECK=1` (debug build — `global_order::record_and_check` is `#[cfg(debug_assertions)]`, `lock_tracker.rs:112-121`), let one AI-package CTDA evaluate `GetActorValue`, then swing at an NPC: the second-observed edge closes the cycle and `record_and_check` panics. The static `access_report` KPIs cannot see it — all the sites involved are `Stage::Update` **exclusives**, and exclusives are never paired by `analyze_pair`.
- **Description**: `evaluate_function`'s `GetActorValue` arm binds an `ActorValues` **storage read guard** and keeps it live across a `CharacterRuleset` **resource read**, because the guard is used again at the end of the arm:

  ```rust
  // crates/scripting/src/condition.rs:470-509
  let Some(avs) = world.get::<ActorValues>(entity) else {
      return 0.0; // no `ActorValues` → absent-AV default
  };
  if avs.get(condition.param_1).is_some() {
      return avs.current(condition.param_1);
  }
  …
  if let Some(rs) = world.try_resource::<CharacterRuleset>() {     // ← ActorValues → CharacterRuleset
      if let Some(formula) = rs.derived_formula(condition.param_1) {
          …
          let level = world.get::<CharacterLevel>(entity).map_or(0, |l| l.level);
          return rs
              .derived_value(condition.param_1, &avs, level)        // ← `avs` still live
              .unwrap_or(0.0);
  ```

  Two in-tree sites acquire the same pair in the opposite order, and one of them takes `ActorValues` for **write**:

  ```rust
  // crates/core/src/character/regen.rs:176-180
  let Some(ruleset) = world.try_resource::<CharacterRuleset>() else { return; };
  let Some(mut avs_q) = world.query_mut::<ActorValues>() else { return; };   // ← CharacterRuleset → ActorValues (W)
  ```

  ```rust
  // byroredux/src/combat.rs:353-357
  let Some(ruleset) = world.try_resource::<CharacterRuleset>() else { return 0.0; };
  let Some(avs) = world.get::<ActorValues>(aggressor) else { return 0.0; };  // ← CharacterRuleset → ActorValues (R)
  ```

  `lock_tracker` keys one thread-local `LOCKS` map and one global `GRAPH` by `TypeId` for **both** storages and resources (`crates/core/src/ecs/lock_tracker.rs:69-73`, `:112-121`), so this is a genuine 2-cycle in the detector's graph, not a category confusion.

  The reason this matters beyond the detector is that both reverse-edge sites document their own correctness as resting on the *absence* of this edge. `regen.rs:156-164` says the 3-deep stack's "only correctness argument was 'this system is registered exclusive'"; today's `AUDIT_ECS_2026-08-27.md` (ECS-2026-08-27-04) states outright that "the `CharacterRuleset → ActorValues` ordering also matches `pool_regen_tick_system`'s, so **no reverse edge exists in-tree today**". That premise is what makes a future move of `combat_damage_system`, `pool_regen_tick_system` or any condition-evaluating dispatcher into a parallel lane look like a one-line change. It is not.
- **Evidence**: see the three snippets above; blame confirms `condition.rs`'s edge landed in `2b9147ae` (2026-07-01) and `combat.rs`'s in `08434727` (2026-08-19), so the cycle has been live for eight days and predates the report that cleared it.
- **Impact**: No live deadlock **today** — every site (`pool_regen_tick_system` `boot.rs:993-1001`, `combat_damage_system` `boot.rs:856`, and every condition-evaluating dispatcher: `trigger_detection_dispatch` `:877`, `quest_advance_dispatch` `:885`, `scene_playback_system` `:905`, `ambient_ai_package_system` `:916`) is a `Stage::Update` exclusive on the main thread, and an ABBA deadlock needs two threads. The concrete damage is (a) any `BYRO_LOCK_ORDER_CHECK=1` FNV/FO3/Oblivion session aborts once both edges are observed — the same failure mode as the closed #3260 — and (b) the invariant that would make a future parallelisation safe is already broken, silently, with a same-day report on record asserting the opposite.
- **Related**: #2153 (the original 3-deep stack), #2391 / ECS-D5B-03 (`add_exclusive_with_access` remedy), #3260 (identical class, rated HIGH), #2270 (`world.rs` "snapshot before you iterate" house rule), ECS-2026-08-27-04 in `AUDIT_ECS_2026-08-27.md` (the falsified premise), and finding CONC-D3-2026-08-27b-02 below (the same function's inert guard drop).
- **Suggested Fix**: Break the edge at the `condition.rs` end, where it is cheapest and where the guard is only read: snapshot what the arm needs out of `ActorValues` before touching the ruleset. `ActorValues::get`/`current` already return `Copy` scalars, so the only real dependency is `derived_value(&avs, …)` — give it an owned clone (or restructure to compute `derived_value` from a copied SPECIAL/skill slice). Then pick the surviving direction (`CharacterRuleset → ActorValues`, matching `regen.rs` and `combat.rs`) and add it to the canonical acquisition order in `docs/engine/ecs.md:602-605`, which currently names neither type — the same gap #3261 closed for `CharacterController`/`RapierHandles`.

---

### CONC-D3-2026-08-27b-02: #2153's hold-stack reduction never happened — `let config = *config;` shadows but does not drop the `PoolRegenConfig` guard

- **Severity**: MEDIUM
- **Dimension**: ECS Lock Ordering & Deadlock
- **Location**: `crates/core/src/character/regen.rs:153-180`
- **Status**: NEW
- **Trigger Conditions**: Every `pool_regen_tick_system` tick with a live `PoolRegenConfig` (Oblivion wiring). The defect is unconditional; only its *observability* needs `BYRO_LOCK_ORDER_CHECK=1`.
- **Verification Path**: Source-only. Rust's drop semantics are the whole argument: `let config = *config;` introduces a *new* binding that shadows the old one; the shadowed `ResourceRead<PoolRegenConfig>` is neither moved nor dropped at that point, so its `Drop` (which is what calls `lock_tracker::untrack`) runs at end of function scope. A regression test can assert this directly by checking `lock_tracker` held-state after the shadowing line, or by source-asserting on an explicit `drop(...)` the way `physics_diagnostics_resolve_forms_after_storage_guards_drop` (`crates/physics/src/sync.rs:1999`) already does for the sibling discipline.
- **Description**: #2153 (filed as CONC-2026-07-25 in `AUDIT_CONCURRENCY_2026-07-25.md:280-285`) asked for the `PoolRegenConfig` guard to be dropped before the `CharacterRuleset` acquire, reducing the hold-stack from 3 to 2. The implementation used shadowing, and the accompanying comment states the outcome as fact:

  ```rust
  // crates/core/src/character/regen.rs:153-180
  let Some(config) = world.try_resource::<PoolRegenConfig>() else { return; };
  // Copy out and drop the guard immediately (#2153) — `PoolRegenConfig` is
  // `Copy`, so nothing downstream needs the resource lock itself, only its
  // three AVIF ids. Holding it across the `CharacterRuleset` acquire below
  // built a 3-deep stack (`PoolRegenConfig` -> `CharacterRuleset` ->
  // `ActorValues`) whose only correctness argument was "this system is
  // registered exclusive" — true today, but unstated here and not enforced
  // by the lock order itself. Dropping it here reduces the hold-stack to 2
  // for the rest of the function, matching how `accumulator` is already
  // dropped before `elapsed` is used.
  let config = *config;
  ```

  Contrast the immediately following `accumulator`, which the same comment cites as the model and which *does* use an explicit `drop(accumulator);` at `:171`. The `config` guard gets no such call, so the stack at `query_mut::<ActorValues>()` (`:179`) is still `{PoolRegenConfig(R), CharacterRuleset(R), ActorValues(W)}` — exactly what #2153 was filed against. The identical shadowing mistake exists at `byroredux/src/combat.rs:352` and is correctly identified there by today's `AUDIT_ECS_2026-08-27.md`; what is new here is that the *canonical fix site* has the same bug **plus** a comment asserting it doesn't.
- **Evidence**: the snippet above, and `crates/core/src/character/regen.rs:170-171`:
  ```rust
  let ticks = accumulator.advance(frame_dt);
  drop(accumulator);
  ```
  — the explicit `drop` the `config` half was supposed to mirror.
- **Impact**: The hold-stack #2153 was filed to shrink is unchanged, so the risk #2153 described is unmitigated; worse, a reader (or auditor) who trusts the comment will conclude the site is clean. Combined with finding #1 the stack now sits on one leg of a real cycle. A stale comment that asserts a *safety property* is materially worse than no comment — this is the doc-rot class that `_audit-common.md`'s finding-hygiene rule exists for.
- **Related**: #2153, CONC-D3-2026-08-27b-01, ECS-2026-08-27-04 (`AUDIT_ECS_2026-08-27.md`, the combat.rs instance of the same shadowing mistake).
- **Suggested Fix**: One line — `let config = *config; drop(…)` cannot name the shadowed binding, so rename: `let config_guard = world.try_resource::<PoolRegenConfig>()…; let config = *config_guard; drop(config_guard);`. Apply the same rename+drop at `byroredux/src/combat.rs:351-352`. Then pin it with a source-assert test in `regen.rs`'s existing test module (it already source-asserts on `"try_resource::<CharacterRuleset>"` at `:325`, so the harness is there).

---

### CONC-D3-2026-08-27b-03: `studio_host::snapshot` inverts the canonical order's `Name → StringPool` tail, closing a 2-cycle against `resolve_entity_name` and the debug evaluator

- **Severity**: MEDIUM
- **Dimension**: ECS Lock Ordering & Deadlock
- **Location**: `byroredux/src/studio_host.rs:11-48`; reverse edges at `byroredux/src/commands/shared.rs:35-40`, `byroredux/src/commands/assets.rs:591-608`, `crates/debug-server/src/evaluator.rs:346-353` and `:877-881`
- **Status**: NEW
- **Trigger Conditions**: A `--studio <mesh>.nif` session (`byroredux/src/scene.rs:695`, `:931`) with the debug-UI panel snapshot running — `build_panel_snapshot` calls `studio_host::snapshot` unconditionally (`byroredux/src/main.rs:725`) and the function only short-circuits if `StudioSession` is absent — plus any console command that resolves an entity name (`prid`, `entities`, `skin.list`, the debug server's `EntityList`).
- **Verification Path**: Source-only for the ordering; observable as a `BYRO_LOCK_ORDER_CHECK=1` abort in a debug `--studio` run that also issues one name-resolving console command.
- **Description**: `docs/engine/ecs.md:602-605` fixes one process-wide order for the hierarchy/skinning/naming cluster, ending `… → Name → StringPool`. `studio_host::snapshot` acquires `StringPool` **first** and then walks storages beneath it:

  ```rust
  // byroredux/src/studio_host.rs:11-24
  pub(crate) fn snapshot(world: &World) -> Option<StudioSnapshot> {
      let session = world.try_resource::<StudioSession>()?.clone();
      let pool = world.try_resource::<StringPool>();          // ← guard held for the whole walk
      let objects = session.objects.iter().filter_map(|&entity| {
          let transform = world.get::<Transform>(entity)?;    // ← StringPool → Transform
          …
          let name = world
              .get::<byroredux_core::ecs::Name>(entity)       // ← StringPool → Name
              .and_then(|name| pool.as_ref().and_then(|pool| pool.resolve(name.0)))
  ```

  `World::get` takes a tracked storage read lock (`crates/core/src/ecs/world.rs:358-376`), so these are real edges. The established reverse edges are explicit and commented:

  ```rust
  // byroredux/src/commands/shared.rs:35-40
  let name_q = world.query::<Name>()?;
  let name = name_q.get(entity)?;
  let pool = world.try_resource::<StringPool>()?;             // ← Name → StringPool
  ```
  ```rust
  // byroredux/src/commands/assets.rs:594-597
  // Name before StringPool — matches `resolve_entity_name`'s order
  // for this pair (#313).
  let name_q = world.query::<Name>();
  let pool = world.try_resource::<StringPool>();
  ```
  `crates/debug-server/src/evaluator.rs:346-353` additionally establishes `Transform → … → StringPool`, and that crate carries a dedicated source-assert regression test (`debug_evaluator_acquires_locks_in_canonical_order`, `:899`) added under #2388 for precisely this violation.

  The sibling function in the same snapshot bridge gets it right and says why:

  ```rust
  // byroredux/src/inventory.rs:335-341
  // Clone each component before acquiring the next storage lock. The
  // menu is off the hot path, and this preserves the ECS invariant that
  // callers never hold independently-acquired component locks in an
  // arbitrary order.
  let inventory = (*world.get::<Inventory>(player)?).clone();
  ```
  `studio_host.rs` is the newer file and did not inherit that discipline.
- **Evidence**: the four snippets above.
- **Impact**: No live deadlock — the panel snapshot runs on the main thread in the frame loop and console commands run under the `Stage::Late` exclusive `DebugDrainSystem`, so the two orders are never concurrent. The damage is the detector abort plus the loss of `StringPool`'s sink property (see finding #4). Reachability is gated on `--studio`, which is why this is MEDIUM rather than HIGH.
- **Related**: #313 and #2388 (the canonical order and the last time this exact pair was inverted), #3261 (canonical-order doc completeness), CONC-D3-2026-08-27b-04.
- **Suggested Fix**: Move the `StringPool` acquisition inside the per-entity closure, *after* the `Name` read, mirroring `resolve_entity_name` — or better, resolve names into an owned `Vec<(EntityId, String)>` up front under `Name → StringPool` and drop both guards before the `Transform`/`Material` walk, mirroring `inventory::snapshot`. Consider extending `debug-server`'s `debug_evaluator_acquires_locks_in_canonical_order` pattern to a shared source-assert covering `studio_host.rs`, since this is now the second recurrence.

---

### CONC-D3-2026-08-27b-04: `cinematic_animation_event_system` is a second `StringPool`-before-storage site, demoting `StringPool` from a lock-order sink to a mid-graph node

- **Severity**: LOW
- **Dimension**: ECS Lock Ordering & Deadlock
- **Location**: `byroredux/src/systems/cinematic.rs:145-163`
- **Status**: NEW
- **Trigger Conditions**: Every frame in which any entity carries `AnimationTextKeyEvents` (the M47.2 cinematic slice, MQ101). Latent only — no in-tree site acquires `AnimationTextKeyEvents` before `StringPool` today (`byroredux/src/systems/animation.rs:632-658` and `:932-936` take `AnimationTextKeyEvents` under the `AnimationClipRegistry` guard, never `StringPool`).
- **Verification Path**: Source-only. It would become a detector abort — and, if either side moved to a parallel lane, a real hang — the moment any code path reads a `Name`/`AnimationTextKeyEvents` pair in the other order.
- **Description**:
  ```rust
  // byroredux/src/systems/cinematic.rs:145-152
  pub(crate) fn cinematic_animation_event_system(world: &World, _dt: f32) {
      let deliveries: Vec<(EntityId, CinematicAnimationEvent)> = {
          let Some(pool) = world.try_resource::<StringPool>() else { return; };
          let Some(event_query) = world.query::<AnimationTextKeyEvents>() else { return; };
  ```
  `StringPool` sits at the tail of `docs/engine/ecs.md:602-605`'s canonical order precisely so that no lock is ever taken beneath it. Six in-tree sites respect that (`commands/shared.rs:38`, `commands/assets.rs:597`, `debug-server/src/evaluator.rs:270`, `:353`, `:511`, `:879`); this one and finding #3's `studio_host.rs:13` do not. The system's exclusivity (`add_exclusive_with_access`, `byroredux/src/boot.rs:1057-1068`) is the whole safety argument, and — unlike `pool_regen_tick_system`, whose #2391 declaration exists specifically to surface such a contract — nothing at this site says so.
- **Evidence**: the snippet above, plus the six correctly-ordered sites listed.
- **Impact**: None today. The cost is that the "`StringPool` is a sink" invariant, which is what makes the canonical order's tail cheap to reason about, is no longer true; two independent sites now record edges out of it, and a third would only need to be a parallel system to matter.
- **Related**: CONC-D3-2026-08-27b-03 (same shape, reachable cycle), #313, #2388, #2391.
- **Suggested Fix**: Reorder to `AnimationTextKeyEvents` then `StringPool` (both are reads; nothing in the block needs the pool before the query), and add `StringPool`'s sink property as an explicit sentence in `docs/engine/ecs.md`'s canonical-order section so the next site has something to violate visibly.

---

## Dimension-by-dimension verification trail

### Dimension 1: Vulkan Queue & Acceleration-Structure Sync — **0 findings**

Scope re-derived from `crates/renderer/src/vulkan/context/{draw,resize,teardown,skinned_blas_refit}.rs`, `sync.rs`, `texture.rs`, `acceleration/{blas_static,blas_skinned,tlas,memory,mod}.rs`, `mesh.rs`.

- **Single-Mutex queue submission** — `graphics_queue`/`present_queue` are both `Arc<Mutex<vk::Queue>>` (`context/mod.rs:1814`, `:1820`) and `present_queue` is `Arc::clone(&graphics_queue)` when the families match (`context/init.rs:125-126`). Both call sites bind the `MutexGuard` (not `*queue.lock()`) so it spans the call: `context/draw.rs:3824-3852` (submit, with `drop(queue)` before the fence-free error recovery) and `:3934-3944` (present, guard scoped to the `unsafe` block). The one-time path in `texture.rs:815-818` scopes the guard to the submit only and explicitly does **not** hold it across `wait_for_fences` at `:826` — the #1713 discipline, with its rationale intact at `:801-814`.
- **Frame-in-flight discipline** — dual-fence wait (`in_flight[frame]` + `in_flight[prev]`) at `draw.rs:1624-1636`, before any per-frame resource reuse; `reset_fences` sits immediately before `queue_submit` (`:3795-3800`, #952) rather than 2200 lines earlier. `draw_frame` returns *before* `acquire_next_image` on the early-out paths (`:1590`) so `image_available[frame]` is never left signal-pending. Failure recovery recreates both the acquire semaphore and the fence (`:3837-3850`).
- **AS build → read barrier** — static BLAS `WRITE→READ` at `acceleration/blas_static.rs:604-611`; skinned-refit scratch-serialize barrier's dst mask is still `WRITE|READ` (`blas_skinned.rs:692-700`), the #1790 regression guard; TLAS at `acceleration/tlas.rs:200-247`; refit→TLAS handoff at `skinned_blas_refit.rs:671-679`.
- **AS build INPUT barrier access flag (#507945d8)** — inputs still use `SHADER_READ` at `ACCELERATION_STRUCTURE_BUILD_KHR`, not `ACCELERATION_STRUCTURE_READ_KHR`: `tlas.rs:237-243` (instance-buffer copy) and `skinned_blas_refit.rs:484-487` (skin output). Rationale comment intact at `skinned_blas_refit.rs:44-52`.
- **Deferred destruction** — BLAS eviction routes through `pending_destroy_blas` (`blas_static.rs:1094`, `blas_skinned.rs:730`); scratch retirement through `pending_destroy_scratch` (`blas_static.rs:518`, `memory.rs:92`, `:113`), the #1782 guard; shutdown drains via `destroy()`→`drain_pending_destroys` (`acceleration/mod.rs:333-351`). The TLAS slot-resize path's immediate destroy at `tlas.rs:987-1004` is guarded by a defensive `device_wait_idle` at `:985` with the invariant spelled out at `:967-984`.
- **Swapchain recreate** — `context/resize.rs:487-760` rebuilds G-buffer, SVGF, ReSTIR reservoirs, caustic, water-caustic, bloom, composite and egui framebuffers; per-FIF slots iterated for every in-flight index.
- **One-time blocking submits in the frame path** — the #3298 chunked geometry rebuild (`crates/renderer/src/mesh.rs:1386`, `:1414` → `vulkan/buffer.rs::copy_bytes_range` → `with_one_time_commands`) now performs one submit + fence wait **per frame while a rebuild is in flight**, driven from `byroredux/src/app_frame.rs:205-215`. Traced and deliberately **not** filed: it runs outside `draw_frame`, on the main thread, holds the queue Mutex only across the submit, waits its own dedicated fence (not `queue_wait_idle`), and bounding that stall is the entire point of #3298. See "Candidates considered and NOT reported".

### Dimension 2: Compute → AS → Fragment Chains — **0 findings**

- **Skin chain** — palette build → `COMPUTE_SHADER_WRITE → SHADER_READ` (`draw.rs:2555-2580`) → skin output → `COMPUTE/SHADER_WRITE → (AS_BUILD|FRAGMENT)/SHADER_READ` (`skinned_blas_refit.rs:442-490`) → refit → `AS_BUILD_WRITE → AS_BUILD_READ` (`:671-679`) → TLAS → ray query. Intact end to end.
- **`MorphSlot` weight publish (#3244)** — the prior report's HIGH is fixed in code: `stage_weights` writes a CPU-side handoff (`morph_compute.rs:173-186`), `flush_pending_morph_weights` (`draw.rs:1506-1511`) publishes it, and it is called at `draw.rs:1640` — after the dual-fence wait at `:1624`. `morph_compute.rs:220-229` source-asserts the ordering.
- **Cross-frame ping-pong** — SVGF/TAA/caustic/water-caustic/volumetrics all index the previous frame's slot per FIF; verified via each pipeline's `mark_frame_completed` gating (`svgf.rs:1395`, `taa.rs:790`, `volumetrics.rs:2392`), all advanced only after `queue_submit` returns Ok (`draw.rs:3869-3885`).
- **Volumetrics gate (#1105)** — `tlas_written: [bool; MAX_FRAMES_IN_FLIGHT]` latch set in `write_tlas` (`volumetrics.rs:2650`), `debug_assert!`ed and reset in `dispatch` (`:2060-2066`). Symmetry intact.
- **`MaterialBuffer` SSBO** — upload still lands before draw recording; no compute-path move.

### Dimension 3: ECS Lock Ordering & Deadlock — **4 findings** (above)

Regression guards re-verified INTACT:
- **TypeId-sorted acquisition** — `world.rs`'s `if id_a < id_b { … } else { … }` branches present in the paired accessors, with the tracker scopes set up in the same order.
- **#2384 check-before-insert** — `global_order::record_and_check` runs on a `borrow()` snapshot at `lock_tracker.rs:112-121`, *before* `locks.borrow_mut().insert(...)` at `:123-130`.
- **#2385 `GRAPH` poison recovery** — every `GRAPH.read()`/`.write()` uses `unwrap_or_else(|poison| poison.into_inner())` (`lock_tracker.rs:364`, `:392`, `:449`, `:456`). No `.expect("GRAPH poisoned")` reintroduced.
- **#2386 recursive-read warning** — still a `log::warn!` on the 1→2 transition (`lock_tracker.rs:98-106`), not a panic. (Its unbounded/context-free shape remains open as #3249.)
- **Poisoning** — storage acquisitions resolve through `storage_lock_poisoned::<T>()` / `resource_lock_poisoned::<R>()`; no silent `unwrap()` of a poisoned guard found.
- **Canonical order spot-checks** — `make_transform_propagation_system` (`crates/core/src/ecs/systems.rs:78-84`: `Transform(W) → Parent → Children → GlobalTransform(W)`), `world_bound_propagation_system` (`byroredux/src/systems/bounds.rs:133-172`), `pull_dynamic`'s two-pass split (`crates/physics/src/sync.rs:1159-1229`), `character_controller_system`'s scoped `Transform` (`byroredux/src/systems/character.rs:192-217`), `ragdoll_writeback_system`'s long span (`byroredux/src/ragdoll.rs:488-505`) — all mutually consistent.

### Dimension 4: Scheduler Access Declarations — **0 findings**

- `cargo test -p byroredux --bin byroredux scheduler_access` → **15/15 pass**, including `build_scheduler_reports_zero_access_conflicts`, `scheduler_access_invariants_hold_on_the_real_schedule`, `contract_bearing_exclusives_declare_their_access` and `player_wind_read_is_declared_and_weather_writer_is_exclusive`.
- **Conflict model** — `AccessConflict` still has exactly `None` / `Unknown` / `Conflict`; no `Parallel` variant.
- **#3111 `WindField` pairing** — `weather_system` is still `add_exclusive_with_access(Stage::Early, …)` (`boot.rs:764-780`) declaring both read and write of `WindField`; `player_controller_system` declares the read (`:746`). Cross-stage sequencing hand-checked (the KPIs cannot see it): the writer is a `Stage::Early` exclusive and the `Stage::PostUpdate` billboard, `Stage::Physics` `physics_sync_system` and `Stage::Late` `submersion_system` readers all execute in later stages, so they read this frame's field. The one same-stage reader, `player_controller_system`, runs in Early's parallel phase *before* the exclusive phase and therefore sees the previous frame's field — one-frame latency, which is the documented intent ("the controller sees one stable snapshot", `boot.rs:761-763`), not a race.
- **Parallel batch census** — Early: `player_controller_system` + `timer_tick_system` (disjoint). Update: `make_animation_system()` alone. PostUpdate: `make_transform_propagation_system()` alone. Physics: `physics_sync_system` alone (grep confirms one `Stage::Physics` registration, `boot.rs:1274`). Late: `camera_follow_system`, `reverb_zone_system`, `log_stats_system`, `metrics_sample_system` — no overlapping declared lock with a writer.
- **Exclusive phase** — `audio_system` (Late) and `spin_system` (Update) still exclusive; `player_controller_system` remains the declared union of `fly_camera` + `character_controller`.

### Dimension 5: RwLock Patterns — Resource↔Storage & Physics Step — **0 findings**

- **4-phase `physics_sync_system`** — `collect_newcomers` (`crates/physics/src/sync.rs:807-869`) collects to a `Vec` under read guards that end with the function; `register_newcomers` takes `PhysicsWorld` at `:890` and `drop(pw)` at `:1019` with **no** storage acquisition inside that span (verified by grep over `:880-1019`); `RapierHandles` write follows at `:1035`.
- **Helper lock order** — `set_linear_velocity` (`:53-75`) and `set_kinematic_translation` (`:85-110`) both read `RapierHandles` in a `match` scrutinee that drops at the statement's `;` before `resource_mut::<PhysicsWorld>()`. Callers (`systems/cinematic.rs:138-140`) hold no `PhysicsWorld` guard at the call.
- **`ContactConfig`** — snapshotted once at `sync.rs:886-889`, outside the per-newcomer loop.
- **Cell-unload teardown (#1520)** — `byroredux/src/cell_loader/unload.rs:516-534` still collects handles under the read guard and releases before the `PhysicsWorld` mutation.
- **WATAL buoyancy** — `apply_buoyancy` (`crates/physics/src/water.rs:558-576`) `mem::take`s `WaterContactScratch` and drops the guard before the working pass; `apply_buoyancy_with_scratch` snapshots `PhysicsWaterConstants`/`TotalTime`/`WindField` as owned copies before any storage guard (`:601-615`), scopes its quiesced-scene `PhysicsWorld` read (`:640-654`), and drops `handles_q`/`body_q`/`contact_q` at `:703-705` before the write guard at `:716`. `clear_stale_water_contacts` (`:452-499`) follows the same shape.
- **Single-threaded placement** — `Stage::Physics` has exactly one registration.

### Dimension 6: Resource Lifecycle (GPU teardown ordering) — **0 findings**

- **Reverse-order destruction** — `context/teardown.rs:27-130+` destroys allocator-owned resources in reverse creation order inside the allocator guard; the #1483 hoist of allocator-independent destroys (`skin_palette`, `gpu_timers`, the SDK context half) is documented in place and not regressed.
- **`SkinSlot` teardown gate** — `teardown.rs:46-52` frees skin slots only when `skin_compute` is `Some`. Checked for the #3374 class: `skin_slots.insert` has exactly one call site (`context/skinned_blas_refit.rs:315`) and it is *inside* the `skin_pipeline` arm, so a `None` pipeline implies an empty map. Not a leak. **`MorphSlot`'s** equivalent gate was the real instance and is fixed — `teardown.rs:52-57` destroys them unconditionally, and the eviction/drain pass moved outside the `(skin_compute, accel_manager)` guard in `95005d87` (`skinned_blas_refit.rs:774-822`, source-asserted at `:1005-1045`).
- **#3298 in-flight rebuild buffers** — `MeshRegistry::destroy_all` explicitly destroys `geometry_rebuild`'s two target buffers (`crates/renderer/src/mesh.rs:1684-1688`) rather than relying on `GpuBuffer::Drop`, which is the #927 discipline.
- **Swapchain recreate coverage** — see Dimension 1.
- **Per-frame leaks** — no per-frame descriptor/command-buffer/staging allocation without a matching free found; the geometry staging pool is retained and released in `destroy_all`.

### Dimension 7: Worker Threads & Thread-Safety Bounds — **0 new findings**

- **Streaming Drop ordering (#1167)** — unchanged; `request_tx` taken before the worker handle, `shutdown` takes the handle so `Drop` short-circuits.
- **Worker ↔ main data flow** — the #3385 memo maps added to `WorldStreamingState` (`byroredux/src/streaming.rs:646-655`) are main-thread-only and cleared in `drain_streaming_state` (`streaming_helpers.rs:410-411`), matching their doc claim. `merge_external_material` remains main-thread-only; no worker call site.
- **Debug server** — command queue still bounded with an atomic check-and-push (`crates/debug-server/src/listener.rs:65-84`, `:306`); per-client threads never touch `World`. `DebugDrainSystem` remains a Late exclusive. (#3090's early `return` on a cancelled screenshot is still present at `system.rs:72-77` — still open, not re-filed.)
- **Allocator sharing** — `SharedAllocator` guards are scoped to individual allocate/free calls (`vulkan/buffer.rs:1470-1494` is the newest instance, #3298); none held across a queue submit.
- **Persistent-cell drain** — #3376 fixed, #3377 still open (see reconciliation).

---

## Process Notes (not codebase findings)

1. **#3244 is fixed but still OPEN on the tracker.** The `MorphSlot::weight_buffer` WAR race the 2026-08-24 report filed as HIGH is resolved in code (staged handoff + post-fence flush, with its own source-assert). Someone should close it, or say why it isn't closed. Same shape as the #3111 note in the 2026-08-24 report.
2. **`gh` was unreachable** from this environment. Dedup used the same-day cached `/tmp/audit/issues.json`. If any of the four findings below turns out to duplicate an issue filed after 23:23 today, that cache is why.
3. **A same-day sibling report carries a falsified premise.** `AUDIT_ECS_2026-08-27.md` § ECS-2026-08-27-04 asserts "no reverse edge exists in-tree today" for `CharacterRuleset → ActorValues`. Finding #1 disproves it. Whoever publishes both reports should reconcile them rather than filing two issues with contradictory impact statements.

---

## Candidates considered and NOT reported

- **Per-frame blocking submit in the #3298 chunked geometry rebuild** (`crates/renderer/src/mesh.rs:1386`/`:1414`, `byroredux/src/app_frame.rs:205-215`). Dimension 1's checklist asks to flag blocking one-time submits in the per-frame path. Not filed: it runs outside `draw_frame`, waits a dedicated fence rather than the queue, holds the queue Mutex only across the submit, and writes only into buffers nothing has bound yet — the old buffer keeps serving draws until swap-in (#3372). Bounding this stall is the feature. Any residual cost is a `/audit-performance` question, not a correctness one.
- **Mid-rebuild mutation of `pending_vertices`** — `rebuild_geometry_ssbo` early-returns to `advance_geometry_rebuild` before `plan_geometry_compaction` (`mesh.rs:1216-1222`), so compaction cannot rewrite the pools a chunked copy is reading; `register_mesh` only appends, leaving prior offsets valid; the finish gate re-checks `pending_vertices.len() == job.target_vertex_count`. Disproved.
- **`skin_slots` / `morph_slots` are `std::collections::HashMap<EntityId, …>`** and probed per-draw (`context/mod.rs:1467`, `:1478`; `draw.rs:3038`). The #2923 hot-path hashing rule enumerates specific collections and these are not among them, and it is a performance rather than a concurrency question. Flagged here for `/audit-performance`.
- **`condition.rs:618-643` (`EquipmentSlots`/`Inventory` → `CharacterRuleset`)** — looked like a third edge into `CharacterRuleset`, but the two acquisitions live in *different `match` arms* (`GetEquipped` vs `GetXPForNextLevel`), so no guard overlaps. Disproved.
- **`quest_stages.rs:924-948` and `fragment.rs:1837-1846`** — both looked like resource-held-across-storage; in both cases the resource guard is closed by a block that ends before the storage acquisition. Disproved.
- **`studio_host::pick_from_view`** (`studio_host.rs:114-130`) — the `WorldBound` guards are per-iteration temporaries inside a lazy `filter_map`, and the `StudioSession` write comes after `pick_spheres` consumes it. No overlap. Disproved.
- **`dump_awake_fallers` / `spawn_collider_census_report`** — re-checked the #3266 fix; guards are dropped before `FormIdPool`. Disproved.

---

## Coverage gaps in this run

Per `_audit-common.md`'s un-owned-subsystem list, the following were touched only
incidentally and are **not** claimed as covered:

- **ByroRedux SDK** (`crates/sdk/src/`) — reached only through `byroredux/src/studio_host.rs` (finding #3). The SDK's own types were not audited.
- **Mod runtime** (`crates/mod-runtime/src/`) — not examined; still has no engine consumer, and its trust boundary is `/audit-safety` Dim 11's.
- **FSR3 upscaler + FFI** (`crates/fsr3-sys/`, `vulkan/frame_upscaler.rs`) — the `mark_dispatch_completed` post-submit gating was verified as part of Dimension 2, but the FFI lifetime surface was not (that is `/audit-safety` Dim 1).
- **FaceGen** (`crates/facegen/`) and **Havok packfile** (`crates/hkx/`) — no threading surface; not examined.
- **Runtime confirmation** — no engine process was launched, so no `BYRO_VALIDATION` sync-validation counts and no `BYRO_LOCK_ORDER_CHECK=1` run back these findings. Findings #1, #3 and #4 predict a detector abort; that prediction is the cheapest confirmation available and has not been run here.

---

## Next step

```
/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-08-27b.md
```

Domain label: `concurrency` for all four (CPU-side lock ordering); none are
GPU-side `sync`. Suggested type/severity: #1 `bug`+`high`+`ecs`+`concurrency`;
#2 `bug`+`medium`+`concurrency`+`character` (it lives in CHARAL's regen module);
#3 `bug`+`medium`+`concurrency`+`ecs`; #4 `bug`+`low`+`concurrency`.
