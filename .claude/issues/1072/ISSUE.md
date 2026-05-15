# #1072 — F-WAT-12: WaterFlow.direction doc says "Z-up before swizzle" but component is Y-up

**Severity**: LOW / INFO  
**Audit**: `docs/audits/AUDIT_RENDERER_2026-05-14_DIM17.md`  
**Location**: `crates/core/src/ecs/components/water.rs:190-191`

## Summary

Doc comment says "Z component is non-zero for waterfalls — falls go down in world Z-up before the Y-up swizzle". The component lives in Y-up space (swizzle already applied in cell loader). Waterfalls fall in -Y, not -Z. Misleads future maintainers about the coordinate space.

## Fix

```rust
/// Unit vector in **world Y-up space**. Y component is typically
/// `-1.0` for waterfalls (falls are downward in Y-up); horizontal
/// currents keep Y=0. Set from the WATR `wind_direction` angle after
/// Z→Y swizzle in `cell_loader/water.rs`.
pub direction: [f32; 3],
```
