# Save / Load Subsystem Audit (M45 + M45.1) — 2026-09-11

Scope: `crates/save/src/` (`lib`, `snapshot`, `registry`, `driver`, `disk`,
`validate`, `tests/round_trip.rs`) plus the engine-side consumer
`byroredux/src/save_io.rs` and its `save_io/*_tests.rs` siblings, and the
cross-cut ground truth the flow depends on (`app_events.rs`, `app_frame.rs`,
`boot/`, `cell_loader/{spawn,load,unload,transition,exterior}.rs`,
`extensions/{persist,capture}.rs`, `crates/physics/src/sync.rs`,
`docs/engine/save-load-roundtrip.md`).

This is the **thirteenth** save audit (prior: `2026-06-23` … `2026-08-30`).
Run as six parallel Task-agent dimensions (max 3 concurrent, per the skill's
orchestrator architecture) against HEAD `b3db49fa`, delta-audited against the
prior cycle's HEAD `64f64480`. Every dimension read the actual source files
directly; the most severe new finding (CRITICAL, Dimension 1) was
independently re-verified against live code by the orchestrator before
inclusion in this report, since it revises a mechanism a sibling dimension
(6) had separately assessed as clean.

## Executive Summary

`crates/save/src/lib.rs` / `snapshot.rs` docstring claims verified against live code:

