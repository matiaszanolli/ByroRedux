# #3225 — OBL-2026-08-20-D5-02: watal.md §4's per-game matrix states three Oblivion rows as SENTINEL that real data shows are AUTHORED — the false premise behind #3145 and #3222

**Issue**: #3225 — https://github.com/matiaszanolli/ByroRedux/issues/3225
**Finding ID**: `OBL-2026-08-20-D5-02`
**Severity**: MEDIUM
**Dimension**: 5 — WATAL design contract
**Audit**: `/audit-oblivion` — `docs/audits/AUDIT_OBLIVION_2026-08-20.md` (HEAD `bb0b92f2`, 2026-08-20 comprehensive suite)
**Labels**: medium, legacy-compat, bug
**Filed**: 2026-08-20 · `/audit-publish`

---

**Audit**: `/audit-oblivion` — `docs/audits/AUDIT_OBLIVION_2026-08-20.md` (Dim 5 — WATAL design contract), HEAD `bb0b92f2`
**Finding ID**: `OBL-2026-08-20-D5-02`

- **Severity**: MEDIUM
- **Status**: NEW

> **Labelled `bug`, not `documentation`, deliberately.** This is not stale prose — it is the design document's *contract table*, and two of its three wrong rows are the stated justification for a live defect that has already been filed. The false premise is the cause; correcting it is part of the fix, not cosmetic upkeep.

## Location

`docs/engine/watal.md:475-490` — the §4 "GameVariant doctrine for water" table, **Oblivion column**

## Description

`watal.md` §3 defines the contract that governs the whole layer: *"**SENTINEL** = explicit canonical game-default … **never** a render-time guess"*, and §4's matrix is the authoritative per-game statement of which fields each game authors.

Three of its Oblivion rows are wrong against vanilla `Oblivion.esm`, and **each wrong row is the stated justification for a live defect**:

| Row (`watal.md`) | Doc says (Oblivion) | Real `Oblivion.esm` | Defect it justified |
|---|---|---|---|
| `legacy water damage` (`:479`) | SENTINEL | **AUTHORED** — `FNAM` bit 0x01 on 5 records; damage `5000 / 65535 / 50 / 50` | **#3145** — all Oblivion lava harmless |
| `diffuse/normal texture` (`:483`) | SENTINEL `u32::MAX` -> procedural | **AUTHORED** — non-empty `TNAM` on **15 of 23** | **#3222** — diffuse bound as a normal map, 163 cells |
| `fog_near`/`fog_far` (`:480`) | SENTINEL 80/600 (short DATA) | **AUTHORED** on the 17 full-length (102 B) records; `decode_data_oblivion` reads them at `DATA[36]`/`[40]` | — (the row simply predates the Oblivion offset fix) |

The same `diffuse/normal texture` row also attributes `NNAM` to FO3/FNV and `TNAM` to **Skyrim** — but `TNAM` is *Oblivion's* field, and the row has no Oblivion entry for it at all. Two further rows are missing entirely: `MNAM` (material) and `SNAM` (sound).

## Evidence

- The `WATR` census in **#3223** (`OBL-2026-08-20-D3-01`): 23 records, `DATA` lengths `102x17, 86x2, 62x1, 42x2, 2x1`; `FNAM` bit 0x01 on 5.
- The `TNAM` listing in **#3222** (`OBL-2026-08-20-D5-01`): 15 of 23 author a non-empty `TNAM`, all of them reused architecture/landscape/dungeon albedo.
- For the fog row: `crates/plugin/src/esm/records/misc/water.rs:497-502` (`decode_data_oblivion`) reads `fog_near` at offset 36 and `fog_far` at 40 — the code already contradicts the doc.

Verified at HEAD: the three rows read exactly as quoted above.

## Impact

MEDIUM rather than LOW because this is the design document's **contract table**, and two of its three wrong rows encode exactly the false premise *"Oblivion authors nothing here"* that produced **#3145** (all Oblivion lava harmless) and **#3222** (diffuse bound as a normal map on 163 cells).

A reader checking whether the Oblivion water path is complete is told, **by the authority**, that it already is.

`watal.md` is also the single most-changed file in this delta (63 touches), so these rows were live-edited *around* without being re-checked.

## Related

- **#3145** — the `legacy water damage` row's live consequence
- **#3222** (`OBL-D5-01`) — the `diffuse/normal texture` row's live consequence
- **#3223** (`OBL-D3-01`) — the two rows the table is missing (`MNAM`, `SNAM`)
- **#3157** (`LC-D6-03`) — a **different set of rows in the same §4 table** (the *"WATR appearance payload"* row's FO3/FNV and FO76 columns). Same table, disjoint rows; fix them in one pass.
- **#3200** — the FO3/FNV column of the `legacy water damage` row is *also* false on data (`AUTHORED when FNAM bit 0x01 is set` — never set on any of 78 FNV or 53 FO3 records). Between this issue and #3200, **three of that one row's four cells are wrong.**

## Suggested Fix

Correct the three Oblivion rows and add the two missing ones (`MNAM` material, `SNAM` sound). Fold in #3157's corrections to the same table.

Then adopt the convention `/audit-esm` uses elsewhere: **cite the measured vanilla record count next to each AUTHORED cell**, so the table can be re-checked against data instead of re-asserted. The censuses in #3223, #3222, #3157 and #3200 already supply the numbers for all seven games.

## Completeness Checks
- [ ] **SIBLING**: every column of the `legacy water damage` row is re-checked, not just Oblivion (#3200 refutes the FO3/FNV cell too)
- [ ] **SIBLING**: #3157's corrections to the same §4 table land in the same pass
- [ ] **CANONICAL-BOUNDARY**: each corrected AUTHORED cell has a real parser arm, or is filed as a gap — the table must not be corrected into describing behaviour the code does not have
- [ ] **TESTS**: the per-game record census becomes checked-in ground truth (§9) so the row can be re-derived, not re-guessed
