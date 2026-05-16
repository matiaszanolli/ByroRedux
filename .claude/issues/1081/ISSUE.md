# REN-D19-001: No fallback binding for bloomTex when bloom pipeline is absent

**GitHub**: #1081  
**Severity**: HIGH  
**Domain**: renderer  
**Audit**: AUDIT_RENDERER_2026-05-15

## Root Cause

The struct-field doc at `mod.rs:929-930` promises "composite's bloom binding falls back to a black dummy so the additive contribution becomes a no-op." The code at `mod.rs:1597-1606` and `resize.rs:468-476` instead returns `Err(...)` causing a hard engine init failure if bloom allocation fails.

## Files to Change

1. `crates/renderer/src/vulkan/composite.rs` — add 1×1 dummy bloom image when bloom_views is empty
2. `crates/renderer/src/vulkan/context/mod.rs` — remove hard Err, pass empty slice when bloom is None; fix doc comment
3. `crates/renderer/src/vulkan/context/resize.rs` — same, pass empty slice when bloom is None
