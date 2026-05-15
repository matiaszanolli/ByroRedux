# #1066 — REN-D14-NEW-06: ScratchTelemetry doc has inverted dedup-ratio formula

**Severity**: LOW  
**Audit**: `docs/audits/AUDIT_RENDERER_2026-05-14_DIM14.md`  
**Location**: `crates/core/src/ecs/resources.rs:333-334`

## Summary

Doc says `Dedup ratio = materials_unique / materials_interned` (fraction < 1).
Actual code in `commands.rs:350` computes `materials_interned / materials_unique` (multiplier > 1).

## Fix

Update the doc comment to reflect the actual formula:
```rust
/// Dedup ratio = `materials_interned / materials_unique`. A value > 1
/// means dedup is saving SSBO space; near 1.0 means nearly every draw
/// uses a unique material. A *drop* signals a dedup regression.
```
