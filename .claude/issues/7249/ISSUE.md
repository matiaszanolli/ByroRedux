# #7249: INC-2026-11-22-07: `plugin_key` folds with Unicode `to_lowercase`, but the previous load-order key was ASCII-only, so identities of non-ASCII-uppercase plugin names change

**Labels**: low,esm-plugin,save-load,bug

From `docs/audits/AUDIT_INCREMENTAL_2026-11-22.md` (report ID **IA-B-03**), audited over `2b40f76b6..f9f65373a`.

- **Severity**: LOW
- **Dimension**: Save / identity (audit-save Dim 2; audit-legacy-compat)
- **Location**: `crates/core/src/form_id.rs:100-103` (`plugin_key`), `byroredux/src/cell_loader/load_order.rs:29-31` (`plugin_basename_lc`)
- **Changed in**: `crates/core/src/form_id.rs` (285633f27), `byroredux/src/cell_loader/load_order.rs` (884627235)
- **Status**: NEW
- **Description**: before this range `plugin_basename_lc` was `Path::file_name().to_ascii_lowercase()`, and the load order hashed those names into `PluginId` (`load_order.rs:196,1031`). `plugin_key` is `basename.to_lowercase()`, which also lowercases non-ASCII letters (the new test pins `Ünïcödé.esp` to `ünï…`). For a plugin whose name has a non-ASCII uppercase letter, the registered `PluginId` and the load-order key therefore change from `hash("Ünï.esp")` to `hash("ünï.esp")`. The commit message and docs say "a lowercase name keeps the identity it always had, so nothing saved changes". That holds for ASCII-lowercased names, not for these.
- **Evidence**: `plugin_identity_folds_case_and_path` asserts only the ASCII case against `Uuid::new_v5(.., b"skyrim.esm")`. Saved `FormIdPair.plugin` values for such a plugin no longer resolve against the new `LoadOrderIdentity`. `str::to_lowercase` also applies context-sensitive final-sigma and multi-codepoint (`İ`) rules, which NTFS-style simple case folding does not.
- **Impact**: only mods named with non-ASCII capitals (rare; most are ASCII). A save made before the change would fail to match that plugin's forms. The project accepts save-format bumps, so this is a wording and awareness item. The practical risk is the claim "nothing saved changes" being relied on.
- **Related**: LMC-ID-001, LMC-ID-009, `docs/engine/plugin-loading.md:24,157`.
- **Suggested Fix**: either state the non-ASCII exception in the doc and commit trail, or fold per-character with simple case mapping (`chars().flat_map(char::to_lowercase)` still has the İ expansion, so pick the mapping to match the target filesystems deliberately) and record the decision under LMC-ID-009.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
