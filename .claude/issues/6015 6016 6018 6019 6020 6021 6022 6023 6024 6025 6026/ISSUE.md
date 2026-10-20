## #6015 SCR-D6-2026-10-17-05: Oblivion-empirical command ids run ungated on FO3/FNV quest scripts; `GetSelf` (0x10ce) has no recorded evidence
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D6-2026-10-17-05: Oblivion-empirical command ids run ungated on FO3/FNV quest scripts; `GetSelf` (0x10ce) has no recorded evidence

- **Severity**: LOW
- **Dimension**: Legacy ObScript
- **Location**: `byroredux/src/asset_provider/script.rs:962` (no game gate), `obscript_quests.rs:336-338`, `docs/engine/scripting.md` ~2250 ("Command ids are shared across the three games")
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The ids (0x1039, 0x103a, 0x1059, 0x1000, 0x101f, ...) were aligned against Oblivion.esm SCTX only (`obscript_vm.rs:29-34`); the "shared across three games" claim is documented but not backed by an alignment run on FO3/FNV, and no local reference (CS/GECK wiki) lists opcode numbers (grep: none) - so it is unverifiable from the reference set (feedback_no_guessing). `0x10ce => GetSelf` appears only in `obscript_quests.rs` and the original commit message; it is absent from the module-doc table, the skill's list and any test. Only fn-level data (`real_data_tests`, FNV `Patch04Quest00SCRIPT`) covers structure, not command identity. If any id differs on FNV, a Fallout quest script would execute a different command (e.g. a `SetStage` lookalike) with a guessed meaning instead of declining.
- **Suggested Fix**: Run the SCTX-vs-SCDA alignment on FalloutNV.esm/Fallout3.esm once and record the vote counts, or gate the host's stage-mutating commands on `ScriptGame` until verified; add a `GetSelf` row (with evidence) to the module doc.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix


