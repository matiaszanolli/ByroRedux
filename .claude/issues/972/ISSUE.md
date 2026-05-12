# FO4-D4-NEW-09: TextureSet HasModelSpaceNormals flag parsed but renderer never branches on it

**Labels**: bug, renderer, low, legacy-compat

**Audit**: `docs/audits/AUDIT_FO4_2026-05-11_DIM4.md`
**Severity**: LOW
**Domain**: ESM / FO4 / renderer normal-decode

## Premise

`DNAM` is parsed into `TextureSet::flags: u16` at [crates/plugin/src/esm/cell/support.rs:251-257](../../crates/plugin/src/esm/cell/support.rs#L251-L257) with 100% vanilla FO4 TXST coverage. Bit 2 = `HasModelSpaceNormals`. Doc at [crates/plugin/src/esm/cell/mod.rs:579-583](../../crates/plugin/src/esm/cell/mod.rs#L579-L583) explicitly notes the renderer must branch on this once it consumes the field. Test at `cell/tests.rs:2536-2537` asserts the bit is readable.

## Gap

The renderer's normal-map decode branches off `BGSM.model_space_normals` ([crates/bgsm/src/bgsm.rs:94](../../crates/bgsm/src/bgsm.rs#L94)) but never reads the TXST DNAM-derived flag for **direct-TXST (non-MNAM) REFRs**. A grep for `HasModelSpaceNormals` outside tests/issue-docs finds zero consumers.

## Impact

FO4 TXST records that override a host mesh's normal map AND specify a model-space (not tangent-space) normal will decode as tangent-space. Visual symptom: **discoloured / wrongly-shaded normal mapping on TXST-overridden re-skinned props**.

Vanilla count is small (most FO4 model-space normals route through BGSM), but mod content that authors direct TXST overrides — especially face-tint / actor-skin TXSTs and any mod that swaps a single normal without going through BGSM — will hit it.

## Suggested Fix

Plumb the flag through to the renderer:

1. Add `model_space_normals: bool` field to `RefrTextureOverlay`.
2. In `build_refr_texture_overlay` (`cell_loader_refr.rs:186`), set it from `texture_set.flags & 0x04 != 0` when the overlay sources its normal slot from a TXST.
3. Branch the renderer's normal-decode path at the existing BGSM tap so the model-space variant fires for either source (BGSM.model_space_normals OR TXST.flags bit 2).

Can be deferred until the renderer has a stable model-space normal path (currently BGSM-only). Filing here so it's not lost.

## Completeness Checks

- [ ] **SIBLING**: Confirm the renderer's BGSM normal-decode branch is the single tap-point — no duplicate decode paths to keep in sync.
- [ ] **SIBLING**: Verify other DNAM flag bits (`NoSpecular`, `FaceGenTinting`) get the same plumbing pattern if/when their renderer consumers land.
- [ ] **TESTS**: Regression test asserts the flag flows through `RefrTextureOverlay` (parse + overlay-build round-trip).
- [ ] **TESTS**: Visual regression test: a TXST with `HasModelSpaceNormals` set + the base mesh's normal map overridden renders with model-space decode (golden-image or normal-vector spot check).
- [ ] **GATE**: Hold open until M-renderer-normal-decode-stable lands; then complete.
