# TD4-303: 4 compute shaders hardcode local_size_x = 8 instead of WORKGROUP_X

## Source Audit
`docs/audits/AUDIT_TECH_DEBT_2026-05-17.md` — Dimension 4 (Magic Numbers / workgroup-sizing drift hazard)

## Severity
**LOW** — present-day correct (8 matches WORKGROUP_X); if WORKGROUP sizes are tuned later for GPU occupancy, these 4 shaders silently miss the rebalance.

## Locations
- `crates/renderer/shaders/taa.comp:16` — `layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;`
- `crates/renderer/shaders/ssao.comp:9` — same
- `crates/renderer/shaders/svgf_temporal.comp:21` — same
- `crates/renderer/shaders/caustic_splat.comp:25` — `layout(local_size_x = 8, local_size_y = 8) in;`

## Canonical
`WORKGROUP_X = WORKGROUP_Y = 8` in `crates/renderer/src/shader_constants_data.rs:34-35`, included via `#include "include/shader_constants.glsl"`.

## Contrast (already correct)
- `bloom_downsample.comp` / `bloom_upsample.comp` use `layout(local_size_x = WORKGROUP_X, local_size_y = WORKGROUP_Y)`
- `volumetrics_inject.comp` / `volumetrics_integrate.comp` use same pattern

## Proposed Fix
Replace hardcoded `8` with `WORKGROUP_X` / `WORKGROUP_Y` across all 4 shaders. SPV must be regenerated after edit (see `feedback_speculative_vulkan_fixes.md` — these are compute pipelines, not Vulkan command recording, so the regen is safe to verify with cargo test if a `dispatch` count assertion exists).

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: All 4 shaders updated in lockstep (don't leave 1 hardcoded — that defeats the rebalance fix)
- [ ] **DROP**: N/A
- [ ] **TESTS**: SPV regeneration; verify cargo test passes after recompile
