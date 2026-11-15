# #6963: TD2-2026-11-15-10: Two groundcover files hand-write the global memory barrier that `descriptors::memory_barrier` exists for

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6963
- **Labels**: low,renderer,sync,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Dimension**: 2 · **Severity**: LOW · **Effort**: trivial · **Labels**: `tech-debt` · **Existing**: none (regression of #1061 / TD3-103, the `descriptors.rs` helper)
- **Age**: `groundcover_models.rs:973` aabd99a05 (2026-09-24), `groundcover/frame.rs:791` 637b65264 (2026-09-06)
- **Location**: `crates/renderer/src/vulkan/groundcover/frame.rs:791` (`fn buffer_barrier`, five call sites at `:363,388,468,537,562`), `crates/renderer/src/vulkan/groundcover_models.rs:973` (`fn barrier`, call sites `:811,848`); the home is `crates/renderer/src/vulkan/descriptors.rs:305` (`pub(crate) unsafe fn memory_barrier`, about twenty other call sites across `context/` and the pass files)
- **Description**: both local functions are the same body as the home: build one `vk::MemoryBarrier` from the two access masks and `cmd_pipeline_barrier(cmd, src_stage, dst_stage, empty, &[barrier], &[], &[])`, only wrapped in a safe `fn` with its own SAFETY comment. Every other pass imports `memory_barrier`.
- **Suggested Fix**: delete both, call `unsafe { memory_barrier(device, cmd, ..) }` at the seven sites (each already sits in a recording context). The `frame.rs` scan at `:963` that looks for the literal `buffer_barrier(` between two markers needs the new spelling.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
