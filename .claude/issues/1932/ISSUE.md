# TAA-D13-01: Halton jitter gate omits the taa_failed check present on TAA's other two gates

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1932

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/src/vulkan/context/draw.rs:2490`
**Status**: NEW

## Description
The Halton jitter gate is `if self.taa.is_some()` only. It does not also check `!self.taa_failed`, unlike the dispatch gate and `upload_params`, both of which gate on `if !self.taa_failed`. If `taa_failed` were ever latched at runtime, geometry would keep being rendered with a per-frame sub-pixel Halton offset while composite falls back to raw, temporally-unresolved HDR — full-frame sub-pixel shimmer/wobble until the next swapchain resize clears the latch.

## Evidence
grep of all five `taa_failed` sites: set at `draw.rs:754`, read at `:744` and `:3635`, reset only at `resize.rs:736`; the jitter block at `draw.rs:2490` reads only `self.taa.is_some()`.

## Impact
None today — `TaaPipeline::dispatch` is infallible (every device call is void-returning, body ends `Ok(())`), so the `Err` arm that sets `taa_failed` can never fire. Reachable only if a future change makes `dispatch` fallible (e.g. a push-constant upload or a fallible barrier helper).

## Related
Same `taa_failed` machinery as the SVGF `svgf_failed` path

## Suggested Fix
Change the gate to `if self.taa.is_some() && !self.taa_failed`, so a fallback frame renders as a stable pinhole image rather than a jittered-but-unresolved one. Low urgency; safe to defer until `dispatch` gains a fallible path.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
