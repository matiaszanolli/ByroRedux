# TD2-2026-10-19-10: Residue of #5809 and #5823: three clamp samplers and eight macro-shaped `GpuUploadCtx` literals are still hand-built (prior-partial: TD2-2026-10-15-08, -22)

Labels: low,renderer,vulkan,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: samplers `crates/renderer/src/vulkan/context/helpers.rs:612` (depth-history: NEAREST/NEAREST + `max_lod(0.0)`, i.e. `clamp_sampler(NEAREST, NEAREST)` for a 1-mip image), `placeholder.rs:106`, `groundcover_bench.rs:556`; upload ctx `context/resize.rs:667,718,1284`, `byroredux/src/cell_loader/terrain.rs:994`, `byroredux/src/scene.rs:784`, `byroredux/src/cornell/builders.rs:388,434`, `cornell/oracle.rs:433`
- **Age**: sampler/ctx macros 2026-10-15; the stragglers were never migrated
- **Effort**: trivial
- **Description**: `clamp_sampler`'s doc says the two different samplers (shadow, wrap/repeat) are the only exceptions, but the three above are plain clamp samplers. Each of the eight literals has the exact `device/allocator/queue/transfer_pool` shape `upload_ctx!($ctx)` expands to (the `scene.rs`/`builders.rs` ones pre-bind `alloc` locally, which the macro's own doc says keeps compiling).
- **Proposed fix**: migrate the three to `descriptors::clamp_sampler` (the extra `.mipmap_mode` is a parameter) and the eight to `upload_ctx!` / `byroredux_renderer::upload_ctx!`.

Related: #5809, #5823 (closed; residue).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
