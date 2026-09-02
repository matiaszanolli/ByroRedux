# ByroRedux SDK: next action plan

Status: **in progress**
Checkpoint: `9297681b` (`fix(save): version computed provider receivers`; receiver implementation `fed3e550`)
Date: 2026-09-02

This is the hand-off plan after the first extended SDK implementation pass.
The engine-owned boundary is useful already, but the SDK is not finished and
the v0.1 release gates are not met. Work should continue one bounded slice at
a time, with tests, a conventional commit, and an Ensue checkpoint for each
slice.

## Priority order

### 1. Reconcile the contract and documentation

- Update the main development plan so its older “remaining StorageUtil list”
  wording matches the delivered list, array, slice, and form-filter packs.
- Keep the unsupported boundary explicit: file-backed and object-scoped
  StorageUtil state, cross-mod shared state, JContainers JDB/path/JSON/Lua and
  keyed-map packs, physical input polling/injection, menu registration or
  mutation, arbitrary Scaleform access, and binary extender DLLs.
- Add a single compatibility status table linking each provider alias to its
  route, capability policy, fixture, and known semantic gaps.

### Compatibility status table

The alias column is exhaustive for compatibility calls with an executable or
mapped SDK route. Rows group aliases only when they share one route and one
policy; the final row records recognized calls that are deliberately
unsupported rather than implying that recognition makes them executable.

