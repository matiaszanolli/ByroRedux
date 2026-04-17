# FO4-D1-H1: BSVER 131 has flag-pair/CRC-count gap — Next-Gen patch NIFs misalign silently

**Issue**: #409 — https://github.com/matiaszanolli/ByroRedux/issues/409
**Labels**: bug, nif-parser, high, legacy-compat

---

## Finding

`crates/nif/src/blocks/shader.rs` has two BSVER gates in `BSLightingShaderProperty::parse` that disagree on the FO4 / Next-Gen boundary:

- Line 411-415: flag-pair read gated at `bsver < 130 || bsver == 130` (equivalent to `<= 130`):
  ```rust
  let (shader_flags_1, shader_flags_2) = if bsver < 130 || bsver == 130 {
      (stream.read_u32_le()?, stream.read_u32_le()?)
  } else {
      (0, 0)
  };
  ```
- Line 427: CRC32 array counts gated at `bsver >= 132`:
  ```rust
  if bsver >= 132 {
      num_sf1 = stream.read_u32_le()?;
      num_sf2 = stream.read_u32_le()?;
      ...
  }
  ```

BSVER 131 reads **neither** the u32 pair NOR the CRC counts — a 4-byte or 8-byte gap depending on wire layout.

## nif.xml reference

nif.xml `#BS_FO4_2#` covers BSVER 130..=139 inclusive with the CRC array layout. This suggests the pair-drop happens at **131**, not 132. The audit cannot fully confirm without a canonical BSVER 131 NIF to compare against; Next-Gen patch NIFs from the user's FO4 install would resolve.

## Impact

- Any FO4 Next-Gen patch mesh at BSVER 131 misaligns at the shader block.
- Dim 1 H-2: no synthetic fixture or integration test targets BSVER 131+. The claimed "100% parse rate" counts graceful recovery (per-block `block_size` suppression), NOT structural correctness. A shader flag read as f32, or emissive_color offset by 4 bytes, won't throw — it renders a black/purple material.

## Fix

Two steps:

1. Inspect a canonical BSVER 131 NIF from Next-Gen patch content to determine whether the pair-drop is at 131 or 132 per the actual wire format.
2. Align both gates on the correct boundary:
   ```rust
   // assuming pair-drop at 131:
   let (shader_flags_1, shader_flags_2) = if bsver <= 130 {
       (stream.read_u32_le()?, stream.read_u32_le()?)
   } else {
       (0, 0)
   };
   if bsver >= 131 {   // not 132
       num_sf1 = stream.read_u32_le()?;
       num_sf2 = stream.read_u32_le()?;
       ...
   }
   ```
3. Add fixture tests at BSVER 130, 131, 132, 139 to lock the boundaries.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Related to FO4-D1-C1 (`wetness.unknown_1` BSVER gate at 530 was also off-by-one). Same audit class — verify every BSVER gate in `shader.rs` against nif.xml.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Per-BSVER fixture tests mirroring the existing FO4/FO76 harness in `shader.rs:1427` and `:1506`.

## Source

Audit: `docs/audits/AUDIT_FO4_2026-04-17.md`, Dim 1 H-1 + H-2.
