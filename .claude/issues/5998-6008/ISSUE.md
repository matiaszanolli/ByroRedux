######## 5998
# SCR-D4-2026-10-17-02: VM / recognized scripts attached to non-primary SCOL/PKIN children have no FormID, so the VM can never deliver to them
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D4-2026-10-17-02: VM / recognized scripts attached to non-primary SCOL/PKIN children have no FormID, so the VM can never deliver to them

- **Severity**: LOW
- **Dimension**: Attach & Reference Identity
- **Location**: `byroredux/src/cell_loader/references/synth_child.rs:437-475` (LIGH-only / fxlight non-primary), `:933-945` (static non-primary: `placement_form_id_pair` is `is_primary_synth.then_some(..)`), `crates/scripting/src/fragment/reference.rs:119-153,607-612` (index and delivery both need `entity_global_form_id`)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: Per #3016 each synth child attaches its own base-record SCRI/VMAD, but `stamp_quest_reference` runs only for the primary child, so a non-primary child's root carries no `FormIdComponent`. `ReferenceScriptIndex::refresh` skips such an entity, and `collect_deliveries` logs "skipped: the reference has no FormID". `attach_vm_reference_script` still counts it as attached and `attach_script_for_refr` emits `OnCellLoadEvent`, so `scripts_recognized` is inflated while the script is inert.
- **Impact**: A scripted base object that is a secondary content of a Pack-in or SCOL (for example a scripted activator inside a FO4 PKIN) gets a VM instance that never runs. This is silent and only affects load-time telemetry accuracy. Whether the engine gives PKIN children their own reference identity is unverified.
- **Related**: #3016, #2026.
- **Suggested Fix**: Either document that secondary children's VM scripts are intentionally inert and skip the VM attach for them, or give them a synthetic per-child identity as the created-reference path does.

######## 5999
# SCR-D4-2026-10-17-03: `.pex` is re-extracted, parsed, preflighted and decompiled for every scripted REFR; only the VM verdict is cached
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D4-2026-10-17-03: `.pex` is re-extracted, parsed, preflighted and decompiled for every scripted REFR; only the VM verdict is cached

- **Severity**: LOW
- **Dimension**: Attach & Reference Identity
- **Location**: `byroredux/src/cell_loader/references/attach.rs:765-790` (`resolve_pex` + `translate_pex_detailed_with_providers` per REFR per script), `byroredux/src/asset_provider/script.rs:131-172` (cache only after translate)
- **Status**: NEW
- **Untrusted-Input**: Yes (the work is proportional to attacker-supplied script size)
- **Description**: `VmReferenceScriptChecks` caches the checker verdict per script name, but each REFR carrying the same script still pays archive extraction (zlib / LZ4), `byroredux_pex::parse`, the compatibility preflight, `decompile_script` and the provider lowering before the cache is consulted. The recognizer result depends on per-REFR properties, but the decompiled AST does not.
- **Evidence**: No cache in `resolve_pex` or `translate_pex_detailed_with_providers`. A cell with N REFRs of one script does N decompiles. This is unmeasured; #3938 shows decompile cost on this path was a real hotspot earlier.
- **Impact**: Cell-load CPU scales with scripted-REFR count instead of unique-script count. Modded cells with thousands of scripted REFRs of one script stall the budgeted loader.
- **Suggested Fix**: Cache `(script name -> Arc<ast::Script> + PexCompatibility)` next to `VmReferenceScriptChecks` and run `lower_provider_program` / `translate_script` against the cached AST. Measure first with the `m47-triggers.sh` timing.

######## 6000
# SCR-D4-2026-10-17-04: `check_reference_script` / `check_alias_script` run outside the panic net the rest of the `.pex` path uses
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D4-2026-10-17-04: `check_reference_script` / `check_alias_script` run outside the panic net the rest of the `.pex` path uses

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

######## 6001
# SCR-D4-2026-10-17-05: `check_running_quest_alias_scripts` bounds quests per frame, not scripts, so one quest can decompile an unbounded number of `.pex` in a frame
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D4-2026-10-17-05: `check_running_quest_alias_scripts` bounds quests per frame, not scripts, so one quest can decompile an unbounded number of `.pex` in a frame

- **Severity**: LOW
- **Dimension**: Attach & Reference Identity
- **Location**: `byroredux/src/asset_provider/script.rs:187-286` (`ALIAS_CHECKS_PER_FRAME` = 4 applies to quests; the inner loop at `:225` decompiles every alias script of each)
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: The comment justifies the cap as avoiding "a second of load", but the unit is the quest. A quest whose VMAD alias section lists many distinct scripts (a mod quest, or a vanilla quest with dozens) triggers one `extract_or_warn` + decompile + check per distinct name in a single frame. The verdict cache only helps repeated names.
- **Impact**: A frame hitch when such a quest starts. It is bounded by the VMAD size and has no persistent effect.
- **Suggested Fix**: Budget by script count (for example 8 distinct uncached scripts per frame) and leave the quest unchecked until the next frame when the budget is hit.