| Provider aliases currently claimed | Engine route | Disposition and capability policy | Regression evidence | Known semantic gap |
| --- | --- | --- | --- | --- |
| `Game.GetPlayer` | `byro.world.compat.get-player` / `byro.world` | Native engine bridge; stable opaque entity handle, no extender package | `game_content_aliases_run_without_an_extension_package`; `game_get_player_binds_to_an_opaque_object_local` | No-player worlds return `None`; latent receiver ownership and broader Game APIs remain |
| `Game.GetModCount`, `GetModByName`, `GetFormFromFile`, `GetModName`, `GetModDependencyCount`, `IsPluginInstalled`, `GetLightModCount`, `GetLightModByName`, `GetLightModName`, `GetLightModDependencyCount`, `GetNthLightModDependency` | `byro.content.catalog.get-*` | Native read-only catalog adapter; requires `byro.content.catalog.read` | `papyrus_game_content_aliases_preserve_regular_and_light_indices`; `game_content_aliases_run_without_an_extension_package` | Names/keywords/weights and broader form metadata are pending |
| xNVSE/OBSE `IsModLoaded`, `GetModIndex`, `GetNumLoadedMods`, `GetNumLoadedPlugins`, `GetNthModName` | `byro.content.catalog` (`find`, `find-index`, `count`, `plugin-name`) | Native bounded ObScript source adapter; requires `byro.content.catalog.read` | `legacy_load_order_commands_map_to_content_catalog_recipes`; `compiled_get_mod_index_executes_against_engine_content_catalog`; `source_get_num_loaded_plugins_executes_against_engine_content_catalog` | `GetSourceModIndex`, reference construction, and unknown opcode/command execution are not implemented |
| Extender version probes: SKSE/F4SE `GetVersion*`, xNVSE `GetNVSEVersion`/`GetNVSERevision`/`GetNVSEBeta`, and OBSE `GetOBSEVersion`/`GetOBSERevision` | `byro.context` feature discovery | Mapped diagnostic/source-migration path; never fabricates an installed DLL version | `legacy_obscript_scanner_finds_probes_but_ignores_comments_and_strings`; service-catalog capability tests | Callers must migrate to SDK feature discovery; no binary extender identity is promised |
| `Self.Method(...)`, typed `ObjectReference.Method(...)`, and bounded `Game.GetPlayer().Method(...)` receiver forms | Manifest provider route plus the stable entity-handle registry | Native provider dispatch; receiver resolution is callback-local and fails closed on `None`/type mismatch | `self_receiver_dispatch_resolves_the_current_owner_handle`; `typed_object_receiver_dispatch_resolves_the_event_entity_handle`; `receiver_expression_dispatch_evaluates_inner_call_before_outer_call` | Latent self receivers, arbitrary computed arguments, and unproven receiver-producing expressions remain pending |
| StorageUtil scalar (`Get`/`Pluck`/`Has`/`Set`/`Unset`/`Adjust` for supported value types), typed lists, slices, form filters, and prefix count/clear aliases | `byro.storage` (`storage.*`) | Native principal-private, save-backed adapter; reads require `byro.storage.read-own`, writes require `byro.storage.write-own`; `ObjKey` must be `None` | `storage_aliases_are_exact_global_scalar_operations`; `source_papyrus_runs_principal_private_storage_util_across_wait` | Object-scoped/file-backed values and cross-mod sharing are deliberately unsupported |
| ModEvent static `Create`/`Send`/`Release`/`Push*` plus instance `SendModEvent`, `RegisterForModEvent`, `UnregisterForModEvent`, `UnregisterForAllModEvents` | `byro.events` | Native bounded event bus; publish requires `byro.events.publish`, subscriptions require `byro.events.subscribe` | `fixed_mod_event_adapter_preserves_name_payload_and_sender`; `dynamic_mod_event_registration_delivers_typed_callback_and_unregisters`; `source_papyrus_sends_typed_mod_event_across_wait` | Broader event payload schemas and additional canonical events remain to be added |
| JValue/JArray/JMap typed core (`isExists`, `isArray`, `isMap`, `count`, create/get/set/add/remove/clear/copy/retain/release) | `byro.legacy-containers` | Mapped to bounded principal-local save-backed objects; guarded by `byro.storage.read-own` / `byro.storage.write-own` | `jcontainers_aliases_cover_typed_nested_values_negative_indices_and_isolation`; `jcontainers_aliases_only_claim_the_executable_core_surface` | JDB/path, JSON/Lua, keyed maps, pools, timed release, and cross-mod databases are unsupported |
| Input `GetMappedKey`, `GetMappedControl` | `byro.input.compat.*` / `byro.input` | Native read-only binding snapshot; normalized action subscriptions separately require `byro.input.actions.subscribe` | `input_mapping_aliases_are_read_only_bounded_and_case_insensitive`; `engine_compatibility_catalog_lowers_read_only_input_aliases` | Custom action registration, rebinding writes, physical polling, and key injection remain pending |
| UI `IsMenuOpen` | `byro.ui.compat.is-menu-open` / `byro.ui` | Native read-only active-menu snapshot | `ui_menu_alias_reads_only_the_active_visible_menu`; `ui_alias_reads_the_active_engine_menu_snapshot` | Menu registration/mutation and arbitrary Scaleform access remain unsupported |
| Instance `RegisterForKey`/`UnregisterForKey`/`UnregisterForAllKeys` | `byro.events` via normalized input-action subscriptions | Mapped migration path; requires `byro.input.actions.subscribe` | `canonical_input_action_requires_sensitive_capability_and_preserves_semantics` | Physical-key identity compatibility is intentionally not claimed |
| Recognized but not claimed: unknown SKSE/F4SE/ModEvent calls, unknown xNVSE/OBSE commands, `JsonUtil.*`, JFormMap/JIntMap/JDB calls, Input polling/injection, UI menu registration/Scaleform mutation | None (diagnostic classification only) | Unsupported; no engine route or capability grant can make these calls valid | `jcontainers_aliases_only_claim_the_executable_core_surface`; `mod_event_catalog_does_not_map_unknown_provider_functions`; compatibility negative-case tests | Requires a separately designed canonical service, source migration, or explicit port |

Done when the plan has no contradictory “implemented/pending” claims and a
reader can distinguish a native engine service, a compatibility alias, and an
unsupported call.

### 2. Finish the provider execution substrate

- **Delivered:** typed integer/float arithmetic and string concatenation now
  lower from source and execute through the engine provider host, including
  nested provider results and continuation-safe assignments. Save format v17
  records the new expression nodes.
- **Delivered in this slice:** `Game.GetPlayer()` now resolves through the
  engine's canonical `PlayerEntity` resource and the same stable generational
  entity-handle registry used by sandbox projections. No-player/flycam worlds
  return `None`, and malformed arguments fail closed.
- **Delivered with the player bridge:** Papyrus `ObjectReference` locals can
  hold nullable opaque entity results and pass them to typed provider calls.
