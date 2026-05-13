# #1003 — REN-D3-NEW-02: SkinSlot output buffers freed by eviction policy, not by entity despawn

- **Severity**: MEDIUM
- **Domain**: renderer / memory
- **Audit**: `docs/audits/AUDIT_CONCURRENCY_2026-05-13.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1003

## TL;DR
SkinSlot output buffers released only by per-frame eviction pass (~3 frames after cell unload). Cell-unload-without-render-tick (headless tests, paused world) silently retains all slots.

## Fix
Hook into `unload_cell`: walk `ctx.skin_slots` + `ctx.accel_manager.skinned_blas_entities()` for victim membership, call `skin.destroy_slot` + `accel.drop_skinned_blas` directly. Symmetric with mesh/texture refcount drop loop in `unload.rs:217-222`.

## Bundle with
#1004 (failed_skin_slots leak — same `unload_cell` hook site).
