# FO4-DIM4-05: EsmCellIndex.texture_sets populated but never consumed by byroredux/

State: OPEN

**From**: `docs/audits/AUDIT_FO4_2026-04-23.md` (Dim 4)
**Severity**: LOW
**Location**: `crates/plugin/src/esm/cell.rs:414` (map populated); no consumer in `byroredux/src/`

## Description

`parse_txst_group` at `cell.rs:1581-1642` extracts all 8 texture slots (TX00..TX07) + FO4 MNAM material path into `EsmCellIndex.texture_sets`. The doc comment (`cell.rs:407-414`) marks the field as infrastructure for "future REFR XTNM/XPRD overrides". Nothing in `byroredux/src/` reads this map — `rg texture_sets byroredux/` returns zero hits.

## Evidence

`rg -n "texture_sets|TextureSet" byroredux/` — no match (only an unrelated `components.rs` doc hit).

## Impact

No current bug (XTNM / XPRD are not parsed on the REFR side either — verified at `cell.rs:174-205` enumerating parsed REFR sub-records: XESP, XTEL, XPRM, XLKR, XRMR, XPOD, XRDS but not XTNM / XPRD). When XTNM parsing lands, the consumer-side wire is already there.

## Suggested Fix

None today. When REFR XTNM / XPRD sub-records are parsed (FO4-DIM6-02), route their TXST FormID through `EsmCellIndex.texture_sets` to produce a per-REFR material override.

**This issue is a marker** — closable once FO4-DIM6-02 lands and routes through `texture_sets`.

## Completeness Checks

- [ ] **TESTS**: Coverage arrives with FO4-DIM6-02.

## Related

- Blocks on: FO4-DIM6-02 (REFR override sub-records + texture_sets consumer).
