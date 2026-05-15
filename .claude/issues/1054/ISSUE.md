# Tech-Debt: Audit-skill path rot after Session 36 monolith splits [batch]

**Labels**: documentation, medium, tech-debt
**Status**: Open

## Description

Session 36 split 7 files into directory modules (acceleration / scene_buffer / anim / import-mesh / blocks-collision / cell-tests / dispatch_tests). The load-bearing docs (CLAUDE.md, ROADMAP, HISTORY, docs/engine/*) were refreshed in commit `5ab6a8b`, but the audit-skill source files in `.claude/commands/` were intentionally **not** swept (mirroring the Session 34 → #1040 cycle). The 2026-05-14 audit found 29 stale path references across the audit skill set that will mislead future audit runs.

## Findings

Each row below: file:line in the audit skill, stale ref → correct post-Session-36 path.

| Skill file | Stale reference | Correct path |
|---|---|---|
| `_audit-common.md:16` | `crates/nif/src/anim.rs + anim/{types.rs, tests.rs}` | `crates/nif/src/anim/` (now 9 siblings: coord/controlled_block/transform/sequence/keys/channel/bspline/entry/types) |
| `_audit-common.md:30` | `crates/renderer/src/vulkan/acceleration.rs` | `crates/renderer/src/vulkan/acceleration/` (9 siblings) |
| `_audit-common.md:43` | `crates/renderer/src/vulkan/scene_buffer.rs` | `crates/renderer/src/vulkan/scene_buffer/` (5 prod + 3 test siblings) |
| `_audit-common.md` (NIF Blocks line) | `blocks/collision.rs` (single file) | `blocks/collision/` (9 prod siblings + 5 pre-existing test files) |
| `_audit-common.md` (NIF Import line) | `mesh.rs + mesh_*_tests.rs siblings` | `import/mesh/` (8 prod + 7 test siblings, `mesh_` prefix dropped, `#[path]` shims removed) |
| `audit-renderer.md` | references to `acceleration.rs:NNN` line anchors | translate via section markers (`constants` / `predicates` / `blas_static` / `tlas` / `memory`) |
| `audit-renderer.md` | `scene_buffer.rs::MaterialBuffer::upload_materials` symbol-anchored | now in `scene_buffer/upload.rs` |
| `audit-renderer.md` | `GpuInstance` "lives in 3 shaders" claim | actually 5: `triangle.vert`, `triangle.frag`, `ui.vert`, `water.vert`, `water.frag` |
| `audit-nif.md` | `anim.rs:1057 deboor_cubic` | now `anim/bspline.rs::deboor_cubic` |
| `audit-nif.md` | `import/mesh.rs::synthesize_tangents` | now `import/mesh/tangent.rs::synthesize_tangents` |
| `audit-nif.md` | `import/mesh.rs::extract_tangents_from_extra_data` | now `import/mesh/tangent.rs::extract_tangents_from_extra_data` |
| `audit-nif.md` | `import/mesh.rs::material_path_from_name` | now `import/mesh/material_path.rs::material_path_from_name` |
| `audit-nif.md` | `import/mesh.rs::try_reconstruct_sse_geometry` | now `import/mesh/sse_recon.rs::try_reconstruct_sse_geometry` |
| `audit-nif.md` | `blocks/collision.rs::BhkRigidBody` | now `blocks/collision/rigid_body.rs::BhkRigidBody` |
| `audit-nif.md` | `blocks/collision.rs::BhkCompressedMeshShape*` | now `blocks/collision/compressed_mesh.rs` |
| `audit-skyrim.md` | `dispatch_tests.rs::oblivion_shader_variants_route_to_bsshader_pp_lighting` | now `dispatch_tests/shader.rs::oblivion_shader_variants_route_to_bsshader_pp_lighting` |
| `audit-skyrim.md` | `dispatch_tests.rs::starfield_*_dispatches` | now `dispatch_tests/starfield.rs::*` |
| `audit-fo4.md` | `dispatch_tests.rs::fo4_bhk_np_collision_object_dispatches_and_consumes` | now `dispatch_tests/havok.rs::*` |
| `audit-fo4.md` | `dispatch_tests.rs::fo4_bs_cloth_extra_data_omits_name_field` | now `dispatch_tests/extra_data.rs::*` |
| `audit-fnv.md` | `cell/tests.rs::parse_real_fnv_esm` | now `cell/tests/integration.rs::parse_real_fnv_esm` |
| `audit-fnv.md` | `cell/tests.rs::parse_ligh_*` | now `cell/tests/light.rs::*` |
| `audit-oblivion.md` | `cell/tests.rs::oblivion_cells_populate_xcll_lighting` | now `cell/tests/integration.rs::oblivion_cells_populate_xcll_lighting` |
| `audit-oblivion.md` | `cell/tests.rs::parse_cell_tes4_xcmt_populates_music_type_enum` | now `cell/tests/cell.rs::*` |
| `audit-fo3.md` | `cell/tests.rs::parse_real_fo3_megaton_cell_baseline` | now `cell/tests/integration.rs::*` |
| `audit-renderer.md` | `acceleration.rs::SKINNED_BLAS_REFIT_THRESHOLD` | now `acceleration/constants.rs::SKINNED_BLAS_REFIT_THRESHOLD` |
| `audit-renderer.md` | `acceleration.rs::AccelerationManager::build_tlas` | now `acceleration/tlas.rs::AccelerationManager::build_tlas` |
| `audit-renderer.md` | `acceleration.rs::AccelerationManager::evict_unused_blas` | now `acceleration/memory.rs::AccelerationManager::evict_unused_blas` (the `evict_unused_blas` body moved to `memory.rs` during the split; verify) |
| `audit-renderer.md` | `acceleration.rs::AccelerationManager::tick_deferred_destroy` | now `acceleration/blas_static.rs::AccelerationManager::tick_deferred_destroy` |
| `audit-renderer.md` | `DBG_FORCE_NORMAL_MAP` constant (#1035 closeout) | renamed to `DBG_RESERVED_20` — already swept in `audit-renderer.md` by #1040, double-check the rename landed everywhere |

## Severity rationale

**MEDIUM** (promoted from default LOW). The "stale baseline that misled an audit in the last 90 days" promotion trigger fires: when I ran the 2026-05-14 tech-debt audit, the Dim 7 + Dim 10 agents both spent extra cycles confirming-then-translating every audit-skill reference, and one (`DBG_FORCE_NORMAL_MAP` rename close-out) almost got re-reported. The same drift will mis-target the next audit-renderer / audit-nif / per-game-compat audit run.

## Proposed fix

Sweep each row above with the documented translation. Same shape as #1040 (Session 34 sweep); estimated effort **small** (~1 h).

For the post-sweep state, document the convention in CLAUDE.md or the audit-skill files themselves: "When a module splits, update the skill's path anchors in the same PR as the split, by adding a row to the per-session-layout memory note that the next audit cycle consumes."

## Completeness Checks

- [ ] **UNSAFE**: n/a (docs only)
- [ ] **SIBLING**: Check every audit-*.md file, including `audit-suite.md`, `audit-incremental.md`, `audit-regression.md`, and `audit-tech-debt.md`. The 29 sites I caught are line-anchored; a symbol-search pass may surface more
- [ ] **DROP**: n/a
- [ ] **LOCK_ORDER**: n/a
- [ ] **FFI**: n/a
- [ ] **TESTS**: After sweep, run `audit-incremental --since main~30` and confirm the per-game-compat skill resolves all symbol anchors to existing files

## Dedup notes

Distinct from **#1040** (CLOSED — that was the Session-34 sweep). #1040's fixes are still in place; this is the Session-36 successor. Translation map for both sessions lives in the per-session-layout memory notes.
Status: Closed (3f75b39)
