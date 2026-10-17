# REN-VOL-L-04: the fog-colour render-boundary decode has no call-site pin

Labels: low,renderer,test-gap,bug
Source: docs/audits/AUDIT_RENDERER_2026-10-17.md

- **Severity**: LOW (test gap)
- **Dimension**: Volumetrics / fog colour
- **Location**: `byroredux/src/render/mod.rs` (`let fog_color = crate::fog::fog_colour_radiance(…)`); tests in `render/fog_curve_propagation_tests.rs`
- **Status**: NEW
- **Description**: #5773 documented the convention "decoded exactly once at the boundary, `GpuCamera.fog.xyz` and the volumetric `fog_tint` carry linear light", but the only test is `fog_colour_radiance_encodes_back_to_the_authored_display_value`, which pins the function. The existing `run_view` fixtures feed `fog_color: [0.5, 0.4, 0.3]` and never assert `view.fog_color`. Removing the call (or adding a second decode downstream) keeps every test green and would silently shift every fog-lit interior by ~2.7x.
- **Verified clean today**: single decode; composite `fog_color`, `pack_fog_tint` input and `normalize_fog_tint` all consume the decoded value; no shader decodes again; `CellLightingRes` keeps the authored value.
- **Suggested Fix**: add one assertion in the `run_view` tests: `view.fog_color == fog_colour_radiance([0.5, 0.4, 0.3])`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_RENDERER_2026-10-17.md` (area-scoped `volumetrics-deep` suite run, HEAD e16a624e9). Fix with `/fix-issue <number>`._
