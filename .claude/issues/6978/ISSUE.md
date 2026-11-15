# #6978: TD4-2026-11-15-03: `_audit-common.md` hub-size range "~100-230 LOC" no longer holds

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6978
- **Labels**: low,documentation,doc-rot,tech-debt
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW. **Dimension**: 4. **Effort**: trivial. **Labels**: `tech-debt`, `doc-rot`. **Existing**: none.
- **Evidence**: `.claude/commands/_audit-common.md:9` ("**split hubs** (a ~100-230-LOC `.rs` over the bulk in its directory"). Hubs listed there: `crates/scripting/src/condition.rs` 283 lines, `byroredux/src/cell_loader/exterior.rs` 247, `crates/plugin/src/esm/cell/mod.rs` 203, `translate/effects.rs` 177, `fragment/vm.rs` 136, `save_io.rs` 108. The upper bound is exceeded by two of six (`condition.rs` +23 %).
- **Fix**: "~100-290" or drop the number; the point of the bullet (scope to the `<stem>/` directory) does not depend on it. The other numeric claims in the file hold (`crates/platform` 60 LOC = 8 + 52, `cxx-bridge` 9-line C++ stub = `native_utils.cpp` 9 lines, `systems.rs` is a bare re-export at 125 lines).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
