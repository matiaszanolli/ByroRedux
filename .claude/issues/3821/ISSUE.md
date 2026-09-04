# Issue #3821: REN-WD-D8-01: water preserves receiver's un-attenuated GI, widened by b15b0527's coverage change

**Labels**: medium,renderer,water,bug
**Filed**: 2026-09-04, via /audit-publish from the water-deep audit suite

---

**Severity**: MEDIUM
**Dimension**: Denoiser/Composite (water interaction)
**Location**: `crates/renderer/src/vulkan/water.rs` (`build_pipeline`'s `attachments` array — `masked_off` on colour attachments 1–5), `crates/renderer/shaders/composite.frag` (the geometry arm's `combined = direct + indirect * albedo + caustic`), `crates/renderer/shaders/water.frag` (the `reflectedCoverage` alpha block)
**Source report**: `docs/audits/AUDIT_RENDERER_2026-09-04.md` (water-deep suite, Dim 8)

## Description
The water pipeline hard-masks raw-indirect (attachment 4) and albedo (attachment 5) to `color_write_mask = empty()`, so a water fragment leaves the **opaque receiver's** demodulated GI and albedo in the G-buffer untouched. Composite then adds `indirect * albedo` at full strength on top of a `direct` value that already has the water surface alpha-blended into it. For low-alpha water that's roughly "see the bed's GI through the water", but it's not attenuated by the water column at all — and for high-alpha water (waterfalls, lava, and now any grazing-angle fragment) the covered surface's GI is still added at 100%.

The alpha-blend pipeline has a purpose-built alternative for exactly this: `create_blend_pipeline`'s non-`preserve_opaque_gbuffer` arm uses `auxiliary_blend` (a coverage blend) on attachments 4 and 5, so an ordinary transparent attenuates the receiver's indirect by its own coverage. Water has no such variant — it's permanently on the `preserve` shape introduced for refractive glass. `b15b0527` (`reflectedCoverage = 1.0 - (1.0 - baseAlpha) * (1.0 - fresnel)`) raises water's output alpha toward 1.0 at grazing angles, which increases how much of `direct` the water owns while `indirect * albedo` stays at 100% — so the mismatch got larger with that commit without the composite side being revisited.

## Evidence
```
water.rs:884-890   let attachments = [ hdr_blend, masked_off, masked_off,
                     masked_off, masked_off, masked_off, fsr_mask_max, fsr_mask_max ];
pipeline.rs:621-622  auxiliary_blend, // 4 raw_indirect (coverage blend)
                      auxiliary_blend, // 5 albedo (coverage blend)
```
`water.rs`'s comment on the masked attachments is about *denoiser stability* ("SVGF and motion-vector reprojection see only the opaque pass behind the water"), which does not address composite's reassembly.

## Impact
Visual only, exterior/water scenes. Lake and river beds read brighter than they should through water, and opaque water (waterfall, lava) is contaminated by the GI of whatever it covers. Grazing-angle water regions are the worst case post-`b15b0527`. No crash/corruption risk. Needs a visual A/B (waterline capture or RenderDoc) to confirm magnitude, not a `cargo test`.

## Related
#2745 (why refractive glass preserves attachment 3 but not 4/5), `#883f57cd` (the aux-MRT alpha lanes), `b15b0527` (the coverage change that widened the gap).

## Suggested Fix
Give the water pipeline the same `auxiliary_blend` treatment on attachments 4 and 5 that the ordinary blend pipeline uses, so the receiver's demodulated GI is attenuated by the water's own coverage. Do this after REN-WD-D15-01 (the refraction-coverage fix) lands, since the two interact — raising water's effective coverage further changes this finding's magnitude. Verify against `docs/smoke-tests/m-exteriors.sh`'s above/below-waterline captures.

## Completeness Checks
- [ ] **DROP**: If the water pipeline's attachment/blend-state array changes, confirm no other consumer assumed the masked-off shape
- [ ] **TESTS**: A regression test or capture-based check pins the new coverage attenuation
