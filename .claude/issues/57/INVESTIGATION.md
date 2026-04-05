# Investigation: Issue #57

## Root Cause
sample_blended_transform (animation.rs:448-528) allocates:
1. `samples: Vec<(u8, f32, Vec3, Quat, f32)>` — per-layer collection
2. `top: Vec<_>` — filter-collect for highest priority

Called per bone per frame. 100-bone skeleton = 200 allocations/frame.

## Fix
Two-pass in-place approach with no allocations:
1. First pass: find max_priority across layers (no collection needed)
2. Second pass: blend only layers with max_priority, accumulate inline

No Vec, no SmallVec — just two iterations over stack.layers with
early-continue for layers without this channel.
