**HEAD**: 4a3e89d56 · **Baseline**: `docs/audits/AUDIT_SCRIPTING_2026-10-19.md` (HEAD cb5368348, area-scoped: Dims 4, 5 slice, 6 slice) and, for Dims 1, 2, 3 and 7, the last full run `docs/audits/AUDIT_SCRIPTING_2026-10-17.md` (HEAD a071ed217) · **Audited**: Dims 1-7 (delta-first; 132 commits in scope since a071ed217, 156 repo commits since cb5368348) · **Unchanged since baseline (skimmed)**: none. Every dimension has commits. Dim 5 (scene/package/dialogue/cinematic) and Dim 7 (provider layer) were spot-checked only: their deltas are fixes to 10-17 findings, and `/audit-regression` owns re-verifying those.

# Scripting Runtime Audit: 2026-10-21

Suite run (`--preset comprehensive`). I ran it alone, with no sub-agents, read-only on the tree, and launched no engine. Scratch notes are in `/tmp/audit/scripting/dim_{1,2,3,6}.md`. Every item in those notes is either in this report or explicitly dropped below.

## Build and test state: CLEAN

```
cargo test -q -p byroredux-scripting                                         1205 passed; 0 failed; 11 ignored  (10-17: 942)
cargo test -q -p byroredux --bin byroredux -- boot::schedule cell_loader::references reference_enable_gate attach::tests
                                                                             83 passed; 0 failed; 2 ignored
BYRO_LOCK_ORDER_CHECK=1 cargo test -q -p byroredux-scripting --lib           1205 passed; 0 failed
BYRO_LOCK_ORDER_CHECK=1 cargo test -q -p byroredux-core -p byroredux-physics -p byroredux-scripting --lib
                                                                             core 829 / physics 262 / scripting 1205, 0 failed
```

The multi-crate lock-order lane that went red at cb5368348 (SCR-SD-01) is green. The fix was #6101 (`824c620a5`).

## Executive Summary

