# TD8-2026-10-25-07: `worldspace_climates` docs call a live input "back-compat" and point at a deleted line

Labels: low, esm-plugin, doc-rot, tech-debt, documentation
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6400
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW (doc rot, Dim 3 overlap) · **Effort**: trivial · **Status**: NEW
- **Location**: `crates/plugin/src/esm/cell/wrld.rs:15-17` ("preserved for back-compat with the cell loader — see byroredux/src/cell_loader.rs:778"; that file is 271 lines) and `cell/index.rs:73-77`. The map is the live input of `env_translate::resolve_worldspace_climate` (`byroredux/src/env_translate/climate.rs:37,61,98`) and two examples.
- **Fix**: reword both docs to "CLMT FormID per worldspace, consumed by `env_translate::climate::resolve_worldspace_climate`"; drop the stale line reference.


#### Dimension 9 — Test Hygiene (3)

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD8-2026-10-25-07`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
