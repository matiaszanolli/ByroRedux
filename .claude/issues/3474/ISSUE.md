# #3474 — NIF-2026-08-27-D5-01: the `starfield_tail` doc comment claims 38 B; the archive it cites now yields 30 B

Source: `docs/audits/AUDIT_NIF_2026-08-27.md`
Filed: 2026-08-27 via `/audit-publish`
Labels: low, nif-parser, nif, documentation, doc-rot, game:starfield

---

Audit: `docs/audits/AUDIT_NIF_2026-08-27.md` — Dimension 5 (Collision & Shader Block Parsing). Severity **LOW** (doc-rot on a reverse-engineering record). Game: **Starfield** (`bsver >= 172`).

## Location
`crates/nif/src/blocks/shader.rs:754-763` (the `starfield_tail` field doc; re-verified at publish time — the "38 B = 9× f32 + 2 B" wording is still there).

## Description
The field doc records the #1606 byte audit as

```
/// `Starfield - LODMeshes.ba2` as **38 B = 9× f32 + 2 B**, byte-identical
/// across all 26 LOD instances (`[2.0, 3.0, 0.1, 0.0, 0.02, 0.0289,
/// 0.095, 0.003, 1.0, 0x00, 0x00]`), but **undocumented in nif.xml**
```

#2622 subsequently moved the 4-float `BSSPLuminanceParams` quad into the Starfield decode path (and `read_wetness_block` stops 8 bytes earlier for it), which shortens the residual tail by exactly 8 bytes. The doc was not updated, so the recorded byte pattern no longer describes what the field holds.

## Evidence
`BSLightingShaderProperty` census over `Starfield - LODMeshes.ba2` — **the very archive the doc cites** — plus `Meshes02.ba2`:

```
files=27087
shader_type=0	tail_len=0	count=48877   (material-reference stubs)
shader_type=0	tail_len=30	count=26      (inline-authored)
```

The instance count is still exactly **26**, matching the doc; the length is **30 B**, not 38. Independently confirmed over `FaceMeshes.ba2` + `Meshes01.ba2` (32,340 files): 1,879 inline blocks, `tail_len = 30` uniformly, 201,635 stubs at `tail_len = 0`.

## Impact
Documentation only — `read_starfield_tail` consumes to `block_size` rather than a hardcoded length, so the code is correct. But a future decoder author would start from a byte pattern that is 8 bytes longer than reality and mis-assign every field.

## Related
#1606 (the original audit), #2622 / SF-D6-02 (the change that invalidated it), #2625 (opaque-tail capture disabling drift telemetry).

Also observed in the same sweep (recorded here rather than filed separately): every Starfield `BSLightingShaderProperty` carries `shader_type == 0` (252,417 blocks across 59,427 files in `FaceMeshes` + `Meshes01` + `LODMeshes` + `Meshes02`), so the FO76-only `Skin Tint` / `Hair Tint` arms at `shader.rs:1586-1605` are unreachable on vanilla content — the token mismatch itself is already tracked as **#3396**.

## Suggested Fix
Update the doc to record the measured 30 B tail (and the measurement that produced it), keeping the "captured opaquely to `block_size`, not a hardcoded length" note. Optionally note that the shortening is attributable to #2622 pulling `BSSPLuminanceParams` into the decode path.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other reverse-engineering byte-audit records whose decode path #2622 or a successor has since shortened)
