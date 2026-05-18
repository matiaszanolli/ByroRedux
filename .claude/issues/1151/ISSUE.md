# TD4-302: redundant THREADS_PER_CLUSTER redefinition in cluster_cull.comp (already #define'd via include)

## Source Audit
`docs/audits/AUDIT_TECH_DEBT_2026-05-17.md` — Dimension 4 (Magic Numbers / lockstep drift hazard)

## Severity
**LOW** — present-day correct; future-regression hazard per `feedback_shader_struct_sync.md`.

## Location
`crates/renderer/shaders/cluster_cull.comp:30`

## Description
Line 28 includes `shader_constants.glsl` (which `#define`s `THREADS_PER_CLUSTER 32` from `src/shader_constants_data.rs:39`). Line 30 then redefines the same constant locally:
```glsl
#include "include/shader_constants.glsl"

const uint THREADS_PER_CLUSTER = 32;
```

Both currently agree at `32`. If a future Rust-side const change is made AND the shader is recompiled without removing the local override, the duplicate silently shadows the new value.

## Proposed Fix
Delete `const uint THREADS_PER_CLUSTER = 32;` on line 30. The `#define` from the include is sufficient for `layout(local_size_x = THREADS_PER_CLUSTER)` usage (line 32).

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Grep other shaders for hand-rolled redefinitions of constants the build.rs already provides (`grep -rn "const uint .* = [0-9]" crates/renderer/shaders/`)
- [ ] **DROP**: N/A
- [ ] **TESTS**: SPV needs regeneration after edit (no test will catch a silent shadow)
