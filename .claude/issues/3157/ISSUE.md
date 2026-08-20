# #3157 — LC-D6-03: watal.md §4's per-game payload table names the wrong sub-record for FO3/FNV and the wrong size for FO76 — the majority water path is undiscoverable from the spec

**Finding**: LC-D6-03
**Labels**: documentation, low, legacy-compat
**Filed**: 2026-08-20 · `/audit-publish` · HEAD `bb0b92f2`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/3157

---

- **Severity**: LOW
- **Dimension**: LEGACY_COMPAT Dim 6 — per-game translation-survey gaps, spec vs. corpus
- **Location**: `docs/engine/watal.md:476` (§4 GameVariant table, row *"WATR appearance payload"*); §2 *"Decode + translate"*
- **Status**: NEW

## Description

The §4 table row states the per-game payload as:

```
| WATR appearance payload | DATA ~102 B | DATA 186/196 B (opaque 16 B prefix) | DNAM 228/232 B; FO4/FO76 201 B; Starfield 152 B+ | `water.rs:30-61` |
```

Two of those are wrong against vanilla data.

**(a) FO3/FNV's dominant carrier is `DNAM` at 196 bytes, not `DATA`.** `DATA` at 186 B covers only 11/53 FO3 and 8/78 FNV records. The string "196 B" is attributed to `DATA` when 196 is the **`DNAM`** size. Because the table never names `DNAM` in the FO3/FNV column at all, a reader cannot discover from the spec that the majority path even exists — which is the documentation half of the 52-byte-prefix gap in #3107.

**(b) FO76 is 148 bytes, not the 201 the table shares with FO4.**

## Evidence

Sub-record census over all seven installed masters (independent GRUP walks, TES4 using the 20-byte record header and the other six the 24-byte header):

```
Oblivion.esm    WATR 23   DATA 102×17, 86×2, 62×1, 42×2, 2×1      (no DNAM)
Fallout3.esm    WATR 53   DNAM 196×41, 184×1   DATA 186×11, 2×42
FalloutNV.esm   WATR 78   DNAM 196×69, 184×1   DATA 186×8,  2×70
Skyrim.esm      WATR 34   DNAM 228×31, 232×3   DATA 2×34
Fallout4.esm    WATR 42   DNAM 201×40, 188×2   DATA 0×42
SeventySix.esm  WATR 47   DNAM 148×47          DATA 0×47
Starfield.esm   WATR 15   DNAM 152×15          DATA 0×15
```

The **code** is consistent with the corpus even where the doc is not: `decode_dnam_fo76` reads no offset past 112, comfortably inside 148 bytes, and every read is bounds-checked through `read_f32_at`. A candidate finding that FO76's decoder over-reads its 148-byte payload was chased and **disproved** on real data — only the table is wrong. (`decode_dnam_fo76` is separately wrong about *which layout* it decodes; that is #3106, a different defect.)

## Impact

Documentation only. It matters because §4 is the artefact an implementer consults to decide whether a per-game arm is needed, and it currently understates both which sub-record carries FO3/FNV water and how much of it exists — i.e. it actively conceals the majority path that #3107 is about.

## Related

- #3107 — the FO3/FNV `DNAM` (196 B) path stops decoding at byte 52. This is that finding's documentation half: the table's FO3/FNV column marks `fog_near`/`fog_far`, `wave_amplitude`/`wave_frequency`, noise UV scales, underwater fog and the specular tail as **AUTHORED**, but for 88% of FNV and 77% of FO3 records those resolve to SENTINEL — so the "canonical is a genuine superset" claim silently fails for the majority of Fallout water.
- #3106 — `decode_dnam_fo76` decodes a layout FO76 does not use.
- #3154 (LC-D5-02) — §3 contract drift in the same document.
- #2790 (CLOSED) — covered a different `watal.md` §2 paragraph.

## Suggested Fix

Rewrite the *"WATR appearance payload"* row from the census above: name `DNAM` explicitly in the FO3/FNV column with its 196-byte size and its share of the corpus, and split FO76 (148 B) out of the FO4 cell.

Then add the census itself to §9 as standing ground truth, so the row can be **re-checked rather than re-guessed** the next time a decoder is written against it.

While there, re-check the §4 rows the corpus contradicts for the same reason — in particular the *"legacy water damage"* Oblivion column, which the `Oblivion.esm` scan disproves (filed separately), and the *"physical normal magnitude … Skyrim DNAM[92]"* source cell, which #3104 refutes.

---
*Filed from `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-20.md` (LC-D6-03). Verified against HEAD `bb0b92f2` — `watal.md:476` reads exactly as quoted.*

## Completeness Checks
- [ ] **SIBLING**: every other `watal.md` §4 row whose "Source field" cell cites a byte offset re-checked against the census — several are refuted by #3104–#3110
- [ ] **DOC**: the census lands in §9 as re-checkable ground truth, not only as a corrected table row