| Claim | Status |
|---|---|
| Full ECS snapshot (curated game-state set) | **DRIFTED.** `Locked` gained genuine runtime mutators this cycle (#3159's `SetLocked`/`SetLockLevel`) but was never added to `build_save_registry`; the cell-load stamp unconditionally overwrites it. A scripted lock/unlock reverts on save/load and on any live cell revisit (SAVE-D1-2026-09-11-02, HIGH). |
| Atomic write (tmp → fsync → read-back-verify → rename → dir-fsync) | **CODE-CONFIRMED**, byte-for-byte, including its second relocation (`crates/save/src/disk.rs` → `crates/core/src/atomic_file.rs`) so `settings-io`/the launcher now share the identical sequence. |
| Ring never clobbers the last good save | **CODE-CONFIRMED.** `SaveState::new` still calls `SaveRing::resume`, not `SaveRing::new`. |
| Validation gate refuses to persist an inconsistent world | **CODE-CONFIRMED for the mechanism, DRIFTED for coverage.** Nine checks all run pre-write with no bypass. A newly-registered resource (`PapyrusProviderContinuationQueue`) carries a session-local `EntityRef` handle inside its saved locals with no gate inspecting it, and its own registration comment's safety claim is false (SAVE-D4-2026-09-11-01, MEDIUM). |
| Typed-decode preflight rejects a bad snapshot before any teardown | **CODE-CONFIRMED**, and now covers two additional pre-teardown gates added this cycle (`crate::extensions::preflight_extension_state`) with the same abort-and-return shape. |
| `FORMAT_MAJOR` bump is the only sanctioned schema evolution path | **CODE-CONFIRMED for the mechanism, one instance's stated rationale is factually wrong.** Now **22** (was 10). Every real bump has its baseline regenerated in the same commit. v22's commit message claims a "discriminant shift" hazard that does not exist for this codebase's `serde_json` externally-tagged format — the bump is harmless (conservative), but the citable precedent it leaves behind teaches the wrong mechanism (SAVE-D2-2026-09-11-01, MEDIUM). |
| Off-frame load, never inside the scheduler | **CODE-CONFIRMED for the mechanism, DRIFTED for a related invariant.** `restore_world`/`apply_deltas` still never run inside the scheduler. But a quicksave taken while a multi-frame interior-cell transition is still `Pending` writes a save with neither `CurrentCellContext` nor `CurrentExteriorContext` — a save that reports success, consumes a ring slot, and can never be loaded back (SAVE-D5-2026-09-11-01, HIGH). |
| Additive-only overlay + explicit reconciler for removals | **CODE-CONFIRMED, and its own guard's prior defect is now fixed.** The `delta_columns_removed_at_runtime_have_a_load_reconciler` guard (flagged non-functional last cycle) now genuinely scans the tree for both removal idioms (#3793). |
| Saved resources are in force when the reloaded cell is built | **CODE-CONFIRMED for the specific claim that motivated it, but the fix's mechanism has a new CRITICAL side effect.** #3789 correctly moved `restore_resources` to run before the reload, closing last cycle's HIGH (`ReferenceEnableState` read before its saved value arrived). But that fix restores the *entire* resource registry — including `ItemInstancePool` — before the teardown, so the teardown's own item-instance release call now frees slots in the just-restored *saved* pool instead of the discarded *live* one (SAVE-D1-2026-09-11-01, **CRITICAL**). |

**Findings this cycle: 10. 1 CRITICAL, 2 HIGH, 3 MEDIUM, 4 LOW.**

By Data-Loss Class: **corruption-on-load — 3** (1 CRITICAL, 2 HIGH);
**silent-drop — 1** (HIGH); **latent silent-drop escalating to
corruption-on-load — 1** (MEDIUM); **none directly (precedent-quality) — 1**
(MEDIUM); **test-gap only — 2** (LOW); **none (doc gap) — 2** (LOW).

Three prior-cycle items (2026-08-30 report) are confirmed **CLOSED** at HEAD,
verified against live code rather than taken on the fix commits' word:
`SAVE-D6-2026-08-30-01` (HIGH, #3789's ordering — though its mechanism is the
direct cause of this cycle's new CRITICAL), `SAVE-D1-2026-08-30-02` (MEDIUM,
the non-functional companion guard, fixed by #3793), `SAVE-D4-2026-08-30-03`
(MEDIUM, `validate_animation` coverage gap, fixed by #3791), and
`SAVE-D6-2026-08-30-04` (LOW, doc rot). The prior-prior "Prior-Cycle
Disposition" open items — #3491 (`Perks` reason), #3497 (discovery-based scan
roots), #3499 (`FullRadius` worker-disconnect race), #3500 (`save.info`
exterior misreport) — are all now confirmed **CLOSED** as well (`8ad3f7eb`,
`5a432856`).

## Data-Loss Class Matrix

| Finding | Class | Dimension | Severity |
|---|---|---|---|
| SAVE-D1-2026-09-11-01 — pre-reload `restore_resources` clobbers `ItemInstancePool` before teardown releases into it | corruption-on-load | 1 | **CRITICAL** |
| SAVE-D1-2026-09-11-02 — `Locked` runtime-mutable since #3159 but never registered; cell-load stamp overwrites it | silent-drop | 1 | HIGH |
| SAVE-D5-2026-09-11-01 — quicksave during a mid-flight interior-cell transition writes an unloadable save | corruption-on-load | 5 | HIGH |
| SAVE-D4-2026-09-11-01 — `PapyrusProviderContinuationQueue` carries an unvalidated, unrebound `EntityRef` | latent silent-drop → corruption-on-load | 4 | MEDIUM |
| SAVE-D2-2026-09-11-01 — v22 `FORMAT_MAJOR` bump's stated rationale ("discriminant shift") is factually wrong for `serde_json` | none directly (precedent-quality) | 2 | MEDIUM |
| SAVE-D2-2026-09-11-02 — `save_type_sources()`'s discovery roots are a stale hardcoded 5-crate list | latent (guard-coverage gap) | 2 | MEDIUM |
| SAVE-D2-2026-09-11-03 — no round-trip test for `Effect::SetLocked`/`SetLockLevel` | test-gap | 2 | LOW |
| SAVE-D2-2026-09-11-04 — `ReferenceEnableState` has no data-level round-trip test | test-gap | 2 | LOW |
| SAVE-D4-2026-09-11-02 — `validate_cinematic_entity_refs` doc comment stale ("four checks", "AnimationPlayer only") | none (doc rot) | 4 | LOW |
| SAVE-D6-2026-09-11-01 — extensions preflight/restore never named in `save-load-roundtrip.md` §6's numbered ordering | none (doc gap) | 6 | LOW |

## Completeness Ledger

`build_save_registry` (`byroredux/src/save_io.rs:324-508`) ×
`MUTABLE_DELTA_COLUMNS` (`:84-141`, 21 entries). Cross-checked against the
SAVE-D1-12 guard's `NOT_SAVED_BY_DESIGN` allowlist rather than re-derived —
the guard (`every_component_or_resource_impl_is_saved_or_explicitly_allowlisted`)
was run live by the orchestrator before dimension dispatch and is **green**.

| Column | Kind | Saved | Overlaid | Status |
|---|---|---|---|---|
| `Transform`, `Inventory`, `EquipmentSlots`, `LightSource`, `LightFlicker`, `ScriptTimer`, `TwoStateActivator`, `ScriptVariables`, `ActorValues`, `EquippedWeapon`, `Dead`, `WanderState`, `TravelState`, `Traveled`, `GuardState`, `PatrolState`, `Escorted`, `ActorControlState`, `CharacterController`, `RigidBodyData`, `RumbleOnActivate` | Component | yes | yes | SAVED+OVERLAID, pinned by `delta_columns_carry_only_session_stable_fields`. All six AI-procedure production removal sites (`WanderState`/`TravelState`/`Traveled`/`Escorted`/`GuardState`/`PatrolState`) are now correctly classified `NoReconcilerNeeded` by the fixed #3793 guard, alongside `EquippedWeapon`'s `Reconciler` entry. |
| `Name`, `Parent`, `Children`, `FormIdComponent` | Component | yes | no | structural identity — correct by design |
| `AnimationPlayer`, `AnimationStack` | Component | yes | no (deliberate) | #1696 `root_entity`/`clip_handle` hazard. `AnimationStack` still has no production insertion site (forward-latent). Both, plus `Seated.animation_restore.clip_handle`, are now covered by `validate_animation` (#3791, this cycle — closes last cycle's MEDIUM). |
| `FollowState`, `EscortState`, `Seated` | Component | yes | no (deliberate) | `EntityId` hazard, covered by `validate_saved_entity_references`. `Seated`'s exclusion comment now documents both hazards (`EntityId` + the `AnimationClipRegistry` handle in `animation_restore.clip_handle`). |
| `ActorCinematicState`, `HorseTetherState` | Component | yes | no (deliberate) | #2380 `EntityId` hazard; covered by `validate_cinematic_entity_refs` |
| `Material` | Component | yes | no (deliberate) | #2378 blast-radius; `validate_material_finiteness` runs on every save |
| `ActorVitals` | Component | yes | no (documented) | #3027 — FormID key, not an HP value |
| **`Locked`** | Component | **NO** | n/a | **NOT REGISTERED — SAVE-D1-2026-09-11-02 (HIGH).** The allowlist's own reason text: *"This is a real save-fidelity gap, not a justification."* |
| `ItemInstancePool`, `CurrentCellContext`, `CurrentExteriorContext`, `PlayerPose`, `GameTimeRes`, `QuestStageState`, `QuestObjectiveState`, `Globals`, `QuestAliasInjectionState`, `PlayerControlState`, `FragmentExecutionQueue`, `ReferenceEnableState`, `CinematicPresentationState`, `PapyrusProviderContinuationQueue` | Resource | yes | n/a | replaced wholesale by `restore_resources`. **The pre-reload call (added by #3789) now restores this ENTIRE list before teardown** — correct for `ReferenceEnableState` (the fix's target), but the mechanism is the direct cause of SAVE-D1-2026-09-11-01 (CRITICAL) for `ItemInstancePool`. `PapyrusProviderContinuationQueue` (new registration this cycle) carries an unvalidated `EntityRef` — SAVE-D4-2026-09-11-01 (MEDIUM). |

Ledger note (unchanged, re-verified): `QuestAliasInjectionState.factions`'s
`EntityId`-keyed map is `#[cfg_attr(feature = "save", serde(skip, default))]`
and never reaches disk — the registry's comment claiming exactly that remains
accurate.

## Findings

### CRITICAL

#### SAVE-D1-2026-09-11-01: `execute_pending_save_loads`'s #3789 fix restores the WHOLE resource set — including `ItemInstancePool` — before the cell teardown, so the teardown's item-instance release call now frees slots in the pool the player is trying to load, not the one being discarded

- **Severity**: CRITICAL
- **Dimension**: 1 — Snapshot Completeness & Determinism (resource-restore ordering vs. the registry's resource set); the live-apply sequence itself is Dimension 6's territory, but this was found while re-verifying the #3789 fix this cycle's brief called out, and independently confirmed by the orchestrator against live code (`save_io.rs`, `unload.rs`, `registry.rs`) before inclusion here.
- **Data-Loss Class**: corruption-on-load
- **Location**: `byroredux/src/save_io.rs:1546` (pre-reload `restore_resources`, added by `568343f3`/#3789) vs. `:1557-1559` (the reload call) vs. `byroredux/src/cell_loader/unload.rs:556-580` (`release_victim_item_instances`) vs. `byroredux/src/cell_loader/transition.rs:386-396` (`unload_current_interior` → `unload_cell`) vs. `crates/core/src/ecs/resources/mod.rs:1447-1484` (`ItemInstancePool::allocate`/`release`, slot-index based) vs. `crates/save/src/registry.rs:180-187` (`register_resource`'s `load` closure — `world.insert_resource(res)`, a wholesale replace, not a merge)
- **Status**: NEW. Introduced by `568343f3` (Fix #3789), which fixed last cycle's HIGH (`SAVE-D6-2026-08-30-01`, `ReferenceEnableState` read before its saved value arrived). The prior report's own *Disproved Candidates* section explicitly reasoned about this exact hazard in the opposite (safe) direction and warned against the fix that landed: *"the release happens before the wholesale replacement... The constraint is real in the other direction, and is why the fix for SAVE-D6-2026-08-30-01 cannot simply hoist `restore_resources` wholesale."* The fix that shipped is exactly the "hoist it wholesale" move that warning named.
- **Description**: `execute_pending_save_loads` now calls `byroredux_save::restore_resources(world, &registry, &snapshot)` — the full registry, not a filtered subset — at `:1546`, before `reload_interior_session`/`reload_exterior_session` run at `:1557`/`:1559`. `restore_resources` iterates every `register_resource` entry and calls its `load` closure, which is `world.insert_resource(res)` — a full replace. `ItemInstancePool` is one of those registered resources (`save_io.rs:448`).

  `reload_interior_session` then calls `unload_current_interior(world, ctx)` (`:1230`), which reaches `unload_cell` → `release_victim_item_instances`. That function walks the **currently-loaded (pre-load, live)** cell's `Inventory` components, collects every `ItemStack.instance: Some(id)`, and calls `pool.release(id)` against whichever `ItemInstancePool` is currently installed:

  ```rust
  pub(crate) fn release_victim_item_instances(world: &mut World, victims: &[EntityId]) {
      let mut to_release: Vec<ItemInstanceId> = Vec::new();
      { /* collect ids from the live Inventory rows */ }
      let Some(mut pool) = world.try_resource_mut::<ItemInstancePool>() else { return };
      for id in to_release {
          pool.release(id);
      }
  }
  ```

  Because of the #3789 reorder, that pool is now the **just-restored saved one**, not the live one the ids were actually allocated against. `ItemInstancePool::release` is a raw slot-index operation with no ownership/generation check across a resource swap:

  ```rust
  pub fn release(&mut self, id: ItemInstanceId) -> Option<ItemInstance> {
      let slot = id.0.get();
      let cell = self.instances.get_mut(slot as usize)?;
      let taken = cell.take()?;
      if !self.free.contains(&slot) { self.free.push(slot); }
      Some(taken)
  }
  ```

  If the live-session id happens to index a populated slot in the saved pool — likely, since both pools are simple monotonically-growing arenas starting at slot 1 — this **deletes a legitimate saved `ItemInstance`** (a named/modded weapon's condition, a unique item's custom name) and returns that slot to the saved pool's free list, moments before `apply_deltas` overlays the very `Inventory` rows that reference it by that same id.
- **Evidence**: Ordering confirmed by direct read of `save_io.rs:1471-1660` (pre-reload restore at `:1546`, `reload_interior_session` call at `:1557`, second restore at `:1591`, `apply_deltas` at `:1598`) — independently re-verified by the orchestrator, not taken on the sub-agent's word. `unload.rs:556-580` quoted verbatim, called from `unload_cell`, shared by both the interior (`cell_loader/transition.rs:393`) and exterior (`cell_loader/exterior.rs:1277`) paths. `ItemInstancePool::release`/`allocate` confirmed slot-index-based with no cross-swap ownership check. `register_resource`'s `load` closure confirmed a bare `world.insert_resource` — total replace, not additive. No existing test exercises this path: `crates/save/tests/round_trip.rs`'s fixtures call `apply_deltas`/`build_form_id_remap`/`restore_resources` against hand-built `World`s directly, never through `unload_cell`/`release_victim_item_instances`; `byroredux/src/save_io/live_reload_tests.rs`'s own `saved_resources_are_restored_before_the_cell_reload` test's doc comment says outright that `execute_pending_save_loads` needs a live `VulkanContext` and is not reachable from a unit test, which is the same limitation that makes this pool-corruption path untested.
- **Impact**: Every `load <slot>` / quickload / F9 in an active session (not the very first load right after boot) that has any inventory-bearing entity resident in the currently-loaded cell/tiles with an allocated `ItemInstance` — any named/modded/unique item in a container or carried by a nearby NPC — silently wipes that saved instance's data and frees its slot in the pool the player is loading *into*. The overlaid `Inventory` row for the entity that legitimately owns that instance then points at an emptied slot, or a slot a later `allocate()` in the same load reuses for an unrelated new item — silently aliasing two different items' data. This lands exactly in the class the subsystem exists to prevent: a save that round-trips CRC-clean on disk still loses real player-authored item state on the *load* side, with no log line pointing at the cause. Both interior and exterior reload branches are equally exposed, since `unload_cell` is the shared teardown for both.
- **Related**: #3789 (introduced this ordering); `SAVE-D6-2026-08-30-01` (the HIGH #3789 correctly fixed — this is a new side effect of that same fix, not a reopening of it); the prior report's own *Disproved Candidates* entry that named this exact hazard.
- **Suggested Fix**: Don't restore the *entire* registry before the reload — only the resources the spawn path actually consults pre-reload (today: `ReferenceEnableState`; per #3789's own comment, "and any future sibling"). Split `restore_resources` into a small pre-reload subset by resource name (mirroring `MUTABLE_DELTA_COLUMNS`'s explicit-list pattern), leaving `ItemInstancePool` and everything else for the existing post-reload call. Failing that, capture the live pool's state before the pre-reload restore and pass it to `release_victim_item_instances` explicitly, or force `unload_cell`'s teardown to run strictly before any `restore_resources` call. Add a regression test modeling a live entity's `Inventory.instance` allocation, calling the pre-reload restore with a *different* saved pool state, then running `release_victim_item_instances`, and asserting the saved pool's populated slots are untouched.

### HIGH

#### SAVE-D1-2026-09-11-02: `Locked` became a runtime-mutable component this cycle (#3159's `SetLocked`/`SetLockLevel` fragment effects) but was never registered as save state, and the cell-load stamp unconditionally overwrites it — a scripted lock/unlock reverts to the ESM-authored state on every save/load *and* on every live cell reload

- **Severity**: HIGH
- **Dimension**: 1 — Snapshot Completeness & Determinism
- **Data-Loss Class**: silent-drop
- **Location**: `crates/core/src/ecs/components/lock.rs` (the `Locked` component: `lock_level: u8`, `key_form_id: Option<u32>` — no `FixedString`/`EntityId`, delta-safe by the same bar `MUTABLE_DELTA_COLUMNS`'s other entries pass); `crates/scripting/src/fragment/effects.rs:814-852` (`Effect::SetLocked`/`Effect::SetLockLevel`); `byroredux/src/cell_loader/spawn.rs:919-932` (the unconditional ESM-authored `Locked` stamp on every cell load/reload); `byroredux/src/save_io/registry_completeness_tests.rs:428` (the `NOT_SAVED_BY_DESIGN` entry that names the gap but does not close it); `byroredux/src/save_io.rs:324-508` (`build_save_registry` — `Locked` absent)
- **Status**: NEW. `Locked`'s mutability is new this cycle: `e26579c1` (Fix #3159) added `SetLocked`/`SetLockLevel` specifically because an authored lock was previously "a one-way door for the session." Before #3159, `Locked` was correctly write-once-from-ESM and its `NOT_SAVED_BY_DESIGN` classification was accurate.
- **Description**: The registry-completeness guard's own allowlist entry states the gap explicitly rather than hiding it, and explicitly declines to be treated as a justification:

  > `("Locked", "XLOC lock data, rederived from the plugin's parsed REFR every cell load (#3098). NO LONGER rederived *identically*: #3159 added Effect::SetLocked, so a script can now unlock a door mid-session and a reload re-stamps the authored lock over it. **This is a real save-fidelity gap, not a justification** — the component needs registering once a scripted lock change is expected to survive a save...")

  No tracking issue covers it. The Dimension 1 checklist's own rule is direct: *"An unregistered mutable component = HIGH silent-drop finding."* `Locked` is neither registered nor genuinely reconstruct-on-load any more — the allowlist entry itself says so.

  Concretely, `Effect::SetLocked` inserts/removes `Locked` and `Effect::SetLockLevel` mutates its `lock_level` field. Nothing persists that change: it is absent from `build_save_registry`, so a full save/load silently reverts to whatever the ESM authored. The *live* reload path has the identical exposure without even needing a save — `cell_loader/spawn.rs:919-932` unconditionally re-inserts `Locked` from the placement's authored `XLOC` on every cell (re)load, with no check for an already-present (scripted) value:

  ```rust
  if let Some(l) = lock {
      world.insert(placement_root, Locked { lock_level: l.lock_level, key_form_id: l.key_form_id });
  }
  ```

  A player who picks a lock, then triggers any cell reload — a live `load <slot>`, or simply leaving and re-entering the cell in the same session — finds the door re-locked, with no record anywhere of what happened.
- **Evidence**: `lock.rs` struct fields, `fragment/effects.rs:814-852`, `cell_loader/spawn.rs:919-932`, and `registry_completeness_tests.rs:428` all quoted/confirmed directly. `grep -n "Locked" byroredux/src/save_io.rs crates/save/src/validate.rs` returns nothing — no registration, no validation gate.
- **Impact**: Any scripted lock/unlock — vanilla content routinely uses `Lock()`/`Unlock()`/`SetLockLevel()` on quest-gating doors and containers — does not survive a save/load and does not even survive an in-session cell revisit. A door meant to stay open after a quest event re-locking itself can strand a player or re-block content the quest already unlocked.
- **Related**: #3159 (introduced the mutation with no persistence companion); #3098 (the original write-once XLOC stamp, correct at the time); the analogous, already-fixed pattern for `EquippedWeapon` (#3488) and `ReferenceEnableState` (#3278/#3789) — both show the project's established fix shape for exactly this class of gap.
- **Suggested Fix**: Register `Locked` in `build_save_registry` (plain `u8`/`Option<u32>`, no session-local hazard) and add it to `MUTABLE_DELTA_COLUMNS`. Gate the cell-load stamp at `spawn.rs:919-932` on the placement root not already carrying a `Locked` component from a prior overlay/restore. Remove the `NOT_SAVED_BY_DESIGN` entry once both land, and file a tracking issue.

#### SAVE-D5-2026-09-11-01: Quicksave during a mid-flight interior-cell transition writes a save that can never be loaded back

- **Severity**: HIGH
- **Dimension**: 5 — Frame-Boundary Capture & Off-Frame Apply
- **Data-Loss Class**: corruption-on-load (a save that appears to succeed, consumes a ring slot, but is permanently unloadable)
- **Location**: `byroredux/src/cell_loader/transition.rs:386-396` (`unload_current_interior` clears `CurrentCellRoot`/`CurrentCellContext`); `byroredux/src/app_step.rs:904-921` (`step_cell_transition`'s Interior arm: teardown → `InteriorCellApply::begin`, budgeted `advance()` after); `byroredux/src/cell_loader/load.rs:920-984` (`InteriorCellApplyJob::advance`/`finish` — context inserted only inside `finish()`, i.e. only on `Complete`); `byroredux/src/app_step.rs:33` (`STREAMING_APPLY_BUDGET = 16ms`); `byroredux/src/app_events.rs:348-368` (F5/F9 handler queues a save unconditionally, no gate on an in-flight transition); `byroredux/src/save_io.rs:787-841` (`SaveCommand::execute`'s only pre-write gates — none inspect `CurrentCellContext`/`CurrentExteriorContext` presence); `byroredux/src/save_io.rs:1128-1141` (`LoadCommand::execute` — the symmetric gate that DOES exist, but only on the load side)
- **Status**: NEW (not found in any of the 12 prior save audit cycles, and no matching GitHub issue title in a 300-issue keyword sweep)
- **Description**: An interior-cell transition is a resumable, budgeted, multi-frame App-owned job (`self.interior_transition`, deliberately kept off the ECS `World`). `step_cell_transition`'s Interior arm first calls `unload_current_interior` (clearing both `CurrentCellRoot` and `CurrentCellContext`), then starts `InteriorCellApplyJob::advance` against a 16ms/frame budget. For any cell whose reference load doesn't fit in one slice — the entire reason the resumable design exists — `advance()` returns `Pending` and is resumed next frame. `CurrentCellContext`/`CurrentCellRoot` are inserted **only** inside `finish()`, on the terminal `Complete` arm. For the entire `Pending` window (which can span many frames for a real interior), the live `World` has **neither** cell nor exterior context installed — indistinguishable from genuine loose-NIF mode.

  `step_player_save_actions` runs every frame **before** `step_cell_transition` and has no visibility into `self.interior_transition`. The F5/F9 handler queues a quicksave unconditionally — there is no check against an in-flight transition (contrast with `step_save_loads`, which explicitly guards `self.interior_transition.is_some()` on the *load* side — no equivalent exists for *save*).

  If a player quicksaves while a large interior cell is still streaming in, `SaveCommand::execute`'s three referential-integrity gates (none of which check context presence) all pass, and the write succeeds and is reported as success. The resulting snapshot has neither context in its resources map. `LoadCommand::execute` correctly refuses to queue it, but reports it with the same message used for the legitimate loose-NIF case — there is no way to distinguish the two, and no way to recover the location.
- **Evidence**: Full call-site trace quoted in the Dimension 5 sub-report — `cell_loader/transition.rs:386-396`, `app_step.rs:904-921`, `cell_loader/load.rs:977-984`/`:1041-1046`, and the `about_to_wait` ordering (`capture_player_pose` → `step_player_save_actions` → `step_save_loads` → `step_cell_transition`, same tick). No gate in `SaveCommand::execute` references `CurrentCellContext`/`CurrentExteriorContext` — confirmed by reading the full gate list and cross-referencing `crates/save/src/validate.rs`'s exported functions.
- **Impact**: A player who quicksaves during any door transition into a cell heavy enough to exceed 16ms of reference-load work in one frame gets a save that reports success, occupies a ring slot (evicting whatever it would otherwise have kept), and can never be loaded again. The failure surfaces only later, at load, as an ambiguous "loose save" message.
- **Related**: Symmetric to the already-correct `LoadCommand::execute` gate and to `step_save_loads`'s existing `interior_transition`-awareness on the load side — the fix is the missing third leg of a pattern the codebase already applies twice.
- **Suggested Fix**: Give `SaveCommand::execute` (and/or `quicksave`) the same context-presence gate `LoadCommand::execute` already has. Since `self.interior_transition` is App-only, either (a) surface a lightweight `TransitionInFlight` marker resource that `step_cell_transition` sets/clears, checked by both `SaveCommand::execute` and the F5/F9 handler, or (b) have the F5/F9 handler's caller check `self.interior_transition.is_none()` before queuing, with `step_player_save_actions` re-checking before draining. Distinguish the resulting rejection message from the genuine loose-NIF-save case.

### MEDIUM

#### SAVE-D4-2026-09-11-01: `PapyrusProviderContinuationQueue` persists raw session-local `EntityRef` handles inside `ScriptValue::Entity` locals — a newly-registered save column carrying an inter-entity reference type none of the nine gates inspect, and whose own registration comment's safety claim is false

- **Severity**: MEDIUM
- **Dimension**: 4 — Validation Gates
- **Data-Loss Class**: latent silent-drop (common path: `entity_resolver` unavailable → continuation dropped with a warning) escalating to **corruption-on-load** under a specific condition: a stale `EntityRef` misresolves to a *different* live entity rather than failing to resolve.
- **Location**: `byroredux/src/save_io.rs:495-500` (registration + the false claim); `crates/scripting/src/papyrus_provider/ir.rs:98-113`; `crates/scripting/src/papyrus_provider/execute.rs:256-273,299-315,340-355`; `crates/sdk/src/identity.rs:219-227,296-306` (`EntityRef`, scoped to a `world_generation`); `byroredux/src/extensions/mod.rs:301-347` (`EntityHandleRegistry`); `byroredux/src/extensions/persist.rs:341-405` (`preflight_extension_state`/`restore_extension_state`'s early-`Ok` short-circuit when the saved `ExtensionStateSnapshot` is empty)
- **Status**: NEW. `PapyrusProviderContinuationQueue` was registered in this delta window (`3a0ce9bb`), so this could not have been reported by any prior audit.
- **Description**: The registration comment states *"All nested identities are stable manifest strings; no EntityId or process-local handles cross the save boundary."* This is false: `pending: Vec<PendingPapyrusProviderContinuation>`'s `locals: BTreeMap<String, ScriptValue>` can hold `ScriptValue::Entity(EntityRef)` — the SDK's own doc comment calls `EntityRef` "never a raw ECS slot... scoped to a world_generation so a stale one is rejectable," i.e. a process-local handle by definition. `execute.rs` inserts `self`/entity-typed locals at handler dispatch, and if that handler hits `Utility.Wait(...)`, the same map is captured verbatim into a suspended continuation with no rebinding at save time.

  Whether a stale post-load `EntityRef` is safe depends on `EntityHandleRegistry::begin_world_generation()`, called once inside `ExtensionHost::restore_saved_state`, reached via `restore_extension_state`. But `preflight_extension_state`/`restore_extension_state` **both short-circuit to `Ok` before calling into the host** when the save's `ExtensionStateSnapshot` has `rows.is_empty() && principal_storage.is_empty()` — a condition entirely orthogonal to whether `PapyrusProviderContinuationQueue` itself is non-empty. A session using the Papyrus-provider path but with no persisted SDK extension rows hits this short-circuit on every load: `begin_world_generation` never runs, and a stale `EntityRef` in the just-restored queue resolves via the untouched `by_handle` map to whatever `EntityId` it mapped to *before* the reload — which, in a freshly reloaded world with comparable spawn order/count, likely now names a **different, currently-live entity**. The resumed continuation then acts on the wrong object.
- **Evidence**: Capture sites and the double early-return quoted verbatim in the Dimension 4 sub-report. The only existing round-trip test for this column (`round_trip_tests.rs:872-983`) uses a fixture that touches no `self`/entity-typed local and sets no `entity_resolver` — it proves the string/form half survives a round trip and says nothing about the `Entity`-valued-local half.
- **Impact**: Under a real, reachable condition (a Papyrus-provider `Utility.Wait` pending on a handler using `self`, at a save taken while the SDK extension host is active but has zero persisted rows), a resumed continuation after load can silently execute against the wrong live entity. In the more common "no ExtensionHost" case, the continuation is simply dropped with a log warning — a milder, silent feature drop.
- **Related**: Same class as the now-fixed `SAVE-D4-2026-08-30-03` (a saved column carrying a reference type validation doesn't know about) and `SAVE-D6-2026-08-30-01` (a load-time consumer outrunning its protecting restore/rebind step) — distinct from both: a new column, in the SDK's `EntityRef` (not a core `EntityId`), whose own existing rebind mechanism is bypassed by an early-return rather than absent outright.
- **Suggested Fix**: Either (a) drop pending continuations carrying entity-typed locals across a save/load with a diagnostic, since a stale receiver handle can't be safely honored without the rebind this column lacks; or (b) persist entity-typed locals by stable `FormRef` and rebind through `entities_by_form` unconditionally (not gated on `saved.rows`/`principal_storage` non-emptiness). At minimum, correct the registration comment, and add a round-trip test exercising a `self`-referencing `Wait()` continuation across the live load path (not just `restore_world`).

#### SAVE-D2-2026-09-11-01: v22's `FORMAT_MAJOR` bump justification ("discriminant shift") is factually wrong for this codebase's serde_json format — a load-bearing precedent now teaches an incorrect model of what actually requires a bump

- **Severity**: MEDIUM
- **Dimension**: 2 — Registry & (De)serialization Fidelity
- **Data-Loss Class**: None directly (the bump is conservative, not permissive) — but the incorrect stated mechanism is now the citable precedent for future bump/no-bump calls, where the same error in the opposite direction would be a real compatibility bug.
- **Location**: `crates/scripting/src/translate/effects.rs:100` (`enum Effect`, plain `#[derive(Serialize, Deserialize)]`, no tag/repr override); commit `e26579c1` (Fix #3159); baseline comment at `byroredux/src/save_io/serde_default_guard_tests.rs:440-447`; `crates/save/src/snapshot.rs:212` (payload is `serde_json` of `Snapshot`)
- **Status**: NEW
- **Description**: `e26579c1`'s commit message states that adding enum variants "shifts later discriminants under serde's index-based representation" so a pre-v22 snapshot would deserialize as the wrong effect. This is incorrect for the actual on-disk format: `Effect` uses serde's default *externally tagged* JSON representation, which serializes a data-carrying variant as `{"<VariantName>": {…}}` keyed by the Rust variant **name**, never by ordinal. `serde_json`'s `Serializer` ignores the `variant_index` serde_derive passes it — only non-self-describing binary formats care about it, and this codebase doesn't use one for saves. Inserting `SetLocked`/`SetLockLevel` at any position is therefore backward-compatible for deserializing a pre-v22 tail exactly like `Effect::Enable` (#3489, correctly *not* bumped, with the commit's own correct reasoning: "serde only has to recognize the tags actually present"). The two commits give directly contradictory technical justifications for structurally identical changes on the same enum, three commits apart in the same file. Independently corroborated by `docs/audits/AUDIT_INCREMENTAL_2026-09-09.md` §4 for the unrelated `FloatTarget`/`ColorTarget` twin-deletion change, which states the correct principle for the identical format.
- **Evidence**: `effects.rs` enum declaration (no tag/repr attribute); `snapshot.rs:212`; `#3489`'s own (correct) commit message for the same enum three bumps earlier.
- **Impact**: No data loss — the bump fails closed (a clean `UnsupportedVersion` rejection, not silent corruption), consistent with the subsystem's "refuse rather than corrupt" thesis. The cost is (a) every pre-#3159 save was unnecessarily invalidated, working against the subsystem's own "don't make players lose progress" goal, and (b) the precedent-bank comment at `serde_default_guard_tests.rs` — explicitly written for future contributors deciding whether their own change needs a bump — now contains one entry with a wrong stated mechanism, risking either an unnecessary future bump or, more dangerously, an over-generalized "variant insertions are always safe" applied to a case that actually changes an *existing* variant's field shape.
- **Related**: `#3489` (the correct precedent this contradicts); `#3159`/`e26579c1` (the commit under review); `docs/audits/AUDIT_INCREMENTAL_2026-09-09.md` §4.
- **Suggested Fix**: Correct the `serde_default_guard_tests.rs:440-447` comment and the `e26579c1` record to state the real reason the bump was conservatively taken (not worth reverting now that it shipped). If a genuinely index-sensitive save format is ever adopted, this whole "no bump needed for variant insertion" precedent class needs re-auditing together.

#### SAVE-D2-2026-09-11-02: `save_type_sources()`'s discovery roots are a stale hardcoded 5-crate list, not the dynamic scan the sibling completeness guard was rewritten to use after proving the static-list approach misses real crates

- **Severity**: MEDIUM
- **Dimension**: 2 — Registry & (De)serialization Fidelity
- **Data-Loss Class**: Latent — a future `#[serde(default)]` or silent field-shape change on a save-participating type registered from outside the 5 scanned roots would bypass both SAVE-D2 shape/serde-default guards entirely, reproducing exactly the silent-corruption class those guards exist to prevent.
- **Location**: `byroredux/src/save_io/serde_default_guard_tests.rs:45-79` (`save_type_sources()`) vs. `byroredux/src/save_io/registry_completeness_tests.rs:51-84` (`discover_scan_roots()`)
- **Status**: NEW
- **Description**: `save_type_sources()` still uses five hardcoded roots (`byroredux/src`, `crates/core/src`, `crates/plugin/src`, `crates/scripting/src`, `crates/physics/src`) plus four explicit non-turbofish edges. This covers every currently-registered type today, but it is the exact shape of guard `registry_completeness_tests.rs` no longer trusts for itself: `#3497` replaced that file's own hardcoded `SCAN_ROOTS` with `discover_scan_roots()` — a live enumeration of every `crates/*/src` directory — specifically because the static list "has no defense against a root never being added," and doing so immediately surfaced four real types the old list had missed. `serde_default_guard_tests.rs`'s `save_type_sources()` was never given the same treatment. `crates/sdk` in particular (per this repo's `CLAUDE.md`: ~14,245 LOC, no owner audit skill) is a plausible future home for a save-participating type; if one lands there or in any other unscanned crate, both shape/serde-default guards will silently never scan it.
- **Evidence**: Root-list comparison; `registry_completeness_tests.rs:470` comment directly documenting the prior static-list miss.
- **Impact**: No live corruption today (nothing currently registered lives outside the 5 roots), but the guard-coverage gap is structural and will not fail loudly when the next save-participating type lands in an unscanned crate.
- **Related**: `#3497` (the sibling fix this guard was never given).
- **Suggested Fix**: Extract `discover_scan_roots()` into a shared location both test modules call, so `save_type_sources()` scans every workspace crate the same way.

### LOW

#### SAVE-D2-2026-09-11-03: no save/load round-trip test exercises `Effect::SetLocked`/`Effect::SetLockLevel` — the specific v22 shape change — through an actual serialize/deserialize cycle

- **Severity**: LOW
- **Dimension**: 2 — Registry & (De)serialization Fidelity
- **Data-Loss Class**: Test-gap only.
- **Location**: `byroredux/src/save_io/round_trip_tests.rs:757` (`fragment_execution_queue_survives_save_load_round_trip_and_resumes`)
- **Status**: NEW
- **Description**: The one existing `FragmentExecutionQueue` round-trip test queues `Effect::Wait`/`ProviderCall`/`SetHudCartMode` — never `SetLocked`/`SetLockLevel`, the two variants that motivated the v22 bump.
- **Impact**: Low — both are plain-data variants using already-proven-serializable types, so a serde failure is unlikely, but the checklist's round-trip-coverage bar is unmet for the exact shape change `FORMAT_MAJOR` exists to protect.
- **Related**: SAVE-D2-2026-09-11-01 (same commit/variants).
- **Suggested Fix**: Add `SetLocked`/`SetLockLevel` values to the existing round-trip test or a sibling.

#### SAVE-D2-2026-09-11-04: `ReferenceEnableState` is registered (with a correct `ValidateFn`) but has no save/load serde round-trip test — only a source-order text-scan test for restore ordering

- **Severity**: LOW
- **Dimension**: 2 — Registry & (De)serialization Fidelity
- **Data-Loss Class**: Test-gap only.
- **Location**: `byroredux/src/save_io.rs:501` (registration); `byroredux/src/save_io/live_reload_tests.rs:407` (the #3789 ordering test — a text-position check, not a data round trip)
- **Status**: NEW
- **Description**: No test constructs a populated `ReferenceEnableState`, saves it, decodes it, and asserts the restored value matches. `crates/save/tests/round_trip.rs`'s crate-level fixture doesn't include it either.
- **Impact**: Low — no evidence of an actual serde defect, but a `FormId`-keyed map is exactly the shape class worth extra scrutiny per the Dimension 2 checklist.
- **Related**: None.
- **Suggested Fix**: Add a data-level round-trip test mirroring the pattern used for `FragmentExecutionQueue`/`PapyrusProviderContinuationQueue`/`CinematicPresentationState`.

#### SAVE-D4-2026-09-11-02: `validate_cinematic_entity_refs`'s doc comment still enumerates "four reference-class checks" and describes `validate_animation` as covering only `AnimationPlayer` — both stale

- **Severity**: LOW
- **Dimension**: 4 — Validation Gates
- **Data-Loss Class**: none (doc rot — no functional impact)
- **Location**: `byroredux/src/save_io.rs:725-736`
- **Status**: NEW. Introduced at `90ae915c` (#2535) when `validate_world` genuinely had four checks; never updated across every subsequent addition, including this cycle's `validate_animation` extension (#3791).
- **Description**: `validate_world` runs seven core checks today, not four; `validate_animation` covers `AnimationPlayer`, `AnimationStack`, and `Seated.animation_restore.clip_handle`, not "only `AnimationPlayer`" as the comment claims seven lines from the very fix that changed it.
- **Impact**: Documentation-only; risk is to a future auditor/contributor taking the enumeration at face value.
- **Related**: None open.
- **Suggested Fix**: Update the comment to name the two hazards this function adds without re-enumerating `validate_world`'s internals (the enumeration is what keeps going stale).

#### SAVE-D6-2026-09-11-01: extensions preflight/restore are never named in `save-load-roundtrip.md` §6's numbered ordering list, only in a disconnected prose section

- **Severity**: LOW
- **Dimension**: 6 — M45.1 Live Load-Apply (documentation)
- **Data-Loss Class**: none (doc rot / doc gap)
- **Location**: `docs/engine/save-load-roundtrip.md:138-236` (§6's 8-step numbered list) vs. `:261-292` ("Engine-native extension state")
- **Status**: NEW. `24df5304` (sandboxed extensions) landed after the 2026-08-30 audit; the doc section was added in the same era but never folded into §6's step list.
- **Description**: Neither `crate::extensions::preflight_extension_state` (real step 3, between the typed preflight and the #3789 pre-reload `restore_resources`) nor `crate::extensions::restore_extension_state` (real step 7, between the reload and the post-reload `restore_resources`) appears in §6's numbered trace. The separate "Engine-native extension state" section covers the right invariants in prose but names neither function nor its position relative to §6's steps.
- **Impact**: Documentation only — the substance is accurate in isolation, but a reader following §6 to understand control flow has no way to learn these two calls exist or where they sit. This is exactly the kind of gap that let a real ordering bug (`SAVE-D6-2026-08-30-01`) through three audit cycles unnoticed.
- **Related**: `24df5304`; `SAVE-D6-2026-08-30-01`/`-04` (the closed findings this section is adjacent to).
- **Suggested Fix**: Fold two bullets into §6's numbered list — "1b. Extensions preflight" between steps 1 and 2, "6b. Extensions restore" between the reload and "Restore whole resources" — each cross-referencing the existing prose section rather than duplicating it.

## Prior-Cycle Disposition (2026-08-30 report, re-checked at HEAD — not taken on faith)

| 2026-08-30 finding | Issue | State at HEAD `b3db49fa` |
|---|---|---|
| SAVE-D6-2026-08-30-01 — reload consults `ReferenceEnableState` before `restore_resources` | #3278 (regression), fixed by #3789 | **CONFIRMED CLOSED** for the specific claim — `restore_resources` now runs pre-reload. **But its fix mechanism is the direct cause of this cycle's new CRITICAL** (SAVE-D1-2026-09-11-01). |
| SAVE-D1-2026-08-30-02 — the `delta_columns_removed_at_runtime_have_a_load_reconciler` guard scanned nothing | #3793 | **CONFIRMED CLOSED.** Now genuinely scans both removal idioms across the tree and classifies all seven production sites correctly. |
| SAVE-D4-2026-08-30-03 — `validate_animation` didn't cover `AnimationStack`/`Seated` | #3791 | **CONFIRMED CLOSED.** Four new regression tests back the fix. |
| SAVE-D6-2026-08-30-04 — doc calls death "the one case wired today" | doc-rot | **CONFIRMED CLOSED.** `save-load-roundtrip.md` now lists both reconcilers correctly. |
| SAVE-D6-2026-08-27-04 — `FullRadius` worker-disconnect race | #3499 | **CONFIRMED CLOSED**, fixed in `5a432856` (bundled with an unrelated #3518 fix — invisible from the commit title alone). Two unit tests pin both sides. |
| SAVE-D3-2026-08-27-05 — `save.info` misreports every exterior save | #3500 | **CONFIRMED CLOSED**, verified by reading both command implementations side by side and running the regression pair live. |
| SAVE-D1-2026-08-27-02 — `Perks` allowlist reason cites a guard that never inspects `Perks` | #3491 | **CONFIRMED CLOSED.** The reason now correctly states `validate_progression_state` only inspects `CharacterLevel`. |
| SAVE-D1-2026-08-27-03 — `SCAN_ROOTS` cannot notice a new crate | #3497 | **CONFIRMED CLOSED** for `registry_completeness_tests.rs` (now `discover_scan_roots`). **Its sibling `serde_default_guard_tests.rs` was never given the same fix — SAVE-D2-2026-09-11-02 above.** |

## Regression Guards Discovered / Verified This Cycle

| Guard | Location | Invariant pinned | State |
|---|---|---|---|
| `every_component_or_resource_impl_is_saved_or_explicitly_allowlisted` | `save_io/registry_completeness_tests.rs` | every production `impl Component`/`impl Resource`, discovered across every workspace crate (`discover_scan_roots`), is registered XOR allowlisted | **green** — run live by the orchestrator before dimension dispatch |
| `saved_type_shape_changes_require_format_major_bump` + `serde_default_on_saved_struct_requires_format_major_bump` | `save_io/serde_default_guard_tests.rs` | any field add/remove/retype or bare/`cfg_attr` `#[serde(default)]` on a saved type requires a `FORMAT_MAJOR` bump; `BASELINE_MAJOR = 22` | **baseline current** — every real bump (v11–v22) traced to a same-commit baseline regeneration. Discovery roots are stale (SAVE-D2-2026-09-11-02). |
| `delta_columns_removed_at_runtime_have_a_load_reconciler` | `save_io/round_trip_tests.rs` | every production removal of a `MUTABLE_DELTA_COLUMNS` type has a documented reconciler or `NoReconcilerNeeded` reason | **green and now functional** (#3793 fix, closes last cycle's MEDIUM) |
| `delta_columns_carry_only_session_stable_fields` | `save_io/round_trip_tests.rs` | `MUTABLE_DELTA_COLUMNS` equals the hand-audited 21-entry set | **green**, unchanged |
| `dangling_animation_stack_root_entity_is_rejected`, `stale_animation_stack_layer_clip_handle_is_rejected`, `stale_seated_animation_restore_clip_handle_is_rejected`, `healthy_animation_stack_and_seated_restore_are_clean` | `crates/save/src/validate.rs:740-854` | `validate_animation`'s new `AnimationStack`/`Seated` coverage (#3791) | **green**, new this cycle |
| `saved_resources_are_restored_before_the_cell_reload` | `save_io/live_reload_tests.rs:407` | #3789's ordering (source-text position check only — does not exercise `unload_cell`, which is why it cannot catch SAVE-D1-2026-09-11-01) | **green but insufficient scope** |
| `exterior_reload_overlay_tests::zero_pending_is_safe` / `nonzero_pending_is_unsafe` | `save_io.rs:1305-1324` | #3499's `FullRadius` worker-disconnect guard | **green** |
| `save_info_reports_worldspace_and_grid_for_an_exterior_save`, `save_info_reports_cell_for_an_interior_save` | `save_io/command_queue_tests.rs:123-208` | #3500's `save.info` three-arm classification, kept in lockstep with `LoadCommand` | **green** |
| `atomic_write_replaces_the_target_and_consumes_the_temp`, `atomic_write_fails_without_renaming_when_the_temp_cannot_be_created` | `crates/core/src/atomic_file.rs` | shared durable-write sequence, now with a third caller (`settings-io`) | **green** |
| Container gates (`rejects_bad_magic`/`rejects_truncated`/`rejects_payload_truncation`/`detects_crc_corruption`/`rejects_schema_mismatch`/`rejects_major_version_skew`) | `crates/save/src/snapshot.rs` | every header gate precedes `serde_json::from_slice`; CRC is payload-only | **green** |
| `form_id_column_resolves_the_flagged_entry`, `form_id_column_is_none_without_registration`, `registering_a_second_form_id_column_panics` | `crates/save/src/registry.rs` | explicit `is_form_id` flag, not the old heuristic | **green** |
| `typed_snapshot_preflight_rejects_bad_column_without_world_mutation` | `crates/save/tests/round_trip.rs` | `validate_snapshot_types` runs before `clear_entities`/teardown | **green**, now also covers the two new extensions preflight/restore calls by the same abort-before-mutation shape |
| `discover_scan_roots_finds_every_workspace_crate_and_byroredux` | `save_io/registry_completeness_tests.rs` | #3497's discovery-based root enumeration pins `crates/sdk/src` | **green** |
| `retained_form_row_rebinds_before_the_next_activation` | `byroredux/src/extensions/tests.rs:4330-4358` | extension-state rebind-through-`FormRef`, retain-on-miss contract | **green** |

## Summary Table (per dimension)

| Dimension | Findings | Notes |
|---|---|---|
| 1 — Snapshot Completeness & Determinism | **2** (1 CRITICAL, 1 HIGH) | The `ReferenceEnableState` fix's mechanism (whole-registry pre-reload restore) corrupts `ItemInstancePool` on teardown; `Locked` became mutable with no persistence companion. Everything else (registry vs. game-state completeness, two-lists drift, determinism, `next_entity`, StringPool, empty-column omission) clean. |
| 2 — Registry & (De)serialization Fidelity | **4** (2 MEDIUM, 2 LOW) | All `ValidateFn`/FNV/`form_id_column`/FormId-pair mechanisms verified sound; the v22 bump's stated rationale is wrong (harmless in direction); the shape/serde-default guard's discovery roots are stale; two round-trip test gaps. |
| 3 — Disk Format & Durability | **0** | Second consecutive 0-finding cycle. `atomic_write`'s second relocation (into `core::atomic_file`) is a verified no-op; #3500 confirmed closed with live regression tests. |
| 4 — Validation Gates | **2** (1 MEDIUM, 1 LOW) | The nine-check write-path gate, dangling-id semantics, and load-side diagnostic-only re-validation all clean; a newly-registered resource carries an unvalidated `EntityRef`; one doc-rot comment. |
| 5 — Frame-Boundary Capture & Off-Frame Apply | **1** (HIGH) | Boot-split registry install, `about_to_wait` ordering, and the unconditional notification drain all clean; new finding is a save-side gap symmetric to an existing load-side guard. |
| 6 — M45.1 Live Load-Apply | **1** (LOW) | Strict apply ordering matches the skill's documented sequence exactly; last cycle's HIGH/LOW both confirmed closed; the only gap is a doc-integration issue for the new extensions preflight/restore calls. |

**Total: 10 findings — 1 CRITICAL, 2 HIGH, 3 MEDIUM, 4 LOW.**
