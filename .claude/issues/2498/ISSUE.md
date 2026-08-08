# REN-D11-2026-08-07-01: find_depth_format error message names candidates that were removed

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2498
**Finding ID**: REN-D11-2026-08-07-01 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 11 — Pipeline/RenderPass
**Location**: `crates/renderer/src/vulkan/context/helpers.rs:45` (`find_depth_format`)
**Status**: NEW (follow-on drift from the REN-D4-NEW-02 fix, audit 2026-05-11 DIM4)

## Description
The candidate list was narrowed to pure-depth formats to fix the packed depth-stencil aspect/layout foot-gun, but the `bail!` diagnostic still advertises the two removed packed formats.

## Evidence
```rust
let candidates = [vk::Format::D32_SFLOAT, vk::Format::D16_UNORM];
...
anyhow::bail!("No supported depth format found (tried D32, D32S8, D24S8, D16)")
```

## Impact
On the (very unlikely) device where both candidates fail, the error blames the engine for having tried packed formats it never tried, sending the reader looking for a nonexistent fallback path. Diagnostic-only.

## Related
REN-D4-NEW-02 (`AUDIT_RENDERER_2026-05-11_DIM4.md`).

## Suggested Fix
Change the message to `(tried D32_SFLOAT, D16_UNORM)`, or build it from `candidates` so it can't drift again.

## Completeness Checks
- [ ] **TESTS**: N/A (message-only change); consider deriving it from `candidates` so it self-updates
