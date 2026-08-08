# REN-D21-2026-08-07-03: glass()'s alpha:0.25 is not reachable by mat.list's own advertised round-trip

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2515
**Finding ID**: REN-D21-2026-08-07-03 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 21 — Cornell Harness
**Location**: `byroredux/src/cornell.rs::glass`
**Status**: NEW (documentation/observability nit; not a rendering defect)

## Description
`glass()` sets `alpha: 0.25` with a doc comment stating it is "currently unconsumed downstream". That is accurate for `taa.comp`/`composite.frag`, but the value *does* reach `GpuMaterial.material_alpha` through `to_gpu_material` and participates in `hash_gpu_material_fields`, i.e. it forces the two glass probes into distinct material-table slots from an otherwise identical opaque dielectric. The comment reads as "inert", which invites a future reader to treat the field as free to change; it is not free with respect to dedup identity.

## Evidence
`material.rs` `hash_gpu_material_fields` writes `mat.material_alpha.to_bits()`; `MaterialTable::intern_by_hash` keys on that hash.

## Impact
Cosmetic/doc only today. Matters if someone later uses the Cornell glass probes to measure `MaterialTable` dedup ratio (`ctx.scratch`, #780/PERF-N1) and is surprised by the extra slot.

## Related
#676 / DEN-6 (cited in the same doc comment).

## Suggested Fix
Amend the `glass()` doc comment to say the value is unconsumed *by the composite/TAA passes* but is part of the material dedup key.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
