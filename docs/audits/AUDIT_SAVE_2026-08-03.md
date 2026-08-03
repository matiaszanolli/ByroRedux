# Save / Load Subsystem Audit (M45 + M45.1) — 2026-08-03

Scope: `crates/save/src/*` (~1.2k LOC) and its sole engine-side consumer
`byroredux/src/save_io.rs` (1,908 LOC), audited at HEAD `1ae86f62`. This is the
**sixth** save audit (prior: `2026-06-23`, `2026-07-02`, `2026-07-03`,
`2026-07-16`, `2026-07-25`). Rather than delegating to six parallel dimension
agents, every file in `crates/save/src/` was re-read against the prior
(2026-07-25) report, the full diff since that report's HEAD (`ca7a4e0e`,
122 commits) was inspected, and the entire subsystem was cross-checked against
what actually changed — not re-derived from scratch, since five prior clean
passes already established the crate-internal invariants hold.

**Headline: `crates/save/src/*` itself had ZERO diff since the last audit** —
every finding this cycle is new, and all of it comes from **new persistent ECS
state added elsewhere in the codebase** (a large M47.3 scripting expansion —
`crates/scripting/src/{scene,dialogue,package,cinematic,player_control,
vm_state}.rs`, ~4,600 new LOC, plus `crates/core/src/ecs/components/
actor_state.rs`) that was never plumbed into `build_save_registry`. This is
exactly the failure mode Dimension 1 exists to catch, and exactly why an audit
must re-walk the *whole* component surface each cycle rather than trust a
crate that "looks unchanged."

## Executive Summary

Docstring design claims (`crates/save/src/lib.rs`) verified against live code:

| Claim | Status |
|---|---|
| Full ECS snapshot | **DRIFTED** — two real, live-reachable, script-driven components (`TwoStateActivator`, `ScriptVariables`) carrying persistent per-object gameplay state are absent from `build_save_registry`. See SAVE-D1-08. |
| Atomic write (tmp → fsync → read-back → rename → dir-fsync) | **CODE-CONFIRMED** — `crates/save/src/disk.rs` has zero diff since the 2026-07-25 line-by-line verification; re-spot-checked, unchanged. |
| Ring (never clobbers the last good save) | **CODE-CONFIRMED** — unchanged since 2026-07-25 (`SaveRing::resume`, validation-gated `advance()`). |
| Validation gate (refuse to persist inconsistent state) | **CODE-CONFIRMED** — unchanged; `crates/save/src/validate.rs` has zero diff since 2026-07-25. |
| Off-frame load (never runs inside the scheduler) | **CODE-CONFIRMED**, and **improved** this cycle — `#1848` (a second `load` before the drain silently discarding the first) is now fixed: the supersede is reported on both the console and the log (`997c2418`). |

**Findings this cycle: 5 new. 0 CRITICAL, 1 HIGH, 3 MEDIUM, 1 LOW.**
Two prior open items re-verified: `#1848` (CLOSED, fix holds) and
`SAVE-D2-NEW-07`/`#2181` (CLOSED, fix holds, guard is now key-position-
independent). One pre-existing OPEN issue (`#2154`, concurrency-owned, not a
save-crate defect) re-confirmed still open, not re-filed.

By Data-Loss Class: **silent-drop — 4** (SAVE-D1-08 HIGH, SAVE-D1-09 MEDIUM,
SAVE-D1-10 MEDIUM forward-latent, SAVE-D1-11 LOW); **none — 1**
(SAVE-D1-12, process/guard-coverage gap, not itself a data-loss path).

`cargo test -p byroredux-save` (33 tests: 20 unit + 13 integration) and
`cargo test --bin byroredux save_io::` (22 tests, up from 16 — six new tests
from the `#1848`/`#2181` fixes) both **pass 100%**.

## Regression Verification Ledger (2026-07-25 → today)

