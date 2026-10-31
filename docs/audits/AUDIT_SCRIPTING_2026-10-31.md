**HEAD**: 670f81917 · **Baseline**: `docs/audits/AUDIT_SCRIPTING_2026-10-27.md` (HEAD fa6be4a6b) · **Audited**: Dims 1, 2, 3 (delta only), 4, 5, 6 (delta-first; 58 in-scope commits in `fa6be4a6b..HEAD`, 109 files in the scripting-side paths) · **Unchanged since baseline (skimmed)**: Dim 7 (zero commits touch `papyrus_provider/`, `compatibility.rs`, `byroredux/src/extensions`, `crates/pex/src/call_sites.rs`); the Dim 3 core (`cleanup.rs`, `events.rs`, `trigger.rs`, `timer.rs`, `recurring_update.rs`, `quest_stages.rs`) and the Dim 6 VM core (`obscript_vm.rs`, `obscript.rs`, `obscript_quests.rs`, `obscript_results.rs`, `obscript_objects.rs`), `fragment/vm/interp.rs`, `fragment/budget.rs`, `fragment/dispatch.rs`, `package.rs`, `cinematic.rs`

# Scripting Runtime Audit: 2026-10-31

Suite run (`--preset comprehensive`). I ran it alone, with no sub-agents. I left source and skills untouched and did not launch the engine. Scratch notes are in `/tmp/audit/scripting/dim_{1..7}.md`. Every item in them is either in this report or explicitly dropped below.

## Build and test state: CLEAN

```
cargo test -q -p byroredux-scripting                                     1307 passed; 0 failed; 11 ignored  (10-27: 1280)
cargo test -q -p byroredux --bin byroredux -- boot::schedule cell_loader::references reference_enable_gate attach::tests
                                                                         83 passed; 0 failed; 2 ignored
```

These ran in the shared repo `target/` with `CARGO_INCREMENTAL=0`. Main CI and the lock-order lane are red for reasons filed by other audits in this suite: ECS-2026-10-31-01 (`improve_plan`) and REG-2026-10-31-02 (clippy, three errors inside this crate). Neither is a scripting-runtime defect I can add to.

## Executive Summary

Shipped since 10-27 (all of it M43, plus the 10-28 fix wave):
- **Story Manager producers**: `ADIA`, `AHEL` (moving actors), `LEVL`, `ASSU`, `AIPL`, `REMP`, `CRFT`, `OAAT`, `CHRR`, `LCLD`, `CAST`, `SKIL`, `JAIL`, `ARRT`, `ESJA`, `NVPE`, `DEAD`, `IRON`, `TMEE`, `AOBJ`, `LOCK`, `HACK`. All production producers go through `raise_story_event`.
- **Dispatcher**: *Num quests to run*, *Max concurrent quests*, the node-level `HNAM` window, mid-pass do-all wrap, and the inferred per-link `FNAM` shares-event bit.
- **Papyrus VM and lowering**: `Faction.SendPlayerToJail` (deferred as `jail_sends`), `Game.TeachWord` / `UnlockWord` (`Effect::ObtainWord`, deferred as `word_gains`), `SetRelationshipRank` raising `CHRR`.
- **Tempering**: CTDA 659 and `Mod Tempering Health` composition.
- **#6460**: ten FO3/FNV CTDA functions bridged to the ObScript getter bodies.
- **Fixes**: #6443, #6447, #6466, #6467, #6470, #6478, #6355, #6360, #6359.
- **Oblivion**: `IsPlayerInJail` (0x10ab, Oblivion row) and `GetWeaponAnimType` from the WEAP `DATA` type.

