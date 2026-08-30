# Save / Load Subsystem Audit (M45 + M45.1) — 2026-08-30

Scope: `crates/save/src/` (`lib`, `snapshot`, `registry`, `driver`, `disk`,
`validate`, `tests/round_trip.rs`) plus the engine-side consumer
`byroredux/src/save_io.rs` and its seven `save_io/*_tests.rs` siblings, and the
cross-cut ground truth the flow depends on (`app_events.rs`, `app_frame.rs`,
`boot.rs`, `cell_loader/{spawn,load,unload,transition}.rs`, `inventory.rs`,
`npc_spawn/ai_package.rs`, `combat.rs`, `crates/scripting/src/fragment.rs`).
Audited at HEAD `64f64480`.

This is the **twelfth** save audit (prior: `2026-06-23` … `2026-08-27`). Run
solo, in-process — no sub-agent fan-out — per this cycle's explicit
instruction; all six dimensions covered directly by Read/Grep/Bash, with
`cargo test -p byroredux-save` run live (30 unit + 15 integration, green) and
the two binary-side source-scanning guards **replicated statically in Python**
rather than run, to stay inside this run's memory budget (`cargo test -p
byroredux` links the whole engine and OOM-killed a sibling audit earlier in
this suite). The replication is exact: same six `SCAN_ROOTS`, same test-file
and `#[cfg(test)]` exclusions, same registered-XOR-allowlisted assertion.

