# Save/Load Audit — 2026-11-23

**HEAD**: b65be1e1e (clean `origin/main` worktree) · **Baseline**: AUDIT_SAVE_2026-11-17.md (audited 4079a9d33) · **Audited**: Dims 1-5, delta-first (547 commits since the baseline; 53 touch `crates/save` or `byroredux/src/save_io`) · **Scope highlights**: `FORMAT_MAJOR` 133 -> 143 (v134-v143), the `SessionProfile` column (LMC-SAV-001), the load-drain additions (#7227-#7230, #7252, #7253, #7305, #7338, #7341, #7342), the `atomic_file` temp-file work (#7143/#7158/#7172/#7182).

Read-only run: nothing fixed, nothing filed, no engine launched. Three dimension agents (1+2, 3+4, 5), every MEDIUM premise re-verified against the code by the orchestrator. Guard ledger at audit time: `byroredux-save` 41 unit + 15 round-trip, and `-p byroredux --bin byroredux` over `save_io`, `registry_completeness`, `serde_default_guard`, `live_reload`, `scheduler_access`, `boot::schedule`, `app_step`, `profile_fingerprint`: **205 passed, 0 failed, 4 ignored** (the documented real-master tests). All green, so every finding below is a gap the guards do not cover.

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 4 |
| LOW | 5 |
| INFO | 1 |
| **Total** | **10** |

Nothing loses inventory, loot, actor values or a save slot. The subsystem's core promises hold: the atomic write order, the header gate order, the typed preflight before teardown, `restore_world` unreachable from the load drain, no `#[serde(default)]` on a save type, a contiguous bump ledger at v143, no drift between the registry and `MUTABLE_DELTA_COLUMNS`, and every fix of the 2026-11-17 audit (SAVE-1 to SAVE-9) still in place.

- **SAVE-01 (MEDIUM, silent-drop) is a regression of #7342, found in this session's own fix.** A scripted exterior-ring door loses its saved `activated_once` and takes its authored `isOpen` over the saved one, because `restore_parked_door_state` consumes the row before the door script's recognizer re-stamps the whole component.
- **SAVE-02 (MEDIUM, found by two agents independently):** a *successful* load whose saved profile differs from the session's tells the player "Load failed.". `notify_player` has ignored its text since #6825 and the LMC-SAV-001 summary was routed through it.
- **SAVE-03 (MEDIUM, silent-drop):** the `Background` allowlist reason is still false for the class half: `SetClass` writes the player's class and nothing saves or re-derives it.
- **SAVE-04 (MEDIUM, no data loss):** the load's profile diff compares the save's profile with one the reload just rebuilt from the save's own plugin list, so a changed plugin set is never reported.
- **LOW:** two Once Per Day load-path residues (SAVE-05, SAVE-06, both on #7341), a wrong baseline comment (SAVE-07), a quicksave deferral dropped by a same-batch quit (SAVE-08), and stale staging temps after a crash (SAVE-09). **INFO:** two `atomic_write` callers have no clobber-fallback pin (SAVE-10).

## Design claims (`crates/save/src/lib.rs`, `snapshot.rs`)

**Design claims (per dimension)** (crates/save/src/lib.rs docstring, the ones Dim 1/2 cover)
- Full-snapshot, not delta log; one JSON column per component type + one blob per saved resource: CODE-CONFIRMED (`save_world`).
- `StringPool` dumped in symbol order and restored before columns: CODE-CONFIRMED (`restore_world` steps 3-4).
- Registry = pair of type-erased closures per type, populated by the binary: CODE-CONFIRMED; the portable-pair codecs and `SessionProfile` fit the same shape.
- Schema/format discipline ("a type added/removed/renamed is refused, not silently dropped"): CODE-CONFIRMED (fingerprint hashes names, order and replace policy; bump ledger contiguous and current at v143).
- "Save size scales with loaded-cell entity count, never with playthrough length": CODE-CONFIRMED for the new columns (`SessionProfile` scales with plugin count, `MenuEventRegistrations` with registrations, `SkillUseLevel.pending` with unspent levels); not re-audited for older ledgers.
- Dim 3/4 claims (CRC, atomic write, `validate` pass): out of scope for this agent.

**Design claims (per dimension)**

`crates/save/src/lib.rs` / `snapshot.rs`: CODE-CONFIRMED - versioned container with CRC over the payload only and header-first gates; the layout table matches `encode`/`decode`; atomic write "tmp -> fsync -> re-read+verify -> rename" (the parent-directory fsync is also done, undocumented in the crate docstring); ring never clobbers the last good save; validation precedes any write; load runs off-frame via the post-scheduler drain; the bump ledger is complete through v143. DRIFTED (minor) - "save size scales with loaded-cell entity count, never with playthrough length" overclaims: FormID-keyed ledgers and parked reference rows (`PersistentReferenceStates`, `ReferenceEnableState`, `ReferenceLockState`, quest/ledger resources) grow with touched references; and "Save needs only `&World` and fits as a Late-stage exclusive system" is contradicted by `save_now`'s own ABBA note (production runs only post-scheduler or in `DebugDrainSystem`). DRIFTED - `profile_fingerprint.rs` "a load never changes the profile" (SAVE-D4-01).

**Design claims (per dimension)**

`lib.rs` "Load runs off-frame, `&mut World` drained between frames": CODE-CONFIRMED (`step_save_loads` → `execute_pending_save_loads`, after the post-scheduler save drain). The same docstring's "load clears + repopulates storages" describes `restore_world` only (test / loose); the live path is reload + `restore_resources` + `apply_deltas` and never reaches `restore_world` (DRIFTED wording, informational). "Full snapshot, ECS as truth": CONFIRMED for resident state; non-resident state rides the `PersistentReferenceStates` resource, and by decision DRT-STATE-003 an evicted door reverts to its default (#7305), a documented exception, not drift.

## Data-Loss Class Matrix

| Class | Findings |
|---|---|
| silent-drop | SAVE-01 (a scripted ring door's `activated_once` and saved open state), SAVE-03 (`Background.class_form_id` after `SetClass`), SAVE-08 (a quicksave queued in the same batch as a quit) |
| corruption-on-load | none |
| irrecoverable-write | none |
| reference-break | none |
| none (UX, diagnostics, test gaps, comment drift) | SAVE-02, SAVE-04, SAVE-05, SAVE-06, SAVE-07, SAVE-09, SAVE-10 |

## Completeness Ledger

Registry x `MUTABLE_DELTA_COLUMNS`: **no drift.** All 39 delta columns are registered; the columns added since 2026-11-17 are in both lists (`SkillUseLevel`, `ReadiedSpell`, `EquippedAmmo` (replacing since #7228), `SpeakingLine`, the actor-value codec columns). `SessionProfile` is a registered resource codec only (a record of the profile the save was made with, never imported). The one completeness gap is a *reason*, not a row: SAVE-03.

## Findings

### SAVE-01: #7342 consumes a ring door's saved TwoStateActivator before the door script's recognizer, which re-stamps the whole component
- **Severity**: MEDIUM
- **Dimension**: Live Load-Apply
- **Data-Loss Class**: silent-drop
- **Location**: `byroredux/src/systems/door.rs:193-225` (`attach_door` → `restore_parked_door_state`), `byroredux/src/cell_loader/reference_state.rs:596-625`, vs `crates/scripting/src/translate/recognizers/two_state_activator.rs:36-48`; call order `cell_loader/references/synth_child.rs:920` (`attach_door`) before `:983` (`attach_script_for_refr`)
- **Status**: NEW (regression of the scripted half of #7342 / GP-06 relative to the pre-fix path; related #6071)
- **Description**: A save's unresolved references (the exterior hysteresis ring) are parked with their saved `TwoStateActivator` in `scripts[pair].two_state`. Before #7342 a scripted door got it back in `restore_script_state`, which runs after the script attach and overwrites the recognizer's stamp. #7342 moved the consumption into `attach_door`, which runs BEFORE `attach_script_for_refr`. For a door carrying `default2StateActivator` the recognizer then replaces the component wholesale:
  ```rust
  // two_state_activator.rs:39-43
  let is_open = authored_open.unwrap_or_else(|| world.get::<TwoStateActivator>(entity).map_or(default_open, |seeded| seeded.is_open));
  let state = TwoStateActivator { is_open, is_animating, do_once, activated_once: false };
  world.insert(entity, state);
  ```
  So the restored `activated_once` is always reset to `false`, `is_animating` and `do_once` come from the script defaults, and `is_open` comes from the placement's VMAD `isOpen` when one is authored (`authored_open`), not from the save. `restore_parked_door_state` already took `two_state` out of the row (`state.two_state.take()`), so `restore_script_state` later has nothing to put back (it restores only `variables`, mirrored from the already-overwritten component). The test `a_saved_door_state_parked_for_the_ring_is_put_on_the_door_at_attach` runs `scripted: true` but only through `attach_door`; the recognizer never runs in it.
- **Trigger**: exterior save, a Skyrim+/FO4 door with `default2StateActivator` in the ring (`radius_load + 1`, not respawned by the reload), already used. The streamed-in door has `activated_once = false`: a `doOnce` door can be used again (`two_state_activator_system` skips only `do_once && activated_once`, `vm_state.rs:170`); a door whose VMAD authors `isOpen` returns to the authored state instead of the saved one. A resident door is unaffected (the `TwoStateActivator` column overlays after the reload).
- **Impact**: narrow, but it is saved gameplay state silently replaced by authored values, which DRT-STATE-003 / DRT-SAVE-001 say must be kept ("a cell restored from a save MUST keep the saved state"). The fix for the unscripted door left the scripted one worse than before it.
- **Related**: #7342 (closed), #7338, #7305, #6071. Searched "door parked state scripted TwoStateActivator recognizer" and "activated_once door save": only the closed issues above.
- **Suggested Fix**: in `restore_parked_door_state` leave `two_state` in the row when the row also holds `variables` / `timer` / `rumble` (a script will attach and `restore_script_state` puts it back after the recognizer, then re-derives the mirror), and consume it only for a row that holds nothing else. Extend the test to run the recognizer between `attach_door` and `restore_script_state`.
- **Dimension agent ID**: SAVE-D5-02

### SAVE-02: A successful load whose profile differs from the save's tells the player "Load failed."
- **Severity**: MEDIUM
- **Dimension**: Live Load-Apply
- **Data-Loss Class**: none (misleading failure notice on a load that succeeded; no state lost)
- **Location**: `byroredux/src/save_io/live_load.rs:993-1001` (the call), `:113-120` (`notify_player`), `:123` (`LOAD_FAILED`)
- **Status**: NEW (introduced by d3b208048, Refs #6670, which reused a helper that f452ec0e4 / #6825 had made failure-only)
- **Description**: `notify_player` was changed by #6825 to ignore its text and push the fixed line `LOAD_FAILED`. d3b208048 then calls it from the profile-difference report on the success path, to "raise a player notification" naming the changed plugins / mod roots / grants. The text is dropped; the player sees "Load failed." for a load that completed (the drain goes on to `end_sting_layer`, `sync_body_after_load`, `apply_player_pose`).
- **Evidence**:
  ```rust
  // live_load.rs:117-120
  fn notify_player(world: &World, diagnostic: impl Into<String>) {
      let _logged_by_the_caller = diagnostic.into();
      crate::notifications::push(world, LOAD_FAILED);          // "Load failed."
  }
  // live_load.rs:995-1001
  let profile_lines = crate::profile_fingerprint::save_profile_lines(world, &snapshot);
  ...
  if let Some(summary) = profile_lines.first() {
      notify_player(world, format!("{summary} (see the log or save.info)"));   // success path
  }
  ```
  `profile_fingerprint::tests::a_save_carries_its_profile_and_a_load_keeps_the_sessions` covers the lines, not the notification; nothing pins what the player sees.
- **Impact**: any load of a save made with a different plugin set, load order, mod roots, extensions or grants (the case LMC exists for, and the common one for a modded install) shows "Load failed." over a working game. A player who believes it failed may reload an older slot or overwrite the loaded one. No data is lost by the code itself.
- **Related**: #6670 (LMC Phase 1), #6825. Open issues searched (`gh issue list --state all`: "profile notification load failed", "SessionProfile load notify"): none.
- **Suggested Fix**: push the summary through `crate::notifications::push(world, ...)` directly (engine text, add it to the native-page string inventory the `LOAD_FAILED` doc names), or drop the notification and keep the WARN + `save.info`. Add a test that a mismatching profile pushes no `LOAD_FAILED`.
- **Dimension agent ID**: SAVE-D5-01
- **Also found by**: SAVE-D2-01 (Dim 2), independently.

### SAVE-03: the `Background` NOT_SAVED reason is still false for `class_form_id`: `SetClass` writes the player's class and no save keeps it
- **Severity**: MEDIUM
- **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:487` (`Background`); writer `crates/scripting/src/obscript_more/tail.rs:474-483` (`cmd_10f0`, registered `tail.rs:764` for Oblivion); reader `crates/scripting/src/oblivion_skill_use.rs:107-116` (`player_class`); partial fix `byroredux/src/systems/character.rs:131-153` (`player_identity_sync_system`).
- **Status**: NEW
- **Dimension**: Snapshot Completeness
- **Data-Loss Class**: silent-drop
- **Where**: `byroredux/src/save_io/registry_completeness_tests.rs:487` (`Background`); writer `crates/scripting/src/obscript_more/tail.rs:474-483` (`cmd_10f0`, registered `tail.rs:764` for Oblivion); reader `crates/scripting/src/oblivion_skill_use.rs:107-116` (`player_class`); partial fix `byroredux/src/systems/character.rs:131-153` (`player_identity_sync_system`).
- **Premise (quoted)**: reason = "derived character-creation metadata: re-created with the actor, and on the player rewritten from the saved CharacterIdentity by player_identity_sync_system ... so the saved column is the source of truth". `CharacterIdentity` is `{ name, female, race_form_id }` (`crates/core/src/character/components.rs:343-349`) — no class. `player_identity_sync_system` writes only `race_form_id` (and inserts `class_form_id: 0` when no Background exists). `cmd_10f0` does `background.class_form_id = class` on `caller_or_player` (GECK/CS *SetClass*: "Changes the characters class of the actor"). `player_class()` reads `Background.class_form_id` first and falls back to the NPC record's class; that class's seven majors decide which rank-ups count toward a level (`SkillUseLevel`, saved).
- **Problem**: the 2026-11-17 SAVE-9 fix replaced "no live production mutator" with a reason that is true for race and still false for class. After a cold-start load the player's `Background` is re-stamped from the `NPC_` record, so a script-set class (a mod's `player.SetClass`, the OSL-CLS-003 case the skill-use docstring promises) reverts and the saved `SkillUseLevel` tallies/pending levels are then judged against the wrong majors. After an in-process live load the process-lifetime player keeps whatever class the outgoing timeline set (cross-timeline leak, the inverse failure). Same hole for any actor whose class a script changed (NPC `Background` is not parked either). Vanilla Oblivion scripts have zero `SetClass` calls (cs-uesp-wiki "List of function appearances in scripts"), hence MEDIUM, not HIGH.
- **Fix sketch**: carry the player's class in the saved identity (`CharacterIdentity.class_form_id`, a v144 bump, restored by `player_identity_sync_system`) or register `Background` as a replacing column; then restate the reason.
- **Dedup**: no open or closed issue (`gh issue list --state all --search "SetClass Background class save"` empty; not in issues.json). Regression note on SAVE-9 (fixed 110790dce, partial).
- **Dimension agent ID**: SAVE-D1-01

### SAVE-04: The load's profile diff is computed against a profile the reload just rebuilt from the save's own plugin list, so plugin additions, removals and reorders can never be reported
- **Severity**: MEDIUM
- **Dimension**: Save-Side Gates (profile capture/compare, d3b208048 / #6670)
- **Data-Loss Class**: none (diagnostic only; no saved state is lost by this)
- **Location**: `byroredux/src/cell_loader/load_order.rs:381` (`install_form_resolver` -> `world.insert_resource(session_profile(&load_order))`), reached from `byroredux/src/cell_loader/load.rs:590` (`InteriorCellApplyJob::begin` -> `PreviousFormResolver::install`) with `plugin_paths = masters + esm_path` taken from the save's `CurrentCellContext` (`byroredux/src/save_io/live_load.rs:163,194`); compared at `live_load.rs:995` (`save_profile_lines`); claim at `byroredux/src/profile_fingerprint.rs:10-12` and `save_io/registry.rs:410-411`.
- **Status**: NEW (not covered by #6670 / #6676: #6676 is the LMC-SAV-002..004 *consequences*, not the report that triggers them)
- **Description**: `profile_fingerprint.rs` promises "loading a save compares the record the save carries with the session's and reports every difference ... while the session keeps its own: a load never changes the profile". But the live load (and a `--load` at startup) does not keep the session's profile: it reloads the cell from the *save's* plugin set (`cell_ctx.masters` + `esm_path`), and every load-order install goes through `install_form_resolver`, which overwrites `SessionProfile` with `session_profile(&load_order)` built from that list. The post-reload comparison therefore diffs the saved record against a record whose `plugins` are, by construction, the save's own plugins. `differences()` lines `plugin X: in the save's profile, absent now`, `plugin X: added since the save` and `load order changed` cannot fire; only a plugin's content-hash drift, mod roots, extensions, grants and record policy (which `session_profile` still reads from the current process) can.
- **Evidence**:
  ```rust
  // load_order.rs:377-382
  world.insert_resource(resolver);
  world.insert_resource(crate::profile_fingerprint::session_profile(&load_order));
  // load.rs:565-590: plugin_paths = masters (the save's) + esm_path
  // live_load.rs:995: let profile_lines = save_profile_lines(world, &snapshot);   // AFTER the reload
  ```
  The only test (`a_save_carries_its_profile_and_a_load_keeps_the_sessions`) calls `restore_resources` on a hand-built world; no test drives a reload with a different plugin set, which is why the masking is unpinned.
- **Impact**: A player who adds a mod and loads an older save gets the save's plugin set silently (the new mod is not in the reloaded session) and no "added since the save" notification; a removed plugin fails the load earlier (parse error) rather than through the profile report. The next save then records the save's plugin set as the session profile. LMC-SAV-002..004 will hang their load reports off this diff, so it has to see the pre-load profile.
- **Related**: #6670 (Phase 1), #6676 (Phase 7 consequences), `docs/prd/legacy-mod-compatibility.md` LMC-SAV-001.
- **Suggested Fix**: capture the session's `SessionProfile` before `reload_*_session` (the drain already captures `LiveReloadLedgers`/`identity_before`) and diff the saved record against that; restore it after the reload if "the session keeps its own" is the intended contract. Add a live-reload test with a differing plugin list.
- **Dimension agent ID**: SAVE-D4-01

### SAVE-05: #7341 restarts every block the reload noted, including those of packages the re-seat then discards
- **Severity**: LOW
- **Dimension**: Live Load-Apply
- **Data-Loss Class**: none (ambient AI timing; nothing saved is lost)
- **Location**: `byroredux/src/npc_spawn/package_flags.rs:118-125` (`restart_day_blocks_at_loaded_clock`), `byroredux/src/save_io/live_load.rs:843`, `byroredux/src/npc_spawn/ai_package.rs:1232-1235` (`note_selected` at spawn)
- **Status**: NEW (refinement of the closed #7341 fix)
- **Description**: The reload's spawns note a block for the Once Per Day package they pick under the OUTGOING clock (`apply_ai_package_behavior`, `ai_package.rs:1213-1235`). `restart_day_blocks_at_loaded_clock` then sets EVERY held block to the loaded clock. `reseat_ambient_packages_after_restore` runs at the loaded clock afterwards; when the loaded hour is outside that package's window it installs another winner and the noted package stays blocked, now for a full 24 hours from the load (`once_per_day_blocked` exempts only `current == Some(package)`). Example: the reload spawns an actor at 09:00 under its Once Per Day breakfast package (block noted), the save is 20:00; the re-seat installs the evening package and the breakfast block now runs until 20:00 the next day, so the actor skips tomorrow's breakfast. The saved session had run breakfast that morning (blocked until about 08:05 next day) and a cold start from the same save would have the spawn-time block lapse as the clock jumps ahead. Every other check in the claim holds: at the restart moment all held entries are the reload's own (`reload_with_fresh_day_blocks` took the outgoing map, no other writer runs between the reload and line 843, and no path in the drain before it can leave the blocks under a different clock; an aborted or failed load before line 836 keeps the outgoing clock and the reload's blocks together).
- **Impact**: one skipped daily package per affected actor after a load, an ambient behavior difference only.
- **Related**: #7252, #7340, #7341.
- **Suggested Fix**: keep only the blocks of the package each actor is running after the re-seat (drop the rest), or restart a block only when its package is still the actor's `active_package_form_id` after the re-seat.
- **Dimension agent ID**: SAVE-D5-03

### SAVE-06: nothing pins `restart_day_blocks_at_loaded_clock` in the drain
- **Severity**: LOW
- **Dimension**: Live Load-Apply
- **Data-Loss Class**: none
- **Location**: `byroredux/src/save_io/live_reload_tests.rs` (`the_day_blocks_restart_before_the_reload_and_survive_an_aborted_one`, `post_reload_drain_steps_keep_their_order`), `byroredux/src/save_io/live_load.rs:843`
- **Status**: NEW (test-gap)
- **Description**: `restart_day_blocks_at_loaded_clock` has a unit test on the function (`package_flags.rs:466-480`) but no test that the drain calls it, after the wholesale `restore_resources` (it reads `GameTimeRes`, restored there) and before `reseat_ambient_packages_after_restore`. Deleting line 843, or moving it above line 836, leaves the whole save_io suite green (205 passed, 4 ignored real-master). `post_reload_drain_steps_keep_their_order` does not list it.
- **Impact**: the #7341 regression (blocks dated in the future after loading an earlier save) can return silently.
- **Suggested Fix**: add `"package_flags::restart_day_blocks_at_loaded_clock(world)"` to the `steps` array of `post_reload_drain_steps_keep_their_order`, between `restore_resources` and `restore_resident`.

---
- **Dimension agent ID**: SAVE-D5-04

### SAVE-07: the 449ea0fed baseline refresh absorbed an undocumented new `Option` field in a saved type (`VmResume.answer`) with no bump
- **Severity**: LOW
- **Location**: `byroredux/src/save_io/serde_default_guard_tests.rs:1349-1353` (justification); `crates/scripting/src/translate/effects/vm_bridge.rs:14-24` (`VmResume`) and `:50-56` (`VmStep::Replay`), both from b35f2881e.
- **Status**: NEW
- **Dimension**: Format & Schema Discipline
- **Data-Loss Class**: none (v135 saves are refused by the major since v136; no live exposure)
- **Where**: `byroredux/src/save_io/serde_default_guard_tests.rs:1349-1353` (justification); `crates/scripting/src/translate/effects/vm_bridge.rs:14-24` (`VmResume`) and `:50-56` (`VmStep::Replay`), both from b35f2881e.
- **Premise (quoted)**: justification: "`FragmentResumeCondition::MessageAnswered { ticket }` was appended ... no existing variant or field moved". The same refresh also swallowed `pub answer: Option<i32>` on `VmResume` (inside saved `Effect::Interpret { resume }` continuations) and the `VmStep::Replay(u32)` variant. The skill's rule (and `serde_default_guard_tests`' own policy comments): a new `Option` in a saved type bumps, because a missing `Option` field default-fills on decode, which is the `#[serde(default)]` mechanism the blanket rule forbids (#1714).
- **Problem**: the enum variants are name-tagged and read-compatible, so the refresh without a bump is defensible for them. The `answer` field is not covered by that argument: a pre-b35f2881e v135 save decodes only because serde fills a missing `Option` with `None`. Harmless here (a missing answer is "no answer yet"), but the baseline comment misdescribes what moved, and the next such field may not default to a safe value.
- **Fix sketch**: amend the comment at the refresh (or the skill's triage list) to name `VmResume.answer` / `VmStep::Replay`; no code change needed.
- **Dedup**: none.
- **Dimension agent ID**: SAVE-D2-02

### SAVE-08: A quicksave deferred by the SAVE-4 wait is dropped silently when `engine.quit` / a script `QuitGame` is in the same batch
- **Severity**: LOW
- **Dimension**: Save-Side Gates (quiescent deferral, `MAX_SAVE_DEFERRALS`)
- **Data-Loss Class**: silent-drop (an explicitly requested save is not written; nothing is reported)
- **Location**: `byroredux/src/save_io/commands.rs:618-628` (deferral), `byroredux/src/app_events.rs:112-122` (`GracefulExitRequested` docs: the queued save "execute[s] in that frame's post-scheduler step before the teardown begins"), `app_events.rs:1534-1542` (top of `about_to_wait` runs `shutdown` before any drain), `shutdown` at `app_events.rs:138` (no `PendingPlayerSaveActions` flush).
- **Status**: NEW
- **Description**: SAVE-4 (`d72e1f45e`) made the player-action drain return an empty batch (leaving the `Quicksave` queued) for up to `MAX_SAVE_DEFERRALS` = 3 ticks while `PendingRelocations` / `PendingActorMoves` are non-empty. `step_actor_relocations` runs inside `step_cell_transition`, *after* `step_player_save_actions` in `about_to_wait` (`app_events.rs:1719` vs `1730`), so any script move raised during the scheduler run makes the drain defer. The `engine.quit` contract (one-frame-later polling so `input.press quicksave` + `engine.quit` in one batch still saves) was written before the deferral; the next frame's `about_to_wait` now calls `shutdown` at its head, before the queued save can run, and `shutdown` does not drain the queue.
- **Evidence**:
  ```rust
  // commands.rs:623
  if moves_pending && pending.0.contains(&PlayerSaveAction::Quicksave) && pending.1 < MAX_SAVE_DEFERRALS {
      pending.1 += 1;  return Vec::new();
  }
  // app_events.rs:1538  (before step_player_save_actions)
  if self.world.try_resource::<GracefulExitRequested>().is_some_and(|r| r.0) { self.shutdown(event_loop); return; }
  ```
- **Impact**: Needs a same-tick actor move plus a same-batch quit; the window is one tick, so rare, but the result is a save the operator/automation asked for that never lands and no message. Smoke/automation harnesses that quicksave then quit are the exposed users.
- **Related**: SAVE-4 / SAVE-7 (fixed 11-17); the 11-17 report did not examine the quit ordering.
- **Suggested Fix**: have `shutdown` (or the `GracefulExitRequested` branch) run `execute_pending_player_save_actions` once with deferral disabled (call `save_now` path) before teardown; or make the quit poll wait while `PendingPlayerSaveActions` is non-empty and deferrals remain.
- **Dimension agent ID**: SAVE-D4-02

### SAVE-09: Staging temps are never reclaimed after a process death, and `atomic_write`'s cleanup is an `Err` branch, not an unwind guard
- **Severity**: LOW
- **Dimension**: Container & Disk
- **Data-Loss Class**: none (no save is lost; hidden files accumulate)
- **Location**: `crates/core/src/atomic_file.rs:58-66` (cleanup only in `if let Err(error) = stage_and_rename(..)`), `:233` (`fs::File::create(tmp_path)`), `crates/save/src/disk.rs:47-52` (`write_slot` stages `.save_<slot>.ess.<pid>.<n>.tmp`); no reaper anywhere (`rg '\.tmp'`/"sweep" over `byroredux/src` and the four writer crates finds none).
- **Status**: NEW (#5163 removed the temp on every *returned* error; #5247 made the name unique per attempt, which turned the old overwrite-on-next-save behaviour into permanent leftovers)
- **Description**: A power cut or kill between `create` and `rename` (the case the module docs design for: "A power cut mid-write leaves the old save intact") leaves `.save_N.ess.<pid>.<n>.tmp` holding up to a full snapshot. The pre-#5247 fixed `save_<slot>.ess.tmp` was overwritten by the next save of that slot; the unique name never repeats, and `parse_slot_filename` / `slots_by_recency` / `SaveRing::resume` correctly ignore it (pinned by `newest_slot_ignores_newer_tmp_and_empty_directory`), so nothing ever deletes it. The same holds for the settings, boot-request, overrides, terrain-height cache (`asset_provider/terrain_height.rs:205`) and texture-upscale writers. A panic unwinding through `stage_and_rename` would also skip the cleanup (no `Drop` guard), although the same file now has `TempPath` for exactly that class (#7158) and no reachable panic exists on the current path. Separately, `File::create` (not `create_new`) follows a planted symlink at the staging name; the #7143 exclusive-creation work covered `create_temp_file`/`write_new_*` only. With pid + a per-process counter starting at 0, a post-reboot pid reuse can truncate a stale temp, which is harmless.
- **Evidence**: `if let Err(error) = stage_and_rename(final_path, tmp_path, bytes) { let _ = fs::remove_file(tmp_path); return Err(error); }`; `write_slot` has no sweep of `.save_*.tmp` before or after.
- **Impact**: Disk-space leak of one save per crash, invisible to the player. No durability or correctness impact on live slots.
- **Related**: #5163, #5247, #7143, #7158.
- **Suggested Fix**: in `write_slot` (and `SaveState::new`) best-effort delete sibling `.save_*.ess.*.tmp` files whose pid is not alive or older than a minute; optionally stage through `create_new`.
- **Dimension agent ID**: SAVE-D3-01

### SAVE-10: Two `atomic_write` callers carry no clobber-fallback pin, and the SKILL's "three non-save callers" is stale
- **Severity**: INFO
- **Dimension**: Container & Disk
- **Data-Loss Class**: none
- **Location**: `byroredux/src/asset_provider/terrain_height.rs:203-208` (`write_atomic`), `tools/texture-upscale/src/lib.rs:208-209` (`write_manifest`, #7192 `f561ceedf`); pins exist only in `crates/settings-io/src/lib.rs:800`, `crates/boot-request/src/lib.rs:349`, `crates/game-detect/src/overrides.rs:185`, `crates/save/src/disk.rs` (`write_slot_has_no_clobber_fallback`).
- **Status**: NEW (guard-ledger gap, not a defect)
- **Description**: both files call `atomic_temp_path` + `atomic_write` correctly and have no fallback today, but nothing asserts it; a fallback re-grown there would be the #5143 hazard (copying a partial temp over a good file) with no test to catch it. `assert_no_clobber_fallback` is `include_str!`-per-file, so it has to be added per caller.
- **Suggested Fix**: add the one-line `assert_no_clobber_fallback(include_str!(...))` test to both (the `terrain_height.rs` writer binds `temp_path` once and passes it once, so the guard applies as-is), and update the SKILL's caller list.

---
- **Dimension agent ID**: SAVE-D3-02

## Dimension notes

## No-bump baseline refreshes — judged
| Commit | Claim | Verdict |
|---|---|---|
| 01d390583 (prior audit, D2-02) | dead derives dropped from `ReferenceState`/`PersistentReferenceStates` | still true (no `Serialize` on them at HEAD) |
| 449ea0fed | `FragmentResumeCondition::MessageAnswered` appended | variant claim TRUE; refresh also absorbed `VmResume.answer` + `VmStep::Replay` (SAVE-D2-02) |
| a0e45a578 #7225 | `TravelBehavior`/`WanderBehavior.near_editor_location`, NOT_SAVED siblings of registered `TravelState`/`WanderState` in `travel.rs`/`wander.rs` | CONFIRMED: both files' diffs are exactly one `pub near_editor_location: bool`; both Behavior rows are in `NOT_SAVED_BY_DESIGN` (`registry_completeness_tests.rs:705-706`), both States registered (`registry.rs:363-364`) |
| bfcca10be #7254 | `GuardBehavior.near_editor_location`, sibling of registered `GuardState` | CONFIRMED: `guard.rs` diff = one field; `GuardBehavior` NOT_SAVED (`:676`), `GuardState` registered |
| 5ede7b032 #7262 | `SandboxBehavior.near_editor_location`, "beside the registered `Seated`" | CONFIRMED: `sandbox.rs` diff = one field (plus docs); `Seated` is registered (`registry.rs:371`), `SandboxBehavior` NOT_SAVED (`:695`). The skill/ledger text calls Seated a `*State`; it is not (wording only) |
Every other refresh in the window landed with its major bump in the same commit (v134 77ba57d6a, v135 bcfe6b5dd, v136 ff06d0dec, v137 9ea611dec, v138 7b11ebe07, v139 f3677b704, v140 9feaee8c5, v141 e15645976, v142 d3b208048, v143 ed8c4c968) and each doc-ledger entry names the right reason (v134 resource `MenuEventRegistrations`; v135 `Effect::ShowAppearanceMenu`; v136 required `ActorValue.stored`; v137 `SkillUseLevel`; v138 required `pending`; v139 required `ActiveEffect.effect_index` — `SavedActiveEffect.effect_index: u16`, no default; v140 `ReadiedSpell`; v141 `EquippedAmmo`; v142 `SessionProfile`; v143 `ObScriptLedger.forms` removed). #7228 (EquippedAmmo -> replacing, no bump) is correct: the policy is hashed into `schema_fingerprint` (`registry.rs:527-529`) and moved `BASELINE_SCHEMA_FINGERPRINT`; the v141 doc entry predates it and is not wrong.

### Two-list parity (registry vs `MUTABLE_DELTA_COLUMNS`)
Parsed 51 component columns, 39 delta entries; every delta entry is registered. Registered-not-delta: `ActorCinematicState`, `HorseTetherState` (documented at `cinematic.rs:758-763`: EntityId/handle), `AnimationPlayer`, `AnimationStack`, `Children`, `Parent`, `EscortState`, `FollowState`, `FormIdComponent` (is_form_id), `Material`, `Name`, `Seated` — all pre-existing documented exclusions. New columns this window, all in the delta list AND with a `NOT_PARKED` reason: `SkillUseLevel`, `ReadiedSpell`, `EquippedAmmo` (replacing), `ActorVitals` (SAVE-1). New resources `MenuEventRegistrations` (`register_resource`, `fragment::register_menu_events_saved`, after `register_hits_saved`) and `SessionProfile` (`register_resource_codec`, right after `CurrentExteriorContext`) are restored wholesale. No two-list drift found.

### ValidateFn parity / codec tolerance
`register_resource_codec` validates `from_value::<S>` (the wire type); `register_component_codec` validates `Vec<(u32, S)>`; `register_replacing_component` uses the same `Vec<(u32,T)>` validate as plain columns. `load_profile` ignores the saved record and returns the live one (or `Default` when absent), so it cannot fail on a saved shape; `save_world` skips a Null resource so an absent `SessionProfile` writes no column and a load of such a save leaves the live one. `register_component_codec` apply path: a `decode -> None` row is dropped and, on a replacing column, clears the live value (D2-03 contract, pinned by `a_codec_row_that_cannot_be_restored_clears_a_replacing_column_only`). `ProfileRecord`/`HashedEntry` (wire types) are in the shape scan through the codec-path module `profile_fingerprint` (`codec_registrations`), so a field change forces a bump.

### Reasons spot-checked (NOT_SAVED_BY_DESIGN / NOT_PARKED)
TRUE: `FallTracker` (`place_character_body` resets it, `systems/character.rs:1059-1064`; load pose restore calls it, `player.rs:246`; a save with no `PlayerPose` skips it — minor); `CastState`/`BowState`/`StanceTimers`/`MineClock`/`LevelNotices` (drain `reset_resource`, `live_load.rs:486-495`; `LevelNotices.announced: Option` re-seeds at `skill_use_emit.rs:334`); `RandomConversationState`/`DeadBodyState`/`AttractionState`/`JailEscapeWatch`/`ObScriptQuestTimers` (`:500-504`); `ActiveProjectiles`/`PendingTrapHits` (`:484-485`); `PackageDayBlocks` (cleared before reload by `reload_with_fresh_day_blocks`, restarted at the loaded clock after `restore_resources`, `live_load.rs:839-840`, #7341); `PapyrusMessageAnswers` (`forget_boxes` in the drain, ticket counter survives so a loaded run's ticket is never re-issued; `take_papyrus_answer` returns the default 0 for an unknown ticket); `SpellCastModel`/`BowModel`/`BlockModel`/`ArmorRatingModel`/`FallModel` and `UiLabels` and `SkillUseClasses` (re-installed by `populate_scene_runtime`/`scene_runtime.rs:79-83,242` on every cell load, live load included); `FollowUpTopics` (keyed by `Weak::ptr_eq` on the index, `npc_dialogue.rs:741-776`); `Door` (#7226 invariant; `TwoStateActivator` is the saved half); `EditorPlacement` (re-stamped at spawn; also why #7225/#7254/#7262 are safe); `ReleasePackageHold`/`AmbientPackageRequestsSeen` (rewritten every ambient pass, `cleanup.rs` table); `ActorVitals` NOT_PARKED reason (stamped by `attach_to_player` / NPC spawn); `ReadiedSpell`/`EquippedAmmo`/`SkillUseLevel` NOT_PARKED "player-scoped" (writers are `player_cast`, `bow`, `inventory/actions.rs:537` equip-on-player, and the player-only skill rank-up; no NPC writer found); `TriggerVolume` (None-sentinel re-seed, unchanged by 00c2bf882's Oblivion extension).
FALSE/PARTIAL: `Background` (SAVE-D1-01). 
Pre-existing "forward-latent / no live production spawn site" rows (`ActorStats`, `Dlc2Ttr4aPlayerScript`, `KeystoneInventory`, `MG07LabyrinthianDoor`): grep shows only `papyrus_demo` registrations + tests; still true.

### Order and invariants re-verified in code

- **`atomic_write`** (`atomic_file.rs:58-72`, `stage_and_rename` `:227-252`): create tmp -> `write_all` -> `flush` -> `sync_all` -> read-back `readback != bytes` byte compare -> `rename` -> parent-dir `sync_all`; any `Err` through the rename removes the temp. No clobber fallback in any caller (`rg` of `fs::write(`/`fs::rename(` in the five writers). The new shared `create_temp_file` / `TempPath` / `confined_output_path` / `write_new_confined` helpers (#7143/#7158/#7172/#7182) are separate from the durable writer: temp name `.{name}.{pid}.{counter}.tmp`, location (sibling) and permissions of `atomic_write` are unchanged. `write_slot` still does `create_dir_all` then stages via `atomic_temp_path`.
- Note (not a finding): the parent-dir `sync_all()?` runs after the rename, so an error there returns `Err` after the new file is already in place. `SaveCommand` then reports "write failed" and does not advance the ring; the next quicksave rewrites the same slot atomically. No loss.
- **Ring**: `SaveState::new` -> `SaveRing::resume` (`save_io/registry.rs:234`); only `SaveRing::new` use is tests; cursor advances after `write_slot` Ok only (`commands.rs:386-391`). `BYROREDUX_SAVE_DIR` spelling intact (`registry.rs:177`, pinned by a test).
- **Container**: header gate order length -> magic -> major -> fingerprint -> `checked_add` bounds -> CRC over payload -> `from_slice` (`snapshot.rs:757-805`) unchanged; minor still advisory.
- **`SaveCommand::save_now`** order: registry/state present -> `CellTransitionInFlight` -> `LoadedCellIndex` without context (#5253) -> `disable_saving` -> slot pick via `peek` -> `validate_world` + `validate_form_ids` + `validate_cinematic_entity_refs` + `validate_actor_value_identities` (abort, 20 lines) -> `drop(state)` -> `save_world` (captures `SessionProfile` through its resource codec) -> `capture_extension_state` (which runs the host's own `validate_saved_state`) -> `encode` -> `decode` + `validate_snapshot_types` (SAVE-3) -> `write_slot` -> ring advance + `SaveComplete`. The console `execute` refuses first on `actor_moves_pending` (SAVE-7); the player drain defers up to 3 ticks (SAVE-4). `LoadCommand` runs the typed preflight when queuing (SAVE-3), so `quickload_latest` falls back past an undecodable slot.
- **SessionProfile**: encode is infallible (`save_profile` clones), the wire type has only strings/`u64`, so it cannot be non-finite; the SAVE-3 gate decodes it as `ProfileRecord` (resource `validate` closure, `registry.rs:402-409`); `load_profile` returns the live record. A malformed profile would therefore be refused before `write_slot`. The `SessionProfile` column is omitted when the resource is absent (`save_world` skips `Null`), and `saved_record`/`save_profile_lines` handle that as "none recorded". See SAVE-D4-01 for the compare-side problem.
- **Ingresses**: console `save` (`SaveCommand::execute` -> `save_now`), winit F5/F9 and the pause menu and debug `input.press` (all `queue_player_save_action` -> `execute_pending_player_save_actions` -> `quicksave` -> `save_now`). No SDK or script ingress initiates a save (`crates/sdk` only mirrors the `Quicksave` input action for events; no `SaveGame` command exists in scripting; ObScript console commands run through `ObScriptCommandHost`, not the `CommandRegistry`). The only other `save_world` / `write_slot` callers are tests.
- **Frame order** unchanged: `capture_player_pose` -> `step_player_save_actions` (publishes `CellTransitionInFlight` first, `app_step.rs:1127-1141`) -> `step_save_loads` -> `step_cell_transition` (`app_events.rs:1714-1730`); the paused-frame path (#7065) still reaches them.
- **`validate_world` coverage**: runs hierarchy, equipment, `FollowState` / `EscortState` / `Seated.furniture`, animation (`AnimationPlayer`, `AnimationStack`, `Seated.animation_restore`, now skipping the `NO_PLAYER_CLIP` sentinel via `player_was_inserted`), inventory instances, `Material` finiteness. A scan of every `Serialize`-deriving struct/enum that carries an `EntityId`/`Entity` field gives: `AnimationPlayer`, `AnimationStack`, `Parent`, `Children`, `FollowState`, `EscortState`, `Seated` (covered above); `ActorCinematicState`, `HorseTetherState` (`validate_cinematic_entity_refs`); `FindProgress`, `SubmersionState`, `WaterContact` (all on `NOT_SAVED_BY_DESIGN`); `QuestAliasInjectionState.factions` (`serde(skip)`), `StoryEventFill` (reference FormIds on disk). None of the types added since 11-17 (`SkillUseLevel`, `ReadiedSpell`, `EquippedAmmo`, `SpeakingLine`, `SessionProfile`, `MenuEventRegistrations`) holds an `EntityId`; their u32 fields are FormIDs (`MenuEventRegistration.owner` is the script owner FormID like `EventRegistration.owner`). `validate_actor_value_identities` dry-runs every actor-value codec registration (`ActorValues`, `ActorVitals`, `TimedRestorations`, `ActiveMagicEffects`, `SkillProgress`, parked rows); `SessionProfile` is the only other codec and is infallible.
- **Transients**: `ActiveProjectiles`, `PendingTrapHits`, `RestSession`, `PendingActivateChoice`, `CastState`, `BowState`, `StanceTimers`, `MineClock`, `LevelNotices` are on `NOT_SAVED_BY_DESIGN` and none is in `build_save_registry`; the first four are reset in `discard_outgoing_session_requests` (`live_load.rs:484-538`), which the allowlist-reason guard cross-checks. `PendingDetectionEvents` / `PlayerActionNoise` (11-17 SAVE-6) are still swept (`live_load.rs:570-573`); the `Background` reason (SAVE-9) is the corrected text.
- **Regression spot-check of 11-17 SAVE-1..9**: SAVE-3 gate (`commands.rs:373-384`), SAVE-4/7 deferral and refusal (`:196`, `:618-628`), SAVE-5 (`validate_actor_value_identities`, `:50-99`), SAVE-8 (`disk.rs:105-111` + test) all in place; SAVE-1/2/6/9 pinned by the passing guards.

### Notes that are not findings

- The SAVE-3 post-encode decode only validates *registered* columns. The extension-state blob (`EXTENSION_STATE_RESOURCE`, an unregistered resource) is not round-tripped at save; `capture_saved_state` does call the host's `validate_saved_state`, and `ExtensionValue` has no float variant, so no concrete unloadable-save path exists today. Cheap defense-in-depth would be `decode_saved_state` on the decoded snapshot in the same gate.
- `ReadiedSpell` / `EquippedAmmo` hold raw global `u32` FormIDs (not portable pairs); that is the open Portable Save References work (#7255 / #7258), not a new gap.

### Apply sequence as read (`execute_pending_save_loads`, `byroredux/src/save_io/live_load.rs`)

1. `:664-672` take the snapshot from `PendingSaveLoadSlot` (no-op when empty).
2. `:674-676` build the registry; note `identity_before`.
3. `:681` `validate_snapshot_types`; abort keeps the session (`:688`).
4. `:689` `preflight_extension_state`; abort keeps the session (`:696`).
5. `:758` `LiveReloadLedgers::capture`; `:759` `restore_resources_subset(PRE_RELOAD_RESOURCES)`; failure reinstates and returns (`:762-769`).
6. `:776` `without_parked_state` { `:777` `reload_with_fresh_day_blocks` { `:778-787` interior arm `reload_interior_session` (preflight `:162`, `purge_cinematic_retention_state` `:183`, `drain_streaming_state` `:187`, `unload_current_interior` `:189`, `load_cell_with_masters` `:193`) or exterior arm `reload_exterior_session` (preflight `:276`, purge `:317`, drain `:321`, unload `:323`, `assemble_exterior_streaming` `:327`, FullRadius pending check `:383`) } }.
7. `:790-799` outcome `None`: `live_ledgers.reinstate`, return.
8. `:803` `reset_story_location_cursor`; `:804` `discard_pending_story_events`; `:808` `discard_package_reports`; `:813` `discard_pending_unload_deliveries`; `:816` `discard_pending_cell_resets`; `:817` `discard_outgoing_session_requests`.
9. `:823` `restore_extension_state`; failure returns (`:828`).
10. `:836` wholesale `restore_resources` again; failure returns (`:840`).
11. `:843` **`restart_day_blocks_at_loaded_clock` — differs from the skill's sequence (new, #7341; see SAVE-D5-03/-04).**
12. `:852` `restore_resident`; `:854` `release_held_dead_actor_reconciliations`.
13. `:871-881` `drop_entity_bound_continuations`.
14. `:894` `build_form_id_remap`; `:895` `apply_deltas(["ScriptPackage"])` pre-seat.
15. `:902` `reseat_ambient_packages_after_restore`.
16. `:907-912` `unresolved_form_id_pairs` + `park_unresolved_snapshot_rows`.
17. `:919` `apply_deltas(MUTABLE_DELTA_COLUMNS)`; on Ok `:921` `reconcile_dead_actor_runtime_state`, `:931` `reconcile_player_equipped_weapon`, `:937` `clear_player_gear_handoff_scratch`, `:942` drop `PerkBodyState`, `:943` `reconcile_worn_gear`, `:947` `reset_player_factions_to_record`, `:949` `reapply_scripted_faction_changes`; on Err `:967-973` reconcile dead, notify, return (no pose restore).
18. `:980` `rebaseline_actor_watch`.
19. `:988-991` diagnostic `validate_world` + `validate_form_ids` + `validate_cinematic_entity_refs`.
20. `:995-1001` `profile_fingerprint::save_profile_lines` + log + notification (informational; notification wrong, SAVE-D5-01).
21. `:1007` `end_sting_layer`; `:1009` `sync_body_after_load`.
22. `:1010-1025` `apply_player_pose` (last state change).
23. `:1026-1036` `queue_session_event(LoadComplete)` — **differs from the skill (not listed there); runs only on the full-success path, so an aborted tail (steps 9, 10, 17-Err) queues no event.**

Everything else matches the skill, including the order of the discard calls and "any failure after the reload returns immediately".


## Guards Verified

**Guards verified live** (run in /mnt/data/src/gamebyro-redux-audit at b65be1e1e)
- `cargo test -p byroredux --bin byroredux -- save_io registry_completeness serde_default_guard live_reload scheduler_access boot::schedule app_step profile_fingerprint`: 205 passed, 4 ignored (the four real-master `consumable_tests`, as documented), 0 failed.
- Re-run: `serde_default_guard` + `registry_completeness` + `a_saved_absence_clears`: 23 passed, 0 ignored — includes `saved_type_shape_changes_require_format_major_bump`, `serde_default_on_saved_struct_requires_format_major_bump`, `every_codec_registration_reaches_the_scan`, `the_schema_fingerprint_is_pinned`, `every_component_or_resource_impl_is_saved_or_explicitly_allowlisted` (incl. the `claims_a_load_reset` matcher self-checks and `RESET_OUTSIDE_THE_DRAIN` stale check), `every_delta_column_is_parked_or_explicitly_not`.
- `cargo test -p byroredux-save --lib`: 41 passed (incl. `the_bump_ledger_is_contiguous_and_current`, `a_codec_row_that_cannot_be_restored_clears_a_replacing_column_only`); `cargo test -p byroredux-save` integration 15 passed. Read-only audit, so no mutation test run to prove each guard fails on breakage.
- Regression spot-check of the 2026-11-17 fixes: SAVE-1 `"ActorVitals"` in `MUTABLE_DELTA_COLUMNS` (`registry.rs:~76`), SAVE-2 `LoadOrderBasis` + `install_form_resolver` reinstalls `SessionProfile`/resolvers (`load_order.rs:372-383`), SAVE-6 `PendingDetectionEvents`/`PlayerActionNoise` cleared in `discard_outgoing_session_requests` (`live_load.rs:~567-575`), SAVE-8 `cursor_after_newest` tie-break (`disk.rs:105-111`), D2-01/-02/-03/-04 present (codec discovery, dead derives gone, None-decode doc, scanner note). SAVE-9: only half holds — see SAVE-D1-01.

**Guards verified live**

- `write_slot_has_no_clobber_fallback`, settings-io / boot-request / game-detect `assert_no_clobber_fallback` tests: pass, scan >0 items (each asserts exactly one `atomic_file::atomic_write(` and two `temp_path` identifiers; the self-test `clobber_guard_accepts_the_writer_shape_and_rejects_fallbacks` rejects three fallback shapes and a needle that lives only in the test module).
- `failed_rename_removes_the_temp`, `atomic_write_round_trips_and_renames_the_temp_away`, `atomic_temp_paths_are_unique_hidden_siblings`, `temp_files_are_new_private_and_unique`, `a_temp_file_is_removed_unless_kept`, `confined_outputs_stay_inside_their_folder`: pass.
- `parse_slot_names`, `newest_slot_ignores_newer_tmp_and_empty_directory`, `cursor_after_newest_points_past_latest_mtime` (incl. the SAVE-8 tie-break), `resume_on_empty_dir_starts_at_zero`, `write_read_round_trip_and_atomic_rename`: pass.
- `crates/save` header-gate and CRC tests, `the_bump_ledger_is_contiguous_and_current` (ledger v133 -> v143 has every step), typed-preflight round trip: pass.
- `command_queue_tests` (mid-transition refusal, chargen refusal, ring cursor on abort, quiescent drain, non-finite refusal, fallback past an undecodable slot, unowned actor-value key, console save in a scripted-move tick, quicksave waits for queued moves): 23 passed.

**Guards verified live**

- `/tmp/audit/save/ledger_save.txt`: `byroredux-save` suites 41 + 15 passed, 0 ignored.
- `/tmp/audit/save/ledger_bin.txt`: `byroredux --bin byroredux save_io` 205 passed, 4 ignored (the four real-master `consumable_tests`).
- Targeted run (filter `live_reload_tests package_flags reference_state profile_fingerprint`): 73 passed, 1 ignored (a real-master census in `reference_state`).
- `post_reload_drain_steps_keep_their_order` still matches the real order: `restore_resources` (836) < `restore_resident` (852) < `build_form_id_remap` (894) < `["ScriptPackage"]` pre-seat (895) < `reseat_ambient_packages_after_restore` (902) < `MUTABLE_DELTA_COLUMNS` apply (919) < `reconcile_worn_gear` (943). It does not cover the new step at 843 (SAVE-D5-04).
- `LiveReloadLedgers` has six fields for the six `PRE_RELOAD_RESOURCES` entries (`live_load.rs:43-54` vs `:67-74`, `capture` 77-89, `reinstate` 93-110); every abort before the reload (`:762`) and the reload's `None` (`:797`) reinstates; the post-reload early returns (`:828`, `:840`, `:973`) correctly do not.
- `reload_with_fresh_day_blocks` (`:628-642`) wraps the reload inside `without_parked_state` (`:776-789`); `mem::take` leaves the resource installed and empty; an aborted reload restores it. `the_day_blocks_restart_before_the_reload_and_survive_an_aborted_one` passes and its source pin is non-vacuous (it finds the call inside the closure).
- `discard_outgoing_session_requests` (`:480-622`): every `reset_resource::<R>` type (`ActiveProjectiles`, `PendingTrapHits`, `LevelNotices`, `CastState`, `BowState`, `StanceTimers`, `MineClock`, `RandomConversationState`, `DeadBodyState`, `AttractionState`, `JailEscapeWatch`, `ObScriptQuestTimers`) is a plain-data `Default` that boot builds with `Default` (`boot/world.rs:197-202,398-411`; `ObScriptQuestTimers` by `init_resource`), none is filled by the reload's spawns, and the sweep runs before the second `restore_resources`, so a saved resource would still win. `ActiveProjectiles` holds no entity (Rapier handles and a shooter id), so resetting it orphans nothing. `RestSession` is a snapshot-driven resource (no menu-stack entry), so `Closed` leaves no stale UI. `RESET_OUTSIDE_THE_DRAIN` rows spot-checked: `FallTracker` (`place_character_body`, `character.rs:1062-1064`, reached from `apply_player_pose` only when the live mode is Character and a body exists; a refused corrupt pose or FlyCam leaves it, harmless), `PackageDayBlocks`, `StealthPoints` (`install_stealth_rules` from `populate_scene_runtime`, `scene_runtime.rs:98`, funnel for both loaders), `InstalledOutdoorEnvironment`.
- `place_character_body` resets `FallTracker` and `clear_momentum` (`systems/character.rs:1055-1064`); `place_character_body_ends_a_fall_in_flight` passes.
- `EquippedAmmo` is `register_replacing_component` (`registry.rs:343`) and in `MUTABLE_DELTA_COLUMNS` (`registry.rs:103`); the round-trip tests at `round_trip_tests.rs:2575-2650` pass.
- `profile_fingerprint::save_profile_lines` (`:156-173`) runs after the diagnostic validation, takes `&World` and `&Snapshot`, has no `?`/`unwrap`/panic path, returns an empty `Vec` for a missing or unparseable record (`from_value(..).ok()`) or a missing live record, and `load_profile` returns the live record; it cannot abort or mutate state. (Only its notification is wrong, SAVE-D5-01.)
- `step_save_loads` forgets `created_step_key` / `relocation_step_key` / `enabled_step_key` after the drain (`app_step.rs:1119-1121`; pinned at `:2000-2006`). Frame order `step_debug_loads` (1707) < `capture_player_pose` (1714) < `step_player_save_actions` (1719) < `step_save_loads` (1724) in `app_events.rs`.
- SAVE-1..SAVE-9 regression: fixes in place and covered by the green suites (ActorVitals codec `registry.rs:338`; `discard_stale_load_order_caches` `load_order.rs:405`; non-finite refusal `commands.rs:373-381`; `validate_actor_value_identities` `commands.rs:54`; `MAX_SAVE_DEFERRALS` `commands.rs:609`; ring tie-break `disk.rs:106`; `Background` reason names `player_identity_sync_system`). `crates/save/src/driver.rs` has no commits since 4079a9d33.
- Door path traced against the delta: `capture_script_states` skips only the open state of an entity with `Door` (`reference_state.rs:528-538`, one guard at a time); `restore_script_state` re-derives `::isOpen_var` / `::isAnimating_var` for a door (`:580-584`); the ring-door row is consumed once (`restore_parked_door_state` removes it when empty, a scripted door's `variables` stay for its one attach); a RESIDENT door never meets a parked row (it was resident at save, so its row was consumed; the reload runs with the store absent and `apply_deltas` overlays its saved `TwoStateActivator` / `ScriptVariables`); a door saved mid-swing loads with `is_animating = true` and `Door.remaining = 0`, which `door_motion_system` clears on its first tick (`door.rs` idle path, #7226) and poses by the `posed != Some(is_open)` stale path.

## Known-Open Register (cited, not re-filed)

- #6670 legacy mod compatibility: the save profile (LMC-SAV-001) is a slice of it; partial forms (#7275) are not saved concerns.
- #7059 / #7062 door runtime: the door state this audit's SAVE-01 touches is part of Phase 1.
- #6350 VATS: `VatsState` is session state and is ended by the load drain (#6508); no open save gap.
- The v133 actor-value identity arc (OAI-SAVE) is closed; SAVE-1/SAVE-2 of the 2026-11-17 audit are regression-checked and intact.