######## 6002
# SCR-D5-2026-10-17-01: Reference-alias fills read last refresh's location-alias bindings, so a reference alias gated on a location alias of the same quest lags (or never fills) on the quest-start refresh
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D5-2026-10-17-01: Reference-alias fills read last refresh's location-alias bindings, so a reference alias gated on a location alias of the same quest lags (or never fills) on the quest-start refresh

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

######## 6003
# SCR-D5-2026-10-17-02: Story Manager dispatcher records a `StoryManagerNodeState <-> QuestStageState` lock-order cycle since 35f1cf051
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D5-2026-10-17-02: Story Manager dispatcher records a `StoryManagerNodeState <-> QuestStageState` lock-order cycle since 35f1cf051

- **Severity**: MEDIUM
- **Dimension**: Scene/Package/Cinematic
- **Location**: `crates/scripting/src/story_manager.rs:715-735` (phase A: `node_state` read guard held across `dispatch_story_event` -> `evaluate`), `:751-790` (phase B: `QuestStageState` write guard held while `StoryManagerNodeState` is taken for write); comment at :748-750 claims "registry -> stages -> node_state" order for both phases
- **Status**: NEW (introduced by 35f1cf051, after the baseline)
- **Untrusted-Input**: No
- **Description**: 35f1cf051 replaced phase A's `QuestStageState` read guard with a `statuses()` snapshot (to stop the nested-read warning). Before it, phase A held `stages` -> `node_state`, consistent with phase B (`stages` -> `node_state`). Now the stages guard is a statement-scoped temporary, so during the walk phase A holds `{registry, aliases, node_state}` while every `evaluate` of a quest/stage/running gate (GetStage, GetStageDone, GetQuestRunning, ... via `try_resource::<QuestStageState>`) acquires `QuestStageState` read: edge `StoryManagerNodeState -> QuestStageState`. Phase B then holds `QuestStageState` write and acquires `StoryManagerNodeState` write (:784): edge `QuestStageState -> StoryManagerNodeState`. Both edges are observable in one production run (the first radiant start after a gated node evaluates). The detector records process-wide edges "even when the hold periods are sequential" (lock_tracker.rs:272-300), so `BYRO_LOCK_ORDER_CHECK=1` on a real SM start closes the cycle and aborts in a debug build. It is not a live deadlock today (both phases run inside one exclusive Update system), which is why this is MEDIUM; the CI lane cannot see it because no unit test installs `StoryManagerNodeState` (guard-run note above).
- **Impact**: a debug/lock-check engine run panics on the first SM quest start with a gated node and installed node state; the cycle becomes a real ABBA the moment any other (non-exclusive) system reads/writes either resource.
- **Related**: #5628, #5366 (earlier SM lock fixes), 35f1cf051.
- **Suggested Fix**: snapshot what phase A needs from `node_state` (clone or release before `dispatch_story_event`, as is already done for the stages) or take `node_state` for phase B before `stages`; add an SM dispatch test that installs `StoryManagerNodeState` and a `GetStage`-gated quest node, run under the lock-order lane.

######## 6004
# SCR-D5-2026-10-17-03: A Location Alias Reference fill ignores the bound location alias (and keyword), binding any candidate of the ref type anywhere
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D5-2026-10-17-03: A Location Alias Reference fill ignores the bound location alias (and keyword), binding any candidate of the ref type anywhere

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

######## 6005
# SCR-D5-2026-10-17-04: `resolve_wanted` restarts every actor sharing a behavior graph, not only the graphs that were waiting on the clip
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D5-2026-10-17-04: `resolve_wanted` restarts every actor sharing a behavior graph, not only the graphs that were waiting on the clip

- **Severity**: MEDIUM
- **Dimension**: Scene/Package/Cinematic
- **Location**: `byroredux/src/asset_provider/actor_graph.rs:322-335` (`.filter(|(_, graph)| std::ptr::eq(graph.graph(), &*job.loaded))` then `graph.start()` + `apply_playback_for`); `crates/scripting/src/animation_graph.rs:830-850` (`start`)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The doc (module header, scripting.md "restarts the graph") says the pass restarts "the graphs that were waiting" on a clip. The filter selects *all* `ObjectAnimationGraph`s whose `LoadedGraph` is the job's shared graph (`ActorGraphCache` shares one `LoadedGraph` per `idle_key`, so every humanoid with the same behavior/folders). `start()` clears the schedule, re-schedules triggers from now and returns the playback of the *current* configuration with `start_time`/pose phase 0; `apply_playback_for` then replaces each actor's `AnimationPlayer`/`AnimationStack` clip at local time 0. Every first use of a new clip by any one actor (hundreds of lazily-resolved clips per FO4 graph, resolved during gameplay) therefore snaps all other same-graph actors' current animation back to its first frame (a mid-swing attack replays; a looping idle pops; a synced locomotion blend resets its phase) and re-bases their completion triggers.
- **Impact**: visible animation pops across a cell's NPCs on each clip first-use, until the graph's clip set is warm. Self-healing, visual only.
- **Related**: M47.2 3bb6806b5 (PlayAnimation reaches FO4 actors), #5861.
- **Suggested Fix**: restart only graphs that actually asked (record a per-graph `wanted` flag in `LazyClips::lookup`, or restart a graph only if its current `terms()` was empty before the clip was provided) and leave the rest running.

