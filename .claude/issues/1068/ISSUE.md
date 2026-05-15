# #1068 — F-WAT-06: Duplicate trig computations in WATR resolver

**Severity**: LOW  
**Audit**: `docs/audits/AUDIT_RENDERER_2026-05-14_DIM17.md`  
**Location**: `byroredux/src/cell_loader/water.rs:346-353`

## Summary

`theta.cos()`, `theta.sin()`, and `wind_speed.abs().max(0.5)` each computed twice. Cache into locals before both uses.

## Fix

```rust
let (cos_theta, sin_theta) = (theta.cos(), theta.sin());
let speed = rec.params.wind_speed.abs().max(0.5);
flow = Some(WaterFlow { direction: [cos_theta, 0.0, sin_theta], speed });
let dir = (cos_theta, sin_theta);
```