| Severity | New | Carried from 10-27 (still present, filed) |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 1 (#6510) |
| MEDIUM | 0 | 0 |
| LOW | 3 | 2 (#6511, #6512) |

No new HIGH. The three new items are LOW: a catalog index that is another function in FO4, a scratch-entity leak, and a lock-inventory drift with no cycle behind it. All three 10-27 findings are still present and already filed; they are cited under "Existing", not re-reported.

### Untrusted-input verdict (runtime-side decoders)

**No panic, OOB, OOM or overflow found.** The delta adds no new decoder over mod-supplied bytes. `obscript_vm.rs`, `obscript.rs` and the result-script entry have no commits since 10-27, so the verdict stands. The new `prim_obtain_word` / `sendplayertojail` checker arm run on `.pex`-derived AST. They bound their argument count and decline named or non-literal shapes. Compiler-frontend verdicts live in `AUDIT_PAPYRUS_2026-10-31`.

## Decline-Invariant Audit (delta)

| Decline point | Verdict |
|---|---|
| `prim_obtain_word` (`Game.TeachWord` / `UnlockWord`) | Conservative. It takes exactly one positional argument (a named one declines in `static_call`) and the receiver must be a script property. A linked-ref receiver names no property, so the apply arm skips. |
| VM `Faction.SendPlayerToJail` (`ownership_crime.rs`) | Conservative. The checker accepts only 0–2 literal-bool or Bool-kind arguments, as a statement. The run requires `is_faction`, otherwise the arm falls through and the run stops. Defaults match the CK (`abRemoveInventory = True`, `abRealJail = True`). |
| `send_player_to_jail` | A fake jail, an already-served sentence, an unknown faction, or a faction with no jail of its own declines after raising `ARRT` only. Gold, inventory and ledger are touched only on the success path. |
| Story Manager *Num quests to run* / *Max concurrent* | Conservative. Phase A counts the running pool from a snapshot, and phase B re-checks the cap per candidate (`max_concurrent_holds_across_events_in_one_frame`). |
| `HNAM` node window, `FNAM` bit 2 | The `HNAM` window gates the whole node. `FNAM` bit 2 is the documented inferred share bit (`af51f6730`, one-line removal), not a defect. |
| CTDA 659 `EPTemperingItemIsEnchanted` | **Leaks on FO4**: modeled in every game, while FO4 authors 659 as `HasVMScript` (SCR-2026-10-31-01). |
| #6460 bridged getters | Conservative. `is_modeled` is true only where `obscript_cmd::getter_answers_conditions` finds the VM row (FO3/FNV). Oblivion, Skyrim and FO4 keep declining them. |
| ObScript `IsPlayerInJail` 0x10ab, `GetWeaponAnimType` | The table census pins both. `IsPlayerInJail` is an Oblivion-only row, so FO3/FNV's 0x10ab is untouched. |

## Runtime Lifecycle Matrix (delta)

| Item | State |
|---|---|
| `StoryEvent` markers | Pattern B. `story_manager_dispatch_system` drains the markers and `PendingScriptStoryEvents` before its `SmTree` early-out. The only production code touching the marker storage directly, other than `raise_story_event`, is the `sm.event` console (documented) and the `live_load` clear. |
| New markers | None. The new components (`ArmedMine`, `SpentMine`, `ConversationHold`, `PickedUp`, `PlacedItemCount`) are persistent state owned by gameplay and carry save allowlist reasons. `JailEscapeWatch` is inserted at boot (`boot/world.rs:376`) and read by `detect_escape` (`crime.rs:353`). |
| Producer scheduling | Every new producer is `add_exclusive` in `Stage::Update` (`update/quest_scene.rs`), ahead of the dispatcher. |
| Journal subscribers | Four, unchanged. |
| Deferred flush | `flush_jail_sends` runs after `flush_crime_gold_changes`; `flush_word_gains` follows the skill advances. Both run after the quest guards drop. |
| Cascade / budget | Unchanged. |
| CATALOG | 88 rows; the module-doc table, the array length and the row-count pin agree. |

## Findings

### SCR-2026-10-31-01: CTDA 659 `EPTemperingItemIsEnchanted` is modeled in every game, but Fallout 4 authors 659 as `HasVMScript`
- **Severity**: LOW
- **Dimension**: Event Runtime (condition catalog)
- **Location**: `crates/scripting/src/condition/catalog.rs:475` (row), `:620` (`FOREIGN_INDICES`, no 659 entry), `crates/scripting/src/condition/eval.rs:95` (`is_modeled`: `_ => true`), `:595` (arm)
- **Status**: NEW (the tempering commits 2c55546f4 / 31aca901d)
- **Untrusted-Input**: No
- **Description**:
  - xEdit's `wbDefinitionsTES5.pas:3435` has `(Index: 659; Name: 'EPTemperingItemIsEnchanted')`.
  - `wbDefinitionsFO4.pas:4110` has `(Index: 659; Name: 'HasVMScript'; ParamType1: ptString)`.
  - The row is Skyrim's, but it is not listed in `FOREIGN_INDICES`, which exists for exactly this (#5990: *an index another game reuses is unmodeled there*). So on an FO4 load order, `function_of` returns `EpTemperingItemIsEnchanted`, `is_modeled` says true, and the evaluator answers `TemperingItem.enchanted`, which is always `false` outside the Skyrim improve action.
  - Every fail-closed gate (dialogue INFO / quest gates, Story Manager nodes, perk entries, scene starts, quest-log entries) therefore evaluates a `HasVMScript` term as `0.0` instead of declining. The other games in the `FOREIGN_INDICES` table (403, 449, 533, 550, 555, 630, 131, 431) are handled; this row is the only one added since without an entry. FO76 and Starfield were not read, as the table doc says for the others.
- **Evidence**: A raw CTDA census of `Fallout 4/Data/Fallout4.esm` (decompressing records, CTDA bytes 8–11) found 101 882 conditions, of which exactly two use function 659: one `QUST` and one `PERK`. DLC masters were not scanned.
- **Impact**: Small today: one quest condition and one perk condition on FO4, each wrongly evaluated as "HasVMScript == 0" when the term should make the gate decline. It is the same defect shape the table was built to stop, and any FO4 mod using `HasVMScript` is affected.
- **Suggested Fix**: Add `(659, &[GameKind::Fallout4])` to `FOREIGN_INDICES`, with a `| 659 | EPTemperingItemIsEnchanted (Skyrim) | FO4: HasVMScript |` doc row. Extend the real-data foreign-index test with the new pair.

### SCR-2026-10-31-02: `tempering_multiplier` leaves a `SceneAliasCandidate` on the `TemperingSubject` scratch entity forever
- **Severity**: LOW
- **Dimension**: Scene/Package/Cinematic (alias substrate)
- **Location**: `crates/scripting/src/tempering.rs:64-72` (lend), `:52-58` (`install` spawns the entity), consumers `byroredux/src/crafting.rs:223,258,329`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**:
  - To give item-tab conditions a subject, `tempering_multiplier` inserts `SceneAliasCandidate { base_form_id: item_base, .. }` on a scratch entity spawned once at boot.
  - It resets `TemperingItem.enchanted` after the evaluation but never removes the candidate. `with_item_enchanted`, the sibling helper, restores its mark.
  - `refresh_scene_actor_bindings` enumerates every `SceneAliasCandidate`. Its eligibility closure (`quest_alias.rs:~1180`) checks only `Dead`, reuse and reservation, with no liveness or `GlobalTransform` test on the candidate path. So after the first improve action, a placeless candidate with `reference_form_id = 0` and the last item's base FormID is a standing alias candidate.
  - Other `SceneAliasCandidate` scanners (`mines.rs`, `attraction.rs`, `random_conversation.rs`) also read `GlobalTransform`, which the scratch entity lacks, so they skip it.
- **Impact**: Latent. A Find-Matching alias gated on `GetIsID(<that weapon or armor base>)` could bind the scratch entity, and the quest's alias would point at nothing placeable. I found no vanilla alias of that shape, so this is a standing-state leak rather than an observed misbinding.
- **Suggested Fix**: Remove the candidate (or the whole component) from the scratch entity at the end of `tempering_multiplier`, as the enchanted flag already is, or keep the base form in `TemperingItem` and have the condition subject read it from there.

### SCR-2026-10-31-03: `Effect::SetRelationshipRank` (and the VM friend-rank native) raise `CHRR` under the paired quest guards, outside `apply_effect`'s lock inventory
- **Severity**: LOW (doc and inventory drift; no cycle found)
- **Dimension**: Fragment Dispatch & Locks
- **Location**: `crates/scripting/src/fragment/effects/reference_state.rs:62-80`, `crates/scripting/src/fragment/vm/natives/favor.rs:126-129`; inventory in `crates/scripting/src/fragment/effects.rs` (doc block, ~lines 55–125)
- **Status**: NEW (the CHRR producer commit 6298eda29)
- **Untrusted-Input**: No
- **Description**: `apply_reference_state_effect` runs inside the `QuestStageState` + `QuestObjectiveState` pair. The M43 change added a call to `relationships::raise_relationship_change`, which reaches `story_manager::raise_story_event`: a `StoryEvent` storage write and, on a same-entity collision, a `PendingScriptStoryEvents` write. It also reads `ScriptedRelationships` / `AuthoredRelationships` through `relationship_rank` for the old rank. None of these is in the doc block that inventories what the effect arms acquire. The `nested_lock_residual_list` scan does not follow the cross-file hop into `relationships.rs` / `story_manager.rs`. `Effect::SendStoryEvent` queues after the guards drop, specifically to avoid this nesting.
- **Evidence**: The dispatcher drops its `StoryEvent` / `PendingScriptStoryEvents` guards before taking any quest lock, and every quest-resource system is `add_exclusive`. So I found no ABBA cycle, and the lock-order lane is not red on it.
- **Impact**: None today. The next change that holds a `StoryEvent` guard across a condition read would form the cycle that the doc block's invariant is meant to flag first.
- **Suggested Fix**: Collect `(actor, other, old, new)` on `DeferredFragmentEffects` and raise after the guards drop, as `jail_sends` and `word_gains` do. Alternatively, add the four types to the inventory. The deferred form also removes the ordering dependence on the rank flush.

## Existing, not re-filed (still present at HEAD)

| ID | Issue | State |
|---|---|---|
| SCR-2026-10-27-01 | #6510 | HIGH. `get_crime_gold` / `cmd_1075` / `cmd_1076` still ignore the caller (`obscript_more/state.rs:35-47`). |
| SCR-2026-10-27-02 | #6511 | LOW. `SendTrespassAlarm` still reports done with no effect (`state.rs:160-164`). |
| SCR-2026-10-27-03 | #6512 | LOW. A non-Int cast is still transparent (`translate/effects/guards.rs:30,50,79,350,405`). |
| Known-open | #5764, #5586, #5802 | Unchanged. #5586 is debt only; #6360 already split the example. |

## Cross-references to other audits in this suite

| Item | Owner |
|---|---|
| `improve_plan` holds `Inventory` while reading `ItemInstancePool`; the lock-order lane is red | ECS-2026-10-31-01 / CONC-2026-10-31-01 / REG-2026-10-31-01 |
| `mine_system` not in the `HitEvent` producer pin | ECS-2026-10-31-02 |
| `send_player_to_jail` falls back to `CrimeModel::SKYRIM` (`jail.rs`, `unwrap_or(CrimeModel::SKYRIM)`) | CHAR-2026-10-31-L-02 |
| Clippy `-D warnings`, three errors in `jail.rs:305,389` and `ownership_crime.rs:108` | REG-2026-10-31-02 |
| Form-keyed `remove_items` in `fragment/effects/inventory.rs` consumes an improved item first | GAME-D1-2026-10-31-01 |
| Mine / attraction / random-conversation runtime issues | GAME-D4-2026-10-31-01 / -02, GAME-D5-2026-10-31-01 |
| `raise_actor_attach_story_event` displaced a doc block | GAME-D7-2026-10-31-02 |
| Nine `m43-*.sh` smokes not in a contract loop | REG-2026-10-31-04 |

## Dropped after verification

- **`send_player_to_jail` mutating state before `request_move`.** `request_move` can fail only if `PlayerMoveRequest` is absent, and `fragment/systems.rs` always installs it (same as the 10-27 drop).
- **`KnownWords` having no reader.** The state is recorded, but `TeachWord` / `UnlockWord` also raise `NVPE` for the first obtained word, so the effect is observable. It is not a ledger-honesty (#6190) case.
- **Condition 5 / 18 / 32 / 50 / 277 / 365 / 372 / 382 / 471 / 492 on Skyrim and Oblivion.** They are still unmodeled there (`getter_answers_conditions` is Fallout-only), which is the pre-#6460 state. This is a documented coverage gap, not a regression.
- **The `#6133` held-corpse list is global, not per cell.** A completing cell releases every held corpse. With one reference load in flight at a time this is unreachable, and it belongs to the gameplay owner.
- **`GetWeaponAnimType` Oblivion row `needs_caller: false`.** The handler resolves its own caller and declines with none.

## Future-Phase Readiness

- SCR-2026-10-27-01 (#6510) remains the largest open item. The command table makes the caller audit mechanical (272 of 377 rows are `needs_caller: false`); the new rows this window (`IsPlayerInJail`, `GetWeaponAnimType`) are player-only or resolve their own caller.
- The Story Manager now has about 22 producers. The dispatcher's per-event cost is linear in the event count, and `OAAT` raises one marker per attached actor but early-outs on an empty root, so FO3/FNV and Skyrim pay nothing. A future FO4 `OAAT` root with a cheap node still walks the tree once per attached actor in a cell load. That is worth a bench when FO4 exterior streaming is measured.
- Per-frame ordering is pinned by `story_dispatch_and_forcegreet_update_ordering_is_pinned`. The M43 producers all sit ahead of the dispatcher in `quest_scene.rs`.

Next: `/audit-publish docs/audits/AUDIT_SCRIPTING_2026-10-31.md` (labels `scripting`; add `game:fo4` for SCR-2026-10-31-01; `quests` for SCR-2026-10-31-02).
