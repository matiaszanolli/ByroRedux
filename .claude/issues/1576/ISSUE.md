**Severity**: LOW · **Dimension**: SF ESM Resolve-Rate
**Location**: `crates/plugin/src/esm/cell/support.rs:38-160` (only top-level `MODL` is read)
**Source**: `docs/audits/AUDIT_STARFIELD_2026-06-14.md` (SF-D4-03)

## Description
`build_static_object_from_subs` extracts the model only from a top-level `MODL` subrecord (`support.rs:41`). Some Starfield STAT/BNDS/ACTI/ARMO records put the model reference inside a `BFCB`-wrapped component, so they return `None` and their REFRs drop.

## Evidence
`STAT 00000021 subs: EDID OBND ODTY OPDS BFCB BFCE FLLD PRPS DNAM` (no MODL); `BNDS 000001F9 subs: EDID OBND ODTY DNAM(28) MNAM(4)`. Counts: STAT 44/2, BNDS 60/2, ACTI 33/11, ARMO 3/1 — ~140 REFRs (~0.5% of cell).

## Impact
Small. The two unresolved STAT forms are very low FormIDs (0x21/0x43 — likely default/template/marker statics); BNDS is bendable-spline (needs a generator). Tail content; no structural architecture lost.

## Related
SF-D4-01 (shares the `BFCB` component-block walker need).

## Suggested Fix
When SF-D4-01's `BFCB` component walker lands, reuse it to recover a model reference for STAT/ACTI/ARMO. BNDS needs a dedicated spline-mesh generator — track separately.

## Completeness Checks
- [ ] **SIBLING**: Reuses the exact `BFCB`/`BFCE` walker from SF-D4-01 (not a second copy); STAT/ACTI/ARMO all route through it
- [ ] **TESTS**: A test pins a model-less-`MODL` STAT form recovering its model ref from the `BFCB` block
