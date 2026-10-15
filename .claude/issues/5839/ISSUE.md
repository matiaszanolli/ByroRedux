# TD4-2026-10-15-03: `.claude/agents/*.md` is outside both the validator and the sync, and two of the three agents carry stale architecture facts

Labels: low,tech-debt,documentation,doc-rot

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD4-2026-10-15-03`).

- **Severity**: LOW
- **Dimension**: 4 - Audit-Finding Rot
- **Location**: `.claude/agents/renderer-specialist.md:14,31,32-34`; `.claude/agents/ecs-specialist.md:29`
- **Status**: NEW
- **Age**: last touched 2026-05-12 (`b38f3ac74`), five months and hundreds of commits ago
- **Effort**: small
- **Description**: These agents are loaded into renderer/ECS audit sub-agents. Verified stale: (1) `src/vulkan/context.rs` is now `context/` (a dir; the validator misses it because it checks only `command_files` + `docs/engine`); (2) "gpu-allocator 0.27" but `Cargo.toml:87` pins 0.28; (3) "Push constants: 128 bytes (mat4 viewProj + mat4 model)": the main pipeline in `pipeline.rs` declares no push-constant range (per-draw data is the instance SSBO); the remaining ranges are small per-pass ones (`water.rs`, `svgf.rs`, `caustic.rs`, `presentation.rs`, `groundcover_models.rs`); (4) "`GpuBuffer` (vertex/index, HOST_VISIBLE for now)"; (5) ecs-specialist: "`systems.rs` itself is a 27-line module index" (it is 81 lines), `footstep_system` "lives in `animation.rs`" (it lives in `systems/audio.rs`), and the `systems/{...}.rs` list names 9 files of the 36 in the directory. `audit-sync`'s `meta` group owns `_audit-*` and `.agents/skills` but not `.claude/agents/`.
- **Evidence**: `grep -n "push_constant_ranges" crates/renderer/src/vulkan/pipeline.rs` (none); `wc -l byroredux/src/systems.rs` = 81; `ls byroredux/src/systems | wc -l` = 36 files; `grep -rln "fn footstep_system" byroredux/src` = `systems/audio.rs`.
- **Impact**: specialists asked to review sync or push constants start from a false premise; same failure class as skill premises, with no gate.
- **Related**: `audit-sync/SKILL.md:43-45` (meta group scope).
- **Suggested Fix**: rewrite the two agent files' layout/invariant sections to point at `CLAUDE.md` + `docs/engine/renderer.md` instead of hand-typed trees; add `.claude/agents/*.md` to `command_files` in `_audit-validate.sh` and to the `meta` group's remit.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
