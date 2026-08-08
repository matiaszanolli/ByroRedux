# Save / Load Subsystem Audit (M45 + M45.1) — 2026-08-07

Scope: `crates/save/src/*` (~1.2k LOC, unchanged this cycle except a 9-line
additive helper) and its sole engine-side consumer `byroredux/src/save_io.rs`
(grew **1,908 → 2,860 LOC**, +962 lines), audited at HEAD `79bfc76e`. This is
the **seventh** save audit (prior: `2026-06-23`, `2026-07-02`, `2026-07-03`,
`2026-07-16`, `2026-07-25`, `2026-08-03`). 65 commits landed since the prior
audit's `1ae86f62` baseline, 11 of them save-specific — most closing the prior
cycle's findings (SAVE-D1-08/09/10/11/12), plus a large `feat(quests)` /
`feat(world)` day-night landing that added two new save-registered resources
today (`GameTimeRes`, `QuestAliasInjectionState`).

All six dimensions ran as parallel Task agents this cycle (the 2026-08-03
report used a single-pass re-read; the scale of this cycle's diff — 962 new
lines in the one file every dimension touches — justified going back to the
full fan-out).

## Executive Summary

Docstring design claims (`crates/save/src/lib.rs`) verified against live code:

| Claim | Status |
|---|---|
| Full ECS snapshot | **CODE-CONFIRMED** — all 5 gaps from the 2026-08-03 report (SAVE-D1-08/09/10/11/12) are genuinely fixed (13 new registrations, each with a passing round-trip test). One new **process** gap found (SAVE-D1-18): the `#2295` completeness guard that replaced the old NPC-spawn-only guard doesn't scan `byroredux/src/` itself, where `GameTimeRes` and 44 other component/resource impls live — today's own headline fix would not have been guard-caught if forgotten. |
| Atomic write (tmp → fsync → read-back → rename → dir-fsync) | **CODE-CONFIRMED** — `crates/save/src/disk.rs` has zero diff since 2026-08-03; fourth consecutive clean re-verification. |
| Ring (never clobbers the last good save) | **CODE-CONFIRMED** — unchanged; `SaveState::new` still calls `SaveRing::resume`, not `::new`. |
| Validation gate (refuse to persist inconsistent state) | **CODE-CONFIRMED for the pre-existing 4+1 reference classes**, but **DRIFTED at the edges** — two of this cycle's new types (`HorseTetherState.horse`, `ActorCinematicState.vehicle`) carry raw `EntityId` references no check inspects (SAVE-D4-02), and one resource (`QuestAliasInjectionState.inventory_grants`) carries a raw `EntityId` that survives to the live-load path unguarded and unremapped, producing a real item-dupe exploit (SAVE-D6-01, HIGH — the most severe finding this cycle). |
| Off-frame load (never runs inside the scheduler) | **CODE-CONFIRMED** — `crates/save/src/driver.rs` has zero diff since 2026-08-03; both new large systems this cycle (quest lifecycle, day-night) are ordinary per-tick scheduler systems that complete before `Stage::Late`'s exclusive console drain, so neither can straddle the capture/load boundary. |

**Findings this cycle: 4 new. 0 CRITICAL, 1 HIGH, 3 MEDIUM, 0 LOW.**
All 5 prior findings (SAVE-D1-08/09/10/11/12) confirmed closed, with working
registrations and round-trip tests, not just allowlist rewording. The prior
cross-referenced open item, `#2154` (SaveState guard held too long across the
save-write walk), is confirmed **FIXED** by `8dd03c1d` — verified sound by
three independent dimensions (3, 4, 5) from three different angles (durability
ordering, validation-gate ordering, frame-boundary timing), with no
interleaving hazard introduced.

By Data-Loss Class: **reference-break — 2** (SAVE-D6-01 HIGH — item-dupe
exploit; SAVE-D4-02 MEDIUM — dangling refs silently accepted at write time);
**none — 2** (SAVE-D1-18, SAVE-D2-19 — both process/guard-coverage gaps, not
themselves lossy today).

