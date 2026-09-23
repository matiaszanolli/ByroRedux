# #4778: REN-D3-2026-09-23-01: `VolumetricsParams` has a size-only Rust↔GLSL pin; a lane transposition inside the 15-member UBO is invisible

**Severity**: LOW
**Labels**: low, renderer, shaders, test-gap, bug
**Source**: docs/audits/AUDIT_RENDERER_2026-09-23.md (REN-D3-2026-09-23-01)

- **Severity**: LOW (test gap; the layout is currently correct by hand diff).
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/reflect.rs` `volumetrics_ubo_sizes_match_host_structs_in_every_shader`; `volumetrics.rs` `VolumetricsParams`.
- **Status**: NEW
- **Description**: The reflection test compares block *sizes* only. `GpuFogVolume` has a field-order lockstep test (`gpu_fog_volume_glsl_field_order_matches_rust_struct`), but `VolumetricsParams` has none, although it is 13 same-typed vec4 lanes whose `.w` slots are overloaded (`render_origin.w` = is_exterior, `fog_reference.w` = dt, `wind_gust.y` = BFECC switch). REN-D8-01 shows that same-typed `f32` transpositions do happen in this plumbing.
- **Suggested Fix**: Add a GLSL-member-order test mirroring the `GpuFogVolume` one, parsing the `uniform VolumetricsParams { … }` block in `volumetrics_inject.comp`.

**Note**: Also reported independently by the safety leg of the same suite as `SAFE-D6-2026-09-23-01` (docs/audits/AUDIT_SAFETY_2026-09-23.md); filed once here.

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-23.md` (`/audit-suite --preset volumetrics-deep`, HEAD `2237da9c3`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related passes (other compute passes / history buffers / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
