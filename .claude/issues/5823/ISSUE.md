# TD2-2026-10-15-22: `GpuUploadCtx { device, allocator, queue, command_pool }` is spelled out 22 times with no constructor

Labels: low,renderer,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD2-2026-10-15-22`).

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `byroredux/src/{scene.rs (3), cornell/builders.rs (3), cell_loader/water.rs (3), hud.rs (2), cell_loader/spawn/mesh_instance.rs (2), app_frame.rs (2), scaleform_hud.rs, loading_screen.rs, scene/nif_loader.rs, cornell/oracle.rs, cell_loader/terrain.rs}`; `crates/renderer/src/vulkan/{buffer.rs (5), context/resources.rs (3), context/init.rs (2), context/resize.rs (2), taa.rs, svgf.rs, texture.rs}`; `crates/renderer/src/mesh/geometry_ssbo.rs (3)`; struct `crates/renderer/src/vulkan/mod.rs:83`
- **Status**: NEW (borrow-checker caveat below; one of the weaker findings)
- **Age**: the struct was introduced to stay under `too_many_arguments`
- **Effort**: small
- **Description**: the 7-line literal with `ctx.allocator.as_ref().unwrap()` recurs, and `hud.rs:578` / `scaleform_hud.rs:286` additionally repeat `register_rgba(upload_ctx, w, h, &vec![0u8; (w * h * 4) as usize])`.
- **Evidence**: `grep -rn 'command_pool: ctx.transfer_pool\|command_pool: self.transfer_pool'` = 22 outside tests.
- **Impact**: a fifth handle (a timeline semaphore for the upload queue) is a 22-site edit. A `&self` constructor would conflict with the `&mut ctx.texture_registry` borrow at most call sites, which is likely why none exists.
- **Related**: #5593.
- **Suggested Fix**: `impl VulkanContext { fn upload_parts(&mut self) -> (GpuUploadCtx<'_>, &mut TextureRegistry, &mut MeshRegistry) }` (split borrow in one place), or a `macro_rules! upload_ctx!(ctx)` that expands to the literal.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
