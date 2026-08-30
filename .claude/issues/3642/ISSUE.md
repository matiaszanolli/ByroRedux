# #3642: FO4-2026-08-30-D9-01: ROADMAP's 100.00% / 235,082 FO4 headline conflates the .nif and distant-LOD halves

**Source**: `docs/audits/AUDIT_FO4_2026-08-30.md` — Dimension 9
**Severity**: LOW
**Location**: `ROADMAP.md` — the Fallout 4 compat-matrix row (and the per-game integration-sweep totals line)

## Description

ROADMAP's FO4 row reads `**100.00%** (**235 082 / 235 082**)` with no split between the
`.nif` corpus and the `.bto`/`.btr` distant-LOD corpus. It is correct as a **parse**
statistic, but is being read as "renders correctly" — and 9,073 of those files are LOD meshes
in which 90.01% of shapes get fabricated normals.

## Evidence

Current `ROADMAP.md` (verified 2026-08-30) — the Fallout 4 row states
`100.00% (235 082 / 235 082) · recover 100%`; the per-game sweep totals line repeats
`FO4 235 082`. Neither mentions the LOD half.

Re-measured exactly this run: **226,009 `.nif` + 9,073 `.bto`/`.btr` = 235,082**, 0 parse
failures, 0 truncations, 0 `NiUnknown` recovery blocks, 115 distinct on-disk block-type
strings all dispatching. The parse claim is exact and reproduces.

But over the `.bto`/`.btr` half: **14,054 of 15,614 LOD shapes (90.01%) carry no authored
normals** (attribute masks `0x0003` VERTEX|UVS = 8,271 shapes and `0x0001` VERTEX-only =
5,783), so the importer substitutes a constant `[0,1,0]`. Near-corpus contrast: only 62 of
130,480 imported `.nif` meshes (0.05%) hit the same fallback. That normal-synthesis gap is
tracked separately as #3541.

## Impact

The headline vouches for geometry whose attributes the importer is inventing. A reader
scanning the compat matrix for "is FO4 done" gets a 100% that is true of parsing and false of
rendering fidelity on 9,073 files.

## Suggested Fix

Split the compat-matrix cell into its `.nif` (226,009) and LOD (9,073) halves, or footnote
the LOD normal gap on that row, so the parse-rate claim does not carry an unearned rendering
implication.

## Related

#3541 (the cross-cutting missing `synthesize_normals` — FO4's measured LOD exposure is the
90.01% above), #3466 (the 2026-08-29 corpus-gate widening that produced the 235,082 figure).

## Completeness Checks
- [ ] **SIBLING**: the same headline appears twice in `ROADMAP.md` (the compat-matrix row and the per-game sweep totals) — correct both
- [ ] **TESTS**: doc-only; `parse_rate_fo4_all_meshes` already pins the 235,082 parse figure and stays valid
