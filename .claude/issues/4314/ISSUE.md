# #4314: REN-2026-09-14-D18-03: new cloud-shape constants in `clouds.glsl` carry no citation, and skyal.md's provenance list names only two of them while the constants banner claims every value is sourced

- **Labels**: low,renderer,terrain-exterior,doc-rot,documentation
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4314
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: LOW
- **Dimension**: Sky/Weather
- **Location**: `crates/renderer/shaders/include/clouds.glsl` (`cloud_height_gradient`, `cloud_density`, `cloud_march`); `docs/engine/skyal.md` (§2.3 "Still open"); `crates/renderer/src/shader_constants_data.rs` (SKYAL banner above `CLOUD_LAYER_BOTTOM`)
- **Status**: NEW
- **Description**:
  - The No-Guessing policy requires a source, or an explicit "uncited" record, for tuned values.
  - skyal.md "Still open" records two items as unjustified: the coverage mapping (0.86/0.80/0.70/0.40/0.55) and the noise frequencies (`0.00008`/`0.0009`).
  - The following literals are also new with SKYAL (`c379898f`/`564d0d2f`) but carry no citation in code or doc, and are not on that list:
    - `cloud_height_gradient` remap breakpoints `0.15` and `0.55`;
    - erosion strength `erosion * 0.45`;
    - erosion height blend `height_fraction * 5.0`;
    - detail-noise advection `wind_offset * 3.0`;
    - wind scale `* 0.00002` in `cloud_march`.
  - The `shader_constants_data.rs` banner says "every value below follow[s] Schneider & Vos … not tuned by eye". The constants below it are cited individually, but the MS falloffs cite Skybolt as a secondary reference with owner sign-off. The in-shader literals above sit outside that banner and are sourced nowhere.
  - Verified none of these came from the pre-volumetric 2D body: a grep of `62a09fd9:crates/renderer/shaders/composite.frag` returns 0 hits for each. By contrast, the tint `0.45`/`0.08`, alpha `0.78`/`0.96` and horizon fade `0.015/0.16` were in that file, so the "carried over verbatim" claims for those do hold.
- **Evidence**: `cloud_height_gradient`: `cloud_remap(height_fraction, 0.0, 0.15, 0.0, 1.0)` / `cloud_remap(height_fraction, 0.55, 1.0, 1.0, 0.0)`; `cloud_density`: `detail_uvw = vec3(position.xz * 0.0009 + wind_offset * 3.0, …)`, `mix(detail, 1.0 - detail, clamp(height_fraction * 5.0, 0.0, 1.0))`, `cloud_remap(shaped, erosion * 0.45, 1.0, 0.0, 1.0)`; `cloud_march`: `… * time * 0.00002`.
- **Impact**:
  - Doc/provenance only. These values shape cloud silhouettes and drift speed, and the "sourced, calibrated" framing of `564d0d2f` makes them look already justified to the next reader.
  - That invites a future retune to treat them as reference values, or to skip justifying them.
- **Related**: `564d0d2f`, `66e86ce1` (skyal.md), #4230.
- **Suggested Fix**:
  - Add these literals to skyal.md's "Still open" provenance list, or cite them if a source exists; the Schneider & Vos height-gradient discussion does not state breakpoints.
  - Narrow the constants banner to "cited per constant below".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
