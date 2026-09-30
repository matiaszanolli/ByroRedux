# #4861: REN-D10-2026-09-24-04: no guard pins the interior isolation of the outdoor sky, though every interior now bakes and uploads an outdoor cube and SH

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D10-2026-09-24-04**._

- **Severity**: LOW (test gap; behaviour is correct today).
- **Dimension**: Sky/Weather (consumption)
- **Location**: `shaders/include/bindings.glsl` `exteriorSkyDiffuseOr` (`jitter.w <= 0.5` gate); `include/raytrace.glsl` `traceReflection` (`_isExt ? … : sceneFlags.yzw`); `include/lighting.glsl` `pathEnvironmentRadiance` (`jitter.w > 0.5`); `water.frag` (~line 994).
- **Status**: NEW
- **Description**: Since `0572bfd5a` every interior has `portal_outdoor_sky`, so `sky_cube` bakes the real outdoor palette, the sky SH is projected from it, and `exteriorSkyTint.w` (cube ready) is 1 in interiors. The only barrier between "interior surfaces and rays see cell ambient" and "they see outdoor SH/cube" is the per-call-site `jitter.w` (`SkyParams::is_exterior`) predicate. The rule holds today (every consumer was read), but nothing asserts an `is_exterior` gate at the four gated consumers.
- **Impact**: A future edit dropping one predicate would silently light interior surfaces from outdoor SH or reflect outdoor sky in sealed rooms, a whole-scene brightness regression invisible to `cargo test`.
- **Suggested Fix**: A source-shape pin: `exteriorSkyDiffuseOr` returns `fallback` when `jitter.w <= 0.5`, and each `exteriorSkyRadianceOr(` call outside the portal escape is dominated by a `jitter.w` / `_isExt` / `isExteriorGlass` check.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)

