# Batch: #2578, #2581, #2582, #2583

All LOW severity, from the Skyrim SE audit sweep. Processed together, fixed/committed individually.

---

## #2578 — SK-D1-03: packed-vertex parser ignores all ten BSVertexDesc offset nibbles

**Severity**: LOW
**Dimension**: BSTriShape Packed Geometry + SSE Skinned Reconstruction
**Location**: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:205-237,951-1097`; same shape in `sse_recon.rs:276-436`
**Status**: NEW (adjacent to #336, CLOSED)

### Description
`decode_bs_vertex_stream` walks a fixed field order and never consults the ten 4-bit offset nibbles `BSVertexDesc` publishes. The `VF_UVS_2` doc comment claims the reserved bytes are absorbed by "the trailing skip" — per nif.xml the `UV2 Offset` nibble sits *between* `UV1 Offset` and `Normal Offset`, implying UV2 is mid-vertex, in which case every attribute after UV1 would misalign. No sample exists to confirm either reading.

### Evidence
Corpus scan of 81,226 SSE `BSTriShape` blocks: `UVS_2 = 0`, `LAND_DATA = 0`, `INSTANCE = 0` occurrences — no vanilla Skyrim SE content exercises this path, and the fixed-order walk is provably correct for all 22 descriptors that do occur.

### Impact
Zero on vanilla Skyrim SE. On a mod-authored mesh setting bit 2, the whole post-UV1 attribute set would silently misalign with no diagnostic.

### Related
#336, #358, #359

### Suggested Fix
Soften the comment to state the assumption, not the conclusion; add a cheap post-walk offset comparison + `log::warn!` on mismatch (silent on 100% of vanilla content today, so free insurance).

### Completeness Checks
- [ ] **TESTS**: N/A for vanilla content; the suggested post-walk comparison itself would act as a runtime regression guard

---

## #2581 — SKY-D2-03: No wire-level Skyrim parse test for shader types 6/7/14

**Severity**: LOW
**Dimension**: BSLightingShaderProperty / BSEffectShaderProperty Shader-Type Dispatch
**Location**: `crates/nif/src/blocks/shader_tests/skyrim.rs:6-119`
**Status**: NEW

### Description
Skyrim-era wire-parse tests cover shader types 0, 1, 5, 11, 16 only. Types 6 (HairTint, 10,817 vanilla instances — more than SkinTint/EyeEnvmap/MultiLayerParallax combined), 7 (ParallaxOcc, 0 vanilla but mod-reachable) and 14 (SparkleSnow, 19 instances) have no Skyrim wire-level test; the corresponding `apply_shader_type_data` tests construct the enum directly and never exercise the byte reader.

### Impact
Test-coverage gap only — code is currently correct (verified against nif.xml and the zero-drift corpus run). A future field-count regression in arm 6/7/14 would ship silently.

### Related
SKY-D2-01 (this session — shares the same under-tested HairTint surface).

### Suggested Fix
Add three `build_bs_lighting_common(N)` + trailing-bytes tests mirroring the existing `skin_tint` one, keeping the over-read-detecting `stream.position() == data.len()` assertion.

### Completeness Checks
- [ ] **TESTS**: Three new wire-level tests for shader types 6/7/14, mirroring the existing `skin_tint` test's over-read assertion

---

## #2582 — SKY-D2-04: BSEffectShaderProperty.env_map_min_lod has no consumer past MaterialInfo

**Severity**: LOW
**Dimension**: BSLightingShaderProperty / BSEffectShaderProperty Shader-Type Dispatch
**Location**: `crates/nif/src/blocks/shader.rs:1560,1736`; `import/material/shader_data.rs:37`; `import/material/mod.rs:894`
**Status**: NEW

### Description
Unlike its packed-field siblings (`texture_clamp_mode` reaches sampler selection, `lighting_influence` reaches `material_flags`), `env_map_min_lod` stops at `BsEffectShaderData` — no packer, no `Material` field, no GLSL uniform — and carries no "parked for a future consumer" comment.

### Evidence
All 8,116 vanilla Skyrim `BSEffectShaderProperty` blocks author `env_map_min_lod = 0`, so nothing is lost today.

### Impact
None on vanilla Skyrim. On FO4+ effect materials that clamp the env-map mip chain, the authored floor is silently ignored.

### Related
#345/S4-01

### Suggested Fix
Either document it as explicitly parked, or plumb it into `GpuMaterial` alongside `soft_falloff_depth`.

### Completeness Checks
- [ ] **TESTS**: N/A unless plumbed to `GpuMaterial`, in which case a regression test confirms the value reaches the shader

---

## #2583 — SKY-D4-01: CLAUDE.md's documented --master repro command is wrong (fails verbatim)

**Severity**: LOW (documentation defect — the engine's own error handling did its job correctly)
**Dimension**: Multi-Master Load Order + TES5 Cell-Load Regression
**Location**: `CLAUDE.md` Usage section; this audit skill's own Dimension-4 brief cites the identical broken repro
**Status**: NEW

### Description
`cargo run -- --master Skyrim.esm --esm Dawnguard.esm --cell ForebearsHoldoutInt01` fails outright against real data: `Dawnguard.esm`'s actual `MAST` list is `["Skyrim.esm", "Update.esm"]` (the doc omits the second master), and `ForebearsHoldoutInt01` is not a real cell EditorID (the real interior is `Forelhost01`). With both corrections (`--master Skyrim.esm --master Update.esm --esm Dawnguard.esm --cell Forelhost01 --bsa …`), the cell loads cleanly: 10,045 entities, 928 meshes, 343 textures, 78.5 FPS, zero errors — proving the underlying M46.0 repeatable-`--master` FormID remap works correctly. The failure mode is soft: the engine logs one clear `ERROR` line naming the missing master but then falls back to the default 6-entity demo scene and keeps running rather than exiting non-zero, so a `--bench-hold` run not watching stderr closely could believe the repro "worked."

### Evidence
Confirmed directly: `CLAUDE.md:314` — `cargo run -- --master Skyrim.esm --esm Dawnguard.esm --cell <id> --bsa …` — single `--master`, missing the required second (`Update.esm`).

### Impact
Anyone verifying multi-master support via the documented command gets a false failure signal.

### Suggested Fix
Update the `--master` line in `CLAUDE.md`'s Usage section to `--master Skyrim.esm --master Update.esm --esm Dawnguard.esm --cell Forelhost01 --bsa …`. No code change needed — engine behavior is correct per #561's design intent.

### Completeness Checks
- [ ] **TESTS**: N/A (doc-only change); manually re-verify the corrected command loads cleanly before closing
