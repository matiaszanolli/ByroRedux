# TD3-201: shader-pipeline.md's GpuLight byte-layout table is stale — offsets 52/56 now carry shadow-segment radius + SHADOW_POLICY_* encoding

Severity: medium
Source audit: docs/audits/AUDIT_TECH_DEBT_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2252

**Dimension**: 3 (Stale Documentation & Comments)
**Location**: `docs/engine/shader-pipeline.md:288-302` (`GpuLight` — 64 bytes section)
**Status**: NEW

**Description**: Commit `1fb79038` ("Refactor shadow handling and introduce shared shadow policies", 2026-08-01) repurposed `GpuLight.params`'s trailing two floats: offset 52 (`params.y`) is now "finite luminous-source radius used by shadow segments," offset 56 (`params.z`) is now `SHADOW_POLICY_*` encoded as f32. Both are consumed by the new `shadow_common.glsl`/`shadow_transport.glsl` includes and by `triangle.frag`/`water.frag`/`volumetrics_inject.comp`. The struct's own Rust doc comment (`crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:183-208`) was updated in the same commit and is accurate; `shader-pipeline.md`'s markdown table — last touched by an unrelated commit three days earlier — still reads "52-63 (reserved)".

**Evidence**: `gpu_types.rs:203-207` — `/// x = attenuation exponent; y = finite luminous-source radius used by shadow segments; z = SHADOW_POLICY_* encoded as f32; w = reserved.` vs. `docs/engine/shader-pipeline.md:301-302` — `| 48 | falloff_exponent | ... |` then `| 52-63 | (reserved) | — |`.

**Impact**: `shader-pipeline.md` is the project's designated authoritative GPU-layout reference. Notably, the very next day's full 23-dimension renderer audit (`AUDIT_RENDERER_2026-08-02.md`) reviewed this same commit's other changes but didn't catch this specific doc gap — genuinely fresh, unflagged rot.

**Suggested Fix**: Update to `52 | shadow_segment_radius | Finite luminous-source radius used by shadow segments |`, `56 | shadow_policy | SHADOW_POLICY_* encoded as f32 (see shadow_common.glsl) |`, `60-63 | (reserved) | —`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix, if applicable
