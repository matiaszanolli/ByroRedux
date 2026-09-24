# #4806: PERF-D5-2026-09-23b-03: The composite sky-aperture mask costs render pixels × marked apertures with no screen-space cull, and every interior runs the opening depth probe on its background pixels

**Severity**: LOW
**Labels**: low, performance, renderer, shaders, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23b.md (PERF-D5-2026-09-23b-03)

- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/composite.frag:420-440` (`boundedInteriorOpening`, ≤ 20 `texelFetch`), `:448-474` (≤ 256-aperture loop, surface pixels included), `:518-525`; `context/draw.rs:801-837` (packing, no cull); `render/fog_volumes.rs:89`
- **Status**: NEW (`0572bfd5a`)
- **Description**:
  - Each pixel loops over every uploaded aperture, off-screen ones included, with 2 quaternion rotations plus a plane intersection each. The first rotation is uniform per aperture but recomputed per pixel.
  - The pass-inventory invariant is "O(pixels), never O(scene content)".
  - Every interior (`sky_lower.w = 2` is always set in interiors) runs the depth probe on background pixels.
- **Impact**: *est.* 0.1 ms (32 apertures, 1080p) to 0.7 ms (the 256 cap). Today only the Nellis converter emits marked apertures, so the trigger is narrow.
- **Suggested Fix**: Frustum-cull apertures, precompute each one's camera-local origin and clip-space rect on the CPU, and reject pixels outside the rect before any rotation.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