- **Delivered in this slice:** event-projected typed object locals, including
  `ObjectReference akActionRef`, can dispatch `akActionRef.Method(...)` when
  the declared Papyrus object type has a matching provider route. The engine
  resolves the local through the event's stable opaque entity handle and
  prepends it to the required first `Entity` parameter.
- **Delivered in this slice:** bounded chained receiver expressions such as
  `Game.GetPlayer().Method(...)` evaluate the inner provider call before the
  outer dispatch. Proven `ObjectReference` result types select the route;
  nullable `None` results fail closed before the outer callback. Save format
  v18 records the computed receiver shape and proven object type.
- **Delivered in this slice:** provider conditions can compare engine-owned
  entity handles by stable identity with `==` and `!=`, including `None` for
  missing-player/null object results. Ordered entity comparisons and runtime
  type mismatches remain fail-closed.
- **Delivered in this slice:** a reserved `self.Method(...)` spelling lowers
  to `Self.Method` routes whose required first `Entity` parameter is resolved
  from the current script owner. Latent handlers using `self` remain rejected
  until continuation ownership is persisted.
- Continue with receiver-producing expressions beyond the currently proven
  `Game.GetPlayer`/`ObjectReference` cases and the next justified latent
  primitive. Preserve the current fail-closed rule for receiver calls across
  waits until continuation ownership and resolved object locals are persisted
  safely. The current computed-argument representation is also bounded to
  receiver position; general computed call arguments remain unsupported.
- Add broader event coverage only when the canonical ECS payload and save
  behavior exist; preserve guard-free guest entry and whole-handler rejection
  on unsupported syntax.
- Complete source/byte-level PEX parity tests, continuation migration tests,
  and malformed-result/fault isolation coverage.

Done when an equivalent `.psc` and byte-level `.pex` fixture produces the same
typed result and deferred mutation across a wait and save/reload boundary.

### 3. Close the preserved-SCPT bridge deliberately

- **Delivered in this slice:** source-backed `GetNthModName <index>` now
  lowers through the same bounded immutable content catalog as the existing
  SCDA opcode path. Source and source-less conditional fixtures agree, and a
  non-numeric source argument rejects translation before attachment.
- **Delivered in this slice:** the `GetNumLoadedPlugins` source spelling now
  normalizes through the shared catalog-count adapter, with strict zero-argument
  validation and a live source fixture writing the engine's plugin count.
- Extend the bounded SCDA expression evaluator and non-literal argument
  handling only for commands with a semantic SDK route.
- Add the remaining safe load-order/content commands and explicit diagnostics
  for `GetSourceModIndex`, reference construction, and unsupported xNVSE/OBSE
  operations.
- Keep eager archive scans, command-pack classification, and general ObScript
  execution as separate milestones; do not infer behavior from unknown opcode
  bytes.

Done when source-backed and source-less SCPT fixtures agree, malformed records
fail closed, and unsupported commands are reported before launch.

### 4. Add compatibility packs behind canonical services

Implement these in fixture-driven slices, in this order:

1. **Content/form metadata:** names, keywords, weights, stable lookup, and
   source/projectile form projection where the resolver can prove identity.
2. **Input:** custom action registration and rebinding-aware queries; defer
   physical polling/injection until a trust, lifetime, and capability contract
   is written.
3. **UI:** a bounded menu registration/data/action service for engine-owned
   widgets and notifications; never expose arbitrary Scaleform objects or
   mutation of renderer internals.
4. **StorageUtil/JContainers follow-ups:** file-backed collections, object
   scopes, JDB/path/JSON/Lua, keyed maps, pools, and timed release only where
   bounded persistence and principal isolation are well-defined.

Done for each slice means a real source/PEX call site, a semantic route, a
negative/unsupported test, and no dependency on an extender DLL.

### 5. Complete gameplay and lifecycle gaps

- Add the remaining actor services (magic, appearance, broader animation
  graph control, inventory instance mutation) only after stable identity and
  atomic reconciliation rules exist.
- Define safe spawn/despawn and reference lifecycle semantics; never return
  raw ECS IDs.
