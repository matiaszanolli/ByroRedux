# TD3-2026-10-25-04: status floors still say the six 10-24 AI package procedures are parse-only (ROADMAP is updated, feature-matrix and one ROADMAP bullet are not)

Labels: low, ai, doc-rot, tech-debt, documentation
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6375
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW (the matrix is the "status floor"; no audit misled yet)
- **Dimension**: 3
- **Effort**: trivial
- **Location**: `docs/feature-matrix.md:108-113` ("The remaining 7 procedures (Find/Accompany/UseItemAt/Ambush/FleeNotCombat/CastMagic/UseWeapon) are parse-only"), `docs/feature-matrix.md:354` (gap row "Remaining PACK procedures (...)"), `ROADMAP.md:99-100` ("7 of ~17 package procedures need subsystems that do not exist (item use, package combat, magic)"). `ROADMAP.md:61-62` and `:296` are correct (only CastMagic open).
- **Age**: `b540b104a` Flee, `10f8cbb91` UseWeapon, `4b31a415c` Accompany, `9aaf5e1cf` Ambush, `7fded9423` UseItemAt, `fcd1cce4a` Find (all 2026-10-24)
- **Related**: `byroredux/src/npc_spawn/ai_package.rs:3470` test comment, see TD3-2026-10-25-09.
- **Fix**: Replace with "only CastMagic (no FO3/FNV package uses it) and Find for non-item object types remain; UseWeapon declines unmodelled conditions", mirroring `ROADMAP.md:296`.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD3-2026-10-25-04`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