######## 6006
# SCR-D5-2026-10-17-05: A FromEvent alias fill ignores the Reserves / Allow Reserved rule
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D5-2026-10-17-05: A FromEvent alias fill ignores the Reserves / Allow Reserved rule

- **Severity**: LOW
- **Dimension**: Scene/Package/Cinematic
- **Location**: `crates/scripting/src/scene/quest_alias.rs:1017-1090` (FromEvent branch checks `used`/Allow Reuse and Dead, never `reserved`) vs `:1099-1107` (`eligible` honours `reserved`/`allow_reserved`)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: CK *Quest Alias Tab* (ck-uesp-wiki line 33): once a reference is in a "Reserves Reference" alias the Story Manager fills no other alias with it unless the new alias is Allow Reserved or is a From External fill (From Event is not an exception). The candidate path enforces it through the shared `reserved` set; the early-`continue` FromEvent branch never consults it, nor does `event_aliases_fill` at quest-selection time. An event slot naming a reserved reference double-books it.
- **Impact**: rare double-booking of a reserved actor by an event-started quest; quest pre-emption semantics differ from the CK.
- **Suggested Fix**: apply `!allow_reserved && reserved.contains(&entity)` to the FromEvent branch before binding.

######## 6007
# SCR-D5-2026-10-17-06: Regression of #5682 — Topic-menu clicks re-roll a stochastic INFO gate (residual of #5682)
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

> Residual of closed #5682: the fix holds within one operation only. Filed as a new issue because #5682 is closed.

### SCR-D5-2026-10-17-06: Topic-menu clicks re-roll a stochastic INFO gate (residual of #5682)

- **Severity**: LOW
- **Dimension**: Scene/Package/Cinematic
- **Location**: `byroredux/src/systems/npc_dialogue.rs:213-225` (`topic_entries` keeps only `{topic_form_id, name}`), `:838-849` (`select_topic_by_form_id` calls `select_first_info` fresh)
- **Status**: Regression of #5682 (partial fix: within one operation only)
- **Untrusted-Input**: No
- **Description**: 8daa24105 carries the passing `&InfoRecord` from menu filtering into `select_on_topic`, so one open/click evaluates each INFO once. The list the player sees was filtered in an earlier operation, and the click is a new operation that evaluates the clicked topic's INFOs again. A topic whose INFOs are all gated on `GetRandomPercent` (fn 77) can pass at menu time and draw a failing roll at click time, yielding "no INFO on topic ... passes its conditions right now" for a topic the player just clicked - the second symptom #5682 listed. Closed as fixed; the cross-operation half is not.
- **Impact**: intermittent no-op click on fn-77-gated menu topics (vanilla reach near zero per #5682's census).
- **Related**: #5682, #5645.
- **Suggested Fix**: persist the chosen INFO FormID per menu entry in `NpcDialogueTopic.topics` and have the click look it up (re-validating non-stochastic gates only), or draw fn 77 once per menu build.

######## 6008
# SCR-D5-2026-10-17-07: Scene-trigger router holds a `QuestStageState` read guard across `evaluate_condition_list` (recursive read)
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D5-2026-10-17-07: Scene-trigger router holds a `QuestStageState` read guard across `evaluate_condition_list` (recursive read)

- **Severity**: LOW
- **Dimension**: Scene/Package/Cinematic
- **Location**: `byroredux/src/systems/cinematic.rs:979-1053` (`match (..., world.try_resource::<QuestStageState>(), ...)` keeps `stages` alive for the arm; `evaluate_condition_list` at :1048 re-acquires it for GetStage-family conditions)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The same recursive-read shape 35f1cf051 removed from the Story Manager ("a second read guard on one thread can deadlock behind a queued writer, #2386"). Here the system is `add_exclusive`, so no writer can queue (hence LOW), but the lock tracker's once-per-acquisition recursive-read warning fires and the pattern is one non-exclusive registration away from a real deadlock.
- **Suggested Fix**: copy the few `QuestStageState` facts the router needs (`is_running`, `get_stage`, `get_stage_done`) into a snapshot before the loop, or evaluate the advance conditions after dropping `stages`.

