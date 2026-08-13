# REN-D11-2026-08-12-02: four production early-returns in triangle.frag skip the FSR reactive/transparency mask writes

- **Severity**: MEDIUM
- **Dimension**: 11 — Pipeline/RenderPass (G-buffer attachment 6/7 write completeness)
- **Location**: `crates/renderer/shaders/triangle.frag` — the mask initialisation at the top of `main()`, the four early `return`s (the `MATERIAL_KIND_EFFECT_SHADER` additive arm, the `MATERIAL_KIND_NO_LIGHTING` arm, the IOR/RT glass arm, and the `DBG_VIZ_GLASS_PASSTHRU` arm), and the tail policy
- **Description**: `main()` opens by zeroing both FSR masks, with a comment scoping the default to "the debug visualizations, the sky/background arms". Four arms that return early are neither — they are production transparent-surface paths. The RT/IOR glass arm is the strongest case: the tail policy explicitly says `isGlass → outFsrTransparency = 1.0`, yet glass that takes the IOR branch returns before that line and reports 0.0, while the same glass falling back to the Fresnel path reaches the tail and reports 1.0. The `FIRE_REFRACTION` arm sets both masks to 1.0 before its `return`, showing the intended discipline the other three omit.
- **Evidence**: The `EFFECT_SHADER`, `NO_LIGHTING` and IOR-glass arms each end with `return;` and no mask write, against `FIRE_REFRACTION`'s `outFsrReactive = 1.0; outFsrTransparency = 1.0; return;` and the tail `outFsrTransparency = isGlass ? 1.0 : (isAlphaBlend ? fsrCoverage : 0.0);`.
- **Impact**: FSR 3.1 gets no reactive / transparency hint for refractive glass on the RT path, additive `BSEffectShaderProperty` FX cards, and `BSShaderNoLightingProperty` surfaces. Because both masks MAX-blend, writing 0.0 never corrupts another draw, so history is kept where it should be rejected (smearing/ghosting behind flames, glow cards, terminal screens, RT glass); the same glass object can flip its transparency mask between 1.0 and 0.0 frame-to-frame as the adaptive ray budget crosses `RT_LOD_IOR`. On the engine's default render path.
- **Related**: REN-D11-2026-08-12-01, #2518, Dimension 23.
- **Suggested Fix**: Hoist the tail policy into a small helper called immediately before each production early `return`, and narrow the top-of-`main` default's comment to what it actually covers.

## Completeness Checks
- [ ] SIBLING: Same pattern checked in all early-return arms of triangle.frag
- [ ] TESTS: A regression test pins this specific fix

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2749
