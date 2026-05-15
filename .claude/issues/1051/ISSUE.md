# #1051 — Tech-Debt: 7 files >2000 LOC splits [batch]

**Labels**: nif-parser, renderer, tech-debt, low

## Status

| ID | File | LOC (audit) | Status |
|----|------|------------|--------|
| TD9-001 | acceleration.rs | 4200 | DONE — Session 35 split into 9 submodules (blas_static/skinned, tlas, constants, types, predicates, memory, tests) |
| TD9-002 | context/draw.rs | 2554 | BLOCKED — requires RenderDoc baseline (feedback_speculative_vulkan_fixes.md); currently 2612 LOC |
| TD9-003 | scene_buffer.rs | 2367 | DONE — Session 35 split into buffers/constants/descriptors/gpu_types/upload/tests |
| TD9-004 | context/mod.rs | 2348 | BLOCKED — requires RenderDoc baseline; currently 2411 LOC |
| TD9-005 | import/mesh.rs | 2212 | DONE — Session 35 split into decode/material_path/ni_tri_shape/bs_tri_shape/bs_geometry/mod |
| TD9-006 | blocks/collision.rs | 2162 | DONE — Session 35 split into collision_object/compressed_mesh/constraints/phantom_action/mod |
| TD9-007 | anim.rs | 2101 | DONE — Session 35 split into bspline/channel/controlled_block/coord/entry/keys/sequence/transform |
