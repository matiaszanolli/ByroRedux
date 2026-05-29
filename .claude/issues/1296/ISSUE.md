# #1296 — C-1: skin-pool overflow_attempt_count is cumulative per-call, not per-frame demand

_Snapshot as filed 2026-05-28 from /audit-publish (AUDIT_RENDERER_2026-05-28_DIM12). GitHub is authoritative for current state._

**Severity**: MEDIUM · **Dimension**: GPU Skinning — telemetry semantics
**Source**: `docs/audits/AUDIT_RENDERER_2026-05-28_DIM12.md` (finding C-1)

**Location**: `crates/core/src/ecs/resources.rs:738-739` (increment), `:685-690` (field doc), `:777-783` (accessor); consumed by the #1284 cap-sizing comment in `crates/renderer/src/vulkan/scene_buffer/constants.rs` + `DebugStats::skin_pool_overflow_attempts`.

**Issue**: `overflow_attempt_count` increments on every over-cap `allocate()` call and is never reset. Over-cap entities are never inserted into `entity_to_slot`, so one persistently-overflowing entity re-increments it every frame. The field docstring says "distinct entities" and the #1284 cap-sizing comment treats it as per-frame over-demand, but it is a monotonic session-cumulative count of over-cap *calls* — reading it as per-frame demand overshoots by the frame count.

**Risk**: The #1284 cap-sizing feedback loop would over-size the pool if it trusts this as per-frame demand. Misleading, not corrupting.

**Suggested fix**: (a) relabel field/accessor/comment as a monotonic cumulative count of over-cap `allocate()` calls, OR (b) if the cap-sizing loop wants per-frame distinct-entity demand, track a separate per-frame `HashSet<EntityId>` high-water and reset each frame.

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in the related path (refit vs batched-build; raster vs compute)
- [ ] **DROP**: if Vulkan objects change, Drop impl still correct
- [ ] **CANONICAL-BOUNDARY**: if the fix touches `material_translate.rs::translate_material` / `Material::resolve_pbr` / import-walk emitter params, per-game logic stays at the NIFAL parser→Material boundary (see /audit-nifal). _(N/A for these skinning/accel findings)_
- [ ] **TESTS**: regression test added for this specific fix
