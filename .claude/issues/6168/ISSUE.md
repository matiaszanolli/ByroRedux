# TD3-2026-10-19-08: docs/engine names renamed/split symbols and paths as live

Labels: low,documentation,doc-rot,tech-debt
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- Severity: LOW · Effort: trivial · Status: NEW
- Evidence / fix:
  - `scripting.md:2282` `QuestCommandHost` -> `ObScriptCommandHost` (`obscript_quests.rs:376`).
  - `archives.md:266,454` `linear_size_for` -> `pitch_or_linear_size_for` (`crates/bsa/src/ba2.rs:1187`).
  - `m47-0-design.md:113` `quest_advance_on_activate_system` -> `quest_advance_system` (`papyrus_demo/quest_advance.rs`); doc is retained history, so just rename in the table.
  - `boot.rs` (now `boot/mod.rs`): `launcher.md:618`, `npc-spawn-ai-packages.md:16`, `exterior-readiness-plan.md:712`, `charal-oblivion-ruleset.md:416`; `streaming.rs` (now `streaming/mod.rs`): `coordinate-system.md:341`. `_audit-validate.sh` lists these as deleted-basename advisories and no one clears them.
  - The other ~30 basename advisories are legitimate (data files, struck-through, rapier's `sap_axis.rs`) - italicise to retire them.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in the neighbouring docs / skills
- [ ] `.claude/commands/_audit-validate.sh` still passes after the edit

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
