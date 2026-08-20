# Save / Load Subsystem Audit (M45 + M45.1) — 2026-08-20

Scope: `crates/save/src/` (6 modules, 1,949 LOC — `validate.rs` grew 388→603
this cycle) plus the engine-side consumer `byroredux/src/save_io.rs` (1,076 LOC
production) and its six `byroredux/src/save_io/*_tests.rs` siblings. Audited at
HEAD `bb0b92f2`. This is the **ninth** save audit (prior: `2026-06-23`,
`2026-07-02`, `2026-07-03`, `2026-07-16`, `2026-07-25`, `2026-08-03`,
`2026-08-07`, `2026-08-16`).

Two things dominate this cycle. First, **seven of the nine 2026-08-16 findings
were fixed** — the largest single-cycle closure this subsystem has had
(`8a56a2b6` "make persistence truthful and player-facing", `219e876c`, `3a39ca47`).
Second, the delta's **335 commits of WATAL water work** added a large component
surface (`WaterMaterial` 18→63 fields, `WaterContact`, `WaterFlow`,
`WaterCurrentVolume`, `WaterLodInfo`, `WaterNoiseMapHandles`, `SubmersionState`,
`WaterAudioConfig/State`, `SplashEvent`, `RippleEvent`) plus drowning. All of it
was correctly classified — the completeness guard is green and every new water
allowlist reason was spot-checked against live mutator sites and holds.

