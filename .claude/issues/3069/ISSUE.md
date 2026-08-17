# SKY-2026-08-16-D3-01: expand_leveled_form_id never reads TES5 LVLF bit 0x04 — 289 NPCs wear one piece of a four-piece set

**Issue**: #3069
**Severity**: HIGH
**Labels**: `high,import-pipeline,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_SKYRIM_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SKYRIM_2026-08-16.md` (Dimension 3 — leveled lists / NPC equip).

**Location**: `crates/plugin/src/equip.rs`:363-373 · flag capture at `crates/plugin/src/esm/records/container.rs`:87, :189

## Description

`expand_leveled_form_id` never reads TES5 `LVLF` bit `0x04` — the "Use All" flag — so a leveled list that should yield **every** entry yields only one.

## Impact

**289 vanilla Skyrim NPCs spawn wearing one piece of a four-piece armour set.** The list is authored to grant the whole set; the expander picks a single entry as if it were a random-one-of list.

Visually this reads as an NPC in partial armour — cuirass but no gauntlets/boots/helm — which is indistinguishable from a missing-asset problem rather than a list-expansion one.

## Suggested Fix

Read `LVLF` bit `0x04` at the capture site (`container.rs`:87/:189 already parse the flags byte) and have `expand_leveled_form_id` return all entries when it is set, rather than selecting one.

Verify against the other `LVLF` bits at the same time — if `0x04` was missed, its neighbours are worth checking.

## Related

- #2986 (ESM-D7-01) — the other "a per-game field is parsed but never consulted" finding this sweep
- `crates/plugin/src/equip.rs` (xEdit-derived biped-slot constants — the same module)

## Completeness Checks
- [ ] **SIBLING**: The other `LVLF` bits checked for the same omission
- [ ] **PER-GAME**: The flag's meaning verified for TES5 specifically, not assumed uniform across games
- [ ] **REAL-DATA**: Verified against the 289 affected NPCs, not a synthetic list
- [ ] **TESTS**: A regression test expands a Use-All list and asserts every entry is returned

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3069 --json state` when live state is needed.*
