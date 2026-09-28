# #4950: REN-D2-2026-09-27-02: The interior window portal samples the sky cube along the pane normal, so each flat pane shows one sky texel (including the sun disc when the normal faces the sun)

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4950
- **Labels**: low,renderer,shaders,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D2-2026-09-27-02**._

- **Severity**: LOW
- **Dimension**: Ray Queries
- **Location**: `crates/renderer/shaders/triangle.frag` (fn `main`, window-portal block `vec3 skyColor = exteriorSkyRadianceOr(throughDir, exteriorSkyTint.rgb);`); pinned by `window_portal_orients_gate_ray_and_sky_sample_from_the_viewer` (`scene_buffer/shader_contract_tests.rs`)
- **Status**: NEW. `0572bfd5a` introduced the cube sample, and `3c9d44f20` (#4832) re-oriented it to `-N_bias`.
- **Description**: The portal's *occlusion* ray is deliberately fired along the pane's normal axis. That is #421: `-V` hits interior side walls at oblique angles. `0572bfd5a` then used that same direction for the *radiance* lookup. The rationale in its comment is "so a window sees the actual horizon, sun and clouds instead of one zenith swatch". For a flat pane, `throughDir = -N_bias` is constant (or near-constant under a normal map), so every pixel of the pane returns the same cube texel at LOD 0. The result is still one swatch per pane, now the horizon azimuth of the pane normal instead of the zenith, and it does not change with viewing angle. When the pane normal lies inside the baked sun disc or glare lobe (a west window at sunset), the whole pane reads the sun-disc radiance from every viewpoint. Seen through clear thin glass, the sky lies along the camera ray `-V`; thin glass does not bend it.
- **Evidence**: `vec3 throughDir = -N_bias;` feeds both `rayQueryInitializeEXT(windowRQ, …, throughDir, …)` and `exteriorSkyRadianceOr(throughDir, …)`. The test's required-strings list includes `"exteriorSkyRadianceOr(throughDir, exteriorSkyTint.rgb)"`, so the per-pane-constant behaviour is now pinned.
- **Impact**: Visual only. Interior windows show a flat, view-independent colour, and a pane blows out when its normal faces the sun. #4832's own note says the live sky effect is unverified, because the FNV panes tried do not discriminate.
- **Related**: #421, #3323, #4832, #925.
- **Suggested Fix**: Keep `throughDir` for the escape ray but sample `exteriorSkyRadianceOr(-V, …)` for the transmitted radiance, and update the #4832 pin accordingly. Confirm on a Skyrim or Vault 21 window capture.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