**0 CRITICAL, 0 HIGH, 7 MEDIUM, 5 LOW = 12.** Status: 8 NEW, plus 4 residuals of closed issues (#5974, #5994, #5997, #6102). The three 2026-10-19 findings are closed: SCR-SD-01 → #6101, SCR-SD-02 → #6102 (fix incomplete, see SCR-2026-10-21-05), SCR-SD-03 → #5997 (OnLoad half fixed; the GameMode half is SCR-2026-10-21-06). The 57 findings of 2026-10-17 are all closed, as #5969-#6025.

**Overall.** The architecture held. Every Papyrus-call system goes through the shared budget tail. All new request markers drain at their consumers' heads. The new `collect_deliveries` early-out (`11685934d`, today) covers every source it walks. The ObScript SCDA decoders the 10-18 wave added are bounds-clean. The new defects are semantic:

- **Integer semantics are lost on both Papyrus paths.**
  - The lowering strips `as Int` casts and reads `GetValueInt()` untruncated (01).
  - The VM returns every native numeric read as `Float` and keeps locals untyped. Since the decompiler maps `IDiv` and `FDiv` to the same `/`, integer division by an `Int`-returning native runs as float division (02).
- **Fixes that missed a consumer.**
  - #5994 missed two `OnActivate` consumers (04).
  - #6102 missed the worldspace persistent-CELL job (05).
  - #5997 fixed `OnLoad` but not `GameMode` (06).
- **The M47.3 tail answers some commands with constants** instead of declining them (07).

### Untrusted-input verdict (runtime-side decoders)

**NO panic, OOB, OOM or unbounded recursion found** in the runtime-side decoders added since 10-17. I re-read the following against the statement payload bound:

- `obscript_vm.rs`:
  - `decode_args_for`: axis letters, `decode_string_form`, `decode_ref_strings`, `TrapUpdate`.
  - `decode_show_message`, the `'z'` f64 literal, `exec_remote_set`.
  - The expression-level `' r<ref>X'` call and `' r<ref>s|f'` remote-read arms.
  - `blocks()`.
- `messages.rs::substitute_numbers` (#6014): width is clamped at parse; precision is clamped at use in `fixed` and `scientific`.
- The provider `SendModEvent` `unreachable!` arms (`execute.rs:1039-1049`): they rest on `materialize_mod_event_argument`, which uses `ScriptValue::matches`, which is strict per type. The invariant holds.

Every decode arm advances at least one byte. Every slice is bounded by the statement end, which `payload_span` bounds to the buffer. For the frontends' verdict, see the newest `AUDIT_PAPYRUS_*`.

## Decline-Invariant Audit (delta)

| Decline point | Verdict |
|---|---|
| `classify_value` Div (#5974) | Conservative. A division declines unless an operand is provably Float. |
| `int_store_is_exact` / `SetValueInt` (#5974) | **Leaks.** `GetValueInt` is in `INT_READS`, but its value is evaluated untruncated (SCR-…-01). |
| `classify_value` / `guard_reading` cast stripping | **Leaks.** `x as Int` lowers as `x` (SCR-…-01). |
| Compound assignment (#5976), non-numeric script var (#5978), `None` VMAD object (#5971), `SetEssential` receiver (#5972), arity upper bounds (#5977), inline statement cap (#5970), mixed provider script (#6021), barrier cap (#6022) | Verified in place, conservative. |
| VM native value typing | **Leaks** a Float where the CK signature returns Int (SCR-…-02). |
| ObScript unmodelled commands | Mostly decline and count. Four constant answers (SCR-…-07). |

## Runtime Lifecycle Matrix (delta)

| Item | State |
|---|---|
| Marker drains | All new request markers (`FleeRequest`, `PickIdleRequest`, `PushActorRequest`, `ResetInventoryRequest`, `ReviveRequest`, `UseWeaponRequest`, `OnResetEvent`) drain unconditionally at their consumers' heads. None of them is in `cleanup.rs`'s table (SCR-…-10). |
| `OnDeathEvent` Pattern B | Still drained ahead of the new early-out in `collect_deliveries`. Held. |
| `NativeActivation` consumers | 5 of 7 script-side consumers use `papyrus_activations`; 2 do not (SCR-…-04). |
| Two-phase lock / TypeId pair | `evaluate_guard_guard_free` uses `try_resource_2` (#6101). The lane is green in both configurations. |
| Parallel-system declarations | One cross-crate hop is undeclared (SCR-…-03). |
| `OnReset` lifecycle | Interior and tile loads retire their entries. The persistent CELL never does (SCR-…-05). |
| Cascade bound, journal subscribers, CTDA OR-precedence, edge-trigger seed | Unchanged since 10-17. Guards green. |

## Findings

### SCR-2026-10-21-01: `GetValueInt()` and `x as Int` keep the untruncated float in fragment guards and values; #5974 fixed only `/` and `SetValueInt`
- **Severity**: MEDIUM (same as #5974)
- **Dimension**: Recognizer Chain
- **Location**: `crates/scripting/src/translate/effects/guards.rs:77-81` (`classify_value` strips every `Cast`), `:357-360` (`guard_reading` strips casts), `:407-412` (`GetValue` and `GetValueInt` → the same `GuardReading::Global`), `:21` (`INT_READS` lists `GetValueInt` as provably integral); evaluated raw at `crates/scripting/src/fragment/effects/evaluate.rs:68-72` and `:263-269`; applied raw at `fragment/effects.rs:162-167`
- **Status**: NEW (residual of closed #5974)
- **Untrusted-Input**: No
- **Description**: Papyrus `GlobalVariable.GetValueInt()` and `<float> as Int` truncate. The lowering removes the cast and maps `GetValueInt` to the same `ValueExpr::Global` / `FragmentGuard::Global` as `GetValue`. Both evaluate `Globals::get`, which is the stored float. #5974's own doc on `provably_int_valued` says "classify_value strips casts, so the truncation a `Float as Int` performs would be lost". That doc uses the fact only to refuse `SetValueInt` of a cast. It still accepts `SetValueInt(G.GetValueInt())` as exact, and it accepts every guard comparison.
- **Evidence**: `GameHour.GetValueInt() == 12` at 12.5 lowers to `12.5 == 12`, which is false; Papyrus gives true. `(GameDaysPassed.GetValue() as Int) >= N` is off by the fraction at the boundary. `Counter.SetValueInt(Clock.GetValueInt())` stores 3.7 into the target global; Papyrus stores 3. Integral-typed (`s`/`l`) globals are unaffected. Float globals such as GameHour, GameDaysPassed and mod timers are affected.
- **Impact**: Stage-fragment `If` branches and set-values on float globals decide differently from the original game, with no decline.
- **Related**: #5974, SCR-2026-10-21-02 (the same loss on the VM side).
- **Suggested Fix**: In `classify_value` and `guard_reading`, decline a `Cast { Int }` whose operand is not `provably_int_valued`. Decline `GetValueInt` unless the global is integral-typed. A `ValueExpr::Trunc` would also work: it would let both lower exactly.

### SCR-2026-10-21-02: The VM returns every native numeric read as `Float` and stores locals untyped, so `IDiv` over an Int-returning native runs as float division
- **Severity**: MEDIUM
- **Dimension**: Fragment Dispatch & Locks (Papyrus VM)
- **Location**: `crates/scripting/src/fragment/vm/interp.rs:1134-1139` (`classify_native_value` → `Ok(Value::Float(number))`), `:454-460` (`Stmt::VarDecl` inserts the evaluated value, ignoring `variable.ty`), `:520-527` (`assign` to a local stores untyped); `crates/scripting/src/fragment/vm/value.rs:359-385` (integer `/` only when both runtime values are `Value::Int`), `:72` (`Float` text is `{:.6}`); `crates/pex/src/decompile/lift.rs:125` (`IDiv | FDiv` → `"/"`)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The `.pex` carries the integer/float distinction in its opcodes. The decompiler erases it, so the VM must recover it from runtime value types. Natives the CK declares `Int` all come back as `Value::Float`: `GetStage`, `GetItemCount`, `GetValueInt`, `GetLevel`, `GetRelationshipRank`, `GetFactionRank`. A typed local (`Int i = GetStage()`) does not coerce on declaration or assignment.
- **Evidence**:
  - `Int i = GetStage()` at stage 15, then `i / 10`: the VM computes 1.5, Papyrus 1. `(i / 10) * 10` gives 15 against Papyrus's 10.
  - `GetItemCount(x) / 2` and `GetValueInt() / 2` are float divisions.
  - `"Count: " + GetItemCount(x)` gives `"Count: 3.000000"`, against Papyrus `"Count: 3"`.
  - Script variables are coerced by declared type on read (`interp.rs:849-858`). Locals are not.
  - `docs/engine/scripting.md` lists no such gap.
- **Impact**: A VM run reaches a different integer result or message text than the original. The run completes and the state it writes is wrong, which is the failure mode the VM's "run whole or not at all" contract exists to exclude.
- **Related**: SCR-2026-10-21-01; closed #5974 and #5982.
- **Suggested Fix**: Give each native value read its declared return type (`Value::Int(number as i32)` for the Int-returning table rows). Coerce `VarDecl` and local `assign` to the declared type: Int truncates, Float widens.

### SCR-2026-10-21-03: The parallel `player_controller_system` writes `KillRecords` (and reads the FormID/alias storages) through the drowning death, undeclared
- **Severity**: MEDIUM (declaration drift on a parallel lane; no nested lock today)
- **Dimension**: Fragment Dispatch & Locks (scheduler access)
- **Location**: `byroredux/src/boot/schedule/early.rs:21` (`add_to_with_access`, parallel); call path `systems/character.rs:1593-1612` `apply_player_drowning_damage` → `systems/story_events.rs:199` `raise_kill_story_event` → `:185` `raise_death_event` → `crates/scripting/src/obscript_state.rs:501-516` `note_death`; scan list `byroredux/src/boot/schedule/mod.rs:752-768`
- **Status**: NEW. `note_death` was wired into `raise_death_event` by `d1b298921` (2026-10-18).
- **Untrusted-Input**: No
- **Description**: For #5641 the row declares `StoryEvent` and `OnDeathEvent`. The `PARALLEL_SYSTEMS` scan follows `raise_kill_story_event` inside `story_events.rs`. It does not follow the cross-crate call into `note_death`. `note_death` takes `try_resource_mut::<KillRecords>()` and reads `FormIdComponent`, `SceneAliasCandidate` and `PapyrusPlayerEntity` through `entity_global_form_id`. When the killer is the player, it also writes `MiscStats` and `ChallengeState` and starts magic effects. That branch cannot occur on the drowning path (killer `None`), but it shows how far the hop reaches.
- **Evidence**: `grep KillRecords byroredux/src/boot/schedule/` returns nothing.
- **Impact**: There is no conflict today, because no other Early parallel system touches these types and the guard is taken alone. A future Early system that reads `KillRecords` would be scheduled beside it with no ordering. The `every_parallel_system_declares_everything_it_acquires` scan cannot see this hop.
- **Related**: #5641, #4994 (the explicit cross-file hop list).
- **Suggested Fix**: Declare `.writes_resource::<KillRecords>()` plus the reads on the `player_controller_system` row. Add `(…obscript_state.rs, "note_death")` to its hop list, or move `note_death` out of `raise_death_event` into the exclusive reconciler.

### SCR-2026-10-21-04: Two script-side `OnActivate` consumers still read raw `ActivateEvent`, so default-processing-only activations reach them (#5994 residual)
- **Severity**: MEDIUM
- **Dimension**: Event Runtime
- **Location**: `crates/scripting/src/vm_state.rs:131-137` (`two_state_activator_system`); `crates/scripting/src/obscript_runtime.rs:700-703` (`legacy_obscript_load_order_system`)
- **Status**: NEW (residual of closed #5994; `6f7dd2213` changed extensions, MG07, rumble, quest_advance and the provider only)
- **Untrusted-Input**: No
- **Description**: CK *Activate - ObjectReference*: with `abDefaultProcessingOnly`, "no OnActivate event will be sent to any scripts". The ObScript runtime queues the same `NativeActivation` when an `OnActivate` block calls `Activate`. Two consumers ignore the marker:
  - `two_state_activator_system` is the lowered `OnActivate` of the `two_state_activator` recognizer. On such an activation it toggles `is_open` and `activated_once` and writes `ScriptVariables`, all of which are saved state.
  - `legacy_obscript_load_order_system` re-runs the reference's `OnActivate` extension-call handler on the native activation its own ObScript requested.
- **Evidence**: `grep 'query::<…ActivateEvent>' crates/scripting/src` finds both sites. Neither filters `NativeActivation`. Compare `fragment/reference.rs:684-688` and `obscript_objects.rs:283-293`, which do.
- **Impact**: A two-state lever or door scripted through the recognizer flips state when another script default-activates it, and stays flipped across save/load. Extender calls in a legacy `OnActivate` run twice.
- **Related**: #5994.
- **Suggested Fix**: Read `crate::papyrus_activations(world)` in both systems, and add them to the #5994 test set.

### SCR-2026-10-21-05: #6102's fix never retires the worldspace persistent CELL's pending `OnReset`, which brings back the per-frame rescan and late spurious resets
- **Severity**: MEDIUM (same as SCR-SD-02 / #6102)
- **Dimension**: Legacy ObScript (loader-fed state)
- **Location**: `byroredux/src/cell_loader/exterior.rs:1166-1170` (the persistent root calls `note_player_entered_cell`); `PersistentCellApplyJob::advance`, `exterior.rs:329-431` (returns `Complete` with no `cell_load_finished`); the only retirers are `load.rs:733` and `exterior.rs:2521`; `crates/scripting/src/obscript_state.rs:243-280` (`mark_cell_reset`)
- **Status**: NEW (residual of closed #6102, `2efed9843`)
- **Untrusted-Input**: No
- **Description**: The persistent CELL is "entered" each time the worldspace is built. When the player returns after 72 game hours or more (`iHoursToRespawnCell`), `note_cell_entered` queues a reset for it. `mark_cell_reset` now keeps an entry until `cell_load_finished` marks it, or until its root despawns. The persistent job never calls `cell_load_finished`, so the entry lives as long as the player stays in the worldspace. Every frame, `mark_cell_reset` rebuilds the scripted-instance list and does two lookups per instance; that is the cost #6102 set out to remove. The persistent root also keeps acquiring references after its job completes: #6076 tile-streamed persistent actors are stamped under it (`exterior.rs:1329`), and so are re-spawns after eviction. Each of those receives `OnReset` when it appears, possibly hours after the entry.
- **Evidence**: grep finds `cell_load_finished` only at `load.rs:733` and `exterior.rs:2521`. #6102's three tests cover interior and tile loads, not the persistent job.
- **Impact**: On FNV, FO3 and Oblivion, trap and animated-object `OnReset` blocks fire on references the player meets mid-session, and per-frame work persists for the whole exterior stay.
- **Related**: #6102, #6076, SCR-SD-02.
- **Suggested Fix**: Call `obscript_state::cell_load_finished(world, self.cell_root)` before `PersistentCellApplyProgress::Complete`. Add a test that loads a persistent root and then stamps a later reference under it.

### SCR-2026-10-21-06: ObScript `GameMode` still runs every frame on identity-only stubs whose home cell is not loaded (#5997 fixed only `OnLoad`)
- **Severity**: MEDIUM
- **Dimension**: Attach & Reference Identity / Legacy ObScript
- **Location**: `crates/scripting/src/obscript_objects.rs:347` (`BLOCK_GAME_MODE => runs.push`, no residency test); `byroredux/src/cell_loader/references/attach.rs:368` (`attach_object_script` runs whatever the `has_3d` value); stub producers `exterior.rs:393` (remote persistent actors) and `asset_provider/script.rs:1586` (`RemoteSceneActorStub`)
- **Status**: NEW (residual of closed #5997, `a44d4a20d`; this is the GameMode half SCR-SD-03 named on 10-19, now untracked)
- **Untrusted-Input**: No
- **Description**: The local CS wiki answers the open question SCR-SD-03 raised. `(main)/GameMode.wiki`, Notes: "Not all scripts run every frame, e.g, Quest scripts … and scripts on references not loaded". `Disable.wiki` (both CS and GECK) says scripts *do* run on disabled references, so disabled placements in loaded cells are correct. Only stubs standing for references in unloaded cells are wrong.
- **Evidence**: Since #5997, `collect_due`'s `OnLoad` arm is gated on `OnCellLoadEvent`, which a stub no longer gets. No gate exists for `GameMode`.
- **Impact**: `GameMode` bodies (timers, `MoveTo`, `PlaceAtMe`, `AddScriptPackage`) of persistent scripted actors run every frame from the far side of the worldspace. The per-frame cost grows with the persistent cell's scripted-actor count.
- **Related**: #5997, SCR-SD-03, #2664 (stub/real reconciliation, unbuilt).
- **Suggested Fix**: Skip `BLOCK_GAME_MODE`, and the other per-frame blocks, for entities carrying `RemoteSceneActorStub` or a logical-stub marker the persistent loop stamps. Or attach no `ObScriptInstance` until the reference spawns with 3D.

### SCR-2026-10-21-07: Four ObScript commands answer with constants for state the engine does not read, instead of declining
- **Severity**: MEDIUM (`GetAnimAction` changes behaviour; the other three only hide coverage gaps)
- **Dimension**: Legacy ObScript
- **Location**: `crates/scripting/src/obscript_more/tail.rs:460-463` (`IsGuard` 0x107d → `flag(false)`), `:458` (`GetIsPlayerBirthsign` 0x10e0 → `false`), `:573-579` (`IsPlayerActionActive` 0x1170 → `false` for indices 1..=10 except 6); `crates/scripting/src/obscript_more/state.rs:357-360` (`GetAnimAction` 0x10db → 2 for any armed combatant, else -1)
- **Status**: NEW. `IsGuard` landed in `5d0891e78`, a commit mislabelled "Add physics and renderer audits for 2026-10-18". None of the four is in the `scripting.md` M47.3 tail table.
- **Untrusted-Input**: No
- **Description**: The Dim 6 contract says anything unmodelled is "a traced no-op returning 0, counted per owner in `ObScriptQuestTimers.unknown_commands`, never faked". `obscript_more` "answers only from a named page, engine state or the ledger".
  - `IsGuard`: the CS page reads the CLAS Guard flag, and the Oblivion CLAS flags are parsed (`crates/plugin/src/esm/records/actor/class.rs:55-67`, `:129`). The answer exists in the data but is not read.
  - `GetIsPlayerBirthsign`: answers for a player who always has a birthsign.
  - `IsPlayerActionActive`: answers "not attacking / blocking / …" for actions nobody tracks.
  - `GetAnimAction`: invents ATTACK (2) for every armed actor in combat.

  `HasBeenEaten` and `GetCrimeKnown` → `false` are defensible: the engine's world cannot produce that state.
- **Evidence**: the arms above. Three return the same number a decline would (0), but they bypass the `unknown_commands` census that `obscript_tools` uses to measure coverage. `GetAnimAction` returns 2 or -1 where a decline returns 0.
- **Impact**: `if GetAnimAction == 2` fires every frame for any armed combatant. The coverage census overstates what runs.
- **Related**: `feedback_no_guessing`; #6139 (structure of the same command chain).
- **Suggested Fix**: Read `flags_oblivion` for `IsGuard` (bit per UESP *CLAS*). Return `Action::Declined` for the birthsign, the untracked action indices and `GetAnimAction` outside a tracked attack state.

### SCR-2026-10-21-08: Challenge completion and reward-script order follow `HashMap` iteration
- **Severity**: LOW
- **Dimension**: Legacy ObScript
- **Location**: `crates/scripting/src/challenges.rs:227` (`note`: `for def in defs.0.values()`), `:278` (`challenge_system`), `ChallengeDefs(Arc<HashMap<u32, ChalRecord>>)` at `:86`
- **Status**: NEW (same class as closed #6013)
- **Untrusted-Input**: No
- **Description**: When one event or stat rise completes several challenges, the challenges complete, increment "Challenges Completed" and start their reward scripts in per-process hash order. "Challenges Completed" itself feeds misc-stat challenges, so which one finishes first can differ between runs.
- **Suggested Fix**: Iterate in FormID order: sort the finished list, or store the definitions in a `BTreeMap`.

### SCR-2026-10-21-09: `ChallengeEvent::ItemAcquired` has no producer, so *Acquire Item* challenges never progress
- **Severity**: LOW
- **Dimension**: Legacy ObScript
- **Location**: `crates/scripting/src/challenges.rs:134`, `:233-234`; producers exist only for `ItemUsed` (`byroredux/src/inventory.rs:1589`) and `Kill` (`obscript_state.rs:514`)
- **Status**: NEW
- **Description**: The module doc lists the challenge types with no source (damage, hit, skill, craft, weapon-keyed kills) but not the acquire types. Those have a matched event variant and no producer, so they silently sit at 0.
- **Related**: ESM-2026-10-21-04 (CHAL decoded with the FNV schema everywhere; the decode side of the same table).
- **Suggested Fix**: Raise `ItemAcquired` from the player's inventory-add path, or list the acquire types under "no source" in the module doc and delete the variant.

### SCR-2026-10-21-10: The new request markers are not in `cleanup.rs`'s drain table
- **Severity**: LOW
- **Dimension**: Event Runtime
- **Location**: `crates/scripting/src/cleanup.rs` module-doc table; markers `FleeRequest`, `PickIdleRequest`, `PushActorRequest`, `ResetInventoryRequest`, `ReviveRequest`, `UseWeaponRequest`, `OnResetEvent` (consumers: `systems/{flee,cinematic,knockdown,script_requests,revive}.rs`, `obscript_objects.rs:264-275`)
- **Status**: NEW
- **Description**: All seven drain unconditionally at their consumers' heads; I checked each. Because they are undocumented, `every_documented_pattern_b_marker_drains_in_its_named_consumer` does not pin them, and a later early return placed before a drain would go unnoticed.
- **Suggested Fix**: Add the seven rows, each naming its consumer.

### SCR-2026-10-21-11: Doc rot in two touched functions
- **Severity**: LOW
- **Dimension**: Fragment Dispatch / Legacy ObScript
- **Location**: `crates/scripting/src/fragment/reference.rs:629-641`; `crates/scripting/src/obscript_vm.rs:1286-1296`
- **Status**: NEW
- **Description**:
  - `11685934d` inserted `fn raised` between `collect_deliveries`' doc block and the function. The "deliveries this frame's markers make…" doc now heads `raised`, and `collect_deliveries` has none. This is the same pattern as #6136.
  - `resolve_ref_var`'s first paragraph still says "the VM models no ref-valued command, so a ref local is never assigned a real reference". The `ref_locals` paragraph under it contradicts that.
- **Suggested Fix**: Move `raised` above the doc block, and delete the stale paragraph.

### SCR-2026-10-21-12: `ResetInventory` replaces a container's `Inventory` without releasing its stacks' `ItemInstancePool` entries
- **Severity**: LOW (gameplay-owned; reported here because `script_requests.rs` is a Dim 6 request consumer)
- **Dimension**: Legacy ObScript (request consumers)
- **Location**: `byroredux/src/systems/script_requests.rs:186-192`
- **Status**: NEW
- **Description**: `inventories.insert(entity, fresh)` drops the old stacks. Elsewhere, consumption releases `stack.instance` back to `ItemInstancePool` (`inventory.rs:1576-1585`). The reset does not, so each scripted reset orphans the pool entries of the instanced items the container held.
- **Suggested Fix**: Release every `stack.instance` of the old inventory before the insert. Route to `/audit-gameplay` for the loot-persistence ledger interaction.

## Considered and dropped

- **`UseWeapon` hits go through `PendingTrapHits` with the raw weapon damage** (`script_requests.rs:105-131`), bypassing the combat damage formula. This is a gameplay-domain approximation and is logged at the site. Left to `/audit-gameplay`.
- **Stale `TextArguments` reaching `PlayBink` / `EssentialDeathReload` without a string argument.** Every consumer `take`s, and `text_argument` overwrites. It needs a declined `SwapTexture` immediately followed by a string-less `PlayBink`. Not worth a finding.
- **`idle_resume_system` / `idle_clip_playback_system` read `AnimationClipRegistry` while holding `AnimationPlayer` mut.** Both are exclusive (`update.rs:579-583`), so there is no live ABBA. Noted for `/audit-concurrency`.
- **`challenges::note` → `complete` → `start_effects` under a quest guard.** `start_effects` only queues `ActiveMagicEffects` and does not run the reward script inline, so it does not re-enter `QuestStageState`.
- **#5802 slice `e619dbbaf`.** Save-row positions are preserved (Speechcraft is still first after `magic_effects`). The fingerprint is `/audit-save`'s.
- **XESP resolver (#6077).** It is cycle-capped at 16 hops. A live parent toggle does not strip resident children; that is the open #5065-class live-strip gap, not new.
- **`screenshots/look_b.png` / `look_before.png` (2 MB) committed by `d1b298921`.** They are no longer in the tree (`git ls-files screenshots` lists only `fo4_magnolia.png`).
- **Suite dedup**: ESM-2026-10-21-01 (PSDT minutes as hours) does not touch the scripting runtime's package fragments; cited, not re-reported. ESM-2026-10-21-04 is cross-referenced under SCR-…-09.

## Future-Phase Readiness

- The pins that hold for the remaining ObScript phase: `MAX_OBSCRIPT_VM_NESTING` on both recursive paths, `excessive_nested_*`, `truncated_bytecode_reports_malformed`, and the FormID-ordered quest tick (#6013).
- **Missing pins:**
  - A Papyrus "Int-returning native ÷ Int" VM test (SCR-…-02).
  - A `GetValueInt` / `as Int` lowering test (SCR-…-01).
  - The persistent-CELL `OnReset` retirement (SCR-…-05).
  - `NativeActivation` coverage for `two_state_activator_system` and the legacy load-order system (SCR-…-04).
  - A `GameMode` residency test (SCR-…-06).

## Could not check

Anything that needs the engine or game data: `m47-3-obscript.sh`, `m47-triggers.sh`, the corpus count of fragments comparing `GetValueInt` or `as Int` on float globals (SCR-…-01), and the VM census of `/` over Int natives (SCR-…-02).

Next step: `/audit-publish docs/audits/AUDIT_SCRIPTING_2026-10-21.md` (labels `scripting`; add `game:fnv`/`game:oblivion`/`game:fo3` to 05-09, and `concurrency` to 03).