| Finding | Issue | Fix commit | Re-verified today |
|---|---|---|---|
| `#1848` (second `load` before drain silently discards the first) | #1848 | `997c2418` | **HOLDS** — `PendingSaveLoadSlot` grew a `slot: u32` field alongside `snapshot`; `LoadCommand::execute` now detects `pending.snapshot.is_some()` before overwriting, logs `"queued load of slot {prev} superseded by slot {slot} before drain"` via `log::info!`, and echoes it in the command output. New test `second_load_before_drain_supersedes_and_reports` (`byroredux/src/save_io.rs`) issues two same-frame loads from two different saved cells and asserts the surviving snapshot is identifiably the second one, with the supersede named on the second response only. Idempotency of the single-load drain (`.take()`) is unchanged. |
| `SAVE-D2-NEW-07` (`#1714` guard's `#[serde(default)]` detection was a line-prefix match, missed non-first-key ordering) | #2181 | `8709e12d` | **HOLDS** — the guard now calls `serde_attr_declares_default(line)`, which parses the attribute's comma-separated key list (string-literal-aware) rather than string-prefix-matching, so `#[serde(skip_serializing_if = "...", default)]` trips it regardless of key position. Five new tests cover: any-position `default`, the original first-key forms (must still catch), `default` appearing only as a *value* (must NOT false-positive), non-attribute mentions (doc comments, string literals — must NOT false-positive), and commas inside string literals (must not be treated as key separators). |

## Data-Loss Class Matrix

| Finding | Class | Dimension | Severity |
|---|---|---|---|
| SAVE-D1-08 (`TwoStateActivator`/`ScriptVariables` unregistered) | silent-drop | 1 — Snapshot Completeness | HIGH |
| SAVE-D1-09 (`PlayerControlState`/`ActorControlState` unregistered) | silent-drop | 1 — Snapshot Completeness | MEDIUM |
| SAVE-D1-10 (`Dead` marker unregistered, forward-latent) | silent-drop (not yet exploitable) | 1 — Snapshot Completeness | MEDIUM |
| SAVE-D1-11 (Scene/Dialogue/Package playback progress undocumented-omitted) | silent-drop (likely self-healing, undocumented) | 1 — Snapshot Completeness | LOW |
| SAVE-D1-12 (registry-completeness guard only covers NPC-spawn-stamped components) | none (process gap, not itself lossy) | 1 — Snapshot Completeness | MEDIUM |

## Completeness Ledger (delta from 2026-07-25)

All 2026-07-25 rows hold unchanged (re-verified: `Transform`, `Inventory`,
`EquipmentSlots`, `LightSource`/`LightFlicker`, `ScriptTimer`, `ActorValues`,
the nine M42 AI-procedure components, `FormIdComponent`, and the five
resources). New rows this cycle, none of which are in `build_save_registry`:

| Type | Kind | Registered (saved) | In `MUTABLE_DELTA_COLUMNS` | Status |
|---|---|---|---|---|
| `TwoStateActivator` (`crates/scripting/src/vm_state.rs:50-58`) | Component | **no** | n/a | **MISSING — SAVE-D1-08** |
| `ScriptVariables` (`crates/scripting/src/vm_state.rs:20-28`) | Component | **no** | n/a | **MISSING — SAVE-D1-08** |
| `TwoStateTransitionBatch` (`vm_state.rs:66-72`) | Component | no | n/a | correctly transient (one-frame presentation event, drained same tick — same class as `ActivateEvent`) |
| `PlayerControlState` (`crates/scripting/src/player_control.rs:44-56`) | Resource | **no** | n/a | **MISSING — SAVE-D1-09** |
| `ActorControlState` (`player_control.rs:110-117`) | Component | **no** | n/a | **MISSING — SAVE-D1-09** |
| `Dead` (`crates/core/src/ecs/components/actor_state.rs:8-15`) | Component | **no** | n/a | **MISSING — SAVE-D1-10** (forward-latent) |
| `ScenePlayer` (`crates/scripting/src/scene.rs:142-178`) | Component | no | n/a | undocumented — **SAVE-D1-11** |
| `DialoguePlayback` (`crates/scripting/src/dialogue.rs:85-92`) | Component | no | n/a | undocumented — **SAVE-D1-11** |
| `ScenePackagePlayback` (`crates/scripting/src/package.rs:131-138`) | Component | no | n/a | undocumented — **SAVE-D1-11** |
| `ActorCinematicState` / `HorseTetherState` / `CinematicPresentationState` (`cinematic.rs`) | Component/Resource | no | n/a | scoped to the MQ101 cinematic demo slice, one-shot/serial-tracked; not flagged as a separate finding (narrow, likely non-savepoint-adjacent), but shares the same undocumented-omission pattern as SAVE-D1-11 |
| `PackageRegistry` / `PackageTargetRegistry` / `DialogueRegistry` / `SceneRegistry` / `SceneQuestAliasRegistry` | Resource | no | n/a | correct — static, parsed from ESM at boot/cell-load, no runtime mutator, same class as `AnimationClipRegistry` |
| `*EventBatch` / `*CompletionBatch` types (scene/dialogue/package) | Component | no | n/a | correct — one-frame transient event markers, same class as `ActivateEvent`/`HitEvent` |

## Findings

### HIGH

#### SAVE-D1-08: `TwoStateActivator` + `ScriptVariables` — live, script-driven per-object state — are absent from `build_save_registry`
- **Severity**: HIGH
- **Dimension**: Snapshot Completeness & Determinism
- **Data-Loss Class**: silent-drop
- **Location**: `crates/scripting/src/vm_state.rs:20-58` (struct defs), `:129` (`two_state_activator_system`), `crates/scripting/src/translate/recognizers/two_state_activator.rs` (recognizer), `byroredux/src/boot.rs:681` (scheduler wiring), `byroredux/src/save_io.rs:188-249` (`build_save_registry`, neither type present)
- **Status**: NEW
- **Description**: `default2StateActivator` is a real, ubiquitous vanilla Skyrim Papyrus script class (levers, switches, portcullis triggers, puzzle doors). This session's scripting expansion added a recognizer (`recognizers::two_state_activator::recognize`, wired into the always-on dispatch table at `crates/scripting/src/translate/mod.rs:51`) that converts real ESM-authored instances of that script into two ECS components — `TwoStateActivator` (is_open/is_animating/do_once/activated_once) and `ScriptVariables` (a `HashMap<ConditionStringId, f32>` backing `GetVMScriptVariable` CTDA reads of `::isOpen_var`/`::isAnimating_var`) — and a system, `two_state_activator_system`, that is unconditionally registered in the default scheduler at `byroredux/src/boot.rs:681` (not gated behind an opt-in env var the way the M42 AI procedures are). `world.register::<Dead>()`-style ECS registration for these two types happens via `byroredux_scripting::register(&mut world)` (`boot.rs:493`), confirming they are live, spawnable component types in the running engine today — not scaffolding for a future feature.
  Neither type appears anywhere in `build_save_registry` (`byroredux/src/save_io.rs:188-249`). A player who pulls a lever, flips a switch, or opens a puzzle door driven by this script class, then saves and reloads, will find every such object silently reverted to its ESM-authored default state — the exact "invisible corruption" class this subsystem exists to prevent, now reachable through a path that didn't exist at the 2026-07-25 audit.
- **Evidence**:
  ```rust
  // crates/scripting/src/vm_state.rs
  pub struct ScriptVariables { values: HashMap<ConditionStringId, f32> }
  impl Component for ScriptVariables { type Storage = SparseSetStorage<Self>; }
  pub struct TwoStateActivator { pub is_open: bool, pub is_animating: bool, pub do_once: bool, pub activated_once: bool }
  impl Component for TwoStateActivator { type Storage = SparseSetStorage<Self>; }
  ```
  ```rust
  // byroredux/src/boot.rs:681 — unconditional, default scheduler
  byroredux_scripting::two_state_activator_system,
  ```
  ```rust
  // byroredux/src/save_io.rs:188-249 — build_save_registry: no TwoStateActivator, no ScriptVariables
  ```
- **Impact**: Any interactable object recognized as a `default2StateActivator` instance loses its open/closed/animating state across every save/load cycle. Blast radius scales with how much real vanilla content this recognizer already matches (levers and switches are common across interiors); this is core, expected, always-visible gameplay state, not an edge case.
- **Related**: Same class as the fixed `#1834` (`ActorValues`) and `#1862` (`QuestStageState`) — a runtime-mutated component the registry hadn't caught up to yet. Distinguished from the deliberately-excluded `FollowState`/`EscortState`/`Seated` pattern (`#1696`) because neither `TwoStateActivator` nor `ScriptVariables` carries a session-local `EntityId`/handle — both are plain-data (`bool`/`f32` keyed by a stable `ConditionStringId`), so they are delta-safe and belong in `MUTABLE_DELTA_COLUMNS` too, not just `build_save_registry`.
- **Suggested Fix**: `.register_component::<ScriptVariables>("ScriptVariables")` and `.register_component::<TwoStateActivator>("TwoStateActivator")` in `build_save_registry`; add both names to `MUTABLE_DELTA_COLUMNS` (both are plain-data, no `EntityId`/`FixedString`, so they pass the `delta_columns_carry_only_session_stable_fields` tripwire). Add a round-trip test analogous to `ai_procedure_state_and_terminal_markers_survive_save_load_round_trip`.

### MEDIUM

#### SAVE-D1-09: Player-control-lock state (`PlayerControlState`/`ActorControlState`) written by recognized `EnablePlayerControls`/`DisablePlayerControls`/`SetRestrained` calls is absent from the registry
- **Severity**: MEDIUM
- **Dimension**: Snapshot Completeness & Determinism
- **Data-Loss Class**: silent-drop
- **Location**: `crates/scripting/src/player_control.rs:44-56` (`PlayerControlState`, a `Resource`), `:110-117` (`ActorControlState`, a `Component`), `crates/scripting/src/translate/effects.rs:589-606` (`prim_player_controls`, the `EnablePlayerControls`/`DisablePlayerControls`/`SetRestrained` lowering, reached from `fragment.rs:54`'s `lower_fragment` — the live recognized-effect dispatch), `byroredux/src/save_io.rs:188-249` (absent from registry)
- **Status**: NEW
- **Description**: Skyrim quest-intro scripts routinely call `Game.DisablePlayerControls(...)`/`Game.EnablePlayerControls(...)` to lock movement/fighting/menu/etc. during a scripted sequence, and `Actor.SetRestrained(...)` to freeze an NPC. This session's `translate/effects.rs` recognizes both call families (not test-only — reached via `fragment.rs`'s live `lower_fragment`, the same dispatch path real VMAD fragment effects go through) and writes them into `PlayerControlState` (a `Resource`, default all-enabled) and `ActorControlState` (a per-actor `SparseSetStorage` component, default `restrained: false`). Neither is in `build_save_registry`. A save taken mid-scripted-sequence (while controls are locked or an NPC is restrained) reloads with both silently reset to their defaults.
- **Evidence**: `player_control.rs:78`: `impl Resource for PlayerControlState {}` with no corresponding `register_resource` call anywhere in `save_io.rs`.
- **Impact**: Narrower than SAVE-D1-08 — the window is only "mid-cutscene, controls locked" rather than "any permanently-toggled world object." Self-correcting in the common case (the driving quest stage, which IS saved via `QuestStageState`, will typically re-assert or release the lock as the scene progresses on reload), but a save taken in the exact locked window loses the lock/restrain flag, which could let a player walk during a sequence intended to hold them in place, or leave a restrained NPC free to move.
- **Suggested Fix**: `.register_resource::<PlayerControlState>("PlayerControlState")` and `.register_component::<ActorControlState>("ActorControlState")`; both are plain `bool`/`i32` fields, delta-safe for `MUTABLE_DELTA_COLUMNS` too.

#### SAVE-D1-10: `Dead` actor-lifecycle marker (new component, `crates/core/src/ecs/components/actor_state.rs`) is unregistered — forward-latent, not yet exploitable
- **Severity**: MEDIUM
- **Dimension**: Snapshot Completeness & Determinism
- **Data-Loss Class**: silent-drop (no live trigger today)
- **Location**: `crates/core/src/ecs/components/actor_state.rs:8-15` (new file this cycle), `crates/scripting/src/condition.rs:484-485` (`GetDead` CTDA reads it), `byroredux/src/boot.rs:414` (`world.register::<Dead>()`), `byroredux/src/save_io.rs:188-249` (absent)
- **Status**: NEW
- **Description**: `Dead` is a sparse marker ("absence means alive") added this cycle so combat, scripts, resurrection, and condition evaluation share one source of truth. It is registered as an ECS type (`boot.rs:414`) and consumed by the `GetDead` condition function (`condition.rs:484-485`), but **nothing in the live codebase currently inserts it** outside of `condition.rs`'s own unit test (`world.insert(dead, Dead)` at `condition.rs:1082`) — `crates/core/src/combat.rs` contains only pure damage-formula helpers (Oblivion/FNV weapon-damage/unarmed math), with no death-resolution system that would ever set it during real gameplay. Verified no combat/kill system exists anywhere in `crates/` or `byroredux/src/systems/`.
- **Evidence**: `grep -rn "insert(.*Dead)"` across the whole tree returns exactly one production-adjacent site, and it is `#[cfg(test)]`.
- **Impact**: None today — there is no live path that sets `Dead`, so nothing is lost by a save/load. This is flagged as **forward-latent**: the moment a combat/death-resolution system lands, a dead NPC reviving on every load is a much worse variant of the exact bug class SAVE-D1-08 demonstrates is easy to introduce silently, and there is currently no tracking issue reserving the follow-up.
- **Suggested Fix**: No urgent code change required. File a tracking issue ("register `Dead` in `build_save_registry` in the same commit that ships the first system to insert it") so the dependency is explicit, rather than relying on whoever ships combat to remember this audit.

#### SAVE-D1-12: The only automated registry-completeness guard covers NPC-spawn-stamped components only — script/system-inserted components have zero static coverage
- **Severity**: MEDIUM
- **Dimension**: Snapshot Completeness & Determinism
- **Data-Loss Class**: none (this is a process/tooling gap, not itself a data-loss path — but it is *why* SAVE-D1-08/09/10 went unnoticed through the M47.3 scripting landing)
- **Location**: `byroredux/src/save_io.rs:1142-1177` (`npc_spawn_stamped_components_are_saved_or_intentionally_rederived`)
- **Status**: NEW
- **Description**: The guard's own doc-comment states its scope precisely: "Persistent gameplay-state components stamped on the placement root by `spawn_npc_entity` + its `stamp_*` helpers... Pure placement scaffolding, GPU handles, and transient markers are out of scope." Its `NPC_SPAWN_STAMPED` list is nine names, manually maintained, and only cross-references what `npc_spawn.rs` writes at actor-creation time. It has no visibility into components a *system* inserts later during gameplay (script recognition, condition evaluation, package/scene execution) — precisely the class `TwoStateActivator`/`ScriptVariables`/`ActorControlState`/`Dead` all belong to. There is no companion guard scanning, e.g., every `impl Component for` in `crates/scripting/src/` or `crates/core/src/ecs/components/` against the registry.
- **Evidence**: `NPC_SPAWN_STAMPED` (`save_io.rs:1147-1157`) lists exactly `Transform`, `Name`, `Inventory`, `EquipmentSlots`, `ActorValues`, `FactionRanks`, `CharacterLevel`, `Background`, `Perks` — none of the six new scripting types from this cycle could ever appear here even in principle, since none of them are stamped by `spawn_npc_entity`.
- **Impact**: This is the structural reason five consecutive clean save audits (2026-06-23 → 2026-07-25) did not anticipate SAVE-D1-08/09/10 — the crate itself hadn't changed, but the completeness net doesn't extend past spawn-time components, so an entire new persistent-state surface landed with zero tripwire. Every future scripting/system feature that adds a mutable component repeats this risk unless a broader check exists.
- **Suggested Fix**: A source-scan guard (same manually-maintained-allowlist philosophy as the existing two guards, not full reflection) that greps every `impl Component for` / `impl Resource for` across `crates/core/src/ecs/components/`, `crates/scripting/src/`, `crates/physics/src/`, etc., and requires each name to appear in `build_save_registry`'s registered set OR an explicit `NOT_SAVED_BY_DESIGN` allowlist with a one-line reason per entry (mirroring `REDERIVED_NOT_SAVED`'s per-entry justification). This is exactly the kind of blind-spot the `#1714`/`#2181` serde-guard lineage already treats as worth closing for a narrower surface (attribute detection); the same discipline applied to component registration would have caught this cycle's gap at PR time instead of at audit time.

