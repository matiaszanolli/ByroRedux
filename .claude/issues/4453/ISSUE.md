# D3-01: D3-01: FO76 and Starfield profile rows claim NpcStatModel::Stored with no capture support

- **Labels**: low,character,bug,game:fo76,game:starfield
- **Filed from**: docs/audits/AUDIT_CHARACTER_2026-09-19.md (/audit-character 2026-09-19)
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4453

---

**Source**: charal-fo76-ruleset.md:121-124 — "NPC SPECIAL storage for FO76: **not researched**"; the Starfield capture contains no NPC-stat-storage claim at all. The FO4 `Stored` row itself IS sourced (charal-fo4-ruleset.md:500-559), which is what makes the unqualified reuse look captured when it is not.

**Description**

Both unwired ruleset families nonetheless author a wired NPC stat model (`crates/core/src/character/profile.rs:151-165`, `npc_stats: NpcStatModel::Stored`). These profiles are selectable at parse time via HEDR detection (crates/plugin/src/esm/reader.rs:216/228 → parse.rs:87-88), so loading such a master would route `NPC_` records through `derive_stored_actor_values` — the FO4 PRPS/DNAM wire format — with no capture line saying FO76/Starfield records carry that layout (FO76's record formats restructure FO4's; Starfield's more so).

**Evidence**

Profile rows vs the two captures above; the FO76 doc header only says the FO4 population path is "implied" to be reused — a presumption, not a captured fact.

**Impact**

Low today — both games' rulesets are `None` and full support is out of the rollout — but the row is a silent presumption a future FO76/Starfield wiring would inherit as if verified.

**Related**

CHARAL audit 2026-09-19 coverage matrix (FO76/Starfield rows).

**Suggested Fix**

Either capture a source for each row (FO76: xEdit wbDefinitionsFO76.pas NPC_ properties layout; Starfield: equivalent) or make the rows `NpcStatModel::None` with the same "blocked, not forgotten" comment pattern the Oblivion `RulesetBuilder::None` arm already uses (profile.rs:51-57).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_CHARACTER_2026-09-19.md` (finding D3-01, /audit-character 2026-09-19, HEAD `479163836`).*