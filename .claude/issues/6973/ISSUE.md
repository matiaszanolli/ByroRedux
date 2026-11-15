# #6973: TD3-2026-11-15-08: ROADMAP.md still lists #6608 (Skyrim exterior->interior device loss) as an open known issue; it closed 2026-11-05

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6973
- **Labels**: low,renderer,game:skyrim,documentation,doc-rot,tech-debt
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW. **Dimension**: 3. **Effort**: trivial. **Labels**: `tech-debt`, `doc-rot`. **Existing**: none.
- **Evidence**: `ROADMAP.md:638-640` ("Skyrim exterior -> interior door loses the Vulkan device (#6608, 2026-10-31) ... not bisected"). `gh issue view 6608`: CLOSED COMPLETED 2026-11-05, "Fixed in 7c72057be" (a loading-stage frame stops dispatching stale ground-cover scatter). The section's own rule (`ROADMAP.md:553`) is "each closed item is removed at the session close that closes it". The closing comment mentions "remaining P1 flakiness"; if that is still wanted, restate it as its own item with its own tracker.
- **Cross-check of the other 19 issue refs in the Known Issues block** (`ROADMAP.md:553-700`): the 3 open (#3398, #5764, #6350) are correctly described; the other 16 closed ones are cited as fixed or historical. Milestone rows (M39, M24.2, M42, M48, M65 trackers #6626/#6635/#6634/#6641/#6908) match open trackers.
- **Fix**: delete the bullet (or rewrite per above).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