## #6016 SCR-D6-2026-10-17-06: `If`/`ElseIf` expression span is bounded by the script buffer, not the statement payload (parity gap with #5655)
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D6-2026-10-17-06: `If`/`ElseIf` expression span is bounded by the script buffer, not the statement payload (parity gap with #5655)

- **Severity**: LOW
- **Dimension**: Legacy ObScript
- **Location**: `obscript_vm.rs:344-353` (`cond_end` checked only against `self.bytes.len()`)
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: #5655 bounded argument reads by the statement region and `exec_set` is bounded by `end`, but the If/ElseIf arm header reads `expr_len` at `offset + 6` and evaluates `[offset+8, offset+8+expr_len)` against the whole buffer. A statement whose declared payload is shorter than `4 + expr_len` makes the condition read the following statements' bytes (`next_statement` then continues from the declared payload end), so a malformed script evaluates a wrong condition rather than reporting `Malformed`. No panic/OOB (all slices are inside the buffer); correctness-of-malformed-input only.
- **Suggested Fix**: Take `payload_span(self.bytes, offset)?` and require `cond_end <= payload_end`; add a leg to `straddling_argument_payloads_report_malformed_not_next_statement_bytes`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix


## #6018 SCR-D7-2026-10-17-02: Papyrus ModEvent registrations are never pruned on despawn, unload or load - the 4 096 cap fills cumulatively and then silently refuses all new registrations
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D7-2026-10-17-02: Papyrus ModEvent registrations are never pruned on despawn, unload or load - the 4 096 cap fills cumulatively and then silently refuses all new registrations

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

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix


## #6019 SCR-D7-2026-10-17-03: ModEvent delivery fan-out is 256 events x 4 096 registrations = ~1M handler runs per frame, all deep-cloned up front, with no frame budget
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D7-2026-10-17-03: ModEvent delivery fan-out is 256 events x 4 096 registrations = ~1M handler runs per frame, all deep-cloned up front, with no frame budget

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

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix


## #6020 SCR-D7-2026-10-17-04: `papyrus_provider_system` builds a map of every FormID-bearing entity every frame, even with nothing pending
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D7-2026-10-17-04: `papyrus_provider_system` builds a map of every FormID-bearing entity every frame, even with nothing pending

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

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix


## #6021 SCR-D7-2026-10-17-05: A script with provider handlers runs only those handlers - its non-provider handlers are neither lowered nor given to the Papyrus VM
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D7-2026-10-17-05: A script with provider handlers runs only those handlers - its non-provider handlers are neither lowered nor given to the Papyrus VM

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

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix


## #6022 SCR-D7-2026-10-17-06: Lowering accepts fragments with more provider barriers than the runtime can cross; the tail past barrier 64 is dropped after the prefix has applied
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

> Residual of closed #3946 (its fix chose the runtime-side option). Filed as a new issue because #3946 is closed.

### SCR-D7-2026-10-17-06: Lowering accepts fragments with more provider barriers than the runtime can cross; the tail past barrier 64 is dropped after the prefix has applied

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

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix


## #6023 SCR-D7-2026-10-17-07: A failing provider call discards the handler's collected ModEvent registrations although earlier callbacks already had effect
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D7-2026-10-17-07: A failing provider call discards the handler's collected ModEvent registrations although earlier callbacks already had effect

- **Severity**: LOW
- **Dimension**: Provider/Extender Layer
- **Location**: `crates/scripting/src/papyrus_provider/execute.rs:362-383`
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: `execute_statements` pushes `Register/Unregister` actions into a local `registrations` Vec that is only applied on `Ok(..)`. On `Err(error)` (a host callback error, a type mismatch at execution, a limit hit) the arm only logs, so a handler that does `RegisterForModEvent(...)` and then a failing `StorageUtil` call leaves the earlier provider-call side effects applied but the registration lost. Papyrus semantics keep the statements that ran before a failed one.
- **Impact**: Handler never receives its mod events after any transient error in a later statement.
- **Suggested Fix**: Apply `registrations` on the `Err` path too (they correspond to statements that already executed).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix


## #6024 SCR-D7-2026-10-17-08: `CompatibilityRegistry` bounds script count but not bytes; the ObScript source path has no per-script finding budget and truncation is silent in the log
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D7-2026-10-17-08: `CompatibilityRegistry` bounds script count but not bytes; the ObScript source path has no per-script finding budget and truncation is silent in the log

- **Severity**: LOW
- **Dimension**: Provider/Extender Layer
- **Location**: `crates/scripting/src/compatibility.rs:30, 61-90` (registry), `compatibility.rs:166-250` (`analyze_obscript_compatibility` / bytecode path), caller `cell_loader/references/attach.rs:305-335`
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: The PEX path is budgeted (`ScanBudget`, #4317). The ObScript source path is not: every recognized extender command in an SCTX produces a `CompatibilityFinding` that owns ~6 heap strings, so a ~64 KiB script of repeated `IsModLoaded` yields ~5k findings (~2 MB). Retention is capped only at 65 536 scripts. When the cap is hit `record` sets `truncated = true` and returns `false` with no log line; the flag is visible only to an operator who runs `sdk.compat`.
- **Evidence**: no byte/finding cap in `analyze_obscript_compatibility`; `record` has no `log::warn!`. Amplification bounded by input size x ~35, and only for scripts attached to loaded refs, hence LOW.
- **Impact**: Memory amplification from a hostile plugin; silent evidence loss after 65 536 scripts.
- **Suggested Fix**: Charge ObScript findings against the same style of per-script budget (or cap findings per script and set a per-report `truncated`), and `warn!` once when the registry first truncates.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix


## #6025 SCR-D7-2026-10-17-09: Receiver-expression chains and saved Wait values are not covered by the nesting / wait caps (defense in depth)
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D7-2026-10-17-09: Receiver-expression chains and saved Wait values are not covered by the nesting / wait caps (defense in depth)

- **Severity**: LOW
- **Dimension**: Provider/Extender Layer
- **Location**: `lower_call.rs:481` (`lower_provider_value(&object.node, .., 1)` restarts depth at 1 for each receiver link), `execute.rs:1037, 715, 682` (`materialize_provider_arguments(.., 0)` restarts depth per call), `execute.rs:503-507` (`Wait` validation)
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: (a) `MAX_PROVIDER_HANDLER_NESTING` does not bound a `a.f().f().f()...` receiver chain: lowering, `validate_provider_call` and `materialize_provider_arguments` all restart their depth counter at each call boundary, so the bound is really the front-ends' `MAX_EXPR_DEPTH` = 256 (`papyrus/src/parser/expr.rs:24`, `pex/decompile/lift.rs:380`) -> at most ~128 recursion levels, small frames, so not exploitable today; but the cap this layer advertises is not what enforces it. (b) `validate_provider_statements` re-validates a saved continuation's `Wait` only as finite and >= 0, not `<= MAX_PROVIDER_WAIT_SECONDS`; a continuation saved before #5657 (or a hand-edited save) with `3e38` s therefore survives and, because overflow drops the newest tails, can starve the 4 096-slot queue.
- **Impact**: Hardening only.
- **Related**: #5657, #5656.
- **Suggested Fix**: Thread `depth` through the receiver-expression calls; add the `<= MAX_PROVIDER_WAIT_SECONDS` check to the `Wait` arm of `validate_provider_statements`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix


## #6026 EXT-D1-2026-10-18-01: The Show-Sky interior climate (and the forced-weather re-apply) enqueue cloud / sun-sprite DDS uploads that nothing flushes. Debug builds trip `finish`'s own `debug_assert`; release builds sample the fallback…
**Full title**: EXT-D1-2026-10-18-01: The Show-Sky interior climate (and the forced-weather re-apply) enqueue cloud / sun-sprite DDS uploads that nothing flushes. Debug builds trip `finish`'s own `debug_assert`; release builds sample the fallback checkerboard for the life of the cell

- **Severity**: MEDIUM. It defeats #5785's purpose (authored clouds in a sky-exposed interior) on every first entry, and it is a debug-build panic on the new path. Contained to TES4 / FO3 / FNV cells that author `XCCM` + Show Sky (Skyrim / FO4 route `XCCM` to `sky_region` since #5463).
- **Dimension**: EXAL boundary discipline
- **Location**:
  - `byroredux/src/cell_loader/load.rs:729` (call) and `:789` (`debug_assert_eq!(pending_dds_upload_count(), 0)`), both in `InteriorCellApplyJob::finish`.
  - `byroredux/src/scene/world_setup.rs:529` (`apply_show_sky_interior_climate`) -> `:299` (`apply_environment_with_sources`) -> `:132` `resolve_cloud_layer` (`:168` `resolve_texture`) and `:909` `resolve_sun_sprite`.
  - Second caller: `world_setup.rs:272` `apply_forced_weather` (`app_step.rs`, new in `d1b298921`).
- **Status**: NEW (introduced by `34a820ceb` and `d1b298921`).
- **Tier Violated**: n/a (a missing batch-flush owner)
- **Game Affected**: Oblivion, FO3, FNV (Show-Sky interiors with `XCCM`); any exterior session using ObScript `ForceWeather`.
- **Description**:
  - `resolve_texture` does not upload. It reserves a bindless slot, points the descriptor at the fallback checkerboard, and queues the DDS (`texture.rs:743-768`, `upload.rs:231-253`). "The engine has no per-frame flush"; the owning loader must drain the queue (`load.rs:1137` doc, `loot_appearance.rs:729` "the frame renderer does not drain it").
  - Interior loads flush once, forced, at `load_references`' tail (`references/mod.rs:902`). `finish` runs after that, and `34a820ceb` put `apply_show_sky_interior_climate` at its top. The clouds' four layers and the CLMT sun sprite are therefore queued with nobody to flush them.
  - The exterior callers of `apply_environment` are followed by a cell load, which flushes (`begin_exterior_streaming`, then the first cell's forced flush). The new interior caller is not followed by one. `apply_forced_weather` is called mid-session from `app_step.rs` and is flushed only if a cell or LOD attempt happens to follow (`flush_pending_lod_textures` runs `if attempted > 0`).
  - `finish`'s own assertion exists for exactly this class (#3320): "a `resolve_texture` call has moved below `load_references`' flush".
- **Evidence**:
  - `finish` order: `apply_show_sky_interior_climate(...)` (`:729`) ... `debug_assert_eq!(ctx.texture_registry.pending_dds_upload_count(), 0, ...)` (`:789`). Nothing between them flushes (`populate_*` load clips, not textures).
  - The only `flush_pending_uploads` callers are `streaming_helpers.rs:485`, `scene.rs:182`, `loot_appearance.rs:740/1056` and `references/mod.rs:919`. None runs after `finish` for an interior.
  - The new test `show_sky_interior_climate_resolves_its_xccm_default_weather` covers only the climate / weather resolution (a `HashMap` lookup); no test reaches a `VulkanContext`.
  - Cache-hit case: if the realm's cloud textures are already resident (second entry), `acquire_by_path` hits and nothing is queued, so the defect shows on first entry per session.
- **Disproof attempted**: could `resolve_cloud_layer` bypass the queue? No: it calls `resolve_texture` (`world_setup.rs:168`). Could the cover logic or a frame hook drain it? `grep flush_pending_uploads` finds no per-frame caller. Not confirmed at runtime (no engine launch allowed).
- **Impact**: Debug build: panic when entering an Oblivion-realm-style Show-Sky interior for the first time. Release build: the cloud layers render the fallback checker (magenta / black) through windows and in the Show-Sky sky until the next cell load flushes. For `ForceWeather` while stationary, the new weather's clouds show the checker until the next cell / LOD apply.
- **Related**: #5785 (closed, introduced the path), #3320 (the invariant), `apply_cell_climate_override` has the same shape but is always followed by a streaming apply.
- **Suggested Fix**: One flush owner. Either call `references::flush_pending_cell_textures(ctx)` after `apply_show_sky_interior_climate` in `finish` and after `apply_forced_weather`, or make `apply_environment_with_sources` end with the same forced flush the loose-load path uses (`scene.rs:172`). Add a source-shape pin that every `apply_environment*` caller is followed by a flush before the `finish` assertion.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' arms, other callers)
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._


