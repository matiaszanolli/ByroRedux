# FO4-D8-NEW-02: BGEM grayscale_texture (palette/gradient LUT) not forwarded to ImportedMesh

**Severity**: MEDIUM  
**Source**: AUDIT_FO4_2026-06-02 (D8-NEW-02)  
**Location**: `byroredux/src/asset_provider.rs` — BGEM branch of `merge_bgsm_into_mesh`

`BgemFile.grayscale_texture` parsed at `crates/bgsm/src/bgem.rs:105` but never forwarded to `ImportedMesh`. Fire/electricity/magic effects lose their colour-ramp palette lookup.

**Fix**: Populate `mesh.effect_shader` with `greyscale_texture` and appropriate `effect_palette_color` flag in the BGEM branch. Requires renderer-side consumer; pair with effect-shader render pipeline milestone.