- Finish schema migrations, corrupt/missing-extension/downgrade handling,
  status and resource telemetry, in-memory-host parity, and richer
  new-game/cell/save transitions.

Done when the same lifecycle fixture passes in-memory and live, reverse-order
shutdown is observable, and save/load failures isolate one extension without
damaging the base save or another principal.

### 6. Run conformance and release gates

- Add at least one real extender-dependent fixture per covered compatibility
  pack and record supported, mapped, and unsupported calls before launch.
- Run the full SDK, runtime, scripting, executable, documentation, safety,
  and Studio gates from the main development plan.
- Publish the v0.1 compatibility statement and porting guidance for native
  plugins; explicitly state that binary ABI/memory-hook plugins must be
  ported.

Done when all required gates pass and every claimed alias has behavior,
persistence, and failure-handling evidence.

## Session discipline

Do not attempt the whole backlog in one session. Select one numbered slice,
write its contract and negative cases first, implement it, run the narrow and
workspace-relevant tests, commit only owned files, then update Ensue with the
commit, tests, and remaining boundary. Generated codebase-memory artifacts and
unrelated lockfile changes stay out of feature commits.

## Explicit non-goals

The project will not load SKSE/F4SE/xNVSE/OBSE DLLs, emulate their binary ABI,
expose process addresses or raw pointers, or claim compatibility for an
unimplemented call merely because its provider name is recognized.

## v0.1 real-mod coverage: what actually runs today (Session 78)

The hand-off list above states the out-of-scope boundary honestly, but never
answered the question a "v0.1" label implies: for a real mod that a player
would actually install, does it run? This section answers that from the code
that exists today, not from the provider-name catalog.

**Evidence basis.** This is a coverage inventory against `crates/sdk`'s own
classification logic (`compatibility.rs::source_alias` /
`classify_static_call`, `storage.rs`, `legacy_containers.rs`), cross-referenced
against the persistence and configuration idioms real Skyrim/FO4/FNV mods use
— idioms that are public, standardized API surface documented on the Creation
Kit wiki and in mod-author guides (StorageUtil's `(object, key, type)` model,
JContainers' object-handle vs. `JDB` path-database split, SkyUI's MCM
registration contract), not something specific to any one mod's private
source. No actual mod `.psc`/`.pex` source was read for this pass — none
ships in this repo — so the per-mod claims below are "this mod's
documented/well-known persistence pattern hits an unsupported call", not "this
mod's exact source was traced end to end". That distinction matters and is
kept explicit rather than overstated.

### What the SDK supports today

- **Global-scope `StorageUtil`** (`akObj == None`): full scalar (Int/Float/
  String/Form — get/set/has/unset/adjust/pluck) and list coverage
  (`storage.rs`, `compatibility.rs` `source_alias`). This is real, save-backed,
  principal-isolated storage.
- **JContainers object handles** (`JValue`/`JArray`/`JMap`/`JIntMap`/
  `JFormMap`): `legacy_containers.rs` implements a bounded, save-backed,
  per-principal object table (256 objects / 4096 entries / 4 KiB strings per
  principal) behind the same integer-handle contract JContainers exposes to
  Papyrus.
- **`ModEvent`** (`RegisterForModEvent`/`Send*`/`Create`/`Release`) and basic
  `Input`/`UI.IsMenuOpen` queries are Native.
- `Game.GetPlayer()`, entity-handle identity comparison, and typed
  int/float/string provider expressions are Native as of this session's
  earlier checkpoints.

### What breaks a real mod's first persistent write

- **Object-scoped `StorageUtil`** — `StorageUtil.SetIntValue(akObj, key,
  value)` with a non-`None` `akObj` — is explicitly unsupported
  (`source_alias`'s own doc comment: "These aliases therefore cover only
  global (`ObjKey == None`) values"; `storage.rs` has no object-identity
  dimension at all, only a flat per-principal key/value map). Any mod that
  keys data to a specific actor/reference rather than a single global bucket
  — the standard pattern for anything per-NPC or per-item — fails here.
