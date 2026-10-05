# Scripting Subsystem Audit — 2026-10-05

**HEAD**: `a2c24b16e` · **Baseline**: `docs/audits/AUDIT_SCRIPTING_2026-09-29.md` (HEAD `9fcfdc3fc`) ·
**Audited**: Dim 1 (Recognizer Chain), Dim 2 (Fragment Dispatch & Locks), Dim 3 (Event Runtime), Dim 4 (Attach &
Reference Identity), Dim 5 (Scene/Package/Dialogue/Cinematic) ·
**Unchanged since baseline (skimmed)**: Dim 6 (Legacy ObScript — test-only commits; guards spot-checked), Dim 7
(Provider/Extender Layer — clippy-only; guards spot-checked)

This audit ran as part of `/audit-suite --preset comprehensive`. A single agent analysed every dimension directly, with no
sub-agents. It wrote each dimension to `/tmp/audit/scripting/dim_N.md` before starting the next, and this report was
reconciled against all seven scratch files. Dedup sources:

- the pre-fetched open issues in `/tmp/audit/issues.json`;
- `gh issue list --state closed --search …` for each finding;
- today's sibling reports (`AUDIT_{ECS,CONCURRENCY,GAMEPLAY,ESM,CHARACTER}_2026-10-05.md`).

The engine was not launched. All evidence is static, plus `cargo test`.

## What changed since 2026-09-29

There were 28 files and +2,203 / −232 lines in this skill's core scope. The commits that matter here:

| Commit | Effect on this domain |
|---|---|
| `c59600138` | **#5152 spoken-line fragments**: `DialogueInfoFragments` (TIF_ INFO VMAD), `populate_info_fragments`, `apply_spoken_info_fragment`, OnBegin at selection and OnEnd on change/close. **#5153** `QuestObjectiveEvent` transitions |
| `3c08cfe50` | #5017 `SetUnconscious` primitive + `ActorControlState.unconscious` (save format 31→32), Starts-Unconscious spawn |
| `1816bbc14` | #5041 `GetIsID` compares the Run-On's base object (closes baseline SCR-D3-2026-09-29-01) |
| `c9254beb8` | #5042 `GetActorValue` through `CharacterRuleset::actor_value` |
| `fef67f3c3` | #5046 `StartCombat` removes `AmbientEngagement` |
| `7ab87c0fb`, `305363122` | #5037/#5045 DIAL categories + DLBR branches; #5224 FO3/FNV Top-level flag |
| `1d31ced2b`, `2a1a7a362`, `83d52707e` | #5038/#5043 surface keyed on the NPC, refusal gates; #5025/#5109 alias guards taken alone, bulk Talk filters |
| `a197e8563` | #5056 `purge_cinematic_retention_state` on session replacement |
| `717f39a82` | #5223 FO3/FNV dismember-trigger / kill-on-load corpse recognition at load |
| `1377d3463` | #5088 ObScript 255-script real-data gate made strict and scheduled in CI |

## Build & test state: CLEAN

```
$ TMPDIR=/mnt/data/tmp cargo test -p byroredux-scripting -j 8                          502 passed; 0 failed; 1 ignored
$ <1.96.0 cargo> test -p byroredux --bin byroredux -j 8 -- boot::schedule cell_loader::references \
    reference_enable_gate npc_dialogue cinematic strip_ active_tether attach::tests extensions interaction
                                                                                       230 passed; 0 failed; 1 ignored
```

Every listed guard passes. The HIGH finding below is not caught by any of them. The one test that covers the path,
`the_spoken_lines_fragments_advance_the_stage`, asserts only on the batch contents and never runs the frame's cleanup or
the next frame's dispatcher.

## Executive Summary

**Shipped and verified this cycle:**

- `GetIsID` now tests the base object. The alias pre-filter stays an exact superset because it shares `run_on_identity`.
- `SetUnconscious` follows the literal-default-or-decline rule against `actor.psc`, with the caveat in SCR-D1-2026-10-05-01.
- `StartCombat`'s `AmbientEngagement` removal and `SetUnconscious` take their guards sequentially, and the lock inventory
  names both types.
