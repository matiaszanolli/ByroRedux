# #7247: INC-2026-11-22-05: `FoldProvenance` production fold leaves overridden injected forms in `injected`, contradicting its doc and the fixture path

**Labels**: low,esm-plugin,legacy-compat,bug

From `docs/audits/AUDIT_INCREMENTAL_2026-11-22.md` (report ID **IA-B-01**), audited over `2b40f76b6..f9f65373a`.

- **Severity**: LOW
- **Dimension**: ESM / load order (audit-esm Dim 3)
- **Location**: `crates/plugin/src/esm/records/provenance.rs:44-60` (`note_plugin`), `:80-110` (`injected`, `counts`, `note_writer`)
- **Changed in**: `crates/plugin/src/esm/records/provenance.rs` (884627235, 15d5d459b)
- **Status**: NEW (part of #6670 Phase 1)
- **Description**: `injected()` is documented as "The injected forms that no later plugin overrode". `note_writer` (the fixture path) honours that: it `remove`s the form from `injected` when a second writer arrives. `note_plugin` (the production fold) does not. When a later plugin overrides a form an earlier plugin injected, the branch at `:47-58` reads `self.injected.get(&form_id)` as the definer and inserts into `overridden`, but never removes the `injected` entry.
- **Evidence**: the test `an_injected_form_names_its_injector_and_a_later_override_follows_it` folds `[0x1], [0xF00], [0xF00]`. After the fold `carriers(0xF00) == [1, 2]` and `injected` still holds `(0xF00, 1)`. The test asserts only `carriers` and `winner`, never `injected()`. By code reading `counts()` then reports `(1, 1)` where the doc implies `(1, 0)`. `summary_lines` builds its "injected" column from `injected()`, so it counts forms a later plugin overrode, while `note_writer`-built fixtures do not.
- **Impact**: the "N injected into a master's ID space" figure (fold log line and `plugins.conflicts` summary) and the per-plugin injected column over-count forms that were also overridden. Report-only today, but LMC-REC-005 is the requirement the column evidences, and the production and fixture paths disagree.
- **Related**: #6670, LMC-REC-005, `byroredux/src/commands/plugins.rs` `summary_lines`.
- **Suggested Fix**: in `note_plugin`'s overridden branch `self.injected.remove(&form_id)` after taking the definer (`winner()` already prefers `overridden`), and assert `injected()` in the test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