### LOW

#### SAVE-D1-11: Scene/Dialogue/Package mid-playback progress is omitted from the registry without the `#1696`-style documented rationale
- **Severity**: LOW
- **Dimension**: Snapshot Completeness & Determinism
- **Data-Loss Class**: silent-drop (likely self-healing, but undocumented)
- **Location**: `crates/scripting/src/scene.rs:142-178` (`ScenePlayer`: `scene_form_id`, `state`, `current_phase`, `active_actions`, `completed_actions`), `crates/scripting/src/dialogue.rs:85-92` (`DialoguePlayback`), `crates/scripting/src/package.rs:131-138` (`ScenePackagePlayback`)
- **Status**: NEW
- **Description**: `ScenePlayer` is explicitly documented in its own doc-comment as "Persistent playback state for one scene definition," tracking which phase a Bethesda `SCEN` scene has reached and which numbered actions have completed (needed because `IsSceneActionComplete` can be queried after an action leaves the active set). This is meaningful mid-progress state, structurally similar to `AnimationPlayer`/`AnimationStack`, which were deliberately excluded from the live-overlay path (`#1696`) with an explicit code comment citing the issue and explaining why (the reloaded cell's systems reconstruct the equivalent state from scratch). `ScenePlayer`/`DialoguePlayback`/`ScenePackagePlayback` have no equivalent comment, issue reference, or registry entry — the omission may well be the same intentional call (a reloaded cell's package/quest re-evaluation likely restarts or re-derives scene state from the saved `QuestStageState`), but nothing in the code states that decision was made rather than simply not yet made.
- **Evidence**: `grep` for `#1696`-style citations near `ScenePlayer`/`DialoguePlayback`/`ScenePackagePlayback` definitions returns nothing.
- **Impact**: Low today — likely self-correcting via quest-stage-driven scene re-entry, and scenes/dialogue are not typically save-point-adjacent gameplay in the way lever state is. But an undocumented omission is indistinguishable from an oversight on the next read, which is exactly the ambiguity `#1696`'s comment exists to remove.
- **Suggested Fix**: Either register all three (low cost, they're plain data plus one `HashSet<u32>`) or add a one-line comment at each definition site citing this finding and stating explicitly why a cell reload safely reconstructs the equivalent state.

## Known Open Issue (Cross-Referenced, Not Re-Filed)

- **`#2154` / SAVE-D3-02** — `SaveCommand::execute` holds `ResourceRead<SaveRegistry>` + `ResourceWrite<SaveState>` across the entire ~30-storage validate+save walk. Filed by `/audit-concurrency` (2026-07-25), not a save-crate correctness defect — safe today only because command dispatch is exclusive-scheduled, an invariant not restated at the call site. Re-confirmed still OPEN, still reproducible by direct code reading, not re-filed here (owned by the concurrency audit's dimension, not this one).

## Verified Clean — No New Findings (Unchanged Since 2026-07-25)

- **Disk Format & Durability** (`crates/save/src/disk.rs`, `snapshot.rs`): **zero diff** since the prior audit's line-by-line verification (`git diff --stat ca7a4e0e..HEAD` shows no changes to either file). Atomic write dance, header gate ordering, CRC scope, ring resume-from-mtime, and `parse_slot_filename` strictness all re-spot-checked, all hold.
- **Registry & (De)serialization Fidelity** (`crates/save/src/registry.rs`): **zero diff**. `FnvHasher` constants, `form_id_column()`'s explicit-flag keying, `register_form_id_component`'s no-panic-on-unresolvable-handle behavior all unchanged.
- **Validation Gates** (`crates/save/src/validate.rs`): **zero diff**. All five reference classes (Hierarchy, Equipment, Animation, ItemInstance, FormId) still run pre-write in `SaveCommand::execute`; post-load diagnostic re-run still wired into both restore paths.
- **Frame-Boundary Capture & Off-Frame Apply**: unchanged — `capture_player_pose` still runs immediately before `step_save_loads()` every frame; `execute_pending_save_loads` remains the sole `&mut World` consumer, structurally unreachable from inside a system.
- **M45.1 Live Load-Apply**: apply ordering (drain → resolve `CurrentCellContext` → pre-flight → teardown → reload → `restore_resources` → `build_form_id_remap` → `apply_deltas` → post-load validation → `apply_player_pose`) unchanged and re-confirmed. `PLAYER_FORM_ID_PAIR` still attached at spawn.

## Regression Guards Discovered / Reconfirmed

All guards listed in the 2026-07-25 report still exist and pass. New this cycle:

| Test | Invariant it pins |
|---|---|
| `second_load_before_drain_supersedes_and_reports` (#1848) | A second same-frame `load` names both the superseded and surviving slot, on both the log and the command output; the drain still applies only the surviving snapshot |
| `serde_guard_catches_default_in_any_key_position` / `serde_guard_still_catches_the_original_first_key_forms` / `serde_guard_ignores_default_appearing_only_as_a_value` / `serde_guard_ignores_non_attribute_mentions` / `serde_guard_does_not_split_on_commas_inside_string_literals` (#2181) | `serde_attr_declares_default` catches `default` in any key position, doesn't false-positive on the word appearing as a value or in prose, and correctly treats commas inside string literals as non-separators |

## Doc-Rot Observations (Not Filed as Findings)

- `docs/feature-matrix.md:189`'s `TD3-002` comment (Save/load M45/M45.1 shipped
  2026-06-21) re-confirmed still reads correctly, per the skill's explicit
  instruction not to re-flag it.
- The stale `main.rs` line-number references in `SKILL.md`/`_audit-common.md`
  noted in the 2026-07-25 report are unchanged (still stale, still
  non-blocking — the invariants they describe are still accurate, only the
  file/line pointers rotted pre-2026-07-25).

---

**Net assessment**: the M45/M45.1 subsystem's own machinery (registry
plumbing, disk durability, validation gates, frame-boundary discipline,
live-apply ordering) remains provably solid — zero drift in the crate itself,
two prior LOW findings fixed cleanly with good test coverage. All new findings
this cycle trace to one root cause: a large scripting feature landed real,
live, player-mutable ECS state (`crates/scripting/src/vm_state.rs` and
siblings) without a corresponding save-registry update, and the project's only
completeness tripwire (SAVE-D1-12) structurally cannot see that class of
addition. The fix is two-fold: register the concrete gap (SAVE-D1-08, HIGH)
and close the process gap that let it in undetected (SAVE-D1-12, MEDIUM) —
otherwise the next scripting feature repeats this exact cycle.

Suggested next step: `/audit-publish docs/audits/AUDIT_SAVE_2026-08-03.md`