- **`JDB`** (`JDB.solveObj`/`solveFlt`/`solveStr`/`solveIntArray`/…, the
  dot-path global database most JContainers-dependent mods actually use for
  persistence) is explicitly `Unsupported`
  (`classify_static_call("JDB", "solveObj")` pinned Unsupported by its own
  test at `compatibility.rs:4252`). This is a different thing from the
  `JValue`/`JMap`/`JArray` object handles above, which JDB is usually built
  *on top of* in real JContainers — the handle primitives exist here, the
  path convenience layer over them does not.
- **MCM (SkyUI's Mod Configuration Menu)** has no support surface anywhere in
  `crates/sdk` (`grep` for `MCM`/`ConfigMenu`/`SKI_` returns nothing). MCM is
  a Papyrus quest + a Scaleform settings UI that a mod registers pages into;
  the plan's own item 4.3 ("a bounded menu registration/data/action service
  … defer") is precisely this, and it is not started. Any mod that ships an
  MCM settings page — which is most mods with more than one tunable — never
  reaches the point of calling `StorageUtil`/`JDB` at all, because its
  `OnConfigInit`/page-registration entry point has no route.

### The split

Putting the two gaps together against well-known widely-used mods and their
documented persistence patterns:

- **Runs (or gets close):** mods whose only persistent state is global-scope
  `StorageUtil` and/or `JValue`/`JMap`/`JArray` handles, with no MCM page —
  a real but narrow class: simple quest-trigger/behavior mods, "headless"
  gameplay tweaks that read `StorageUtil.GetIntValue(None, …)` for a single
  global toggle.
- **Breaks in the first minute:** the vast majority of what "widely-used"
  actually names — SkyUI itself and the whole MCM-dependent ecosystem
  (Frostfall/Campfire, iNeed, Wildcat, Immersive Citizens, Ordinator's own
  settings, Sim Settlements 2/Workshop Framework, Extensible Follower
  Framework) either registers an MCM page before doing anything else, or
  persists its working state through object-scoped `StorageUtil` (per-NPC/
  per-settlement data) or `JDB` paths (JContainers-heavy frameworks — SexLab
  and its dependents, RaceMenu preset storage, most "framework" mods that
  predate widespread `StorageUtil` adoption). Both blockers are independent
  of each other: fixing one without the other still leaves most of this
  class broken.

So: v0.1 today covers the calls a small, config-free mod makes in its first
minute, and neither of the two things almost every configurable or
framework-shaped mod needs in its second. Naming that class honestly: v0.1 is
a **native dispatch substrate with global-scope persistence**, not yet a
"mods with settings run" release.

### Proposed next slice

Checked against the two candidates rather than assuming: object-scoped
`StorageUtil` needs a new identity dimension the plan's own text ties to
"extension components" not yet designed, and MCM needs a genuine menu
registration/data/action service plus Scaleform page-hosting review — both
larger, and MCM additionally blocks on UI capability policy the plan
deliberately has not written yet (item 4.2/4.3).

**`JDB`, scoped to one principal's private namespace (not the fully
cross-mod-shared tree real JContainers exposes), is the smaller of the two
and unblocks the more common code path.** The reason it is smaller: JDB's
real complexity — dot/bracket path parsing, auto-vivification of intermediate
maps/arrays, typed leaf get/set — sits *on top of* the `JValue`/`JMap`/
`JArray` object model, which `legacy_containers.rs` already implements,
already save-backs, and already bounds. A principal-private JDB is "one
well-known root `JMap` handle per principal, plus a path-parsing convenience
layer that resolves/auto-vivifies through the existing container primitives"
— not a new persistence mechanism. True cross-mod JDB sharing (mod A writing
a path mod B reads) is a distinct, harder problem — the plan's own "cross-mod
shared state" line — and should stay out of scope for this slice; a
principal-private JDB does not by itself enable the mod-interop pattern real
JDB is sometimes used for, and that limitation should be stated plainly when
this slice ships, not discovered later.

This does not by itself flip the "breaks" bucket to "runs" — MCM-gated mods
still won't reach their persistence layer — but it is the next slice that
converts the JContainers-heavy non-MCM frameworks (the SexLab-adjacent and
RaceMenu-preset class) from "breaks" to "runs", and it is buildable from
primitives that already exist rather than from a new mechanism.
