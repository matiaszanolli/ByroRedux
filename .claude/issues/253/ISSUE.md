# Issue #253 — PERF-04-11-L8

**Title**: build_render_data allocates skin_offsets HashMap every frame
**Severity**: LOW
**Dimension**: CPU Allocations
**Audit**: `docs/audits/AUDIT_PERFORMANCE_2026-04-11.md`
**Location**: `byroredux/src/render.rs:37`

## Summary
`let mut skin_offsets: HashMap<EntityId, u32> = HashMap::new();` fresh every frame. Move to persistent render state, rebuild only on skinned spawn/despawn.

## Fix with
`/fix-issue 253`
