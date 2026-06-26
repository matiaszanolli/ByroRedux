# TD6-001: condition.rs docstrings claim GetFactionRank/HasPerk stubbed (both implemented + tested)

_Filed 2026-06-26 as #1757 from docs/audits/AUDIT_TECH_DEBT_2026-06-26.md (immutable snapshot; query `gh issue view 1757` for live state)._

**Severity**: MEDIUM (stale doc that misleads a stub audit) · **Dimension**: 6 — Stub & Placeholder (doc-rot subspecies)
**Location**: `crates/scripting/src/condition.rs:24-39` (header table) · `:71-76` (GetFactionRank doc) · `:82-86` (HasPerk doc)
**Status**: NEW · **Audit**: TD6-001
**Note**: no `scripting` domain label exists in the repo — filed with `tech-debt,documentation` only.

## Description
The condition-evaluator docstrings + header table claim a stubbed / 6-function status the implementation has outgrown (one-month divergence: docstrings `ea9d0cfa` 2026-05-23, contradicting impls `f73c6fd7` 2026-06-24):

- **GetFactionRank** doc (73-75): *"Stubbed today: always returns -1 … until a faction-membership component lands."* — but the impl (309-321) reads `world.get::<FactionRanks>(entity).and_then(|f| f.rank(...))`, tested by `get_faction_rank_reads_membership`. The component landed.
- **HasPerk** doc (82-85): *"Stubbed today: always returns 0.0 until a perk-list component lands."* — but the impl (345-366) reads `PerkList` via `FormIdPool`, tested by `has_perk_checks_actor_perk_list`.
- **Header table** (25-35): advertises *"6 representative functions"* and lists 6 rows, but `from_index` (96-107) maps **7** — `GetStageDone` (idx 59) was added (impl 292-308) and is absent from the table.

## Impact
An auditor reading these docstrings concludes GetFactionRank/HasPerk are still stubs and re-files or mis-scopes #1316. The 6-vs-7 undercount compounds it. (This Dim-6 sweep's own premise was nearly tripped by it.)

## Suggested Fix
1. Rewrite the GetFactionRank/HasPerk docstrings to describe the real lookups (−1 / 0.0 are now sentinel / not-held results, not stub returns).
2. Header table: "6 representative functions" → "7"; add `| 59 | GetStageDone | QuestStageState |`.

## Completeness Checks
- [ ] **SIBLING**: every `from_index` variant has an accurate docstring + a table row
- [ ] **TESTS**: `get_faction_rank_reads_membership` / `has_perk_checks_actor_perk_list` still pass (they pin the real impls)
