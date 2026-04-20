# Issue #479

FNV-REN-L1: TAA dispatch failure silently falls through — stale frozen HDR on permanent device loss

---

## Severity: Low

**Location**: `crates/renderer/src/vulkan/context/draw.rs:1032-1040` (TAA); same pattern at `:1008-1010` (SVGF) and `:1027-1029` (caustic)

## Problem

```rust
if let Some(ref mut taa) = self.taa {
    if let Err(e) = taa.dispatch(&self.device, cmd, frame) {
        log::warn!("TAA dispatch failed: {e}");
    }
}
```

Dispatch failure logs `warn!` and continues. Composite still samples the descriptor bound to the TAA output image — which still holds whatever TAA wrote on the last successful dispatch.

On **permanent** failure (lost device, descriptor pool exhaustion, driver crash), the screen keeps rendering a stale frozen HDR frame with no user-facing indication that the pipeline has failed.

Same silent-fall-through pattern applies to SVGF (line 1008) and caustic (line 1027).

## Impact

Hard to reproduce (needs lost-device condition). But when it hits, diagnostic is much harder than it should be — a black screen or explicit error would save minutes of debugging.

## Fix

On the **first** dispatch failure in a given frame, switch the composite descriptor back to raw HDR (call `rebind_hdr_views` with a known-good raw HDR image view at `SHADER_READ_ONLY_OPTIMAL` layout). Alternatively, flag `self.taa_failed = true` and skip the dispatch permanently, rebinding once.

## Completeness Checks

- [ ] **TESTS**: Simulate `taa.dispatch` failure (inject error), assert composite still produces output
- [ ] **SIBLING**: Apply same recovery to SVGF (`:1008`) and caustic (`:1027`)
- [ ] **DROP**: If rebinding HDR views, ensure we don't free the TAA image while descriptors still point to it

Audit: `docs/audits/AUDIT_FNV_2026-04-20.md` (FNV-REN-L1)
