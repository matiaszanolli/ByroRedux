# #3843: TD1-2026-09-05-01: `extensions.rs` is the largest production file in the workspace — a 28-field / ~60-method `ExtensionHost` God Object built in five days

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD1-2026-09-05-01) via `/audit-publish`, 2026-09-05. Labels: `medium,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3843 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD1-2026-09-05-01), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: MEDIUM
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `byroredux/src/extensions.rs` (5921 production / 10652 total LOC); `ExtensionHost` struct at `:329`, its single `impl` block `:360`–`:3539`
- **Status**: NEW
- **Age**: created `24df5304`, 2026-08-31 ("feat(engine): host sandboxed extensions natively") — 2144 total LOC at birth, **10652 today across 66 commits in 5 days**
- **Description**: One file carries 2.96× the split threshold and is 1.6× the next-largest
  production file. `impl ExtensionHost` is a single 3180-line block holding ~60 methods over
  six responsibilities that share no state beyond the struct itself. This is the same
  architectural shape #3736 was promoted to MEDIUM for (`VulkanContext`, 128 fields) — CLAUDE.md
  Architecture Invariant 1, "ECS over scene graph … No God Objects".
- **Evidence**: the six responsibilities are physically contiguous and separable —

  | Region (symbols) | ≈LOC | Responsibility |
  |---|---|---|
  | imports `:8`–`:117` + `EXTENSION_STATE_RESOURCE`…`MAX_PENDING_REPUTATION_WRITES`, `EntityHandleRegistry`, `HostedComponent`, `HostedConsoleCommand`, `HostedScriptFunction`, `RecurringCadence` | 360 | types + 110 lines of `use`, 44 of which are `PAPYRUS_STORAGE_UTIL_*_ROUTE` constants |
  | `ExtensionHost::new` → `install_package` → `console_commands` / `invoke_console_command` / `papyrus_provider_catalog` / `invoke_papyrus_provider` / `invoke_owned_papyrus_provider` / `enqueue_published_event` / `invoke_mod_event` | ~690 | lifecycle + host-service dispatch |
  | `invoke_storage_util`, `invoke_storage_util_prefix`, `invoke_storage_util_form_filter`, `invoke_storage_util_list`, `invoke_legacy_container` | **~1040** | PapyrusUtil / JContainers legacy-extender shims |
  | `bind_entity`…`dispatch_updates` (8 public `dispatch_*` + their `_with_projections` / `_inner` twins) | ~800 | canonical event delivery |
  | `validate_saved_state`, `capture_saved_state`, `restore_saved_state`, `decode_saved_state` | ~420 | persistence |
  | `apply_delivery_result`, `Resolved{ActorValueWrite,PlayIdle,ReputationWrite}`, `DeliveryCommitContext`, `take_resolved_*`, `apply_pending_{actor_value_writes,package_evaluations,animation_commands,reputation_writes,world_commands}` | ~600 | command write-back |
  | `entity_projection`, `RawEntityProjection`, `capture_spatial_snapshot`, `capture_entity_projections`, `capture_package_form`, `capture_package_candidates`, `forms_by_entity`, `entities_by_form` | ~640 | ECS → SDK snapshot capture |
  | `extension_{activation,cell_load,equipment,input,session,hit,update}_dispatch_system`, `emit_diagnostics` | ~430 | the seven scheduler-registered ECS systems |
  | `ExtensionHostSlot`, `ExtensionConsoleCommand`, `SessionEventQueue`, `sync_extension_script_function_invoker`, `engine_settings_snapshot`, `settings_snapshot_from_registry`, `register_extension_setting` | ~430 | ECS resources + settings registration |

  Six production functions here exceed 200 LOC: `invoke_storage_util_list` (388),
  `capture_entity_projections` (383), `invoke_storage_util` (259), `invoke_legacy_container` (256),
  `apply_delivery_result` (221), `install_package` (220).
- **Impact**: every extension-host change — a new event kind, a new legacy shim, a new snapshot
  field — recompiles and re-reviews a 10.6k-line translation unit, and every one of the 66 commits
  so far has landed in it. The split cost is growing roughly 1.7k lines/day; deferring is not
  neutral. Blast radius is contained to the binary (nothing outside `byroredux/` imports it).
- **Related**: #3736 (same God-Object class, `VulkanContext`); TD1-…-02 (`crates/sdk/src/compatibility.rs`
  is this file's declaration-side twin and carries the same StorageUtil bulk); the crate is listed
  as an un-owned subsystem in `.claude/commands/_audit-common.md`.
- **Suggested Fix**: promote the table above to a directory — `extensions/{mod,install,legacy_compat,dispatch,persist,commands,capture,systems}.rs`. Take
  `legacy_compat.rs` first: it is the largest single block (~1040 LOC), it is the only region that
  needs the 44 `PAPYRUS_STORAGE_UTIL_*_ROUTE` imports, and moving it deletes ~40 lines from the
  `use` wall at the top of every other region. `ExtensionHost` stays one struct — this is a file
  split, not a state split — so no ordering or guard-drop invariant is touched.
- **Effort**: large (decompose first: land `legacy_compat.rs` alone as step 1)

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
