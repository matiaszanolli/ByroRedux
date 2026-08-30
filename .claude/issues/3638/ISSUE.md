# #3638: FO4-2026-08-30-D6-02: TXST DecalData is parsed and never read — 303 vanilla DODT payloads dropped while the DNAM sibling is consumed

**Source**: `docs/audits/AUDIT_FO4_2026-08-30.md` — Dimension 6
**Severity**: LOW
**Location**: `crates/plugin/src/esm/cell/support.rs` (the DODT write), `crates/plugin/src/esm/cell/mod.rs` (`decal_data` field) — no consumer

## Description

TXST `DecalData` (DODT) is parsed into `decal_data` and never read anywhere in the tree.

## Evidence

Verified 2026-08-30:

```
$ grep -rn decal_data --include='*.rs' .
crates/plugin/src/esm/cell/support.rs:555   <- the single write
crates/plugin/src/esm/cell/mod.rs:881       <- the field declaration
crates/plugin/src/esm/cell/tests/txst.rs    <- 5 test lines
```

One write, one declaration, five test lines, **zero consumers**.

MEASURED: **303 vanilla DODT payloads** across the masters — Fallout4 207/382, DLCCoast
31/37, DLCNukaWorld 63/73, DLCRobot 1/1, DLCworkshop03 1/1 (the skill's 207/382 figure is
exactly confirmed, all 36 bytes). Each carries min/max decal width + height, depth,
shininess, parallax scale + pass count, flags and RGB.

The **DNAM sibling from the same #813/#814 pair is consumed** —
`byroredux/src/cell_loader/refr.rs` reads `TXST_FLAG_MODEL_SPACE_NORMALS` — which makes the
asymmetry read as an oversight rather than a deliberate deferral.

## Impact

303 authored decal parameter sets are discarded, so FO4 decals fall back to whatever the
engine's defaults are rather than their authored size/depth/shininess/parallax. Low, because
decals are a small visual class — but the "parsed then dropped" defect class that #813 closed
at the parser is still open at the consumer for this half of the pair.

## Suggested Fix

Either wire `decal_data` into the decal material/placement path alongside the DNAM flags, or
add an explicit deferral comment at the field so the next reader does not mistake a live
field for a consumed one.

## Related

#813 / #814 (TXST DODT + DNAM).

## Completeness Checks
- [ ] **SIBLING**: the DNAM half is already consumed at `cell_loader/refr.rs` — match its plumbing rather than adding a second path
- [ ] **TESTS**: a regression test pins one of the 303 vanilla DODT payloads reaching whatever consumer is added
