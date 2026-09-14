# #4363 — TD4-002: The skill's GPU-struct size recipes grep a file that has no `GpuMaterial` pin

**Labels**: low, tech-debt, documentation, doc-rot
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4363

- **Severity**: LOW · **Dimension**: 4
- **Location**: `.claude/commands/audit-tech-debt/SKILL.md:321-327` (Dim 3), `:452-458` (Dim 8) · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot
- **Finding**: Both recipes grep only `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs`; `gpu_material_size_is_428_bytes` lives in `crates/renderer/src/vulkan/material_tests.rs`.
- **Suggested Fix**: `grep -rn "fn gpu_.*_is_[0-9]\+_bytes\|size_of::<Gpu" crates/renderer/src/vulkan/`.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: If practical, a hygiene/gate check prevents this doc-rot class recurring
