**Severity**: ENHANCEMENT (re-flag of MEDIUM `D1-M3` from 2026-04-20)
**Dimension**: GPU Pipeline
**Source**: AUDIT_PERFORMANCE_2026-05-01.md
**Status**: STILL OPEN since 2026-04-20 — never filed

## Location
- [crates/renderer/shaders/triangle.frag:1-30](../../tree/main/crates/renderer/shaders/triangle.frag#L1-L30) (no `early_fragment_tests` declaration after the `#version 460` directive)

## Description

Verified this audit (2026-05-01): `triangle.frag` declares no `layout(early_fragment_tests) in;`. The shader has two `discard` paths (lines 683, 694) but both are derived from texture-sampled alpha + `mat.alpha_threshold` — i.e. NOT from RT ray query results — meaning early-Z is legal. The shader writes to G-buffer storage attachments, fires reflection + GI ray queries, and runs the cluster light loop. **Without early-Z, every overdrawn fragment pays for ray queries before the depth test culls it.**

## Evidence

```glsl
// triangle.frag:1-4 (current)
#version 460
#extension GL_EXT_ray_query : enable
#extension GL_EXT_nonuniform_qualifier : require

// missing: layout(early_fragment_tests) in;
```

The `discard` calls at :683 (`if (!pass) discard;`) and :694 (alpha-test discard) are both derived from texture sample alpha + `mat.alpha_threshold`. Neither depends on RT ray query results, so early-Z is spec-legal.

## Impact (re-stated from 04-20)

- 2-3× fragment-invocation count on FO3 exterior overdraw
- ~2 ray queries per culled fragment (reflection + shadow), so ~6-8 fewer queries per overdrawn pixel
- Estimated **0.5-1.5 ms/frame GPU recovery** on overdraw-heavy scenes (Megaton firelight, FO3 worldspaces with stacked rocks)

This is the highest-leverage outstanding perf item from the 04-20 audit. It remained unfiled because the 04-20 audit's MEDIUM-tier items got bundle-merged into broader closures and this one slipped.

## Suggested Fix

Add `layout(early_fragment_tests) in;` after the `#extension` lines at the top of `triangle.frag`:

```glsl
#version 460
#extension GL_EXT_ray_query : enable
#extension GL_EXT_nonuniform_qualifier : require

// Early-Z: depth test runs before fragment shader. Legal because
// the shader's `discard` calls only depend on texture samples +
// alpha_threshold, not on RT ray query results. Saves 2-3× RT
// queries on overdrawn fragments. See D1-M3 / PERF-N6.
layout(early_fragment_tests) in;
```

Recompile:
```bash
cd crates/renderer/shaders
glslangValidator -V triangle.frag -o triangle.frag.spv
```

## Completeness Checks

- [ ] **UNSAFE**: N/A — pure GLSL change
- [ ] **SIBLING**: No other shaders fire RT ray queries; verify ssao.comp / cluster_cull.comp / svgf_temporal.comp / taa.comp / caustic_splat.comp don't have analogous overdraw issues (they're compute shaders — N/A)
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: A grep test in `scene_buffer.rs::gpu_instance_layout_tests` (sibling of the `triangle_vert_uses_bones_prev_for_motion_vectors` pattern) — assert `triangle.frag` contains `layout(early_fragment_tests)`. Prevents accidental removal.
