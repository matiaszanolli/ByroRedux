# #6316: PERF-D5-02: The cloud-shadow bake is an unmeasured fixed per-frame pass, it runs in interiors that read one texel of it, and it has no bracket of its own

**Labels**: low,performance,renderer,terrain-exterior,bug

**Source**: docs/audits/AUDIT_PERFORMANCE_2026-10-21.md · **Severity**: LOW · **Dimension**: GPU Pipeline / Telemetry · **Location**: `crates/renderer/src/vulkan/sky_cube.rs (record_bake -> cloud_shadow.record)`

- **Location**: `crates/renderer/shaders/cloud_shadow.comp` (512² invocations × `CLOUD_SHADOW_STEPS` 16 × `cloud_column_coverage` + `cloud_density`, i.e. 2–3 3D-noise fetches per step), dispatched unconditionally by `crates/renderer/src/vulkan/sky_cube.rs:569-571` → `crates/renderer/src/vulkan/sky_cube/cloud_shadow.rs:195-255`, inside the `sky_cube_ms` bracket.
- **Status**: NEW (`c231f5b5f`, `fdf4bfc1e`, `b6d6f8320`, 2026-10-20). The skill's inventory row already lists it, and memory-budget.md has its 512 KiB row. No timing is recorded anywhere.
- **Description**: ~262 k columns × 16 steps is ~4 M density evaluations per frame, about 2.7× the sky cube's own texel count. `skyal.md:248-250`'s `gpu_sky_cube ~0.1 ms` was measured before the bake existed. An interior consumes only the camera column (`cameraSunTransmittance`, read back for the portal sun), yet the whole map is rebaked. The clouds drift with wind at sub-texel speed per frame (125 m texels), so amortising the bake (half the rows per frame, or rebaking on a wind or texel-snap delta) is plausible.
- **Evidence**: shader read; `record_bake` has no gate (only `to_sun.y <= 0` short-circuits each invocation).
- **Impact**: unknown fixed GPU cost on every frame of every cell.
- **Related**: skill Dim 5 "sky-cube bake ... runs in interiors, quantify first"; PERF-D5-01.
- **Suggested Fix**: give the bake a nested `cloud_shadow_ms` bracket (+2 queries) and measure. Then gate interiors to a 1×1 camera-column dispatch, and consider amortising exteriors.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
