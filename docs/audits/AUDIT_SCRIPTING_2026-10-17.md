# Scripting Subsystem Audit — 2026-10-17

**HEAD**: a071ed217 · **Baseline**: `docs/audits/AUDIT_SCRIPTING_2026-10-12.md` (HEAD `5d31fd637`) · **Audited**: Dims 1–7 (all changed: 530 commits since baseline, ~60k lines added in scope — the M47.2 Papyrus VM wave, #5800/#5801/#5468 file splits, #5802 save-registry moves, #5763 frame budget, #5791 VM allocation caps) · **Unchanged since baseline (skimmed)**: none (Dim 7's `papyrus_provider/` has only 2 commits, but its seams changed)

Orchestrated run: one read-only agent per dimension (3 concurrent). Scratch notes: `/tmp/audit/scripting/dim_{1..7}.md` (removed at cleanup; full text is embedded below). No engine launch, no repo edits.

## Build & test state: CLEAN

```
cargo test -q -p byroredux-scripting                         942 passed; 0 failed; 7 ignored   (10-12 baseline: 562 / 5)
cargo test -q -p byroredux --bin byroredux -- boot::schedule  21 passed; 0 failed
```
Dimension agents additionally ran their filtered guards green: `translate::` 199, `fragment:: quest_stages::` 262, Dim 3 filters 109, `cell_loader::references reference_enable_gate` 59, Dim 5 129 + 60 (+26 under `BYRO_LOCK_ORDER_CHECK=1`), `obscript` 42, `papyrus_provider compatibility` 56, `extensions::tests` 80. **No guard test catches any finding below** — the new findings are all in code the guards do not model.

## Executive Summary

**Findings: 0 CRITICAL, 10 HIGH, 18 MEDIUM, 29 LOW = 57, all NEW** except two residual links (D5-06 regression of closed #5682; D7-06 residual of closed #3946). The 2026-10-12 carry-overs are resolved: #5681 (fragment wait/queue cap), #5682 (within one operation), #5752, #5734, #5787–#5790 are all closed and verified holding (see Dim 5/6 notes). Open and unchanged: #5647, #5750, #5778, #5764, #5646.

**Shape of the result.** The decline-on-unmodeled lowering and the lock/marker architecture held up: every delivery system is exclusive and ordered, all 30 transient markers sit in a sanctioned drain pattern, `apply_effect`'s nested-lock *safety argument* is sound, the recognizer chain/arity bounds are clean across all 85 primitives, and the ObScript SCDA decoders are bounds-clean. The new risk is concentrated in **what the M47.2 VM wave added on top of mod-supplied `.pex` bytes**: the interpreter runs on the main thread with no `catch_unwind`, so any single-statement panic is a process abort, and the allocation/queue caps (#5681/#5791) were applied to the paths they named but not to their siblings.

### HIGH findings (ranked)

| ID | Title | Untrusted | Verified |
|---|---|---|---|
| D2-01 | `Array.Remove(index, count)`: `index + count` overflows → `Vec::drain` panic (crash from one script statement) | Yes | **Orchestrator re-read the code** (`natives/collections.rs:338-346`; release has no overflow-checks, wraps negative) |
| D2-02 | Float `%` does `(a as i32) % (b as i32)` — `i32::MIN % -1` panics in every profile | Yes | **Orchestrator re-read** (`vm/value.rs:367-373`; Int branch uses `wrapping_rem`, float branch does not) |
| D6-01 | `substitute_numbers` printf precision unbounded from mod text — `%.65536f` panics, repeated `%.65535f` allocates ~537 MB from 64 KB text; also gates MESG / `Message.Show` | Yes | **Orchestrator re-read** (`messages.rs:189-232`); agent reproduced with rustc 1.96 |
| D2-03 | `to_stored` recurses without depth limit on cyclic/deep struct values (`s.m = s` → stack overflow at run-end write-back) | Yes | Code reading |
| D2-04 | Zero-delay deferred/event-enqueued runs bypass `MAX_FRAGMENT_CONTINUATIONS`; `SendCustomEvent` fan-out grows the saved queue unbounded (#5681 incomplete) | Yes | Code reading |
| D1-02 | Quest-script inlining has no total-effect cap — multiplicative expansion of a mod `.pex` at cell load | Yes | Code reading, no repro |
| D1-01 | `defaultSetStageTrig*` recognizers default absent `doOnce`/`disableWhenDone`/`onlyOnce` to false; real scripts default true (~97 % of Skyrim.esm placements omit `doOnce`) — wrongly-lowered, wrong state, pinned by the wrong tests | No | Agent census of Skyrim.esm |
| D3-01 | Authored `GetActorValue` (fn 14) reads an AV *enum code* as an AVIF FormID on every pre-FO4 game — reads 0.0 yet `is_modeled` says modeled (1 576 FNV, 958 FO3, 120 Oblivion, 543 Skyrim uses) | No | Agent CTDA census + xEdit defs |
| D3-02 | Six more CTDA indices collide across games (403, 449, 533, 550, 555, 630); `is_modeled` special-cases only fn 79 | No | Agent census |
| D7-01 | Provider `OnInit` handlers re-run on every reference spawn (no once-only flag as on the VM path) — non-idempotent StorageUtil bodies replay into saved state | No | Code reading |

### Untrusted-input verdict (runtime-side decoders)

**NOT clean** — reversing the 10-12 verdict, because the surface grew (the VM interpreter, message formatter and fragment queue did not exist or were not audited then). Reachable from mod bytes:
- **Panic / abort (3):** D2-01, D2-02, D6-01(`%.65536f`); plus stack overflow D2-03. The run is not under `catching_panics` (only `translate_pex`/populate are), so each aborts the process, and re-crashes on load if the handler runs at `OnInit`/`OnCellLoad`.
- **Unbounded alloc / growth (4):** D6-01 (~537 MB), D2-04 (saved queue), D1-02 (inlining), D6-02 `MessageBoxQueue` (MEDIUM), D7-02/-03 (ModEvent registration/fan-out).
- **Clean:** `SCDA` bytecode decode (`obscript_vm.rs`, `obscript.rs`, `obscript_runtime.rs`: every read bounded, nesting caps on every recursive path, operand stack cannot grow from a lying count, one parity gap D6-06 LOW); provider runtime `execute.rs` (no reachable panic; the `unreachable!` arms rest on checks that run immediately before them); SDK payload decode; `.pex` attach path (no `unwrap` on `.pex` data; checker recursion bounded). Hardening: D4-04 (`check_*` outside the panic net).

The three one-line crash fixes (D2-01 `saturating_add`, D2-02 `wrapping_rem`, D6-01 precision clamp) plus a `catch_unwind` around `run_fragment` would close the whole panic class cheaply.

## Decline-Invariant Audit (Dim 1 + seams)

| Decline point | Verdict |
|---|---|
| `classify_guard_atom`/`split_and`, `classify_condition` (De Morgan, `&&`, flattened `Any`, every unclaimed atom → `None`), unresolvable guard declines whole `Conditional` (#3785) | Verified conservative |
| `lower_statements`: `Return` position (#5631), branch rebind (#5632), non-local assignment (#5638), depth cap 256 | Verified conservative — **except** compound `AssignOp` on a local is ignored (D1-08 LOW, unreachable from `.pex` today) |
| Arity bounds: all 85 `EFFECT_PRIMITIVES` rows have a real upper bound | Verified conservative (the guard test is only a substring check — D1-09 LOW) |
| Default-only parameters literal-only (`bool_arg`): AddItem/RemoveItem/Lock/StartCombat/AddPerk/SetObjectiveDisplayed/BlockActivation/MoveTo | Verified conservative |
| Receiver binding: `OwningQuest`, `SelfRef`, `receiver_object`, quest-receiver proof before scene fallbacks | Verified conservative — **except** `SetEssential`/`SetProtected` on FO4/SF per-instance `Actor` receivers lower as ActorBase writes (D1-04) |
| Recognizer boot-state handler (#5639), chain order, `translate_pex` `catching_panics` | Verified conservative |
| `DamageValue`/`RestoreValue` negative amounts | **Leaks** (D1-05): CK says sign discarded, lowering heals |
| `ValueExpr` untyped f32 (Int division / `SetValueInt` truncation) | **Leaks** (D1-06) |
| VMAD `None` object property → form 0 | **Leaks** (D1-03) |
| `defaultSetStageTrig*` property defaults | **Leaks** (D1-01 HIGH) |
| Provider seam: primitives first, servable catalog; barrier count not bounded at lowering | Conservative except D7-06 (LOW, residual of closed #3946) |
| ObScript VM: unresolved caller, variable slot, remote `Set`, `Ref(0)` | Verified conservative; `GetSelf` 0x10ce unevidenced (D6-05 LOW) |
| Story Manager: uncatalogued CTDA declines node (#5380) | Verified conservative; but scene/phase/quest-log/quest-advance/inventory consumers still trust the fail-open pass (D3-05) |

## Runtime Lifecycle Matrix (Dims 2–5)

| Item | State at HEAD |
|---|---|
| Marker drains | All 30 markers in a sanctioned pattern (Pattern A list `cleanup.rs:93-118`, Pattern B owners drain before any early return). `HavokImpulseRequest` doc contract inaccurate (D5-08 LOW, no strand today); `OnDeathEvent` missing from the cleanup table (D3-08) |
| Multi-producer markers | `HitEvent` (#4324/#5860/#5872), `OnTriggerEnterEvent`, `Item/EquipmentEventBatch`, `QuestStageAdvancedBatch`, `Splash/RippleEvent` all merge. `StoryEvent` one-slot: Existing #5647 |
| Two-phase lock discipline (`timer_tick`, `trigger_detection`, `recurring_update`) | Holds |
| Fragment lock nesting | Safety argument sound (every quest-resource system exclusive); **inventory doc + scan wrong/blind** (D2-06 LOW). New lock-order edge: SM dispatcher `StoryManagerNodeState ↔ QuestStageState` (D5-02 MEDIUM, introduced by `35f1cf051`; lane test blind — no test installs `StoryManagerNodeState`) |
| Cascade `MAX_CASCADE`=64, objective transitions, journal subscribers | Hold |
| Frame budget (#5763) | Renewed in `event_cleanup_system`; `exhausted()` before snapshot. `hit_event_system` drops rather than defers (D2-07 LOW) |
| Edge-trigger seed, CTDA OR-precedence, CATALOG pin (48 rows) | Hold; catalog is game-agnostic for 7 colliding indices (D3-01/-02) |
| Hit ordering | `reference_script_event_system` runs before projectile/`ProcessTrapHit` producers → their hits never reach `OnHit` (D3-03 MEDIUM; guard lists it only as producer) |
| Provider/fragment continuation queues | Provider queue capped on every path (no D2-04 analogue); fragment queue bypassed (D2-04) |
| Cinematic retention/readoption, force-greet hold/consume, #5867/#5861 death teardown, SM bounds | Hold |

## Future-Phase Readiness

- **Before wider M47.2 content:** the panic class (D2-01/-02/-03, D6-01) and queue cap (D2-04) should be closed; add a `catch_unwind` boundary at `run_fragment` and a hostile-operand test sweep next to the #5791 tests.
- **CTDA catalog:** make `ConditionFunction` carry its game scope (or key by `(game, index)`) so `is_modeled` generalises beyond fn 79 (D3-01/-02/-05); convert the remaining fail-open consumers to the fail-closed gate.
- **Alias follow-ups:** location-alias ordering within one refresh (D5-01) and the Location Alias Reference fill (D5-03) are now unblocked by the shipped LCTN model; the m47-3 design table still lists the row "partial".
- **ObScript phase 2:** invariants pinned (bounds, nesting caps, dialect separation); #5787–#5790 closed.
- **Docs/pins:** inventory scan (D2-06), arity-bound guard (D1-09), cleanup table (D3-08/D5-08), D1-12 split rot.

---


# Dimension 1 Recognizer Chain & Effect Lowering

### Delta scope

- Baseline `5d31fd637` .. HEAD `a071ed217`: 84 commits touch `crates/scripting/src/translate/**` and `papyrus_provider/**`.
  The big ones: `c0de708ae` (#5801, `translate/effects.rs` split into `effects/{model,guards,lower,inline,receivers,scope,vm_bridge,prims/*}.rs`), the ~60 `M47.2:` primitive commits,
  If/ElseIf/While lowering (`2ae893463`, `04cfff05a`, `9a0f68fa6`), `Any` guards (`dd2d3b3e8`), run-time `ValueExpr`/`SetLocal` (`5a8ac6181`, `93ea0ed5d`), quest-script inlining with arguments (`b01649090`),
  `defaultSetStageTrigScript` recognizer (`a649b55b4`), rumble VMAD overrides (`dbc80585f`), `GetLinkedRef` receivers (`b1bfa635d`).
- Read in full at HEAD: all of `translate/effects/prims/*.rs` (85 table rows + 3 helpers), `guards.rs`, `lower.rs`, `inline.rs`, `receivers.rs`, `scope.rs`, `vm_bridge.rs`, `model.rs` (`kind`/`is_placeholder`/`ValueExpr`),
  `translate/mod.rs`, `compose.rs` diff, `recognizers/{quest_stage_gate (diff + recognize path),rumble (diff),two_state_activator}.rs`, and the appliers they feed
  (`fragment/effects.rs`, `effects/{ai_combat,inventory,scene,resolve,evaluate}.rs`, `deferred.rs` fields).
- Real-signature evidence: decompiled vanilla `.pex` (Skyrim SE `Skyrim - Misc.bsa`, Fallout 4 `Fallout4 - Misc.ba2`, Starfield `Starfield - Misc.ba2`) with the existing `dump_pex_script` example
  (outputs kept in the scratchpad, not the repo), plus CK / FO4-CK wiki pages under `/mnt/data/src/reference/`. A raw byte scan of `Skyrim.esm` (python, read-only) counted VMAD property presence for the set-stage trigger scripts.

### Guards run

- `cargo test -q -p byroredux-scripting -- translate::` -> 199 passed, 0 failed (includes `every_effect_primitive_bounds_its_argument_count`, `only_the_counter_only_primitives_are_placeholders`, `a_decompile_panic_is_a_silent_none`, `unrecognized_script_is_a_silent_miss`, `every_production_file_is_in_the_sources_concat`).
- `cargo test -q -p byroredux-scripting --test pex_recognize_e2e -- --ignored da10_pex` -> 2 passed (`da10_pex_is_recognized_as_a_quest_stage_gate`, `da10_pex_reproduces_hand_builder_byte_for_byte`, real Skyrim SE data).
- `cargo test -q -p byroredux --bin byroredux every_lowering_seam` -> 1 passed (`every_lowering_seam_reads_the_servable_catalog`).

### Verified-clean items

- Arity bounds: manual sweep of every one of the 85 `EFFECT_PRIMITIVES` rows (`translate/effects/prims/*.rs`) - each has a real upper bound (`== n` / `> n` / `(a..=b)`), not just a guard-test substring; the post-split test still sees all bodies (88 `prim_*` fns vs 85 rows + 3 helpers, `prims/mod.rs:81-167`).
- Primitive table vs real Papyrus signatures (12+ spot-checked, all agree on name/arity/param meaning): `Actor.AddSpell(akSpell, abVerbose)`, `RemoveSpell`, `StartCombat` (Skyrim 1 arg, FO4/SF `abPreferredTarget`), `StopCombat()`, `StopCombatAlarm()`, `SetCrimeFaction`, `SetUnconscious`, `SetGhost`, `EvaluatePackage` (FO4 `abResetAI`), `Kill`/`KillSilent`, `SetRelationshipRank(akOther, aiRank)`,
  `SetDoingFavor`, `AddPerk` (FO4 `abNotify`), `EquipItem(akItem, abPreventRemoval, abSilent)`, `Faction.SetEnemy/SetAlly` flag names + directions, `ObjectReference.Lock(abLock, abAsOwner[, abTraverseLinkedLockables])`, `Unlock(abAsOwner)`, `MoveTo` (5 args Skyrim/FO4, 6 Starfield), `AddItem`/`RemoveItem` (3/4 args), `Disable/Enable(+NoWait)`, `AddToMap`, `SetLockLevel`, `Quest.SetObjective*`, `SetActive`, `GlobalVariable.SetValue/SetValueInt/Mod`, `Game.AddAchievement/IncrementStat/AdvanceSkill/GivePlayerCaps/RemovePlayerCaps/RewardPlayerXP/SetInChargen/SetSittingRotation`, `Message.Show/ShowAsHelpMessage`, `Location.SetKeywordData/SetCleared`, FO4 `Quest.SetObjectiveSkipped` body and `Game.GetCaps()` = `GetForm(15)`.
- Over-arity FO4/SF `Actor.SetPlayerTeammate(abTeammate, abCanDoFavor, abGivePlayerXP)` (3 args) declines at `prims/actor_state.rs:53` - safe decline (coverage miss only, not a finding).
- Default-only parameters take literal-only (`bool_arg`) everywhere: `AddItem/RemoveItem abSilent` (`inventory.rs:101,131`), `Lock` (`lock.rs:49-50`), `StartCombat abPreferredTarget` (`ai_combat.rs:50`), `AddPerk abNotify` (`actor_state.rs:185`), `SetObjectiveDisplayed abForce` (`quest_scoped.rs:132`), `BlockActivation abHideActivateText` (`world.rs:246`), `MoveTo` offsets/rotation (`placement.rs:47-55`), `SetLinkedRef abPromoteParentRefr` (`world.rs:157-161`), `ShowAsHelpMessage` args (`world.rs:35-52`).
- Enable/DisablePlayerControls default tables are per-function (`player_control.rs:58-62`, #5651); `SetMotionType` integer literals only `1` (`vehicle_cinematic.rs:74-77`, #5633).
- Receiver binding: `QuestRef::OwningQuest` only from `Self.GetOwningQuest()` (`compose.rs:155-165`, #5634); `SelfRef` declines in REFR handler recognizers (`quest_stage_gate.rs:95`); `receiver_object` rejects `self`/quest/player/speaker/decl locals (`receivers.rs:360-370`); `Start/Stop/Reset/SetActive` require a provable quest receiver before the scene/other fallbacks (`receivers.rs:34-50`); no `ObjectRef`/`QuestRef` lowers to a literal 0.
- Control flow: `Return` only as final statement at depth 0 (`lower.rs:457`, #5631); undeclared bare-ident assignment declines unless `vmad_is_quest && is_script_variable` (`lower.rs:435-446`, #5638); a branch re-binding a pre-branch local declines (`lower.rs:538`, #5632); depth cap `MAX_CONDITIONAL_DEPTH` = `MAX_STMT_DEPTH` = 256 on both `lower_statements` and the ElseIf chain (`lower.rs:381,510`); `While` only via the two closed shapes (`lower.rs:466-471`, `prim_wait` caps seconds at `MAX_PROVIDER_WAIT_SECONDS`, `prims/mod.rs:180`).
- `classify_condition`: `Not` pushed down by De Morgan, `&&`/negated `||` = conjunction, `||` = flattened `Any`, every unclaimed atom returns `None` (`guards.rs:151-234`). Unresolvable guard at run time declines the whole `Conditional` (`fragment/effects.rs:869-905`, #3785 fix holds).
- Chain order: `RECOGNIZERS` = `two_state_activator`, `rumble`, `quest_stage_gate` (`translate/mod.rs:49-55`); the two `defaultSetStageTrig*` name shortcuts live inside `quest_stage_gate::recognize` and cannot shadow a per-script recognizer. Within `EFFECT_PRIMITIVES` the only shared-method-name pairs (`Start`/`Stop` quest vs scene; `SetValue` 1-arg global vs 2-arg actor value; `Clear`) are ordered/arity-disjoint (`prims/mod.rs`).
- `translate_pex` wraps parse -> compat analysis -> decompile -> provider lowering -> recognition in `catching_panics` (`translate/mod.rs:143`); the three `populate.rs` fragment paths and `decompile_pex_for_library` catch the decompile (`populate.rs:337,564,719`, `inline.rs:23`); the lowerer itself is index-safe after its arity checks and expression recursion is bounded by the decompiler's `MAX_EXPR_DEPTH` (`pex/src/decompile/lift.rs:380`).
- Provider seam: primitives first (`prims/mod.rs:59`), servable catalog at all production lowering seams (guard test passes).
- `a649b55b4` prerequisite fallback (`quest_stage_gate.rs:236-249`) and `two_state_activator`'s source-default fallback (`two_state_activator.rs:25-33`) are correct; rumble VMAD-wins-over-source with mistyped-property decline is correct (`rumble.rs:44-48,76-109`).
- Closed trackers re-verified holding: #3785, #2186, #3940 (mistyped decline), #4318, #4323, #5068, #5407, #5631-#5634, #5638, #5651-#5653, #4694 (form->entity half).

### Findings


#### SCR-D1-2026-10-17-01: Set-stage trigger recognizers default absent `doOnce` / `disableWhenDone` / `onlyOnce` to false; the shipped scripts default them to true
- **Severity**: HIGH
- **Dimension**: Recognizer Chain
- **Location**: `crates/scripting/src/translate/recognizers/quest_stage_gate.rs:266-281` (and the pinning tests at `:1028-1048`, fixture at `:736-746`)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: `recognize_set_stage_trigger` reads the optional Bool properties as `opt_bool_property("disableWhenDone")?.unwrap_or(false)` and `opt_bool_property(once_only)?.unwrap_or(false)` (`:269,279`). When the placed reference's VMAD does not list the property, the Papyrus semantics are "the script's declared default", not `false`. The same function already does this correctly for `prereqStageOPT` (`:236-249`, falls back to the decompiled script's initializer) and `two_state_activator` does it for all three of its bools - the two Bool options are the outliers.
- **Evidence**: Decompiled vanilla scripts (Skyrim SE `Skyrim - Misc.bsa`, `dump_pex_script`): `defaultSetStageTrigScript` declares `Property doOnce Bool = True`, `Property disableWhenDone Bool = True`, `prereqStageOPT = -1`, and its `OnTriggerEnter` does `If doOnce GotoState("hasBeenTriggered")` and `If disableWhenDone Self.Disable(false)`. `defaultSetStageTRIGSpecificActor` declares `disableWhenDone = True` and `onlyOnce = True`. Raw VMAD scan of `Skyrim.esm` (uncompressed records): 330 `defaultSetStageTrigScript` instances - `doOnce` present in only 10, `disableWhenDone` in 69, so ~97% / ~79% of placements omit them; 49 `defaultSetStageTRIGSpecificActor` instances - `onlyOnce` present in 10, `disableWhenDone` in 15. The test at `quest_stage_gate.rs:1043-1046` asserts "absent `disableWhenDone`/`onlyOnce` default to false" and the new player-trigger fixture declares `doOnce = True` yet asserts `!component.disable_after_advance, "doOnce comes from the VMAD, not the source"` (`:~800`), so the wrong default is pinned by tests. The fixture also declares `disableWhenDone = False` where the real script says `True`.
- **Impact**: For the large majority of vanilla placements the recognized `QuestAdvanceOnActivate` has `disable_after_advance = false` and `disable_reference_after_advance = false`: instead of firing once and disabling itself, the trigger re-issues `SetStage` on every re-entry (`papyrus_demo/quest_advance.rs:389-437` has no other once-guard), re-running the stage's fragment effects and able to move the quest's current stage backwards (`QuestStageState::set_stage` sets unconditionally). This is an approximate lowering of 600+ vanilla Skyrim placements - worse than leaving them inert.
- **Related**: #3940 (mistyped-vs-absent split, fixed the other half), #4326 (`onlyOnce` vs `disableWhenDone` semantics), #4334 (once-only persistence).
- **Suggested Fix**: Fall back to the script's own `Bool Property` initializer (same helper shape as `script_int`) before `false`, for `disableWhenDone` and the variant's once option; correct the fixtures (`disableWhenDone = True`) and invert the pinning assertions; add a real-data check on `defaultSetStageTrigScript.pex`.

#### SCR-D1-2026-10-17-02: Quest-script inlining has no total-effect cap - multiplicative expansion of a mod `.pex` can exhaust memory at cell load
- **Severity**: HIGH
- **Dimension**: Recognizer Chain
- **Location**: `crates/scripting/src/translate/effects/inline.rs:45-199` (`inline_quest_script_call`), `lower.rs:375-494`, `fragment/populate.rs:437,670,777`
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: A call to a quest-script function is lowered by re-lowering the callee body at every call site and splicing the result in. The only bounds are `MAX_INLINE_DEPTH = 4` and the recursion guard `inline_stack.contains(&frame)` (`:80`), which stops the same function re-entering itself but not distinct functions. There is no memoisation and no cap on the number of effects a fragment may expand to, so a script whose functions each contain N calls to the next function lowers to ~N^4 effects (the fragment calls F1; F1 calls F2 N times; F2 calls F3 N times; F3 calls F4 N times; F4 holds N leaf effects). N is bounded only by the pex size / the per-function instruction limit.
- **Evidence**: code path above; `lower_statements` -> `ExprStmt` -> `classify_effect_with_providers` miss -> `inline_quest_script_call` (`lower.rs:461`) -> `lower_statements(&function.body, ..)` (`inline.rs:182`) with `effects.extend(inlined)`. Callee `Self.G()` calls inline again because `callee_scope` binds `self` as a quest local (`inline.rs:125-126`). No `MAX_*` effect or work budget exists in `translate/effects/**` or `fragment/populate.rs`. (Not exercised - the repo is read-only for this audit; the multiplication follows directly from the code.) `prune_unread_locals`, `locals_never_suspend` and `quest_property_names_with_library` also re-walk the full expanded tree / library per call.
- **Impact**: Fragment population runs on the cell-load path for every quest/scene/INFO with a quest script; N=100 gives 10^8 `Effect` values (tens of GB) - an out-of-memory abort or minutes-long stall from a modded quest script. Skill escalation: unbounded allocation on mod-supplied bytes.
- **Related**: #3279 (depth cap precedent for `lower_statements`), #5801 (split moved this verbatim).
- **Suggested Fix**: Thread a shared work budget (e.g. max lowered statements / effects per fragment, counted across `inline_quest_script_call` and `lower_statements`) that declines the fragment when exceeded; optionally memoise per `(script, function)` within one lowering.

#### SCR-D1-2026-10-17-03: A VMAD `None` object property resolves to form 0 and effects act on it instead of skipping
- **Severity**: MEDIUM
- **Dimension**: Recognizer Chain
- **Location**: `crates/plugin/src/esm/records/script_instance.rs:137-142` (`object_form_id`), consumed at `crates/scripting/src/fragment/effects/resolve.rs:18-22,52-57`, e.g. `fragment/effects/inventory.rs:170-172`, `quest_stages.rs:349`, `quest_stage_gate.rs:96-102,227`
- **Status**: NEW
- **Untrusted-Input**: Yes (VMAD property table)
- **Description**: `ScriptInstance::object_form_id` returns `Some(form_id)` for any `PropertyValue::Object { form_id, alias: -1 }`, including the null object (`form_id == 0`, how an unset `Quest`/`Form`/`Actor Property` is stored). Every `QuestRef::Property` / `ObjectRef::Property` that the lowering emits therefore resolves to `Some(0)` for an unset property, and the appliers then act on form 0 (`resolve_quest` -> `QuestFormId(0)`; `resolve_property_form_id` -> 0). The lowering side never defaults a receiver to 0, but the shared resolver does at run time. Papyrus semantics for a call on/with `None` is a logged no-op.
- **Evidence**: `AddItem(Item, n)` with `Item` unset -> `add_items(world, container, 0, n)` pushes `ItemStack::new(0, n)` into the inventory and emits `OnItemAdded` for form 0 (`inventory.rs:170-172,269-294`, no zero filter); `SomeQuestProp.SetStage(n)` with the property unset -> `stages.set_stage(QuestFormId(0), n)` creates a phantom quest-0 entry and a `QuestStageAdvanced` for it (`quest_stages.rs:349-362`); the recognizers read `myQuest` the same way (`quest_stage_gate.rs:227`) so a trigger with `myQuest` unset arms against quest 0. The only form-0 handling in the effects code is for alias-bound properties (`evaluate.rs:46,627`).
- **Impact**: An authoring mistake (or a hostile VMAD) writes phantom inventory rows / quest-state entries that are saved with the world instead of being skipped. Low frequency in vanilla, but the failure is state corruption, not a skip.
- **Related**: #2186 (strict alias == -1 check was centralised in this exact accessor).
- **Suggested Fix**: Make `object_form_id` return `None` for `form_id == 0` (single shared choke point, keeps `alias_property` unaffected), with a test that an unset property declines the effect.

#### SCR-D1-2026-10-17-04: `SetEssential` / `SetProtected` claim FO4/SF per-instance `Actor` receivers as ActorBase writes
- **Severity**: MEDIUM
- **Dimension**: Recognizer Chain
- **Location**: `crates/scripting/src/translate/effects/prims/actor_state.rs:144-170`; applier `fragment/effects.rs:402-424`; override store `bleedout.rs:65-110`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: `prim_set_base_protection` matches the method names `SetEssential` / `SetProtected` on any receiver. If the receiver is not `<x>.GetActorBase()`, it treats the expression as an `ActorBase` property (`ActorBaseRef::Form(receiver_object(..))`) - the lowerer has no property-type table to tell an `ActorBase Property` from an `Actor Property`. Fallout 4 and Starfield declare `SetEssential(Bool)` / `SetProtected(Bool)` on `Actor` itself with per-instance semantics ("This works on an individual actor, not the Actor Base"), and on `RefCollectionAlias`.
- **Evidence**: FO4 `actor.pex` (dumped): `Function SetProtected(Bool abProtected)`, `Function SetEssential(Bool abEssential)`; FO4 CK wiki `SetEssential - Actor` / `SetProtected - Actor` ("individual actor, not the Actor Base"); `actorbase.pex` and `refcollectionalias.pex` also declare them. Applier: `ActorBaseRef::Form(form)` -> `resolve_property_form_id(vmad, form.property_name())`; for `Alias.GetActorRef().SetEssential(x)` the property is alias-bound so it resolves to `None` and the effect is logged and skipped; for an `Actor Property X; X.SetEssential(x)` it resolves to the reference's form id and `base_protection` keys the override by that id, which `bleedout::base_protection` (keyed by `SceneAliasCandidate::base_form_id`) never matches. Either way the call is silently inert while the fragment is reported as fully lowered, so the VM fallback never gets the statement.
- **Impact**: FO4/SF fragments that flip a companion's essential/protected state lower "successfully" with no effect (or a wrong-keyed write), losing a gameplay effect instead of declining to the VM; the per-instance vs per-base distinction is also collapsed.
- **Related**: #4415 (base protection model), M47.2 `dd843f9e6`.
- **Suggested Fix**: Lower only the `<actor>.GetActorBase().SetEssential/SetProtected(..)` shape (drop the bare-property branch, or gate it on a known `ActorBase`-typed property), and decline other receivers so the VM (which knows declared types) handles the per-instance form.

#### SCR-D1-2026-10-17-05: `DamageValue` / `RestoreValue` accept negative amounts; the Skyrim CK says the sign is discarded
- **Severity**: MEDIUM
- **Dimension**: Recognizer Chain
- **Location**: `crates/scripting/src/translate/effects/prims/actor_state.rs:211-229`; applier `fragment/effects.rs:517-560`; `crates/core/src/ecs/components/actor_values.rs:196-205`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The amount is lowered through `classify_value` (a negative literal or a run-time number is fine) and applied as `apply_damage(avif, amount)` / `restore(avif, amount)`. A negative `amount` therefore heals on `DamageActorValue` and re-damages on `RestoreActorValue`.
- **Evidence**: CK wiki (`ck-uesp-wiki/(main)/DamageActorValue - Actor.wiki`, `RestoreActorValue - Actor.wiki`): "Negative numbers will be converted to positive so -100 and 100 will have the same effect." `actor_values.rs:196-205` has no `abs`/`max(0)` on `amount`. The FO4 `DamageValue - ObjectReference` page is silent on sign (unverified for FO4/SF); `docs/engine/scripting.md:476-480` cites only the FO4 page.
- **Impact**: Scripted `DamageActorValue("Health", -N)` (a common way to write "damage by N") inverts to a heal on Skyrim.
- **Related**: M47.2 `02f22d793`.
- **Suggested Fix**: Take `abs()` of the evaluated amount in the `DamageActorValue` arm (Skyrim spellings), or decline a negative literal; document the FO4 sign rule as unverified.

#### SCR-D1-2026-10-17-06: `ValueExpr` arithmetic is untyped f32 - Papyrus `Int` division / `SetValueInt` truncation is lost
- **Severity**: MEDIUM
- **Dimension**: Recognizer Chain
- **Location**: `crates/scripting/src/translate/effects/guards.rs:34-47` (`BinaryOp::Div` -> `ArithOp::Div`), `model.rs:278-288` (`ArithOp::apply` f32), `prims/quest_scoped.rs:26-33` (`SetValueInt` lowered identically to `SetValue`), apply `fragment/effects.rs:~145`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The decompiler's `IDIV`/`FDIV` both become `BinaryOp::Div`, `classify_value` strips `as Int` casts (`guards.rs:12-14`), and `ValueExpr::Arith` evaluates in f32 with float division. So `G.SetValueInt(H.GetValueInt() / 2)` stores 7.5 for H=15 where Papyrus stores 7, `(a / 2) * 2`, and `If a / 10 == 3` comparisons differ from Papyrus. `SetValueInt` is not distinguished from `SetValue` at all. This is an unmodeled-term acceptance (integer typing) rather than a decline. (The VM keeps `Int` arithmetic exact - `docs/engine/scripting.md:739-740` - so the two fragment paths disagree.)
- **Evidence**: code above; no `Div` test exists in `effects/tests.rs`; `ItemCount` only truncates at the final count (`resolve.rs:82`). Reachability in the vanilla corpus was not measured (corpus instruments need the full fragment walk).
- **Impact**: Wrong stored global / wrong branch for integer-division expressions; also f32 cannot represent ints above 2^24.
- **Related**: none filed.
- **Suggested Fix**: Decline `Div` (and `SetValueInt` with a non-literal) at lowering time unless both operands are provably Float, or carry an Int/Float tag in `ValueExpr::Arith` and truncate toward zero for Int.

#### SCR-D1-2026-10-17-07: Placeholder accounting - `AddAchievement` records into a store nothing reads, and `is_placeholder` ignores nested effects
- **Severity**: LOW
- **Dimension**: Recognizer Chain
- **Location**: `crates/scripting/src/translate/effects/model.rs:1014-1019`, `crates/scripting/src/misc_stats.rs:64-82`, `crates/scripting/examples/fragment_coverage.rs:421`, `mq101_conformance.rs:1744`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: (a) `Effect::AddAchievement` only does `Achievements::award(id)`; `Achievements::has` has no production reader (only `fragment/vm/tests.rs`), and `docs/engine/scripting.md:738-740` says the platform profile is the host's. By the skill's rule ("any new effect whose runtime arm only records a value needs listing in `is_placeholder` and a pin") it belongs in `is_placeholder` and `only_the_counter_only_primitives_are_placeholders`. (b) `is_placeholder` is a flat `matches!`; the harnesses call `effects.iter().any(Effect::is_placeholder)` on the top level only, so a `ShowRaceMenu`/`RequestSave`/`SetHudCartMode` inside an `Effect::Conditional` (If/ElseIf, new in M47.2), `QuestScope` or similar is not reported as a placeholder-claimed fragment; the `kind()` histogram likewise counts the wrapper, not its contents.
- **Evidence**: `rg` for `Achievements` readers: only `misc_stats.rs`, `registration.rs`, `deferred.rs`, save registry; `rg is_placeholder`: model + two examples + the pin test.
- **Impact**: Coverage numbers overstate behavioural coverage (no gameplay effect).
- **Related**: #4328, #4372.
- **Suggested Fix**: List `AddAchievement` (and extend the pin test); make the harness check recurse through `Conditional`/`QuestScope` (or give `Effect` a `contains_placeholder`).

#### SCR-D1-2026-10-17-08: Local re-assignment in `lower_statements` ignores the compound `AssignOp`
- **Severity**: LOW
- **Dimension**: Recognizer Chain
- **Location**: `crates/scripting/src/translate/effects/lower.rs:426-449`
- **Status**: NEW
- **Untrusted-Input**: No (latent)
- **Description**: `Stmt::Assign { target, op, value }` on a declared local calls `bind_local(scope, name, &value.node)` and never looks at `op`. `Int n = 1; n += 2` (`AssignOp::PlusEq`) lowers as `SetLocal n = 2`, `n *= 3` as `n = 3`, etc. The sibling writers (`lower_quest_variable_write`, `lower_self_variable_write`, `lower_reference_variable_write`) all decline `op != Eq`.
- **Evidence**: `AssignOp` has `Eq/PlusEq/MinusEq/MulEq/DivEq/ModEq` (`papyrus/src/ast.rs:225`); the parser produces them (`parser/stmt.rs:296-301`); the `.pex` decompiler only ever emits `Eq` (`pex/src/decompile/lower.rs:221`) and the `.psc` route is test-only, so this is unreachable from compiled content today.
- **Impact**: None in production today; a wrong-AST-accepted hazard for any future `.psc`-source lowering.
- **Related**: #5638 (assignment-to-undeclared decline, same code).
- **Suggested Fix**: `if *op != AssignOp::Eq { return None; }` before `bind_local` (one line + a test).

#### SCR-D1-2026-10-17-09: The arity-bound guard is a substring check that a lower bound (or a comment) satisfies
- **Severity**: LOW
- **Dimension**: Recognizer Chain
- **Location**: `crates/scripting/src/translate/effects/tests.rs:4381-4418`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: `every_effect_primitive_bounds_its_argument_count` passes a primitive if its body contains `args.len()` / `args.is_empty()` anywhere (an `args.len() >= 1` or `args.is_empty() || ..` lower-bound-only check qualifies) or if it contains the text `prim_` (a doc/comment mention counts as "delegates"). The completeness margin `seen >= EFFECT_PRIMITIVES.len()` is slack by the 3 non-table helpers (`prim_player_controls`, `prim_player_animation_event`, `prim_race_spells_arity_ok`: 88 bodies vs 85 rows). Today every primitive does have a real upper bound (verified manually), so this is a latent weakening of the #3496 gate, not a live hole.
- **Evidence**: test body above; manual sweep in "Verified-clean".
- **Impact**: A future primitive could pass the gate while still lowering an over-arity call.
- **Related**: #3496, #5801.
- **Suggested Fix**: Count only bodies that compare `args.len()` with `!=`/`>`/`<=`/range against a literal or named bound (or compare against a per-primitive max table), and exclude helper fns from `seen` by name.

#### SCR-D1-2026-10-17-10: Non-numeric script variables/properties are classified as numeric quest-variable reads that default to 0.0
- **Severity**: LOW
- **Dimension**: Recognizer Chain
- **Location**: `crates/scripting/src/translate/effects/guards.rs:18-33`, `fragment/effects/evaluate.rs:71-83`, `inline.rs:108-120`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: `classify_value` turns any identifier starting `::` (quest VMAD) or listed in `script_variables` (which `inline_quest_script_call` fills with every `Variable` *and* `Property` of the callee script, any type) into `ValueExpr::QuestVariable(SelfRef, name)`; `evaluate_value` reads an unknown variable as `0.0` (`.unwrap_or(0.0)`) rather than failing. In `classify_condition`'s final `Compare` fallback (`guards.rs:225-231`) a `==`/`!=` between two bare object-typed properties (which `reference_side` rejects because it only takes locals/calls) therefore compares `0.0 == 0.0` and is true.
- **Evidence**: code above. Reachability needs both operands to be bare non-numeric script properties; not seen in the checked samples.
- **Impact**: Wrong branch for such a comparison instead of a decline.
- **Related**: none.
- **Suggested Fix**: Resolve the variable's declared type in the inline scope (the library script is available) and only classify Int/Float/Bool variables; treat a non-numeric ident as a decline.

#### SCR-D1-2026-10-17-11: The player's reference id (0x14) is special-cased in six places; `entity_global_form_id` documents itself as the inverse but is not for the player
- **Severity**: LOW
- **Dimension**: Recognizer Chain
- **Location**: `crates/scripting/src/condition/eval.rs:106-125,142-148`; copies at `keywords.rs:91-97`, `fragment/distance.rs:166-170,179-185`, `fragment/reference.rs:593-599,821-827`, `fragment/effects/ai_combat.rs:112-117`, `fragment/effects/evaluate.rs:546-548`, `story_manager.rs:849-851`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: `resolve_entity_by_global_form_id` handles `0x14` (#4694) but `entity_global_form_id` ("the inverse ... keyed the same way so the two agree") returns the player body's pool pair (local 1) instead, so each FormID-keyed ledger writer re-implements `if Some(entity) == player { 0x14 }` (five distinct copies, plus `distance.rs:168` re-doing the form->entity half the shared resolver already owns - exactly the drift the skill warns about). A new FormID-keyed effect that forgets the copy silently keys the player under the wrong id.
- **Evidence**: grep above.
- **Impact**: Maintenance / drift hazard only today.
- **Related**: #4694.
- **Suggested Fix**: Make `entity_global_form_id` (or one shared `reference_form_id(world, entity)` next to `resolve_entity_by_global_form_id`) return `0x14` for the player and delete the copies.

#### SCR-D1-2026-10-17-12: Doc rot from the #5801 split and M47.2 additions
- **Severity**: LOW
- **Dimension**: Recognizer Chain
- **Location**: `translate/effects/model.rs:895-914,914-1012`; `prims/world.rs:8-11`; `prims/placement.rs:36-39`; `lower.rs:831-837`; `effects.rs:19-26`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: `is_placeholder`'s doc block now sits on `kind()` (the #5807 `kind` doc was spliced under it, `is_placeholder` has none) and the `kind()` match arms are unindented (not rustfmt'd); `prim_add_to_map`'s doc starts with a stale paragraph about `<actor>.SetValue(akAV, afValue)`; `prim_move_to`'s doc repeats the `TryToMoveTo` comment twice; the `lower_3d_loaded_wait` doc paragraph is attached to `lower_wait_while`; the hub doc still says "Scope (this increment) ... conservative set".
- **Evidence**: files above.
- **Impact**: Misleading docs only.
- **Related**: #5801, #5807.
- **Suggested Fix**: Re-attach the docs to the right items and run rustfmt on `model.rs`.

### Dedup notes and cross-dimension pointers


- `SetLocRefType` drops the `akLoc` argument (lowered, validated, not carried; override is not location-keyed): documented residual (`docs/engine/scripting.md:1988-1996`, `loc_ref_types.rs` module doc) - not filed.
- `ShowAsHelpMessage` lowers to a plain message and drops duration/interval/max-times/context: documented (`scripting.md:2082`, `world.rs` doc) - not filed.
- `BlockActivation abHideActivateText`, `SetLinkedRef abPromoteParentRefr`, `StartCombat abPreferredTarget`, `AddPerk abNotify`, `AddItem abSilent`, `Lock abAsOwner`: accepted as literals and not applied, per the skill's accepted discipline and the prim docs - not filed.
- `SetPlayerTeammate` / `SetDoingFavor` only feed `FollowerState` (conditions read it; the behaviours do not): documented "None of them reads it yet" (`scripting.md:~730`) - not filed.
- Closed trackers re-verified holding (no regression): #3785, #2186, #3940, #4318, #4323, #5068, #5407, #5631, #5632, #5633, #5634, #5638, #5651, #5652, #5653, #4694.
- Known-open list (#5646, #5647, #5682, #5764, #5752/#5750/#5778, #5787-#5790, #5586): none intersect this dimension.
- Unverified (not filed per no-guessing policy): `prim_activate` maps an explicit `Activate(None)` to the player (`prims/scene.rs:34-39`, applier `scene.rs:87-89`); both CK pages list `akActivator` with no default and describe nothing for `None`.

#### Items for other dimensions

- Dim 2 (VM / appliers): VM-side natives that are no-ops (`MusicType.Add/Remove`, `scripting.md:1998-2008`) are invisible to `Effect::is_placeholder` (same coverage-accounting gap as -07); `object_form_id` None -> form 0 (-03) also reaches VM-resolved properties; `QuestAdvanceOnActivate` re-fire / backwards `set_stage` consequences of -01 (`papyrus_demo/quest_advance.rs`, Dim 4 territory); Int-vs-f32 disagreement between lowered fragments and VM (-06).
- Dim 2: `Conditional` guard evaluation uses `guards.iter().all(..)` (`fragment/effects.rs:880`) - short-circuits on the first false guard before later unresolved ones; harmless for a conjunction but worth a glance when auditing `Any`.
- Dim 7 / provider seam unchanged; guard passes.

# Dimension 2 Fragment Dispatch, Quest Stages & Lock Nesting

### Delta scope
- 184 commits since baseline 5d31fd637 touch `crates/scripting/src/{fragment.rs,fragment/**,quest_stages.rs,quest_variables.rs,globals.rs}` + `byroredux/src/boot/schedule/` (VM split #5800 into `fragment/vm/{checker,interp,registrations,script_model,value}.rs` + `vm/natives/*`, #5468 `effects/*` split, #5791 caps, #5763 budget + `dispatch.rs` shared tail (#5797), reference/alias delivery, updates/distance/events/hits/anim_events registration families, package/perk/terminal fragments).
- Read in full or in the relevant ranges: `fragment/effects.rs` (doc block + `apply_effects`), all `effects/*.rs` acquisitions, `systems.rs`, `budget.rs`, `dispatch.rs`, `reference.rs` (delivery, index, unload), `events.rs`, `updates.rs`, `hits.rs`, `anim_events.rs`, `distance.rs` (budget gates), `alias_scripts.rs`, `owners.rs`, `populate.rs` (chain/replace), `state.rs` (queue/caps), `vm/interp.rs` (run_fragment, tick, assign, waits), `vm/value.rs`, `vm/natives/collections.rs`, `vm/registrations.rs`; `boot/schedule/update.rs` 400-650.
- `quest_stages.rs` (+115/-17 since baseline): journal subscribers re-checked (only SCENE/FRAGMENT/TERMINAL constants poll; no private cursor); `quest_variables.rs` is new (+273) - read the key/owner doc.

### Guards run
- `cargo test -q -p byroredux-scripting -- fragment:: quest_stages::` -> 262 passed, 0 failed, 1 ignored (includes `nested_lock_contract_documents_apply_effect_itself`, `the_nested_lock_residual_list_names_every_type_apply_effect_acquires`, `every_production_file_is_in_the_sources_concat`).
- `/tmp/audit/scripting/test_scripting.log`: 942 passed / 0 failed. `/tmp/audit/scripting/test_sched.log` (`boot::schedule`): 21 passed / 0 failed (covers the order, access-declaration and zero-conflict guards).
- Both nested-lock tests are green, yet the inventory is wrong - see SCR-D2-2026-10-17-06 (the scan has blind spots; green is not evidence here).

### Verified-clean items
- Every delivery/dispatch system is `add_exclusive*`; none on the parallel lane (`boot/schedule/update.rs:518-581`). Order matches the skill: `check_alias_scripts_dispatch` 518 -> `alias_script_event_system` 519 -> `reference_script_event_system` 520 -> `activate_parent_relay_system` 525 -> `animation_event_system` 528 -> `scripted_sound_system` 530 -> `quest_fragment_dispatch` 534 -> `quest_terminal_stage_system` 537 -> `fragment_continuation_system` 544 -> `script_update_dispatch` 550 -> `distance_event_system` 552 -> producers (trap_contact/projectiles 567-569, `pending_trap_hit_flush_system` 577) -> `hit_event_system` 581 -> `combat_damage_system` 582.
- Budget is renewed: `VmFrameBudget::reset()` in `event_cleanup_system` (`cleanup.rs:90`); `exhausted()` is checked before the per-delivery snapshot in `fragment_continuation_system` (`systems.rs:52`, before `DeferredFragmentEffects::new` at :70), `run_papyrus_call` (`dispatch.rs:30`) and `apply_effects` (`effects.rs:761`) before `run_fragment`.
- `QuestStageFragments` is `Arc`-backed (`state.rs:356`); `quest_fragment_dispatch_system` clones it before the quest guard (`systems.rs:344`) - no read->write nesting.
- `apply_effects` is an explicit cursor stack (`effects.rs:716-1020`); `QuestScope` recursion is bounded by lowering. `apply_effect` has exactly one caller (`effects.rs:1004`), so the `unreachable!` arms (`effects.rs:706`, `quest_scoped.rs:222`, `*_effect` helpers) rest on a pre-dispatch that is local and exhaustive (no wildcard arm on `Effect`).
- Cascade: `MAX_CASCADE` counts only `is_cascade` entries (`systems.rs:375-378`); no-op re-set filtered (`systems.rs:411`); stage_set==false quests skip the fragment (`systems.rs:365`). Journal mirror multiset dedup intact (`systems.rs:341-396`, `deferred.rs:1241-1276`); the spoken-INFO path does not poll the journal (#5297).
- Populate does not duplicate chains: `populate_quest_fragments_from_script_internal` builds one chain per stage and `insert` replaces (`populate.rs:409-512`, `state.rs:399`). `interpret_fallback` installs `Effect::Interpret` only when `check_function` accepts (`populate.rs:277-294`).
- Registration families key by owner id (quest/ref FormID or alias owner id): `UpdateRegistration.owner`, `EventRegistration.owner`, `HitRegistration.owner`, `Delivery.owner`; `ReferenceScriptIndex::lookup` re-validates entity + FormID (`reference.rs:147-154`). VM variable stores `QuestScriptVariables`/`QuestScriptObjects` are keyed by owner id (`interp.rs:600-611`).
- #5791 caps are present and tight for the paths they name: `Expr::New` (`interp.rs:744-748`), array literal (`:750-753`), `Array.Add` (`collections.rs:318-329`, `len+count` via `saturating_sub`), `Insert` (:330-334), string `+` and `Add`-with-string via `concat_texts` (`value.rs:405-412`; both `StrCat` and `arithmetic` Add route through it). `Utility.Wait` / `WaitForAnimationEvent` / `PlayAnimationAndWait` are capped (`interp.rs:1169,1192,1217`; NaN seconds -> `max(0.0)`). `MAX_DEPTH` 16 is checked in `run_frame` (`interp.rs:232`); event delivery never nests (receivers are queued as separate runs, `events.rs:230-266`).
- `Value::Int` arithmetic is wrapping with explicit zero-divisor stops (`value.rs:352-359`); float casts saturate.
- Nested-lock safety argument itself is sound: no reverse-order acquirer found - every path that takes `QuestStageState` outside the paired guard (`reference.rs:551,760`, `events.rs:291`, `updates.rs:352`, `distance.rs:201`, `anim_events.rs:183`, `hits.rs`) does so at system level, releases it before any run, and the comment at `reference.rs:538-541`/`raise_unload_events` keeps `alias_script_targets` ahead of the `PapyrusReferenceScripts` query.
- Journal subscribers: only `SCENE_`/`FRAGMENT_`/`TERMINAL_QUEST_EVENT_SUBSCRIBER` poll (`scene/playback.rs:533`, `systems.rs:358`, `deferred.rs:1247`, `quest_stages.rs:1333`); `quest_fragment_dispatch_system` returns before claiming when the table is empty (#3012).

### Findings


#### SCR-D2-2026-10-17-01: `Array.Remove(index, count)` overflows `index + count` and panics the engine on mod bytes
- **Severity**: HIGH
- **Dimension**: Fragment Dispatch & Locks
- **Location**: `crates/scripting/src/fragment/vm/natives/collections.rs:338-346` (checker accepts it at :374)
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: The `remove` arm computes `let end = (index + count).min(len);` on `i32` where `index` is range-checked against `len` (<= 512) but `count = int(1, 1).max(0)` is any non-negative `i32` the script supplies. `check_array_call` accepts any numeric argument for `Remove` (`:374`). `arr.Remove(1, 2147483647)` makes `index + count` overflow: in a debug/test build that is an arithmetic-overflow panic; in release (no `overflow-checks` in the workspace `[profile.release]`, `panic = "unwind"`) it wraps negative, `.min(len)` keeps the negative, and `items.drain(index as usize..end as usize)` gets an end near `usize::MAX` and panics in `Vec::drain`.
- **Evidence**: `let index = int(0, 0); let count = int(1, 1).max(0); ... let end = (index + count).min(len); items.drain(index as usize..end as usize);`. `run_fragment` is not under `catching_panics` (only `populate`/`translate_pex` are, `translate/mod.rs:143,245`), and the run happens on the main thread inside an exclusive `Update` system, so the panic takes the process down. Reachable from any VM-run quest/reference/alias script in a mod `.pex`.
- **Impact**: A single statement in a mod script crashes the game (and re-crashes on every load if the handler runs at `OnInit`/`OnCellLoad`). Same class as the already-fixed #5791 (one-statement hostile operand).
- **Related**: #5791 (caps); sibling SCR-D2-2026-10-17-02/-03.
- **Suggested Fix**: `let end = index.saturating_add(count).min(len);` (the `Value::int` operands are `i32`), and add a test beside the #5791 array tests for `Remove(1, i32::MAX)`.

#### SCR-D2-2026-10-17-02: Float `%` casts both operands to `i32` and panics on `i32::MIN % -1`
- **Severity**: HIGH
- **Dimension**: Fragment Dispatch & Locks
- **Location**: `crates/scripting/src/fragment/vm/value.rs:367-373` (`arithmetic`, float branch of `BinaryOp::Mod`); reached by `AssignOp::ModEq` at `interp.rs:474` too
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: When either operand is not an exact `Int` the modulo falls to `if b as i32 == 0 { stop } Value::Int((a as i32) % (b as i32))`. The zero case is guarded, the overflow case is not: `f32` casts saturate, so `-2147483648.0` (or anything below) becomes `i32::MIN`, and a divisor of `-1`/`-1.5` becomes `-1`; `i32::MIN % -1` panics ("attempt to calculate the remainder with overflow") in every profile, release included - Rust emits that check unconditionally. The Int/Int branch two lines above correctly uses `wrapping_rem`.
- **Evidence**: `BinaryOp::Mod => { if b as i32 == 0 { return stop("modulo by zero"); } Value::Int((a as i32) % (b as i32)) }`. Papyrus `%` is Int-only, but nothing in the checker or the decompiler's AST types forces that, so a `.pex` `IMOD` over a Float local/property (e.g. `-2147483648.0 % -1`) reaches this arm. No `catch_unwind` around the run (see -01).
- **Impact**: One expression in a mod script aborts the process.
- **Related**: SCR-D2-2026-10-17-01.
- **Suggested Fix**: Use `(a as i32).wrapping_rem(b as i32)` (and reuse the zero stop), or stop on a non-Int operand as Papyrus does.

#### SCR-D2-2026-10-17-03: Cyclic or deeply nested struct values recurse without bound in `to_stored` (stack overflow)
- **Severity**: HIGH
- **Dimension**: Fragment Dispatch & Locks
- **Location**: `crates/scripting/src/fragment/vm/value.rs:266-300` (`to_stored`), called from `interp.rs:599-603` (`write_object`) and `interp.rs:617-622` (`flush_aggregates`); assignment at `interp.rs:536-546`
- **Status**: NEW (verified by inspection; not executed - the repo is read-only for this audit)
- **Untrusted-Input**: Yes
- **Description**: Structs are `Rc<RefCell<Vec<(String, Value)>>>` shared by reference. A member assignment stores whatever value it is given (`interp.rs:541`, `members[index].1 = value;` - no type coercion; `cast` keeps objects as-is), so `s.child = s` (or `a.m = b; b.m = a`) builds a cycle. `to_stored` rejects nested *arrays* but recurses into nested *structs* with no depth limit or visited set. When a struct held in a script variable/property is written back (`flush_aggregates` at every run end, or `write_object`), the walk never terminates -> native stack overflow (SIGSEGV, uncatchable). The checker only requires `static_type(target)` to resolve (`checker.rs:489-494`), not that the value's struct type matches the member's. Without a cycle, a chain built one `x.m = y` per statement is as deep as the 100 000-statement budget allows (recursion in `to_stored`, and in `Rc` drop).
- **Evidence**: `Value::Struct(members) => { for (name, member) in members.borrow().iter() { if matches!(member, Value::Array(_)) { return None; } stored.push((name.clone(), to_stored(member)?)); } ... }`. `#5791` capped allocation but not structural depth.
- **Impact**: Mod `.pex` (FO4/Starfield struct scripts) can crash the engine with an uncatchable stack overflow at the end of the run.
- **Related**: #5791; SCR-D2-2026-10-17-01/-02.
- **Suggested Fix**: Thread a depth counter (cap ~8, structs in real content nest 1-2 deep) through `to_stored`/`from_stored` and refuse the write (`None` -> the existing "cannot be stored" stop); optionally reject a struct member assignment whose value is the container.

#### SCR-D2-2026-10-17-04: Deferred and event-enqueued runs bypass `MAX_FRAGMENT_CONTINUATIONS`; `SendCustomEvent` fan-out grows the saved queue without bound
- **Severity**: HIGH
- **Dimension**: Fragment Dispatch & Locks
- **Location**: cap at `fragment/systems.rs:121-130`; uncapped pushes at `fragment/budget.rs:164-185` (`defer`), `fragment/events.rs:230-266` (`enqueue`), `fragment/effects.rs:792,937,983`; amplifier `fragment/vm/registrations.rs:163-181` (`send_custom_event`)
- **Status**: NEW (relation to #5681 below)
- **Untrusted-Input**: Yes
- **Description**: #5681 added `MAX_FRAGMENT_CONTINUATIONS = 4_096` so "a mod that re-fires ... cannot grow the queue - saved with the world - without limit". The cap is applied in exactly one place: when `fragment_continuation_system` re-partitions entries whose `remaining_seconds > 0` (`still_pending.truncate`). Every zero-delay entry - budget deferrals (`budget::defer`), custom/remote/animation event deliveries (`events::enqueue` -> `defer`), `Interpret` yields - goes `ready`, never counts against the cap, and when the frame budget is exhausted the system returns early (`systems.rs:52-60`) or re-pushes the ready tail unchanged (`:145-161`) with no cap at all. `send_custom_event` queues *every* registered receiver's handler as a new queue entry each call and nothing tracks event generation or sender depth. A script that registers for its own custom event and re-sends it twice in the handler (`RegisterForCustomEvent(Self, "Ping")` / `Event Script_Ping(...)  SendCustomEvent("Ping") x2`) adds one net entry per run: the queue grows without bound at roughly the frame budget's run rate (~600 runs/frame), each entry holding a cloned `ScriptInstanceData` (`budget.rs:172`, `vmad.cloned()`), and it is serialised into every save (`FragmentExecutionQueue` is a saved resource).
- **Evidence**: `still_pending` (the only truncated vector) is filled only with entries that stayed `remaining_seconds > 0` (`systems.rs:76-110`); the existing test `continuation_queue_drops_newest_past_the_cap` (`tests.rs:3726`) uses timed entries and so cannot see the hole. No other reference to `MAX_FRAGMENT_CONTINUATIONS` exists (`grep`).
- **Impact**: Unbounded memory/save growth from a mod script (or a plain Papyrus event storm), the exact outcome #5681 set out to prevent, now reachable through the #5763/#593b1cfaa paths added after it. Not a crash, but unbounded allocation driven by mod bytes (HIGH under the skill's escalation rule).
- **Related**: #5681 (closed; fix incomplete for zero-delay entries), #5764 (budget-split residual).
- **Suggested Fix**: Enforce the cap at the push sites (one `FragmentExecutionQueue::push` that drops the newest and warns past the cap, used by `defer`, `enqueue` and the three `effects.rs` sites), and give `send_custom_event` a per-frame delivery ceiling.

#### SCR-D2-2026-10-17-05: `GetVMScriptVariable` (CTDA 630) cannot see a VM-run reference script's variables
- **Severity**: MEDIUM
- **Dimension**: Fragment Dispatch & Locks (owner-keyed variable stores) / Event Runtime
- **Location**: reader `crates/scripting/src/condition/eval/global.rs:124-139`; VM writes `fragment/vm/interp.rs:560-611` (`write_quest_variable` under the reference FormID) and `docs/engine/scripting.md:975-978`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: A reference script that runs on the VM keeps its variables in the saved `QuestScriptVariables`/`QuestScriptObjects` stores under the *reference FormID* (correctly owner-keyed, outliving the cell). `get_vm_script_variable` however reads only the entity component `vm_state::ScriptVariables` (`world.get::<ScriptVariables>(target)`), which is written by the lowered `SetReferenceVariable` effect and the two native activator systems - never by the VM. For a `CTDA GetVMScriptVariable(ref, "::x_var")` that targets a VM-run script the condition therefore always evaluates `0.0` ("the engine ran the script, the condition silently reads nothing"). The quest-side twin `get_vm_quest_variable` goes through `quest_variables::quest_variable` and does see VM writes, so the asymmetry is specific to reference scripts. `docs/engine/scripting.md:411` documents the entity component only for the *lowered* write path; the VM wave (`852c61078`, `5f1671e59`) did not extend it.
- **Evidence**: `get_vm_script_variable`: `resolve_entity_by_global_form_id(...)` -> `world.get::<ScriptVariables>(target).and_then(|v| v.get(variable)).unwrap_or(0.0)`; no reference to `QuestScriptVariables` in that function or in `condition/`.
- **Impact**: Dialogue/package/perk conditions gated on a VM-script reference variable (CTDA 630 is the reference-script analogue of the 3 319 Skyrim `GetVMQuestVariable` rows; census for 630 not taken - unverified) evaluate to a fixed 0.0 - wrongly open or closed gates with no diagnostic. Not a regression (no condition read of VM state existed before the wave), but the VM wave made the gap reachable.
- **Related**: #5752 / #5750 / #5778 (catalog gaps of the same fixed-0.0 flavour; this is a different cause). Not filed.
- **Suggested Fix**: Make `get_vm_script_variable` fall back to `QuestScriptVariables` under the target's FormID (same `property_key` spelling), or mirror numeric VM writes into `ScriptVariables`.

#### SCR-D2-2026-10-17-06: `apply_effect`'s nested-lock inventory is wrong and its scan has blind spots
- **Severity**: LOW
- **Dimension**: Fragment Dispatch & Locks
- **Location**: doc block `crates/scripting/src/fragment/effects.rs:45-134`; scan `fragment/tests.rs:4252-4415` (`apply_effect_family_bodies`, `the_nested_lock_residual_list_names_every_type_apply_effect_acquires`)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The guards are green but the inventory they pin is incomplete, and one sentence is false:
  1. The doc says "The `FragmentExecutionQueue` write is *not* in this list: it belongs to the latent-continuation path in the dispatch system, outside these guards" (`effects.rs:115-117`). `apply_effects` itself takes `try_resource_mut::<FragmentExecutionQueue>()` at `effects.rs:792` (VM `Utility.Wait`), `:937` (`WaitWhile`) and `:983` (`Wait`) - all inside the paired `QuestStageState`+`QuestObjectiveState` guard (`deferred.rs:1222-1233`). Also `budget::defer` (`effects.rs:764`).
  2. Types acquired under the guards but absent from the doc: `EquipmentSlots` (`inventory.rs:97`, `query_2_mut_mut` at :208), `EquippedWeapon` (:109), `ItemInstancePool` (:120) and the `EquipmentEventBatch`/`ItemEventBatch` batches (`equipment.rs:51,75`) via `remove_items`/`add_items` (called inline from `apply_inventory_effect` :172/:189 and `effects.rs:371-373`); `MiscStats`, `Dead`, `ActorVitals`, `PendingScriptKills` via `evaluate.rs:185,283,594-604`.
  3. The Papyrus VM interpreter runs inside the same guards (`effects.rs:769` -> `run_fragment`) and its natives take well over twenty more component/resource locks (`Inventory`, `SpellList`, `Seated`, `PapyrusReferenceScripts`, `InputLayers`, `EvaluatePackageRequest`, `ConditionForms`, `HavokImpulseRules`, `VmFrameBudget` write at `interp.rs:145`, ...). The doc block says "this list still inventories every lock any of them takes" and has no mention of the VM.
  Why the scan passes: it only reads functions named `apply_*_effect`, only matches the `query/query_mut/resource/resource_mut/try_resource(_mut)/get/get_mut` turbofish forms (not `has`, `query_2_mut_mut`, `resource_2_mut`), and does not follow helper functions (`remove_items`, `add_items`, `evaluate_guard`, `evaluate_value`, `resolve_actor`, `update_actor_cinematic_state`, `emit_*`, any VM native).
- **Evidence**: see line refs above; `grep -n 'FragmentExecutionQueue' effects.rs` shows the three acquisitions below the doc block.
- **Impact**: No deadlock today - the safety argument (every quest-resource system exclusive; no non-exclusive system takes quest resources) holds and was re-verified - but the artefact meant to let an auditor spot a reverse-order acquirer is false for the highest-traffic path (the VM), so the next reverse-order acquirer would not be seen by the guard.
- **Related**: #3949 / #5071 (earlier rounds of the same inventory rot), #3493.
- **Suggested Fix**: Rewrite the block to describe the real invariant ("everything under the paired guards, including the VM, relies on exclusive scheduling; no component/resource in this crate may take QuestStageState/QuestObjectiveState while holding another lock") and extend the scan to follow `fn`s reachable from `apply_effects` (or scan all of `SOURCES` for `QuestStageState`/`QuestObjectiveState` acquisitions inside non-system functions).

#### SCR-D2-2026-10-17-07: `hit_event_system` drops a hit the budget cannot start instead of deferring it
- **Severity**: LOW
- **Dimension**: Fragment Dispatch & Locks
- **Location**: `crates/scripting/src/fragment/hits.rs:205-214`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: With `starts == 0` the matching registration is skipped (`continue` at :210) while the `HitEvent` marker is drained at the end of the frame by `event_cleanup_system`. Every other budgeted path either defers the run (`run_papyrus_call`, `events::enqueue`) or leaves a *state-based* registration due (`OnUpdate`, distance polling). A hit is a one-shot marker, so this loses it; `RegisterForHitEvent` is itself one-shot, so the script stays armed for a hit that was already consumed by the engine. `docs/engine/scripting.md:909-918` describes deferral and OnUpdate coalescing only and does not list hits.
- **Evidence**: `if starts == 0 { continue; } starts -= 1; fired.push(...)` with the loop above reading `hits` (the marker snapshot).
- **Impact**: Under budget saturation (already a failing-script condition) FO4/Starfield hit handlers miss events silently.
- **Related**: #5764.
- **Suggested Fix**: When `starts == 0` build the `Effect::Interpret` and `budget::defer` it (removing the registration as for a fired one), as `reference_script_event_system` does for `OnHit`.

#### SCR-D2-2026-10-17-08: `ALIAS_OWNER_BASE` (0xFC00_0000) overlaps regular plugin slot 0xFC
- **Severity**: LOW
- **Dimension**: Fragment Dispatch & Locks
- **Location**: `crates/scripting/src/fragment/alias_scripts.rs:26-28,262-264`; slot map `crates/scripting/src/load_order.rs:84-97`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The doc says the alias-owner range "is no plugin's" and `is_alias_owner` tests only `owner >> 24 == 0xFC`. `global_slot_of` treats `0xFE` as light and `0xFD` as Starfield medium, so `Regular(0xFC)` (the 253rd full plugin) is a valid slot. A reference or quest FormID in that slot would be classified as an alias owner by `owners.rs`/`reference.rs` (`standing`, `vmad`, `alias_owner_*`) and resolve to `None`/stopped.
- **Evidence**: `owner >> 24 == ALIAS_OWNER_BASE >> 24` with no check against the loaded slot table.
- **Impact**: Only load orders with >= 253 full (non-ESL) plugins; scripts of that plugin lose events or run as aliases. Narrow.
- **Related**: none.
- **Suggested Fix**: Move the synthetic range above the real slot space (e.g. keep a separate `OwnerId` enum/flag bit) or assert at boot that no `Regular(0xFC)` slot is allocated.

### Dedup notes and cross-dimension pointers

- #5764 (budget-split residual), #5681 (continuation cap - SCR-D2-04 is a bypass of its fix, not a re-file of the Wait cap, which holds), #5791 (VM caps - the three panics above are not covered by it), #5646/#5647/#5682 (not Dim 2), #3949/#5071/#3493 (earlier nested-lock doc rot, all closed; -06 is new drift after #5468/#5800).
- Dropped after premise check: "script variables keyed by entity" - the lowered `SetReferenceVariable` -> `vm_state::ScriptVariables` (entity-keyed, saved) is the documented condition channel (`scripting.md:409-412`), not a regression of the three ledgers; the VM stores are owner-keyed. `quest_terminal_stage_system` claiming the journal with no `QuestDefinitionRegistry`: the registry is `init_resource`d at registration (`quest_stages.rs:1053`). `Value::Array` default-fill aliasing of struct elements: `default_for` yields `Value::None` for objects, so no aliasing. Collections (`addref`/`addarray` `contains` quadratic): bounded by distinct forms the script can name; not filed.
- `OnTimer`/`OnUpdate` coalescing and `ReferenceScriptIndex` staleness verified fine (re-validation at `reference.rs:147-154`).

#### Items for other dimensions
- Dim 4 (Attach & Reference Identity): `raise_reference_call` (`reference.rs:326-332`) `Box::leak`s the function name on every `script.call` console invocation (debug-server reachable, one leak per call); documented as "a handful of console calls" but the debug server is network-reachable.
- Dim 3 (Event Runtime): SCR-D2-05 overlaps the condition-evaluator side (`condition/eval/global.rs`).
- /audit-save: `FragmentExecutionQueue` growth (SCR-D2-04) is serialised into saves; no format change proposed.

# Dimension 3 Event Runtime — Markers, Timers, Conditions, Triggers

### Delta scope
- HEAD a071ed217; baseline 5d31fd637 (530 commits repo-wide). Dim-3 paths touched by ~35 commits, mostly bulk/refactor waves
  (#5802 first slice `bde3478db`, #5804 `83fce5388` evaluate_function split, #5860 `ea28a35b8`, #5866 `ac0abfbae`, #5872 `275fc103f`,
  M47.2 condition additions 6f6a9eb3d..44e4d948d, `f67d82b3c` GetQuestVariable, `a5e5346d0`, `bd85de5ea` dialogue unmodeled decline,
  `8daa24105` dialogue INFO reuse, `173f9e155` GetButtonPressed global).
- Read in full: `cleanup.rs`, `events.rs` (marker decls), `trigger.rs` (detection + gate), `recurring_update.rs`, `timer.rs`, `condition.rs`,
  `condition/{catalog,eval}.rs`, `papyrus_demo/quest_advance.rs`, `trap_hit_requests.rs`, `projectile_requests.rs`, `sound_requests.rs`,
  `havok_impulse.rs`, `activate_parents.rs`, `ownership.rs`; engine-side consumers `systems/cinematic.rs` (motion-type + havok impulse),
  `systems/projectiles.rs` / `scripted_sound.rs` heads, `combat.rs` / `combat_ai.rs` HitEvent producers, `fragment/systems.rs` (activation flush),
  `fragment/reference.rs::collect_deliveries`, `boot/schedule/{update,mod}.rs` stage order and the order guards.
- Marker inventory: every `impl Component` marker in `crates/scripting` + `byroredux` (30 markers) cross-checked against `event_cleanup_system`
  (Pattern A) or an owner-head drain (Pattern B); every `insert`/`query_mut` producer of ActivateEvent, HitEvent, OnTriggerEnterEvent,
  OnCellLoadEvent, OnInitEvent, OnDeathEvent, NativeActivation, Item/EquipmentEventBatch, Splash/Ripple, AnimationTextKeyEvents, OnUpdateEvent,
  MotionTypeChangeRequest, HavokImpulseRequest, EvaluatePackageRequest located.
- Condition catalog checked against xEdit `wbDefinitions{TES4,FO3,FNV,TES5,FO4}.pas` (fetched from TES5Edit/dev; scratch copies) and a raw CTDA
  census of Oblivion/Fallout3/FalloutNV/Skyrim/Fallout4 `.esm` masters (python, read-only, scratchpad only).

### Guards run
- `cargo test -q -p byroredux-scripting --lib -- cleanup:: condition:: trigger:: recurring_update:: quest_advance` : 109 passed, 0 failed.
- Orchestrator logs: `test_scripting.log` 942 lib tests ok; `test_sched.log` 21 schedule tests ok
  (incl. `hit_event_consumers_run_after_every_producer`, `trap_hit_flush_runs_after_the_script_systems_that_queue_into_it`,
  `transient_cleanup_is_the_last_late_exclusive`, `activation_flush_is_scheduled_before_every_activate_event_consumer`).
- All green - and none of the findings below is caught by them (SCR-D3-01..05 live in the gaps the guards do not model).

### Verified-clean items
- Marker lifetimes: all 30 markers are in a sanctioned pattern. Pattern A list `cleanup.rs:93-118`; Pattern B owners drain before any early return:
  `scene_playback_system` `scene/playback.rs:508-524`, `scene_package_system` `package.rs:635-641`, `scene_dialogue_system` `dialogue.rs:586-589`,
  `story_manager_dispatch_system` `story_manager.rs:697-701`, `collect_deliveries` OnDeathEvent `fragment/reference.rs:584-588`
  (`relay_engine_events` reads OnDeathEvent at `:851` *before* the drain, `:221`), `scripted_motion_type_system`/`scripted_havok_impulse_system`
  `systems/cinematic.rs:266-382` (the only early return after the snapshot is the `HavokImpulseRules` guard at `:349`, which the producing native
  pre-empts at `fragment/vm/natives/reference.rs:200`, so no strand).
- `PendingTrapHits` / `PendingProjectileLaunches` / `PendingSoundPlays` are Resources (not markers); consumers `mem::take` at the head
  (`projectiles.rs:129-133`, `scripted_sound.rs:21-25`, `trap_hit_requests.rs:42-45`). Two-phase discipline in the flush: resource guard dropped
  before `query_mut::<HitEvent>`, held hits re-queued after the query guard is released (`trap_hit_requests.rs:42-82`).
- HitEvent multi-producer (#4324/#5860/#5872): player strike inserts first (`combat.rs:280-286`, `combat_input_system` runs first), NPC strikes hold on an
  occupied slot (`combat_ai.rs:266-278`), projectile + ProcessTrapHit ride `PendingTrapHits` and hold (`trap_hit_requests.rs:56-60`).
  Producers vs `combat_damage_system`/`hit_event_system` verified in `update.rs:283-584`.
- OnTriggerEnterEvent producers (`trigger.rs:373-391`, `systems/cinematic.rs:1166-1182`) both merge into `triggerers`; no consumer reads `[0]` only
  (`reference.rs:622`, `quest_advance.rs:373`, `papyrus_provider/execute.rs:150`).
- `Item/EquipmentEventBatch` extend-or-insert (`equipment.rs:57-109`); `QuestStageAdvancedBatch` append (`quest_stages.rs:815-828`);
  `SplashEvent/RippleEvent` keep the stronger (`water.rs:520-548`); `AnimationTextKeyEvents` player/stack paths are exclusive per entity.
- Trigger edge-seed: `occupant_inside: None` seeds silently (`trigger.rs:195-207`); `contains` sphere/OBB math (`:62-81`); `trigger_detection_system`
  is an exclusive Update system (`update.rs:411`) so holding `TriggerOccupancyState` across `condition::evaluate` cannot deadlock; Phase-1
  `TriggerVolume` write guard is scoped and released before Phase 2 (`:403-422` ... `:370-395`).
- `timer_tick_system` / `recurring_update_tick_system`: collect under one `query_mut`, drop, then insert markers (`timer.rs:48-70`,
  `recurring_update.rs:150-178`); no fire on zero dt (counter starts at interval), overshoot fires once, no-burst.
- CTDA OR precedence `eval.rs:303-337` (`or_blocks` + `any` per block, AND across blocks, trailing-OR clamp, empty list true); guards present.
- CATALOG pin: `CATALOG`/`ROWS` = 48; module-doc table 48 rows + prose "48"; feature-matrix + npc-spawn docs pin the same (`condition.rs:112-205`).
  Plugin mirror `param1_is_form_id` (`condition.rs:527-562` in plugin) agrees with `first_param_form_id` for every catalogued index (exhaustive test
  `condition/tests.rs:1166`). `param2_is_form_id` (605/181/576/606/608) has no exhaustive twin but is correct today.
- Intrusive consumers that gate on `is_modeled`: dialogue INFO (`dialogue.rs:423,480`, new in bd85de5ea), Story Manager (`story_manager.rs:1076`),
  perk entry points (`perk_entry_points.rs:493`), ambient package install (`ai_package.rs:49,74`), ConditionForm (`condition_form.rs:76`).
- `quest_advance_system`: one signal per entity per frame (`signalled` set, `quest_advance.rs:361-376`), ActivateEvent beats OnTriggerEnter; no
  double delivery of the two signals. Fire-once variants park in `ReferenceScriptState`/`ReferenceEnableState` (`:513-546`).
- `QuestAliasReadinessGate` order: refresh (`update.rs:437`) -> readiness stage (`:441`) -> `scene_playback_system` (`:448`).
- Schedule order: activation flush (235) before every ActivateEvent consumer; trigger detection (411) -> quest_advance (419) -> reference scripts (521);
  `scripted_motion_type`/`scripted_havok_impulse` (559/561) after the script systems; `event_cleanup_system` last Late exclusive (`late.rs:829`).
- `activate_parent_relay_system` (525) queues through `PendingFragmentActivations` (flushed at 235 next frame): relayed children activate exactly once,
  one frame per chain hop - by design, not a stranded marker.

### Findings


#### SCR-D3-2026-10-17-01: Authored `GetActorValue` (fn 14) reads an actor-value *enum code* as an AVIF FormID on every pre-FO4 game
- **Severity**: HIGH
- **Dimension**: Event Runtime
- **Location**: `crates/scripting/src/condition/catalog.rs:282` (row), `crates/scripting/src/condition/eval.rs:367` (`actor_value_view` keyed lookup),
  `crates/plugin/src/esm/records/condition.rs:532` (fn 14 in `param1_is_form_id`)
- **Status**: NEW (the 2026-10-12 report noted the type mismatch in passing - `AUDIT_SCRIPTING_2026-10-12.md:148` - "not verified"; verified here)
- **Untrusted-Input**: No
- **Description**: xEdit types fn 14's `param_1` as `ptActorValue` = `wbActorValue` *enum* for TES4, FO3, FNV and TES5 (`wbDefinitionsTES4.pas:776`,
  `FO3.pas:2149`, `FNV.pas:2240`, TES5 `GetActorValue ptActorValue`); only FO4+ stores an AVIF FormID (`GetValue`, `wbActorValue := wbFormIDCkNoReach`).
  The catalog documents/handles it as "the global-space AVIF FormID" for all games, the plugin remaps it as a FormID, and `ActorValues` is keyed by
  AVIF FormID (FNV `Strength` 0x3E8, Skyrim `AVHealth` 0x3E8).
- **Evidence**: CTDA census of the vanilla masters: FalloutNV.esm fn 14 = 1 576 uses (INFO 1 264, IDLE 162, PACK 37, MESG 41, TERM 36, SPEL 35) with
  `param_1` in {0x2b (540), 0x20 (209), 0x28, 0x25, 0x1d, 0x1e, 0x19, 0x09 ...} - SPECIAL codes 5-11, skill/stat codes - while the FNV AVIF FormIDs
  are 0x3E8+ (Strength 0x3E8, Intelligence 0x3EC, Health 0x450); Fallout3.esm 958 uses; Oblivion.esm 120; Skyrim.esm 543 (INFO 318, PACK 169) with
  `param_1` in {68, 95, 17 (Speechcraft), 73, 74, ...} vs Skyrim AVIF FormIDs from 0x3E8. No translation exists anywhere between `parse_ctda`
  and `actor_value_view` (grep: no code->AVIF table; the only construction sites with real AVIF FormIDs are engine-built conditions
  `barter.rs:361`, `combat.rs:1138`, `encumbrance.rs:143`, `commands/gameplay.rs:400`). The unit test `condition/tests.rs:747` uses FormID stand-ins,
  so it cannot see this. Not verified by running the engine.
- **Impact**: every authored skill/SPECIAL/karma/stat gate on FO3/FNV/Skyrim dialogue, packages, quests and scenes evaluates the *absent-AV default
  0.0* (`actor_value_view` returns 0.0 for a missing key) instead of the stat, and `is_modeled` reports it as modeled so no fail-closed consumer
  declines it: `GetActorValue Speech >= 25` options never unlock, `== 0` / `< N` gates always pass. The plugin remap additionally rewrites the code
  into `masters[0]`'s global slot (identity today only because masters sit at slot 0).
- **Related**: `/audit-character` Dim 4 (ActorValues key space); #1663 (closed, implemented the FormID reading for FO4-style authoring); #2987.
- **Suggested Fix**: translate the enum code to the game's AVIF FormID once at the parser boundary (per-game `wbActorValue` table -> `index.actor_values`
  by EditorID) and drop fn 14 from `param1_is_form_id` for the enum games; until then make `is_modeled` return `false` for fn 14 when the loaded game
  is not Fallout 4 / Starfield so the fail-closed gates decline instead of reading 0.0.

#### SCR-D3-2026-10-17-02: The CTDA catalog is game-agnostic for six more colliding indices; `is_modeled` only special-cases fn 79
- **Severity**: HIGH
- **Dimension**: Event Runtime
- **Location**: `crates/scripting/src/condition/catalog.rs:296,298,299,300,303,340` (rows for 449, 533, 550, 555, 630, 403);
  `crates/scripting/src/condition/eval.rs:69-75` (`is_modeled`) and `:36-43` (the only per-game branch, fn 79)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: `ConditionFunction::from_index` maps an index to one function regardless of game; only `GetQuestVariable` (79) is gated by game
  (`uses_obscript_quest_variables`). xEdit (`wbDefinitions*.pas`) shows these catalog indices are a *different function* on other games:
  | idx | catalog (where valid) | other game(s) | evaluated as |
  |---|---|---|---|
  | 403 | `GetRelationshipRank` (Skyrim/FO4) | FO3/FNV `HasFriendDisposition` | GetRelationshipRank(actor 0) |
  | 449 | `HasPerk` (FO3/FNV) | Skyrim/FO4 `GetFactionRelation(ptActor)` | HasPerk(actor FormID) = 0 |
  | 533 | `GetXPForNextLevel` (FNV) | Skyrim/FO4 `GetInfamy` | ruleset XP |
  | 550 | `IsSceneActionComplete` (Skyrim/FO4) | FO3/FNV `IsGoreDisabled` | scene history |
  | 555 | `HasLoaded3D` (Skyrim/FO4) | FO3/FNV `GetSpellUsageNum` | GlobalTransform probe |
  | 630 | `GetVMScriptVariable` (Skyrim) | FO4 `GetCombatAudioDetection` (FO4's is 660) | VM script variable |
  (`FO4.pas:4087`, `FO3.pas:2324/2375/2376`, `TES5.pas:3310/3349`.)
- **Evidence**: vanilla CTDA census - Fallout3.esm fn 403 x8 (all INFO: 0x1d7ac, 0x1d7ad, 0x1fda2, 0x1fda3, 0x1fda5, 0x1fda6, 0x8726f, 0x1e374;
  comparators `== 1` / `== 0`, no parameter); Skyrim.esm fn 449 x21 (INFO 16, QUST 5); Fallout4.esm fn 449 x35 (QUST 29, INFO 3, ENCH 2, SCEN 1),
  fn 630 x25 (INFO). The evaluator computes a real-looking value for each (e.g. `get_relationship_rank` returns 0 for a null actor -> a
  `HasFriendDisposition == 0` FO3 line passes for everyone, the `== 1` twin never does), and `is_modeled` is `true` for them, so the
  fail-closed gates added by #5380/#5376/#5734/bd85de5ea accept the result.
- **Impact**: wrong dialogue variant selection on FO3, wrong quest-level INFO gating (`#5734`) on Skyrim/FO4 through fn 449, and a wrong VM
  variable read on FO4 INFOs (fn 630) - the exact class the fn-79 special case was added to stop. Escalated to HIGH under the skill rule (an
  intrusive consumer accepts a term the evaluator does not actually model for the loaded game).
- **Related**: `f67d82b3c` (fn 79 game gate - the precedent), #5380, #5752 (closed by bd85de5ea), `condition.rs` doc ("180/610 stay unmapped").
- **Suggested Fix**: give each `ROWS` row a game set (`valid_for`) next to the existing 79 branch and have both `evaluate_condition` and `is_modeled`
  treat a collision on the wrong game as `Unknown`; add a drift test that walks `CATALOG` against a checked-in per-game index-name table.

#### SCR-D3-2026-10-17-03: Reference/alias-script `OnHit` is delivered before the projectile and `ProcessTrapHit` hits exist
- **Severity**: MEDIUM
- **Dimension**: Event Runtime
- **Location**: `byroredux/src/boot/schedule/update.rs:519-523` (`reference_script_event_system`) vs `:569,577` (`projectile_flight_system`,
  `pending_trap_hit_flush_system`); consumer `crates/scripting/src/fragment/reference.rs:605` (`collect_deliveries`) and `:858` (`relay_engine_events`)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: `HitEvent` is a one-frame marker drained in Late. #5872/ee6468fd1 moved the queued hits (projectile flight, `ProcessTrapHit`) to land
  at `update.rs:577`, "after the script systems ... before every consumer", and `hit_event_consumers_run_after_every_producer` pins
  `combat_damage_system` and `hit_event_system` after every producer. But `reference_script_event_system` is itself a `HitEvent` *consumer* (it turns
  the marker into the Papyrus `OnHit(akAggressor, ..)` delivery, and `relay_engine_events` into remote `OnHit` relays) and sits at `:521`, before
  both producers; the same guard lists it only as a *producer*. A projectile or trap hit lands at `:577`, is consumed by damage/RegisterForHitEvent,
  and is drained by `event_cleanup_system` before the next frame's `:521` ever runs, so `OnHit` never reaches a reference/alias script for those
  hits. Melee/NPC strikes (`:283/:346`, earlier) are delivered.
- **Evidence**: stage order above; `HitEvent` landed only via `trap_hit_requests.rs:62-78` for the queued sources; no other reader of the queue.
- **Impact**: scripts whose `OnHit` reacts to dart-trap / `Weapon.Fire` / `ProcessTrapHit` damage on their reference never run (the damage itself
  does land). One event class silently lost; no state corruption.
- **Related**: #5872, #5860, ee6468fd1.
- **Suggested Fix**: either register a second `HitEvent`-reading delivery after `pending_trap_hit_flush_system` (or move `reference_script_event_system`'s
  OnHit collection there), and add `reference_script_event_system` to the consumer list of `hit_event_consumers_run_after_every_producer`.

#### SCR-D3-2026-10-17-04: `GetTriggerObjectCount` returns a wrong count (excludes the player) instead of declining
- **Severity**: MEDIUM
- **Dimension**: Event Runtime
- **Location**: `crates/scripting/src/trigger.rs:145-149` (`trigger_object_count`), `:229` (candidate set excludes the player);
  VM native `fragment/vm/natives/reference.rs:126-129`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The count is `|{(trigger, actor)}|` from `TriggerOccupancyState.inside`, whose actor set is `SceneAliasCandidate` entities
  **minus the player** (`*entity != player`, `:229`); the player's occupancy lives only in `TriggerVolume.occupant_inside`. The CK says "the number of
  objects currently inside this trigger volume" (any object). The native returns the number as a normal `Int` rather than stopping the run (the VM
  contract is "a native that cannot honour its values stops the run"). Secondary: when the candidate list is empty the occupancy pass is skipped
  entirely (`if !actors.is_empty()`, `:254`), so `inside` keeps stale pairs for actors that despawned and the count stays > 0 until another candidate
  exists.
- **Evidence**: `trigger.rs:229` filter, `:254-345` pass gated on `!actors.is_empty()`, `:651`/test `:687` only exercises actor pairs.
- **Impact**: a script gating on `GetTriggerObjectCount() == 0` / `> 0` (trap reset, "area is empty") reads 0 while the player stands in the volume
  and a stale non-zero after the last occupant unloads.
- **Related**: `3cb6056e0` (introduced), docs/engine/scripting.md:1272.
- **Suggested Fix**: add `occupant_inside == Some(true)` of the volume to the count, clear `inside`/`observed_*` when the candidate list is empty, or
  stop the VM run when the occupancy pass has not covered the player.

#### SCR-D3-2026-10-17-05: Scene / phase conditions trust the fail-open Unknown-function pass
- **Severity**: MEDIUM
- **Dimension**: Event Runtime (consumer lives in Dim 5)
- **Location**: `crates/scripting/src/scene/playback.rs:385,402` (`evaluate(&scene.conditions ..)`, `evaluate(&phase.start_conditions ..)`);
  same pattern at `quest_stages.rs:1011,1373`, `trigger.rs:541`, `byroredux/src/inventory.rs:1432`
- **Status**: NEW (the fail-closed `is_modeled` gate exists for the five consumers listed in Verified-clean; these were not converted)
- **Untrusted-Input**: No
- **Description**: A scene starts (Begin fragments, actor movement/dialogue) and each phase starts on the bare `evaluate` result; an uncatalogued term
  evaluates 0.0 and satisfies any `== 0`/`<=` comparison. The skill's rule is "a consumer that *acts* on a pass must decline on any `Unknown` term
  first". Skyrim.esm SCEN carries 301 unmodeled CTDA terms out of 2 791 (top: fn 289 x130, fn 18 x78, fn 249 x19, fn 214, 32, 45 ...); QUST 2 184 of
  11 427.
- **Evidence**: census above (python walk over `Skyrim.esm`, SCEN/QUST CTDA function indices minus `CATALOG`); call sites above carry no `is_modeled`.
- **Impact**: scenes whose authored gate cannot be read can start (or phases can advance) on the vacuous 0.0; quest log entries with
  Complete/Fail flags and objective-target highlighting have the same exposure. Bounded to terms outside the catalog.
- **Related**: #5380 (same class, Story Manager), bd85de5ea (dialogue). Scene ownership: Dim 5.
- **Suggested Fix**: route these through one `evaluate_modeled` helper that returns `false` when any term fails `is_modeled` (the same wrapper the SM gate uses).

#### SCR-D3-2026-10-17-06: `NativeActivation` is honoured by only some OnActivate consumers
- **Severity**: LOW
- **Dimension**: Event Runtime
- **Location**: `crates/scripting/src/events.rs:30-39` (contract) vs consumers `papyrus_demo/quest_advance.rs:363`, `papyrus_demo/mod.rs:298`,
  `papyrus_demo/mg07_door.rs:325`, `papyrus_provider/execute.rs:109`, `byroredux/src/extensions/systems.rs:570`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: CK `Activate(ref, true)`: "no OnActivate event will be sent to any scripts". Only the VM delivery (`fragment/reference.rs:602-614,830`)
  filters `NativeActivation`. The lowered quest-advance, rumble, MG07, Papyrus-provider and WASM extension `OnActivate` paths read the raw
  `ActivateEvent`.
- **Impact**: a script's native-only activation of an object that carries one of those handlers runs the handler anyway. Low frequency; no loop
  today (provider IR has no Activate op).
- **Suggested Fix**: share one `papyrus_activations(world)` accessor (ActivateEvent minus NativeActivation) used by all OnActivate-modelling consumers.

#### SCR-D3-2026-10-17-07: `quest_advance_system` double-fires for two triggerers on one volume in a frame
- **Severity**: LOW
- **Dimension**: Event Runtime
- **Location**: `crates/scripting/src/papyrus_demo/quest_advance.rs:370-376,389-437`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The per-entity `signalled` dedup stops ActivateEvent+OnTriggerEnter double delivery, but `triggerers` expands to one `(entity, triggerer)`
  pair each and every passing pair pushes its own `PendingAdvance`; each does `set_stage` (which journals a `QuestStageAdvanced`, `quest_stages.rs:349-362`).
  With an `Any` gate (or `BaseForm` plus a second matching actor) two actors entering in the same frame advance the stage twice - including past
  `onlyOnce`/`disableWhenDone`, whose disarm only applies after the loop.
- **Impact**: the stage fragment can run twice for a fire-once trigger (non-idempotent effects such as AddItem repeat). Needs two simultaneous
  entrants; PlayerOnly gates are unaffected.
- **Suggested Fix**: break after the first passing triggerer of an entity (or dedupe `PendingAdvance` by `(entity, quest, stage)`).

#### SCR-D3-2026-10-17-08: Marker-contract docs and pins drifted (cleanup table, catalog doc, stale count comment)
- **Severity**: LOW
- **Dimension**: Event Runtime
- **Location**: `cleanup.rs:36-46,163-232`; `condition/catalog.rs:78`; `condition/tests.rs:1183`; `condition/catalog.rs:19-21`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: (a) The Pattern-B table and `every_documented_pattern_b_marker_drains_in_its_named_consumer` omit `OnDeathEvent` (drained at
  `reference.rs:588`) and do not row-check the `HavokImpulseRequest`/`MotionTypeChangeRequest` drains the table does list; the pin also scans whole
  files via `include_str!` instead of `production_text`. (b) `catalog.rs:78` says "FO3 / FNV author no CTDA at 71"; the masters carry 2 450 (FO3) and
  3 535 (FNV) fn-71 uses (71 is `GetInFaction` on TES4/FO3/FNV too). (c) `tests.rs:1183` "24 catalog entries" (live: 48 + HasPerk's second index);
  `catalog.rs:19-21` still says new functions land as "a variant + a match arm in evaluate_function" (now a ROWS row + arm).
- **Impact**: comment/pin rot only.
- **Suggested Fix**: add the three rows to the table and the pin (scan `production_text`), correct the 71 sentence, refresh the two comments.

### Dedup notes and cross-dimension pointers

- #5647 (`StoryEvent` single slot, CLOC/KILL producers) - still open, not re-filed; `StoryEvent` is Pattern B and drains at the head (`story_manager.rs:701`).
- #5646 (GetRandomPercent 0..=100) - open, unchanged; `GetRandomPercent` row `deterministic: false` (`catalog.rs:314`) keeps #5645 intact.
- #5682 (INFO conditions double-evaluated) closed by `8daa24105`; #5752 closed by `bd85de5ea` - both fixes read correct for the catalogued/uncatalogued split
  but inherit SCR-D3-02 (collisions count as catalogued).
- #5790 (GetButtonPressed global) closed by `173f9e155`; touches `messages.rs` / `notifications.rs` / `obscript_quests.rs` (Dim 6), not re-audited here.
- #5608 (ScriptTimer/TimerExpired no producer) closed; `timer.rs` docs honest, unchanged.
- #4324 (HitEvent overwrite) closed; re-verified across the three producers + queue (clean). #5860/#5872 closed; fixes hold (only the OnHit-consumer ordering
  gap, SCR-D3-03, is new).
- No existing issue found for GetActorValue enum-vs-FormID, game-index collisions, OnHit ordering, GetTriggerObjectCount, scene fail-open
  (`gh issue list --search` on each keyword returned nothing relevant).

#### Items for other dimensions
- Dim 5 (Scene/Package/Cinematic): SCR-D3-05 call sites in `scene/playback.rs`; `Scene.Start` explicit path shares the same condition gate.
- `/audit-character` Dim 4: SCR-D3-01 co-owner (ActorValues key space vs authored CTDA enum codes; FO3/FNV 1.5k + 0.96k uses, Skyrim 543).
- Dim 7 / `/audit-papyrus`: Papyrus provider `OnActivate` ignores `NativeActivation` (SCR-D3-06).
- `/audit-save` / `/audit-ecs`: `EditorLocation` and `ExplicitEncounterZone` have `impl Component` but no production `world.register::<>` (they auto-register on first
  `insert`; readers use `query` and tolerate None, so no defect today - flag only for the registry-completeness scanner).
- `#5802` first-slice modules (`ownership.rs`, `crime_gold.rs`, `crime_faction.rs`) are Resources + saved rows, not markers; their save-row order sensitivity
  (`register_saved`) belongs to `/audit-save`.

# Dimension 4 Attach Path & Reference Identity

### Delta scope
- Baseline 5d31fd637 -> HEAD a071ed217. Scope files changed: `asset_provider/script.rs` (+572), `cell_loader/references/{attach,synth_child,mod,import}.rs`, `cell_loader/{spawn,unload}.rs`, `crates/scripting/src/fragment/{reference,alias_scripts}.rs` (both new, 880 + 320 lines), `crates/plugin/src/esm/records/index.rs` (+240). `cell_loader/exterior.rs` changed only for unrelated climate / LOD work; its persistent-stub loop is unchanged.
- Read in full or in the relevant window: `references/{mod.rs (load loop, actor arm, yield points), synth_child.rs (all of it), attach.rs (trigger volume, attach_script_for_refr, attach_vmad_scripts, dedup_vmad_scripts)}`, `asset_provider/script.rs` 1-356 (ScriptProvider, VmReferenceScriptChecks, ALIAS_CHECKS_PER_FRAME, pex_archive_path, legacy_script_principal) plus `populate_record_fragments` and `materialize_scene_actor_alias_stubs`, `spawn.rs` gates (reference_is_disabled / placement_is_disabled / scripted_lock_override and the Locked stamp), `unload.rs` unload_cell_inner, `exterior.rs` persistent stub loop, `created.rs`, `fragment/reference.rs` (ReferenceScriptIndex, raise_unload_events, collect_deliveries), `fragment/alias_scripts.rs` (QuestAliasScripts), `fragment/vm/checker.rs` entry points and recursion guards, `index.rs::base_record_script{,_instance}`, `commands/quest.rs`, `complete.rs` summary counters.
- Spawn paths enumerated: actor job (`mod.rs`), disabled-actor identity branch, trigger volume, LIGH-only, marker/fxlight, static-mesh miss, cached-NIF miss, static mesh (`spawn_placed_instances`), persistent-worldspace stubs (`exterior.rs`), scene-actor stubs (`script.rs`), created references (`created.rs`, which re-enters `load_references`).

### Guards run
- `cargo test -q -p byroredux --bin byroredux -- cell_loader::references reference_enable_gate`: 59 passed, 0 failed, 1 ignored (3004 filtered out), 6.55s.
- `grep -rn spawn_logical_quest_reference byroredux/src`: bare spawner is called only in `synth_child.rs` (405, 461, 500, 543, 745) and `references/mod.rs:676`. Each call is paired with `attach_quest_reference_script` in the same arm. Both outside callers (`exterior.rs:386`, `script.rs:1430`) use `_with_scripts`.
- Source pins are valid: `exterior.rs` pin uses `strip_test_modules`, `script.rs` pin uses `source_scan::production_text`, `synth_child_tests.rs:449` pins the (4 stamp, 5 bare-spawn) call-site count against the `is_primary_synth` gate count. No new vacuous guard found.

### Verified-clean items
- Silent-miss: no `unwrap`/`expect` on `.pex`-derived data in the attach path. `attach_vmad_scripts` (`attach.rs:727-846`) and `attach_vm_reference_script` (`script.rs:131-172`) use `let Some ... else continue/return`. `translate_pex_detailed_with_providers` runs under `catching_panics` (`translate/mod.rs:143`). The `decompile_pex_for_library` catch is at `translate/effects/inline.rs:23`. The only `.expect` in `script.rs` is `legacy_script_principal` (`script.rs:340`). It is fed by the `--scripts-bsa` CLI path, not by `.pex` bytes, and cannot fail: slug <= 64 ASCII alnum/'-', 15-byte prefix + 16-hex hash = 96 B < 128 limit, start char is 'l'.
- Checker recursion: `visiting` set (`checker.rs:402`), `chain()` capped at 16 (`:359`), `extends_quest` depth 16 (`:201`), `script_extends_alias` 16 iterations (`:1079`). An extends-cycle or mutual recursion in a hostile `.pex` cannot loop. Stack: `check_stmt` frame is 0xa8 B, `kind` 0xe8, `check_call_into` 0x1e8 (objdump of the debug test binary). The decompiler's MAX_REBUILD_DEPTH of 1024 gives well under 1 MB of checker stack, so the #3783 abort class does not apply to the checker.
- Archive precedence: `build_script_provider` pushes archives in flag order and `resolve_pex` is `find_map` over them, so first-listed wins. A present-but-corrupt `.pex` is named and then falls through to the next archive (`extract_or_warn`, #4658). `pex_archive_path` (`script.rs:290`) lowercases, folds `/` and `:` to `\`, and adds the `scripts\` and `.pex` affixes. Starfield / FO4 namespaced names (`Fragments:Quests:QF_*`, e5c0bac60) are pinned by `a_namespaced_script_name_maps_to_its_archive_folders`. The VM library key (`script.rs:103`) folds `:` the same way.
- Attach order is as documented: provider program and recognized archetype each attach, and the VM runs only when neither claimed the script (`attach.rs:812-841`). The VM verdict is cached per lowercase script name. A missing `ScriptProvider` returns before anything is cached (`script.rs:150`), so there is no poisoned `None` entry.
- `dedup_vmad_scripts` (`attach.rs:851`) collapses names case-insensitively with the REFR first, and merges base properties into a REFR copy that lacks them. The unit tests are green.
- `base_record_script_instance` (`index.rs:1228-1272`): ACTI, CONT, NPC_, CREA, items, `cells.statics` (STAT/MSTT/FURN/DOOR/LIGH/FLOR/IDLM/BNDS/ADDN/TACT), TERM. Typed maps take priority. Legacy `base_record_script` has the same families plus statics.
- Stub paths: both go through `spawn_logical_quest_reference_with_scripts` (`synth_child.rs:179`), which attaches once per `reference_form_id` via the `already_claimed` scan. Pins are production-text only (#5650). The documented residual (stub plus real cell both carry scripts) is unchanged and still untracked.
- Identity stamping: `stamp_quest_reference` (`synth_child.rs:53-124`) stamps `FormIdComponent`, `EditorLocation` (from the spawn Transform, before `restore_actor_snapshot` moves an evicted actor), `ExplicitOwner`, `ExplicitEncounterZone`, `SceneAliasCandidate`, `mark_scene_actor_bindings_dirty`, and `reapply_scripted_faction_changes`. It is called on every primary-synth path (actor root `mod.rs:762`, trigger, LIGH-only, fxlight, static `synth_child.rs:873`) and by the bare spawner. Stamp always precedes attach, so `ReferenceScriptIndex::refresh` sees the FormID.
- Enable/lock gates: `reference_is_disabled` is asked once per REFR ahead of every branch (`mod.rs:645`) and passed to the actor-disabled arm, the trigger volume (`synth_child.rs:362`), LIGH-only (`:431`), fxlight (`:~505`) and `spawn_placed_instances`. `scripted_lock_override` is consulted at the Locked stamp (`spawn.rs:1024`). Created references re-enter `load_references`, so the gates apply (`created.rs:~225`). The persistent-stub and scene-stub loops spawn no content, so they need no gate. #5523 SCOL expansion fallback hands `placement_disabled` to every child.
- XPRM: `trigger_volume_from_primitive` (`attach.rs:124-155`) uses z-up half-extents unscaled by 0.5, `[x, z, y]` with `.abs()`, scale baked in, shape 1 -> Box, 3 -> Sphere, else None, `occupant_inside: None`. `water_current_volume_from_ref` uses the same permutation. The trigger branch is gated on `is_primary_synth && !has_mesh && has_script`.
- Unload: `raise_unload_events` (`unload.rs:437`) runs after retention / player-protection filtering and before `release_entities_timed`, so it sees the pre-despawn victims. Deliveries are queued in `PendingUnloadDeliveries` and drained by the next `reference_script_event_system` run (`reference.rs:~290`). The only despawn that skips it is the whole-session purge (`unload.rs:126`), which is acceptable.
- `ReferenceScriptIndex::lookup` (`reference.rs:147-153`) rebuilds only when `attached != built`, then re-checks that the entity still has `PapyrusReferenceScripts` and that its FormID equals the key. Entity ids are never reused (#372), so the re-check is belt and braces. Nothing inserts `PapyrusReferenceScripts` outside `attach_reference_script`.
- `ALIAS_CHECKS_PER_FRAME` = 4 (`script.rs:187`): only running quests, `.take(4)` after the `is_running` filter, and a quest with no runnable scripts is `set_checked(empty)` so it leaves the queue. `VmAliasScriptChecks` is initialized at `script.rs:1064`, so its cache write at `:248` is live. No lock is held across a second mutation (read guards are dropped before the writes).
- `M47.2 scripts:` counters are wired: `scripts_recognized` is incremented in `attach_quest_reference_script` and in the trigger and static arms, `trigger_volumes` in the trigger arm, and both are printed at `complete.rs:186-196`. The exterior stub loop reports its own attach count (`exterior.rs:~410`). `m47-triggers.sh` greps the exact format string.
- `commands/quest.rs`: `quest.effects`, `quest.show`, `quest.aliases`, `scene.show`, `dialogue.status` are read-only (`quest.show` is tested "without refreshing aliases"). The mutators are the explicit `quest.start/stop/setstage` (via `apply_control_effect`), `terminal.select`, `perk.run`, `dialogue.forcegreet` and `sm.event`.
- `populate_record_fragments` temporarily removes `ScriptProvider` from the world (`script.rs:~700`). It is single-threaded under `&mut World`, so no attach can observe the gap.

### Findings


#### SCR-D4-2026-10-17-01: Attach emits `OnCellLoadEvent` for 3D-less identity-only entities, so the VM delivers OnLoad / OnCellAttach / OnCellLoad to them
- **Severity**: MEDIUM
- **Dimension**: Attach & Reference Identity
- **Location**: `byroredux/src/cell_loader/references/attach.rs:158-185` (unconditional marker), `crates/scripting/src/fragment/reference.rs:615-649` (marker -> `OnInit/OnCellAttach/OnLoad/OnCellLoad`)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: `attach_script_for_refr` inserts `OnCellLoadEvent` on every entity where anything attached, including VM reference scripts. It does not distinguish entities that have no 3D or no loaded cell. Such entities are: (a) the `placement_disabled` actor / trigger / light identity branches (#4326/#4327 deliberately keep scripts but no body), (b) persistent-worldspace logical-actor stubs and `RemoteSceneActorStub` scene stubs (home cell not loaded), (c) marker-model and missing-mesh fallbacks that spawn `spawn_logical_quest_reference`. `collect_deliveries` turns the marker into `OnInit, OnCellAttach, OnLoad, OnCellLoad` for any attached script that handles them, with no 3D or cell-loaded check.
- **Evidence**: CK `OnLoad - ObjectReference`: "Event called when the object's 3d is loaded... This event is only sent when an object's 3d is loaded." `OnCellLoad`: "every object's 3d in this object's parent cell loaded". The scene-stub materializer exists precisely because the actor's home cell is not loaded (`script.rs:1337-1347`), yet `spawn_logical_quest_reference_with_scripts` -> `attach_script_for_refr` fires the load events at stub creation. Pre-VM this was harmless because the recognized archetypes ignore `OnCellLoadEvent`. The VM now runs arbitrary `OnLoad` bodies.
- **Impact**: A quest actor's `Event OnLoad()` / `OnCellAttach()` body (state setup, `Enable`/`Disable`, `MoveTo`, `EvaluatePackage`, `SetStage`) runs while the actor is a stub or disabled, and runs again when the real cell streams in. This is the wrong-state-without-fallback class the audit treats as worse than inert. Not demonstrated on vanilla data; it needs a `refr_script_coverage` run or an `m47-triggers`-style smoke on a stub actor with an `OnLoad` handler.
- **Related**: #4112, #4331, #4326, #4327 (identity-only paths kept scripts by design), the documented stub/real double-attach residual.
- **Suggested Fix**: Emit `OnCellLoadEvent` only from paths that spawn real 3D into a loaded cell (the static-mesh and actor-complete arms). Pass a `has_3d` flag into `attach_script_for_refr`, or insert the marker at the call sites. Keep identity-only entities on `OnInit` only.

#### SCR-D4-2026-10-17-02: VM / recognized scripts attached to non-primary SCOL/PKIN children have no FormID, so the VM can never deliver to them
- **Severity**: LOW
- **Dimension**: Attach & Reference Identity
- **Location**: `byroredux/src/cell_loader/references/synth_child.rs:437-475` (LIGH-only / fxlight non-primary), `:933-945` (static non-primary: `placement_form_id_pair` is `is_primary_synth.then_some(..)`), `crates/scripting/src/fragment/reference.rs:119-153,607-612` (index and delivery both need `entity_global_form_id`)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: Per #3016 each synth child attaches its own base-record SCRI/VMAD, but `stamp_quest_reference` runs only for the primary child, so a non-primary child's root carries no `FormIdComponent`. `ReferenceScriptIndex::refresh` skips such an entity, and `collect_deliveries` logs "skipped: the reference has no FormID". `attach_vm_reference_script` still counts it as attached and `attach_script_for_refr` emits `OnCellLoadEvent`, so `scripts_recognized` is inflated while the script is inert.
- **Impact**: A scripted base object that is a secondary content of a Pack-in or SCOL (for example a scripted activator inside a FO4 PKIN) gets a VM instance that never runs. This is silent and only affects load-time telemetry accuracy. Whether the engine gives PKIN children their own reference identity is unverified.
- **Related**: #3016, #2026.
- **Suggested Fix**: Either document that secondary children's VM scripts are intentionally inert and skip the VM attach for them, or give them a synthetic per-child identity as the created-reference path does.

#### SCR-D4-2026-10-17-03: `.pex` is re-extracted, parsed, preflighted and decompiled for every scripted REFR; only the VM verdict is cached
- **Severity**: LOW
- **Dimension**: Attach & Reference Identity
- **Location**: `byroredux/src/cell_loader/references/attach.rs:765-790` (`resolve_pex` + `translate_pex_detailed_with_providers` per REFR per script), `byroredux/src/asset_provider/script.rs:131-172` (cache only after translate)
- **Status**: NEW
- **Untrusted-Input**: Yes (the work is proportional to attacker-supplied script size)
- **Description**: `VmReferenceScriptChecks` caches the checker verdict per script name, but each REFR carrying the same script still pays archive extraction (zlib / LZ4), `byroredux_pex::parse`, the compatibility preflight, `decompile_script` and the provider lowering before the cache is consulted. The recognizer result depends on per-REFR properties, but the decompiled AST does not.
- **Evidence**: No cache in `resolve_pex` or `translate_pex_detailed_with_providers`. A cell with N REFRs of one script does N decompiles. This is unmeasured; #3938 shows decompile cost on this path was a real hotspot earlier.
- **Impact**: Cell-load CPU scales with scripted-REFR count instead of unique-script count. Modded cells with thousands of scripted REFRs of one script stall the budgeted loader.
- **Suggested Fix**: Cache `(script name -> Arc<ast::Script> + PexCompatibility)` next to `VmReferenceScriptChecks` and run `lower_provider_program` / `translate_script` against the cached AST. Measure first with the `m47-triggers.sh` timing.

#### SCR-D4-2026-10-17-04: `check_reference_script` / `check_alias_script` run outside the panic net the rest of the `.pex` path uses
- **Severity**: LOW
- **Dimension**: Attach & Reference Identity
- **Location**: `byroredux/src/asset_provider/script.rs:151-163` (reference), `:246-258` (alias, per frame), `crates/scripting/src/translate/mod.rs:338-380` (net-coverage pin lists only translate/populate entry points)
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: The 3948/1816 contract wraps every untrusted-derived stage in `catching_panics`, and a source pin enforces it for the translate and populate entry points. The M47.2 checkers consume the decompiler's AST and are called from the cell-load attach path and from a per-frame system (`check_alias_scripts_dispatch`), with no net and no pin. A panic there would abort cell load or the frame loop instead of declining.
- **Evidence**: A grep of `checker.rs` and `script_model.rs` found no `unwrap`/`expect`/`unreachable!`/`panic!`, and the two index expressions (`owners[0]`, `arg_kinds[1]`) are guarded. This is hardening, not a demonstrated panic. A stack overflow is uncatchable regardless, but the frame sizes above show it is not reachable.
- **Impact**: Defense-in-depth gap on mod-supplied bytes.
- **Related**: #3948, #1816, #3287.
- **Suggested Fix**: Wrap the two `check_*` calls in `catching_panics("check_reference_script", ..)` and add the entry points to the pin in `translate/mod.rs`.

#### SCR-D4-2026-10-17-05: `check_running_quest_alias_scripts` bounds quests per frame, not scripts, so one quest can decompile an unbounded number of `.pex` in a frame
- **Severity**: LOW
- **Dimension**: Attach & Reference Identity
- **Location**: `byroredux/src/asset_provider/script.rs:187-286` (`ALIAS_CHECKS_PER_FRAME` = 4 applies to quests; the inner loop at `:225` decompiles every alias script of each)
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: The comment justifies the cap as avoiding "a second of load", but the unit is the quest. A quest whose VMAD alias section lists many distinct scripts (a mod quest, or a vanilla quest with dozens) triggers one `extract_or_warn` + decompile + check per distinct name in a single frame. The verdict cache only helps repeated names.
- **Impact**: A frame hitch when such a quest starts. It is bounded by the VMAD size and has no persistent effect.
- **Suggested Fix**: Budget by script count (for example 8 distinct uncached scripts per frame) and leave the quest unchecked until the next frame when the budget is hit.

### Dedup notes and cross-dimension pointers

- Stub scripts never attached: #4112 / #4331 (closed). The fix holds: both stub paths use `_with_scripts` and the pins scan production text (#5650).
- Disabled actor / trigger / light still spawning content: #4326 / #4327 / #4813 / #3278 (closed). The gate is consulted ahead of every branch and guarded by `reference_enable_gate_tests.rs`.
- Scripted-lock re-stamp: #4136 (closed), holds.
- Outer-REFR VMAD replicated per child: #2026, base-record per-child policy #3016 (closed), holds.
- Stub / real double-attach: still documented in the `spawn_logical_quest_reference_with_scripts` doc; no open issue owns it (the doc points at #2664, which closed on the transform half). Not re-filed.
- `ScriptedFactionChanges` not in `PRE_RELOAD_RESOURCES`: Existing #5919 (OPEN, save side). `reapply_scripted_faction_changes` stamping itself is correct.
- `OnCellLoadEvent` drain: #1736 (closed, stays drained by cleanup).
- `StoryEventAliasFill` liveness: #5419 (closed). `ReferenceScriptIndex` has the equivalent re-check.
- Candidates dropped after premise check:
  - an NPC job returning `root: None` leaves no identity (`UnitOutcome::Complete(None)` is only the wrong-game-variant defensive arm; a missing skeleton still returns the placement root);
  - checker stack overflow on a 1024-deep `.pex` (frame sizes measured, about 0.3 MB);
  - `ReferenceScriptIndex` stale on a late `FormIdComponent` (stamp always precedes attach);
  - `legacy_script_principal().expect` (grammar and length proven, input is a CLI path).
- Doc note for the skill text only: `references/mod.rs:676` also calls the bare spawner (paired with attach); the skill says all non-`synth_child.rs` callers use `_with_scripts`. Also `commands/quest.rs` has four explicit mutators beyond start/stage (`terminal.select`, `perk.run`, `dialogue.forcegreet`, `sm.event`).

#### Items for other dimensions
- Dim 3 (event runtime): `PendingUnloadDeliveries` drains on the next `reference_script_event_system` after the entity is despawned. VM natives that resolve `Self` through `ReferenceScriptIndex` (`interp.rs:886`) decline for `OnUnload` bodies. Check that this is the documented behavior.
- Dim 2 (fragment dispatch / locks): `ReferenceScriptIndex::refresh` holds the `PapyrusReferenceScripts` read guard while `entity_global_form_id` takes the `FormIdComponent` and `FormIdPool` read locks. Confirm the scheduler orders it against any writer of those.
- `/audit-papyrus`: `check_*` and the interpreter rely on the decompiler's MAX_REBUILD_DEPTH of 1024 for AST depth; the frame measurements above can feed that dimension.

# Dimension 5 Scene / Package / Dialogue / Cinematic Playback

### Delta scope
- HEAD a071ed217 vs baseline 5d31fd637. 66 commits touch the Dim 5 paths (scene/*, package.rs, dialogue.rs, cinematic.rs, story_manager.rs, location.rs(+location/), animation_graph.rs(+animation_modifiers.rs), systems/{cinematic,npc_dialogue,forcegreet,story_events}.rs, cell_loader/unload.rs, asset_provider/{object_graph,actor_graph,animation}.rs).
- Net diff in crates/scripting Dim 5 files: quest_alias.rs +459 (location aliases, Created Object At/In, ForceRefTo/Clear overrides), story_manager.rs +961 (script-raised events, #5643 linear tree build + capped explicit stack, #5406 phase B, 35f1cf051 status snapshot), dialogue.rs +342 (#5734 quest gates, bd85de5ea #5752), location.rs new (375), animation_graph.rs new since baseline (generator tree / modifiers / lazy clips, 49529db7c, 6db3805a5, 08e4d2ec5 #5892).
- Engine side read in full: forcegreet.rs, story_events.rs, object_graph.rs, actor_graph.rs, cinematic.rs 1-845 + 846-1180 (router), npc_dialogue.rs 100-945, unload.rs retention (15-135, 224-262); animation.rs `convert_hkx_clip` head. Commits examined by diff: 8daa24105 (#5682), bd85de5ea (#5752), ca3caf411 (#5867), 001de8741 (#5861/#5864/#5870), 35f1cf051, b79f0d2eb, 5ea89f3be, cfabdfd6a.
- hkx decode caps cross-checked (crates/hkx/src/behavior.rs: MAX_GENERATOR_DEPTH 32, MAX_GRAPH_STEPS 1M, per-list 1024, visited-set cycle guard; animation.rs MAX_TRANSFORM_SAMPLES 1M) only as far as they bound what the Dim 5 runtime traverses; decode itself is /audit-parsers.

### Guards run
- `cargo test -q -p byroredux-scripting -- scene:: package:: dialogue:: cinematic:: story_manager:: animation_graph location::` -> 129 passed, 0 failed, 3 ignored (the bare 5-filter command from the skill matches 0 tests in the integration targets and the lib count is in this run).
- `BYRO_LOCK_ORDER_CHECK=1 cargo test -q -p byroredux-scripting --lib -- story_manager::` -> 26 passed. NOTE this is blind to SCR-D5-2026-10-17-02: the test worlds (`setup_world()` = `crate::register`) never install `StoryManagerNodeState` (only `install_story_manager` does), so neither edge of the cycle is driven.
- `cargo test -q -p byroredux --bin byroredux -- cinematic npc_dialogue forcegreet story_dispatch_and_forcegreet` -> 60 passed (log: /tmp/audit/scripting/dim5_bin_tests.log).
- Orchestrator full runs (test_scripting.log 942 passed, test_sched.log 21 passed) green.

### Verified-clean items
- Pattern-B drains are at the head of their owners, before any early return: `scene_playback_system` scene/playback.rs:508-524; `scene_dialogue_system` dialogue.rs:585-590; `scene_package_system` package.rs:635-641 (incl. EvaluatePackageRequest after `ambient_ai_package_system` per update.rs:456 ordering); `story_manager_dispatch_system` story_manager.rs:697-707 (marker drain + `PendingScriptStoryEvents` append before the `events.is_empty()` return); `scripted_kill_system` combat.rs:1215 (`mem::take` first); `scripted_motion_type_system` cinematic.rs:268-317 (tail-drains exactly the snapshot; early returns strand nothing).
- Story Manager bounds: sibling-group build is O(g log g) (`order_sibling_group` heap + linked list, story_manager.rs:~500-600; cycle fallback appends lowest unplaced), walk is an explicit stack capped at `MAX_WALK_DEPTH`=64 (walk_siblings ~961-1105), `visited` guards re-entry, choose-one / consume early-exits preserved in the frame rewrite (post-subtree checks run before `consumed`). `StoryManagerRng` write guard is taken for the shuffle alone (enter_group), `PendingScriptStoryEvents` is a queue so same-frame script events all dispatch.
- Dialogue: #5682 fix holds inside one operation (top_level_menu/linked_menu carry `&InfoRecord`; `select_on_topic` takes the INFO; test `selected_info_is_not_re_evaluated_after_menu_filtering`); #5752 fix holds (select_info declines any INFO with `!is_modeled` condition, dialogue.rs:~473-483); the `DialogueQuestConditions` Arc is cloned out and the guard dropped before `evaluate` (dialogue.rs:~410-420); scene dialogue with no eligible INFO completes the action rather than hanging the scene (dialogue.rs:~650-665).
- #5867: partner-gone check runs first in `npc_dialogue_goodbye_close_check` (npc_dialogue.rs:593-609), before the `player_can_act` early return in the selection system, uses the same `npc_refuses_dialogue`; `begin_barter` applies the same refusal.
- Force-greet hold/consume rules intact (forcegreet.rs:127-135, 205-214, 232-263): hold while talking to another NPC, consume when already talking to the greeter, second same-tick greeter re-reads the live partner (`open_conversation_partner` per open), dead player holds all, failed open consumes, `PhysicsWorld` never held across other acquisitions (cached_walk_step scopes it), walk writes `Transform`.
- `raise_hello_story_event` / `raise_kill_story_event` resolve the location before the `StoryEvent` write guard (story_events.rs:144, 179).
- Lock nesting: `scene_trigger_actor_approach_system_inner` drops the `SceneRegistry` guard before any quest resource (cinematic.rs:974); `scene_playback_system` holds `SceneRegistry` only inside the definitions block (playback.rs:583-593).
- Behavior-graph step loops: `step` caps handled triggers at 1024/frame and skips missed periodic occurrences (animation_graph.rs:1038-1070, 961-1005); `run_events` caps chains at 32 (:933); `seconds_until` is 64 iterations (:1021); `rearm`/`skip_missed` handle NaN/inf/rounding periods (period=inf drops the trigger with a once-only warning). Tree recursion (`eval_machine`/`eval_generator`/`dispatch`/`MachineRun::enter`) is bounded by the decoder's depth 32 + MAX_MACHINE_NESTING and cycle detection; modifier lists are capped at 1024 per list (hkx behavior.rs:1445). Tests `a_huge_playback_speed_cannot_hang_the_step`, `a_loop_the_graph_clock_cannot_resolve_does_not_hang_the_step`, `a_long_stall_skips_the_missed_loops_and_keeps_the_phase` pass.
- Death teardown (#5861/#5870/#5864): `reconcile_dead_actor` removes `ObjectAnimationGraph`/`GraphLocomotion`/`ConsoleWalk`, reports the `-> None` package transition, `populate_actor_graphs` skips `Dead` (actor_graph.rs:163); `retire_active_packages` at unload (unload.rs:~437); all live death producers (combat, drowning water.rs:77, SDK batch commands.rs:564, `scripted_kill_system`) go through `queue_dead_actor_reconciliation` -> `reconcile_dead_actor` -> `note_actor_death`.
- Cinematic retention / re-adoption unchanged in substance since baseline: `cinematic_retained_entities` + `strip_retained_cell_root` scoped to victims, `release_finished_tethers` / `retry_cinematic_readoption` parentless-only queue, player subtree never queued, `purge_cinematic_retention_state` closure release; `collect_subtree` (#5513/#5713) replaces the hand loops.
- Alias pre-filters: `candidate_matches_fill` and `CandidateIndex::fill_positions` arms mirror each other (quest_alias.rs:330-345 vs 411-458); `subject_requirement` folds only deterministic, entity-independent members and shares `run_on_identity` with the evaluator (condition/requirement.rs:46-90, eval.rs:489-490; `GetRandomPercent` is the sole non-deterministic row, catalog.rs ROWS); `RunOn::CombatTarget/LinkedReference/Target` resolve from ctx fields, never the subject, so folding them is exact. The `reads_run_on_entity: false` rows (GetStage/GetStageDone/GetQuestVariable/IsHardcore/IsSceneActionComplete/GetVM*/GetQuestRunning/GetGlobalValue/GetRandomPercent/LocAliasIsLocation/GetEventData/GetLocationCleared/GetLocationAliasCleared/GetKeywordDataFor{Location,Alias}) each match their evaluator arm.
- `LocationTable::lineage` is bounded by table size (cyclic PNAM chain terminates, test `a_cyclic_parent_chain_terminates`).
- `ObjectGraphCache`/`ActorGraphCache` cache the failure once; `LazyClips::fail` stops re-asking for a missing clip; a clip that fails to load costs only its states (object_graph.rs:48-91).

### Findings


#### SCR-D5-2026-10-17-01: Reference-alias fills read last refresh's location-alias bindings, so a reference alias gated on a location alias of the same quest lags (or never fills) on the quest-start refresh
- **Severity**: MEDIUM
- **Dimension**: Scene/Package/Cinematic
- **Location**: `crates/scripting/src/scene/quest_alias.rs:1138-1144` (per-candidate `eligible` ctx), `:1175-1181` (`subject_requirement` ctx), `:1238-1241` (location bindings committed only at the end), vs `:1322-1329` (the location fill does attach them); `crates/scripting/src/condition/context.rs:166-178`; `condition/eval.rs:198-225, 301-305`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: M47.2 location aliases fill inside `refresh_scene_actor_bindings` "in list order with the reference aliases, so either kind can depend on one above it" (comment at ~:925). The location-to-location and location-to-reference-alias directions are handled (`fill_location_alias` attaches `with_pending_alias_bindings(resolved)` and `with_pending_location_bindings(locations)`). The inverse direction is not: when a *reference* alias's match conditions use `GetInCurrentLocAlias`, `GetIsEditorLocAlias`, `LocAliasIsLocation`, `GetLocationAliasCleared` or `GetKeywordDataForAlias` (all read `ctx.alias_location`), the contexts built for the candidate scan and for `subject_requirement` carry only `.with_pending_alias_bindings(&resolved)`. `alias_location` therefore falls through to the committed `LocationAliasBindings`, which `refresh` replaces only after the whole loop (`.replace(locations)` :1238-1241). This is exactly the #2671 "fill lagged a refresh behind its own dependency" class, for locations. The canonical radiant shape (location alias "Find Matching Location" + reference aliases "in that location" conditioned on `GetInCurrentLocAlias`) evaluates the reference aliases against an empty table on the refresh that starts the quest. Nothing re-dirties `SceneActorBindings` afterwards (bindings.dirty is cleared at :1264), so the alias stays unbound until an unrelated cell load / stage change marks it dirty. A sibling lag exists for an `ExternalAlias` location alias whose source quest has a higher FormID than the consumer and starts in the same pass (`locations.get` reads the map before the source's alias is filled; quests are processed in FormID order, and only reference-alias externals get the fixed-point loop at :1196-1214).
- **Evidence**: no test pins it (`rg "InCurrentLocAlias|360" scene/quest_alias_tests.rs` -> none; `location_aliases_fill_by_each_authored_mechanism_and_hold_until_stop` exercises location-only chains).
- **Impact**: Story-Manager-started quests start with their location-scoped reference aliases (actors that receive injected packages/factions/inventory) unbound for an unbounded time; non-optional aliases are not enforced at start (only FromEvent slots are, `event_aliases_fill`), so the quest runs half-populated.
- **Related**: #2671 (same class, reference aliases), M47.2 6f6a9eb3d.
- **Suggested Fix**: attach `with_pending_location_bindings(&locations)` to the reference-alias scan and `subject_requirement` contexts (the map is complete for aliases above in list order at that point), and run the external-location fill in the same fixed-point loop as the reference externals.

#### SCR-D5-2026-10-17-02: Story Manager dispatcher records a `StoryManagerNodeState <-> QuestStageState` lock-order cycle since 35f1cf051
- **Severity**: MEDIUM
- **Dimension**: Scene/Package/Cinematic
- **Location**: `crates/scripting/src/story_manager.rs:715-735` (phase A: `node_state` read guard held across `dispatch_story_event` -> `evaluate`), `:751-790` (phase B: `QuestStageState` write guard held while `StoryManagerNodeState` is taken for write); comment at :748-750 claims "registry -> stages -> node_state" order for both phases
- **Status**: NEW (introduced by 35f1cf051, after the baseline)
- **Untrusted-Input**: No
- **Description**: 35f1cf051 replaced phase A's `QuestStageState` read guard with a `statuses()` snapshot (to stop the nested-read warning). Before it, phase A held `stages` -> `node_state`, consistent with phase B (`stages` -> `node_state`). Now the stages guard is a statement-scoped temporary, so during the walk phase A holds `{registry, aliases, node_state}` while every `evaluate` of a quest/stage/running gate (GetStage, GetStageDone, GetQuestRunning, ... via `try_resource::<QuestStageState>`) acquires `QuestStageState` read: edge `StoryManagerNodeState -> QuestStageState`. Phase B then holds `QuestStageState` write and acquires `StoryManagerNodeState` write (:784): edge `QuestStageState -> StoryManagerNodeState`. Both edges are observable in one production run (the first radiant start after a gated node evaluates). The detector records process-wide edges "even when the hold periods are sequential" (lock_tracker.rs:272-300), so `BYRO_LOCK_ORDER_CHECK=1` on a real SM start closes the cycle and aborts in a debug build. It is not a live deadlock today (both phases run inside one exclusive Update system), which is why this is MEDIUM; the CI lane cannot see it because no unit test installs `StoryManagerNodeState` (guard-run note above).
- **Impact**: a debug/lock-check engine run panics on the first SM quest start with a gated node and installed node state; the cycle becomes a real ABBA the moment any other (non-exclusive) system reads/writes either resource.
- **Related**: #5628, #5366 (earlier SM lock fixes), 35f1cf051.
- **Suggested Fix**: snapshot what phase A needs from `node_state` (clone or release before `dispatch_story_event`, as is already done for the stages) or take `node_state` for phase B before `stages`; add an SM dispatch test that installs `StoryManagerNodeState` and a `GetStage`-gated quest node, run under the lock-order lane.

#### SCR-D5-2026-10-17-03: A Location Alias Reference fill ignores the bound location alias (and keyword), binding any candidate of the ref type anywhere
- **Severity**: MEDIUM
- **Dimension**: Scene/Package/Cinematic
- **Location**: `crates/scripting/src/scene/quest_alias.rs:330-345` (`candidate_matches_fill`), `:424-427` (`fill_positions` bucket by ref type only); same shape in `byroredux/src/asset_provider/script.rs:~1387` (`matches_fill`)
- **Status**: NEW (the m47-3 design table lists the row as "partial", but its stated blocker, an LCTN model, shipped in M47.2)
- **Untrusted-Input**: No
- **Description**: For a *reference* alias the CK fill is "pick a Location Alias from this quest ... and a Location Ref Type. When the quest starts, the Story Manager will attempt to find a matching loc ref type **from that location**" (ck-uesp-wiki `Quest Alias Tab.wiki` line 61). The recognizer keeps only `ref_type` and never consults `alias_id` or `keyword`: any loaded candidate whose own XLRT (or script-set `SetLocRefType`) tags that type matches, from any location. Now that location aliases bind (`LocationAliasBindings`) and `LocationTable::refs_of_type(location, type)` exists, the constraint is decidable; the fill still approximates, so with several locations loaded (exterior streaming: many Boss/Marker-tagged refs) it silently binds the nearest-ordered wrong actor, which then receives the alias's injected factions/packages/inventory. Per the skill's own rule a partial lowering is worse than declining.
- **Evidence**: `AliasFillType::LocationAliasReference { alias_id, keyword, ref_type }` is destructured with `..` in both matchers; only `fill_location_alias` (location aliases) reads `alias_id`/`keyword` (:1338-1349).
- **Impact**: wrong-actor alias bindings on any radiant/location quest using this fill; state corruption via alias injection. Reach is bounded by how many tagged refs are resident at once.
- **Related**: m47-3-quest-alias-design.md row 208 ("partial"), #2661 precedent (collection alias half-working instead of declining).
- **Suggested Fix**: when `alias_id` names a location alias, require the candidate's reference to be in `refs_of_type(bound_location, ref_type)` (or its cell location within it, honouring `keyword`), and decline (leave unbound) while that location alias is unbound.

#### SCR-D5-2026-10-17-04: `resolve_wanted` restarts every actor sharing a behavior graph, not only the graphs that were waiting on the clip
- **Severity**: MEDIUM
- **Dimension**: Scene/Package/Cinematic
- **Location**: `byroredux/src/asset_provider/actor_graph.rs:322-335` (`.filter(|(_, graph)| std::ptr::eq(graph.graph(), &*job.loaded))` then `graph.start()` + `apply_playback_for`); `crates/scripting/src/animation_graph.rs:830-850` (`start`)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The doc (module header, scripting.md "restarts the graph") says the pass restarts "the graphs that were waiting" on a clip. The filter selects *all* `ObjectAnimationGraph`s whose `LoadedGraph` is the job's shared graph (`ActorGraphCache` shares one `LoadedGraph` per `idle_key`, so every humanoid with the same behavior/folders). `start()` clears the schedule, re-schedules triggers from now and returns the playback of the *current* configuration with `start_time`/pose phase 0; `apply_playback_for` then replaces each actor's `AnimationPlayer`/`AnimationStack` clip at local time 0. Every first use of a new clip by any one actor (hundreds of lazily-resolved clips per FO4 graph, resolved during gameplay) therefore snaps all other same-graph actors' current animation back to its first frame (a mid-swing attack replays; a looping idle pops; a synced locomotion blend resets its phase) and re-bases their completion triggers.
- **Impact**: visible animation pops across a cell's NPCs on each clip first-use, until the graph's clip set is warm. Self-healing, visual only.
- **Related**: M47.2 3bb6806b5 (PlayAnimation reaches FO4 actors), #5861.
- **Suggested Fix**: restart only graphs that actually asked (record a per-graph `wanted` flag in `LazyClips::lookup`, or restart a graph only if its current `terms()` was empty before the clip was provided) and leave the rest running.

#### SCR-D5-2026-10-17-05: A FromEvent alias fill ignores the Reserves / Allow Reserved rule
- **Severity**: LOW
- **Dimension**: Scene/Package/Cinematic
- **Location**: `crates/scripting/src/scene/quest_alias.rs:1017-1090` (FromEvent branch checks `used`/Allow Reuse and Dead, never `reserved`) vs `:1099-1107` (`eligible` honours `reserved`/`allow_reserved`)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: CK *Quest Alias Tab* (ck-uesp-wiki line 33): once a reference is in a "Reserves Reference" alias the Story Manager fills no other alias with it unless the new alias is Allow Reserved or is a From External fill (From Event is not an exception). The candidate path enforces it through the shared `reserved` set; the early-`continue` FromEvent branch never consults it, nor does `event_aliases_fill` at quest-selection time. An event slot naming a reserved reference double-books it.
- **Impact**: rare double-booking of a reserved actor by an event-started quest; quest pre-emption semantics differ from the CK.
- **Suggested Fix**: apply `!allow_reserved && reserved.contains(&entity)` to the FromEvent branch before binding.

#### SCR-D5-2026-10-17-06: Topic-menu clicks re-roll a stochastic INFO gate (residual of #5682)
- **Severity**: LOW
- **Dimension**: Scene/Package/Cinematic
- **Location**: `byroredux/src/systems/npc_dialogue.rs:213-225` (`topic_entries` keeps only `{topic_form_id, name}`), `:838-849` (`select_topic_by_form_id` calls `select_first_info` fresh)
- **Status**: Regression of #5682 (partial fix: within one operation only)
- **Untrusted-Input**: No
- **Description**: 8daa24105 carries the passing `&InfoRecord` from menu filtering into `select_on_topic`, so one open/click evaluates each INFO once. The list the player sees was filtered in an earlier operation, and the click is a new operation that evaluates the clicked topic's INFOs again. A topic whose INFOs are all gated on `GetRandomPercent` (fn 77) can pass at menu time and draw a failing roll at click time, yielding "no INFO on topic ... passes its conditions right now" for a topic the player just clicked - the second symptom #5682 listed. Closed as fixed; the cross-operation half is not.
- **Impact**: intermittent no-op click on fn-77-gated menu topics (vanilla reach near zero per #5682's census).
- **Related**: #5682, #5645.
- **Suggested Fix**: persist the chosen INFO FormID per menu entry in `NpcDialogueTopic.topics` and have the click look it up (re-validating non-stochastic gates only), or draw fn 77 once per menu build.

#### SCR-D5-2026-10-17-07: Scene-trigger router holds a `QuestStageState` read guard across `evaluate_condition_list` (recursive read)
- **Severity**: LOW
- **Dimension**: Scene/Package/Cinematic
- **Location**: `byroredux/src/systems/cinematic.rs:979-1053` (`match (..., world.try_resource::<QuestStageState>(), ...)` keeps `stages` alive for the arm; `evaluate_condition_list` at :1048 re-acquires it for GetStage-family conditions)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The same recursive-read shape 35f1cf051 removed from the Story Manager ("a second read guard on one thread can deadlock behind a queued writer, #2386"). Here the system is `add_exclusive`, so no writer can queue (hence LOW), but the lock tracker's once-per-acquisition recursive-read warning fires and the pattern is one non-exclusive registration away from a real deadlock.
- **Suggested Fix**: copy the few `QuestStageState` facts the router needs (`is_running`, `get_stage`, `get_stage_done`) into a snapshot before the loop, or evaluate the advance conditions after dropping `stages`.

#### SCR-D5-2026-10-17-08: `scripted_havok_impulse_system` can strand `HavokImpulseRequest` markers; the documented Pattern-B contract is inaccurate
- **Severity**: LOW
- **Dimension**: Scene/Package/Cinematic
- **Location**: `byroredux/src/systems/cinematic.rs:349-351` (early `return` when `HavokImpulseRules` is absent, before the tail drain at :379-383); contract text `crates/scripting/src/cleanup.rs:45` ("early-returns only when there are none")
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The marker table promises the system snapshots, drains exactly those, and early-returns only on an empty set. After the body has resolved `reference_body_entities` for every request, a missing `HavokImpulseRules` returns without draining, so the requests sit and are re-resolved (an O(requests x rigid bodies) scan) every frame until the resource returns. The native queues only when the resource exists (reference.rs:200), so reachability is a load-order change inside one frame; the defect is the contract rot plus the latent strand.
- **Suggested Fix**: drain `asked` before the rules check (or check the resource first), and correct the cleanup.rs table row.

#### SCR-D5-2026-10-17-09: After #5892 a one-shot trigger ping-pong still runs 1024 transitions every frame, indefinitely
- **Severity**: LOW
- **Dimension**: Scene/Package/Cinematic
- **Location**: `crates/scripting/src/animation_graph.rs:288, 1038-1057, 974-998`
- **Status**: NEW (residual of #5892)
- **Untrusted-Input**: Yes
- **Description**: `MAX_TRIGGERS_PER_STEP` bounds one `step`, and `skip_missed` only skips *periodic* triggers ("a one-shot stays due"). Two states whose zero-length / `relative_to_end` triggers raise the event that transitions to each other re-schedule at `now` on every transition, so each frame handles 1024 `send_event`s (three `terms()` evaluations, a trigger purge and a `schedule_from` apiece), forever, for one graph. A mod-supplied behavior graph (cell-placed object, Untrusted-Input) can hold the main thread at a constant multi-millisecond cost without ever tripping a guard; the runaway warning is only emitted from the periodic path. Cost per frame is unmeasured here (no engine run).
- **Impact**: sustained frame-time burn from one malformed placed-object graph.
- **Related**: #5892.
- **Suggested Fix**: when `MAX_TRIGGERS_PER_STEP` is hit with non-periodic triggers still due, drop the due one-shots (and report once), or disable the graph's trigger schedule after N saturated frames.

### Dedup notes and cross-dimension pointers

- #5647 (StoryEvent one slot per entity): still open; the new SCPT queue and `PendingScriptKills` do not change it; AHEL/KILL keyed by greeter/victim do not collide with each other. Not refiled.
- #5682 closed by 8daa24105: intra-operation half verified, residual filed as -06 (regression of #5682, LOW).
- #5752 closed by bd85de5ea: verified; the decline applies to every INFO with any `!is_modeled` condition, including members of OR blocks the CK would pass via another member (fail-closed by design). Collateral worth tracking under the open #5750 / #5778: Oblivion INFOs outside the 24-function catalog (39 % per #5752's census) now decline outright instead of evaluating against 0.0, so #5778's "87 % of GREETING declined" figure (measured on quest-level gates) understates the INFO-level cut; not remeasured, no new issue.
- #5750, #5778 open; cited, not refiled.
- Candidates dropped after premise-check: (a) hostile-graph O(n^2) in `ModifierState::sync` - decoder caps each modifier list at 1024 and depth 16 (hkx behavior.rs:1445, MAX_MODIFIER_DEPTH), so the active set is small; (b) `object_graph_step_system` holding `AnimationStack` write while reading `AnimationPlayer` (animation_graph.rs:1512-1516) - single-system same-thread order, no opposing edge found; (c) FO4 corpse re-animation (#5861) - fixed and verified; (d) `ObjectAnimationGraph` stuck on a transition into a state id the machine lacks - vanilla ids resolve, produces no clips not a hang; (e) `fill_location_alias` re-scanning the whole LCTN pool every dirty refresh for unmatched find-matching aliases - plausible perf cost on FO4/Skyrim, unmeasured, not filed; (f) permanent `QuestAliasInjectionState.inventory_grants` never regranting on a quest rerun - documented as permanent, CK semantics not established locally (no guessing).
- Closed issues verified holding: #5643, #5404, #5406, #5419, #5435, #5372, #5392, #5642, #5371/#5373, #5452, #5430, #5432, #5861, #5864, #5870, #5867, #5892, #5894, #5898, #5645, #5628, #3817, #5379, #5384, #5490.

#### Items for other dimensions
- Dim 2 (locks): SCR-D5-2026-10-17-02 and -07 are lock-order items; `SceneActorBindings` dirty/commit ordering in `refresh_scene_actor_bindings` is clean but see -01.
- Dim 3/4 (event runtime / attach): `created.retain_quests` (quest_alias.rs:1250-1256) drops Created Object "At" records for non-running quests without despawning the spawned reference; check the owning spawn/cleanup path for a leak when a quest stops.
- /audit-gameplay: `cinematic_horse_route_system` keeps driving a tether whose horse died (no abort on `Dead`; tether and its retention only release at the terminal marker); `graph_locomotion`/`ConsoleWalk` interplay.
- /audit-save: `LocationAliasBindings` / `LocationClearState` are saved (FORMAT bump ledger is theirs); `StoryManagerNodeState` ordering finding -02 touches its lock graph only.
- /audit-parsers: all hkx caps relied on above (MAX_GRAPH_STEPS 1M, depth 32, 1024 list cap) are what keeps Dim 5 traversal bounded; any loosening there reopens the Dim 5 recursion/CPU questions.

# Dimension 6 Legacy ObScript Execution

### Delta scope
- Baseline 5d31fd637 -> HEAD a071ed217. Commits touching the dimension's files:
  - `5182f7817` (10-13) ObScript Message/MessageBox/GetButtonPressed: `obscript_vm.rs` `decode_message`, `messages.rs` path. First audit of this code.
  - `f67d82b3c` (10-13) quest locals persist + fn 79 GetQuestVariable.
  - `173f9e155` GetButtonPressed global slot (closes #5790), `1502fbd3d` width/precision parse (closes #5789),
    `fecdd043a` command-table doc (closes #5788), `a5e5346d0` fn-79 / persisted-locals tests (closes #5787) (all 10-17).
  - `251d491ef` / `d7c47d741` hygiene + file-split only on `obscript_quests.rs`.
  - `obscript.rs`, `obscript_runtime.rs`: NO commits since baseline (guards spot-checked, bounds re-read).
  - `attach.rs` `attach_scpt_script` / `obscript_dialect_for`: no change to the attach logic; guards re-run.
- Read in full: `obscript_vm.rs` 1-900 (all production code), `obscript.rs` 1-505, `obscript_quests.rs` 1-470 (host, tick, locals),
  `messages.rs` 100-250 + 325-345, `notifications.rs` 1-100, `condition/eval/global.rs` get_quest_variable, `attach.rs` 255-345.
- Wiki checks: `/mnt/data/src/reference/cs-uesp-wiki/(main)/{GetButtonPressed,MessageBox,Message}.wiki`, `Template/MessageFormatting.wiki`.

### Guards run
- `cargo test -q -p byroredux-scripting --lib -- obscript`: 42 passed, 0 failed, 2 ignored (real-data).
- `cargo test -q -p byroredux-scripting --lib -- messages printf`: 3 passed.
- Scratch-only experiments (copies of `substitute_numbers`, outside the repo, rustc 1.96.0 = the repo pin): see SCR-D6-2026-10-17-01.

### Known-open #5787-#5790: verification against current code
- #5787 (Oblivion half of the f67d82b3c flip untested): CLOSED, fix holds. `condition/tests.rs::get_quest_variable_uses_obscript_locals_only_for_legacy_game_paths`
  covers Oblivion + FO3/NV modelled, Skyrim/FO4 pass-through; `obscript_quests.rs::persisted_locals_tests` runs two GameMode ticks and reads the slot via fn 79. FIXED.
- #5788 (stale module doc): CLOSED, fix holds. `obscript_quests.rs:20-23` table now describes Message UI, GetButtonPressed consume, fn 79. FIXED.
- #5789 (`%0.f` precision): CLOSED. `%0.f`/`%2.0f` now parse (`messages.rs:218-225`, tests `messages/tests.rs:11-12`). The *specific* bug is FIXED, but the
  same parse is the source of the new HIGH below (precision unbounded), and flags other than `0` are still unparsed (LOW finding 04).
- #5790 (GetButtonPressed keyed by raising quest): CLOSED. `ButtonPresses(Option<i32>)` is one global consume-once slot (`messages.rs:136-153`), wired at
  `obscript_quests.rs:339` and `notifications.rs:79-83`; test `obscript_button_answer_is_consumed_globally_once`. FIXED, with two residual semantic notes (findings 03).

### Verified-clean items
- Statement framing: `next_statement` / `payload_span` (`obscript_vm.rs:218-234`) use `le_u16_at` (Option) + bound `end <= bytes.len()`; offsets are usize on 64-bit so `offset + 8 + u16` cannot overflow; every statement advance is >= 4 bytes (0x1c: >= 8), so no zero-progress loop.
- 0x1c explicit-caller path (`:286-310`): `le_u16_at` for caller/cmd/len, `checked_add`, `args_end > len` rejected; unresolved caller declined with `CMD_DECLINED` (#5629).
- Recursion caps: `exec_if_chain` <-> `run_arm_body` carry `depth` (`:337-338`, `:386`), `decode_args_at_depth` -> `decode_arg_item` 'X' -> `decode_args_at_depth(depth+1)` (`:624`, `:782`); `decode_message` enters at depth 0 and does not recurse. `skip_to_*` helpers are iterative with an integer depth counter (no recursion). eval_expr's 'X' calls `decode_args` (depth 0) and never re-enters `eval_expr`, so no unbounded cycle. Guards pass.
- Operand stack (`eval_expr`, `:519`): grows by at most one entry per token inside a `u16`-bounded expression (<= 65 535 bytes), no count field to lie about; `apply_operator` pops via `unwrap_or(0.0)` (no underflow panic); `/` guards zero divisor.
- `decode_args_at_depth` / `decode_arg_item` (`:614-798`): every read bounded by the argument region `end` (#5655), `z` slice `bytes[o+1..end]` safe because `o < end`; `count` loop (<= 65 535) terminates early via `None` when the region is exhausted.
- `decode_message` (`:651-693`): every string length checked against `end` (`text_end > end -> None`), `checked_add`, button/variable loops terminate on region exhaustion; a failed decode is declined and counted `CMD_DECLINED` (`:315-321`), never shown half-decoded.
- `exec_set` (`:471-514`): tag, idx, expr_len all range-checked against the payload `end`; remote-variable `Set` declined (#5630).
- `obscript.rs` `decode_extender_calls` / `scan_expression` / `decode_known_arguments`: all lengths `checked_add`-ed and compared to the slice; `.get(a..b)` for sub-slices; diagnostics not panics; loop advances >= 4 bytes. `obscript_runtime.rs`: `MAX_LEGACY_OBSCRIPT_NESTING` = 32 enforced at `:290` and `:518`, SDK caps `MAX_SCRIPT_CALL_BYTES` / `MAX_SCRIPT_STRING_BYTES` at `:614`, `:644`.
- Dialect separation: `obscript_dialect_for` (`attach.rs:263-278`) selects by profile row; FO3 -> `None` (no xNVSE table); `push_known_call` (`obscript.rs:318-332`) keys on `(dialect, opcode)` pairs, tables not shared. Tests `obscript_dialect_follows_the_profile_not_the_game_kind`, `source_less_fo3_scpt_does_not_apply_xnvse_opcode_table` pass.
- Command table honesty: unknown commands -> `decline()` (`obscript_quests.rs:381-392`) returning 0 and counted per quest; `0x1059`/`0x1000` fall-through returns -1 (the MessageBox "window open" value, matches CS MessageBox page). No command returns a guessed value except as noted in finding 05 (0x10ce unverified).
- Locks: `obscript_quest_tick_system` drops the `QuestScriptTable` guard before running scripts, `run_quest_game_mode` drops `table` and holds no guard across host calls; `ObScriptQuestLocals` written after the run with a fresh guard. No lock held across a second mutation.
- GetButtonPressed vs CS wiki: "The first time this is called after a MessageBox button has been pressed, it returns the button number ... At all other times -1" matches `take_button` consume-once. The wiki's spell example (two scripts both calling it, only one receives the answer) matches a single global slot.
- Fn 79: `get_quest_variable` (`condition/eval/global.rs:53-60`) reads `ObScriptQuestLocals[param_1][param_2 as u16]`; `is_modeled` split per game pinned by the new test; `ObScriptQuestLocals` registered in the save registry (`save_io/registry.rs:497`).
- Quest-local persistence: seeded into `VmState` at `run_quest_game_mode` start, stored back after the run, unassigned slot reads 0; `StartQuest`/`StopQuest` flow through `QuestStageState` (running filter in the tick).
- No `include_str!`-based source-scan guards in the dimension's files (grep clean), so no vacuous-guard risk added.

### Findings


#### SCR-D6-2026-10-17-01: `substitute_numbers` precision is taken unbounded from mod text - panic at >= 65 536, ~64 KB output per conversion below that
- **Severity**: HIGH
- **Dimension**: Legacy ObScript
- **Location**: `crates/scripting/src/messages.rs:189-232` (precision parse `:218-225`, `format!("{value:.precision$}")` at `:226`); reached from `obscript_quests.rs:215` -> `messages.rs:169`
- **Status**: NEW (predates `1502fbd3d`, which rewrote the parse but kept it unbounded; present since `98f707493`/`5182f7817`, 2026-10-13, after the baseline)
- **Untrusted-Input**: Yes
- **Description**: The `Message`/`MessageBox` text comes straight out of SCDA (`decode_message`, up to 65 535 bytes, no content check). `substitute_numbers` accumulates any run of `.`/digits after `%` into `spec`, then `precision = precision.parse::<usize>().unwrap_or(6)` with no upper bound, and feeds it to `format!("{value:.precision$}")`.
- **Evidence** (copy of the function compiled in a scratch dir with rustc 1.96.0, the repo's pinned toolchain):
  - `substitute_numbers("Rank %.65536f", &[3.0])` -> `panicked: Formatting argument out of range` (Rust's fmt precision is a u16; `%.65535f` is accepted and emits 65 537 bytes).
  - `"%.65535f".repeat(2000)` (16 KB of text) -> a 131 074 000-byte result; a 64 KB text (8 192 conversions) is ~537 MB for one message, then cloned into `ScriptMessage` / `MessageBoxQueue`.
  - `%.99999999999999999999f` fails `usize` parse -> falls back to 6 (harmless); `%.70000f` is the panic trigger. There is no `catch_unwind` around the Update-stage `obscript_quest_tick_system` (grep: none in core scheduler/byroredux), so the panic takes down the engine process as soon as the (start-game-enabled) quest ticks, i.e. a mod-triggered crash on load.
  - Width is ignored (`spec` is only used for `split_once('.')`), so a huge *width* allocates nothing; only the precision is the problem.
- **Impact**: Any plugin whose quest script carries `Message "%.70000f", x` crashes the game; `%.65535f` repeated exhausts memory (multi-hundred MB per statement per 5 s tick). The same function also formats MESG record text (`messages.rs:337`, Papyrus `Message.Show`), so a malformed ESM MESG hits it without any script.
- **Related**: #5789 (closed; the parse that this finding lives in), #5791 (VM alloc caps, same house rule).
- **Suggested Fix**: Clamp precision (e.g. `.min(20)`; f32 has no digits beyond ~17 significant) and cap the total output length of `substitute_numbers`; add a test with `%.65536f` and `%.99999f`.

#### SCR-D6-2026-10-17-02: `MessageBoxQueue` has no bound, unlike every sibling presentation queue
- **Severity**: MEDIUM
- **Dimension**: Legacy ObScript
- **Location**: `byroredux/src/notifications.rs:30-56` (`MessageBoxQueue`, `route_script_messages` `push_back`); producers `obscript_quests.rs:213-216`
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: `PlayerNotifications` caps at 8 (`MAX_PENDING`) and `ObScriptEffectLog` at 64, but boxes are `push_back`-ed unconditionally. Boxes drain only when the player answers (`app_frame.rs:884-895`: while any page is open the queue just waits; with no UI it is dismissed). An unconditional `MessageBox` in a quest GameMode block (common mod bug; the real engine blocks, here nothing pauses the 5 s tick) enqueues one box per statement per tick, each with up to 64 KB text plus buttons, until the player clears them one by one.
- **Impact**: Unbounded memory growth and an unwinnable modal chain on a buggy/hostile mod; defense-in-depth gap.
- **Suggested Fix**: Cap the queue (drop-oldest or refuse when full, with a log) and optionally suppress a new box while one from the same quest is already pending.

#### SCR-D6-2026-10-17-03: GetButtonPressed global slot - unspecified consumer order and stale answers
- **Severity**: LOW
- **Dimension**: Legacy ObScript
- **Location**: `obscript_quests.rs:401-440` (`running` built from `QuestStageState::iter()` = HashMap order, `quest_stages.rs:105,407`); `messages.rs:136-153`
- **Status**: NEW (residual of the #5790 fix)
- **Untrusted-Input**: No
- **Description**: Since `173f9e155` the answer is one slot consumed by whichever quest script calls `GetButtonPressed` first. Due quests run in `HashMap` iteration order (random per process), so with two polling quests in the same tick the winner is nondeterministic (also any same-tick cross-quest `SetStage`/`GetStage` ordering). Additionally `record_button` never clears on a new box: a previous unread answer is returned while the next box is still open, where the CS says -1 until pressed (MessageBox page: "sends -1 while the window is still open").
- **Evidence**: The CS wiki itself is internally inconsistent (Notes: "will only react to a MessageBox created from within the same script"; spell example and the lead sentence imply a global slot). The global behaviour is a defensible reading; the nondeterminism is what is not.
- **Suggested Fix**: Sort `running` by form id (deterministic, matches record order); clear `ButtonPresses` when a script box is queued.

#### SCR-D6-2026-10-17-04: printf flags (`+`, `-`, space, `e`, `g`) fall through as literal text and do not advance the argument index
- **Severity**: LOW
- **Dimension**: Legacy ObScript
- **Location**: `messages.rs:203-231`
- **Status**: NEW (documented as "width ignored", but flags are not mentioned; #5789 only covered `0`)
- **Untrusted-Input**: No
- **Description**: `Template/MessageFormatting.wiki` lists the switches `+`, `<space>`, `-`, `0` and `%g`/`%.3e`. `%+-5.0f`, `% 5.2f`, `%e` stop the `spec` scan at the flag char, emit `%` + spec literally and do **not** consume an argument, so every later conversion in the same message reads the wrong value. `%05.2f` (flag `0`) parses, but width/zero-fill is ignored (`"123.45"` instead of `"00123.45"`).
- **Suggested Fix**: Parse `[+- 0#]*` flags before width; consume the argument for any recognised conversion even if the flag is not honoured; add `g`/`e`.

#### SCR-D6-2026-10-17-05: Oblivion-empirical command ids run ungated on FO3/FNV quest scripts; `GetSelf` (0x10ce) has no recorded evidence
- **Severity**: LOW
- **Dimension**: Legacy ObScript
- **Location**: `byroredux/src/asset_provider/script.rs:962` (no game gate), `obscript_quests.rs:336-338`, `docs/engine/scripting.md` ~2250 ("Command ids are shared across the three games")
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The ids (0x1039, 0x103a, 0x1059, 0x1000, 0x101f, ...) were aligned against Oblivion.esm SCTX only (`obscript_vm.rs:29-34`); the "shared across three games" claim is documented but not backed by an alignment run on FO3/FNV, and no local reference (CS/GECK wiki) lists opcode numbers (grep: none) - so it is unverifiable from the reference set (feedback_no_guessing). `0x10ce => GetSelf` appears only in `obscript_quests.rs` and the original commit message; it is absent from the module-doc table, the skill's list and any test. Only fn-level data (`real_data_tests`, FNV `Patch04Quest00SCRIPT`) covers structure, not command identity. If any id differs on FNV, a Fallout quest script would execute a different command (e.g. a `SetStage` lookalike) with a guessed meaning instead of declining.
- **Suggested Fix**: Run the SCTX-vs-SCDA alignment on FalloutNV.esm/Fallout3.esm once and record the vote counts, or gate the host's stage-mutating commands on `ScriptGame` until verified; add a `GetSelf` row (with evidence) to the module doc.

#### SCR-D6-2026-10-17-06: `If`/`ElseIf` expression span is bounded by the script buffer, not the statement payload (parity gap with #5655)
- **Severity**: LOW
- **Dimension**: Legacy ObScript
- **Location**: `obscript_vm.rs:344-353` (`cond_end` checked only against `self.bytes.len()`)
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: #5655 bounded argument reads by the statement region and `exec_set` is bounded by `end`, but the If/ElseIf arm header reads `expr_len` at `offset + 6` and evaluates `[offset+8, offset+8+expr_len)` against the whole buffer. A statement whose declared payload is shorter than `4 + expr_len` makes the condition read the following statements' bytes (`next_statement` then continues from the declared payload end), so a malformed script evaluates a wrong condition rather than reporting `Malformed`. No panic/OOB (all slices are inside the buffer); correctness-of-malformed-input only.
- **Suggested Fix**: Take `payload_span(self.bytes, offset)?` and require `cond_end <= payload_end`; add a leg to `straddling_argument_payloads_report_malformed_not_next_statement_bytes`.

### Dedup notes and cross-dimension pointers

- #5787, #5788, #5789, #5790: all closed on 2026-10-17 by `a5e5346d0`, `fecdd043a`, `1502fbd3d`, `173f9e155`; each fix verified above (the #5789 fix leaves the unbounded-precision defect, filed as new 01).
- #5646 (`GetRandomPercent` 0..=100) visible in `condition/eval/global.rs:46` (`rng.pick(101)`) - existing, not in this dimension.
- #5791 (VM alloc caps) is the closest filed class; it does not cover `substitute_numbers`.
- No open issue found by keyword (message/box/precision/printf/panic) for findings 01-06.
- Message text `c as char` (Latin-1, not CP-1252 for 0x80-0x9F) is a cosmetic gap acknowledged in the code comment ("ASCII range is all vanilla uses"); not filed.

#### Items for other dimensions
- Dim 7 / Provider: `messages.rs:337` `show_message` (Papyrus `Message.Show`, MESG records) feeds the same `substitute_numbers`, so finding 01 also gates MESG text; the fix should land in the shared function.
- `/audit-save`: `ObScriptQuestLocals` keys (`u16` slot) are not validated against the script's declared SLSD locals (`exec_set` stores any `u16` idx); bounded (65 536 slots per quest) but a save-size/shape question.
- Dim 2 (Fragment Dispatch / events): quest tick order is HashMap order (finding 03); the Papyrus quest stage-event cascade shares `QuestStageState`.

# Dimension 7 Provider / Extender-Compatibility Layer

### Delta scope
- `git log 5d31fd637..HEAD` over `papyrus_provider/`, `compatibility.rs`, `byroredux/src/extensions/`, `crates/pex/src/call_sites.rs`: 10 commits. Only `106f636be` (#5813 `PapyrusPlayerEntity::get`), `78cfa46df` (#5810 `with_extension_host` tail), `91a28a4c9` (#5592 one delivery body), `ef130cd6c` (#5681 wait cap + fragment queue cap) touch this layer's seams; the rest are VM/clippy/AV work that only brushes it.
- Read in full: `papyrus_provider/{execute,ir,runtime,mod}.rs`, `lower_call.rs` (310-657), `lower_program.rs` (1-365, 395-540, 675-1000, 1177-1240), `compatibility.rs` (1-330, 385-407), `crates/pex/src/call_sites.rs` (1-290), `extensions/{systems,dispatch(1-260,420-480),install(375-640)}.rs`, `legacy_compat.rs` (340-420, 700-800), `boot/schedule/late.rs` (370-500, 760-830), `schedule/mod.rs` access-declaration tests, `fragment/effects/deferred.rs` (330-500) + `fragment/effects.rs` (720-790) (provider barrier tail), `cell_loader/references/attach.rs` (690-870), `save_io/{registry,live_load}.rs` call sites, `obscript_runtime.rs::legacy_obscript_load_order_system`.
- Cross-crate checked for the `unreachable!` invariants: `crates/sdk/src/script_function.rs::ScriptValue::matches`, `crates/sdk/src/compatibility/storage_util/list.rs`, `crates/sdk/src/event.rs` payload decode.

### Guards run
- `cargo test -q -p byroredux-scripting -- papyrus_provider compatibility` -> 56 passed, 0 failed.
- `cargo test -q -p byroredux-scripting --test extender_compat_e2e` -> 0 run, 1 ignored (needs the Workshop Framework archive on disk).
- `cargo test -q -p byroredux --bin byroredux -- extensions::tests` -> 80 passed.
- Orchestrator's `boot::schedule` run (/tmp/audit/scripting/test_sched.log): 21 passed, including `extension_event_adapters_run_before_transient_cleanup`, `papyrus_provider_system_declares_everything_it_acquires`, `legacy_obscript_load_order_system_declares_everything_it_acquires`.

### Verified-clean items
- `execute.rs:885-895` three `unreachable!` SendModEvent arms: established by `materialize_mod_event_argument` (`execute.rs:955-958`) returning the value only when `value.matches(expected, false)`, and `ScriptValue::matches` is variant-exact (`sdk/script_function.rs:67-77`: `Float` only for `Float`, `None` only if optional). No lowering invariant is relied on; the check runs immediately before the destructure. Not reachable.
- `execute.rs:1003` `unreachable!("handled above")`: structurally dead arm of a two-phase match (the first match `return`s for Not/And/Or/Compare). Fine.
- `lower_program.rs:788` `.expect("comparison operator was matched")`: guarded by the `if comparison_operator(*op).is_some()` match guard on the same arm. `lower_program.rs:627` `ordered[0]` indexes a fixed `[Option<_>; 3]`.
- `catalog.rs:41-66` `.expect("built-in ... declaration is valid")` run on static engine declarations; covered by the catalog tests.
- `legacy_compat.rs:393-400, 754-787` `unreachable!` homogeneity arms (StorageUtil list results): established by `decode_storage_util_list` (every element is decoded per `kind`, mismatch -> `TypeMismatch`) and `ListOp::slice` (`encode_storage_util_list_values(self.kind, ..)` before returning the replacement). Form filter keeps only `Form(Some(_))`. Not reachable from stored/saved bytes.
- Panic audit of `papyrus_provider_system` (no `catch_unwind` anywhere under `papyrus_provider/` or around the system - confirmed, `grep catch_unwind` hits only `translate/`, `populate.rs`, `inline.rs`): every runtime op is checked (`checked_add/sub/mul/div/rem`, finite float results, zero-divisor guards, `i64::MIN / -1` via `checked_div`, string concat re-checked by `matches`), `locals.get` -> `Err`, `resolver` / `callback` / `publisher` all `Option` + early `continue`/`Err`, host mutex poison -> `map_err` string (`systems.rs:62-98`). The only panicking resource read is `resource_mut::<PapyrusProviderContinuationQueue / PapyrusModEventRuntime>()` (`execute.rs:31, 41, 391, 409`), and both are inserted unconditionally by `papyrus_provider::register` (`runtime.rs:136-140`, called from `registration.rs:135`). `world.register::<PapyrusProviderProgram>()` is in the same function, so the `let Some(programs) = world.query::<..>() else { return }` at `execute.rs:180` cannot drop a drained queue. Conclusion: no reachable panic found; the absence of a net is acceptable but not enforced.
- Caps: `MAX_PROVIDER_HANDLER_NESTING` enforced at lowering (`lower_program.rs:370, 727, 824`) and per dispatch (`execute.rs:458, 738, 770`, `materialize_provider_arguments:650`, `evaluate_provider_value:1027`); `validate_provider_statements` runs on every handler AND every resumed continuation before any callback (`execute.rs:316`). `MAX_PROVIDER_WAIT_SECONDS` enforced in `lower_wait` (`lower_program.rs:707`). `MAX_SCRIPT_ARRAY_ELEMENTS`/`MAX_SCRIPT_STRING_BYTES`/`MAX_SCRIPT_CALL_BYTES` enforced by `ScriptValue::matches` + `ScriptFunctionDeclaration::validate_arguments` at `execute.rs:718-729`. `MAX_PENDING_PAPYRUS_MOD_EVENTS` = 256 enforced by `queue_papyrus_mod_event` (`ir.rs:197-209`) and mirrored by the host's `MAX_PENDING_CUSTOM_EVENTS` = 256 (`extensions/mod.rs:210`), so no silent mismatch. `MAX_COMPATIBILITY_SCRIPTS` = 65 536 enforced in `CompatibilityRegistry::record` (`compatibility.rs:82-85`); `truncated()` is surfaced by `sdk.compat` (`world_info.rs:269-275`), but see SCR-D7-2026-10-17-08.
- Continuation queue vs the Dim-2 finding SCR-D2-2026-10-17-04: `PapyrusProviderContinuationQueue` does NOT have the zero-delay bypass. Every continuation, including `Wait(0)`, leaves `execute_statements` through `still_pending` and passes the single `truncate(MAX_PROVIDER_CONTINUATIONS)` at `execute.rs:385-390` every run (the queue is drained into locals at the top and rewritten from `still_pending` at the end; no other writer exists - `grep PapyrusProviderContinuationQueue` shows only `execute.rs`). Within one frame the vector is bounded by (events x handlers), not unbounded.
- Continuation persistence: `PendingPapyrusProviderContinuation` is `{f32, Vec<PapyrusProviderStatement>, BTreeMap<String, ScriptValue>, Option<PrincipalId>}` - principal-owned, typed, no closures/handles; registered at `save_io/registry.rs:589`; `drop_entity_bound_continuations` is called after restore in `live_load.rs:600-609` and unit-tested (`ir.rs:462-540`). Handlers that would put `self` or an Entity parameter into a continuation are declined at lowering (`lower_program.rs:104-113`, test `self_receiver_handlers_reject_latent_owner_use_until_continuations_persist_it`), so the `self`-has-no-owner abort at `execute.rs:279` is unreachable for lowered programs. Wait + RegisterForModEvent is declined too (`lower_program.rs:114-118`).
- Uninstalled resolver/callback/publisher: entity resolver (`execute.rs:283, 322`), form resolver (`303, 342`), publisher (`879`), callback (`26-29`) all degrade to logged abort/early return. `PapyrusProviderRuntime::default()` with callback `None` is non-servable via `servable_catalog` (#3939), consulted by the attach path (`attach.rs:776-778`).
- Lock discipline: no ECS guard crosses a host callback in `papyrus_provider_system` (queries are collected into owned maps, `drop(programs)` at `execute.rs:275`, `resource_mut` temporaries are statement-scoped); `deferred.rs:473-509` releases the `QuestStageState`/`QuestObjectiveState` guards (block-scoped) before each provider callback. The only two callback call sites in the tree are `execute.rs` and `deferred.rs` (`grep .callback()`). `apply_pending_world_commands` (ECS locks under the host mutex) never calls back into fragments/provider callbacks (`commands.rs` has no `apply_effects`/`invoke_*`), so no self-deadlock on the non-reentrant `std::sync::Mutex`.
- Schedule/Access: `papyrus_provider_system` row (`late.rs:435-455`) declares the runtime (read), continuation queue + mod-event runtime (write), `FormIdPool`, `PapyrusProviderProgram`, the seven event/marker storages and `FormIdComponent`; `legacy_obscript_load_order_system` row (`late.rs:414-428`) declares catalog, invoker, program, two markers, `ScriptVariables`. Both tests are green and both systems are exclusives (declarations are documentation, as the in-file comments say). Order: settings/catalog/input_bindings/player sync -> legacy_obscript -> papyrus_provider -> custom_event -> activation -> ... -> setting_apply -> `event_cleanup_system` is asserted by `extension_event_adapters_run_before_transient_cleanup`.
- SDK payload decode (`LegacySkseVariadicModEventPayload::decode`, `sdk/event.rs:166-215`): count capped at 128, lengths checked with `split_at_checked`, UTF-8 validated; no OOB/overflow.
- `crates/pex/src/call_sites.rs` consumer: O(D) debug-line index (#3938) and per-file copy budget (#4317) are in place; malformed-call diagnostics are charged to the same budget.
- Dispatch tail #5810/#5592: `with_extension_host` drops the host lock before `emit_diagnostics`; the legacy-SKSE mod-event queueing happens after the lock drops (`systems.rs:250-266`). `PapyrusPlayerEntity::get` (#5813) is a pure `try_resource` read; the activation row declares the resource.
- No new vacuous source-scan guards found in this layer (`extensions/mod.rs` SOURCES convention is the existing sentinel-checked one; `execute.rs` has no inline test module so the schedule scan of `EXECUTE_SRC` is complete).

### Findings


#### SCR-D7-2026-10-17-01: Provider `OnInit` handlers re-run on every reference spawn, replaying persisted-state side effects
- **Severity**: HIGH
- **Dimension**: Provider/Extender Layer
- **Location**: `crates/scripting/src/papyrus_provider/lower_program.rs:1177-1188` (`attach_papyrus_provider_program` inserts `OnInitEvent` unconditionally), reached from `byroredux/src/cell_loader/references/attach.rs:796-806`; consumer `execute.rs:197-199`
- **Status**: NEW
- **Untrusted-Input**: Yes (mod-supplied script semantics driving saved StorageUtil/JContainers state)
- **Description**: The provider path emits `OnInit` as a one-frame marker every time the program component is attached. `attach_vmad_scripts` runs for every REFR spawn, i.e. every first stream-in, every cell re-entry after an unload, and every cell reload after a save load. There is no per-reference "already initialized" flag. The Papyrus VM reference-script path has exactly that (`ReferenceScriptState` / saved `initialized_key`, `fragment/reference.rs:200-213`, docs/engine/scripting.md:984 "OnInit the first time the reference loads (a saved per-script flag)"). The CK page for OnInit (`/mnt/data/src/reference/ck-uesp-wiki/(main)/OnInit.wiki`): "This event will run once, when the script is initialized".
- **Evidence**: `attach_papyrus_provider_program` -> `world.insert(entity, OnInitEvent)` with no guard; `papyrus_provider_system` -> `initialized.contains(&entity)` -> `enqueue(OnInit)`; the program component is not saved (`registry_completeness_tests.rs:687` "write-once ... attached from static script content on load") so nothing remembers a prior init. A handler `Event OnInit() / StorageUtil.FormListAdd(None, "registry", Self) / EndEvent` (FormListAdd allows duplicates by default) appends on every re-entry; the StorageUtil list lives in saved principal storage, so the duplicates persist across saves.
- **Impact**: Silent, persisted state corruption for any SKSE-era script whose OnInit is non-idempotent (counters, list registration, `SendModEvent` announce, quest-gating flags). Only idempotent RegisterForModEvent-style OnInit bodies are safe, which is the case the tests exercise.
- **Related**: #4334 (once-only persistence for VM reference scripts); docs/engine/scripting.md:147 table row for `OnInitEvent`.
- **Suggested Fix**: Gate the provider OnInit on the same saved per-reference/per-script initialized flag the VM path uses (keyed by FormID + script name), or decline provider `OnInit` bodies that are not provably idempotent until that flag exists.

#### SCR-D7-2026-10-17-02: Papyrus ModEvent registrations are never pruned on despawn, unload or load - the 4 096 cap fills cumulatively and then silently refuses all new registrations
- **Severity**: MEDIUM
- **Dimension**: Provider/Extender Layer
- **Location**: `crates/scripting/src/papyrus_provider/execute.rs:405-431` (insert + cap), `ir.rs:183-190` (`PapyrusModEventRuntime`, transient); no pruning site exists (`grep PapyrusModEventRuntime` -> only `execute.rs`, `ir.rs`, `runtime.rs`, `late.rs`)
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: Registrations are keyed `(EntityId, PrincipalId, EventId)`. Entity ids are never reclaimed (`world_tests.rs:137 despawn_does_not_reclaim_entity_ids`), so a despawned (cell-unloaded) registrant is never matched again, yet its row stays. The module doc says scripts "refresh registrations from OnInit/OnLoad after a world replacement", which re-registers under NEW entity ids and adds rows. Nothing removes the old ones (no despawn hook, no clear in `live_load`, no `UnregisterAll` on unload). `MAX_PAPYRUS_MOD_EVENT_REGISTRATIONS` = 4 096 is therefore consumed by dead entities over a session of cell streaming / F9 reloads.
- **Evidence**: `apply_mod_event_registrations`: `if contains_key || len < MAX { insert } else { warn "limit exceeded" }`. With ~40 scripted refs per cell and a handful of cell entries / save loads the cap is reached; afterwards every `RegisterForModEvent` is dropped with a warn and the script receives no events for the rest of the session. Compounded by SCR-D7-2026-10-17-01 (OnInit re-registers on every spawn).
- **Impact**: Silent loss of ModEvent delivery in long sessions; also unbounded-until-cap stale memory and a per-frame grouping pass (`execute.rs:50-58`) walking dead rows whenever an event is pending.
- **Related**: #5640 (per-frame grouping), #5535 era lock work (unrelated).
- **Suggested Fix**: Drop `registrations` rows whose entity no longer has a `PapyrusProviderProgram` (cheap retain when events are pending, or on cell unload), and clear the resource in the live-load path.

#### SCR-D7-2026-10-17-03: ModEvent delivery fan-out is 256 events x 4 096 registrations = ~1M handler runs per frame, all deep-cloned up front, with no frame budget
- **Severity**: MEDIUM
- **Dimension**: Provider/Extender Layer
- **Location**: `crates/scripting/src/papyrus_provider/execute.rs:245-273` (push), `277-384` (run), constants `mod.rs:77-79`
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: For every pending event (<=256) and every matching registration of every program entity (<=4 096 total) the loop pushes `(handler.statements.clone(), locals, ..)` into one `handlers` Vec before running anything. `PapyrusProviderInvocation` carries a full `PapyrusProviderRoute`/`ScriptFunctionDeclaration` (parameter id Strings), so a statement clone is not small. Each handler then runs `validate_provider_statements` and one host callback (a Wasm guest entry for extension-published routes) per call statement. There is no analogue of the VM's per-frame statement budget (#5763) here.
- **Evidence**: bound = `MAX_PENDING_PAPYRUS_MOD_EVENTS` (256) x `MAX_PAPYRUS_MOD_EVENT_REGISTRATIONS` (4 096) = 1 048 576 handler instances in one `Late` system. A guest can publish 256 events in one callback, and 4 096 registrations is one script on 4 096 refs registering the same event (see -02, they accumulate anyway). Memory estimate (unmeasured): ~1 KB per call statement -> GB-scale transient allocation and a multi-second stall. Every handler also may `SendModEvent`, so a self-sustaining storm is rate-limited only by the 256/frame publish cap.
- **Impact**: Mod-triggerable frame-time/memory spike (bounded, but the bound is far above any legitimate load). Not an OOB/panic.
- **Related**: #5763 (VM frame budget), #5640.
- **Suggested Fix**: Add a per-frame handler-run budget to `papyrus_provider_system` (carry the excess pending events to the next frame instead of cloning them all), and iterate registrations lazily instead of materializing every handler first.

#### SCR-D7-2026-10-17-04: `papyrus_provider_system` builds a map of every FormID-bearing entity every frame, even with nothing pending
- **Severity**: MEDIUM
- **Dimension**: Provider/Extender Layer
- **Location**: `crates/scripting/src/papyrus_provider/execute.rs:163-179`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: `owner_form_ids` iterates the whole `FormIdComponent` storage and does a `FormIdPool::resolve` + `BTreeMap::insert` per entity, unconditionally, to serve only `SendModEvent` senders (`execute.rs:297-315`). The system runs every frame in `Stage::Late` whenever the extension host exists (the default: `ExtensionHostSlot::initialize_default`), because the early return at `execute.rs:26-29` only triggers when no callback is installed. #5640 removed the equivalent per-frame registration walk; this one remains.
- **Evidence**: O(entities with FormId) per frame (every placed ref in loaded cells), allocation of a fresh BTreeMap, regardless of `handlers` being empty. Cost not measured; estimated on the order of 1-3 ms at tens of thousands of refs.
- **Impact**: Constant per-frame CPU tax for a feature (SendModEvent senders) that almost never has work; "CPU bottleneck = bug" on the dev profile.
- **Related**: #5640.
- **Suggested Fix**: Build the map lazily - only when some collected handler `statements_need_owner_sender`, and only look up the specific owner entities (`world.get::<FormIdComponent>(owner)`) instead of materializing the full table.

#### SCR-D7-2026-10-17-05: A script with provider handlers runs only those handlers - its non-provider handlers are neither lowered nor given to the Papyrus VM
- **Severity**: MEDIUM
- **Dimension**: Provider/Extender Layer
- **Location**: `crates/scripting/src/papyrus_provider/lower_program.rs:20, 94-103` (non-provider handlers skipped: `return Ok(())` at the "mentions provider" check), `byroredux/src/cell_loader/references/attach.rs:812-831` (`None if !provider_attached && attach_vm_reference_script`)
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: `lower_event_into` skips any handler that does not mention a provider ("remain available to existing recognizers"), but the recognizers are whole-script, and the VM attach is explicitly suppressed once a provider program attached (`!provider_attached`, added with the VM reference-script wave `5f1671e59`). For a script such as `OnInit: StorageUtil.IntListAdd(...)` plus `OnActivate: self.Disable()` the first runs and the second never does, so the script executes half-way. The decline-on-unmodeled contract is per script ("a partial or approximate lowering is worse than none").
- **Evidence**: no test covers a mixed script; `attach_vmad_scripts` has no path that runs the remaining handlers. The VM cannot run the provider handlers (it has no provider natives), so the suppression avoids double-fire but at the price of leaving the vanilla handlers inert.
- **Impact**: State drift between handlers that share script variables (e.g. provider OnActivate increments a counter, a non-provider OnUpdate reads it): silent half-behaviour for every mixed SKSE-using script.
- **Related**: pre-VM behaviour was the same (inert), so this is a gap rather than a regression; flagging for an explicit decision.
- **Suggested Fix**: Either decline the provider program when the script also has handlers the engine cannot run (whole-script decline), or let the VM run the non-provider handlers alongside the provider program with a handler partition so each event has exactly one owner.

#### SCR-D7-2026-10-17-06: Lowering accepts fragments with more provider barriers than the runtime can cross; the tail past barrier 64 is dropped after the prefix has applied
- **Severity**: LOW
- **Dimension**: Provider/Extender Layer
- **Location**: `crates/scripting/src/translate/effects/prims/mod.rs:61-70` (no barrier count), `fragment/effects/deferred.rs:464-488` (`at_barrier_limit` skip), `fragment/state.rs:637`
- **Status**: Existing: #3946 (closed; its fix chose "skip the tail whole at runtime", leaving the lowering-side bound as the issue's own suggested alternative)
- **Untrusted-Input**: Yes
- **Description**: `classify_effect_with_providers` turns every claimed provider call into `Effect::ProviderCall` with no count. A straight-line quest-stage fragment with >= 64 StorageUtil/JContainers calls (a plausible mod "init" fragment) lowers fine; at runtime barrier 64 executes and its tail - including any later `SetStage` - is silently dropped with one warn. The fragment-level contract is "check ahead of time, run whole or not at all".
- **Evidence**: `deferred.rs:464-488`; test `provider_barrier_cap_declines_the_deepest_tail_whole` pins the runtime behaviour only.
- **Impact**: Quest stuck mid-fragment on a mod with a long provider-heavy fragment. Low because #3946's author judged the number "plausible for StorageUtil-heavy init fragments" and accepted a warn.
- **Related**: #3946.
- **Suggested Fix**: Count `ProviderCall` effects along the longest path in `lower_fragment` and decline above `MAX_PROVIDER_FRAGMENT_BARRIERS`.

#### SCR-D7-2026-10-17-07: A failing provider call discards the handler's collected ModEvent registrations although earlier callbacks already had effect
- **Severity**: LOW
- **Dimension**: Provider/Extender Layer
- **Location**: `crates/scripting/src/papyrus_provider/execute.rs:362-383`
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: `execute_statements` pushes `Register/Unregister` actions into a local `registrations` Vec that is only applied on `Ok(..)`. On `Err(error)` (a host callback error, a type mismatch at execution, a limit hit) the arm only logs, so a handler that does `RegisterForModEvent(...)` and then a failing `StorageUtil` call leaves the earlier provider-call side effects applied but the registration lost. Papyrus semantics keep the statements that ran before a failed one.
- **Impact**: Handler never receives its mod events after any transient error in a later statement.
- **Suggested Fix**: Apply `registrations` on the `Err` path too (they correspond to statements that already executed).

#### SCR-D7-2026-10-17-08: `CompatibilityRegistry` bounds script count but not bytes; the ObScript source path has no per-script finding budget and truncation is silent in the log
- **Severity**: LOW
- **Dimension**: Provider/Extender Layer
- **Location**: `crates/scripting/src/compatibility.rs:30, 61-90` (registry), `compatibility.rs:166-250` (`analyze_obscript_compatibility` / bytecode path), caller `cell_loader/references/attach.rs:305-335`
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: The PEX path is budgeted (`ScanBudget`, #4317). The ObScript source path is not: every recognized extender command in an SCTX produces a `CompatibilityFinding` that owns ~6 heap strings, so a ~64 KiB script of repeated `IsModLoaded` yields ~5k findings (~2 MB). Retention is capped only at 65 536 scripts. When the cap is hit `record` sets `truncated = true` and returns `false` with no log line; the flag is visible only to an operator who runs `sdk.compat`.
- **Evidence**: no byte/finding cap in `analyze_obscript_compatibility`; `record` has no `log::warn!`. Amplification bounded by input size x ~35, and only for scripts attached to loaded refs, hence LOW.
- **Impact**: Memory amplification from a hostile plugin; silent evidence loss after 65 536 scripts.
- **Suggested Fix**: Charge ObScript findings against the same style of per-script budget (or cap findings per script and set a per-report `truncated`), and `warn!` once when the registry first truncates.

#### SCR-D7-2026-10-17-09: Receiver-expression chains and saved Wait values are not covered by the nesting / wait caps (defense in depth)
- **Severity**: LOW
- **Dimension**: Provider/Extender Layer
- **Location**: `lower_call.rs:481` (`lower_provider_value(&object.node, .., 1)` restarts depth at 1 for each receiver link), `execute.rs:1037, 715, 682` (`materialize_provider_arguments(.., 0)` restarts depth per call), `execute.rs:503-507` (`Wait` validation)
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: (a) `MAX_PROVIDER_HANDLER_NESTING` does not bound a `a.f().f().f()...` receiver chain: lowering, `validate_provider_call` and `materialize_provider_arguments` all restart their depth counter at each call boundary, so the bound is really the front-ends' `MAX_EXPR_DEPTH` = 256 (`papyrus/src/parser/expr.rs:24`, `pex/decompile/lift.rs:380`) -> at most ~128 recursion levels, small frames, so not exploitable today; but the cap this layer advertises is not what enforces it. (b) `validate_provider_statements` re-validates a saved continuation's `Wait` only as finite and >= 0, not `<= MAX_PROVIDER_WAIT_SECONDS`; a continuation saved before #5657 (or a hand-edited save) with `3e38` s therefore survives and, because overflow drops the newest tails, can starve the 4 096-slot queue.
- **Impact**: Hardening only.
- **Related**: #5657, #5656.
- **Suggested Fix**: Thread `depth` through the receiver-expression calls; add the `<= MAX_PROVIDER_WAIT_SECONDS` check to the `Wait` arm of `validate_provider_statements`.

### Dedup notes and cross-dimension pointers

- Dim-2 candidate SCR-D2-2026-10-17-04 (FragmentExecutionQueue zero-delay runs bypass the 4 096 cap): checked the provider analogue - not present (see Verified-clean, single truncate at `execute.rs:385-390` on every run).
- #3948 `catching_panics` covers translate/lower only; the runtime has no net but no reachable panic was found - recorded as clean, not a finding.
- #3951 (access rows) and #3939 (servable catalog) both verified holding; #4139 (`drop_entity_bound_continuations`) holds; #5635/#5640/#5656/#5657/#5681 fixes hold in current code.
- SCR-D7-2026-10-17-06 is the residual of closed #3946 (its fix picked the runtime-side option) - filed LOW with "Existing: #3946" rather than as a regression.
- Not filed: two player resources (`PapyrusPlayerEntity` vs `character::PlayerEntity`) that are unsynchronized - documented in the `PapyrusPlayerEntity::get` doc comment ("agree because scene setup writes both ... harmless in FlyCam").

#### Items for other dimensions
- Dim 4 (attach): `attach_vmad_scripts` calls `translate_pex_detailed_with_providers` (pex parse + compatibility scan + decompile + provider lowering + recognize) per REFR script instance at `attach.rs:775-785` with only the bytes resolved, not the translation, cached (the VM check has `VmReferenceScriptChecks`); cost per placed ref per cell load not measured - worth a look.
- Dim 6 (ObScript): `legacy_obscript_load_order_system` clones every program's `GameMode` handler (`to_vec`) each frame (`obscript_runtime.rs:709-713`) and `collect_writes` recurses on `If` nesting with no depth bound visible in this file (translate-side cap not checked here).
- Dim 2/3: `OnInitEvent`/`OnCellLoadEvent` markers for provider programs are inserted at attach time outside the schedule and drained by `event_cleanup_system` (`cleanup.rs:116`); if a frame has no callback installed (`execute.rs:26-29` early return) the OnInit is consumed unseen - harmless today because `servable_catalog` suppresses lowering without a callback.
- `/audit-save`: `PapyrusProviderContinuationQueue` restore has no length bound on load (the 4 096 truncate runs at the end of the first provider tick, after every restored continuation was validated and run).
