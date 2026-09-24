# #4805: PERF-D4-2026-09-23b-03: `CompositeParams` grew from 496 B to 12,800 B, is written in full every frame, and is built twice per frame in interiors

**Severity**: LOW
**Labels**: low, performance, renderer, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23b.md (PERF-D4-2026-09-23b-03)

- **Severity**: LOW
- **Dimension**: SSBO Sizing & Upload
- **Location**:
  - `crates/renderer/src/vulkan/composite.rs:1280-1287` (`upload_params` → a full-struct `write_mapped`);
  - `context/draw.rs:807-825` (a zero-initialised 12 KB aperture array per build) and `:1031-1044` (`build_sky_cube_params` calls `build_composite_params` a second time);
  - `byroredux/src/render/sky.rs:160` (`portal_outdoor_sky: Some(Box::new(..))`, one heap allocation per interior frame).
- **Status**: NEW (`0572bfd5a`)
- **Description**: The full 12.8 KB is written even with 0 apertures, although `write_mapped_prefix` exists. Interiors build a second full `CompositeParams` only to read its sky fields.
- **Impact**: ~25–40 KB of stack and write-combined traffic plus one heap allocation per interior frame: µs-scale. No quantitative guard exists for this site.
- **Suggested Fix**: Upload a prefix over the populated aperture count; derive `SkyCubeParams` from the outdoor `SkyParams` directly; hold the outdoor palette by value or in a persistent resource.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