- The #5153 objective events are emitted only on false→true transitions and are bounded.
- INFO fragment population sits inside `catching_panics` and lowers against `servable_catalog()`.
- The Talk/dialogue alias reads take each guard alone, and the bulk forms are equivalent to the per-entity forms.
- The #5056 purge runs only on session replacement.
- `#4751` is fixed in code. The issue is still open.

**New findings**: 0 CRITICAL, 1 HIGH, 2 MEDIUM, 2 LOW (5 total).

- **HIGH (SCR-D2-2026-10-05-01).** When a line is spoken from the player's activation, its `SetStage` never runs the new
  stage's quest fragment. The spoken-line path claims the fragment journal cursor in `Stage::Late`, then re-emits the
  claimed events into a Pattern-A batch. `event_cleanup_system` drains that batch before any reader sees it. Unrelated
  quest transitions still pending on that cursor are dropped the same way.
- **MEDIUM (SCR-D1-2026-10-05-01).** The "player receiver declines" rule in `SetUnconscious`, `StartCombat` and
  `StopCombat` is only syntactic. A `PlayerRef` VMAD property passes it, and since #4694 it resolves to the player at
  runtime.
- **MEDIUM (SCR-D5-2026-10-05-01).** Spoken-line fragments re-run on every selection. The INFO "Say Once" flag is neither
  decoded nor honoured, so a reward line can be farmed.

**Untrusted-input verdict (runtime-side decoders):**

- `SCDA`: no production change, so **NO panic / OOB / unbounded recursion / OOM**, unchanged from 09-29.
- Provider manifests and `.pex`-derived provider lowering: unchanged, caps intact.
- The new INFO `.pex` population runs inside the `catching_panics` net (parse → decompile → lower), and silently misses on
  bad bytes.
- The `.pex`/`.psc` frontends' verdict is in the newest `AUDIT_PAPYRUS_*`.

## Decline-Invariant Audit (Dims 1, 6)

