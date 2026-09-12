# REN-2026-09-11-D23-01: resize path recomputes BLAS budget with stale FSR SDK memory footprint before upscaler recreation

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4111
**Labels**: bug, renderer, medium, memory

**Severity**: MEDIUM
**Dimension**: FSR/Presentation (Memory/Lifecycle)
**Location**: `crates/renderer/src/vulkan/context/resize.rs` (`recreate_bloom_and_volumetrics`'s `recompute_blas_budget` call at line 885; `recreate_screen_passes`'s ordering relative to `recreate_taa_and_presentation` at line 1051)
**Status**: NEW (from `docs/audits/AUDIT_RENDERER_2026-09-11.md`)

## Description
`#3988` correctly widened `recompute_blas_budget`'s signature to include the FSR SDK memory footprint and fixed both *construction-time* call sites (`init.rs`) with an explicit two-phase pattern (call with `0` before the upscaler exists, call again with the real figure right after). The *resize* path only got the first half: `recreate_bloom_and_volumetrics` reads `self.frame_upscaler.sdk_memory_bytes()` from the pre-resize upscaler object while pairing it with the already-updated new `frame_extents`, and `recreate_taa_and_presentation` (which actually calls `frame_upscaler.recreate(...)`, refreshing `sdk_memory_bytes` for the new extent) runs *after* that budget recompute. There are only 3 call sites for `recompute_blas_budget` in the crate (`init.rs` ×2, `resize.rs` ×1) — nothing re-derives the budget after the upscaler catches up, so the mismatch persists until the *next* resize (repeating the same one-cycle lag against the next extent). `set_upscaler_mode` reuses this same resize path, so runtime upscaler switches are affected identically.

## Evidence
Confirmed by direct read of `crates/renderer/src/vulkan/context/resize.rs`:
```rust
// recreate_bloom_and_volumetrics (~line 881-885)
let extents = self.frame_extents;               // already the NEW extent
let sdk_bytes = self.frame_upscaler.as_ref().map_or(0, |u| u.sdk_memory_bytes()); // still the OLD upscaler
accel.recompute_blas_budget(extents, volumetrics_config, sdk_bytes);
// ... later in the same recreate_screen_passes sequence ...
// recreate_taa_and_presentation (line 1051), which calls:
upscaler.recreate(..., self.frame_extents)?;   // NOW rebuilds sdk_memory_bytes for the new extent — too late
```
`grep -rn recompute_blas_budget crates/renderer/src` confirms exactly 3 call sites (`resize.rs:885`, `init.rs:1013`, `init.rs:1457`), none after the `recreate_taa_and_presentation` step.

## Impact
Budget-accuracy only — same failure class as the now-fixed `#3839`/`#3988` (BLAS eviction starting too late on small-VRAM cards under-reserves for whatever the FSR SDK context's actual footprint is at the new resolution). Not reproduced on the 12 GB dev card; flagged per this project's speculative-Vulkan-fix policy as a code-shape finding needing verification on small-VRAM hardware, not a measured failure.

## Related
`#3839`, `#3988` (fixed the construction-time half of this exact bug class)

## Suggested Fix
Move the `recompute_blas_budget` call in the resize path to after `recreate_taa_and_presentation`, or add a second call there once the upscaler reflects the new extent — mirroring `init.rs`'s two-phase shape.

## Completeness Checks
- [ ] **SIBLING**: `init.rs`'s two-phase `recompute_blas_budget` pattern (call before upscaler exists, call again after) applied identically to the resize path
- [ ] **TESTS**: A regression test pins that the resize-path budget recompute runs after (or is re-run after) `recreate_taa_and_presentation`
