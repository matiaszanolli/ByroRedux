# D12-2026-08-07-01: record_post_passes returns a Result that can never be Err -- the caller's recovery branch is dead code that contradicts the #2146 invariant

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2503
**Finding ID**: D12-2026-08-07-01 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 12 — Command-buffer recording
**Location**: `crates/renderer/src/vulkan/context/post_passes.rs::record_post_passes` (sig at :168, body :194-223); caller `crates/renderer/src/vulkan/context/draw.rs:2914-2943`
**Status**: NEW (pre-existing; predates the #2258 split — verified `7bb517b2^` was equally infallible)

## Description
`record_post_passes` calls eight `record_*_pass` helpers, all of which return `()`, then ends with an unconditional `Ok(())`. It is structurally incapable of returning `Err`. The caller nevertheless wraps it in a 30-line `if let Err(e) = ... { recreate_image_available_for_frame(); return Err(e); }` recovery block. That block is unreachable today — but it is exactly the escape hatch `#2146` warns must not exist. `record_upscale_pass`'s own doc says: "`record` is infallible on purpose. It runs after `svgf.dispatch`/`taa.dispatch` have latched `dispatched_this_frame`, so an error escaping to `draw_frame` would skip `queue_submit` *and* `mark_frame_completed`, leaving those latches set for a dispatch that never reached the GPU." Keeping the fallible signature means a contributor who adds a single `?` inside any of the eight new helpers silently activates that hazard with no compile-time or test signal.

## Evidence
```rust
// post_passes.rs:194-223 — no `?`, no fallible call
self.record_svgf_pass(cmd, frame);
... self.record_presentation_pass(cmd, frame, img, underwater, image_space_modifier);
Ok(())
```
vs `draw.rs:2914` `if let Err(e) = self.record_post_passes(...) { ... return Err(e); }`

## Impact
No runtime effect today. Latent: a future fallible call between the SVGF/TAA `dispatched_this_frame` latch and `queue_submit` would bail the frame with the latches set, so `mark_frame_completed` never runs and the next frame assumes temporal history the GPU never wrote (ghosting / stale-history artifacts) — the precise failure #2146 documented. Blast radius: the whole post chain.

## Related
#2146 (`FrameUpscaler::record` infallibility contract), #2258 (`7bb517b2` per-pass split), #917 / REN-D10-NEW-03 (`mark_frame_completed` moved to post-submit).

## Suggested Fix
Change `record_post_passes` to return `()` and delete the caller's recovery branch, so any future fallible call is a compile error at the point of introduction rather than a silent semantic change; carry the #2146 rationale onto the new signature as a doc comment.

## Completeness Checks
- [ ] **TESTS**: Compile-time check: any future fallible call inside the eight helpers now fails to compile rather than silently changing semantics
