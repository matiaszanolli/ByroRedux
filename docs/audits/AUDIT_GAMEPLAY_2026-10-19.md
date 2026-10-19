**HEAD**: 41f9c2c6c · **Baseline**: `AUDIT_GAMEPLAY_2026-10-15.md` (`9e11eb1fb`) · **Audited**: Dim 2 (persistence half: `reference_state`, `stream_snapshot`, `unload`, `synth_child`/`attach`), Dim 5 (spawn → AI package selection, `resumable/`, `ai_package.rs`, #5408 head-part fallback), Dim 7 (saved-or-rederived across the stream boundary, stage order, determinism); scope=streaming-deep AREA-SCOPED · **Unchanged since baseline (skimmed)**: Dim 1 (`loot_appearance.rs` has no commits since 10-15; guard tests green) · **Out of area (one line each)**: Dim 3 (consumables), Dim 4 (combat damage path, `combat.rs` / `systems/combat_*`; only the area's death producers `restore` + `apply_starts_dead` were read), Dim 6 (player feedback)

# Gameplay Audit — 2026-10-19 (streaming-deep, AREA-SCOPED)

**This report covers only `byroredux/src/streaming/`, `npc_spawn/`, `cell_loader/` and the callers/callees needed to follow a data flow. It is NOT a full-codebase gameplay audit.** The next full `/audit-gameplay` run must not treat it as a baseline for the dimensions it skipped (3, 4, 6, and the non-area halves of 1, 2, 5, 7).

## Scope and evidence

- Delta since the 10-15 baseline inside the area: #5859 (bleedout in the eviction row), #5861 / #5870, #5523, #5496, #6051 + `AddScriptPackage` (`af5db229d`), #5408 head parts (`2a8663f39`), and the M47.3 ObScript object-script wiring (`da2676b70`, `6b9397ed2`, `d1b298921`, `07bd22a6b`). All were read.
- **Baseline run** (`cargo test -p byroredux --bin byroredux -- npc_spawn:: reference_state ambient_locomotion walk_anim eat_sleep forcegreet dialogue_voice save_io:: synth_child scripted_lock_gate inventory_release stream`, built in the shared repo `target/`): **380 passed, 0 failed, 17 ignored**. The green set does not cover any finding below; each one lives on a path no test builds (an eviction → respawn cycle for a fact outside `ReferenceState`).
- **Not checked** (no engine launch, per the suite rules): every smoke script (`m42-eat-sleep.sh`, `m47-2-bleedout.sh`, `p5-*.sh`, `m47-3-obscript.sh`), anything needing a Vulkan device, and any real-data `--ignored` test. All four findings are static: code read plus the cited tests' shapes. No cell was loaded.
- **Dedup**: `/tmp/audit/issues.json` (121 open), closed-issue keyword searches (`2StateActivator`, `ScriptVariables`, `evicted`, `eviction`, `ReferenceState`, `ScriptPackage`, `AddScriptPackage`, `corpse`, `stream snapshot`, `active_package_form_id`, `object script locals`), and the forwarded sibling findings. Forwards honoured, none overlap: EXT-D6-01 / EXT-D6-02 / #6053 / EXT-D6-04 / EXT-D2-05, CONC-SD-01 / -02 / -03, PERF-SD-01 … -05, #5715, #5999. Closed issues checked and their fixes verified still in place: #5859 (`bleedout` field in `ReferenceState`, `capture`/`restore` arms, test `an_actor_evicted_mid_bleedout_still_bleeds_out_and_recovers` green), #5501, #5267, #4819, #6051 (`selectable_by_installer` shared by the schedule scan and the script slot, `ai_package.rs:92-99`, `1036-1040`).

## Executive Summary

| Severity | NEW | Regression | Existing (open) re-confirmed |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 4 | 0 | 0 |
| LOW | 0 | 0 | 2 (#5495 dormant gate, #5721) |

The recurring theme is the one the 10-15 audit named: **a per-placement fact is saved for resident entities, but the eviction path (`reference_state::capture` / `stream_snapshot`) never learned about it**, so leaving a tile or cell and coming back resets it. Three of the four findings are that class (`TwoStateActivator` + ObScript object-script locals; `ScriptPackage`; a corpse's position). The fourth (GAME-SD-04) is a different defect: the snapshot restore of `active_package_form_id` makes the first ambient tick conclude "no change", so the spawn-time behavior (chosen without alias overlays) is never replaced.

Games and cells reasoned about (statically): FNV (Goodsprings saloon ObScript objects, Mojave exterior radius streaming, quest `AddScriptPackage`), Skyrim (alias-injected follower/escort packages, `default2StateActivator` gates and levers, interior re-entry), Oblivion/FO3 (ObScript object scripts).

## Invariant Matrix

| Invariant | Status | Evidence |
|---|---|---|
| Single damage path | out of area | The area contains no damage producer. `restore` / `apply_starts_dead` write `Dead` only through `queue_dead_actor_reconciliation`. |
| Death reconciled once | **VERIFIED** (one drift in what survives) | `reference_state.rs:325-341` and `:391-397` queue the Late sink; `apply_starts_dead` is a no-op when `restore` already marked the actor dead; `AuthoredRagdollPose` is parked before the Late drain (`references/mod.rs:779-787`). The reconcile re-collapses the corpse at the authored placement (GAME-SD-03). |
| Index stability | **VERIFIED** | `restore` replaces the whole `Inventory` from rows captured in order, restores `EquippedWeapon` from the same capture, and removes it when the row has none (`reference_state.rs:278-305`). No row is removed. |
| Fail-closed consumables vs fail-open packages | **VERIFIED** | Unchanged. `selectable_by_installer` (#6051) is shared by the schedule scan and the script slot, so the two selection paths cannot diverge on a declined Dialogue package. |
| Stage order | **VERIFIED** | Update: `quest_alias_refresh_system` (`update.rs:442`) → `ambient_ai_package_system` (`:463`) → `forcegreet_system` → `scene_package_system` (`:496`). This order is what makes GAME-SD-04 reachable on the first frame after a respawn. |
| State saved or re-derived | **DRIFTED** | GAME-SD-01 (`TwoStateActivator`, `ScriptVariables` incl. ObScript object locals), GAME-SD-02 (`ScriptPackage`), GAME-SD-03 (corpse position). The `Bleedout` and `SpellList` rows are in `ReferenceState` and tested. |
| Determinism | **VERIFIED** | `rand::|Instant::now|SystemTime|thread_rng` over `npc_spawn*`, `reference_state.rs`, `stream_snapshot.rs`, `references/*.rs` hits only `references/mod.rs:918` and `resumable/mod.rs:184,198` (budget/telemetry timing). No `#[allow(dead_code)]` in the area's gameplay files. |

## Findings

### GAME-SD-01: Per-reference script state (`TwoStateActivator`, `ScriptVariables`, incl. the new ObScript object-script locals) is not captured at eviction, so every cell re-entry resets it
- **Severity**: MEDIUM
- **Dimension**: Persistence across the stream boundary (Dim 2 / Dim 7)
- **Location**: `byroredux/src/cell_loader/reference_state.rs:28-62` (the row has no field for either), `:119-264` (`capture`); respawn stamps `crates/scripting/src/translate/recognizers/two_state_activator.rs:34-52` and `crates/scripting/src/obscript_objects.rs:204-208`
- **Status**: NEW (the 2026-09-21 audit left a one-line `/audit-scripting` pointer, `AUDIT_GAMEPLAY_2026-09-21.md:697`, and no issue was filed; #2291 closed registering the columns for *resident* saves only)
- **Trigger**: any game; a recognized `default2StateActivator` (Skyrim levers, gates, puzzle doors) opened by the player, or any Oblivion/FO3/FNV `SCRI` object script that keeps a local (`short bDone`, a timer), then leave the cell (interior door, or the tile leaves the streaming radius) and come back.
- **Description**: `ReferenceState` carries inventory, equipment, weapon, actor values, spells, perks, control, bleedout, dead and picked-up. `TwoStateActivator` and `ScriptVariables` are saved components (`save_io/registry.rs:299-300`), but the registry only serialises entities that are *resident* at save time. Eviction despawns the entity (`unload.rs:631`), and the respawn runs the recognizer / `attach_object_script`, which stamp the authored `isOpen` and a fresh `ScriptVariables::default()`. Nothing in `capture`, `stream_snapshot` or a ledger (`ReferenceScriptState` holds only the `onlyOnce` trigger parked bit) carries them.
- **Evidence**: `two_state_activator.rs:34-42` builds `TwoStateActivator { is_open: <authored>, activated_once: false, .. }` on every attach. `obscript_objects.rs:204-208` is `if !world.has::<ScriptVariables>(entity) { world.insert(entity, ScriptVariables::default()); }`. `attach_script_for_refr` (`attach.rs:152-176`) then emits `OnCellLoadEvent`, which `obscript_objects.rs:263` turns into an `OnLoad` run on every attach. The module doc at `obscript_objects.rs:51-54` states the locals "survive a cell unload and a save without a second ledger"; only the "save" half is true. `docs/engine/stream-boundary-state-continuity.md:133-136` ("Script-local Papyrus VM state — canonical save/load already owns this boundary") has the same blind spot: the save registry owns resident entities, not evicted ones.
- **Impact**: An opened gate/lever/puzzle door closes again after the player leaves and returns; `do_once` / `activated_once` gating re-arms. For ObScript object scripts, which are default-on this week (`BYRO_NO_OBSCRIPT_OBJECTS` is the only switch), a script that guards an `OnLoad` side effect with a local flag runs that effect again on each re-entry while the reference's *inventory* correctly persists (it is in `ReferenceState`), so the effect can duplicate items. The counts of vanilla scripts that depend on this were not measured (no game data read).
- **Related**: #2291 (closed, resident-save columns), #4334 / `ReferenceScriptState` (the existing per-reference ledger shape), #6017 (provider `OnInit` once-only flag, same class), GAME-SD-02.
- **Suggested Fix**: Add a `script_state: Option<(TwoStateActivator, ScriptVariables)>` pair to `ReferenceState` (a `FORMAT_MAJOR` bump; no serde defaults, #4465) captured for any victim carrying either, consumed by `restore` *after* the recognizer/`attach_object_script` stamps (so the authored defaults are overwritten). Correct the two doc claims. Pin with a test that evicts an opened `TwoStateActivator` and a scripted local, respawns, and asserts both survive.

### GAME-SD-02: An ObScript `AddScriptPackage` slot (`ScriptPackage`) is not in the eviction row, so a streamed-out actor drops back to its schedule
- **Severity**: MEDIUM
- **Dimension**: Persistence across the stream boundary / AI packages (Dim 5, Dim 7)
- **Location**: `byroredux/src/cell_loader/reference_state.rs:28-62`, `:240-263`; component `crates/scripting/src/script_packages.rs:45-60`; registered only as a resident column at `byroredux/src/save_io/registry.rs:384`
- **Status**: NEW (from `af5db229d`, FORMAT_MAJOR 110). The #6051 follow-up (`a5343c806`) touched selection, not persistence.
- **Trigger**: FNV/FO3/Oblivion quest script runs `actor.AddScriptPackage <pack>` (a Must-reach / Must-complete Travel is the typical shape); the player moves far enough that the actor's tile (exterior) or cell (interior) unloads; the player returns.
- **Description**: The GECK contract encoded in the module doc is that a constrained script package "stays until removed, replaced, evaluated away, or completed". `ScriptPackage` lives on the actor entity only. `reference_state::capture` has no field for it and `stream_snapshot` carries only `AmbientPackageRuntime.active_package_form_id` (a FormID), which `restore_actor_snapshot` writes back verbatim while the `ScriptPackage` slot that justified it is gone. On the first tick `script_package_of` is `None`, `select_active_package` returns the schedule winner, the id differs from the restored one, and the package handover runs: the actor silently reverts to its own stack and the package's completion (`Traveled` → `finish_script_package`) never happens.
- **Evidence**: `grep -rn ScriptPackage byroredux/src` hits only `ai_package.rs`, `save_io/registry.rs` and its tests; `reference_state.rs` and `stream_snapshot.rs` do not mention it. A save taken while the actor is *evicted* also loses it for the same reason (the actor is not a resident row).
- **Impact**: A quest NPC sent somewhere by a script resumes his routine as soon as the player is out of radius; a "must complete" errand a quest waits on cannot finish. Self-heals to a plausible state (the schedule), so it is not loud.
- **Related**: GAME-SD-01 (same fix shape), GAME-SD-04 (the snapshot's package id also masks the handover in the alias case), #5859 (the precedent: `Bleedout` added to the row).
- **Suggested Fix**: Add `script_package: Option<ScriptPackage>` to `ReferenceState` (bump `FORMAT_MAJOR` with GAME-SD-01), restore it before the actor's first ambient tick, and test evict → respawn → `ambient_ai_package_system` keeps the scripted package.

### GAME-SD-03: A corpse's position is never carried across eviction; it comes back at its authored placement and collapses again
- **Severity**: MEDIUM
- **Dimension**: Death reconciliation across the stream boundary (Dim 4 producers in area, Dim 7)
- **Location**: `byroredux/src/cell_loader/stream_snapshot.rs:96-103` (`has_package_state`), `:156-259`; `byroredux/src/combat.rs:1075-1076` (`reconcile_dead_actor` removes `AmbientPackageRuntime`); `reference_state.rs:325-341`
- **Status**: NEW (the continuity doc's keep-list, `stream-boundary-state-continuity.md:115-118`, includes "live position/orientation if diverged"; the implementation gates that on package state)
- **Trigger**: the player kills an NPC away from its authored spot (an NPC that walked to a Travel/Wander destination, or was chased), then streams the tile out (leaves the radius, or takes an exterior→interior door, which also calls `drain_streaming_state`) and returns.
- **Description**: `capture_actor_snapshots` keeps a row only `if snapshot.has_package_state()` (`active_package_form_id`, travel, traveled or seat). `reconcile_dead_actor` strips `AmbientPackageRuntime` and the ambient behaviors, so a corpse has none of them and produces no row. The `ReferenceState` row records `dead: true` but no position. The respawn builds the actor at the REFR placement, `restore` marks it `Dead`, and the Late sink runs `activate_ragdoll` from the spawn pose. The ragdoll never moves the actor root (`ragdoll.rs:571` "the entity's own `GlobalTransform` never moves during ragdoll"), so the pre-eviction root `Transform` *is* the death position that would have been captured.
- **Evidence**: no `Dead` / `corpse` mention anywhere in `stream_snapshot.rs`; its tests only exercise Travel and Seated. `StreamStateSnapshots` is also cleared wholesale by `drain_streaming_state` (`streaming_helpers.rs:602-606`) and is not saved (`registry_completeness_tests.rs:492`), so even a fix inside the snapshot would not survive a door transition or a save; the parked `ReferenceState` row is the store that does.
- **Impact**: Corpses teleport back to where the actor was authored and visibly die a second time on tile re-entry; a body the player lost track of in the wasteland is no longer where it fell. Loot is unaffected (the inventory row persists). Authored "Starts Dead" corpses are unaffected (their XRGD pose is re-applied).
- **Related**: #5267 (the queued Late reconcile), #5015 (XRGD decode for Skyrim+/FO4, open).
- **Suggested Fix**: Add the root `Transform` (translation + rotation) to the `dead` `ReferenceState` row (alongside GAME-SD-01/02's version bump) and apply it in `restore` before queueing the reconcile, only when it diverges from the authored placement by `POSITION_DIVERGENCE_EPSILON`. A test evicts a killed, displaced actor and asserts the respawned corpse's root position.

### GAME-SD-04: Restoring the snapshot's `active_package_form_id` makes the first ambient tick see "no change", so an alias-injected package's behavior is never installed after a respawn
- **Severity**: MEDIUM
- **Dimension**: NPC spawn → AI package selection (Dim 5)
- **Location**: `byroredux/src/cell_loader/stream_snapshot.rs:372-380` (the overwrite), `byroredux/src/npc_spawn/ai_package.rs:834-873` (spawn-time winner), `:1004-1021`, `:1071-1072` (`if !changed { … continue; }`)
- **Status**: NEW
- **Trigger**: Skyrim (or any game with quest aliases injecting `ALPC` packages): an actor whose running package came from a quest alias overlay (a follower, an escort, one of the 44 forced-ref aliases #4704 mentions) leaves a tile/cell and returns.
- **Description**: At respawn `apply_ai_package_behavior` selects over the NPC's own `PKID` stack only, because the alias overlays are stamped after spawn (its own comment says so, `ai_package.rs:861-864`); `behavior.insert_at_spawn` installs that winner W0 (or nothing, for an alias-only actor). `stamp_quest_reference` then runs `restore_actor_snapshot`, which sets `runtime.active_package_form_id = Some(P_alias)` unconditionally. On the next Update `quest_alias_refresh_system` (`update.rs:442`) stamps `QuestAliasInjectedOverlays` and queues an `EvaluatePackageRequest`; `ambient_ai_package_system` (`:463`) then selects `P_alias`, compares it to the restored id, finds `changed == false` and `continue`s without `clear_ambient_behavior` / `insert_at_runtime` (`:1063-1072`). `report_package_transition` also sees `previous == active`, so no PF_ fragment fires either.
- **Evidence**: `restore_actor_snapshot` sets only the id (plus `TravelState` / `Traveled` markers); nothing re-derives the behavior from it. The id is written by the same code path that, on a fresh spawn, deliberately starts at W0 so the first tick *does* see a change. The stream_snapshot tests never build an `AmbientPackageRuntime`, so the interaction is unpinned.
- **Impact**: The actor stands with W0's behavior (typically Sandbox, or idle for an alias-only actor) while `AmbientPackageRuntime` says its alias package is running, until the next package change (a schedule boundary or a re-evaluation that changes the winner). A follower or escort stops following after a stream-out/in.
- **Related**: #4704 (alias-only actors need the runtime), #3299 (the snapshot), GAME-SD-02 (the `ScriptPackage` case is the benign mirror: the id is restored but the slot is gone, so the tick reverts to the schedule).
- **Suggested Fix**: Do not restore `active_package_form_id` as the "already installed" id. Either restore it only when the spawn-time winner equals it, or leave the runtime's id at W0 so the first tick reselects and installs the right behavior (the snapshot's `TravelState` / `Traveled` markers are re-applied after `insert_at_runtime`, so Travel progress still resumes). Test: an actor with an alias overlay is captured, respawned, and one `ambient_ai_package_system` pass installs the overlay package's behavior.

## Non-finding notes (verified, nothing to file)

- **#5408 head-part fallback** (`2a8663f39`): `prebaked_head_fallback_paths` merges race base parts for the NPC's gender with the NPC's own parts (per-type replace, HNAM extras depth-first with a visited set). The cycle guard and the dangling-reference skip hold; the Misc-type replace rule is documented as read from the CK, not engine code. The helper runs for every prebaked spawn (a small `Vec`/`HashSet` per NPC) but only its result is used on the facegeom miss; not worth a finding.
- **#6051**: confirmed fixed. `a_declined_dialogue_script_package_leaves_the_actor_on_its_schedule` pins it.
- **Conversation partner despawned mid-dialogue**: `npc_refuses_dialogue` tests components only, so a partner despawned by a script teleport would leave `DialogueSurfaceState.npc` dangling. Reaching it needs the player to leave the partner's cell while the modal is open (a dialogue fragment that moves the player); not constructed statically, dropped as speculative.
- **Non-primary SCOL/PKIN children** that resolve to a `CONT` get `attach_container_inventory` (`synth_child.rs:918`) but no `FormIdComponent` (`stamp_quest_reference` is primary-only), so `capture` skips them and `restore` cannot find a row. Whether any vanilla PKIN/SCOL content references a container was not measurable without game data; not filed.

## Known-Open Register (cite, don't re-file)

- #5495 (creatures run the humanoid seating path): re-grepped at HEAD, `CreatureActor` is still inserted only by the sandbox test (`systems/sandbox.rs:547`); the gate in `sandbox.rs:311-313` and `eat_sleep.rs:523` stays dormant.
- #5721 (the #5031 held-out chain excludes only the latest failure): cited from the snapshot, not re-measured.
- #4232 (`effective_actor_level` returns 0), #5015 (Skyrim+/FO4 XRGD corpse poses), #5985 (`GetVMScriptVariable` cannot see VM reference-script variables), #6061 (`dt2-skyrim-forcegreet` open leg): open per `/tmp/audit/issues.json`; not re-verified, out of this area's data path.
- Known-open from the skill, still true by code read: no pickpocket system; seven PACK procedures have no runtime; `SetInChargen` save refusal belongs to `/audit-save`.

## Cleanup

Scratch lives only under `/tmp/audit/streaming-deep/gameplay/` (`baseline_tests.txt`, `dim_notes.md`); the suite removes it. No code, tests or issues were touched.

Suggested next step: `/audit-publish docs/audits/AUDIT_GAMEPLAY_2026-10-19.md` (labels `gameplay` + `save-load` for SD-01/02/03, `gameplay` + `ai` for SD-04; add `scripting` to SD-01/02; no `game:*` label, none is title-specific).
