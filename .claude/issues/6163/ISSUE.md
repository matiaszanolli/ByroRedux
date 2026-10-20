# TD3-2026-10-19-03: ROADMAP "Growth discipline - Tripwire today" names three files that are now 136-224-line hubs and understates five others (carried from 10-15 TD3-09, worse)

Labels: low,documentation,doc-rot,tech-debt
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- Severity: LOW · Effort: trivial · Status: STILL LIVE (worsened by #5800/#5801/#5804)
- Location: `ROADMAP.md:446-452`
- Evidence: `wc -l` today: `translate/effects.rs` 177 (claimed 10 117), `fragment/vm.rs` 136 (6 215), `condition.rs` 224 (3 691) - split into `translate/effects/*`, `fragment/vm/*`, `condition/*`. Others drifted: `physics/world/mod.rs` 5 491 (5 209), `material_translate.rs` 4 468 (4 422), `inventory.rs` 4 337 (4 202), `volumetrics.rs` 4 281 (4 168), `physics/water.rs` 4 093 (3 963), `esm/records/misc/world.rs` 3 508 (3 508). The "re-measured 2026-10-15" stamp is four days old and wrong for 8 of 9 rows.
- Proposed fix: regenerate the list from `git ls-files '*.rs' | xargs wc -l` with the >3500 cut (today's over-the-line production files: those six; test-only files `shader_contract_tests.rs` 7 606, `fragment/tests.rs` 6 595, `parse_real_esm.rs` 6 082, `vm/tests.rs` 5 867, `effects/tests.rs` 5 337, `extensions/tests.rs` 4 863 are the test bucket), or replace the numbers with the command.

Related: #5833 (closed; the tripwire figures drifted again after the splits).

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in the neighbouring docs / skills
- [ ] `.claude/commands/_audit-validate.sh` still passes after the edit

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
