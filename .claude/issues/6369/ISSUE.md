# TD2-2026-10-25-10: residual of #6148 — depth-read-only and arbitrary-layout `DescriptorImageInfo` still hand-spelled in five passes

Labels: low, renderer, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6369
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Dimension**: 2
- **Location**: `DEPTH_STENCIL_READ_ONLY_OPTIMAL` infos at `crates/renderer/src/vulkan/ssao.rs:272`, `caustic.rs:631`, `svgf.rs:925`, `composite.rs:885` and `:1235`; arbitrary-layout infos at `composite.rs:872,897,1225,1251` (`indirect_layout` / `shadow_correction.layout`). `descriptors.rs:93-116` has only `sampled_image_info` (SHADER_READ_ONLY), `general_sampled_image_info` (GENERAL) and `storage_image_info`. #6148 went 92 → 14 hand spellings; these are the remaining 9 of that class (the other 5 are `descriptors.rs` itself and `texture_registry/mod.rs`'s bindless exception).
- **Effort**: trivial
- **Fix**: add `depth_read_only_image_info(sampler, view)` and `image_info(sampler, view, layout)` to `descriptors.rs`; composite's create and recreate paths (`:872-900` vs `:1225-1254`, the same three infos twice) call them. Pure host-side struct building, no barrier/order change.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD2-2026-10-25-10`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct (no barrier/order change without RenderDoc or `BYRO_VALIDATION=1` — feedback_speculative_vulkan_fixes)
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