`cargo test -p byroredux-save` (33 tests: 20 unit + 13 integration) passes
100%. `cargo test --bin byroredux save_io::` (32 tests, up from 22) passes
100% where the workspace compiles — Dimension 4 hit an **unrelated, pre-existing,
uncommitted** compile break in `crates/audio/src/lib.rs` (a mid-refactor
`apply_reverb_send` arg-count mismatch, outside this audit's scope) and fell
back to direct code reading for the two `save_io.rs` functions it needed;
`byroredux-save` is unaffected since it doesn't depend on `byroredux-audio`.
This is flagged here for visibility, not filed as a save-subsystem finding.

## Data-Loss Class Matrix

| Finding | Class | Dimension | Severity |
|---|---|---|---|
| SAVE-D6-01 (`QuestAliasInjectionState.inventory_grants` embeds unremapped `EntityId`, live-load re-grants items) | reference-break | 6 — M45.1 Live Load-Apply (independently flagged by Dim 4 as a validation-gate coverage gap, SAVE-D4-01 — merged, see below) | HIGH |
| SAVE-D4-02 (`HorseTetherState.horse`/`ActorCinematicState.vehicle` invisible to `validate_world`) | reference-break | 4 — Validation Gates | MEDIUM |
| SAVE-D1-18 (`#2295` guard has zero visibility into `byroredux/src/`) | none (process gap) | 1 — Snapshot Completeness | MEDIUM |
| SAVE-D2-19 (`SAVE_TYPE_SOURCES` not updated for 6 new files) | none (latent — same mechanism class as the historical `#1714`/`#2181` bug, currently untriggered) | 2 — Registry & (De)serialization Fidelity | MEDIUM |

## Completeness Ledger

### `build_save_registry` — net new registrations this cycle (13)
`TwoStateActivator`, `ScriptVariables`, `ActorControlState`, `PlayerControlState`,
`RigidBodyData`, `Material`, `RumbleOnActivate`, `FragmentExecutionQueue`,
`ActorCinematicState`, `HorseTetherState`, `CinematicPresentationState`,
`GameTimeRes`, `QuestAliasInjectionState` — all confirmed present in
`build_save_registry`, all with a passing dedicated round-trip test.

### `MUTABLE_DELTA_COLUMNS` cross-check (SAVED vs SAVED+OVERLAID vs structural)

| Column | Kind | Saved | In `MUTABLE_DELTA_COLUMNS` | Status |
|---|---|---|---|---|
| `TwoStateActivator`, `ScriptVariables`, `ActorControlState`, `RigidBodyData`, `RumbleOnActivate` | Component | yes | yes | SAVED+OVERLAID, plain-data, correct |
| `Material` | Component | yes | **no (deliberate)** | blast-radius exclusion, documented at registration site (#2378), verified still accurate |
| `ActorCinematicState`, `HorseTetherState` | Component | yes | **no (deliberate)** | `EntityId`-hazard exclusion (`vehicle`/`horse`), documented at registration site (#2380), verified still accurate |
| `PlayerControlState`, `CinematicPresentationState`, `FragmentExecutionQueue`, `GameTimeRes` | Resource | yes | n/a (resources bypass the delta-column path entirely — replaced wholesale by `restore_resources`) | correct by architecture |
| `QuestAliasInjectionState` | Resource | yes | n/a (same as above) | **architecturally correct placement, but the resource itself embeds an unguarded `EntityId` — see SAVE-D6-01. Resources were never meant to carry entity-keyed data; this one does.** |

No registered mutable **component** landed SAVED-but-never-overlaid this
cycle — the exact drift pattern the skill's Dimension 1 checklist most feared
did not recur. The one real gap this cycle is a **different, new** failure
mode: a **resource** carrying entity-keyed data that the resource-restore
path (`restore_resources`) was never designed to handle (see SAVE-D6-01).

### Cross-check against the SAVE-D1-12-successor guard's `NOT_SAVED_BY_DESIGN` allowlist
The `#2295` guard (`every_component_or_resource_impl_is_saved_or_explicitly_allowlisted`,
`byroredux/src/save_io.rs`) is green and its ~85-entry allowlist was spot-checked
(10 forward-latent/write-once claims sampled) with zero regressions — nothing
allowlisted as "no live inserter" has gained one since 2026-08-03. This guard's
own scope gap (SCAN_ROOTS excludes `byroredux/src/`) is SAVE-D1-18 below, not a
completeness-ledger discrepancy in the registrations it does see.

## Findings

### HIGH

#### SAVE-D6-01: `QuestAliasInjectionState.inventory_grants` embeds session-local `EntityId`s in a Resource the M45.1 live-load path never remaps — every `load` re-grants already-owned quest-alias inventory items
- **Severity**: HIGH
- **Dimension**: 6 — M45.1 Live Load-Apply (also independently surfaced by Dimension 4 — Validation Gates — as the same field's absence from any `validate_world` reference-class check; merged into one finding since both describe the identical root cause and field)
- **Data-Loss Class**: reference-break (manifests as item duplication, not loss)
- **Location**: `crates/scripting/src/scene.rs:162-174` (struct def — `inventory_grants` field, no `#[serde(skip)]`, unlike sibling `factions`), `:668-708` (`apply_alias_injections`'s dedup-by-tuple grant loop), `byroredux/src/save_io.rs:328-332` (`register_resource::<QuestAliasInjectionState>`), `crates/save/src/driver.rs:148-167` (`restore_resources` — wholesale verbatim resource replace by design, no remap parameter exists for resources)
- **Status**: NEW
- **Description**: `QuestAliasInjectionState` (added this cycle by the quest-lifecycle commit `a844c26b`) is registered as a save `Resource`:
  ```rust
  pub struct QuestAliasInjectionState {
      #[cfg_attr(feature = "save", serde(skip, default))]
      factions: HashMap<(EntityId, u32), InjectedFactionMembership>,
      inventory_grants: HashSet<(QuestFormId, i32, EntityId, u32, u32)>,
  }
  ```
  `factions` is correctly `serde(skip, default)`'d, with its doc comment
  explaining why: "reconstructed from the immutable alias definitions on
  load." `inventory_grants` has no such skip, and **does** serialize a raw
  session-local `EntityId` (a `u32` ECS index) inside every tuple — the same
  hazard class `#1696` excluded `AnimationPlayer.root_entity` for, and the
  same class `#2380`'s own doc comment reasons about for
  `ActorCinematicState`/`HorseTetherState`. The difference: those two are
  *components*, so keeping them off `MUTABLE_DELTA_COLUMNS` genuinely
  protects them from being overlaid with stale ids. `QuestAliasInjectionState`
  is a *resource*, and **resources have no remap mechanism at all** —
  `restore_resources` calls `load(world, value.clone())` verbatim for every
  registered resource, on the explicit design assumption "resources aren't
  entity-keyed, so they're replaced outright rather than remapped"
  (`driver.rs:150-151`). That assumption is false for this one resource.

  `apply_alias_injections` (`scene.rs:668-708`) dedups by inserting the full
  tuple `(quest, alias, entity, item, count)` into the set and skipping the
  grant if the insert reports "already present." On a live load:
  1. `restore_resources` installs the *saved* `inventory_grants`, keyed by
     the *previous* session's entity ids.
  2. `load_cell_with_masters` respawns every entity fresh. `EntityId`
     allocation is monotonic and never reclaimed
     (`crates/core/src/ecs/world.rs:115`), and no `set_next_entity` call
     exists anywhere in the live-load path (that's a `restore_world`-only
     operation, and `restore_world` is never called from the live binary —
     confirmed via full-tree grep, every non-test call site is inside
     `#[cfg(test)]`). The reloaded cell's entities get ids continuing from
     wherever this session's counter currently sits — structurally
     guaranteed **not** to match the saved ids.
  3. The unconditionally-scheduled `quest_alias_refresh_system`
     (`byroredux/src/boot.rs:671`) runs `apply_alias_injections` on the next
     tick. Every alias resolves to its **new** entity id, so the dedup
     `.insert((quest, alias, NEW_entity, item, count))` finds no match
     against the restored (old-entity-id) set — returns "not a duplicate" —
     and the item is granted again.

  This repeats on every subsequent `load`, with no convergence, because each
  reload assigns yet another fresh id. A player who repeatedly loads a save
  containing any resolved quest-alias inventory grant (a real, live,
  `a844c26b`-shipped mechanic — vanilla `SetStage`/alias-fill quest rewards)
  accumulates duplicate stacks of that item indefinitely.
- **Evidence**:
  ```rust
  // crates/scripting/src/scene.rs:677-688 — apply_alias_injections
  for (quest, alias, entity, item, count) in desired_inventory {
      if !next_grants.insert((quest, alias, entity, item, count)) {
          continue;   // "already granted" — but `entity` never matches post-reload
      }
      if let Some(inventory) = inventories.get_mut(entity) {
          inventory.push(ItemStack::new(item, count));   // re-granted
      }
  }
  ```
  The existing coverage test, `quest_alias_inventory_grant_ledger_survives_snapshot_round_trip`
  (`save_io.rs:2553-2632`), does **not** exercise this bug: it spawns exactly
  one actor in a fresh `World` before doing anything else and asserts
  `restored_actor == actor` — the one scenario (identical spawn ordering,
  identical starting `next_entity`) where ids coincidentally line up, which
  the real M45.1 path (reloading into a `next_entity` counter that has
  already advanced past the save's) never produces.
- **Impact**: Quest-alias-injected permanent inventory (narrative/reward
  items from vanilla `SetStage`/alias-fill quests) duplicates on every live
  `load` of a save where such a grant has already resolved — an exploitable
  item-dupe path reachable through the ordinary `load <slot>` console
  command, not an edge case, and a direct violation of
  `QuestAliasInjectionState`'s own stated idempotency purpose.
- **Suggested Fix**: Either (a) thread a remap parameter into
  `restore_resources` for this one resource (a shape change — resources
  currently assume no entity keys — reusing the same `HashMap<u32,u32>`
  `build_form_id_remap` already produces for components), or (b), simpler and
  consistent with `factions`' own precedent: `serde(skip, default)` the
  `inventory_grants` field too, and re-derive "already granted" from the
  entity's live `Inventory` contents on the first post-load
  `apply_alias_injections` pass instead of a saved entity-id ledger — a live
  cell reload always respawns authored REFRs fresh, so the ledger doesn't
  need entity-id continuity if it's keyed off content instead of identity.
  Add a regression test that goes through the actual `execute_pending_save_loads`
  shape (spawn unrelated entities before the alias actor so `next_entity` has
  already advanced, the way a real reload does) rather than the current
  same-session, same-entity-id test.

### MEDIUM

#### SAVE-D4-02: `HorseTetherState.horse` / `ActorCinematicState.vehicle` entity references are invisible to every `validate_world` check
- **Severity**: MEDIUM
- **Dimension**: 4 — Validation Gates
- **Data-Loss Class**: reference-break
- **Location**: `crates/scripting/src/cinematic.rs:120-144` (`ActorCinematicState.vehicle: Option<EntityId>`), `:171-179` (`HorseTetherState.horse: EntityId`); `byroredux/src/save_io.rs:273-274` (both registered); `crates/save/src/validate.rs` (no check touches either type)
- **Status**: NEW
- **Description**: Both components carry a direct `EntityId` reference to another entity (mounted vehicle / tethered horse). None of `validate_world`'s four reference-class checks inspect them — `validate_hierarchy` only walks `Parent`/`Children`, `validate_equipment` only walks `EquipmentSlots`↔`Inventory`, `validate_animation` only walks `AnimationPlayer`, `validate_inventory_instances` only walks `Inventory.items[].instance`. A save with `HorseTetherState.horse` (or `ActorCinematicState.vehicle`) pointing at an id `>= next_entity`, or at a live-but-unrelated entity, currently passes `validate_world` cleanly and is written with no diagnostic. Both types are also deliberately excluded from `MUTABLE_DELTA_COLUMNS` (Dimension 1/6-owned finding, not repeated here), so the live `execute_pending_save_loads` path never overlays a stale value onto a reloaded cell and carries no live risk from this specific gap — the residual window is: (a) a pre-write save capturing an already-dangling reference (e.g. the tethered horse despawned mid-session while the tether component survived) is written silently, and (b) the `restore_world` test/loose path restores components verbatim at saved ids with the same blind spot in its post-load diagnostic re-run.
- **Evidence**: Consumption is defensive (`byroredux/src/systems/cinematic.rs:271,306` — `transforms.get(tether.horse)?` / `transforms.get(state.vehicle?)?`), so a dangling id fails the `?` gracefully rather than panicking — this caps severity at MEDIUM (silently-skipped pose sync, not a crash) rather than HIGH/CRITICAL.
- **Impact**: The subsystem's defense-in-depth thesis ("the gate sees every reference class that could go stale") is not actually true for these two types; today's runtime consequence is silent and non-crashing, but a dangling reference is written and reloaded with zero diagnostic anywhere in the pipeline.
- **Suggested Fix**: Add a fifth `validate_world` sub-check (e.g. `validate_entity_refs`) that walks any component known to carry a bare `EntityId` field — starting with these two — and flags `id >= next_entity` the same way `validate_hierarchy`/`validate_animation` already do. Establishes a pattern future `EntityId`-bearing components can slot into instead of each needing a bespoke check.

#### SAVE-D1-18: The `#2295` completeness guard's source scan has zero visibility into `byroredux/src/` — the binary crate where `GameTimeRes` itself lives, plus 44 other component/resource impls
- **Severity**: MEDIUM
- **Dimension**: 1 — Snapshot Completeness & Determinism
- **Data-Loss Class**: none (the guard gap doesn't itself lose data — it removes the tripwire that would catch a *future* gap, recurring on a new axis from the now-closed SAVE-D1-12)
- **Location**: `byroredux/src/save_io.rs:1945-1949` (`SCAN_ROOTS` inside `every_component_or_resource_impl_is_saved_or_explicitly_allowlisted`)
- **Status**: NEW
- **Description**: The `#2295` guard (replacing the old NPC-spawn-only guard closed as SAVE-D1-12) is a real improvement — it recursively scans every `.rs` file under three `crates/` directories for `impl Component for X`/`impl Resource for X` and requires each `X` registered or allowlisted. But `SCAN_ROOTS` is:
  ```rust
  const SCAN_ROOTS: &[&str] = &[
      "../crates/core/src/ecs/components",
      "../crates/scripting/src",
      "../crates/physics/src",
  ];
  ```
  This never includes `byroredux/src/` itself — the binary crate this very test lives in. `GameTimeRes` (`byroredux/src/components/game_time.rs:117`, registered today), `PlayerPose`, `CurrentCellContext`, `SaveState`, and `PendingSaveLoadSlot` all live entirely outside the scan. 45 `impl Component for`/`impl Resource for` lines exist under `byroredux/src/` today, none of which any guard inspects.
- **Evidence**: `grep -rn "^impl Component for\|^impl Resource for" byroredux/src/` returns 45 matches, none reachable by `SCAN_ROOTS`. `GameTimeRes` — the exact type this audit was asked to verify as newly registered — is one of them: correctly registered, but not because the guard would have caught its absence.
- **Impact**: Any future save-relevant `Resource`/`Component` added directly to `byroredux/src/` (as `GameTimeRes` was) ships with zero automated tripwire, relying entirely on the author remembering the hand-add — the same discipline gap that produced the original SAVE-D1-08/09/10 findings, now on a different scope axis (file location instead of spawn-time-vs-runtime).
- **Suggested Fix**: Add a fourth scan root, `"../byroredux/src"`, to `SCAN_ROOTS`. Will require populating `NOT_SAVED_BY_DESIGN` with the ~40 remaining unregistered `byroredux/src/` types using the same one-line-reason convention — most already have adequate doc comments to lift a reason from. Budget as a follow-up with the same per-type care the original #2295 pass gave its first 85 entries.

#### SAVE-D2-19: `SAVE_TYPE_SOURCES` (the `#1714`/`#2181` serde-default guard's scan list) was not updated for six new save-participating source files — guard has zero visibility into ~23 serde-derived types
- **Severity**: MEDIUM
- **Dimension**: 2 — Registry & (De)serialization Fidelity
- **Data-Loss Class**: none today (latent silent-drop risk — same mechanism as the historical `#1714`/`#2181` bug class, currently un-triggered)
- **Location**: `byroredux/src/save_io.rs:2644-2671` (`SAVE_TYPE_SOURCES` const); missing: `crates/core/src/ecs/components/material.rs`, `crates/core/src/ecs/components/collision.rs`, `crates/scripting/src/papyrus_demo/mod.rs`, `crates/scripting/src/cinematic.rs`, `crates/scripting/src/player_control.rs`, `crates/scripting/src/fragment.rs`
- **Status**: NEW
- **Description**: `serde_default_on_saved_struct_requires_format_major_bump` (#1714/#2181) exists to catch a save-participating struct gaining `#[serde(default)]` without a `FORMAT_MAJOR` bump — exactly the drift class `schema_fingerprint()` structurally cannot see (type-key-only). Its coverage is a hand-maintained `SAVE_TYPE_SOURCES` file list, not a directory walk — unlike its sibling `#2295` registry-completeness guard, which does recursively scan. Cross-checking `SAVE_TYPE_SOURCES` against every type wired into `build_save_registry` this cycle found six source files (carrying ~23 serde-derived types, registered by the `#2378`/`#2379`/`#2380`/`#2381`/`#2382`/`c5202627` commit sequence) never added to the list. This is not hypothetical: the identical failure already happened once and was fixed as `#2015`/SAVE-D2-03 ("registered ActorValues for save but never added actor_values.rs to SAVE_TYPE_SOURCES") — it has now recurred across six files in the very next round of registrations.
- **Evidence**: `grep -n "serde(default\|#\[serde"` on the six files returns nothing today (no live exploit), while `grep -n "derive(.*Serialize"` shows 23 sites total that `SAVE_TYPE_SOURCES` never references — the guard silently skips all of them.
- **Impact**: If any future edit adds `#[serde(default)]` to a field on any of these six files' types, the guard test continues to pass green while an old save silently default-fills the changed field on load instead of failing loudly or triggering a `FORMAT_MAJOR` bump. Blast radius is zero today, but the false assurance is real.
- **Suggested Fix**: Add the six missing paths to `SAVE_TYPE_SOURCES` (mirroring the `#2015` fix exactly). Longer-term, replace the hand-maintained list with the same recursive-directory-scan pattern `#2295`'s guard already uses (`SCAN_ROOTS` + `collect_rs_files`) — reusing that machinery makes this class of gap structurally impossible to reintroduce a third time.

## Regression Guards Discovered / Reconfirmed

All guards from the 2026-08-03 report still exist and pass. New this cycle:

| Test | Invariant it pins |
|---|---|
| `every_component_or_resource_impl_is_saved_or_explicitly_allowlisted` (#2295) | Every `impl Component for`/`impl Resource for` under the three scanned `crates/` roots is registered or allowlisted with a reason — closes SAVE-D1-08/09/10/12 |
| `two_state_activator_and_script_variables_survive_save_load_round_trip` | `TwoStateActivator`/`ScriptVariables` round-trip |
| `player_and_actor_control_state_survive_save_load_round_trip` | `PlayerControlState`/`ActorControlState` round-trip |
| `rigid_body_data_survives_save_load_round_trip` | `RigidBodyData` round-trip (#2379) |
| `material_survives_save_load_round_trip` | `Material` round-trip, deliberately excluded from delta columns (#2378) |
| `rumble_on_activate_survives_save_load_round_trip` | `RumbleOnActivate` round-trip (#2382) |
| `cinematic_trio_survives_save_load_round_trip` | `ActorCinematicState`/`HorseTetherState`/`CinematicPresentationState` round-trip, first two deliberately excluded from delta columns (#2380) |
| `fragment_execution_queue_survives_save_load_round_trip_and_resumes` | `FragmentExecutionQueue` round-trip (#2381) |
| `game_time_survives_live_resource_restore` | `GameTimeRes` round-trip through the live-load resource-restore path specifically (not just `restore_world`) |
| `quest_alias_inventory_grant_ledger_survives_snapshot_round_trip` | `QuestAliasInjectionState` round-trip — **does not** exercise the SAVE-D6-01 bug (same-session, same-entity-id scenario only); a stronger regression test is part of SAVE-D6-01's suggested fix |

## Known Open Issue (Cross-Referenced, Confirmed Fixed This Cycle)

- **`#2154` / SAVE-D3-02** — previously: `SaveCommand::execute` held `ResourceRead<SaveRegistry>` + `ResourceWrite<SaveState>` across the entire validate+save walk, safe only via an unenforced exclusive-scheduling convention. Fixed by `8dd03c1d`: the `SaveState` guard now drops immediately after `state.ring.advance()` (the mutating step) completes, with an explanatory doc comment citing why the remaining `SaveRegistry` hold is safe (exclusive `DebugDrainSystem` lane). Independently re-verified sound by three dimensions from three angles — Dimension 3 (durability: ring mutation still precedes the guard drop, `disk::write_slot` never depended on the guard), Dimension 4 (validation-gate ordering: the gate still fully completes before any write), Dimension 5 (frame-boundary: the change is a pure lock-scope reduction, doesn't move `save_world`'s call site relative to the scheduler). No interleaving hazard exists in any of the three framings. **CLOSED, fix holds.**

## Verified Clean — No New Findings

- **Disk Format & Durability** (`crates/save/src/disk.rs`, `snapshot.rs`): zero diff since 2026-08-03 — fourth consecutive clean pass. Atomic write dance (create_dir_all → tmp → flush → sync_all → byte-exact read-back → rename → dir-fsync), header gate ordering (all 7 gates precede `serde_json::from_slice`), CRC scope (payload-only), ring resume-from-mtime, `parse_slot_filename` strictness all re-confirmed.
- **Registry & (De)serialization Fidelity** (`crates/save/src/registry.rs`): the only diff this cycle is a 9-line additive `resource_names()` accessor (feeds the #2295 guard), no behavioral change. `FnvHasher` constants canonical, `form_id_column()`'s explicit-flag keying (#1845) unchanged, no new `register_form_id_component` ambiguity across any of the 13 new registrations, FormId handle-vs-pair save/load error handling unchanged, full round-trip coverage confirmed for every newly-registered type.
- **Frame-Boundary Capture & Off-Frame Apply**: `crates/save/src/driver.rs` zero diff. `capture_player_pose` ordering in `main.rs` byte-for-byte unchanged (line numbers hold). `execute_pending_save_loads` remains structurally unreachable from inside a system. GPU/physics-handle teardown (`unload_current_interior`, unconditional) precedes reload. Zero new production `restore_world` call sites — the live path still calls only `restore_resources` + `apply_deltas`.
- **M45.1 Live Load-Apply — ordering machinery**: drain → resolve `CurrentCellContext` → pre-flight `validate_cell_loadable` → teardown → reload → `restore_resources` → `build_form_id_remap` → `apply_deltas` → post-load diagnostic validation → `apply_player_pose` (last) unchanged and re-confirmed. `AnimationPlayer`/`AnimationStack` exclusion (#1696) holds. Player-pose restore (momentum clear, #2018 mode-mismatch handling, `set_kinematic_translation` no-op safety) untouched by any commit this cycle. Idempotency holds (monotonic, never-reclaimed `EntityId` allocation structurally prevents delta-stacking across repeated loads). 10 of 11 newly-registered types' remap semantics verified correct — the one break (`QuestAliasInjectionState`) is SAVE-D6-01 above.

## Doc-Rot Observations (Not Filed as Findings)

- `docs/feature-matrix.md:189`'s `TD3-002` comment (Save/load M45/M45.1
  shipped 2026-06-21) re-confirmed still reads correctly, per the skill's
  explicit instruction not to re-flag it.

---

**Net assessment**: The M45/M45.1 subsystem's own crate machinery
(`crates/save/src/*`) remains provably solid — disk durability and
frame-boundary discipline had zero diff this cycle; registry gained only a
benign accessor. All 5 findings from the last audit closed cleanly with real
registrations and tests. But the pattern first named in the 2026-08-03
report's closing line — "a large feature landed real, live, player-mutable
state without the matching save-side update" — recurred once more this cycle,
in a new shape: not a missing registration this time (the completeness net
caught all 13 new types correctly), but a **resource** that violates the
resource-restore path's foundational assumption ("resources aren't
entity-keyed"). `QuestAliasInjectionState.inventory_grants` is real,
live, exploitable via the ordinary `load` command, and none of the three
independent safety nets (the completeness guard, `MUTABLE_DELTA_COLUMNS`
review, or `validate_world`) were positioned to catch it, because all three
were built around the component-side hazard shape. The two MEDIUM
process-gap findings (SAVE-D1-18, SAVE-D2-19) are the same structural lesson
repeating on two more axes (file-location scope, serde-guard file-list
maintenance) — each of this cycle's four findings is a guard/net that worked
correctly for the shape of gap it was built for, and missed a gap one shape
to the side of it.

Suggested next step: `/audit-publish docs/audits/AUDIT_SAVE_2026-08-07.md`
