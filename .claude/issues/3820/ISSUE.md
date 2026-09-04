# Issue #3820: REN-WD-D2-01: water caustic imageAtomicAdd bounds-checks against screen.xy, not the bound image

**Labels**: medium,renderer,water,bug
**Filed**: 2026-09-04, via /audit-publish from the water-deep audit suite

---

**Severity**: MEDIUM
**Dimension**: SSBO/Indexing (water-side caustic write)
**Location**: `crates/renderer/shaders/water.frag` (the `causticSize` / `imageAtomicAdd(waterCausticAccum, q, fixedVal)` block at the end of `main()`); fallback binding in `crates/renderer/src/vulkan/context/resize.rs` (`placeholder_caustic_sink`)
**Source report**: `docs/audits/AUDIT_RENDERER_2026-09-04.md` (water-deep suite, Dim 2)

## Description
`#2784` replaced the float `uv01 <= 1.0` guard with an integer pixel bound and its comment states the change means the splat "no longer depends on" Vulkan's out-of-range-write discard rule, explicitly naming the 1×1 `placeholder_caustic_sink` fallback as the case that used to rely on it. But the bound it introduced is `ivec2 causticSize = ivec2(screen.xy)` — `GpuCamera.screen` is the **render extent**, not the size of the image currently bound at set 2 binding 0. When `WaterCausticAccum::new` or `recreate_on_resize` fails, `resize.rs` deliberately binds the 1×1 `placeholder_caustic_sink` view instead, and every water fragment then passes the render-extent bound and issues `imageAtomicAdd` at coordinates far outside a 1×1 image. Unlike a plain image store (which Vulkan defines as discarded when out of range), an image *atomic* out of range is not covered by that guarantee.

## Evidence
```
water.frag:1248   ivec2 causticSize = ivec2(screen.xy);
water.frag:1250   && all(lessThan(pixel, causticSize)))
water.frag:1290   imageAtomicAdd(waterCausticAccum, q, fixedVal);
```
`resize.rs` binds `placeholder_caustic_sink` (`vec![p.view; MAX_FRAMES_IN_FLIGHT]`) on both accumulator-failure arms. `caustic_splat.comp` has the same shape (`ivec2 size = ivec2(causticScreen.xy)`), so the glass writer inherits the same assumption for its own sink path.

## Impact
Only reachable on the degraded path (accumulator allocation or layout transition failed at init/resize — i.e. under the VRAM pressure the fallback exists to survive). Blast radius is one atomic per water fragment per frame at undefined coordinates. Also a correctness-of-documentation problem: the `#2784` comment asserts an independence the code does not have.

## Related
#2784 (the integer-bound change), #2142 (the sink fallback), the water caustic phases (#1210/#1255).

## Suggested Fix
Bound on `imageSize(waterCausticAccum)` instead of `screen.xy` in `water.frag` (and mirror it in `caustic_splat.comp` with `imageSize(causticAccum).xy`); keep `screen.xy` only for the world→screen projection. This is a two-line shader change plus a `.spv` recompile, pinnable by the existing `water.rs` source-assertion test style.

## Completeness Checks
- [ ] **SIBLING**: `caustic_splat.comp` has the identical `screen.xy` bound pattern — fix both together
- [ ] **TESTS**: A regression test pins the fix (source-assertion style, matching `water.rs`'s existing tests)
