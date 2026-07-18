# Batch: #2030, #2031, #2052, #2053

## #2030 — MEM-D3-01: Bindless texture slots leak on every cell revisit
- Severity: MEDIUM · Labels: bug, medium, memory, performance
- Location: `crates/renderer/src/texture_registry.rs:452-459,1024-1054,1063-1083`,
  `check_slot_available:221-229`; drop call site `byroredux/src/cell_loader/unload.rs:170`
- Grow-only slot allocation: re-entering a previously-unloaded cell re-registers
  textures as NEW slots instead of hitting dedup cache. GPU memory reclaimed via
  deferred destroy (not a VRAM leak), but the finite bindless-array slot INDEX
  space (ceiling ~65535) never reclaims dead slots.
- Suggested fix: generational free-list (recycle dropped index after deferred-
  destroy fence proves no live GpuInstance.texture_index references it), OR
  minimum: track live_count separately, gate check_slot_available on it +
  periodic compaction at cell-unload boundaries + surface high-water telemetry.
- Domain: renderer (byroredux-renderer)
- Needs investigation to scope a minimal, safe fix.

## #2031 — PERF-D7-01: NPC spawn re-resolves the same active AI package 14 times
- Severity: MEDIUM · Labels: bug, medium, performance
- Location: `byroredux/src/npc_spawn.rs:1443-1626` (14 call sites),
  `crates/plugin/src/esm/records/misc/ai.rs:288-296` (active_package walk)
- 14 separate active_package_is_*/active_*_location/active_*_target calls each
  re-run the find()+CTDA-evaluation walk from scratch; NPC active package is a
  single winning PackRecord by construction (confirmed AUDIT_ECS_2026-07-16).
- Suggested fix: resolve active_package(...) once at top of spawn tail, match
  procedure_type against 7 PROCEDURE_* constants to build the one Behavior
  component directly. ~40-60 LOC, behavior-preserving.
- Domain: binary (byroredux, npc_spawn.rs)

## #2052 — TD1-003: npc_spawn.rs crossed 2000 LOC — spawn_npc_entity 1045-LOC function
- Severity: LOW · Labels: bug, import-pipeline, low, tech-debt
- Location: `byroredux/src/npc_spawn.rs:671-1815` (spawn_npc_entity),
  `:1813-2400` (spawn_prebaked_npc_entity)
- 6 phases (placement root/skeleton/body/head+hair/equipment/idle anim/
  AI-package gating) in one function; spawn_prebaked_npc_entity duplicates a
  parallel shorter sequence and is missing AI-package gating (already diverged).
- Suggested fix: extract each phase into a private helper; let prebaked path
  share equipment/skeleton helpers instead of reimplementing.
- Domain: binary (byroredux, npc_spawn.rs) — larger refactor, needs care with
  Phase 4 scope check (spawn_npc_entity + spawn_prebaked_npc_entity both huge).

## #2053 — TD1-004: particle.rs crossed 2000 LOC — 867 lines of embedded tests
- Severity: LOW · Labels: bug, nif-parser, low, tech-debt
- Location: `crates/nif/src/blocks/particle.rs` (2273 LOC total)
- Mechanical: extract `mod tests` into `particle_tests.rs`, mirroring the
  existing shader.rs/shader_tests.rs split. No logic change.
- Domain: nif (byroredux-nif)

## Domain classification
- #2030 → `byroredux-renderer`
- #2031 → `byroredux` (binary crate, npc_spawn.rs)
- #2052 → `byroredux` (binary crate, npc_spawn.rs)
- #2053 → `byroredux-nif`

## Ordering (risk-ascending)
1. #2053 (trivial mechanical split)
2. #2031 (well-scoped, concrete suggested fix)
3. #2030 (needs investigation to scope minimal safe fix)
4. #2052 (largest refactor — evaluate scope carefully, may need to split further
   or confirm with user if >5 files / high risk)
