# #3769 — CHAR-2026-08-30-D6-04: docs/feature-matrix.md's CHARAL section has no row or prose for the CREA stat model, three days after it shipped

**Repo**: matiaszanolli/ByroRedux · **Filed**: 2026-08-30 · **HEAD**: `64f64480`
**Labels**: low, character, doc-rot, documentation, game:fnv, game:fo3

---

**Audit**: `/audit-character` — `docs/audits/AUDIT_CHARACTER_2026-08-30.md` (Dimension 6 — Coverage, Documentation & Doctrine Drift), HEAD `64f64480`
**Finding ID**: `CHAR-2026-08-30-D6-04`

- **Severity**: LOW
- **Status**: NEW

## Location

`docs/feature-matrix.md:248-272` — the CHARAL section

## Description

#3390 added a fourth NPC stat model, `NpcStatModel::CreatureData`, populating SPECIAL + Health for 1,578 FNV and 533 FO3 `CREA` records from the record's own `DATA`.

The matrix's "NPC actor-value population at spawn" row still reads `✓ class auto-calc` for FO3/FNV, and the prose paragraph beneath enumerates the mechanisms — "class auto-calc", "Health only", "stored" — without mentioning creatures at all.

`grep -n 'CREA\|creature' docs/feature-matrix.md` finds only an unrelated physics row (`:159` NPC / creature physics).

## Evidence

- `crates/core/src/character/profile.rs:37-44` — `NpcStatModel::CreatureData`
- `crates/core/src/character/profile.rs:100` / `:115` — `creature_stats: NpcStatModel::CreatureData` on both Fallout profiles
- `crates/plugin/src/esm/records/actor_value_derive.rs:222` — `derive_creature_actor_values`

Landed in `a1327227`; the matrix section is unchanged since before it. Re-verified at HEAD.

## Impact

The matrix is this project's designated "what actually works per game" artifact and is documented as lagging the code, so a lag is reportable doc rot. Here it **under-reports shipped coverage** on the two reference titles — 2,111 actors' worth — which is the direction that causes duplicated work rather than false confidence, but is still wrong.

It also leaves no place to record the gap `CHAR-2026-08-30-D5-01` (#3762) identifies: creature attack damage parsed but unconsumed.

## Related

- #3484 (the same section's Skyrim population row, stale in the opposite direction — OPEN)
- #3390 (the `CREA` stat model)
- #3762 (`CHAR-2026-08-30-D5-01` — the unread `DATA.Damage`)

## Suggested Fix

Add a "Creature (`CREA`) actor-value population" row (FO3 ✓, FNV ✓, Oblivion ✗ — `CREA.DATA` layout unsourced, others n/a) and one prose sentence naming the model and what it deliberately omits (the three aggregate skills, and `DATA.Damage`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the neighbouring CHARAL rows, especially the Skyrim one tracked by #3484 — worth one edit pass)
- [ ] **TESTS**: N/A (documentation)