The findings that survived are therefore **not** in the water surface. They are
in the two places the delta moved fastest and the guards do not reach:
`FORMAT_MAJOR` discipline (two required fields landed on saved structs after the
last bump, and the only automated guard is structurally blind to that half of
the footgun), and the brand-new player-facing F5/F9/menu/`--load` entry points
(which discard the validation gate's abort message into `log::info!`).

Per the suite briefing, `cargo` was not run. All conclusions are static reads of
the tree at HEAD plus Python re-implementations of the two guard algorithms
(`save_type_sources`, `every_component_or_resource_impl_is_saved_or_explicitly_allowlisted`)
executed against the live source — noted per finding where used.

## Executive Summary

`crates/save/src/lib.rs` docstring claims verified against live code:

| Claim | Status |
|---|---|
| Full ECS snapshot (curated game-state set) | **CODE-CONFIRMED** — 33 components + 10 resources, unchanged in count from 2026-08-16; the SAVE-D1-12/#2295 guard is green. The additive-only live-overlay caveat that made this DRIFTED last cycle is **closed**: `apply_deltas`'s docstring now names the marker-plus-reconciler contract and `reconcile_dead_actor_runtime_state` is wired into the drain (#3022). |
| Atomic write (tmp → fsync → read-back → rename → dir-fsync) | **CODE-CONFIRMED** — `crates/save/src/disk.rs` has zero production diff since 2026-08-03 (only `latest_slot` was added, read-only). **Sixth consecutive clean verification.** |
| Ring never clobbers the last good save | **CODE-CONFIRMED** — `SaveState::new` still calls `SaveRing::resume`; `SaveCommand` still `peek`s before validation and `advance`s only after it. |
| Validation gate refuses to persist inconsistent state | **CODE-CONFIRMED and materially widened** — `validate_world` now runs SIX checks (hierarchy, equipment incl. `EquippedWeapon` index *and* base-FormID cross-check, saved-`EntityId` references, animation, item instances, progression) plus TWO binary-side (`validate_form_ids`, `validate_cinematic_entity_refs`). Both 2026-08-16 coverage findings are fixed. **But the gate's ABORT is now invisible on the player-facing paths** — SAVE-D4-2026-08-20-01. |
| Off-frame load, never inside the scheduler | **CODE-CONFIRMED** — `restore_world` still has zero production callers. The two new save call sites (`window_event`, `render_one_frame`'s `apply_debug_ui_outputs`) are both between-ticks; see *Cross-audit dispositions* below for the lock-invariant half. |
| `FORMAT_MAJOR` bump is the only sanctioned schema evolution path | **DRIFTED.** The `serde(default)` half of the guard was properly fixed this cycle (matcher widened to `cfg_attr`, discovery derived, 1→4 bumps taken). The **required-field half was never guarded at all**, and two required fields landed on registered saved structs *after* the last bump. SAVE-D2-2026-08-20-01. |

**Findings this cycle: 8. 0 CRITICAL, 2 HIGH, 3 MEDIUM, 3 LOW.**

By Data-Loss Class: **corruption-on-load — 2** (both involve the same
schema-drift path); **irrecoverable-write — 1** (HIGH); **silent-drop — 2**;
**none / process — 3**.

## Data-Loss Class Matrix

| Finding | Class | Dimension | Severity |
|---|---|---|---|
| SAVE-D4-2026-08-20-01 (F5 / F9 / pause-menu / `--load` discard `CommandOutput`; an aborted save is indistinguishable from a written one) | irrecoverable-write | 4 — Validation Gates | HIGH |
| SAVE-D6-2026-08-20-01 (`apply_deltas` mid-column `Err` leaves a partially-overlaid world after an irreversible teardown, skips the dead-actor reconciler, logs only) | corruption-on-load | 6 — Live Load-Apply | HIGH |
| SAVE-D2-2026-08-20-01 (two required fields added to registered saved structs after the last `FORMAT_MAJOR` bump; the guard cannot see required-field additions) | corruption-on-load | 2 — Registry & (De)serialization | MEDIUM |
| SAVE-D1-2026-08-20-01 (`CharacterController.breath_remaining` / `.drowning_damage_accumulator` unsaved under a stale allowlist reason, and the component's own doc claims the opposite) | silent-drop | 1 — Completeness | MEDIUM |
| SAVE-D1-2026-08-20-02 (SAVE-D1-12 `SCAN_ROOTS` misses four directories; `LodCoverageStats` landed unclassified in this very delta) | silent-drop (latent) | 1 — Completeness | MEDIUM |
| SAVE-D2-2026-08-20-02 (serde-guard discovery holes: dead match prefix, unscanned nested type, line-bound matcher) | none (latent) | 2 — Registry & (De)serialization | LOW |
| SAVE-D6-2026-08-20-02 (zero test coverage for every new player-facing entry point) | none (test gap) | 6 — Live Load-Apply | LOW |
| SAVE-D6-2026-08-20-03 (`save-load-roundtrip.md` currency note re-dated to 2026-08-18 while §2/§3/§6 stayed stale) | none (doc rot) | 6 — Live Load-Apply | LOW |

## Per-Dimension Coverage

| Dimension | Findings | Notes |
|---|---|---|
| 1 — Snapshot Completeness & Determinism | **2** (2 MEDIUM) | Registry count unchanged; every new water type correctly classified; determinism, `next_entity`, `StringPool` symbol order all re-verified clean. Both findings are guard blind spots, not registry omissions. |
| 2 — Registry & (De)serialization Fidelity | **2** (1 MEDIUM, 1 LOW) | `registry.rs` has **zero diff** this cycle. FNV constants, `form_id_column` flag, `FormIdPair` handling all re-verified. The finding is the residual of the (correctly closed) #3020/#3025 pair. |
| 3 — Disk Format & Durability | **0** | Fully clean, **sixth** consecutive cycle. `latest_slot` (new) reviewed and correct. |
| 4 — Validation Gates | **1** (1 HIGH) | Gate placement, abort-before-write, single write path, dangling-id semantics and the two new checks all correct. The finding is that the abort has no player-visible channel. |
| 5 — Frame-Boundary Capture & Off-Frame Apply | **0** | Re-verified in full including the two NEW save call sites. See *Cross-audit dispositions*. |
| 6 — M45.1 Live Load-Apply | **3** (1 HIGH, 2 LOW) | Apply ordering, remap, idempotency, pre-flight, pose restore and the new `--load` boot queue all confirmed correct. The HIGH is in the failure path, not the happy path. |

## Completeness Ledger

`build_save_registry` (`byroredux/src/save_io.rs:208-361`): **33 components + 10
resources** (32 turbofish component registrations + `FormIdComponent`).
`MUTABLE_DELTA_COLUMNS` (`:84-130`): **20 columns**. Both counts are unchanged
from 2026-08-16 — **no two-list drift this cycle**; no registration was added or
removed by the delta.

| Column | Kind | Saved | Overlaid | Status |
|---|---|---|---|---|
| `Transform`, `Inventory`, `EquipmentSlots`, `LightSource`, `LightFlicker`, `ScriptTimer`, `TwoStateActivator`, `ScriptVariables`, `ActorValues`, `EquippedWeapon`, `Dead`, `WanderState`, `TravelState`, `Traveled`, `GuardState`, `PatrolState`, `Escorted`, `ActorControlState`, `RigidBodyData`, `RumbleOnActivate` | Component | yes | yes | SAVED+OVERLAID (20), pinned by `delta_columns_carry_only_session_stable_fields`. **`EquippedWeapon` and `RigidBodyData` each gained a field this cycle** — `reach`/`speed` (still delta-safe: plain `f32`) and `collidable` (delta-safe: `bool`). Both remain session-stable; the schema-version half is SAVE-D2-2026-08-20-01. |
| `Name`, `Parent`, `Children`, `FormIdComponent` | Component | yes | no | structural identity — correct by design |
| `AnimationPlayer`, `AnimationStack` | Component | yes | no (deliberate) | #1696 `EntityId`/`clip_handle` hazard, documented at the const, still accurate |
| `FollowState`, `EscortState`, `Seated` | Component | yes | no (deliberate) | `EntityId` hazard, documented — and as of this cycle **also covered by `validate_saved_entity_references`** (closes #3023) |
| `ActorCinematicState`, `HorseTetherState` | Component | yes | no (deliberate) | #2380, documented at the registration site, still accurate |
| `Material` | Component | yes | no (deliberate) | #2378 blast-radius, documented, still accurate. **Gained two required fields this cycle** — SAVE-D2-2026-08-20-01. Also the subject of OPEN #2687 (safety-owned). |
| `ActorVitals` | Component | yes | no (undocumented) | **OPEN #3027**, unchanged at HEAD (`save_io.rs:250` still a bare registration). Noted and skipped per protocol. |
| `ItemInstancePool`, `CurrentCellContext`, `PlayerPose`, `GameTimeRes`, `QuestStageState`, `QuestObjectiveState`, `QuestAliasInjectionState`, `PlayerControlState`, `FragmentExecutionQueue`, `CinematicPresentationState` | Resource | yes | n/a | replaced wholesale by `restore_resources`, which runs before `apply_deltas` — correct |

Cross-check against the `#2295` guard's `NOT_SAVED_BY_DESIGN` allowlist (green,
~173 entries; re-implemented in Python against the live tree — **0 unclassified,
0 double-classified**). Ten entries were added by this delta and every reason was
spot-checked against real mutator sites:

- `WaterFlow`, `WaterCurrentVolume`, `WaterVolume`, `WaterPlane`, `WaterLodInfo`,
  `WaterNoiseMapHandles` — reasons claim "no runtime mutator". **Verified**:
  `grep query_mut::<T>|get_mut::<T>` returns zero production hits for all six.
- `WaterContact` — "per-tick physics-derived output recomputed from body pose".
  **Verified** (`crates/physics/src/water.rs:432,802`,
  `byroredux/src/systems/water.rs:782` all recompute, none accumulate). Its
  trailing parenthetical *"drowning accumulation is not yet wired"* is now stale
  prose — drowning shipped in `byroredux/src/systems/character.rs` — but the
  reason's *conclusion* is unaffected because the accumulator lives on
  `CharacterController`, not here. That is exactly where SAVE-D1-2026-08-20-01
  picks it up.
- `SubmersionState`, `SplashEvent`, `RippleEvent`, `WaterAudioConfig/State`,
  `CombustionState`, `PrecombinedMesh`, `SpeedTreeWind`, `RenderDebugControl`,
  `Locked`, `InteractionCandidateScratch` — all **verified accurate**.

The seven 2026-08-05 gaps (`RigidBodyData` #2379, `RumbleOnActivate` #2382,
`Material` #2378, `FragmentExecutionQueue` #2381, cinematic trio #2380) are all
still registered — **no regression**.

## Findings

### HIGH

#### SAVE-D4-2026-08-20-01: the new player-facing save/load surface discards `CommandOutput` into `log::info!`, so a validation-aborted quicksave is indistinguishable from a written one
- **Severity**: HIGH
- **Dimension**: 4 — Validation Gates (surface owned by Dimension 6)
- **Data-Loss Class**: irrecoverable-write (the write that should have happened didn't, and the session has no signal)
- **Location**: `byroredux/src/app_events.rs:285-301` (F5/F9), `byroredux/src/main.rs:751-759` (pause-menu Quicksave/Quickload), `byroredux/src/main.rs:385-393` (`--load` boot queue); against `byroredux/src/save_io.rs:672-686` (the abort branch that builds the message) and `byroredux/src/main.rs:844-848` (the console scrollback the same function already writes to)
- **Status**: NEW — this is the *consequence* of the fix that closed #3026, not a re-report of it
- **Description**: #3026 closed by giving save/load a real player surface: `InputAction::Quicksave`/`Quickload` bound to F5/F9, two pause-menu buttons, and a `--load <slot>` launch flag. All four route through `save_io::quicksave` / `quickload_latest`, which correctly share `SaveCommand`/`LoadCommand`'s implementation — including the validation gate. What none of them share is the gate's **output**.

  `SaveCommand::execute` deliberately returns, rather than writes, on a non-empty issue list:

  ```
  "save ABORTED: {n} referential-integrity issue(s) — refusing to write a poisoned save:"
  ```

  followed by up to 20 issues. Every one of the four new call sites collapses that into
  `log::info!("player save action: {}", output.lines.join(" | "))` (or the
  `"pause menu quicksave: …"` equivalent). A player with no terminal sees the
  exact same thing on an abort as on a success: nothing. They press F5, hear and
  see no difference, and keep playing on progress that was never written. The
  ring cursor correctly does *not* advance (#2017), so the next F5 retries the
  same slot and fails the same way — silently, indefinitely.
- **Evidence**: `byroredux/src/app_events.rs:298-301` —
  ```rust
  if let Some(output) = save_output {
      log::info!("player save action: {}", output.lines.join(" | "));
      return;
  }
  ```
  `byroredux/src/main.rs:751-759` is the same shape. That this is an omission and
  not a missing capability is provable from the same function: 90 lines below,
  `apply_debug_ui_outputs` takes `debug_ui: Option<&mut DebugUiState>` and pushes
  console-eval responses through `ui.push_console_line(line)`
  (`main.rs:844-848`). The surface was in scope and unused.

  Reachability of the abort path is not hypothetical, and this cycle made it
  *more* reachable, not less: `validate_saved_entity_references` (new) aborts on
  a `Seated.furniture` / `FollowState.target_entity` / `EscortState.target_entity`
  pointing past `next_entity` — an actor seated on furniture that despawned
  mid-session; `validate_equipment` (widened) aborts on an `EquippedWeapon`
  whose `base_form_id` disagrees with `inventory[index]`; and
  `validate_progression_state` (new, #2947) aborts on **any** `CharacterLevel.xp != 0`,
  so the day a leveling runtime lands, F5 stops working for every player at once
  with zero feedback.
- **Impact**: The subsystem's entire thesis is "refuse to persist a poisoned save
  rather than seed a corruption tail." That refusal is correct and now
  well-covered — but on the only surface a player can reach, refusal and success
  are the same observable event. The failure mode is the one the ring design was
  built to prevent, inverted: instead of F5 eating the old save, F5 silently
  writes nothing at all. A secondary instance: `quickload_latest` picks the
  newest slot by mtime with no decode check and no fallback to the next-newest,
  so F9 against a corrupt or stale-`FORMAT_MAJOR` newest slot also fails to
  nothing.
- **Related**: #3026 (the closed issue that created this surface); #2017 (the
  ring-cursor half, which is correct); #2947 / `validate_progression_state` (the
  abort that will make this universal); SAVE-D6-2026-08-20-02 (no test covers
  any of these four entry points).
- **Suggested Fix**: Route the `CommandOutput` of all four call sites into a
  player-visible channel: at minimum `ui.push_console_line` for the pause-menu
  pair (the surface already exists in the same function) and a short on-screen
  toast / HUD line for F5/F9, with `log::warn!` rather than `log::info!` whenever
  `CommandOutput` is an error or its first line starts with `save ABORTED`. Give
  `quickload_latest` a decode-and-fall-back loop over `list_slots` in descending
  mtime order so a corrupt newest slot doesn't dead-end the key.

#### SAVE-D6-2026-08-20-01: a mid-column `apply_deltas` failure leaves the world partially overlaid *after* the irreversible teardown, skips the dead-actor reconciler, and reports only to the log
- **Severity**: HIGH
- **Dimension**: 6 — M45.1 Live Load-Apply
- **Data-Loss Class**: corruption-on-load
- **Location**: `crates/save/src/driver.rs:273-291` (`apply_deltas`'s `?` on a per-column `ApplyFn`), `byroredux/src/save_io.rs:1012-1029` (the drain's `match`), `:964-968` (the teardown that already ran), `byroredux/src/save_io.rs:84-130` (`MUTABLE_DELTA_COLUMNS`, the apply order)
- **Status**: NEW
- **Description**: `apply_deltas` iterates `MUTABLE_DELTA_COLUMNS` in declaration order and propagates the first `SaveError` with `?`:

  ```rust
  for &name in columns {
      …
      applied += apply(world, value.clone(), remap)?;
  }
  ```

  Columns *before* the failing one have already mutated the world through
  `insert_batch`. There is no transaction, no dry run, and no rollback — and by
  the time `apply_deltas` is called, `drain_streaming_state` +
  `unload_current_interior` + `load_cell_with_masters` have all already run, so
  the caller has nothing to fall back to either.

  The caller's handling makes it worse in three specific ways:
  1. The `Err` arm is `log::error!("save load: delta apply failed: {e}")` and then
     **falls through** — `validate_world`, `validate_form_ids` and
     `apply_player_pose` all still run, so the session ends up positioned and
     playable in a half-overlaid world rather than visibly broken.
  2. `reconcile_dead_actor_runtime_state(world)` sits in the **`Ok` arm only**
     (`save_io.rs:1013`). A failure therefore silently reverts the #3022 fix for
     that load: `Dead` may have been overlaid (it is column 11 of 20) while the
     AI/animation teardown it is supposed to imply never runs — the exact
     inconsistency #3022 was filed to remove.
  3. With SAVE-D4-2026-08-20-01, the `log::error!` is the *only* trace on a
     player-facing F9.

  Column order makes the blast radius concrete: `RigidBodyData` is 19th of 20, so
  a failure there applies `Transform`, `Inventory`, `EquipmentSlots`, `ActorValues`,
  `EquippedWeapon`, `Dead` and twelve more, then drops `RumbleOnActivate` and the
  reconciler.
- **Evidence**: `crates/save/src/driver.rs:280-289` — the `?` inside the loop with
  no accumulated-rollback state. `byroredux/src/save_io.rs:1012-1028` — the `Ok`
  arm holds `let dead = crate::combat::reconcile_dead_actor_runtime_state(world);`
  and the `Err` arm holds only the `log::error!`. `:1031-1058` — execution
  continues into `validate_world` and `apply_player_pose` regardless of which arm
  ran. The trigger is reachable at HEAD via SAVE-D2-2026-08-20-01
  (`RigidBodyData.collidable` is a required field with no `FORMAT_MAJOR` bump, so
  a `v4` snapshot written 2026-08-18 decodes cleanly and then fails
  `from_value` on that column).
- **Impact**: Every recoverable per-column deserialisation error becomes an
  unrecoverable half-applied live world. This is the one place in
  `execute_pending_save_loads` that does not follow the function's own
  established standard — `validate_cell_loadable` (#1697) exists precisely so a
  foreseeable failure is detected *before* the destructive teardown, and the same
  reasoning applies here: every column's decodability is knowable from the
  snapshot alone, with no world mutation required.
- **Related**: SAVE-D2-2026-08-20-01 (the reachable trigger); #1697 /
  SAVE-D6-02 (the pre-flight precedent this should follow); #3022 (the reconciler
  the `Err` path skips); SAVE-D4-2026-08-20-01 (why the log line is now the only
  channel); #1847 / SAVE-04 (the additive-only contract).
- **Suggested Fix**: Split `apply_deltas` into a non-mutating decode pass over
  every column in `columns` (deserialise to the typed `Vec<(u32, T)>` and discard)
  followed by the existing apply pass, and hoist the decode pass into
  `execute_pending_save_loads` **before** `drain_streaming_state` — alongside
  `validate_cell_loadable`, whose slot in the sequence it shares. Then move
  `reconcile_dead_actor_runtime_state` out of the `Ok` arm so it runs on any
  outcome in which `Dead` rows were applied.

### MEDIUM

#### SAVE-D2-2026-08-20-01: two required fields were added to registered saved structs after the last `FORMAT_MAJOR` bump — the guard that enforces the rule is structurally blind to required-field additions
- **Severity**: MEDIUM
- **Dimension**: 2 — Registry & (De)serialization Fidelity
- **Data-Loss Class**: corruption-on-load
- **Location**: live violations at `crates/core/src/ecs/components/material.rs:58` (`Material.water_shader_flags`) and `:61` (`Material.is_water_shader`), `crates/core/src/ecs/components/collision.rs:179` (`RigidBodyData.collidable`); the guard at `byroredux/src/save_io/serde_default_guard_tests.rs:114-127` (`serde_attr_declares_unsafe_default`) + `:133-149` (the guard test); the rule at `crates/save/src/snapshot.rs:40-62`
- **Status**: NEW — the `serde(default)` half of this guard (#3020) is **confirmed fixed** and verified; this is the *other* half of the same footgun, which no guard has ever covered
- **Description**: `FORMAT_MAJOR`'s doc block is explicit that an intra-type change to a saved struct requires a major bump, because `schema_fingerprint` hashes only column *keys* and cannot see inside a type. This cycle took that seriously three times — v2 (ActorValues AVHealth keying), v3 (quest lifecycle fields made required), v4 (`EquippedWeapon.reach`/`.speed`), the last of which even carries a model rationale at the field:

  ```rust
  /// Required (no `#[serde(default)]`) per SAVE-D2-01 (#1714): a default
  /// here would silently backfill pre-#3096 saves with fabricated `0.0`
  /// reach/speed instead of rejecting them. `byroredux_save::FORMAT_MAJOR`
  /// was bumped for this field addition instead.
  ```

  Then, on the following day and **after** that bump, two commits added required
  fields to registered saved structs with no bump at all:
  - `8110f359` (2026-08-19) → `Material.water_shader_flags: u32` + `Material.is_water_shader: bool`. `Material` is registered at `save_io.rs:308`.
  - `00fc0f3b` (2026-08-19) → `RigidBodyData.collidable: bool`. Registered at `save_io.rs:293` **and** a `MUTABLE_DELTA_COLUMNS` entry (`:125`).

  Neither touched `crates/save/src/snapshot.rs`. Both fields lack `#[serde(default)]`,
  so a `v4` snapshot written between `219e876c` and those commits passes every
  container gate (magic ✓, `major == 4` ✓, fingerprint ✓ — no column key changed —
  CRC ✓) and then fails `serde_json::from_value` with `missing field`.

  **The blind spot is structural, not an oversight in the fix.** `serde_attr_declares_unsafe_default`
  is by construction a scanner for `serde(default)` attributes. A *required* field
  addition has no attribute to scan for. The engine therefore has one automated
  enforcement of `FORMAT_MAJOR` and it covers exactly the compatible-default half,
  which is the *less* likely half to be written by a developer adding a plain
  `pub collidable: bool`.
- **Evidence**: `git log --format="%H %ad" -G"FORMAT_MAJOR: u16 = " -- crates/save/src/snapshot.rs`
  → last bump `219e876c` 2026-08-18. `git log -S"water_shader_flags" -- crates/core/src/ecs/components/material.rs`
  → `8110f359` 2026-08-19, `--name-only` shows `material.rs` alone.
  `git show --stat 00fc0f3b` lists six files, none of them `snapshot.rs`.
  Diffing every save-participating source file over `219e876c..HEAD` yields
  exactly three field additions to saved structs — `collidable`,
  `water_shader_flags`, `is_water_shader` — and zero corresponding bumps.
  `crates/core/src/ecs/components/material.rs:54` and
  `collision.rs:162-163` confirm both structs derive
  `serde::Serialize/Deserialize` under the `inspect` feature the save build pulls in.
- **Impact**: Real-world blast radius today is bounded — the exposure window is
  one day of development saves, and `Material` is not a delta column, so only
  `RigidBodyData` reaches the live path. What is not bounded is the rule: the
  next such field lands with the same green test suite, and its failure mode is
  SAVE-D6-2026-08-20-01's half-applied world rather than a clean
  `UnsupportedVersion` refusal. The gap also inverts the #1714 doc's own claim
  that the `serde(default)` half is "the caught half" — with the matcher now
  fixed, the required half is the *only* uncaught one, and nothing says so.
- **Related**: #3020 (the `cfg_attr` matcher fix, **confirmed in place** —
  `serde_attribute_body` now parses both forms and three unit tests pin it); #1714
  (the original rule); SAVE-D6-2026-08-20-01 (the failure mode); SAVE-D2-2026-08-20-02
  (residual discovery holes in the same guard).
- **Suggested Fix**: Two parts. (a) Decide the two live violations — bump
  `FORMAT_MAJOR` to 5 with a one-line history entry naming both commits, the same
  treatment v2/v3/v4 each got. (b) Close the class: derive the guard from the
  *shape* rather than the attribute. Because `save_type_sources()` already
  enumerates every save-participating file, a checked-in fingerprint of each
  saved struct's field-name list (a small generated `.txt` the test diffs against,
  regenerated deliberately alongside a `FORMAT_MAJOR` bump) catches additions,
  removals and renames uniformly — where the attribute scanner can only ever catch
  one of the three.

#### SAVE-D1-2026-08-20-01: `CharacterController` gained two mutable gameplay fields this cycle that its allowlist reason predates and its own doc claims are saved — a live load leaks the pre-load breath value into the restored world
- **Severity**: MEDIUM
- **Dimension**: 1 — Snapshot Completeness & Determinism
- **Data-Loss Class**: silent-drop
- **Location**: `crates/physics/src/components.rs:146-152` (`breath_remaining`, `drowning_damage_accumulator` + the doc claim), `byroredux/src/save_io/registry_completeness_tests.rs:224` (the allowlist reason), `byroredux/src/save_io.rs:500-508` (`apply_player_pose`'s momentum clear — the three fields the reason actually describes), `byroredux/src/systems/character.rs:239-241`, `:474-484`, `:1027-1045` (the drowning runtime)
- **Status**: NEW
- **Description**: `CharacterController` is allowlisted as not-saved with the reason:

  > "mutable per-frame fields (velocity/grounded/jump) are deliberately zeroed on
  > reload by the pose-restore path, not carried over"

  That is an accurate description of `vertical_velocity` / `is_grounded` /
  `wants_jump`, and `apply_player_pose` does zero exactly those three. It is not a
  description of the two fields the delta added:

  ```rust
  /// Remaining breath while the player's head is submerged. Seconds.
  pub breath_remaining: f32,
  /// Accumulated drowning damage is kept on the controller so save/load and
  /// fixed-step updates do not lose fractional damage between ticks.
  pub drowning_damage_accumulator: f32,
  ```

  The second field's own doc asserts that keeping it on the controller is what
  makes it survive **save/load**. It does not: `CharacterController` is in no
  registry column, and `apply_player_pose` touches neither field.

  The live-load behaviour is worse than a reset, because the player body is
  **not** cell-owned. `scene.rs`'s player spawn (`:1168-1218`) runs *after*
  `load_cell_with_masters` and is never covered by `stamp_cell_root`'s entity-id
  range, and `World::spawn` is strictly monotonic with no id recycling
  (`crates/core/src/ecs/world.rs:85-92`), so the body's id is always below the
  reloaded cell's `first`. `unload_cell` drains victims from `CellRootIndex` only,
  so the player entity — and its `CharacterController` — survives every live load
  untouched. The result: pressing F9 while drowning at `breath_remaining = 0.2`
  reloads a save taken with a full 15-second reserve and leaves the player at
  0.2 seconds, drowning again immediately in the restored world. Across a process
  restart plus `--load`, the same fields reset to `HUMAN`'s 15.0 / 0.0 instead.
  Neither outcome is the saved value, because the saved value does not exist.
- **Evidence**: `grep -rn "CharacterController" byroredux/src/save_io.rs` → two
  hits, both inside `apply_player_pose` (an `eye_height` read and the
  three-field momentum clear); no registration. The allowlist entry at
  `registry_completeness_tests.rs:224` is unchanged since 2026-08-05 while
  `git log` shows the breath pair arriving with the water/drowning work.
  `grep -rn "CellRoot" byroredux/src/scene.rs` → **zero hits**, so the
  `PlayerEntity` allowlist reason at `:279` ("cleared by cell unload — … it's
  stamped with `CellRoot`") and the identical claim at
  `byroredux/src/systems/character.rs:39-41` are both factually wrong about the
  mechanism, even though the resource itself stays correct because the entity
  simply persists. `grep -rn "spawn_player_character"` returns only those two doc
  comments — the function does not exist.
- **Impact**: A bounded but genuine gameplay-state gap in exactly the surface the
  delta introduced, and the first instance of a general hazard this audit had not
  had to reason about before: because the player body outlives the cell, **any
  unsaved mutable component on it leaks its pre-load value through a live load**
  rather than resetting. The additive overlay cannot correct that, by design.
  Zooming out, the mechanism that let it through is the guard's granularity: the
  SAVE-D1-12 allowlist is keyed by *type*, so once a type is allowlisted for
  reason X, every field added to it afterwards is invisible forever, however
  thoroughly X stops describing it.
- **Related**: SAVE-D1-2026-08-20-02 (the sibling guard-reach gap);
  SAVE-D2-2026-08-20-01 (the same "field added to an already-classified type"
  shape, on the schema side); the `WaterContact` allowlist reason's now-stale
  "drowning accumulation is not yet wired" parenthetical, which is the same
  staleness one component over.
- **Suggested Fix**: Register `CharacterController` (it is delta-safe — nine
  `f32`s, three `bool`s, one field-less enum, no `FixedString`/`EntityId`/handle)
  and add it to `MUTABLE_DELTA_COLUMNS`, letting `apply_player_pose` keep zeroing
  the three momentum fields *after* the overlay so the existing #2018 behaviour is
  preserved; that requires a `FORMAT_MAJOR` bump, which SAVE-D2-2026-08-20-01
  already calls for. If it is instead left unsaved, correct
  `crates/physics/src/components.rs:150`'s doc claim and narrow the allowlist
  reason to name the breath pair explicitly. Separately, correct the two
  `CellRoot`/`spawn_player_character` doc claims — they describe a mechanism that
  does not exist.

#### SAVE-D1-2026-08-20-02: the completeness guard's `SCAN_ROOTS` covers one subdirectory of `crates/core/src` — 63 `impl Component`/`impl Resource` sites sit outside both guards, and one landed unclassified in this delta
- **Severity**: MEDIUM
- **Dimension**: 1 — Snapshot Completeness & Determinism
- **Data-Loss Class**: silent-drop (latent)
- **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:299-304` (`SCAN_ROOTS`); unscanned examples at `crates/core/src/ecs/resources/mod.rs:938` (`LodCoverageStats`, **new this delta**), `crates/core/src/character/affliction.rs:143` (`AfflictionStatus`), `crates/core/src/character/regen.rs` (`PoolRegenAccumulator`), `crates/core/src/character/components.rs` (`FactionReputation`), `crates/core/src/animation/controller.rs` (`AnimationController`), `crates/audio/src/lib.rs` (`AudioEmitter`, `AudioListener`, `OneShotSound`)
- **Status**: NEW — same class as the fixed SAVE-D1-18 (which added `../byroredux/src`), different roots. Not a regression: that specific root is still present and still working.
- **Description**: `SCAN_ROOTS` is `["../crates/core/src/ecs/components", "../crates/scripting/src", "../crates/physics/src", "../byroredux/src"]`. The first entry is a *subdirectory* of `crates/core/src`, so four sibling directories that also define ECS state are invisible: `crates/core/src/ecs/resources/`, `crates/core/src/character/`, `crates/core/src/animation/`, `crates/core/src/string/` — plus `crates/audio/` and `crates/plugin/` entirely.

  Re-implementing the guard in Python over the live tree: within `SCAN_ROOTS` it
  finds 213 distinct types with **0 unclassified and 0 double-classified** (the
  guard is genuinely green). Across the whole workspace there are **63 further
  `impl Component`/`impl Resource` sites outside those roots**, of which only four
  are registered (`Transform`, `AnimationPlayer`, `AnimationStack`,
  `ItemInstancePool`) and only four more are covered by the sibling
  `REDERIVED_NOT_SAVED` list (`CharacterLevel`, `Background`, `Perks`,
  `FactionRanks`). The remaining ~55 are classified by neither guard — not
  *mis*classified, simply never considered.

  This delta produced a live instance: `8e7582ed`/EX-15 added
  `impl Resource for LodCoverageStats` at `crates/core/src/ecs/resources/mod.rs:938`.
  It is neither registered nor allowlisted, and the guard stayed green because the
  file is not scanned. It happens to be pure telemetry and correctly not
  save-worthy — but that outcome was luck, not enforcement, and it is precisely
  the scenario the guard exists to make impossible.
- **Evidence**: Python re-implementation of `impl_target_type` + the XOR
  assertion over `SCAN_ROOTS` → 213 types, 0 offenders (guard confirmed green);
  the same walk over `crates/` + `byroredux/` → 63 additional sites, of which
  `AfflictionStatus`, `PoolRegenAccumulator`, `FactionReputation`,
  `MeleeDamageConfig`, `CharacterRuleset`, `AnimationController`,
  `AnimationClipRegistry`, `RootMotionDelta`, `LodCoverageStats` and the three
  audio types carry no classification anywhere. Spot-checked for live exposure:
  `affliction_tick_system` has no scheduler registration (forward-latent);
  `FactionReputation` has zero production insert or mutate sites;
  `pool_regen_tick_system` **is** scheduled (`byroredux/src/boot.rs:936`) but its
  `PoolRegenAccumulator` holds only fractional carry. **No live silent-drop
  exists through this gap today** — which is exactly why it should be closed now
  rather than after one does.
- **Impact**: The guard is presented in its own doc comment, and consumed by this
  audit's Dimension 1, as *the* completeness ledger. Its coverage is ~78% of the
  workspace's ECS state definitions, and the shortfall is concentrated in
  `crates/core/src/character/` (CHARAL — actively under construction, and the
  home of exactly the accumulating-progress types #2947 already had to build a
  runtime tripwire for) and `crates/core/src/ecs/resources/` (which is accreting
  new resources, one this cycle).
- **Related**: SAVE-D1-18 (the prior, closed instance of this class); #2947 /
  `validate_progression_state` (a bespoke runtime tripwire built for one
  unscanned type, which a wider scan would have generalised);
  SAVE-D2-2026-08-20-02 (the sibling guard's own discovery holes).
- **Suggested Fix**: Widen `SCAN_ROOTS` to `["../crates/core/src", "../crates/scripting/src", "../crates/physics/src", "../crates/audio/src", "../byroredux/src"]`
  and absorb the resulting ~55 new types into `NOT_SAVED_BY_DESIGN` with real
  reasons in one pass — the same exercise that surfaced seven genuine gaps
  (#2378-#2382) when the allowlist was first built. `collect_rs_files` already
  panics on an unreadable root, so a future directory move stays loud.

### LOW

#### SAVE-D2-2026-08-20-02: the rewritten serde guard's file discovery has three residual holes — a dead match prefix, an unscanned nested type, and a line-bound matcher
- **Severity**: LOW
- **Dimension**: 2 — Registry & (De)serialization Fidelity
- **Data-Loss Class**: none today (latent — no unsafe attribute exists in any unreached file at HEAD)
- **Location**: `byroredux/src/save_io/serde_default_guard_tests.rs:24-40` (`registered_type_names`, the dead prefix at `:28`), `:49-72` (`save_type_sources`, the retain predicate at `:63-70`), `:77-108` (`serde_attribute_body`, the `trimmed.starts_with("#[")` gate at `:78-81`)
- **Status**: NEW — residual of the correctly-CLOSED #3025, filed as a new recurrence rather than a regression: the hand-maintained `SAVE_TYPE_SOURCES` really is gone and the derived replacement really does cover the six files #3025 named
- **Description**: #3025's fix replaced the hand list with derivation. Re-implementing `save_type_sources()` in Python against the live tree selects **41 of 393** candidate files. Three mechanisms leave gaps:
  1. **Dead match prefix.** `registered_type_names` matches `".register_form_id_component::<"`, but that method's real signature is `register_form_id_component(&mut self, name: &'static str)` (`crates/save/src/registry.rs:197`) and the sole call site is `.register_form_id_component("FormIdComponent")` (`save_io.rs:315`) — no turbofish. The prefix matches zero occurrences. Consequently `crates/core/src/form_id.rs`, which defines `FormIdPair` / `LocalFormId` / `PluginId` — the exact payload the form-id column serialises — is **not scanned**, and neither is `crates/core/src/ecs/components/form_id.rs`.
  2. **Nested types in files that define no registered type.** The retain predicate keeps a file only if it contains `cfg_attr(feature = "save"` *or* defines a type whose name appears in a turbofish registration. `crates/plugin/src/esm/records/script_instance.rs` satisfies neither, yet `ScriptInstanceData` is nested inside `PendingFragmentExecution` (`crates/scripting/src/fragment.rs:117`), the payload of the registered `FragmentExecutionQueue`. This is one of the two files #3025's own suggested fix named, and the derived replacement does not reach it. (The other, `crates/scripting/src/translate/effects.rs`, **is** reached — it carries a `feature = "save"` `cfg_attr`.)
  3. **Line-bound matcher.** `serde_attribute_body` requires the trimmed line to start with `#[` *and* contain `serde(` on that same line, so a rustfmt-wrapped multi-line attribute is invisible. No such attribute exists in the tree today (`grep -rn --include='*.rs' 'cfg_attr($'` and `'^[[:space:]]*serde('` both return nothing), so this is pure hardening — but the guard's whole purpose is to survive future edits it cannot anticipate.
- **Evidence**: Python re-implementation of `registered_type_names` + `save_type_sources` over HEAD: 42 registered names extracted, 0 of them via the `register_form_id_component::<` prefix; 41 files selected; `crates/core/src/form_id.rs`, `crates/core/src/ecs/components/form_id.rs`, `crates/core/src/string/mod.rs` and `crates/plugin/src/esm/records/script_instance.rs` all **OUT**. `grep -n "serde(" ` on all four returns nothing but one doc-comment mention, confirming zero live exposure. `grep -n "register_form_id_component" crates/save/src/registry.rs byroredux/src/save_io.rs` confirms the `&'static str` signature and the single non-turbofish call site.
- **Impact**: None at HEAD. The cost is that the guard reads as exhaustive — its module docstring says the scan set "is derived from `build_save_registry`… moving or registering a type changes the scan automatically" — while three defining files of save-participating data sit outside it, one of them the form-id payload that every cross-session reference depends on.
- **Related**: `#3025` (the closed predecessor, correctly closed); `#2015` / `#2181` / `#2537` (the hand-list era of the same drift); SAVE-D2-2026-08-20-01 (the same guard's other, larger blind spot); SAVE-D1-2026-08-20-02 (the sibling guard's reach gap).
- **Suggested Fix**: Delete the dead `".register_form_id_component::<"` prefix and add the form-id column's payload files explicitly (or match the string-argument form as well). Widen the retain predicate to accept any `cfg_attr(feature = "` + serde-derive line rather than the literal `"save"` (`crates/core` gates on `"inspect"`). Make `serde_attribute_body` operate on a whitespace-joined attribute span rather than a single line, and add a wrapped-attribute case to the three existing matcher unit tests.

#### SAVE-D6-2026-08-20-02: none of the four new save/load entry points has a single test
- **Severity**: LOW
- **Dimension**: 6 — M45.1 Live Load-Apply
- **Data-Loss Class**: none (test gap)
- **Location**: `byroredux/src/save_io.rs:618` (`quicksave`), `:822` (`queue_load_slot`), `:827` (`quickload_latest`), `crates/save/src/disk.rs:95-107` (`latest_slot`); `byroredux/src/save_io/command_queue_tests.rs` and `live_reload_tests.rs` (both **zero diff** this cycle)
- **Status**: NEW
- **Description**: `git diff 85b77371..HEAD` over `byroredux/src/save_io/command_queue_tests.rs`, `live_reload_tests.rs`, `validation_gate_tests.rs` and `crates/save/tests/round_trip.rs` is **empty**. The delta added four public entry points and a launch flag; the only new assertion anywhere is a single `assert_eq!(latest_slot(&dir), Some(0))` appended to `disk.rs`'s pre-existing `parse_slot_names` test. Nothing exercises `quicksave`, `quickload_latest`, `queue_load_slot`, or the `--load` boot queue; nothing pins that F5 and the `save` console command produce identical results; nothing covers `latest_slot` on an empty directory, on a directory holding only a `.tmp`, or on an mtime tie.
- **Evidence**: as above, plus `grep -rn "quicksave\|quickload_latest\|queue_load_slot" byroredux/src crates/save` returning only production call sites (`app_events.rs:295`, `main.rs:756`, `main.rs:388`) and the definitions.
- **Impact**: The one surface a player touches is the least-guarded part of the subsystem. It is also, per SAVE-D4-2026-08-20-01, the surface whose failures are invisible at runtime — so tests are the only place a regression could surface at all.
- **Related**: #3026 (the closed issue that added the surface); SAVE-D4-2026-08-20-01.
- **Suggested Fix**: Three tests in `command_queue_tests.rs`, all achievable with the existing in-memory fixtures: `quicksave` and `SaveCommand.execute(world, "")` produce byte-identical output on the same world; `quickload_latest` on an empty save dir returns the `"no save slots available"` error rather than panicking; `latest_slot` ignores a `.tmp` sibling that is newer than every real slot.

#### SAVE-D6-2026-08-20-03: `save-load-roundtrip.md`'s currency note was re-dated to 2026-08-18 while three of the four stale passages OPEN #3028 names were left untouched
- **Severity**: LOW
- **Dimension**: 6 — M45.1 Live Load-Apply
- **Data-Loss Class**: none (doc rot)
- **Location**: `docs/engine/save-load-roundtrip.md:10` (the re-dated currency note), `:42-47` (§2), `:62-70` (§3), `:141-147` (§6)
- **Status**: Escalation of OPEN **#3028** — filed separately rather than skipped because the escalation itself is new to this delta and is not described by the open issue
- **Description**: `8a56a2b6` correctly refreshed §1 (save trigger) and §5 (load trigger) for the new F5/F9/menu/`--load` surface, and in the same commit changed the header from *"Verified against the tree as of 2026-07-15"* to *"as of 2026-08-18, all citations checked against current source."* The three passages #3028 filed as stale were not touched, and one of them drifted **further** in the same delta:
  - §2 still reads "today 10+ components" — the registry holds 33 + 10 resources.
  - §3 still reads "`validate_world` … checks four invariants … plus a binary-side `validate_form_ids`". `validate_world` now runs **six**, and there are **two** binary-side checks. This passage was already wrong by one check when #3028 was filed; `8a56a2b6` and `3a39ca47` added two more.
  - §6 step 6 still enumerates seven delta columns against `MUTABLE_DELTA_COLUMNS`'s twenty.
- **Evidence**: `git diff 85b77371..HEAD -- docs/engine/save-load-roundtrip.md` touches exactly the currency line, §1 and §5. `grep -n "today 10+\|four invariants" docs/engine/save-load-roundtrip.md` → `:42`, `:64`, both present at HEAD. Counted against `crates/save/src/validate.rs:70-78` (six calls) and `byroredux/src/save_io.rs:669-671` (two binary-side).
- **Impact**: A stale doc that advertises its own staleness is self-limiting; one that asserts verification it did not receive is worse, because `_audit-common.md` names this file as the authoritative cross-cutting trace for the subsystem. A reader now undercounts the validation surface 3× *and* has been told the citations were checked.
- **Related**: OPEN **#3028** (the three stale passages — this finding adds only the re-dating, and should be folded into that issue rather than tracked separately). `docs/feature-matrix.md`'s `TD3-002` comment was re-verified and reads correctly — **not** re-flagged, per the skill's explicit instruction.
- **Suggested Fix**: Fold into #3028: either finish the pass the currency line claims, or revert the date to the last passage-level verification. Prefer symbol references (`build_save_registry`, `MUTABLE_DELTA_COLUMNS`, `validate_world`) over transcribed counts and enumerations, which is what rotted all three times.

## Cross-audit dispositions (verified, not re-filed)

Both leads handed to this audit were checked at HEAD. Neither becomes a save
finding, and the reasoning is recorded here so it is not re-derived next cycle.

**1. F5 quicksave calls `SaveCommand::execute` outside the `DebugDrainSystem`
lane (owned by `/audit-ecs`, ECS-2026-08-20-05, MEDIUM).** Confirmed present and
correctly described — `save_io.rs:633-639`'s comment still says "command dispatch
(the sole caller of `execute`)" and there are now three callers
(`app_events.rs:290`, `main.rs:752`, plus command dispatch). **Save-side verdict:
no torn-capture hazard, and no consistency cost.** `window_event` and
`about_to_wait` are both driven serially from the main thread by winit;
`Scheduler::run` joins its rayon batch before returning
(`crates/core/src/ecs/scheduler.rs:498-506`) and is called only from
`about_to_wait:637-641`; nothing in the tree spawns a thread that touches the
`World` (`grep -rn "thread::spawn" byroredux/src crates/debug-server/src` → no
production hits; the debug server's client threads queue commands for
`DebugDrainSystem` rather than executing them). Both new call sites therefore run
strictly between ticks: `window_event` before the next `about_to_wait`, and
`apply_debug_ui_outputs` inside `render_one_frame` at the *end* of `about_to_wait`
(`app_frame.rs:109`), after `scheduler.run`, `step_save_loads` and
`step_cell_transition` have all completed. Pose staleness is ≤1 frame on both,
identical to the console path, which the 2026-08-16 cycle already dispositioned as
not-a-finding. The residual defect is exactly what ECS-2026-08-20-05 says it is —
a load-bearing comment that is now factually false — and it is theirs.

**2. Drowned actors get `Dead` without `reconcile_dead_actor`.** Confirmed:
`apply_player_drowning_damage` (`byroredux/src/systems/character.rs:1040-1044`)
inserts `Dead` directly, while `combat_damage_system`'s kill branch
(`byroredux/src/combat.rs:238-242`) inserts `Dead` **and** calls
`reconcile_dead_actor`. **Save-side verdict: nothing is lost, and the round trip
in fact repairs it.** `Dead` is registered (`save_io.rs:251`) and is a delta
column (`:102`), the player body carries `PLAYER_FORM_ID_PAIR` so it participates
in the remap (`scene.rs:1210-1218`), and the drain runs
`reconcile_dead_actor_runtime_state` over **every** `Dead` entity after
`apply_deltas` (`save_io.rs:1013`) — so a drowned actor that was un-reconciled in
the live session comes back reconciled. The only save-relevant observation is
that the invariant "`Dead` ⇒ AI cleared + ragdoll" is enforced at the combat kill
site and on the load path but at neither the drowning site nor in any pre-write
gate; that asymmetry is a combat/ECS defect, not a persistence one. Drowning is
player-only (`character.rs:477-484`), and the player has no
`HavokAnimationTarget`, so `reconcile_dead_actor` returns `"; no ragdoll target"`
either way. Not re-filed.

## Regression Guards Discovered / Reconfirmed

| Guard | Location | Invariant pinned | State |
|---|---|---|---|
| `every_component_or_resource_impl_is_saved_or_explicitly_allowlisted` | `save_io/registry_completeness_tests.rs:76` | every `impl Component`/`impl Resource` under 4 scan roots is registered XOR allowlisted with a reason | green (re-derived in Python: 213 types, 0 offenders) — but see SAVE-D1-2026-08-20-02 for what it doesn't scan |
| `serde_default_on_saved_struct_requires_format_major_bump` + 3 matcher unit tests |  `save_io/serde_default_guard_tests.rs:133` | no `#[serde(default)]` (bare **or** `cfg_attr`) on a saved struct | green and genuinely fixed (#3020) — blind to required-field additions, SAVE-D2-2026-08-20-01 |
| `source_discovery_follows_registry_and_nested_save_modules` |  `save_io/serde_default_guard_tests.rs:155` | the scan set is derived from `build_save_registry`, not hand-maintained | green (#3025 fixed) — residual holes, SAVE-D2-2026-08-20-02 |
| `delta_columns_carry_only_session_stable_fields` | `save_io/round_trip_tests.rs:28` | `MUTABLE_DELTA_COLUMNS` == a hand-audited list; forces review on every addition | green, unchanged |
| `npc_spawn_stamped_components_are_saved_or_intentionally_rederived` | `save_io/round_trip_tests.rs:732` | NPC-spawn state is saved XOR documented re-derived | green; `REDERIVED_NOT_SAVED`'s `CharacterLevel`/`Perks` premise now backstopped at runtime by `validate_progression_state` (#2947) |
| `validate_progression_state` (**new**) | `crates/save/src/validate.rs:416-435` | a save is refused outright if `CharacterLevel.xp != 0` while `CharacterLevel` is unregistered | live, fires ahead of its consumer — the model fix for an allowlist premise that can expire |
| `form_id_column_resolves_the_flagged_entry`, `..._is_none_without_registration`, `registering_a_second_form_id_column_panics` | `crates/save/src/registry.rs:377-405` | remap key comes from the explicit `is_form_id` flag, at most one column | green (#1845 intact; `registry.rs` zero-diff) |
| `rejects_bad_magic` / `rejects_truncated` / `rejects_payload_truncation` / `detects_crc_corruption` / `rejects_schema_mismatch` / `rejects_major_version_skew` | `crates/save/src/snapshot.rs:186-258` | every container gate precedes `serde_json::from_slice`; CRC covers payload only | green |
| `parse_slot_names` (+ `latest_slot` assertion), `cursor_after_newest_points_past_latest_mtime`, `resume_on_empty_dir_starts_at_zero`, `write_read_round_trip_and_atomic_rename`, `ring_wraps`, `ring_size_floored_to_one` | `crates/save/src/disk.rs:183-269` | strict slot parsing, resume-past-newest, atomic rename with no leftover tmp | green |
| `dangling_item_instance_is_rejected`, `item_instance_without_pool_is_rejected`, `live_item_instance_passes`, `stackable_item_without_instance_is_clean` | `crates/save/src/validate.rs:310-379` | `ItemStack.instance` resolves in `ItemInstancePool` before write | green |
| `dangling_horse_tether_reference_is_rejected`, `dangling_cinematic_vehicle_reference_is_rejected` (+2 positive) | `save_io/validation_gate_tests.rs` | cinematic `EntityId` refs are gated; now share `validate_entity_reference` with the three M42 types | green |
| `player_pose_character_tracks_body`, `player_pose_flycam_saved_relocates_body_in_live_character_mode`, `player_pose_round_trips_flycam`, `player_pose_survives_snapshot_round_trip` | `save_io/live_reload_tests.rs` | pose restore across both modes, momentum clear, no-handle no-op | green (#2018 intact) |
| `quicksave_ring_cursor_does_not_advance_on_validation_abort`, `second_load_before_drain_supersedes_and_reports` | `save_io/command_queue_tests.rs` | ring rotation only on committed writes (#2017); supersede is reported (#1848) | green |

## Verified Clean — No New Findings

- **Dimension 3 (Disk)** in full, sixth consecutive cycle: the write dance
  ordering (`create_dir_all` → tmp write → `flush` → `sync_all` → byte-exact
  read-back → `rename` → parent-dir fsync), tmp cleanup on failed read-back, the
  single write path in the process (`grep "write_slot("` → one production caller),
  decode gate ordering, CRC scope, `checked_add` payload bounds, strict slot-name
  parsing, and `SaveRing::resume`. The new `latest_slot` reuses
  `parse_slot_filename`, so a `.tmp` sibling can never surface as loadable.
- **Dimension 5 (Frame boundary)** in full, including both new call sites — see
  *Cross-audit dispositions*. `restore_world` still has **zero production
  callers** (`grep` → tests only), so the two-restore-path id-collision hazard
  remains structurally unreachable.
- **`next_entity` bounds**: `validate_entity_ids_in_bounds`
  (`crates/save/src/driver.rs:76-96`) is a real `Result` check before any
  mutation, not a `debug_assert` — closed and still closed.
- **`StringPool::dump`/`from_dump`** (`crates/core/src/string/mod.rs:102-131`):
  indexes by symbol and panics on a gap; the CRITICAL "every `FixedString` points
  at the wrong symbol" class stays structurally prevented.
- **Determinism**: `BTreeMap` columns, row sort by entity id in both
  `register_component` and `register_form_id_component`; the reproducible-CRC
  claim holds at row level. `FnvHasher`'s constants are the canonical 64-bit
  FNV-1a values and the hash depends only on registered names + order.
- **Live-load ordering** re-verified end to end at HEAD: drain → cell-context
  resolve → `validate_cell_loadable` pre-flight (#1697) → teardown →
  `load_cell_with_masters` → lighting + temporal discontinuity + `LoadedPluginSet`
  → `restore_resources` → `build_form_id_remap` (with the #2019 unresolved-pair
  logging) → `apply_deltas` → reconcile → post-load diagnostics →
  `apply_player_pose` **last**. Correct in every position.
- **`CurrentCellContext` lifetime (#3021 fix)** verified in place:
  `clear_current_interior_identity` (`cell_loader/transition.rs:337-340`) now
  removes the resource alongside the `CurrentCellRoot(None)` reset, so an
  Interior→Exterior transition can no longer leave a stale interior identity for a
  later save to claim.
- **The `--load <slot>` boot queue** (`main.rs:385-393`): sequenced *after*
  `install_runtime_registries` (`boot.rs:1477-1488` installs `SaveRegistry`,
  `SaveState`, `PendingSaveLoadSlot`, `PlayerPose`), so `LoadCommand::execute`
  finds every resource it needs; the queued snapshot is drained by the normal
  between-frames path with no special-casing. Correct.
- **Player-body lifetime across a live load**: traced in full for
  SAVE-D1-2026-08-20-01 and confirmed sound as a *mechanism* — the body survives,
  keeps its `FormIdComponent`, and is found by `apply_player_pose`. Only the
  unsaved-component leak is a finding.

## Disproved Candidates (investigated, not filed)

- **"The F5 quicksave captures a torn mid-frame world."** Rejected — winit
  dispatches `window_event` and `about_to_wait` serially on one thread and
  `Scheduler::run` joins before returning, so no system holds a storage lock when
  the handler executes. See *Cross-audit dispositions*.
- **"The water delta added components that never reached the registry."** Rejected
  — this was the highest-yield hypothesis going in, and it does not hold. All ten
  new water/VFX types are allowlisted with reasons that were individually checked
  against live mutator sites. `WaterMaterial`'s growth 18→63 fields is irrelevant
  to the save path: it is reached only through `WaterContact.material` and
  `WaterPlane`, neither of which is saved.
- **"`WaterContact` accumulates drowning state that isn't saved."** Rejected —
  the accumulator lives on `CharacterController`
  (`crates/physics/src/components.rs:152`); `WaterContact` is fully recomputed
  each tick. The real gap is one component over, filed as
  SAVE-D1-2026-08-20-01.
- **"`quickload_latest` can pick up a half-written `.tmp` as the newest slot."**
  Rejected — `latest_slot` filters through `parse_slot_filename`, which rejects
  `save_42.ess.tmp` (pinned by `parse_slot_names`).
- **"`unload_current_interior` removing `CurrentCellContext` (#3021) strands a
  session whose reload fails after teardown."** Rejected as a *new* finding: the
  post-teardown failure window is the already-documented residual of #1697, and
  the #3021 change strictly improves it — a stranded session now correctly
  reports "loose/exterior save" instead of offering to reload a cell that isn't
  there.
- **"Two registered saved structs gaining fields means `MUTABLE_DELTA_COLUMNS`
  drifted."** Rejected — `EquippedWeapon.reach`/`.speed` (`f32`) and
  `RigidBodyData.collidable` (`bool`) are all session-stable; the delta-safety
  invariant holds. Only the schema-version half is a finding.
- **"`build_save_registry()` rebuilt inside `execute_pending_save_loads` can
  drift from the installed resource."** Rejected again (same reasoning as
  2026-08-16): identical by construction.

## Deduplication

`/tmp/audit/issues.json` (400 issues, #2671-#3103) searched for `save`, `load`,
`snapshot`, `corrupt`, `formid`, `delta`, `serde`, `quicksave`, `quickload`,
`ring`, `validate`, `breath`, `drown`, `collidable`, `format_major`, `pose`,
`registry`. Per the briefing, issue numbers below #2671 cannot be re-queried and
are carried on the 2026-08-16 report's word.

Prior-cycle findings, all nine re-checked at HEAD:

| 2026-08-16 finding | Issue | State at HEAD |
|---|---|---|
| SAVE-D6-…-01 stale `CurrentCellContext` | #3021 | **CLOSED, fix verified** — `clear_current_interior_identity` removes the resource |
| SAVE-D2-…-01 `cfg_attr`-blind `serde(default)` guard | #3020 | **CLOSED, fix verified** — `serde_attribute_body` parses both forms, 3 unit tests pin it, `FORMAT_MAJOR` 1→4, the two `QuestStageData` defaults removed |
| SAVE-D1-…-01 death-teardown removals unreplayable | #3022 | **CLOSED, fix verified** — `reconcile_dead_actor` shared by the kill site and the drain; `apply_deltas`'s docstring rewritten to the marker-plus-reconciler contract |
| SAVE-D4-…-01 three unguarded `EntityId` carriers | #3023 | **CLOSED, fix verified** — `validate_saved_entity_references` + the shared `validate_entity_reference` helper, which the two cinematic checks were refactored onto |
| SAVE-D4-…-02 `EquippedWeapon.inventory_index` unvalidated | #3024 | **CLOSED, fix verified** — and widened beyond the ask with a `base_form_id` ↔ `inventory[index]` cross-check |
| SAVE-D2-…-02 `SAVE_TYPE_SOURCES` hand-list drift | #3025 | **CLOSED, fix verified** — hand list replaced by derivation from `build_save_registry` + a recursive walk. Residual holes → SAVE-D2-2026-08-20-02 |
| SAVE-D6-…-02 no non-console entry point | #3026 | **CLOSED, fix verified** — F5/F9 + two menu buttons + `--load`. Its consequences → SAVE-D4-2026-08-20-01, SAVE-D6-2026-08-20-02 |
| SAVE-D1-…-02 `ActorVitals` exclusion undocumented | #3027 | **OPEN**, unchanged at HEAD (`save_io.rs:250` still bare). Noted and skipped |
| SAVE-D6-…-03 `save-load-roundtrip.md` stale | #3028 | **OPEN**, §1/§5 refreshed, §2/§3/§6 not. The currency-note re-dating is new → SAVE-D6-2026-08-20-03, to fold into #3028 |

Other save-adjacent OPEN issues, none overlapping a finding above:

| Issue | Why not a duplicate |
|---|---|
| `#2687` SAFE-D9-01 save-restore is a `Material` producer that skips `resolve_pbr` | renderer-side consequence of `Material` restore, owned by `/audit-safety`. Adjacent to SAVE-D2-2026-08-20-01 (same component) but a different defect |
| `#2370` EX-09/17 exterior transitions + save/load | scopes *adding* exterior save/load; nothing here touches it |
| `#3088`/`#3087`/`#3086` audio | different subsystem |

`#2947` (CHAR-D3-08, `CharacterLevel`/`Perks` save-exempt) is **CLOSED** and its
fix — `validate_progression_state` — was verified live at
`crates/save/src/validate.rs:416-435`, running inside `validate_world` and
therefore on the pre-write path of every save.

---

TALLY: CRITICAL=0 HIGH=2 MEDIUM=3 LOW=3
