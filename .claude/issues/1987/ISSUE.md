**Source:** FO4 compatibility audit — Dimension 2 (BGSM/BGEM consumption), `docs/audits/AUDIT_FO4_2026-07-13.md`
**Severity:** LOW · **Status when filed:** NEW, CONFIRMED against current code

## Description
The #1476 metalness-from-saturation derivation (`metalness = (max-min)/max` of `specular_color`) and the pbr-branch luminance path are computed **inline** inside `merge_bgsm_into_mesh`, not extracted into a pure helper. The two *adjacent* helpers this area cares about — `conductor_diffuse_tint` (#1591) and `bgsm_blend_to_gamebryo` (#1823) — are both extracted and each carries a dedicated regression test. The metalness formula — **the exact code that regressed once** (luminance → chrome vanilla concrete, fixed in `08ed03be`) — has no equivalent guard: no test builds a white-spec vs tinted-spec `BgsmFile` and asserts `metalness_override ≈ 0.0` vs `> 0`.

## Evidence
- `byroredux/src/asset_provider/material.rs:728-748`: `grep 0.2126|0.7152|0.0722` returns exactly one hit (line 730), correctly inside `if leaf.pbr`; the `else` (non-pbr) branch uses the saturation formula.
- Test search over `asset_provider/tests.rs` finds `conductor_diffuse_tint` coverage and the `.mat` arm — nothing asserts the metalness scalar produced from a BGSM's `specular_color`.
- A future edit that reintroduces `spec_lum` into the `else` (non-pbr) branch would compile, pass the entire suite, and silently re-chrome vanilla concrete.

## Impact
Defense-in-depth gap only; **current code is correct**. The specific regression this pin guards against (the highest-risk pin in this dimension, since it has reverted to luminance once before) is invisible to `cargo test`.

## Suggested Fix
Extract the metalness derivation into `pub(crate) fn bgsm_metalness(spec: [f32;3], mult: f32, pbr: bool) -> f32` and add two asserts:
- `spec=[1,1,1] pbr=false → 0.0` (white spec = concrete dielectric)
- `spec=[1,0.85,0.70] pbr=false → > 0.1` (tinted spec = conductor)

Mirrors the pattern already used for `conductor_diffuse_tint` / `bgsm_blend_to_gamebryo`.

## Related
#1476 (`08ed03be`), #1591 (conductor tint), project memory `fo4_bgsm_metalness`, `chrome_flyer_pbr_classifier_gap`.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: the extracted helper stays on the parse/merge side (pre-`translate_material`), not re-run at draw time
- [ ] **TESTS**: the two white-spec→0 / tinted-spec→>0 asserts land as the regression pin
