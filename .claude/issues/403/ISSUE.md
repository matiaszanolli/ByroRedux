# FO4-D1-C1: wetness.unknown_1 BSVER gate excludes 130 — 1.88M BSLightingShaderProperty under-reads

**Issue**: #403 — https://github.com/matiaszanolli/ByroRedux/issues/403
**Labels**: bug, nif-parser, critical, legacy-compat

---

## Finding

`BSLightingShaderProperty::parse` at `crates/nif/src/blocks/shader.rs:528` gates the `unknown_1` f32 read on `bsver > 130`:

```rust
let unknown_1 = if bsver > 130 {
    stream.read_f32_le()?
} else {
    0.0
};
```

Per nif.xml `#BS_FO4#` condition (BSVER ≥ 130 AND BSVER < 155), plus cross-refs in `bgsm-converter` and `nifly`, the field is present for the **whole 130–139 range**, including BSVER=130 (vanilla FO4 ship stream). Current code skips 4 bytes for every FO4 mesh that carries a wet material, desyncing every subsequent field.

## Evidence — live sweep

Dim 5 of the 2026-04-17 FO4 audit swept all 8 FO4 main + DLC mesh archives (226,009 NIFs) and logged **1,876,931 `block_size` recovery warnings**, all on `BSLightingShaderProperty`, all **exactly 4 bytes short**:

```
'BSLightingShaderProperty': expected 140, consumed 136
'BSLightingShaderProperty': expected 146, consumed 142
```

Coverage: MeshesExtra 100%, Meshes 97%, all DLC main archives 99.5%+. The `block_size` recovery path silently seeks past the missing bytes, so the parse rate stays at 100% while every lit mesh loses a material field.

## Impact

- Every FO4 lit mesh's wet-shader data desyncs. Any downstream code reading subsurface / rimlight / backlight / parallax-scale reads zero-defaults instead of vendor-authored values.
- Dim 3 H-01 cross-linked this as the single root cause of observed FO4 visual regressions.
- Metric-vs-reality divergence: `clean %` reads 100 but every lit mesh is corrupt.

## Fix

One-line change plus a fixture test:

```rust
let unknown_1 = if (130..155).contains(&bsver) {
    stream.read_f32_le()?
} else {
    0.0
};
```

Add a fixture test built at BSVER=130 that round-trips `wetness.unknown_1`.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Re-verify all BSVER gates in `shader.rs` — the H-1 "flag-pair vs CRC-count" BSVER 131 gap (Dim 1 H-1) is the sibling anomaly; check lines 411 and 427 align on the same boundary.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Build a synthetic NIF at BSVER=130 with non-zero `wetness.unknown_1`; assert it round-trips. Also assert the sweep warning count drops to zero on vanilla archives.

## Source

Audit: `docs/audits/AUDIT_FO4_2026-04-17.md`, Dim 1 C-1 (cross-linked from Dim 3 H-01 and Dim 5 H-1).
