# TD3-2026-10-15-06: `DISPATCH_HANDLED_FOURCCS` doc comment says LCTN "has never had a top-level dispatch arm" and the list below it contains LCTN

Labels: low,esm-plugin,tech-debt,documentation,doc-rot

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD3-2026-10-15-06`).

- **Severity**: LOW
- **Dimension**: 3 - Stale Documentation & Comments
- **Location**: `crates/plugin/src/esm/records/parse.rs:35-39` (comment), `:57` (list entry), `:431` (arm), `crates/plugin/src/esm/records/dispatch_misc_stub.rs:144`
- **Status**: NEW (regression of the #4437 drift note)
- **Age**: note `a3d8efb61` (2026-09-19) was true then; `6f6a9eb3d` (2026-10-12, "LCTN parse") made it false.
- **Effort**: trivial
- **Description**: the arm at `dispatch_misc_stub.rs:144` runs `extract_records` for LCTN; the list is guarded by `dispatch_handled_fourccs_matches_the_live_dispatch_arms`, so the list is right and the comment is wrong. Left as is, the comment tells a maintainer to remove a correct entry.
- **Evidence**: `sed -n 35,39p crates/plugin/src/esm/records/parse.rs`; `sed -n 144p crates/plugin/src/esm/records/dispatch_misc_stub.rs`.
- **Impact**: could cause the "regression class inverted" the comment itself warns about.
- **Related**: #4437, #4278, #5758 (open sf-esm baseline staleness, same family).
- **Suggested Fix**: replace the parenthetical with "LCTN gained its arm in 6f6a9eb3d".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
