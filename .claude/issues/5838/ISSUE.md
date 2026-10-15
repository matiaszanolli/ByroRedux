# TD4-2026-10-15-01: Three deleted/historical names are still backticked in skills (the real part of the advisory list)

Labels: low,tech-debt,documentation,doc-rot

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD4-2026-10-15-01`).

- **Severity**: LOW
- **Dimension**: 4 - Audit-Finding Rot
- **Location**: `.claude/commands/audit-oblivion/SKILL.md:101` (`inherit_all_up_chain`), `:103` (`env_translate.rs`), `.claude/commands/audit-nifal/SKILL.md:90` (`SourceSoftAdditive`)
- **Status**: NEW as an issue — carry-over of prior-report TD4-2026-10-12-02 (never published as its own issue; the validator half is Existing #5607)
- **Age**: `inherit_all_up_chain` deleted by `65cadde2f` (#5516, 2026-10-12); `env_translate.rs` split by `c140cb48e` (#5584, 2026-10-12); the sync touched both lines and kept the backticks
- **Effort**: trivial
- **Description**: `_audit-common.md` Path-Reference Convention says deleted things go in italics. `inherit_all_up_chain` ("deleted the dead ... copy") and `env_translate.rs` ("split `env_translate.rs` into ...") are history; `SourceSoftAdditive` is a CDB `BlendingMode` string value, not a symbol. The validator prints all three as advisories on every run, which is exactly the signal that lets a future renamed guard hide.
- **Evidence**: `/tmp/audit/tech-debt/d4_validate.txt` lines 8 and 82-94; `grep -rn inherit_all_up_chain crates byroredux` empty; `git ls-files | grep env_translate` shows only the `env_translate/` dir files.
- **Impact**: advisory noise masks real renames; convention drift.
- **Related**: #5607, TD4-2026-10-12-02.
- **Suggested Fix**: italicise the three names (or drop backticks); re-run `_audit-validate.sh`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
