# #3511: FO3-2026-08-27-D3-03: the /audit-fo3 skill's Dimension-3 checklist asks auditors to verify a REFR texture-overlay path FO3 never authors

**Labels**: low, tech-debt, documentation, doc-rot, game:fo3, legacy-compat
**Audit**: `docs/audits/AUDIT_FO3_2026-08-27.md`

---

Source: `docs/audits/AUDIT_FO3_2026-08-27.md` — finding `FO3-2026-08-27-D3-03` (LOW, Dimension 3 — audit infrastructure / premise correction).

## Location
- `.claude/commands/audit-fo3/SKILL.md:104` — Dimension 3, "REFR per-instance texture overrides (XATO/XTNM/XTXR — #584)"
- against `crates/plugin/src/esm/cell/walkers.rs` — the `b"XATO"` arm and its provenance caveat (~L855-900)

## Description
The skill instructs: *"FO3 cell REFRs can carry per-instance texture-set overrides … Confirm FO3 overlays produce distinct resolved paths, not a collapse to the base mesh material. Regression tests: `byroredux/src/cell_loader/refr_texture_overlay_tests.rs`."*

The parser's own comment at the XATO arm already records the opposite, and has since #1887:

```rust
// On FONV, XATO is the Activation-Prompt subrecord
// (grouped with SCRV/SCVR/SLSD script-vars), a string,
// not a FormID. FO3 REFRs never carry XATO.
```

## Evidence
Two independent measurements agree. Through the engine's own parser, all 566 642 indexed FO3 REFRs carry **0** `alt_texture_ref` / `land_texture_ref` / `texture_slot_swaps`. A raw byte-level sub-record scan over every `REFR`/`ACHR`/`ACRE` in `Fallout3.esm` (573 610 records, decompressing compressed records) finds **0** `XATO`, **0** `XTNM`, **0** `XTXR`. FO3's 243 TXST records are reached through `LTEX.TNAM`, not through REFR overlays.

## Impact
The skill sends every FO3 audit run to verify a behaviour with no vanilla instances, and cites an FO4-shaped regression-test file as the FO3 evidence. This is the exact failure mode #3101 was filed for (`BSSegmentedTriShape` named as an FO3 divergence surface that FO3 authors zero of). It costs auditor time and, worse, invites a "verified working" conclusion drawn from FO4 fixtures.

## Related
#3101 (same class, closed); #1887 (the parser-side provenance caveat). Sibling in spirit to #3422 (the `/audit-fnv` skill's Dimension-1 `_far.nif` premise), which is the FNV analogue on a different skill file and dimension.

## Suggested Fix
Replace the FO3 Dim-3 XATO bullet with the measured fact and point it at the real FO3 TXST consumer (`LTEX.TNAM` → `landscape_texture_sets`), the way the skill's own `BSSegmentedTriShape` note was corrected.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the other `/audit-fo3` dimension checklists and the sibling per-game skills that inherited the same bullet
