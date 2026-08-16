# Save / Load Subsystem Audit (M45 + M45.1) — 2026-08-16

Scope: `crates/save/src/` (5 modules, 1,343 LOC — unchanged since the prior
cycle) plus the engine-side consumer `byroredux/src/save_io.rs` (1,054 LOC
production) and its six `byroredux/src/save_io/*_tests.rs` siblings. Audited at
HEAD `85b77371`. This is the **eighth** save audit (prior: `2026-06-23`,
`2026-07-02`, `2026-07-03`, `2026-07-16`, `2026-07-25`, `2026-08-03`,
`2026-08-07`).

This cycle's material change is not inside the save subsystem at all — it is the
**P2 gameplay slice** (`byroredux/src/combat.rs`, `byroredux/src/inventory.rs`,
landed 2026-08-15/16), whose new components, resources and *component removals*
the save path must now carry. `docs/engine/playable-vertical-slice.md` lists
save-reload continuity for that slice as an open gate, so completeness against
it was this audit's primary target. All six dimensions were run.

`cargo test --bin byroredux save_io` — **36/36 pass**, including both
completeness guards and the serde-default guard. Green tests are exactly why two
of this cycle's findings matter: both are places where a guard passes green over
a live violation.

## Executive Summary

`crates/save/src/lib.rs` docstring claims verified against live code:

