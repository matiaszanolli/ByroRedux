# #6205 — INC-2026-10-20-04: `setting_number` is a copy of `setting_bool` instead of one generic accessor

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6205
- **Labels**: low,tech-debt,bug
- **Filed**: 2026-10-20 by /audit-publish

From `docs/audits/AUDIT_INCREMENTAL_2026-10-20.md` (`/audit-suite --preset quick`, HEAD c4df4c91e).

- **Severity**: LOW
- **Dimension**: tech-debt (duplication)
- **Location**: `byroredux/src/main.rs:1598-1607` vs `:1609-1618`
- **Changed in**: `byroredux/src/main.rs` (commit ea45af067)
- **Status**: NEW
- **Description**: The new helper repeats the `try_resource::<SettingsRegistry>() → get(id) → clone → match variant → unwrap_or(fallback)` chain line for line with only the variant changed. Repo/global guidance prefers extending the existing helper over duplicating it. A third typed reader (Choice) would make a third copy.
- **Suggested Fix**: One `fn setting_value(world, id) -> Option<SettingValue>` (or a generic over an extractor closure) with `setting_bool` / `setting_number` as thin wrappers.

## Completeness Checks
- [ ] **SIBLING**: Other typed `SettingsRegistry` readers in `byroredux/src/` folded onto the same accessor
- [ ] **TESTS**: Existing settings tests still pass; the shared accessor is exercised for Bool and Number
