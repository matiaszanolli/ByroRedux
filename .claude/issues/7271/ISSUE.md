# #7271: INC-R-04: `GpuCamera` grew to 384 B, but two docs and two audit skills still carry 368 B and the old test name

- **Severity**: LOW
- **Dimension**: audit-renderer D3 / doc rot (and audit infrastructure)
- **Location**: `docs/engine/shader-pipeline.md:896`, `docs/engine/memory-budget.md:195`, `.claude/commands/audit-renderer/SKILL.md:86`, `.claude/commands/audit-safety/SKILL.md:279`
- **Changed in**: 84104564e (`GpuCamera` 368 → 384 B with `interior_sky`; the size test was renamed `gpu_camera_is_368_bytes` → `gpu_camera_is_384_bytes`)
- **Status**: NEW. Same class as the closed #3447 / #3450 / #3599.
- **Description**: The struct, its rustdoc (`gpu_types.rs:490`) and its pin (`gpu_instance_layout_tests.rs:60`) all say 384 B, but four other places were not updated:
  - The shader-pipeline binding table still says "`GpuCamera` (368 B)".
  - The memory-budget Camera UBO row still says 368 B per frame, 736 B total.
  - The audit-renderer and audit-safety skills still name the deleted `gpu_camera_is_368_bytes` as the guard.
- **Evidence**:
  - `_audit-validate.sh` now fails with `gpu_camera_is_368_bytes  audit-renderer audit-safety`, 1 suspect symbol.
  - `shader-pipeline.md:896`: `| 1 | 1 | UNIFORM_BUFFER | GpuCamera (368 B) | …`.
  - `memory-budget.md:195`: `| Camera UBO | — | 1 | 368 B (#3323) | 368 B | 736 B |`.
- **Impact**: Doc-only, but it turns the audit validator red. Until it is fixed, the validator exits at the symbol check, which hides its closed-issue advisory pass, and every `/audit-publish` path gate fails.
- **Related**: #3447, #3450, #3599
- **Suggested Fix**: Change the sizes at all four sites to 384 B (the budget doubles to 768 B) and the test name to `gpu_camera_is_384_bytes`. A test that reads the doc's size from `size_of::<GpuCamera>()` would stop this class from recurring.

**Found by**: `docs/audits/AUDIT_INCREMENTAL_2026-11-22b.md`

## Completeness Checks
- [ ] **SIBLING**: every doc / skill naming the GpuCamera size or its test updated
- [ ] **TESTS**: _audit-validate.sh passes
