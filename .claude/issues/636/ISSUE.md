# FNV-D4-02: cluster_cull MAX_LIGHTS_PER_CLUSTER=32 silently drops overflow — no telemetry

## Finding: FNV-D4-02

- **Severity**: LOW (silent clipping)
- **Source**: `docs/audits/AUDIT_FNV_2026-04-24.md`
- **Game Affected**: FNV (Lucky 38, Ultra-Luxe, dense xenon-rack interiors), Skyrim alchemy labs, FO4 Vault interiors with massed lights
- **Location**: [crates/renderer/shaders/cluster_cull.comp:16, 138](crates/renderer/shaders/cluster_cull.comp#L16)

## Description

Loop terminates on `count < MAX_LIGHTS_PER_CLUSTER` (32) and writes no overflow flag. FNV interiors with dense xenon racks (Lucky 38, Ultra-Luxe casino floor) can put 40+ small-radius lights in a single near-camera cluster. The 33rd+ light in that cluster is dropped without diagnostic.

Visible only as "lights closer to the camera win"; hard to diagnose without instrumentation.

## Suggested Fix

Two complementary options:

1. **Atomic overflow counter** (cheap, high-signal): add a global `atomicCounter` (or `atomicCounterIncrement`) when the cluster fill count would have exceeded the cap. Surface in `sys.stats` so the user can see "Cluster light overflow this frame: N" without rebuilding the shader.

2. **Widen MAX_LIGHTS_PER_CLUSTER to 64** (cost: 2× cluster index SSBO, ~256 KB at 16×9×24). Defer until #1 confirms vanilla content actually exceeds 32 in practice.

Start with #1 — it's the diagnostic that tells you whether #2 is needed.

## Related

- FNV-D4-01 (companion) — cluster_cull `FAR=10000` hardcoded; same shader, bundle if both fixes touch it.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: If a counter is added, verify it follows the same pattern as the existing ray-budget counter (#270 / 6f70872) — per-FIF reset, host-readable.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Synthetic cell with > 32 small-radius lights in one cluster volume; assert overflow counter > 0 in `sys.stats`.

_Filed from audit `docs/audits/AUDIT_FNV_2026-04-24.md`._
