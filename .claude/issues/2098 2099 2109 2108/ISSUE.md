# Issues 2098, 2099, 2109, 2108

## #2098 — SF2D2-01: BSGeometry block bounding-sphere scale not cross-checked against havok-scaled vertices
**Location**: `crates/nif/src/import/mesh/bs_geometry.rs:233-249`. LOW, low-confidence, needs-verification.

Raw `bounding_sphere` used verbatim as local bound whenever `radius > 0`, no cross-check against decoded-vertex extent (units could diverge, e.g. havok scale). Not observed as a real bug on Cydonia data. Suggested fix: debug-only sanity check mirroring the existing `bs_geometry_hint_mismatch` pattern; test via synthetic mismatched fixture.

## #2099 — SF2D2-02: Secondary UV channel (uvs1) parsed then dropped by the importer
**Location**: `crates/nif/src/import/mesh/bs_geometry.rs:160`. LOW, enhancement.

`BSGeometryMeshData` decodes `uvs1` but `extract_bs_geometry` only consumes `uvs0`. "Not actionable until `Vertex`/`ImportedMesh` grow a second UV slot" (issue's own words) — a vertex-format change out of scope here. Suggested fix: track as enhancement.

## #2109 — SF-D9-02: BGEM v21/v22 glass-overlay params + envmap-mask-scale + v11 emittance dropped in merge
**Location**: `byroredux/src/asset_provider/material.rs:973-1102`; fields at `crates/bgsm/src/bgem.rs:31-77`. LOW, enhancement.

BGEM merge forwards `glass_enabled` but drops `glass_fresnel_color`, `glass_refraction_scale_base`, `glass_blur_scale_base`, `glass_blur_scale_factor`, `glass_roughness_scratch`, `glass_dirt_overlay`, `environment_mapping_mask_scale`, `emittance_color`. No `ImportedMesh` sink or renderer binding exists for any of these yet. Suggested fix: "Track as a deferred renderer-binding follow-up... No parser change needed."

## #2108 — SF-D9-01: EFFECT_PALETTE_COLOR/ALPHA derived from LUT-texture presence, not the authored palette-enable flag
**Location**: `byroredux/src/asset_provider/material.rs:790-793` (BGSM), `:1001-1008` (BGEM); `byroredux/src/cell_loader.rs:244-250` (packer). MEDIUM — real bug.

Packer sets `EFFECT_PALETTE_COLOR`/`ALPHA` whenever `bgsm_greyscale_lut_path.is_some()`, ignoring the authoritative `grayscale_to_palette_color`/`_alpha` enable flag (parsed at `crates/bgsm/src/base.rs:215` but never consumed). Asymmetric with the inline NIF effect-shader path, which correctly gates on the real SLSF enable bit. A material with a filled-but-disabled greyscale slot gets an unwanted palette remap.

**Fix**: forward the parsed enable bool onto a new `ImportedMesh` field and gate the flag pack on it, mirroring the inline-path gate. Verify BGEM's existing `grayscale_to_palette_alpha` alpha-vs-color forwarding still works.
