# #7161 — PAR-D2-2026-11-21-06: the CDB instance-type peek runs before the #7132 context wrapper

Labels: low,import-pipeline,bug,game:starfield

- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/sfmaterial/src/index.rs:247` (`peek_top_level_class_name(&state)?`, before `let at = …`)
- **Status**: NEW (residual of #7132, closed)
- **Trigger Input**: an instance chunk whose type ref is outside the class table, or whose first u32 is cut.
- **Description**: these errors still carry no instance index. Separately, the peek clones a `String` for each of ~1.4 M instances; that is pre-existing, and returning `&str` would avoid it.
- **Suggested Fix**: wrap the peek error in `AtInstance { index: pos, class: "<unreadable>" }`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / list walker)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21.md`