| Claim | Status |
|---|---|
| Full ECS snapshot (curated game-state set) | **CODE-CONFIRMED for the registry itself** (33 components + 10 resources; the SAVE-D1-12/#2295 guard is green and now scans `byroredux/src/` too, closing the prior cycle's SAVE-D1-18). **DRIFTED for the live-load half**: `apply_deltas` is additive-only, and the P2 combat slice made component *removal* a real gameplay transition for the first time — see SAVE-D1-2026-08-16-01. |
| Atomic write (tmp → fsync → read-back → rename → dir-fsync) | **CODE-CONFIRMED** — `crates/save/src/disk.rs` has zero diff since 2026-08-03; fifth consecutive clean verification. |
| Ring never clobbers the last good save | **CODE-CONFIRMED** — `SaveState::new` still calls `SaveRing::resume`, and `SaveCommand` still `peek`s before validation / `advance`s only after it. |
| Validation gate refuses to persist inconsistent state | **CODE-CONFIRMED for the six checks that exist** (4 core + `validate_form_ids` + `validate_cinematic_entity_refs`), **DRIFTED at the edges** — three more saved `EntityId` carriers and one new intra-entity index reference are outside every check (SAVE-D4-2026-08-16-01/02). |
| Off-frame load, never inside the scheduler | **CODE-CONFIRMED** — `restore_world` has zero production callers; the live drain runs in `App::step_save_loads` between ticks. Dimension 5 is clean. |
| `FORMAT_MAJOR` bump is the only sanctioned schema evolution path | **DRIFTED — the enforcement is bypassable and has already been bypassed.** The guard's matcher cannot see the `#[cfg_attr(feature = "...", serde(...))]` attribute form, which is the house style for every feature-gated saved type. Two live `serde(default)` fields were added to a save-participating struct on 2026-08-07 with no major bump. SAVE-D2-2026-08-16-01. |

**Findings this cycle: 9. 0 CRITICAL, 2 HIGH, 5 MEDIUM, 2 LOW.**
All are NEW; none matches an OPEN issue in `/tmp/audit/issues.json` (269 open,
searched for save / load / snapshot / corrupt / FormId / delta / serde /
exterior / Dead / pose / ring / validate / EntityId keywords) or a prior
`docs/audits/AUDIT_SAVE_*.md` finding.

By Data-Loss Class: **corruption-on-load — 2** (both HIGH);
**silent-drop — 1**; **reference-break — 2**; **none / process — 4**.

## Data-Loss Class Matrix

| Finding | Class | Dimension | Severity |
|---|---|---|---|
| SAVE-D6-2026-08-16-01 (stale `CurrentCellContext` makes an exterior save load into a departed interior) | corruption-on-load | 6 — Live Load-Apply | HIGH |
| SAVE-D2-2026-08-16-01 (serde-default guard blind to `cfg_attr`; two live unguarded `serde(default)` fields) | corruption-on-load | 2 — Registry & (De)serialization | HIGH |
| SAVE-D1-2026-08-16-01 (death teardown removals are unreplayable by the additive-only overlay) | silent-drop | 1 — Completeness | MEDIUM |
| SAVE-D4-2026-08-16-01 (`FollowState`/`EscortState`/`Seated` `EntityId`s invisible to every gate) | reference-break | 4 — Validation Gates | MEDIUM |
| SAVE-D4-2026-08-16-02 (`EquippedWeapon.inventory_index` → `Inventory` unvalidated) | reference-break | 4 — Validation Gates | MEDIUM |
| SAVE-D2-2026-08-16-02 (`SAVE_TYPE_SOURCES` missing/mis-pathed entries) | none (latent) | 2 — Registry & (De)serialization | MEDIUM |
| SAVE-D6-2026-08-16-02 (no non-console save/load entry point) | none (surface gap) | 6 — Live Load-Apply | MEDIUM |
| SAVE-D1-2026-08-16-02 (`ActorVitals` exclusion undocumented) | none (convention) | 1 — Completeness | LOW |
| SAVE-D6-2026-08-16-03 (`save-load-roundtrip.md` stale) | none (doc rot) | 6 — Live Load-Apply | LOW |

## Per-Dimension Coverage

| Dimension | Findings | Notes |
|---|---|---|
| 1 — Snapshot Completeness & Determinism | **2** (1 MEDIUM, 1 LOW) | Registry itself complete; the gap is removal-replay and one undocumented exclusion. Determinism, `next_entity`, `StringPool` symbol order all clean. |
| 2 — Registry & (De)serialization Fidelity | **2** (1 HIGH, 1 MEDIUM) | Fingerprint / FNV constants / `form_id_column` / `FormIdPair` handling all clean; both findings are in the *guards*, not the runtime. |
| 3 — Disk Format & Durability | **0** | Fully clean, fifth consecutive cycle. |
| 4 — Validation Gates | **2** (2 MEDIUM) | Gate placement, abort semantics and dangling-id semantics all correct; both findings are coverage gaps in *which* reference classes are inspected. |
| 5 — Frame-Boundary Capture & Off-Frame Apply | **0** | Capture ordering, exclusive-lane safety, `take()`-once drain and single-restore-path all verified. |
| 6 — M45.1 Live Load-Apply | **3** (1 HIGH, 1 MEDIUM, 1 LOW) | Apply ordering, remap, idempotency, pre-flight and pose restore all confirmed correct; the HIGH is upstream of the drain, in cell-context lifetime. |

## Completeness Ledger

`build_save_registry` (`byroredux/src/save_io.rs:207-361`): **33 components + 10
resources**. `MUTABLE_DELTA_COLUMNS` (`:83-129`): **20 columns**.

| Column | Kind | Saved | Overlaid | Status |
|---|---|---|---|---|
| `Transform`, `Inventory`, `EquipmentSlots`, `LightSource`, `LightFlicker`, `ScriptTimer`, `TwoStateActivator`, `ScriptVariables`, `ActorValues`, `WanderState`, `TravelState`, `Traveled`, `GuardState`, `PatrolState`, `Escorted`, `ActorControlState`, `RigidBodyData`, `RumbleOnActivate` | Component | yes | yes | SAVED+OVERLAID, delta-safe, pinned by `delta_columns_carry_only_session_stable_fields` |
| `EquippedWeapon`, `Dead` | Component | yes | yes | SAVED+OVERLAID — **new this cycle (P2 slice)**, both delta-safe (u32 index + u32 FormID + f32; zero-field marker). No dedicated round-trip test (rolled into SAVE-D1-2026-08-16-01). |
| `Name`, `Parent`, `Children`, `FormIdComponent` | Component | yes | no | structural identity — correct by design |
| `AnimationPlayer`, `AnimationStack` | Component | yes | no (deliberate) | #1696 `EntityId`/`clip_handle` hazard, documented at the const, still accurate |
| `FollowState`, `EscortState`, `Seated` | Component | yes | no (deliberate) | `EntityId` hazard, documented; but see SAVE-D4-2026-08-16-01 for the validation half |
| `ActorCinematicState`, `HorseTetherState` | Component | yes | no (deliberate) | #2380, documented at the registration site, still accurate |
| `Material` | Component | yes | no (deliberate) | #2378 blast-radius, documented at the registration site, still accurate |
| `ActorVitals` | Component | yes | **no (undocumented)** | write-once at `byroredux/src/npc_spawn.rs:112`, no mutator exists → functionally safe, but the exclusion carries no rationale. SAVE-D1-2026-08-16-02 |
| `ItemInstancePool`, `CurrentCellContext`, `PlayerPose`, `GameTimeRes`, `QuestStageState`, `QuestObjectiveState`, `QuestAliasInjectionState`, `PlayerControlState`, `FragmentExecutionQueue`, `CinematicPresentationState` | Resource | yes | n/a | replaced wholesale by `restore_resources`, which runs before `apply_deltas` — correct |

Cross-check against the `#2295` guard's `NOT_SAVED_BY_DESIGN` allowlist (green,
~130 entries, now including the `byroredux/src/` scan root that closed the prior
cycle's SAVE-D1-18): the P2 slice's four new resources are all present and
their reasons were spot-checked against live code —

- `CombatState` — "session-local attack timing and smoke telemetry" — **accurate**
  (`byroredux/src/combat.rs:43-51`: cooldown, counters, one trace entry; the
  canonical `Dead` / Health / `EquippedWeapon` state is registered separately).
- `InventoryCatalog`, `PlayerInventoryTemplate` — "read-only metadata / starting
  loadout rebuilt from the resolved plugin index" — **accurate**
  (`byroredux/src/inventory.rs:52-80`, rebuilt by `install_catalog` on every
  content load; live `Inventory`/`EquipmentSlots` are saved).
- `SettingsPersistence`, `ActionBindings`, `ActionState`, `InteractionState`,
  `InteractionTrace`, `InjectedKeyPulse`, `InjectedKeyHold` — all
  process-session-local, **accurate**.

The seven 2026-08-05 gaps (`RigidBodyData` #2379, `RumbleOnActivate` #2382,
`Material` #2378, `FragmentExecutionQueue` #2381, and the cinematic trio #2380)
are all still registered — **no regression**.

## Findings

### HIGH

#### SAVE-D6-2026-08-16-01: `CurrentCellContext` is never cleared when the session leaves an interior, so an exterior save masquerades as an interior save and `load` reloads the departed cell
- **Severity**: HIGH
- **Dimension**: 6 — M45.1 Live Load-Apply
- **Data-Loss Class**: corruption-on-load (compounded by reference-break — every exterior delta is dropped by the remap)
- **Location**: `byroredux/src/cell_loader/load.rs:542-546` (the only insert site), `byroredux/src/cell_loader/transition.rs:322-332` (`unload_current_interior` — clears `CurrentCellRoot`, not `CurrentCellContext`), `byroredux/src/app_step.rs:758` (Interior→Exterior transition arm), `byroredux/src/save_io.rs:846-850` (`LoadCommand`'s "loose/exterior save" guard), `:905-908` + `:952-960` (the drain's reload target)
- **Status**: NEW
- **Description**: `CurrentCellContext` — the cell identity + plugin set a live load reloads before overlaying deltas — is inserted **only** by `load_cell_with_masters` and is **never removed**. `unload_current_interior` deliberately resets `CurrentCellRoot(None)` but leaves the context installed. The Interior→Exterior transition arm (`app_step.rs:758`) calls `unload_current_interior`, drains the streaming state and builds an exterior world context; the departed interior's `CurrentCellContext` survives all of it. From that point on the world is exterior but every save claims to be an interior save of the cell the player already left.

  `LoadCommand`'s guard (`"save has no cell context (loose/exterior save) — live load needs an interior cell"`) is the only thing standing between an unloadable save and the destructive drain, and it inspects presence, not currency — so the stale context passes it. `execute_pending_save_loads` then pre-flights and reloads the **wrong cell**, tears down the exterior streaming state, and finishes by applying the exterior-captured `PlayerPose` (worldspace coordinates, often tens of thousands of units) inside an interior.
- **Evidence**: `grep -rn "CurrentCellContext" byroredux/src` returns exactly one writer (`cell_loader/load.rs:542`) and no `remove_resource` call anywhere; `World::remove_resource` exists (`crates/core/src/ecs/world.rs:652`), so this is an omission, not a missing capability. `unload_current_interior`'s body ends at `world.insert_resource(CurrentCellRoot(None));` with no companion clear. The exterior entry point is `build_exterior_world_context` (`byroredux/src/cell_loader/exterior.rs:808`), which never touches `CurrentCellContext` — confirming a *pure* exterior session (`--grid`, no interior ever loaded) is still correctly rejected at queue time. The bug requires interior-first, which is precisely the P0/P1 reference route (interior → door → exterior) in `docs/engine/playable-vertical-slice.md`.
- **Impact**: Any save taken outdoors after an interior visit silently loads into the wrong world: the player is dropped into a sealed interior at exterior coordinates (out-of-bounds / falling), the exterior streaming state is destroyed, and every saved delta whose `FormIdPair` isn't in that interior is discarded by `build_form_id_remap` (logged, but as "cell content changed", masking the real cause). The on-disk save survives, but the live session does not, and `save.info` actively misreports the slot's cell.
- **Related**: `#2370` (EX-09/17 exterior transitions + save/load) is the OPEN epic for exterior *support*; this finding is not that gap — it is the currently-shipping interior path producing a wrong-world load, which the epic does not describe. Adjacent to SAVE-D6-02/#1697 (the pre-flight added so a bad reload can't strand the session): the pre-flight passes here, because the target cell is perfectly loadable — just wrong.
- **Suggested Fix**: Clear the resource where the interior stops being current: add `world.remove_resource::<CurrentCellContext>();` to `unload_current_interior` (alongside the existing `CurrentCellRoot(None)` reset), so every path that drops an interior — Interior→Exterior, `debug_load`'s NIF path, the save drain's own teardown — leaves no stale identity. Interior→Interior is unaffected because `load_cell_with_masters` re-inserts wholesale immediately after. Pin it with a test asserting the resource is absent after `unload_current_interior`, and a second asserting `LoadCommand` rejects the resulting exterior save.

#### SAVE-D2-2026-08-16-01: The `FORMAT_MAJOR` tripwire cannot see `#[cfg_attr(…, serde(default))]` — the house attribute form — and two live `serde(default)` fields already slipped past it
- **Severity**: HIGH
- **Dimension**: 2 — Registry & (De)serialization Fidelity
- **Data-Loss Class**: corruption-on-load (silent default-fill of an older save; blast radius today is bounded, the unenforced gate is not)
- **Location**: `byroredux/src/save_io/serde_default_guard_tests.rs:83-113` (`serde_attr_declares_default`), `:132-165` (the guard test); live violations at `crates/scripting/src/quest_stages.rs:105` and `:108` (`QuestStageData.status`, `.active`), plus `:79` and `crates/scripting/src/scene/quest_alias.rs:88` (both `skip, default`)
- **Status**: NEW
- **Description**: `serde_attr_declares_default` opens with

  ```rust
  let Some(rest) = trimmed.strip_prefix("#[serde(") else {
      return false;
  };
  ```

  so it only ever inspects a **bare** `#[serde(...)]` attribute. But every save-participating type in `crates/core` and `crates/scripting` gates its serde behind a feature, and therefore writes its field attributes as `#[cfg_attr(feature = "save", serde(...))]` / `#[cfg_attr(feature = "inspect", serde(...))]`. That form returns `false` unconditionally. The guard's own five sibling unit tests all exercise the bare form, so the blind spot is invisible from the test file.

  This is not theoretical. `crates/scripting/src/quest_stages.rs` **is** in `SAVE_TYPE_SOURCES` (line 29) and **is** scanned, yet it carries:

  ```rust
  /// Running/stopped/completed state exposed by the Papyrus Quest API.
  #[cfg_attr(feature = "save", serde(default))]
  pub status: QuestStatus,
  /// Whether the quest is selected in the player's active journal.
  #[cfg_attr(feature = "save", serde(default))]
  pub active: bool,
  ```

  Both fields were added to the save-participating `QuestStageData` by `a844c26b` (2026-08-07). `FORMAT_MAJOR` has not changed since the format shipped (`git log -S"FORMAT_MAJOR: u16"` → one commit, `bd2d0de2`, the original M45 landing). `schema_fingerprint` hashes column *type keys* only, so a save written before that commit has an identical fingerprint, passes `decode`, and loads with `status = Running` / `active = false` regardless of what it actually held — the exact "silently downgraded" outcome the SAVE-D2-01 doc block on `FORMAT_MAJOR` says must be impossible.
- **Evidence**: `grep -rn "cfg_attr" crates/core/src crates/scripting/src byroredux/src | grep "serde(" | grep -v "derive("` returns exactly four hits — all four in save-participating types, all four invisible to the guard, two of them plain `default` (the dangerous half; the two `skip, default` pairs are deliberate and documented). `cargo test --bin byroredux save_io` passes 36/36 with these in the tree. The prior cycle's SAVE-D2-19 evidence line (`grep -n "serde(default\|#\[serde"` on the six then-missing files returned nothing) reached the same false-clean conclusion from the same matcher assumption.
- **Impact**: The single automated enforcement of the *only* sanctioned schema-evolution path is inert for the attribute form the codebase actually uses. Any future `#[cfg_attr(…, serde(default))]` on a saved struct — including one whose default is semantically wrong (a quest silently un-completing, a lock silently re-opening) — ships green. Today's concrete exposure is limited to saves written before 2026-08-07 loading with reset quest status/active flags.
- **Related**: SAVE-D2-2026-08-16-02 below (the same guard's *file list* gaps); the historical `#1714` / `#2181` / `#2015` / `#2537` chain — this is the fourth distinct escape from the same guard, and the first in the matcher rather than the scan list.
- **Suggested Fix**: Broaden the matcher to accept an optional `#[cfg_attr(<pred>,` prefix before `serde(` (parse the attribute path, then apply the existing key-list logic to the inner `serde(...)` body), and add a unit test for the `cfg_attr` form beside the five existing ones. Then decide the two live `QuestStageData` fields explicitly: either bump `FORMAT_MAJOR` to 2, or record a one-line waiver at the fields stating the pre-`a844c26b` default-fill is acceptable — but not leave them unclassified.

### MEDIUM

#### SAVE-D1-2026-08-16-01: The P2 combat slice made component *removal* a gameplay transition, which the additive-only live overlay structurally cannot replay — a killed NPC reloads standing, animating and AI-capable
- **Severity**: MEDIUM
- **Dimension**: 1 — Snapshot Completeness & Determinism
- **Data-Loss Class**: silent-drop (the removal delta, not the component data)
- **Location**: `byroredux/src/combat.rs:215-241` (the kill branch), `:294-311` (`disable_actor_ai`), `crates/save/src/driver.rs:265-275` (`apply_deltas`'s additive-only contract), `byroredux/src/save_io.rs:83-129` (`MUTABLE_DELTA_COLUMNS`)
- **Status**: NEW — this invalidates the *premise* of the deferred-documented note at `crates/save/src/driver.rs:265-275` (#1847 / SAVE-04), which the audit skill explicitly says to re-flag "once such a component lands without the promised companion despawn/hide pass". It has landed.
- **Description**: `apply_deltas`'s docstring states the additive-only overlay is "a latent gap, not an active bug: nothing regresses today", because "there is currently no enable/disable/delete persistence mechanism to overlay in the first place (no `Disabled`/`Deleted` marker component exists)". The P2 combat slice broke that premise from the other direction: death is expressed as **removals**, not as a marker. `combat_damage_system`'s kill branch inserts `Dead`, then calls `disable_actor_ai`, which removes sixteen components (`SandboxBehavior`, `Seated`, `WanderBehavior`/`WanderState`, `FollowBehavior`/`FollowState`, `TravelBehavior`/`TravelState`/`Traveled`, `EscortBehavior`/`EscortState`/`Escorted`, `GuardBehavior`/`GuardState`, `PatrolBehavior`/`PatrolState`), removes the skeleton root's `AnimationPlayer`, and activates a ragdoll.

  On a live load the reloaded cell respawns the NPC intact — behaviors, `AnimationPlayer`, upright skeleton — and the overlay can only *add*. `Dead` and the zeroed Health both come back (both are delta columns), but nothing removes the AI or the animation player, and `RagdollActive` is deliberately not saved. The result is an actor that is simultaneously `Dead`, at zero health, upright, playing idles, and — with any of the seven `BYRO_*` AI env gates set — walking its package again. No system re-derives the teardown: the only readers of `Dead` are `combat_damage_system`'s own skip check, `ConditionFunction::GetDead` (`crates/scripting/src/condition.rs:499`) and the `ALIAS_FLAG_ALLOW_DEAD` filter (`crates/scripting/src/scene/quest_alias.rs:490`).
- **Evidence**: `apply_deltas` applies rows through `ApplyFn`'s `insert_batch` only — there is no removal path in `crates/save/src/registry.rs`'s closure trio. `restore_world` (clear + repopulate) *does* reproduce removals correctly, but it has zero production callers (`grep -rn "restore_world" byroredux/src` → tests only), so the correct path is unreachable in a real session. No round-trip test names `EquippedWeapon`, `Dead` or `ActorVitals`.
- **Impact**: The one gameplay loop the P2 gate is built around (attack → hit → health → death) does not survive a live reload in a coherent state. It is not progress *loss* — the kill is recorded — but the reloaded world is internally inconsistent in a player-visible way, and it is the concrete instance of the failure mode `#1847` deferred.
- **Related**: `#1847` / SAVE-04 (the deferred additive-only note); `docs/engine/playable-vertical-slice.md` P5 ("Extend change-form save coverage only for mutable state introduced by P0–P4").
- **Suggested Fix**: Add the companion despawn/hide pass `#1847` anticipated, run after `apply_deltas` and keyed by the same `remap`: for each entity the overlay marked `Dead`, re-run the death teardown (`disable_actor_ai` + `AnimationPlayer` removal + ragdoll activation) rather than trying to persist sixteen absences. That keeps the save format additive while making the loaded world consistent, and it generalises to the next removal-shaped transition. Pin it with a round-trip test that kills an actor, live-loads, and asserts the reloaded actor has `Dead` and no `WanderBehavior`.

#### SAVE-D4-2026-08-16-01: Three more saved `EntityId` carriers are invisible to every pre-write gate — `#2535` covered only the two cinematic types
- **Severity**: MEDIUM
- **Dimension**: 4 — Validation Gates
- **Data-Loss Class**: reference-break
- **Location**: `crates/core/src/ecs/components/follow.rs:59-61` (`FollowState.target_entity`), `crates/core/src/ecs/components/escort.rs:74-77` (`EscortState.target_entity`), `crates/core/src/ecs/components/sandbox.rs:54-57` (`Seated.furniture`); registered at `byroredux/src/save_io.rs:265-270`; `crates/save/src/validate.rs` and `byroredux/src/save_io.rs:577-616` (no check touches any of them)
- **Status**: NEW
- **Description**: `#2535` / SAVE-D4-02 added `validate_cinematic_entity_refs` for `HorseTetherState.horse` and `ActorCinematicState.vehicle` — a bespoke check for two types rather than the generic "any component carrying a bare `EntityId`" sweep its own suggested fix proposed. Three registered components carry the identical hazard and were left out, even though they were already known: `MUTABLE_DELTA_COLUMNS`'s doc comment names all three explicitly as excluded from the overlay *because* they carry `EntityId` fields. So the codebase knows about them for delta safety but not for validation.

  A save whose `Seated.furniture` (or either `target_entity`) points at an id `>= next_entity` — the actor's furniture despawned mid-session while `Seated` survived — passes all six pre-write checks silently and is written.
- **Evidence**: `validate_hierarchy` walks only `Parent`/`Children`; `validate_equipment` only `EquipmentSlots`↔`Inventory`; `validate_animation` only `AnimationPlayer`; `validate_inventory_instances` only `Inventory.items[].instance`; `validate_form_ids` only `FormIdComponent`; `validate_cinematic_entity_refs` only the two cinematic types. Consumption is defensive (the M42 procedure systems `get()` and fall through), which caps this at MEDIUM rather than HIGH — the same reasoning `#2535` used.
- **Impact**: The gate's stated thesis — that it sees every reference class that can go stale — remains untrue, now for five types rather than two. The `restore_world` path restores those dangling references verbatim with the same blind spot in its post-load diagnostic.
- **Related**: `#2535` (the two-type predecessor); SAVE-D4-2026-08-16-02 below.
- **Suggested Fix**: Take `#2535`'s own deferred suggestion — replace the bespoke check with a single `validate_entity_refs` that enumerates every component known to carry a bare `EntityId` and flags `id >= next_entity`, seeded with all five types. New `EntityId`-bearing components then slot into one list instead of each needing a new function.

#### SAVE-D4-2026-08-16-02: `EquippedWeapon.inventory_index` is a new save-participating intra-entity reference with no validation, while its structural twin `EquipmentSlots.occupants` has a dedicated check
- **Severity**: MEDIUM
- **Dimension**: 4 — Validation Gates
- **Data-Loss Class**: reference-break
- **Location**: `crates/core/src/ecs/components/inventory.rs:142-146` (`EquippedWeapon`), registered `byroredux/src/save_io.rs:250` and overlaid `:101`; `crates/save/src/validate.rs:184-218` (`validate_equipment`, which checks only `EquipmentSlots`)
- **Status**: NEW
- **Description**: `EquippedWeapon { inventory_index: InventoryIndex, base_form_id: u32, damage: f32 }` is the P2 slice's live weapon binding, written at `byroredux/src/npc_spawn.rs:784` and `byroredux/src/inventory.rs:229`. Its `inventory_index` indexes the *same* entity's `Inventory` — structurally identical to `EquipmentSlots.occupants`, the reference `validate_equipment` exists to check. Nothing validates it, on either the write or the post-load diagnostic path. Both `EquippedWeapon` and `Inventory` are delta columns, so a live load overlays them together and self-consistently; the exposure is a save taken while the index is already stale (an item consumed/removed from `Inventory` without the weapon binding being refreshed).
- **Evidence**: `validate_equipment` iterates `slots.occupants` only. Production consumption is `attack_damage` (`byroredux/src/combat.rs:269-273`), which reads `.damage` and never dereferences `inventory_index` — so nothing panics or mis-indexes today, which is what caps this at MEDIUM. The only site that does deref it is `byroredux/src/npc_spawn/tests.rs:321`.
- **Impact**: A defense-in-depth hole in the newest gameplay-state component, in exactly the reference shape the gate already models for its sibling. It becomes a live corruption-on-load the moment a consumer starts resolving the index (the natural next step for loot/ammo/condition).
- **Related**: SAVE-D4-2026-08-16-01 (same dimension, adjacent class); the "corpse loot" item still open in `docs/engine/playable-vertical-slice.md`, which will introduce exactly such a consumer.
- **Suggested Fix**: Extend `validate_equipment` to also resolve `EquippedWeapon.inventory_index` against the same entity's `Inventory.items.len()`, reusing the existing `None`-Inventory / out-of-bounds error split so the two cases stay distinguishable.

#### SAVE-D2-2026-08-16-02: `SAVE_TYPE_SOURCES` is missing two defining files and points at the wrong module for a third — fourth recurrence of the `#2015` class
- **Severity**: MEDIUM
- **Dimension**: 2 — Registry & (De)serialization Fidelity
- **Data-Loss Class**: none today (latent — the same mechanism as `#2015` / `#2537`, currently untriggered)
- **Location**: `byroredux/src/save_io/serde_default_guard_tests.rs:14-54` (`SAVE_TYPE_SOURCES`)
- **Status**: NEW
- **Description**: The list's own doc says it must carry the defining file of every save-participating type "AND the types nested inside them". Cross-checking it against `build_save_registry` at HEAD finds:
  - **Missing**: `crates/core/src/ecs/components/actor_state.rs` — defines `Dead`, registered at `byroredux/src/save_io.rs:251`. Never scanned.
  - **Wrong path**: the list carries `"../crates/scripting/src/scene.rs"` commented "QuestAliasInjectionState grant ledger", but that type is defined at `crates/scripting/src/scene/quest_alias.rs:85`. The scanned file merely constructs it (`scene.rs:130-131`). The guard scans a module that defines none of the type it is there to protect — and the sub-module it should be scanning holds one of the four `cfg_attr` attributes from SAVE-D2-2026-08-16-01.
  - **Missing nested**: `crates/scripting/src/translate/effects.rs` (`Effect`, `ActorRef`, `QuestRef`) and `crates/plugin/src/esm/records/script_instance.rs` (`ScriptInstanceData`) — both nested inside `PendingFragmentExecution` (`crates/scripting/src/fragment.rs:113-120`), which is the payload of the registered `FragmentExecutionQueue`. `Effect` is a high-churn enum edited whenever a Papyrus effect lands, i.e. precisely where a `serde(default)` would appear.
- **Evidence**: `grep -rn "pub struct QuestAliasInjectionState" crates/scripting/src` → `scene/quest_alias.rs:85`, not `scene.rs`. `crates/core/src/ecs/components/mod.rs:43` → `pub use actor_state::Dead`. `crates/scripting/src/fragment.rs:113-120` shows `vmad: Option<ScriptInstanceData>` and `effects: Vec<Effect>` inside the serde-derived `PendingFragmentExecution`.
- **Impact**: Even after SAVE-D2-2026-08-16-01's matcher is fixed, four files' worth of saved types stay outside the scan. The `#2537` fix added six files by hand and the list drifted again within one cycle — the hand-maintained shape is the root cause.
- **Related**: `#2015`, `#2181`, `#2537` (the same list drifting); SAVE-D2-2026-08-16-01 (the same guard's matcher).
- **Suggested Fix**: Add the two missing files and correct the `scene.rs` entry to `scene/quest_alias.rs`. Then take the prior cycle's own deferred recommendation and replace the hand list with the recursive `SCAN_ROOTS` + `collect_rs_files` walk the sibling `#2295` guard already uses, so a new saved type can't be missed by omission.

#### SAVE-D6-2026-08-16-02: Save and load have no non-console entry point — no action binding, no CLI flag, no menu item
- **Severity**: MEDIUM
- **Dimension**: 6 — M45.1 Live Load-Apply
- **Data-Loss Class**: none (surface/coverage gap)
- **Location**: `byroredux/src/commands/mod.rs:113-115` (the only registration of `SaveCommand`/`SaveInfoCommand`/`LoadCommand`), `byroredux/src/interaction.rs:51-64` (`InputAction` — no Save/Load/Quicksave variant), `byroredux/src/cli_args.rs` (no `--load`)
- **Status**: NEW
- **Description**: M45's entire user surface is three console commands reachable only through `byro-dbg` on port 9876. `InputAction` has ten configurable actions (movement, jump, sprint, activate, attack, block, inventory) and no save or load; there is no `--load <slot>` launch flag and no cold-boot load path; the native game menu behind `byroredux/src/settings_io.rs` persists settings but exposes no save/load entry. `docs/engine/playable-vertical-slice.md` acceptance criterion 1 requires that "`byro-dbg` is not required to move, interact, fight, navigate UI, or **save/load**", and criterion 5 requires state to survive "save → process exit → reload".
- **Evidence**: `grep -n "enum InputAction" -A 14 byroredux/src/interaction.rs` lists ten variants plus the dead `Pause`; no save/load. `grep -rn "SaveCommand\|LoadCommand" byroredux/src` returns only `save_io.rs` (the definitions) and `commands/mod.rs:113-115`.
- **Impact**: The subsystem functionally works but is unreachable to a player, so the P5 persistence gate cannot be closed on the current surface however complete the format becomes. It also means the only exercise the load path gets is manual operator input, which is why removal-replay (SAVE-D1-2026-08-16-01) and stale-context (SAVE-D6-2026-08-16-01) went unobserved.
- **Related**: `docs/engine/playable-vertical-slice.md` §P5; SAVE-D6-2026-08-16-01 (the load path this would expose).
- **Suggested Fix**: Add `Quicksave`/`Quickload` to `InputAction` with default F5/F9 bindings routed through the existing `ActionState` edge machinery, invoking the same `SaveCommand`/`LoadCommand` bodies (extract them into plain functions so the console command and the action share one implementation). A `--load <slot>` launch flag is the cheaper half of criterion 5 and needs only to enqueue `PendingSaveLoadSlot` after boot.

### LOW

#### SAVE-D1-2026-08-16-02: `ActorVitals` is the only registered component excluded from `MUTABLE_DELTA_COLUMNS` with no recorded reason, and it is missing from the NPC-spawn-stamped guard list
- **Severity**: LOW
- **Dimension**: 1 — Snapshot Completeness & Determinism
- **Data-Loss Class**: none (write-once; the risk is convention decay, not lost state)
- **Location**: `byroredux/src/save_io.rs:249` (registration, no comment), `:83-129` (`MUTABLE_DELTA_COLUMNS`, absent), `byroredux/src/save_io/round_trip_tests.rs:737-748` (`NPC_SPAWN_STAMPED`, absent), `byroredux/src/npc_spawn.rs:112` (the stamp site)
- **Status**: NEW
- **Description**: Every other registered-but-not-overlaid component carries an inline rationale at its registration site — `Material` (#2378 blast radius), the cinematic pair (#2380 `EntityId`), `AnimationPlayer`/`AnimationStack` (#1696), `FollowState`/`EscortState`/`Seated` (`EntityId`). `ActorVitals` was registered with a bare `.register_component::<ActorVitals>("ActorVitals")` and no note. It is in fact safe — it holds a single `health: u32` AVIF FormID stamped once at `npc_spawn.rs:112`, with no `query_mut`/`get_mut` site anywhere — but a reader auditing the two-list drift has no way to tell that from a forgotten entry, which is exactly the check the Dimension 1 checklist asks for. Separately, `NPC_SPAWN_STAMPED` in the `#1835` guard lists ten spawn-stamped components and omits `ActorVitals`, so that guard's own premise ("every component `spawn_npc_entity` stamps") has drifted.
- **Evidence**: `grep -rn "query_mut::<ActorVitals>\|get_mut::<ActorVitals>"` over the tree returns nothing; the only writer is `npc_spawn.rs:112`. `crates/core/src/ecs/components/actor_values.rs:77-81` shows the one-field struct.
- **Impact**: None at runtime. The cost is that the ledger's "documented exclusion" convention — the thing that makes a save-but-never-replay drift detectable by reading — now has a hole in it.
- **Related**: `#1835` (the NPC-spawn-stamped guard); the `MUTABLE_DELTA_COLUMNS` doc block.
- **Suggested Fix**: One comment line at `save_io.rs:249` stating `ActorVitals` is a write-once AVIF-FormID stamp with no runtime mutator, hence full round-trip only; and add `"ActorVitals"` to `NPC_SPAWN_STAMPED` (it is registered, so the XOR assertion passes unchanged).

#### SAVE-D6-2026-08-16-03: `docs/engine/save-load-roundtrip.md` — the subsystem's authoritative cross-cutting trace — is stale in three places
- **Severity**: LOW
- **Dimension**: 6 — M45.1 Live Load-Apply
- **Data-Loss Class**: none (doc rot)
- **Location**: `docs/engine/save-load-roundtrip.md:42-47` (§2), `:62-70` (§3), `:141-147` (§6)
- **Status**: NEW
- **Description**: The doc self-certifies "verified against the tree as of 2026-07-15" and three of its factual claims have since drifted:
  - §2 "today 10+ components … and gameplay resources" — the registry now holds 33 components and 10 resources.
  - §3 "`validate_world` checks four invariants … plus a binary-side `validate_form_ids`" — there are now **two** binary-side checks; `validate_cinematic_entity_refs` (`byroredux/src/save_io.rs:577`) was added by `#2535` and runs in the same pre-write and post-load groups.
  - §6 step 6 lists the delta set as "`Transform`, `Inventory`, `EquipmentSlots`, `LightSource`, `LightFlicker`, `ScriptTimer`, `ActorValues`" — `MUTABLE_DELTA_COLUMNS` now has 20 entries.
  Line-number citations elsewhere in the doc (e.g. `save_io.rs:378`, `:589`) also predate the `#2407` test split that moved production code, though the named symbols all still resolve.
- **Evidence**: counted directly from `byroredux/src/save_io.rs:83-129` and `:207-361`; `validate_cinematic_entity_refs` is called at `:669` and `:1014`.
- **Impact**: The doc is named in `_audit-common.md` as the authoritative reference for this subsystem, so a reader trusting it undercounts both the saved surface and the validation surface by roughly 3×.
- **Related**: `docs/feature-matrix.md`'s `TD3-002` comment was re-verified and reads correctly — **not** re-flagged, per the skill's explicit instruction.
- **Suggested Fix**: Refresh the three passages with counts rather than enumerations where possible (an enumeration of 20 columns will rot again), re-date the currency note, and cite `MUTABLE_DELTA_COLUMNS` / `build_save_registry` by symbol instead of restating their contents.

## Regression Guards Discovered / Reconfirmed

| Guard | Location | Invariant pinned | State |
|---|---|---|---|
| `every_component_or_resource_impl_is_saved_or_explicitly_allowlisted` | `save_io/registry_completeness_tests.rs:75` | every `impl Component`/`impl Resource` under 4 scan roots is registered XOR allowlisted with a reason | green; `SCAN_ROOTS` now includes `../byroredux/src` (prior SAVE-D1-18 fixed) |
| `delta_columns_carry_only_session_stable_fields` | `save_io/round_trip_tests.rs:28` | `MUTABLE_DELTA_COLUMNS` == a hand-audited list; forces review on every addition | green; `EquippedWeapon`/`Dead` correctly added with rationale |
| `npc_spawn_stamped_components_are_saved_or_intentionally_rederived` | `save_io/round_trip_tests.rs:732` | NPC-spawn state is saved XOR documented re-derived | green, but its list has drifted — see SAVE-D1-2026-08-16-02 |
| `serde_default_on_saved_struct_requires_format_major_bump` + 5 matcher unit tests | `save_io/serde_default_guard_tests.rs:132` | no `#[serde(default)]` on a saved struct while `FORMAT_MAJOR == 1` | green **over a live violation** — SAVE-D2-2026-08-16-01 |
| `form_id_column_resolves_the_flagged_entry`, `..._is_none_without_registration`, `registering_a_second_form_id_column_panics` | `crates/save/src/registry.rs:377-405` | remap key comes from the explicit `is_form_id` flag, at most one column | green (#1845 intact) |
| `rejects_bad_magic` / `rejects_truncated` / `rejects_payload_truncation` / `detects_crc_corruption` / `rejects_schema_mismatch` / `rejects_major_version_skew` | `crates/save/src/snapshot.rs:186-258` | every container gate precedes `serde_json::from_slice`; CRC covers payload only | green |
| `parse_slot_names`, `cursor_after_newest_points_past_latest_mtime`, `resume_on_empty_dir_starts_at_zero`, `write_read_round_trip_and_atomic_rename`, `ring_wraps`, `ring_size_floored_to_one` | `crates/save/src/disk.rs:183-252` | strict slot parsing, resume-past-newest, atomic rename with no leftover tmp | green |
| `dangling_item_instance_is_rejected`, `item_instance_without_pool_is_rejected`, `live_item_instance_passes`, `stackable_item_without_instance_is_clean` | `crates/save/src/validate.rs:310-379` | `ItemStack.instance` resolves in `ItemInstancePool` before write | green |
| `dangling_horse_tether_reference_is_rejected`, `dangling_cinematic_vehicle_reference_is_rejected` (+2 positive) | `save_io/validation_gate_tests.rs` | the two cinematic `EntityId` refs are gated | green — but only those two, see SAVE-D4-2026-08-16-01 |
| `player_pose_character_tracks_body`, `player_pose_flycam_saved_relocates_body_in_live_character_mode`, `player_pose_round_trips_flycam`, `player_pose_survives_snapshot_round_trip` | `save_io/live_reload_tests.rs` | pose restore across both modes, momentum clear, no-handle no-op | green (#2018 intact) |
| `quicksave_ring_cursor_does_not_advance_on_validation_abort`, `second_load_before_drain_supersedes_and_reports` | `save_io/command_queue_tests.rs` | ring rotation only on committed writes (#2017); supersede is reported (#1848) | green |

## Verified Clean — No New Findings

- **Dimension 3 (Disk)** in full: the write dance ordering (`create_dir_all` → tmp write → `flush` → `sync_all` → byte-exact read-back → `rename` → parent-dir fsync), the tmp cleanup on a failed read-back, the single write path in the process, decode gate ordering, CRC scope, strict slot-name parsing, and `SaveRing::resume`. Fifth consecutive clean cycle.
- **Dimension 5 (Frame boundary)** in full: read-only capture on the exclusive `DebugDrainSystem` lane, the `SaveState` guard dropped before the storage walk (#2154 intact), `capture_player_pose` post-scheduler and pre-drain, the `take()`-once drain, and — structurally the strongest result — `restore_world` having **zero production callers**, which makes the two-restore-path id-collision hazard unreachable.
- `next_entity` bounds: `validate_entity_ids_in_bounds` (`crates/save/src/driver.rs:76-96`) is a real `Result` check that runs before any mutation, not a `debug_assert` — the prior cycles' release-mode-silence concern stays closed.
- `StringPool::dump` / `from_dump` (`crates/core/src/string/mod.rs:102-131`): indexes by symbol explicitly and panics loudly on a gap, so the CRITICAL "every `FixedString` points at the wrong symbol" class is structurally prevented.
- Determinism: both `register_component` and `register_form_id_component` sort rows by entity id before serialising, so the reproducible-CRC claim holds at row level, not just column level.
- Live-load ordering, remap semantics, idempotency, the `validate_cell_loadable` pre-flight (#1697), and the `PLAYER_FORM_ID_PAIR` attachment at `byroredux/src/scene.rs:1283-1291` — all re-verified unchanged and correct.
- P2 inventory continuity specifically: `apply_action` (`byroredux/src/inventory.rs:319-368`) unequips by *mutating* `EquipmentSlots.occupants` in place rather than removing a component, so equip/unequip state does survive the additive overlay. This is the counterexample that keeps SAVE-D1-2026-08-16-01 scoped to death rather than to the whole slice.

## Disproved Candidates (investigated, not filed)

- **"An exterior-mode session can save an unloadable slot."** Rejected: a pure `--grid` session never sets `CurrentCellContext`, and `LoadCommand` refuses at queue time with a clear message. The real defect is the *stale* context (SAVE-D6-2026-08-16-01), which is the opposite failure. Broader exterior save/load support is tracked by OPEN `#2370` and is not re-filed here.
- **"`ItemInstancePool` restored wholesale dangles instance ids allocated during the reload."** Rejected: no production site allocates from the pool (`grep -rn "\.allocate("` → only `SkinSlotPool` in `byroredux/src/render/skinned.rs:93`), and `cell_loader/unload.rs:381-410` releases slots on unload. No live exposure.
- **"The saved player pose is stale by one frame."** Confirmed as behaviour but not filed: `capture_player_pose` runs post-scheduler while the console drain that executes `save` runs *inside* it, so a save records the previous frame's end-of-tick pose. Post-propagation, self-correcting, ≈6 world units at sprint speed — below the body capsule radius, and no data is lost.
- **"`build_save_registry()` being rebuilt inside `execute_pending_save_loads` (`save_io.rs:910`) can drift from the installed `SaveRegistry` resource."** Rejected: both come from the same function, so the fingerprint and column set are identical by construction. It is a small avoidable allocation, not a correctness issue, and the identical pattern exists in `step_cell_transition`.
- **"The read-back verification in `write_slot` is defeated by the page cache."** Rejected as a finding: true of every read-back-after-fsync implementation, and the code's own comment scopes the claim correctly to "a lying filesystem / short write". No better option exists without `O_DIRECT`.
- **"`docs/feature-matrix.md`'s Save/load row is stale."** Rejected — the `TD3-002` comment at line 226 is present and reads correctly; the skill explicitly says not to re-flag it. Verified rather than assumed.

## Deduplication

`/tmp/audit/issues.json` (269 OPEN issues) searched for `save`, `load`, `snapshot`,
`corrupt`, `formid`, `delta`, `serde`, `cfg_attr`, `SAVE_TYPE`, `exterior`,
`dead`, `equippedweapon`, `pose`, `ring`, `fingerprint`, `validate`, `entityid`,
`combat`, `overlay`, `removal`, `despawn`, `keybind`, `quicksave`. Four
save-adjacent OPEN issues exist and none overlaps a finding above:

| Issue | Why not a duplicate |
|---|---|
| `#2370` EX-09/17 exterior transitions, save/load, load-order conformance | scopes *adding* exterior save/load; SAVE-D6-2026-08-16-01 is the shipping interior path producing a wrong-world load |
| `#2947` CHAR-D3-08 `CharacterLevel`/`Perks` save-exempt | a different allowlist entry, unchanged this cycle and correctly classified |
| `#2687` SAFE-D9-01 save-restore is a `Material` producer that skips `resolve_pbr` | renderer-side consequence of `Material` restore, owned by `/audit-safety` |
| `#2670` SCR-D6-NEW11-05 the SAVE-D6-01 rekey drops a grant without `SceneAliasCandidate` | the follow-on to last cycle's `QuestAliasInjectionState` fix, owned by `/audit-scripting` |

All seven prior `docs/audits/AUDIT_SAVE_*.md` reports were scanned. Prior-cycle
findings SAVE-D1-18 (`SCAN_ROOTS` missing `byroredux/src`) and SAVE-D2-19
(`SAVE_TYPE_SOURCES` missing six files) are both **confirmed fixed** — the
former completely, the latter for those six files, though the list has since
drifted again on four different files (SAVE-D2-2026-08-16-02, filed as a new
recurrence rather than a regression, since the specific six were correctly
added). SAVE-D4-02 (#2535) is confirmed fixed for the two cinematic types;
SAVE-D4-2026-08-16-01 extends the same class to three more.

---

Suggested follow-up: `/audit-publish docs/audits/AUDIT_SAVE_2026-08-16.md`
