# REN-D3-2026-09-21-01: `presentation.frag` selects AgX with a hand-written `tonemapOp == 1u` that is neither generated from `TONEMAP_OP_*` nor pinned GLSL-side

**Labels**: low, renderer, shaders, test-gap, bug

Filed via /audit-publish from docs/audits/AUDIT_RENDERER_2026-09-21.md.

**Severity**: LOW (latent wire-contract drift) · **Dimension**: GPU-Struct
**Location**: `crates/renderer/shaders/presentation.frag` `tonemap()` (~:116-118); `crates/renderer/src/tonemap.rs` `TONEMAP_OP_ACES` / `TONEMAP_OP_AGX` (~:28-29) and `operator_ids_are_stable`
**Status**: NEW (introduced by `c5663fe39`)
**Verified against**: HEAD `f97775ca8`

## Description

`tonemap()` dispatches with `params.tonemapOp == 1u ? agx(x) : aces(x)`. The `1u` is typed into the shader by hand. The values `TONEMAP_OP_ACES = 0` and `TONEMAP_OP_AGX = 1` exist only in `tonemap.rs`. They are not emitted into the generated `shader_constants.glsl`, which already carries the other host↔shader enums (debug modes, visibility layers, material kinds).

Two tests touch this and neither covers the value:
- `operator_ids_are_stable` asserts only the Rust values.
- The presentation source-shape test checks that the `uint tonemapOp;` field exists, not which value the shader compares against.

## Evidence

- `grep -rn "TONEMAP_OP" crates/renderer/shaders/ crates/renderer/build.rs crates/renderer/src/shader_constants*.rs` finds only two comments in `presentation.frag`.
- `presentation.frag` (~:117): `return params.tonemapOp == 1u ? agx(x) : aces(x);`.

## Impact

If the Rust side renumbers or adds an operator (a third curve, a reorder), the GPU silently applies a different curve and no test fails. No runtime effect today.

## Related

- REN-D11-2026-09-21-01 (#4578): the AgX curve this literal selects.
- REN-D3-2026-09-21-02 (#4585): the other Stage-1 GPU-contract pin gap.

## Suggested Fix

Emit `TONEMAP_OP_ACES` and `TONEMAP_OP_AGX` into the generated `shader_constants.glsl` (via `shader_constants_data.rs` + `build.rs`, with the usual value pin), and compare against `TONEMAP_OP_AGX` in `presentation.frag`. Failing that, add a source-shape pin that the literal equals `TONEMAP_OP_AGX`.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D3-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: other host enums compared by literal in the Stage-1 shaders (e.g. `params.mode.x < 0.5` in `exposure_meter.comp`) checked
- [ ] **SIBLING**: `presentation.frag.spv` recompiled; `scripts/check-shader-artifacts.sh` green
- [ ] **TESTS**: a generated-header value pin (or a source-shape pin) for the operator ids
