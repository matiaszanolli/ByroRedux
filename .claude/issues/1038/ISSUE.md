# Tech-Debt Batch 1: Shader↔Rust constant drift

**Severity**: MEDIUM
**Domain**: renderer, vulkan
**Audit**: docs/audits/AUDIT_TECH_DEBT_2026-05-13.md
**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1038

## Findings consolidated
TD4-003, TD4-004, TD4-005, TD4-006, TD4-013, TD4-015, TD4-020, TD7-020, TD7-023, TD10-002

## Fix
build.rs codegen emitting shaders/include/shader_constants.glsl + gpu_instance.glsl from a Rust source-of-truth module. Drift becomes compile error.
