# SK-D1-01: SSE skin payload is silently dropped for every BSDynamicTriShape -- all 21,139 Skyrim SE FaceGen head meshes spawn rigid

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2576
**Finding ID**: SK-D1-01

**Severity**: HIGH
**Dimension**: BSTriShape Packed Geometry + SSE Skinned Reconstruction
**Location**: `crates/nif/src/import/mesh/skin.rs:397-415` (`decode_sse_skin_payload`), reaching `crates/nif/src/import/mesh/sse_recon.rs:211-213` / `:232-234`
**Status**: NEW (residual of #2318, CLOSED — #2318 fixed the *geometry* half; this is the unfixed *skin* sibling)

## Description
`#2318` taught `try_reconstruct_sse_geometry` to feed a `BSDynamicTriShape`'s trailing `Vector4` array in as external positions. `decode_sse_skin_payload` was never updated to match — it still calls the plain `decode_sse_packed_buffer(buffer)` wrapper (`_with_external_positions(buffer, None, None)`), which bails at `sse_recon.rs:232` whenever `VF_VERTEX` is clear and no external positions are supplied. Every vanilla Skyrim SE FaceGen partition buffer clears `VF_VERTEX` (positions live in the dynamic array), so the whole weights+indices decode is thrown away — even though the skin lanes sit further down the same vertex layout and don't depend on positions at all.

## Evidence
Measured, `Skyrim - Meshes0.bsa` + `Meshes1.bsa`: 21,140 `BSDynamicTriShape` blocks; 21,139 carry a `skin_ref` resolving to a populated `SseSkinGlobalBuffer`. All observed partition-buffer attribute masks (`0x442`, `0x45a`, `0x462`, `0x47a`, `0x55a`) clear bit 0 (`VF_VERTEX`). Import outcome: `skin weights missing = 21139`, `skin indices missing = 21139`. Confirmed directly: `decode_sse_skin_payload` (`skin.rs:408`) calls `decode_sse_packed_buffer(buffer)` with no external-positions parameter. Consumer `byroredux/src/scene/nif_loader.rs:639-642` filters on non-empty bone arrays, so every head vertex is built with zero bone weights → `triangle.vert`'s `wsum < 0.001` rigid fallback. Live path: `npc_spawn/resumable.rs:992-1024` (`PrebakedPhase::Facegen`) uses exactly this builder.

## Impact
Every Skyrim SE/AE NPC's head, eyes, brows, mouth and hair-cap geometry uploads as rigid geometry parented to the placement root instead of skinned to `NPC Head`/`NPC Neck` — heads stay in bind pose through every animation while the body deforms; the skinned-BLAS refit path sees a static blob. Same defect class as #638 (which fixed bodies), on the head. 21,139 of 26,940 skinned SSE shapes in vanilla — **78% of all skinned Skyrim SE geometry**.

## Related
#2318 (geometry half, CLOSED), #638 (body half, CLOSED), #2322, #341, #559

## Suggested Fix
Make `decode_sse_skin_payload` mirror `try_reconstruct_sse_geometry` — resolve the shape's external positions and `BsTriShapeKind::Dynamic { bitangent_x }`, call the `_with_external_positions` variant (widen visibility to `pub(super)`). Cleaner: split the position-presence guard out of the decoder so a skin-only decode never depends on positions. Pin with a regression test asserting non-empty `vertex_bone_weights` for a synthetic `VF_VERTEX`-clear/`VF_SKINNED`-set global buffer.

## Completeness Checks
- [ ] **TESTS**: A regression test asserts non-empty `vertex_bone_weights` for a synthetic `VF_VERTEX`-clear/`VF_SKINNED`-set global buffer
- [ ] **SIBLING**: Confirm the fix doesn't reintroduce SK-D1-02's single-partition shortcut bug now that head meshes will actually carry weights
