# Renderer Audit — 2026-05-26 (Dimension 19 only: Bloom Pyramid / M58)

**Scope**: single-dimension run via `/audit-renderer 19` — **first focused sweep** of the M58 Bloom Pyramid. No prior Dim 19 audit exists.

## Executive Summary

M58 Bloom is in good shape. All 10 checklist items either PASS outright or fail only against the letter of the spec while implementing the documented intent correctly. The post-#931 barrier budget (10 per dispatch) is verified by direct counting: 1 HOST→COMPUTE UBO + 5 down-pass post-barriers + 4 up-pass post-barriers. Pyramid layout (5 down + 4 up mips, `B10G11R11_UFLOAT_PACK32`), descriptor lockstep with composite resize (#905), and pre-TAA input wiring (#1107 / #1166) are all present and intentional.

**Two LOW findings**, both explanatory rather than actionable bugs: a doc gap on upsample DC-gain absorption, and a dead `Option<BloomPipeline>` guard whose `None` branch is unreachable since #1081 made init failure fatal.

| Severity | NEW | INFO |
|----------|-----|------|
| Critical | 0   | 0    |
| High     | 0   | 0    |
| Medium   | 0   | 0    |
| Low      | 2   | 0    |
| **Total**| **2** | **0**|

## RT Pipeline Assessment

Not in scope for Dim 19. Bloom is a post-process compute step that runs after the geometry/RT passes finish; no interaction with BLAS/TLAS lifecycle, no ray-query dependencies.

## Rasterization Assessment (Bloom-specific)