| Decline point | Verdict |
|---|---|
| `classify_guard_atom` / `split_and` (`||` not split), `While`/`If` exceptions, `MAX_CONDITIONAL_DEPTH` | Holds; untouched since the baseline |
| `prim_set_unconscious`: ≤ 1 arg, `bool_arg` three-case (a non-literal declines) | Holds against `SetUnconscious(bool abUnconscious = true)` |
| Player-receiver declines (`prim_set_unconscious`, `prim_start_combat` #4323, `prim_stop_combat`) | **Syntactic only.** A `PlayerRef` property passes (SCR-D1-2026-10-05-01, MEDIUM) |
| Named call arguments (`CallArg::name`) | Still binds positionally. Existing: #5068 |
| `every_effect_primitive_bounds_its_argument_count`, `only_the_counter_only_primitives_are_placeholders` | Pass. `SetUnconscious` has live readers, so it is not a placeholder |
| Canonical-first provider classification; `translate_pex` panic net | Holds; untouched |
| INFO fragment populate: per-binding lowering (each OnBegin/OnEnd is its own fragment, so it is not a partial lowering) | Holds |
| ObScript VM recursion caps, unknown command → traced no-op | Holds; no production change |
| #5223 corpse recognition: a kill-on-load ObScript is inferred from the **editor ID**, not the SCDA body | Approximate (SCR-D6-2026-10-05-01, LOW) |

## Runtime Lifecycle Matrix (Dims 2–5)

| Invariant | Verdict |
|---|---|
| Nested-lock safety = exclusive scheduling | Holds. `AmbientEngagement` is listed; `ActorControlState` is reached through `update_actor_control`, outside the `fragment::SOURCES` scan, but is already named. Magic types are still unlisted (Existing: #5071) |
| Journal subscribers poll only when they consume, from their owner's slot | **Violated by the #5152 spoken path** (SCR-D2-2026-10-05-01, HIGH) |
| Cascade bound / cursor-stack `apply_effects` / populate-replace / mirror multiset dedup | Untouched |
| #5153 objective transitions: false→true only, `MAX_PENDING` = 16, serde-skipped | Holds (`objective_mutators_emit_transitions_only`) |
| Marker drain coverage (Pattern A/B) | No new marker type. But a Late producer now pushes the Pattern-A `QuestStageAdvancedBatch` after both of its readers have run (same HIGH) |
| `ActivateEvent` consumers | `npc_dialogue_selection` (Late) is still before `event_cleanup_system`. Spoken `Activate` effects go through `PendingFragmentActivations` |
| Two-phase lock drop (timer / trigger / recurring) | Untouched |
| CTDA OR-precedence / empty list = true | Untouched |
| CTDA identity: `GetIsID` | **Fixed** (#5041). It now reads `SceneAliasCandidate::base_form_id` |
| `GetActorValue` composition | Holds. Only `base_authored` values short-circuit (#5042) |
| Alias pre-filters are exact supersets | Hold |
| Talk / dialogue alias reads | Each guard taken alone (#5025). Bulk forms are equivalent |
| FormID-keyed ledgers | Unchanged. A live Enable/Disable consumer is still missing (Existing: #5065) |
| Cinematic retention lifetime | Session-replacement purge added (#5056). The in-session lifetime is still OPEN (#3817) |
| Spoken-line fragment replay | **Not gated** (SCR-D5-2026-10-05-01, MEDIUM) |

## Findings

### HIGH

#### SCR-D2-2026-10-05-01: The spoken-line path claims the fragment journal cursor in `Stage::Late` and re-emits into a batch that `event_cleanup_system` drains unread. A line spoken from an NPC activation never runs the new stage's quest fragment

- **Severity**: HIGH (escalation: a transient marker drained out of stage order, so it is never consumed)
- **Dimension**: Fragment Dispatch & Locks (journal subscribers); Dim 3 has a pointer for marker drain
- **Untrusted-Input**: No
- **Location**:
  - `crates/scripting/src/fragment/systems.rs:229-237`: `apply_spoken_info_fragment` calls `apply_fragment_guard_free`,
    then `poll_fragment_generated_advances`.
  - `crates/scripting/src/fragment/effects.rs:548-584`: `poll_fragment_generated_advances` calls
    `poll_quest_events(FRAGMENT_QUEST_EVENT_SUBSCRIBER)`.
  - `crates/scripting/src/quest_stages.rs:699-717`: the journal `poll` returns every event since the cursor and moves the
    cursor to `next_sequence`, so it is destructive.
  - `byroredux/src/systems/npc_dialogue.rs:374-393`: `dispatch_spoken_fragment` calls `push_quest_stage_advances`.
  - `byroredux/src/boot/schedule/late.rs:437-466`: the selection system is registered in `Stage::Late`.
  - `byroredux/src/boot/schedule/late.rs:521`: `event_cleanup_system` is the last Late exclusive.
  - `crates/scripting/src/cleanup.rs:94`: the `drain_component::<QuestStageAdvancedBatch>` call.
  - `byroredux/src/boot/schedule/update.rs:434`: `quest_fragment_dispatch`, in `Stage::Update`.
- **Status**: NEW. It was introduced by `c59600138` (#5152). Today's ECS D5-02 (Access row) and concurrency #5066 (guard
  shadow) look at the same call chain but not at this.
- **Description**: `poll_fragment_generated_advances` was written for callers that run inside the fragment subscriber's
  own dispatch window:
  - `quest_fragment_dispatch_system` polls that subscriber itself at its head.
  - `scene_fragment_dispatch_system` runs immediately before it, in Update. Its re-emitted batch is read as legacy ingress
    a few systems later.

  #5152 reuses the helper from `npc_dialogue_selection`, which runs in `Stage::Late`, after `quest_fragment_dispatch`.
  The poll claims the spoken fragment's own `SetStage` transition and moves the subscriber's cursor past it. The helper
  returns the transition as an advance, and the caller pushes it onto the player's `QuestStageAdvancedBatch`. That batch
  has only two readers, and both run in Update:
  - `quest_fragment_dispatch_system` (`systems.rs:244`);
  - scene playback (`scene/playback.rs:552`).

  `event_cleanup_system` drains the batch at the end of the same Late stage. On the next frame the dispatcher sees
  neither the journal entry (already claimed) nor the mirror (already drained), so the newly-set stage's QUST fragment is
  never run.
- **Evidence**: the spoken path's own doc (`npc_dialogue.rs:328-332`) states the intended behaviour, which is what does
  not happen:
  > "the fixture's Eltrys entry line lowers to `GetOwningQuest().SetStage(..)`, which journals the transition for
  > `quest_fragment_dispatch_system` (Update stage — next frame) to run the stage fragment that displays the next objective."

  The test notes that the poll swept up other producers' events:
  > "The retained journal also carries the fixture/helper quest-start events the bound_world setup produced"
  (`npc_dialogue.rs:1119-1121`)
- **Impact**:
  - **The activation path is affected.** It is the opening line, the Blocking entry in the MS01 fixture. That line's
    `SetStage` advances `QuestStageState`, but the stage fragment never runs. Its effects (`SetObjectiveDisplayed`,
    `StartScene`, `MoveTo`, …) never apply, and nothing logs it.
  - **Other pending transitions are lost too.** Any transition still pending on the fragment cursor when a line is spoken
    is claimed and dropped the same way. This includes `quest_terminal_stage_system` successor starts/completions and
    `fragment_continuation_system` resumed `SetStage`s, which both run after `quest_fragment_dispatch` in Update.
  - **The UI re-selection and close paths still work.** `select_topic_by_form_id` and `end_open_conversation` are called
    from `main.rs` outside the scheduler, so their batch survives to the next Update and is dispatched as unmirrored
    legacy ingress.
- **Related**: #5152, #3012 (the "don't claim the journal while you have nothing to consume" rule), #3277
  (`push_quest_stage_advances`), ECS-2026-10-05-D5-02, #5066.
- **Suggested Fix**: in `apply_spoken_info_fragment`, return the direct advances without polling, as
  `fragment_continuation_system` already does. The journal entry then stays unclaimed for the next frame's
  `quest_fragment_dispatch_system`, which dedups the mirror. Alternatively, move activation selection into Update ahead
  of `quest_fragment_dispatch`. Extend the test so it runs `event_cleanup_system` and then `quest_fragment_dispatch_system`
  with a stage fragment installed, and asserts that the fragment's effect applied.

### MEDIUM

#### SCR-D1-2026-10-05-01: The "player receiver declines" rule in `SetUnconscious` / `StartCombat` / `StopCombat` is syntactic. A `PlayerRef` property lowers, and since #4694 it resolves to the player at runtime

- **Severity**: MEDIUM
- **Dimension**: Recognizer Chain
- **Untrusted-Input**: No
- **Location**:
  - `crates/scripting/src/translate/effects.rs:1392-1405`: `prim_set_unconscious`.
  - `crates/scripting/src/translate/effects.rs:1353-1370`: `prim_start_combat`.
  - `crates/scripting/src/translate/effects.rs:1376-1384`: `prim_stop_combat`.
  - `crates/scripting/src/translate/effects.rs:1682-1688`: `player_expr_ref`.
  - `crates/scripting/src/translate/effects.rs:1739-1745`: `receiver_actor`.
  - `crates/scripting/src/fragment/effects.rs:88-118`: `resolve_object`, whose `Object{alias:-1}` arm calls
    `resolve_entity_by_global_form_id`.
  - `crates/scripting/src/condition.rs:529-534`: the `0x14` case resolves to `PapyrusPlayerEntity`.
- **Status**: NEW. This is a sibling gap of #4323 (closed; its SIBLING checkbox was left unchecked). #4694 (closed, `a70b54f14`)
  opened it: before #4694 a `PlayerRef` property resolved to nothing and the effect declined at runtime.
- **Description**: `actor == ActorRef::Player` is true only for `Game.GetPlayer()` or a local bound from it. Skyrim's
  ubiquitous `Actor Property PlayerRef Auto` (VMAD `Object { form_id: 0x14, alias: -1 }`) lowers as
  `ActorRef::Object(Property("PlayerRef"))`. A player-filled quest alias lowers the same way. At apply time both resolve
  to the player entity. The primitive's own doc says the player is unmodeled ("an unconscious player would need the
  controls, camera and HUD handling …").
- **Evidence**: `PlayerRef.SetUnconscious(true)` passes the `ActorRef::Player` check. At runtime the
  `Effect::SetUnconscious` arm (`fragment/effects.rs:1559`) calls `set_unconscious(true)` on the player's
  `ActorControlState`, which clears `restrained` (`player_control.rs:138-143`). `player_accepts_movement_input` reads
  `restrained` (`byroredux/src/systems/character.rs:142-150`). `PlayerRef.StartCombat(X)` reproduces #4323's impact
  through the same route.
- **Impact**:
  - **`SetUnconscious`**: a scripted restraint is lifted mid-cinematic, so the player can move again. A saved
    `unconscious` flag is left on the player, and nothing ever reads or clears it.
  - **`StartCombat`**: `npc_combat_ai_system` steers the player's `Transform` and auto-strikes, which is #4323 again.
  - **Reachability**: how often vanilla uses `PlayerRef` with these three calls was not measured. That would need game
    data and the decompiler examples, which are out of scope for a static run.
- **Related**: #4323, #4694, #5017.
- **Suggested Fix**: at apply time, decline when the resolved actor is the `PapyrusPlayerEntity`, in all three arms. A
  shared `resolve_npc_actor` would do it. Lowering cannot see the VMAD value, so the static check alone cannot close
  this. Add a test that drives a `PlayerRef` property through `apply_effect`.

#### SCR-D5-2026-10-05-01: Spoken-line fragments re-run on every selection, and the INFO "Say Once" flag is neither decoded nor honoured

- **Severity**: MEDIUM
- **Dimension**: Scene/Package/Dialogue
- **Untrusted-Input**: No
- **Location**:
  - `byroredux/src/systems/npc_dialogue.rs:279-330`: `apply_selection` runs `speak_info_begin_fragment` unconditionally.
  - `byroredux/src/systems/npc_dialogue.rs:218-250`: `select_on_topic`'s fallback menu re-lists the spoken topic.
  - `crates/scripting/src/dialogue.rs:193-222`: `select_info` / `select_first_info` check only speaker and CTDA.
  - `crates/plugin/src/esm/records/misc/dialogue.rs:249-330`: `InfoRecord` has no ENAM field ("Skyrim+ INFOs author no
    `DATA`").
- **Status**: NEW. No issue mentions "say once". ENAM decode belongs to `/audit-esm`; the runtime gate belongs here.
- **Description**: before #5152, selecting a topic was presentation-only. It now executes the INFO's lowered OnBegin
  effects every time the INFO is selected: on every activation of the NPC, and on every UI click of a topic that is still
  listed. Topics that link nowhere fall back to the Top-Level menu, which includes the topic just spoken. Skyrim INFO
  `ENAM` bit 2 is "Say once" (UESP `Skyrim Mod/Mod File Format/INFO`). The CK's Topic Info page describes it as "this Info
  can only be said once by any given Actor. Once spoken, that actor will never say it again". There is no decode and no
  per-actor said-set, so the vanilla guard against replay is missing.

  There is also an asymmetry: re-selecting the **same** INFO runs OnBegin again but never the outgoing OnEnd. The filter
  is `existing.info_form_id != topic.info_form_id` (`npc_dialogue.rs:287-290`).
- **Impact**: a Say-Once reward or hand-off line whose conditions do not flip after it is spoken replays its effects on
  each click. Examples are `AddItem` to the player, `StartScene`, `Enable`, and objective sets. The result is item
  duplication and repeated scene starts. Lines whose `SetStage` flips their own conditions are self-limiting, so the
  blast radius depends on how the content is authored.
- **Related**: #5152, SCR-D2-2026-10-05-01, GAME-D2-01 (the FO3/FNV owning-quest gap).
- **Suggested Fix**: decode Skyrim `ENAM` flags onto `InfoRecord` (route to `/audit-esm`). Keep a saved `(actor, info)`
  said-set, written when a line is spoken. Have `select_info` skip a Say-Once INFO the actor has already said. Note the
  CK caveat that Say Once resets on reload for non-Start-Game-Enabled quests. Fire the outgoing OnEnd before re-speaking
  the same INFO.

### LOW

#### SCR-D2-2026-10-05-02: #5152 doc rot. `apply_spoken_info_fragment` was spliced into `quest_fragment_dispatch_system`'s doc block, and the `DialogueInfoFragments` doc still says OnEnd is never dispatched

- **Severity**: LOW
- **Dimension**: Fragment Dispatch & Locks
- **Untrusted-Input**: No
- **Location**:
  - `crates/scripting/src/fragment/systems.rs:209-239`: the doc block and the two functions it now straddles.
  - `crates/scripting/src/fragment/state.rs:353-366`: the `DialogueInfoFragments` doc.
- **Status**: NEW. This is the same class as #5104 and #5227 (both closed).
- **Description**: the new `///` block was inserted between `quest_fragment_dispatch_system`'s doc ("Consume
  `QuestStageAdvanced` markers … bounded by `MAX_CASCADE` …") and its `pub fn`. The result:
  - `apply_spoken_info_fragment` now wears both docs.
  - The crate's central public dispatcher is undocumented.

  Separately, the `DialogueInfoFragments` doc says "The OnEnd binding is stored but not yet dispatched … Vanilla's 3 773
  OnEnd-only INFOs stay inert". The same commit dispatches OnEnd on selection change and on close
  (`npc_dialogue.rs:353-362, 400-418`).
- **Impact**: rustdoc and readers are misled about the dispatcher's contract and about OnEnd. No runtime effect.
- **Suggested Fix**: move the `apply_spoken_info_fragment` doc below `quest_fragment_dispatch_system`'s `pub fn`, or above
  its doc block. Reword `state.rs:360-366` so it says OnEnd fires on selection change and on close.

#### SCR-D6-2026-10-05-01: #5223 recognises FO3/FNV kill-on-load ObScript by editor ID, not by bytecode, so a same-EDID override or a name-alike mod script is still treated as an unconditional kill

- **Severity**: LOW
- **Dimension**: Legacy ObScript (owner of the file: `/audit-gameplay`. Filed here for the decline invariant.)
- **Untrusted-Input**: Yes (plugin `SCPT` records)
- **Location**: `byroredux/src/cell_loader/reference_state.rs:360-441`, functions `base_script_editor_id` and
  `script_killed_corpse_forms`.
- **Status**: NEW
- **Description**: Pass 1 marks every XLKR target of any placement whose base script EDID **contains**
  `"DismembermentSCRIPT"`. Pass 2 marks an actor whose base script EDID contains that substring or **equals**
  `GenericKillSCRIPT` / `OnLoadKillSelf`. The semantics ("unconditional `Begin OnLoad` under `doOnce`:
  `linkedRef.killactor`") are inferred from the vanilla record name. The engine already decodes SCDA structurally
  (`obscript_vm`, `decode_extender_calls`), but the body is never checked.
- **Impact**: the vanilla outcome is right, and the census is pinned by the `#[ignore]`d
  `fo3_fnv_script_killed_corpses_match_the_measured_census`. A patch or mod that overrides one of these SCPTs with a
  conditional body, or ships a script whose EDID merely contains the substring, gets live actors spawned as corpses: no
  AI, and lootable. Nothing crashes.
- **Suggested Fix**: gate on the decoded body. Require an `OnLoad` block whose only effect is a `KillActor` on
  `GetLinkedRef`/self, and decline otherwise. Failing that, match the exact vanilla EDID family list instead of a
  substring, and only when the SCPT's defining plugin is a vanilla master.

## Cross-referenced (filed today by sibling audits, not re-filed)

| ID | Covers |
|---|---|
| ECS-2026-10-05-D5-02 (LOW) | `npc_dialogue_selection`'s Access row declares `FragmentExecutionQueue`, `PendingFragmentActivations` and `SceneActorBindings` as reads, but the #5152 path writes them |
| CONC (#5066, LOW, scope grew) | The `LoadedCellIndex` guard in `npc_dialogue` is shadowed rather than dropped. It is now held across `apply_spoken_info_fragment` |
| GAME-D2-01 (LOW) | FO3/FNV dialogue ignores the INFO's owning quest, so a shared topic can speak a line from a quest that is not running |

## Existing findings re-verified

| Issue | State at HEAD |
|---|---|
| SCR-D3-2026-09-29-01 (`GetIsID` reference vs base) | **Fixed** in `1816bbc14` (#5041) and verified, including the alias pre-filter's superset property |
| #5068 (named `CallArg` binds positionally) | Still open, code unchanged |
| #5071 (`apply_effect` lock inventory omits the magic types) | Still open. The doc block still omits `SpellList`, `SpellCatalog` and `ActorValues` |
| #5065 (live Enable/Disable has no consumer) | Still open. `spawn.rs:711` still says "there is no live re-spawn" |
| #5074 (`running_quests_binding_entity` doc) | Still open. The doc at `quest_alias.rs:914-915` is unchanged, and the #5025 in-body comment now contradicts it |
| #4751 (`ObScriptDiagnostics` doc link) | Fixed in code, with no `ObScriptDiagnostics` mention left in `obscript_quests.rs`. **Issue still OPEN, so close it.** |
| #4750 | Closed |
| #3817 (cinematic retention never terminates in-session) | Still OPEN and cited. The save-load half is closed by #5056 |
| #4415 (magic runtime partial) | Still OPEN and cited |
| #4113 / #4115 | `/audit-papyrus` scope; not re-verified |

## Future-Phase Readiness

- **P4 dialogue loop.** The route now reaches fragment execution. Two things gate its correctness:
  - SCR-D2-2026-10-05-01: the activation-spoken stage advance never reaches its stage fragment.
  - SCR-D5-2026-10-05-01: replay protection is missing.

  The force-greet entry is still deliberately undecoded. For Oblivion, FO3 and FNV the route stays unreachable by design,
  because those games have no quest aliases. #5224's Top-level flag is therefore pinned by fixture tests only.
- **Player-as-receiver.** Every primitive that declines the player should take the same apply-time check
  (SCR-D1-2026-10-05-01). Primitives added later inherit the gap otherwise.
- **ObScript phase 2.** Once object-script blocks execute, `script_killed_corpse_forms` should be retired in favour of
  running the real `OnLoad` block. Until then it is the one load-time ObScript emulation keyed on names.
- **Alias follow-ups** (the `m47-3-quest-alias-design.md` "Remaining subsystem boundary" list) are unchanged.

## Findings Count

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 2 |
| LOW | 2 |
| **Total (new)** | **5** |

By dimension: Dim 1 has 1 MEDIUM, Dim 2 has 1 HIGH and 1 LOW, Dim 5 has 1 MEDIUM, and Dim 6 has 1 LOW. Dims 3, 4 and 7 found
nothing new.

---
*Generated by the `/audit-scripting` skill (single-agent run). Suggested next step:*
`/audit-publish docs/audits/AUDIT_SCRIPTING_2026-10-05.md` (domain label `scripting`). Also add:

- `quests` and `dialogue` for SCR-D2-2026-10-05-01;
- `dialogue` for SCR-D5-2026-10-05-01, routing the ENAM decode half to `esm-plugin`;
- `game:fo3` and `game:fnv` for SCR-D6-2026-10-05-01.
