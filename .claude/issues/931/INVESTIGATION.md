# Issue #931 Investigation — Bloom barrier coalesce

## Audit claim recap

> 19 barriers / frame; collapse to ~3 by packing into a single
> mip-chain image and using subresource_range per barrier. Says
> "the barrier API doesn't know that, so we emit a full memory
> barrier instead of relying on the natural execution dependency
> between dispatches reading/writing distinct images."

## The audit's premise is partially wrong

There is no "natural execution dependency between dispatches" in
Vulkan — `vkCmdDispatch` calls overlap on the GPU unless
explicitly sequenced via barriers. Different images do not change
that. So packing 5 down + 4 up into one image with mip levels
**does not** let us collapse inter-dispatch barriers. We still
need a memory barrier between every consecutive dispatch in each
chain.

The only way to get to "~3 barriers" is FidelityFX **SPD** —
a single-pass downsampler that uses workgroup atomic counters +
shared LDS to compute all mip levels inside one dispatch. That's
a several-hundred-LOC shader rewrite, not a 150-LOC refactor.

## What is actually achievable with low risk

`bloom.rs::dispatch` emits two `cmd_pipeline_barrier` calls per
mip iteration:

- **pre-barrier** (`bloom.rs:550-565` for down, `:607-622` for up):
  `SHADER_READ → SHADER_WRITE` on `mip[i]` — gates "last frame's
  read" against "this frame's write".
- **post-barrier** (`bloom.rs:583-598` for down, `:637-659` for up):
  `SHADER_WRITE → SHADER_READ` on `mip[i]` — makes this dispatch's
  write visible to next iteration's read.

The pre-barriers are **redundant**. Reasoning:

1. Each `BloomFrame` slot owns its own `down_mips[]` /
   `up_mips[]` images. Frame 0's mips and frame 1's mips are
   distinct allocations (`bloom.rs:367-380`, `:743-768`).
2. Cross-frame WAR hazard on the shared slot is handled by the
   per-frame fence: frame N waits on frame
   N - MAX_FRAMES_IN_FLIGHT's fence before recording, which makes
   that submission's GPU work fully complete (visible + available)
   before any new command targets the same image.
3. Within the current frame's command buffer, no prior dispatch
   has touched `mip[i+1]` before iteration i+1 writes it. The
   post-barrier on iteration i (srcStage=COMPUTE → dstStage=COMPUTE)
   also acts as an execution barrier — iteration i+1's writes
   can't begin until iteration i's compute completes — which
   suffices because there's no in-frame WAR/RAW conflict on
   `mip[i+1]`.
4. `composite` reads `up_mips[0]` only. That read happens in the
   same command buffer, after the up chain. The last up
   dispatch's post-barrier already targets `FRAGMENT_SHADER` for
   `i==0` (`bloom.rs:646-650`). Cross-frame composite reads on
   the same slot are again gated by the per-frame fence.

Dropping all 9 pre-barriers leaves:

- 1 HOST→COMPUTE UBO barrier
- 9 post-barriers (one per dispatch — needed between consecutive
  reads-writes of the same mip)
- = **10 barriers/frame**, down from 19

That's a 47% reduction with no shader/pipeline changes and no
risk of subresource view churn.

## What about the single-image-multi-mip pack?

It's orthogonal to the barrier count and only buys:

- One `vk::Image` allocation handle instead of 9 per frame
- Marginally simpler `Drop`
- Better L2 locality (same backing memory)

…at the cost of N image-view-per-mip plumbing and rewriting all
the descriptor set construction. Not worth bundling with this
fix — file as a separate cleanup if the VRAM/handle count ever
matters.

## Plan

Single-file change in `crates/renderer/src/vulkan/bloom.rs`:

1. Delete pre-barrier code (down loop and up loop).
2. Add a header comment block explaining why pre-barriers are
   redundant — to prevent a future contributor from "fixing" the
   missing barriers.
3. Update the per-frame barrier-count assertion in any test that
   pins it (none currently exist, per grep).

Audit's "~3 barriers" target is rejected with a clear
explanation in the commit body — single-pass SPD remains a
future ROADMAP item if ever needed.

## Completeness checks

- **UNSAFE**: No new unsafe; existing `cmd_pipeline_barrier`
  calls are simply removed.
- **SIBLING**: SVGF / volumetrics use a different pattern
  (single dispatch per pass, not mip chains) — not the same
  bug. ssao.rs is single-pass too. No siblings to update.
- **DROP**: Untouched.
- **TESTS**: No barrier-count test exists. Cube-demo golden
  frame regression test in `byroredux/tests/golden_frames.rs`
  validates final composite output is bit-identical (or near
  it — bloom uses bilinear sampling so result depends only on
  ordering, which is preserved).
