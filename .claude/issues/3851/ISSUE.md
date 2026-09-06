# #3851: TD1-2026-09-05-02: `compatibility.rs` is 3759 production LOC and 55 % StorageUtil — and the SKILL's proposed `ExtenderFamily` split axis does not exist in the code

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD1-2026-09-05-02) via `/audit-publish`, 2026-09-05. Labels: `low,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3851 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD1-2026-09-05-02), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `crates/sdk/src/compatibility.rs` (3759 production / 5344 total LOC)
- **Status**: NEW
- **Age**: created `287f270f`, 2026-08-31 ("feat(scripting): preflight extender calls") — 246 total LOC at birth, **5344 today across 35 commits in 5 days** (a 21× growth)
- **Description**: The audit skill proposes splitting this file "one module per
  `ExtenderFamily::{Skse,F4se,Xnvse,Obse,PapyrusUtil,JContainers,Shared}`". **That axis is wrong.**
  `ExtenderFamily` is a metadata tag on `SourceAlias` / `CompatibilityMatch`, not an organizing
  principle: it appears on 30 of 3759 production lines, and 23 of those 30 are inside the two
  classifier functions `classify_static_call` and `classify_obscript_command`. Splitting on it would
  produce one ~160-line module and six near-empty ones while leaving the real 2000-line mass intact.
- **Evidence**:
  ```
  $ grep -nE 'ExtenderFamily' crates/sdk/src/compatibility.rs | awk -F: '$1<3760' | wc -l
  30
  ```
  The file's actual axis is **service surface**, and it repeats the same four-layer stack per service:
  1. route constants — `PAPYRUS_GAME_*_ROUTE`, `PAPYRUS_INPUT_*_ROUTE`, `PAPYRUS_UI_*_ROUTE`,
     `PAPYRUS_STORAGE_UTIL_*_ROUTE`, `PAPYRUS_LEGACY_CONTAINERS_ROUTE_PREFIX`, `PAPYRUS_MOD_EVENT_ROUTE_PREFIX`;
  2. declaration builders — `papyrus_game_content_declarations`, `papyrus_input_declarations`,
     `papyrus_ui_declarations`, `papyrus_storage_util_declarations`,
     `papyrus_storage_util_list_declarations`, `papyrus_storage_util_prefix_declarations`,
     `papyrus_legacy_container_declarations`, `papyrus_mod_event_declarations`;
  3. source-alias classifiers — `obscript_source_alias`, `method_source_alias`, `source_alias`,
     `storage_util_prefix_source_alias`, `storage_util_list_source_alias`,
     `legacy_container_source_alias`, `classify_obscript_command`, `classify_static_call`;
  4. runtime adapters — `adapt_papyrus_game_*` (11), `adapt_papyrus_input_*`, `adapt_papyrus_ui_*`,
     `adapt_legacy_obscript_load_order`, `adapt_legacy_send_mod_event`,
     `adapt_storage_util_global_{scalar,prefix,list,form_filter}` + the `checked_*` /
     `encode_*` / `decode_*` / `parse_storage_util_*_route` codec helpers.

  **PapyrusUtil StorageUtil alone touches 505 of the 3759 production lines by name** and owns the
  file's whole type vocabulary (`StorageUtilScalarCall/Result/Adaptation/AdapterError`,
  `StorageUtilList{Kind,Value,Call,Result,Adaptation,Operation}`,
  `StorageUtilPrefix{Kind,Operation,Adaptation}`) plus all three of the file's >200-LOC functions:
  `adapt_storage_util_global_list` (384), `papyrus_storage_util_declarations` (251),
  `papyrus_storage_util_list_declarations` (245). Legacy containers (37 lines by name) and mod
  events (43) are comparatively tiny.
- **Impact**: as with `extensions.rs`, this is the fastest-growing debt in the workspace, not
  settled debt. Secondary impact: the wrong axis is currently written into
  `.claude/commands/audit-tech-debt/SKILL.md`, so the next auditor who trusts it will propose a
  refactor that does not reduce the file (report that half under **Dimension 4**).
- **Related**: TD1-…-01 (`extensions.rs` holds the *invocation* side of the same StorageUtil surface);
  TD1-…-03 (`papyrus_provider.rs` holds the *lowering* side); TD1-…-10 (the 106-arm match in this file).
- **Suggested Fix**: `compatibility/{mod,routes,declarations,source_alias,game_content,input_ui,storage_util,legacy_containers,mod_events}.rs`,
  taking `storage_util.rs` first — it is the only extraction that meaningfully shrinks the file
  (~2000 LOC), and it is self-contained because its types are used nowhere else in the crate.
  Keep every `pub` symbol re-exported from `compatibility::` so `byroredux/src/extensions.rs`'s
  50-symbol `use byroredux_sdk::compatibility::{…}` block does not have to change.
- **Effort**: medium

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
