# #779 Investigation — early_fragment_tests on triangle.frag is UNSAFE as proposed

## Premise of the proposed fix

Add `layout(early_fragment_tests) in;` to `triangle.frag` to skip RT ray queries on overdrawn fragments. Estimated 0.5-1.5 ms/frame GPU on overdraw-heavy scenes.

## Why the simple one-line fix is unsafe

`triangle.frag` has two `discard` paths after texture sampling:

| Line | Trigger | Affected content |
|---|---|---|
| 683 | Alpha-test (`mat.alphaThreshold > 0`, function-gated) | Foliage, fences, wire grates, hair cards, decals |
| 694 | Alpha-blend with `aThresh == 0` and fully-transparent texel | FNV picture/table NIFs (per the comment) |

Per the GLSL spec on `early_fragment_tests`:

> "If using `early_fragment_tests`, depth and stencil tests will be performed before the fragment shader runs, and the writing of depth values is also performed before the fragment shader runs. **Discarding a fragment within the fragment shader does not undo any depth write that has already been performed.**"

Concrete artifact: an alpha-tested leaf billboard with `early_fragment_tests` would commit a **rectangular** depth footprint (polygon extent) before the shader runs. Any geometry behind the transparent areas of the leaf texture is then depth-rejected — leaving visible "ghost rectangles" where the leaves should have transparent windows. Same problem on every alpha-tested mesh.

## Why the audit missed this

The 2026-04-20 audit said "Both `discard` paths derive from texture-sampled alpha + `mat.alpha_threshold`. Neither depends on RT ray query results, so early-Z is spec-legal." That's true — the spec *permits* the declaration — but the visual correctness consequence (ghost depth footprints on alpha-tested geometry) wasn't analyzed. The 2026-05-01 perf re-flag (PERF-N6) inherited the same blind spot.

## Safer alternatives

### Option A: Depth pre-pass (cleanest, ~80% of the perf win)

Render all opaque + alpha-test draws to a depth-only pass first (using a stripped-down shader: vertex transform + alpha test + depth write, no G-buffer, no ray queries). Then run the main pass with `early_fragment_tests` enabled — depth is already correct, so the early-Z reject on overdrawn fragments is artifact-free.

Cost: ~50-100 µs/frame extra GPU for the depth pre-pass; roughly 0.4-1.2 ms saved on the main pass. Net win: 0.3-1.1 ms/frame on overdraw-heavy scenes.

Implementation effort: ~150-300 lines (new pipeline + render pass, modified draw_frame ordering, new minimal vertex/fragment shader pair).

### Option B: Variant shader compilation

Compile two SPV variants of `triangle.frag` — one with `early_fragment_tests`, one without. Use the early variant on opaque-no-alpha-test pipelines only; keep the original on alpha-test pipelines and alpha-blend pipelines.

Cost: same ~0.5-1.5 ms recovery on the opaque-no-alpha-test fraction of draws (which is most of the geometry on typical interiors).

Implementation effort: ~50-100 lines (build script generates two SPVs from the same .frag with a `#define`, pipeline creation picks the right one based on alpha-test state).

### Option C: Specialization constants

GLSL doesn't directly support `layout(early_fragment_tests)` as a specialization constant, so this isn't actually viable. Mention it only to rule it out.

### Option D: Skip the fix

Accept the current overhead. Document that the perf opportunity is bounded by the alpha-test + early-Z incompatibility.

## Recommendation

Do not ship the one-line fix. Pick between:
- **A** (depth pre-pass) if the team values getting all of the perf win and is willing to add a render pass
- **B** (variant shader) if the team prefers minimal new infrastructure
- **D** (skip) if the perf is not currently a bottleneck and the engineering cost isn't justified

This is an architectural decision that warrants user input; not appropriate to autonomously decide under auto mode.
