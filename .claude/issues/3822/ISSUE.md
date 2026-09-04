# Issue #3822: REN-WD-D15-01: water refraction still double-applies Fresnel/coverage; only the reflection half was fixed by b15b0527

**Labels**: medium,renderer,water,bug
**Filed**: 2026-09-04, via /audit-publish from the water-deep audit suite

---

**Severity**: MEDIUM
**Dimension**: Water
**Location**: `crates/renderer/shaders/water.frag` (the `── Alpha ──` block: `reflectedCoverage` / `float alpha = ...`), consumed by `crates/renderer/src/vulkan/water.rs`'s `hdr_blend` (`SRC_ALPHA` / `ONE_MINUS_SRC_ALPHA`)
**Source report**: `docs/audits/AUDIT_RENDERER_2026-09-04.md` (water-deep suite, Dim 15)

## Description
`b15b0527` correctly identified that Fresnel was being applied twice — once inside `surfaceColor = mix(refrColor, reflColor * tint, fresnel)` and again through a low output alpha — and folded the reflected share into the coverage (`reflectedCoverage = 1.0 - (1.0 - baseAlpha) * (1.0 - fresnel)`). The same double-application still exists on the **refraction** half and was not addressed. `refrColor` is a fully RT-resolved trace of whatever is under the water, already attenuated by `absorbWaterColumn`'s Beer-Lambert term. But the surface is then alpha-blended over a framebuffer that already contains the *un-attenuated* raster of that same geometry. With a legacy ANAM near 0.2 and a face-on view (`fresnel ≈ 0.02`), `alpha ≈ 0.22` — so ~78% of the visible pixel is the raw lake bed and only ~22% carries the authored fog/absorption ramp. The authored `fog_near`/`fog_far`, Starfield extinction and pigment-concentration work in `absorbWaterColumn` are proportionally discarded.

## Evidence
```
water.frag:990    refrColor = absorbWaterColumn(hitColor, ...)
water.frag:1109   vec3 surfaceColor = mix(refrColor, reflColor * push.tint_reflect.w, fresnel);
water.frag:1150   outColor = vec4(surfaceColor, alpha);
```
`water.rs::build_pipeline`'s `hdr_blend` uses `SRC_ALPHA`/`ONE_MINUS_SRC_ALPHA`, so the destination is the already-rasterized opaque bed. The in-code comment at `water.frag:1117-1125` explains the reflection-half fix in detail but only discusses the Fresnel/reflected-energy term — nothing in the shader consumes `refrHit` to decide whether the framebuffer path is still needed for the refraction half.

## Impact
Visual only. Vanilla water reads clearer/less tinted than authored on every game whose WATR ANAM is low; the deep-colour and extinction tuning done in #3224 / #3270 / the Starfield absorption work is under-expressed. Bodies whose ANAM is near 1.0 (the 0.88 default and waterfalls) are unaffected.

## Related
`b15b0527` (the reflection-half fix this completes), #2785 (`fog_near` ramp), #3224, #3270.

## Suggested Fix
When the RT refraction resolved (`refrHit` true and `sceneFlags.x >= 0.5`), treat the surface as fully covering (`alpha → max(alpha, 1.0)` for the transmission share) and keep the authored ANAM path only as the no-RT / miss fallback — i.e. let the authored opacity select *how much of the framebuffer* substitutes for a refraction the engine could not trace, rather than competing with one it did. This is a look change; verify against the new above/below-waterline captures in `docs/smoke-tests/m-exteriors.sh`, not a unit test.

## Completeness Checks
- [ ] **SIBLING**: Check the equivalent no-RT / miss fallback path still uses the authored ANAM as-is
- [ ] **TESTS**: Needs a visual A/B (capture), not `cargo test` — record the capture reference alongside the fix
