# #3633 — REN-2026-08-30-D23-03: `PresentationPipeline::recreate` is dead code; the sole resize path open-codes destroy + `new`

**Labels**: `low,renderer,tech-debt,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3633 --json state`.

---

- **Severity**: LOW
- **Dimension**: FSR/Presentation
- **Location**: `crates/renderer/src/vulkan/presentation.rs:686` (`PresentationPipeline::recreate`)
- **Status**: NEW
- **Description**: `recreate` carries the non-obvious contract for rebuilding this pass —
  capture the `VulkanContext`-owned `health_buffers` before `destroy` overwrites them,
  re-read the borrowed (not owned) `overlay_pipeline_layout`, then `Self::new`. Nothing
  calls it. `recreate_swapchain_core` open-codes the same sequence
  (`resize.rs:1007-1050`: `presentation.take()` → `destroy` → `upscaler.recreate` →
  `PresentationPipeline::new(..., &health_handles, ...)`). Because both the struct and the
  method are `pub` inside `pub mod presentation`, rustc raises no dead-code warning.
- **Evidence**: `grep -rn "presentation" $(git ls-files '*.rs') | grep -i recreate` yields
  exactly one hit, `resize.rs:1050`, and that is the `.context("recreate presentation
  pipeline")` string on the `PresentationPipeline::new` call — not a call to `recreate`.
- **Impact**: Two copies of one lifecycle contract, one of which is never exercised by any
  test or run. A future change to the health-buffer or overlay-layout ownership rules can be
  made in the unused copy and appear correct.
- **Needs RenderDoc**: no
- **Suggested Fix**: Delete `recreate`, or make `recreate_swapchain_core` call it (it is
  the shape that documents the contract). Deleting is the smaller change; the resize site's
  own comments already carry both invariants.

---
- **Cross-dimension corroboration**: Found independently three times — also as *D11-05* and *D5-05*.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D23-03

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
