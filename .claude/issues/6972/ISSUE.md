# #6972: TD3-2026-11-15-07: `crates/nif/src/anim/keys.rs:201` cites the deleted `byroredux/src/cell_loader/euler.rs`

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6972
- **Labels**: low,animation,nif-parser,documentation,doc-rot,tech-debt
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW. **Dimension**: 3. **Effort**: trivial. **Labels**: `tech-debt`, `doc-rot`. **Existing**: none.
- **Age**: 13647f400 (2026-11-06, #6713) deleted `euler.rs`, moving the helpers to `byroredux/src/zup_yup.rs`.
- **Evidence**: `crates/nif/src/anim/keys.rs:199-202` (comment "`--rotation-mode 3` (`byroredux/src/cell_loader/euler.rs`), a non-shipping diagnostic"); `--rotation-mode N` is documented at `byroredux/src/zup_yup.rs:26`. The deleted-file sweep (6 files deleted since 10-31) found this as the only live mention of a deleted path; `audit-legacy-compat/SKILL.md:31` names it correctly as moved.
- **Fix**: `byroredux/src/zup_yup.rs`. The same 12-line block is also history narration ("the prior `euler_to_quat_wxyz` composed...", #2434), see TD3-08; rewrite it once to state the rule (Gamebryo Euler is CW-positive XYZ, routed through `euler_zup_to_quat_yup`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
