# Issue #470

FNV-3-M1: spawn_terrain_mesh ignores ATXT/VTXT splat layers — FNV terrain looks like solid dirt

---

## Severity: Medium

**Location**: `byroredux/src/cell_loader.rs:694-716`

## Problem

LAND parser (`crates/plugin/src/esm/cell.rs:855-961`) correctly decodes per-quadrant BTXT + all ATXT+VTXT splat layers with 17×17 alpha grids. Renderer side does:

```rust
quadrants.iter().find_map(|q| q.base)
```

One texture for the whole 4096-unit tile. Parsed `TerrainTextureLayer::alpha` is never read. The splatting chain doesn't exist at the renderer edge. In-code comment acknowledges deferral but no issue was filed.

## Impact

Every FNV road, grass strip, rocky outcrop, paved area — anything requiring ≥2 texture layers — renders as solid base dirt. Capital Wasteland + Mojave exterior visually wrong. DLC worldspaces (Zion, Big MT, Divide, The Pitt) same.

## Reference

UESP "Mod File Format/LAND" — ATXT/VTXT layer format; 8 max layers per cell.

## Fix

Thread `quadrants[i].layers` into a new per-vertex splat-weight attribute (pack into 2× RGBA8; 8 weight channels covers UESP's 8-layer cap). Bind a splat-map texture array, blend in a new terrain fragment shader pass.

This is M32 Phase 2.5 scope.

## Completeness Checks

- [ ] **TESTS**: Load an FNV exterior cell with known multi-layer terrain (e.g. near HELIOS One), visual regression vs single-layer baseline
- [ ] **SIBLING**: Check if Oblivion LAND uses the same layer format — single implementation could serve both
- [ ] **DROP**: New texture array binding — verify teardown in `unload_cell` + `VulkanContext::Drop`
- [ ] **SHADER**: New terrain frag shader — must update triangle.vert GpuInstance sync if vertex attributes change (Shader Struct Sync memory note)

Audit: `docs/audits/AUDIT_FNV_2026-04-20.md` (FNV-3-M1)
