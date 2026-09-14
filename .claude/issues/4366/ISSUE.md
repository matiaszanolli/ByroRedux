# #4366 — TD4-005: `_audit-validate.sh` checks only backticked tokens, so fenced layout maps are outside the gate

**Labels**: low, tech-debt, documentation, doc-rot
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4366

- **Severity**: LOW · **Dimension**: 4
- **Location**: `.claude/commands/_audit-validate.sh` (token extraction, ~`:115-120`) · **Status**: NEW · **Effort**: small · **Kind**: doc-rot (audit infrastructure)
- **Finding**: A prototype fence scan over all skill files (`os.path.exists` on path-prefixed tokens in fenced lines) returned 4 misses: 2 real (`_audit-common.md:63,99`) and 2 filename-template false positives. It is low-noise, and it would have caught TD3-003/TD4-003/TD4-004 and past #4121/#4244/#4020.
- **Suggested Fix**: Scan fenced lines for path-prefixed tokens as STALE (skipping `<…>` placeholders and trailing-`_` templates); route in-row basenames through the existing bare-basename advisory. Consider running the same scan over CLAUDE.md.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: If practical, a hygiene/gate check prevents this doc-rot class recurring