- **5 down + 4 up mips** at [bloom.rs:72,117](crates/renderer/src/vulkan/bloom.rs#L72), each level halves X+Y at [bloom.rs:683-700](crates/renderer/src/vulkan/bloom.rs#L683-L700). `BLOOM_FORMAT = B10G11R11_UFLOAT_PACK32` at [:78](crates/renderer/src/vulkan/bloom.rs#L78). No R16G16B16A16 mid-chain.
- **Down-pass weights sum to 1.0**: 4 taps at (±0.5, ±0.5) × `src_pixel` offsets, multiplied by 0.25 — [bloom_downsample.comp:51-56](crates/renderer/shaders/bloom_downsample.comp#L51-L56).
- **Up-pass additive blend, no clamp**: [bloom_upsample.comp:53-65](crates/renderer/shaders/bloom_upsample.comp#L53-L65). HDR magnitudes preserved by the `B10G11R11_UFLOAT` format (~65k range).
- **Per-frame-in-flight ownership**: each `BloomFrame` owns its own `down_mips`, `up_mips`, descriptor sets, and param buffers ([bloom.rs:115-127](crates/renderer/src/vulkan/bloom.rs#L115-L127)). `frames` has `MAX_FRAMES_IN_FLIGHT` entries. No cross-frame aliasing.
- **Barrier count = 10 per dispatch** (post-#931 target met): 1 HOST→COMPUTE UBO at [bloom.rs:482-488](crates/renderer/src/vulkan/bloom.rs#L482-L488), 5 down-chain post-barriers at [:539-547](crates/renderer/src/vulkan/bloom.rs#L539-L547), 4 up-chain post-barriers at [:587-595](crates/renderer/src/vulkan/bloom.rs#L587-L595).
- **`BLOOM_INTENSITY = 0.15`** flows from [shader_constants_data.rs:77](crates/renderer/src/shader_constants_data.rs#L77) → generated `shader_constants.glsl:44` → consumed at [composite.frag:453](crates/renderer/shaders/composite.frag#L453). Build-time drift caught by `shader_constants.rs::composite_frag_bloom_intensity_not_redeclared`.
- **Resize rebind**: bloom pipeline destroyed + recreated at [resize.rs:478-510](crates/renderer/src/vulkan/context/resize.rs#L478-L510); new `output_views()` passed to `CompositePipeline::recreate_on_resize` at [:560](crates/renderer/src/vulkan/context/resize.rs#L560); composite binding 7 rewritten at [composite.rs:1041-1058](crates/renderer/src/vulkan/composite.rs#L1041-L1058).
- **Tone-map order**: bloom added into `combined` at [composite.frag:453](crates/renderer/shaders/composite.frag#L453), ACES applied to `combined * exposure` at [:471](crates/renderer/shaders/composite.frag#L471). `(scene + bloom) → ACES → display` ordering is correct.
- **Source = un-tone-mapped HDR (not TAA output)**: [draw.rs:2842](crates/renderer/src/vulkan/context/draw.rs#L2842) reads `composite.hdr_image_views[frame]`, not the TAA resolve target. Per #1166 / #1107.

## Checklist Status

| # | Item | Status |
|---|------|--------|
| 1 | Pyramid size: 5 down + 4 up, `B10G11R11_UFLOAT` everywhere | **PASS** |
| 2 | Down-pass 4-tap bilinear box, weights sum to 1.0 | **PASS** |
| 3 | Up-pass 4-tap bilinear additive, no [0,1] clamp | **PASS** (see L1 for doc gap) |
| 4 | Per-FIF slot owns its own pyramid | **PASS** |
| 5 | 10 barriers per dispatch (post-#931) | **PASS** |
| 6 | Intensity = 0.15 from generated shader header | **PASS** |
| 7 | Image-view rebind on composite resize (#905) | **PASS** |
| 8 | Disabled-path short-circuit | **FUNCTIONALLY UNREACHABLE** (see L2) |
| 9 | Tone-map order: bloom added BEFORE ACES | **PASS** |
| 10 | Source = un-tone-mapped HDR (not TAA output) | **PASS** |

## Findings

### Finding L1 — Up-pass shader filter weights sum to 2.0, not 1.0; intensity constant silently absorbs the gain

- **Severity**: LOW
- **Status**: NEW
- **Location**: [bloom_upsample.comp:53-65](crates/renderer/shaders/bloom_upsample.comp#L53-L65)
- **Related issue**: none

**Evidence**

```glsl
vec3 upsampled = vec3(0.0);
upsampled += texture(src_smaller, uv + vec2(-0.5, -0.5) * smaller_pixel).rgb;
upsampled += texture(src_smaller, uv + vec2( 0.5, -0.5) * smaller_pixel).rgb;
upsampled += texture(src_smaller, uv + vec2(-0.5,  0.5) * smaller_pixel).rgb;
upsampled += texture(src_smaller, uv + vec2( 0.5,  0.5) * smaller_pixel).rgb;
upsampled *= 0.25;
// ...
vec3 same = texture(src_same, uv).rgb;
imageStore(dst, dst_coord, vec4(upsampled + same, 1.0));
```

`upsampled` has unit DC-gain (4 taps × 0.25). `same` has unit DC-gain (1 tap × 1.0). Sum is 2.0× the average DC magnitude of each contributor. Across the 4-step up-chain a DC-constant scene accumulates as `down[N-1] → 2·down[N-1] + down[N-2] → ...`, up to ~8× the peak down-mip DC at `up[0]`.

**Impact**

None visually — this is the standard Bjørge / Frostbite progressive accumulation, and `BLOOM_INTENSITY = 0.15` is tuned to absorb the un-normalised gain (Frostbite SIGGRAPH 2015 quotes ~0.04 for a normalised pyramid → 0.15 ≈ 4× compensates the un-normalised sum + Bethesda LDR authoring per the [bloom.rs:80-92](crates/renderer/src/vulkan/bloom.rs#L80-L92) docstring).

The bug is the **asymmetric documentation**: the down-pass shader explicitly states "weights sum to 1.0 — provably equivalent to a 4×4 box filter," but the up-pass doesn't call out that its sum is intentionally ≥ 1.0 by design. A reader making the up-pass changes will assume the down-pass invariant applies.

**Fix sketch**

No code change. Add a comment to [bloom_upsample.comp](crates/renderer/shaders/bloom_upsample.comp) header (near the existing Bjørge SIGGRAPH 2015 reference) documenting that the additive `upsampled + same` sum carries DC gain ≥ 1.0 by design, and that `BLOOM_INTENSITY = 0.15` is the global scale that absorbs it. Mirror the doc-style of the existing "weights sum to 1.0" claim in the downsample shader.

### Finding L2 — Dead `is_some()` guard on bloom dispatch: init failure is now fatal (#1081), so the `None` branch never executes

- **Severity**: LOW
- **Status**: NEW
- **Location**: [context/draw.rs:2840](crates/renderer/src/vulkan/context/draw.rs#L2840) + [context/mod.rs:1953-1967](crates/renderer/src/vulkan/context/mod.rs#L1953-L1967)
- **Related issue**: #1081 (closed, hard-fail policy)

**Evidence**

- [context/mod.rs:1953-1957](crates/renderer/src/vulkan/context/mod.rs#L1953-L1957) — `BloomPipeline::new` failure logs a warn and sets `bloom = None`.
- [context/mod.rs:1958-1967](crates/renderer/src/vulkan/context/mod.rs#L1958-L1967) — then immediately:
  ```rust
  let bloom_views: Vec<vk::ImageView> = match bloom.as_ref() {
      Some(b) => b.output_views(),
      None => {
          return Err(anyhow::anyhow!(
              "Bloom pipeline failed to initialize — composite \
               requires the bloom output view for binding 7 (M58). \
               Check earlier 'bloom' WARN logs."
          ));
      }
  };
  ```
- [draw.rs:2840](crates/renderer/src/vulkan/context/draw.rs#L2840) — `if let Some(ref mut bloom) = self.bloom { ... }` is therefore always taken at runtime.

**Impact**

Misleading reader-of-code surface. The `if let Some(...)` conditional + the per-frame warn-and-skip on `bloom.dispatch` failure ([draw.rs:2850-2852](crates/renderer/src/vulkan/context/draw.rs#L2850-L2852)) imply graceful degradation that is not actually supported. Anyone following the "what if bloom is off?" thread can spend a while tracing it before realising the init path forecloses it.

**Fix sketch (pick one)**

- **(a) Match the contract — make `bloom` non-optional.** Change `pub bloom: Option<BloomPipeline>` ([context/mod.rs:1127](crates/renderer/src/vulkan/context/mod.rs#L1127)) to `pub bloom: BloomPipeline`. Drop the `Some(b)` arm at [context/mod.rs:1945-1952](crates/renderer/src/vulkan/context/mod.rs#L1945-L1952) — just `let bloom = BloomPipeline::new(...)?;`. Unwrap the `if let Some` at `draw.rs:2840` and `resize.rs:478`. Cleanest, but removes the resize-fallback option.
- **(b) Add a doc-only crumb.** Leave the `Option` (the resize-recreate path uses it as a temporary), and add a comment at `draw.rs:2840` cross-referencing #1081 + `context/mod.rs:1958` so readers know the `None` branch is unreachable at runtime. Less invasive.

## Cross-cutting notes

- **GpuTimers bracketing**: `cmd_bloom_start/end` is in place around the dispatch at [draw.rs:2843-2849](crates/renderer/src/vulkan/context/draw.rs#L2843-L2849). Good.
- **Sub-resource helper**: `bloom.rs` uses `super::descriptors::color_subresource_single_mip()` for all views and barriers ([bloom.rs:490,900](crates/renderer/src/vulkan/bloom.rs#L490)). #1149 / TD3-207 cleanup landed.
- **No drift between dispatch-time descriptor writes and pre-baked binding layouts.** Scene HDR view is written into `down_descriptor_sets[0]` binding 0 with `SHADER_READ_ONLY_OPTIMAL` ([bloom.rs:435](crates/renderer/src/vulkan/bloom.rs#L435)); pre-baked downsample bindings for i ≥ 1 use `GENERAL` ([bloom.rs:779](crates/renderer/src/vulkan/bloom.rs#L779)). Both match the image-side layouts at the point of sampling.
- **Workgroup divisibility**: ceiling-divide at [bloom.rs:525-526, 569-570](crates/renderer/src/vulkan/bloom.rs#L525-L526), matching the `greaterThanEqual(dst_coord, dst_size)` early-out in both shaders ([bloom_downsample.comp:41-44](crates/renderer/shaders/bloom_downsample.comp#L41-L44), [bloom_upsample.comp:43-46](crates/renderer/shaders/bloom_upsample.comp#L43-L46)). Safe at non-multiple-of-8 swapchain extents.
- **Minimum mip dimension**: floors at 1×1 via `.max(1)` ([bloom.rs:686,698](crates/renderer/src/vulkan/bloom.rs#L686)), so a 32-px-tall window won't underflow on mip 5 (32 / 2⁵ = 1). Below ~16 px the smallest mip degenerates to 1×1 which still dispatches correctly.

## Methodology

1. No prior Dim 19 audit exists — full walk of the checklist as a first sweep.
2. Read `bloom.rs` end-to-end for the pipeline shape (struct fields, mip count, format, dispatch sites, descriptor lifecycle).
3. Cross-referenced the dispatch site at `draw.rs:2840` against `mod.rs:1958` init policy to surface L2.
4. Counted barriers directly at the dispatch loop to verify the post-#931 budget.
5. Walked `composite.frag` for the bloom intensity consumer + ACES order (item 9).
6. Verified the source-pyramid input is raw HDR not TAA per #1166 / #1107.
7. Dedup baseline at `/tmp/audit/renderer/issues.json` — no existing issue covers L1 or L2.

---

Suggest: `/audit-publish docs/audits/AUDIT_RENDERER_2026-05-26_DIM19.md`