The delta since `969d81c8` is large but only four commits touch this
subsystem: `dd798679` (#3472, `atomic_write` extracted as a shared helper),
`fa511bbf` (#3488, the `EquippedWeapon` reconciler — last cycle's HIGH),
`19813460` (#3530, `FORMAT_MAJOR` 9 → 10), and `265f0c9b` (#3278, which gave
`ReferenceEnableState` its first runtime consumer). **That last one is this
cycle's HIGH**: the consumer reads the resource during the cell reload, one
step before `restore_resources` installs the saved value.

## Executive Summary

`crates/save/src/lib.rs` docstring claims verified against live code:

| Claim | Status |
|---|---|
| Full ECS snapshot (curated game-state set) | **CODE-CONFIRMED.** The SAVE-D1-12 guard is green — statically replicated: 252 production `impl Component`/`impl Resource` lines across the six roots, 47 registered columns, 235 allowlist entries, **0 offenders**. Two known coverage caveats remain open as filed issues (#3491, #3497/#3505), not re-reported here. |
| Atomic write (tmp → fsync → read-back-verify → rename → dir-fsync) | **CODE-CONFIRMED, re-verified against the #3472 refactor.** The sequence is byte-for-byte unchanged; the only delta is the fsync target moving from `dir` to `final_path.parent()`, which resolves to the identical path for `write_slot`. |
| Ring never clobbers the last good save | **CODE-CONFIRMED.** `SaveState::new` still calls `SaveRing::resume`; `SaveCommand::execute` still `peek()`s and only `advance()`s after validation passes. |
| Validation gate refuses to persist an inconsistent world | **CODE-CONFIRMED.** Nine checks (seven core + two binary) all run before `save_world`/`encode`/`write_slot`, with an unconditional early `return`. Coverage has one newly-visible hole — see SAVE-D4-2026-08-30-03. |
| Typed-decode preflight rejects a bad snapshot before any teardown | **CODE-CONFIRMED.** First thing after the slot drain (`save_io.rs:1374`), before the interior/exterior branch is chosen; `restore_world` runs it before `clear_entities`. |
| `FORMAT_MAJOR` bump is the only sanctioned schema evolution path | **CODE-CONFIRMED.** Now `10`. `BASELINE_MAJOR`/`BASELINE_SHAPE_FINGERPRINT` were regenerated in **the same commit** (`19813460`) that moved `FORMAT_MAJOR` — `git log -S` confirms both constants change only there. The v10 doc comment explicitly declines the "but this default is safe" relaxation (#1714). |
| Off-frame load, never inside the scheduler | **CODE-CONFIRMED.** `restore_world` still has zero production callers; the `capture_player_pose` → `step_player_save_actions` → `step_save_loads` → `render_one_frame` ordering inside one `about_to_wait` is intact despite `app_events.rs` changing +99 lines (all bench telemetry / depth capture / unload batching). |
| Additive-only overlay + explicit reconciler for removals | **CODE-CONFIRMED for the mechanism, DRIFTED for its guard.** `fa511bbf` shipped the second reconciler the contract demanded (#3488). Its companion tripwire, however, cannot detect the class it was written for — see SAVE-D1-2026-08-30-02. |
| Saved resources are in force when the reloaded cell is built | **DRIFTED — new this cycle.** `restore_resources` runs *after* the reload, which was harmless while no saved resource had a load-time consumer. #3278 created one. See SAVE-D6-2026-08-30-01. |

**Findings this cycle: 4. 0 CRITICAL, 1 HIGH, 2 MEDIUM, 1 LOW.**

By Data-Loss Class: **corruption-on-load — 1** (HIGH); **latent silent-drop —
1** (MEDIUM); **latent corruption-on-load — 1** (MEDIUM); **none — 1** (LOW).

**9 candidate findings were investigated and dropped** on a stale or wrong
premise — enumerated in *Disproved Candidates* below, so a later cycle does
not re-derive them.

## Data-Loss Class Matrix

| Finding | Class | Dimension | Severity |
|---|---|---|---|
| SAVE-D6-2026-08-30-01 — the cell reload consults `ReferenceEnableState` before `restore_resources` installs the saved one | corruption-on-load | 6 (owner) / 1 | HIGH |
| SAVE-D1-2026-08-30-02 — the #3488 companion guard scans nothing; 6 of 7 production delta-column removal sites use an idiom its audit-claim cannot see | latent silent-drop | 1 | MEDIUM |
| SAVE-D4-2026-08-30-03 — `validate_animation` stops at `AnimationPlayer`; `AnimationStack` and `Seated.animation_restore` carry the same two reference classes unchecked | latent corruption-on-load | 4 | MEDIUM |
| SAVE-D6-2026-08-30-04 — `save-load-roundtrip.md` still calls death the one reconciler wired | none (doc rot) | 6 | LOW |

## Per-Dimension Coverage

| Dimension | Findings | Notes |
|---|---|---|
| 1 — Snapshot Completeness & Determinism | **1** (MEDIUM) | Guard replicated statically, green. Ledger unchanged from last cycle (33 components + 1 form-id + 13 resources × 21 delta columns); every SAVED-ONLY entry keeps its documented reason; none of the seven 2026-08-05 regressions drifted back out; `CharacterController` stays registered AND overlaid with its allowlist entry removed. Both save closures still `rows.sort_by_key`, so the reproducible-CRC claim holds at row level. |
| 2 — Registry & (De)serialization Fidelity | **0** | All three `register_*` variants build a `ValidateFn` decoding the same target type `load` does. `form_id_column()` still keys off the explicit `is_form_id` flag with the registration-time assert. FNV constants canonical; fingerprint depends only on names + order + kind tag. `save → inspect → serde` feature chain confirmed in both manifests. Shape baseline regenerated in the `FORMAT_MAJOR` commit. |
| 3 — Disk Format & Durability | **0** | The #3472 `atomic_write` extraction diffed line-by-line against `969d81c8` — no durability regression. `decode`'s gate order still precedes every parse; CRC still payload-only so the version-skew error stays reachable; `parse_slot_filename` still rejects `.tmp` and non-numeric; `slots_by_recency` deterministic; `quickload_latest` consumes the full ordered list. |
| 4 — Validation Gates | **1** (MEDIUM) | Write-path gate unconditional and pre-write, with no bypassing save path. Coverage is **nine** checks, not the five the skill lists. Dangling semantics still `>= next_entity`. Equipment bounds `>=`, correct, with distinct None/out-of-bounds details. The typed preflight aborts on both restore paths; the later `apply_deltas` `Err` arm reconciles then `return`s without falling through. |
| 5 — Frame-Boundary Capture & Off-Frame Apply | **0** | Ordering intact despite `app_events.rs` +99 (every hunk is bench telemetry, the #3308 depth bridge, or #3386 unload batching). `SaveLoadNotifications` still drained unconditionally by `mem::take`, with `if let Some(ui)` gating only the display — and the drain is reachable exactly when the producer is, so a headless run cannot leak the `Vec`. Live path never calls `restore_world`. |
| 6 — M45.1 Live Load-Apply | **1 HIGH + 1 LOW** | Full ordering re-verified. `restore_resources` still precedes `apply_deltas`; pose restore still last and still zeroes only the three motion fields on top of the `CharacterController` overlay. #3488's fix verified correct end-to-end, including that `InventoryCatalog` — which the new reconciler depends on — is rebuilt from the reloaded ESM index by `populate_scene_runtime` on **both** loader entry points before the reconciler runs. |

## Completeness Ledger

`build_save_registry` (`byroredux/src/save_io.rs:317-496`) × `MUTABLE_DELTA_COLUMNS`
(`:84-134`, twenty-one entries). Cross-checked against the SAVE-D1-12 guard's
`NOT_SAVED_BY_DESIGN` allowlist rather than re-derived. Unchanged from
2026-08-27 — no drift this cycle.

| Column | Kind | Saved | Overlaid | Status |
|---|---|---|---|---|
| `Transform`, `Inventory`, `EquipmentSlots`, `LightSource`, `LightFlicker`, `ScriptTimer`, `TwoStateActivator`, `ScriptVariables`, `ActorValues`, `EquippedWeapon`, `Dead`, `WanderState`, `TravelState`, `Traveled`, `GuardState`, `PatrolState`, `Escorted`, `ActorControlState`, `CharacterController`, `RigidBodyData`, `RumbleOnActivate` | Component | yes | yes | SAVED+OVERLAID, pinned by `delta_columns_carry_only_session_stable_fields`. `EquippedWeapon`'s removal is now reconciled (#3488). Six of the rest have an unguarded production removal site — SAVE-D1-2026-08-30-02. |
| `Name`, `Parent`, `Children`, `FormIdComponent` | Component | yes | no | structural identity — correct by design |
| `AnimationPlayer`, `AnimationStack` | Component | yes | no (deliberate) | #1696 `root_entity`/`clip_handle` hazard. `AnimationStack` has **no production insertion site** (its only constructor is under `#[cfg(all(test, feature = "inspect"))]`) — forward-latent, and unchecked by `validate_animation` (SAVE-D4-2026-08-30-03). |
| `FollowState`, `EscortState`, `Seated` | Component | yes | no (deliberate) | `EntityId` hazard, covered by `validate_saved_entity_references`. `Seated` gained a **second, undocumented** hazard in v9 — see SAVE-D4-2026-08-30-03. |
| `ActorCinematicState`, `HorseTetherState` | Component | yes | no (deliberate) | #2380 `EntityId` hazard; also covered by `validate_cinematic_entity_refs` |
| `Material` | Component | yes | no (deliberate) | #2378 blast-radius; carries the v6/v7/v10 fields. `validate_material_finiteness` runs on every save. |
| `ActorVitals` | Component | yes | no (documented) | #3027 — `ActorVitals.health` is a per-game AVIF **FormID key**, not an HP value |
| `ItemInstancePool`, `CurrentCellContext`, `CurrentExteriorContext`, `PlayerPose`, `GameTimeRes`, `QuestStageState`, `QuestObjectiveState`, `Globals`, `QuestAliasInjectionState`, `PlayerControlState`, `FragmentExecutionQueue`, `ReferenceEnableState`, `CinematicPresentationState` | Resource | yes | n/a | replaced wholesale by `restore_resources`. **The "before `apply_deltas`" placement is correct but no longer sufficient** — `ReferenceEnableState` is now also read *during the reload*, which happens earlier still (SAVE-D6-2026-08-30-01). |

Ledger note (informational): `QuestAliasInjectionState.factions` is a
`HashMap<(EntityId, u32), _>` — an `EntityId` key inside a saved resource —
but it is `#[cfg_attr(feature = "save", serde(skip, default))]`
(`crates/scripting/src/scene/quest_alias.rs:88`), so it never reaches disk.
The registry's comment claiming exactly that is accurate.

## Findings

### HIGH

#### SAVE-D6-2026-08-30-01: the cell reload consults `ReferenceEnableState` *before* `restore_resources` installs the saved one, so a live load rebuilds the cell against the wrong enable ledger — every `Disable()`d reference comes back visible on the primary load path

- **Severity**: HIGH
- **Dimension**: 6 — M45.1 Live Load-Apply (owner); originates in 1 — resource-restore ordering
- **Data-Loss Class**: corruption-on-load
- **Location**: `byroredux/src/save_io.rs:1391-1394` (the reload) vs `:1411` (`restore_resources`); `byroredux/src/cell_loader/spawn.rs:444-460` (`placement_is_disabled`) and `:631` (the gate); `byroredux/src/cell_loader/references/synth_child.rs:647` + `byroredux/src/cell_loader/precombined.rs:391` (the two `spawn_placed_instances` call sites); `crates/scripting/src/fragment.rs:1535` + `byroredux/src/boot.rs:670` (the sole, boot-time installer); `crates/scripting/src/translate/effects.rs:803-811` (`prim_disable`); `docs/engine/save-load-roundtrip.md:222-224` (the claim this disproves)
- **Status**: NEW. Made possible by `265f0c9b` (#3278), which landed after the 2026-08-27 audit and gave a **saved resource** its first load-time consumer. Every prior save audit correctly recorded `ReferenceEnableState` as consumer-less, which is why the ordering was never examined — the `/audit-save` skill's own Dimension-1 checklist still asserts it (see *Skill vs. Code* below).
- **Description**: `ReferenceEnableState` is registered as a save resource (`save_io.rs:488`) and is the FormID-keyed ledger a Papyrus `Disable()` writes to. Until #3278 nothing read it, so it was pure round-trip state. #3278 added the runtime consumer:

  ```rust
  pub(crate) fn placement_is_disabled(
      world: &World,
      placement_fid: Option<byroredux_core::form_id::FormId>,
  ) -> bool {
      let Some(fid) = placement_fid else { return false };
      let Some(local) = world
          .try_resource::<FormIdPool>()
          .and_then(|pool| pool.resolve(fid).map(|pair| pair.local.0))
      else { return false };
      world
          .try_resource::<byroredux_scripting::ReferenceEnableState>()
          .is_some_and(|state| !state.is_enabled(local))
  }
  ```
  (`cell_loader/spawn.rs:444-460`), consulted per placed REFR at `spawn.rs:631`, *after* the placement root and *before* any mesh, collider or light — so one check suppresses all three.

  `execute_pending_save_loads` reloads the cell at `save_io.rs:1392` (interior) / `:1394` (exterior) and calls `restore_resources` only at `:1411`. `byroredux_scripting::register` — the one installer of this resource — runs at boot (`boot.rs:670`), not per cell load, and nothing under `cell_loader/` resets it. So the reload's spawn decisions are taken against the **live session's** ledger, and the saved ledger arrives after every one of them. `apply_deltas`, which follows, is additive-only by contract and can neither spawn nor despawn.

  Two symmetric failures, the first on the *primary* load path:

  - **Fresh session, `--load N` or quickload after a restart.** The live ledger is `ReferenceEnableState::default()` — everything enabled. Every reference the save recorded as disabled respawns with full renderable and collidable content. The saved fact lands in the resource a moment later, but nothing re-reads it until the *next* cell load, which for a player who just loaded into that cell is not going to happen this session.
  - **Same-session load after further `Disable()`s.** A reference disabled *after* the save spawns content-less even though the save says it is enabled, and stays that way for the whole of that cell's residency.

  This is not a hypothetical path. `prim_disable` recognises `X.Disable()` straight out of decompiled vanilla Papyrus (`translate/effects.rs:803-811`), and `DeferredEffects::apply` commits it via `state.set_enabled` (`fragment.rs:594-600`). The exterior branch has the identical exposure — `assemble_exterior_streaming` reaches the same `spawn_placed_instances`.
- **Evidence**: `save_io.rs:1391-1394` and `:1411` (quoted ordering, 17 lines apart with the reload in between). `grep -rn "ReferenceEnableState" byroredux/src/cell_loader/` returns only `spawn.rs` — no reset during unload. `grep -rn "byroredux_scripting::register"` returns one production site, `boot.rs:670`. `docs/engine/save-load-roundtrip.md:222-224` states "Reference visibility is no longer part of that gap: scripted `Disable()` records the stable FormID in the saved `ReferenceEnableState` resource, and **reload**/spawn/render consumers reapply it" — the reload consumer is real, but it runs first, so the doc asserts precisely the guarantee the ordering does not deliver.
- **Impact**: The subsystem's whole thesis is that the loaded world equals the persisted world. Here it does not, on the most common load in the game (start the engine, load a save). Quest-critical scenery, markers and blockers a quest disabled reappear solid and interactive; nothing logs it, because from the loader's point of view it correctly honoured the ledger it was shown. The ledger itself round-trips intact, so this is not permanent data loss — but the session the player is handed contradicts their save file, and the next quicksave re-records the contradicted state as truth.
- **Related**: #3278 (the consumer that created the ordering requirement); #3489 (`Effect::Disable` has no `Enable` counterpart — adjacent one-way-door concern over the same resource, distinct defect); #1847 / SAVE-04 (`apply_deltas` additive-only, the reason the overlay cannot compensate).
- **Suggested Fix**: Do **not** simply hoist `restore_resources` ahead of the teardown: `unload_current_interior`'s inventory sweep (`cell_loader/unload.rs:500-519`) releases `ItemInstanceId`s into whichever `ItemInstancePool` is installed, and doing that to the freshly restored arena would corrupt it — that ordering constraint is why `restore_resources` sits where it does. Two clean options: (a) split the restore, installing the resources the *spawn path* consults (`ReferenceEnableState`, and any future sibling) immediately after the teardown and before the reload, leaving the rest where they are; or (b) keep the ordering and re-run the disable gate over the reloaded cell after `restore_resources`, as a reconciler in the `apply_deltas` tail alongside `reconcile_dead_actor_runtime_state` — which is exactly the marker-plus-reconciler contract `apply_deltas`' doc comment prescribes for a persisted fact whose runtime consequence the overlay cannot express. Option (a) is cheaper and avoids the mid-frame despawn the #3278 comment already flags as out of scope. Either way, correct `docs/engine/save-load-roundtrip.md:222-224`, and add a guard asserting that every resource read by `placement_is_disabled`'s call chain is restored before the reload.

### MEDIUM

#### SAVE-D1-2026-08-30-02: the #3488 companion guard scans nothing — six of the seven production delta-column removal sites go through a `remove_component::<T>` helper its audit claim cannot see

- **Severity**: MEDIUM
- **Dimension**: 1 — Snapshot Completeness & Determinism
- **Data-Loss Class**: latent silent-drop (no live loss today)
- **Location**: `byroredux/src/save_io/round_trip_tests.rs:86-131` (the guard and its doc comment); `byroredux/src/npc_spawn/ai_package.rs:428` (the helper) and `:468-484` (the six sites); `byroredux/src/combat.rs:404` (a second copy of the same helper)
- **Status**: NEW — the guard itself landed this cycle in `fa511bbf` alongside the #3488 fix.
- **Description**: `delta_columns_removed_at_runtime_have_a_load_reconciler` is the tripwire meant to stop #3488 recurring. Its doc comment states the mechanism:

  > Rust has no reflection for "which components does this crate remove", so this **scans the tree** for production `world.remove::<T>` sites the same way the sibling above pins the column set: by hand-audited list. **Adding one makes this fail** and forces the maintainer to write the reconciler.

  It does not scan the tree. The body iterates a one-entry `RECONCILED` list and greps two fixed strings:

  ```rust
  let save_io = include_str!("../save_io.rs");
  for (column, reconciler) in RECONCILED { … assert!(save_io.contains(reconciler), …) }
  let inventory = include_str!("../inventory.rs");
  assert!(inventory.contains("world.remove::<EquippedWeapon>(player)"), …);
  ```

  Adding a second production removal site does not make it fail; nothing enumerates removals at all.

  Its audit-half comment is worse than incomplete, because it is spelling-scoped:

  > The audit half: `EquippedWeapon` must still be the only delta column a production path removes. `Dead` is removed nowhere; every other `world.remove::<T>` in the tree is inside a `#[cfg(test)]` module.

  A production-only scan for **both** idioms finds seven delta-column removal sites, not one:

  | Site | Column | Idiom |
  |---|---|---|
  | `byroredux/src/inventory.rs:531` | `EquippedWeapon` | `world.remove::<T>` — covered |
  | `byroredux/src/npc_spawn/ai_package.rs:471` | `WanderState` | `remove_component::<T>` |
  | `…:473` | `TravelState` | `remove_component::<T>` |
  | `…:474` | `Traveled` | `remove_component::<T>` |
  | `…:479` | `Escorted` | `remove_component::<T>` |
  | `…:481` | `GuardState` | `remove_component::<T>` |
  | `…:483` | `PatrolState` | `remove_component::<T>` |

  `remove_component::<T>(world, actor)` is a local four-line helper (`ai_package.rs:428`, duplicated at `combat.rs:404`). It does not contain the substring `remove::<`, which is why neither the guard nor the audit that wrote it saw these six. The same function also removes three *saved-but-not-overlaid* columns — `Seated`, `FollowState`, `EscortState`.
- **Evidence**: the guard body quoted above, complete. Production-only tree scan (test files and `#[cfg(test)]` tails stripped) over `remove::<T>` ∪ `remove_component::<T>` across `byroredux/src` + `crates/` yields exactly the seven rows tabulated. `grep -rn "fn remove_component"` returns `ai_package.rs:428`, `combat.rs:404`, and one test in `world_tests.rs`.
- **Impact**: Latent today, and honestly so — all six are NPC AI-procedure state, NPCs are destroyed and rebuilt by the cell reload, and the one entity that outlives a reload (the process-lifetime player body) never carries them. The additive-only overlay is therefore *correct* for these six. What is broken is the tripwire: the sole automated defence against the class that produced a HIGH one cycle ago is blind to the idiom used at six of seven sites, and green-ness is currently cited as evidence of coverage in this report and its predecessor. The day an AI-state column lands on the player, or a reload-surviving entity gains one, #3488 recurs with the guard still passing.
- **Related**: #3488 (the HIGH this guard was written for); #1847 / SAVE-04 (the additive-only contract); `delta_columns_carry_only_session_stable_fields` (the sibling tripwire, which does pin its set by equality and is not affected).
- **Suggested Fix**: Make the guard do what it says — walk the same file set the SAVE-D1-12 guard already walks (it has `collect_rs_files` and the `#[cfg(test)]`/`*_tests.rs` stripping ready to reuse), match `remove::<T>` **and** `remove_component::<T>`, and assert every hit whose `T` is in `MUTABLE_DELTA_COLUMNS` appears in `RECONCILED`. Then add the six `ai_package.rs` columns to `RECONCILED` with an explicit "no reconciler needed — cell reload rebuilds the carrier" reason rather than a reconciler name, so the exemption is stated instead of invisible. Failing that, at minimum correct the audit-half comment: it asserts a fact about the tree that is not true as written.

#### SAVE-D4-2026-08-30-03: `validate_animation`'s clip-handle and root-entity checks stop at `AnimationPlayer` — two sibling saved columns carry the same two reference classes with no gate at all

- **Severity**: MEDIUM
- **Dimension**: 4 — Validation Gates
- **Data-Loss Class**: latent corruption-on-load (defense-in-depth gap)
- **Location**: `crates/save/src/validate.rs:334-365` (`validate_animation`); `crates/core/src/animation/stack.rs:17-33` + `:108-112` (`AnimationLayer` / `AnimationStack`); `crates/core/src/ecs/components/sandbox.rs:54-104` (`Seated` / `SeatedAnimationRestore`); `byroredux/src/save_io.rs:104-112` (the exclusion rationale that names only one hazard)
- **Status**: NEW. The `Seated` half was created by `d2d5e067` (#3333), which added `Seated.animation_restore` as the required v9 field; the `AnimationStack` half has been latent since the column was registered but has never been enumerated by a save audit.
- **Description**: The skill's Dimension-4 instruction is to enumerate any inter-entity reference type not covered by an existing gate. `validate_animation` checks exactly two things, and only on `AnimationPlayer`:

  ```rust
  for (entity, player) in q.iter() {
      if let Some(reg) = registry.as_ref() {
          if reg.get(player.clip_handle).is_none() { … AnimationClip … }
      }
      if let Some(root) = player.root_entity {
          if root >= next_entity { … DanglingEntity … }
      }
  }
  ```

  Two other **registered, saved** columns carry the identical pair of reference classes and are inspected by none of the nine gates:

  1. **`AnimationStack`** — `root_entity: Option<EntityId>` plus `layers: Vec<AnimationLayer>`, each layer holding a `clip_handle: u32` (`stack.rs:18`). Both hazards, zero checks. It is forward-latent: the only `AnimationStack::new()` anywhere in the tree sits inside `#[cfg(all(test, feature = "inspect"))]` (`stack.rs:272`), so no production path populates it. That makes it cheap to fix and cheap to ignore — but it is registered, so a future producer inherits an unguarded column.
  2. **`Seated.animation_restore.clip_handle`** — this one **is** production-populated. `sandbox_seat_system` captures the pre-park `AnimationPlayer` state into it, and `clear_ambient_behavior` writes it straight back onto the live player (`npc_spawn/ai_package.rs:455-465`). It is an `AnimationClipRegistry` index — precisely the session-local handle class the allowlist rejects `AnimationClipRegistry` itself for ("numeric handles are session-local") — riding to disk inside a saved column with no gate on the way out.

  There is a second-order consequence worth naming separately. `MUTABLE_DELTA_COLUMNS`' exclusion rationale for `Seated` (`save_io.rs:104-112`) names only the `EntityId` hazard:

  > `FollowState`/`EscortState`/`Seated` are deliberately NOT here — they carry `EntityId` fields (`target_entity`/`furniture`) …

  `Seated.furniture` is exactly the kind of `EntityId` a FormId-keyed remap could legitimately be extended to resolve, since furniture is a placed REFR with a stable pair. A maintainer who does that work will read this comment, see the one hazard they just fixed, and have no way to learn that v9 quietly added a second, independent session-local-handle hazard to the same struct.
- **Evidence**: `validate.rs:336-365` quoted complete — `AnimationStack` and `Seated` appear nowhere in it, and `grep -n "AnimationStack" crates/save/src/validate.rs` returns nothing. `stack.rs:108-112` and `sandbox.rs:81-87` quoted for the field shapes. Production-only scan for `AnimationStack::new()` / `insert(.*AnimationStack)` across `crates/core/src`, `byroredux/src`, `crates/scripting/src` returns one hit, inside `#[cfg(all(test, feature = "inspect"))]`.
- **Impact**: No live loss today — neither column is overlaid, and `restore_world` (the only consumer that would replay a stale handle into a world) has no production callers. The defect is that the gate's coverage is asymmetric in a way nothing records: the identically-shaped `AnimationPlayer` is checked, its two siblings are not, and one of them was extended with a new handle field two commits ago without the gate moving. That is the drift the pre-save pass exists to catch.
- **Related**: #3333 (added `Seated.animation_restore` and the v9 bump); #1696 (the exclusion of `AnimationPlayer`/`AnimationStack` from the overlay, which names the `root_entity`/`clip_handle` hazard the gate then only half-checks); #1700 (the commit that widened `validate_world` past hierarchy+equipment).
- **Suggested Fix**: Extend `validate_animation` to walk `AnimationStack` (its `root_entity` through the existing `validate_entity_reference` helper, each layer's `clip_handle` through the same registry probe) and to check `Seated.animation_restore.clip_handle` — roughly fifteen lines reusing machinery already in the file. Separately, add the session-local-handle hazard to `Seated`'s exclusion rationale at `save_io.rs:104-112` so the comment lists both reasons the column stays off the overlay, not just the one that was true in 2026-08.

### LOW

#### SAVE-D6-2026-08-30-04: `save-load-roundtrip.md` still calls death "the one case wired today", one cycle after the second reconciler shipped

- **Severity**: LOW
- **Dimension**: 6 — M45.1 Live Load-Apply (documentation)
- **Data-Loss Class**: none
- **Location**: `docs/engine/save-load-roundtrip.md:188-198` (§6 step 7); `byroredux/src/save_io.rs:1420-1433` (the actual tail)
- **Status**: NEW — created by `fa511bbf` (#3488), which shipped the code without updating the companion doc.
- **Description**: §6 step 7 reads:

  > **Reconcile derived removals**: `combat::reconcile_dead_actor_runtime_state` (`byroredux/src/save_io.rs`, called immediately after step 6, both on success and on an apply failure). … **Death is the one case wired today** …

  Two things are now wrong. First, there are two reconcilers: `fa511bbf` added `crate::inventory::reconcile_player_equipped_weapon` in the same tail (`save_io.rs:1431-1433`), which is the whole subject of #3488 and the concrete second instance of the marker-plus-reconciler pattern the doc is trying to teach. Second, the parenthetical "both on success and on an apply failure" is true of the dead-actor reconciler (it runs in both arms, `:1420` and `:1446`) but not of the new one, which sits inside the `Ok` arm only — a deliberate asymmetry, since the `Err` arm aborts on an admittedly partial overlay, but one the doc now describes incorrectly for the step as a whole.
- **Evidence**: `docs/engine/save-load-roundtrip.md:188-198` and `save_io.rs:1418-1453` quoted side by side. The doc's own §"What's not covered" (`:210-226`) still frames removal support around `Dead` alone.
- **Impact**: Documentation only. It matters because this doc is the cross-cutting trace an implementer reads before touching the load tail, and it currently understates the pattern's adoption at exactly the moment the pattern acquired its second instance — the point at which "this is a pattern, not a one-off" became demonstrable.
- **Related**: #3488; #3022 (the `Dead` reconciler the doc describes correctly); #3028 (the previous doc-rot finding against the same file, fixed in `5458522d`).
- **Suggested Fix**: Update §6 step 7 to list both reconcilers, note that the equipped-weapon one runs on the success arm only and why, and adjust §"What's not covered" to say the pattern now has two instances. While in the file, correct `:222-224`'s reference-visibility claim per SAVE-D6-2026-08-30-01.

## Skill vs. Code (the skill is stale; the code is authoritative)

Three places where `/audit-save`'s own SKILL.md disagrees with HEAD. Per the
project's standing rule, the code wins and the disagreement is reported:

1. **Dimension 1 checklist — `ReferenceEnableState`.** The skill says it "has **no consumer anywhere in cell_loader/streaming yet** (`is_enabled` is called only from its own test module)", and instructs the auditor not to claim `Disable()` persists visibly. False at HEAD since `265f0c9b` (#3278): `cell_loader/spawn.rs:444-460` consumes it at every REFR spawn. This is load-bearing, not cosmetic — the now-live consumer is the mechanism of this cycle's HIGH, and an auditor who trusted the skill line would have skipped the ordering check. The skill file was itself edited in the newest commit (`64f64480`) and still carries the stale text.
2. **Dimension 4 checklist — "coverage vs. claim".** The skill describes `validate_world` as "FOUR reference classes … plus a FIFTH the binary layers on top". It is now **seven** core checks plus **two** binary ones: `validate_hierarchy`, `validate_equipment`, `validate_saved_entity_references`, `validate_animation`, `validate_inventory_instances`, `validate_progression_state`, `validate_material_finiteness`, `validate_form_ids`, `validate_cinematic_entity_refs`. The 2026-08-27 report already recorded nine; the skill was not updated.
3. **Doc-rot check — `docs/feature-matrix.md:189`.** The `TD3-002` comment the skill points at is at **line 297**, not 189. The substance of the instruction is still correct (the "unstarted" row is gone; `:187` and `:219` describe shipped state accurately), so this is a stale line reference, not doc rot — recorded so the next cycle does not go looking at the wrong line and file a phantom.

## Prior-Cycle Disposition (2026-08-27)

All five findings re-checked at HEAD line-by-line, not taken on the fix
commits' word:

| 2026-08-27 finding | Issue | State at HEAD `64f64480` |
|---|---|---|
| SAVE-D1-2026-08-27-01 — `EquippedWeapon` removal has no reconciler | #3488 | **FIXED, verified, not re-reported.** `fa511bbf` added `reconcile_player_equipped_weapon` (`inventory.rs:492-500`), called from the `apply_deltas` `Ok` arm (`save_io.rs:1428-1433`). It re-derives from the just-overlaid `EquipmentSlots.weapon` + `Inventory` + `InventoryCatalog`, and `InventoryCatalog` is rebuilt from the reloaded ESM index by `populate_scene_runtime` → `install_catalog` (`asset_provider/script.rs:328`) on both loader entry points, so the reconciler cannot fire against a stale or missing catalog. Two new guards ship with it: `delta_columns_removed_at_runtime_have_a_load_reconciler` (see SAVE-D1-2026-08-30-02 for its defect) and `player_body_component_the_save_lacks_is_not_removed_by_the_overlay`. |
| SAVE-D1-2026-08-27-02 — `Perks` allowlist reason cites a guard that never inspects `Perks` | #3491 | **OPEN, unchanged.** `registry_completeness_tests.rs:108` is verbatim identical; `validate_progression_state` (`validate.rs:424-442`) still reads only `CharacterLevel.xp`. Not re-reported. |
| SAVE-D1-2026-08-27-03 — `SCAN_ROOTS` cannot notice a new crate; `crates/sdk` unscanned | #3497 (also #3505) | **OPEN, unchanged.** `SCAN_ROOTS` is the same six entries; the outside-roots scan still returns `crates/sdk/src/studio.rs:120`, `crates/debug-ui/src/lib.rs:179`, `crates/renderer/src/vulkan/allocator.rs:49,70`. Not re-reported. |
| SAVE-D6-2026-08-27-04 — `FullRadius` worker-disconnect re-opens a narrow #3280 window | #3499 | **OPEN, unchanged.** `save_io.rs:1417-1418` still calls `build_form_id_remap` + `apply_deltas` with no guard on `state.pending`. Not re-reported. |
| SAVE-D3-2026-08-27-05 — `save.info` misreports every exterior save | #3500 | **OPEN, unchanged.** `save_io.rs:920-928` still has the two-arm match with `"<none — loose/exterior save>"`. Not re-reported. |

## Regression Guards Verified This Cycle

`cargo test -p byroredux-save` run live (30 unit + 15 integration, green). The
two binary-side source-scanning guards were **replicated statically** rather
than executed, to stay inside this run's memory budget; the replication uses
the same roots, the same exclusions, and the same assertion.

| Guard | Location | Invariant pinned | State |
|---|---|---|---|
| `every_component_or_resource_impl_is_saved_or_explicitly_allowlisted` | `save_io/registry_completeness_tests.rs` | every production `impl Component`/`impl Resource` under six roots is registered XOR allowlisted | **green (replicated)** — 252 impls, 47 registered, 235 allowlisted, 0 offenders. See #3497 for what the roots miss. |
| `saved_type_shape_changes_require_format_major_bump` | `save_io/serde_default_guard_tests.rs:341-351` | any field add/remove/retype on a saved struct requires a `FORMAT_MAJOR` bump; `BASELINE_MAJOR = 10`, `BASELINE_SHAPE_FINGERPRINT = 0x9b75_ff99_1abb_bf91` | **baseline current** — `git log -S` shows both constants change only in `19813460`, the same commit that moved `FORMAT_MAJOR` 9 → 10, and no later commit touches a saved type (`64f64480` is docs-only). |
| `serde_default_on_saved_struct_requires_format_major_bump` + the two shape/`cfg_attr` siblings | same file | no bare/`cfg_attr` `#[serde(default)]` on a save-participating type | **consistent** — implied green by the baseline above; discovery set is content-based (`cfg_attr(feature = "inspect"/"save"`) over five roots plus four explicit edges, so v9's nested `SeatedAnimationRestore` is inside it. |
| `delta_columns_carry_only_session_stable_fields` | `save_io/round_trip_tests.rs:28-84` | `MUTABLE_DELTA_COLUMNS` equals the hand-audited 21-entry set | **consistent** — the live constant matches `AUDITED` element-for-element. Pins *membership*, not removal semantics. |
| `delta_columns_removed_at_runtime_have_a_load_reconciler` | `save_io/round_trip_tests.rs:97-131` | *claims* to scan for production removals of delta columns | **green but non-functional** — SAVE-D1-2026-08-30-02. |
| `player_body_component_the_save_lacks_is_not_removed_by_the_overlay` | `crates/save/tests/round_trip.rs` | the overlay is additive-only from the *player body's* perspective — the #3488 companion | **green** (new this cycle) |
| `typed_snapshot_preflight_rejects_bad_column_without_world_mutation` | `crates/save/tests/round_trip.rs` | `validate_snapshot_types` runs before `clear_entities` and never touches the world on failure | **green** |
| `player_body_inventory_survives_live_load`, `delta_apply_reroutes_by_form_id_after_cell_reload`, `delta_apply_skips_unresolvable_form_id_without_disturbing_others`, `anim_player_root_entity_not_clobbered_by_delta_apply` | `crates/save/tests/round_trip.rs` | `PLAYER_FORM_ID_PAIR` resolution; form-id re-targeting; remap-miss isolation; #1696 exclusion | **green** |
| `restore_world_rejects_snapshot_with_out_of_bounds_entity_id`, `restore_world_does_not_abort_on_referentially_broken_snapshot` | `crates/save/tests/round_trip.rs` | `EntityIdOutOfBounds` is a real (non-`debug_assert`) gate; post-restore validation is diagnostic-only | **green** |
| `form_id_column_resolves_the_flagged_entry`, `form_id_column_is_none_without_registration`, `registering_a_second_form_id_column_panics` | `crates/save/src/registry.rs:408-447` | #1845's explicit `is_form_id` flag, not the old `apply.is_none()` heuristic | **green** |
| Container gates (`rejects_bad_magic` / `rejects_truncated` / `rejects_payload_truncation` / `detects_crc_corruption` / `rejects_schema_mismatch` / `rejects_major_version_skew`) | `crates/save/src/snapshot.rs:240-312` | every header gate precedes `serde_json::from_slice`; CRC is payload-only so version skew stays reachable | **green** |
| `atomic_write_replaces_the_target_and_consumes_the_temp`, `atomic_write_fails_without_renaming_when_the_temp_cannot_be_created` | `crates/save/src/disk.rs:248-299` | #3472's shared durable sequence: temp consumed, target fully replaced, failed write leaves no target | **green** (new this cycle) |
| `write_read_round_trip_and_atomic_rename`, `latest_slot_ignores_newer_tmp_and_empty_directory`, `recency_tie_breaks_by_slot_number`, `cursor_after_newest_points_past_latest_mtime`, `resume_on_empty_dir_starts_at_zero`, `parse_slot_names` | `crates/save/src/disk.rs` | atomic rename, tmp exclusion, deterministic recency, ring resume | **green** |
| `material_with_non_finite_scalar_trips_the_gate` + `sanitize_finite` siblings | `crates/save/src/validate.rs`, `crates/core/src/ecs/components/material.rs` | #2687/#3373 NaN prevention on save + repair on restore, field-list parity | **green** |
| `the_save_dir_override_falls_back_on_absent_and_empty_values` | `save_io.rs:244-265` | #3009's `BYROREDUX_SAVE_DIR` override never resolves to the process cwd | **green** (new this cycle) |

## Disproved Candidates (investigated, not filed)

Nine candidates were chased to the code and dropped. Recorded so a later cycle
does not re-derive them:

- **"`Globals` is consumed during the reload before `restore_resources` overwrites it — the same defect as SAVE-D6-2026-08-30-01."** Rejected. `ensure_globals_resource` (`cell_loader/load.rs:169-179`) is guarded by `is_none()`, so the reload does not rebuild `Globals` from ESM at all; the live resource survives and is then overwritten by the saved value. No load-time consumer of `Globals` exists on the spawn path. End state is correct. (Note: the 2026-08-27 report's claim that `Globals` "is re-installed fresh from ESM by `cell_loader/load.rs:177` on the reload" is inaccurate as written — the outcome it describes is right, the mechanism is not.)
- **"The #3488 reconciler silently unequips whenever `InventoryCatalog` lacks the weapon, discarding the `EquippedWeapon` row `apply_deltas` just overlaid."** Rejected on reachability. `InventoryCatalog` is rebuilt from the reloaded plugin index by `populate_scene_runtime` → `install_catalog` (`asset_provider/script.rs:328`), which both loader entry points funnel through by construction (the #3010 fix explicitly centralised it there), and `LoadedPluginSet` is the save's own master list — so the catalog present at reconcile time is derived from the same plugins the save was taken with.
- **"`EquippedWeapon` is now dead weight in `MUTABLE_DELTA_COLUMNS`, since the reconciler overwrites it."** Rejected — the reconciler runs for the player only (`PlayerEntity`), and NPCs' `EquippedWeapon` rows still need the overlay.
- **"#3472 regressed the save path's directory fsync by changing the target from `dir` to `final_path.parent()`."** Rejected — for `write_slot` the two are the same path by construction, and `discover_save_dir()` cannot yield an empty root (`save_dir_from` falls back to `saves` on both absent and empty).
- **"`unload_current_interior` releases `ItemInstanceId`s into the pool that `restore_resources` is about to replace, corrupting the restored arena."** Rejected — the release happens *before* the wholesale replacement (`cell_loader/unload.rs:500-519` then `save_io.rs:1411`), so the releases land on the discarded pool and are thrown away with it. The constraint is real in the *other* direction, and is why the fix for SAVE-D6-2026-08-30-01 cannot simply hoist `restore_resources`.
- **"`QuestAliasInjectionState` carries `EntityId` keys (`HashMap<(EntityId, u32), _>`) into a saved resource with no validation."** Rejected — the field is `#[cfg_attr(feature = "save", serde(skip, default))]` (`crates/scripting/src/scene/quest_alias.rs:88`), matching the registry's own comment.
- **"`SaveLoadNotifications` grows unbounded in a headless/bench run where no debug UI is installed."** Rejected again this cycle. `app_frame.rs:104-108` `mem::take`s unconditionally and `if let Some(ui)` gates only the display; and the drain site (`render_one_frame`, gated on window+renderer) is reachable exactly when the producer is, since `execute_pending_save_loads` requires a `&mut VulkanContext`.
- **"`atomic_write`'s `if let Ok(dir_file) = fs::File::open(dir)` silently skips the durability step on any open failure, not just on platforms that cannot open directories."** Rejected as a finding — true, but it is the documented shape SAVE-D3-01 shipped and the alternative (hard-failing a completed rename on an `EACCES` directory) is worse. Recorded, not filed.
- **"`AnimationStack` is a live silent-drop: registered and saved but excluded from the overlay, so its state is lost on every live load."** Rejected — the column has no production insertion site at all (its only constructor is under `#[cfg(all(test, feature = "inspect"))]`, `stack.rs:272`). The *validation* half of the same investigation survived as SAVE-D4-2026-08-30-03.

## Deduplication

`gh issue list --repo matiaszanolli/ByroRedux --limit 400 --state all` (fetched
this run, not reused from the prior cycle's scratch) searched for `save`,
`load`, `snapshot`, `registry`, `delta`, `overlay`, `formid`, `serde`,
`quicksave`, `quickload`, `ring`, `corrupt`, `disable`, `enable`,
`referenceenable`, `animationstack`, `validate_animation`, `reconciler`,
`remove_component`, `clip_handle`, `seated`, `perk`, `sdk`, `scan_root`. No
open issue overlaps any of the four findings. The nearest neighbours are
**#3489** (`Effect::Disable` has no `Enable` counterpart — same resource,
different defect: that is about the effect vocabulary being one-way, this is
about load ordering) and **#3488** (CLOSED, the reconciler whose *guard* is
finding 02). `docs/audits/` scanned: no prior `AUDIT_SAVE_*` report mentions
`ReferenceEnableState` ordering, `remove_component`, `AnimationStack`
validation, or `SeatedAnimationRestore`.
