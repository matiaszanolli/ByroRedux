# TD2-2026-10-25-12: compute-pass teardown (pipeline / layout / pool / set layout) is hand-written per pass with two different guard policies

Labels: low, renderer, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6371
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Dimension**: 2
- **Location**: 34 `destroy_pipeline_layout(` and 23 `destroy_descriptor_pool(` sites across 21 files in `crates/renderer/src/vulkan/`. `svgf.rs:1401-1440` null-guards each handle and resets it to null (needed for the `try_or_cleanup!` partial-init path); `ssao.rs:527-530` calls `destroy_pipeline`/`destroy_pipeline_layout` unguarded; `bloom.rs:924-945` guards per handle. Construction is already one home (`pipeline::create_compute_pipeline`, pipeline.rs:58); teardown has none.
- **Effort**: medium
- **Fix**: add `descriptors::destroy_pass_objects(device, &mut PassHandles { pipeline, layout, pool, set_layout })` (guarded + nulling, same reverse order as today's hand code) next to `DescriptorPoolBuilder`, and move passes onto it one at a time (svgf/taa/ssao/caustic first). Order-preserving host refactor; the `// SAFETY` comments the `undocumented_unsafe_blocks` deny requires then live once. Do not combine with any barrier or lifetime change (*feedback_speculative_vulkan_fixes*).

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD2-2026-10-25-12`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct (no barrier/order change without RenderDoc or `BYRO_VALIDATION=1` — feedback_speculative_vulkan_fixes)
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
