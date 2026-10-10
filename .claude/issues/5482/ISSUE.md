# #5482: Skyrim exteriors: distant LOD water renders at exactly-zero radiance, and the black-dominated frame pins auto-exposure at the ceiling — exteriors read as black-void + blown-sky ("posterized" contrast)

User-reported 2026-10-10: "In Skyrim exteriors, contrast is way too high vs interiors, borderline posterized." Two user frames + full live diagnosis session (NVIDIA lane, `--esm Skyrim.esm --grid 0,0 --radius 2`, bench-hold + byro-dbg instrument set; evidence in `/tmp/contrast_ev/` + engine log).

## Quantified symptom

User frames and reproduced locally: **~52-67% of pixels sit at exactly 0**, midtones 6-11%, top ~7% blown >232. NOT posterization in the bit-depth sense (all 255 levels used, no gaps) — the frame is tonally bimodal: black void + flat blown sky + a thin lit band.

## Proven causal chain

1. **The black field is the distant LOD water surface, at exactly-zero radiance.**
   - `render.debug water_term` / `water_refl` oracles light up exactly over the final-black pixels (mean 79-80 over black vs 55 over lit) — the fragments ARE water.
   - The per-pixel ray probe on a black pixel: `fragment=true`, geometry present, sun visibility 1.0 (unshadowed), material role = normal-mapped.
   - `exposure fixed 8.0`: **98% of the black pixels stay < 1/255** — true zero radiance, not dim.
   - Engine log: `LOD water mesh spawned: 472 quads` across a 61-cell reach, kind=River, heights −13670…+12608. XCLW census on the real Skyrim.esm confirms the water authoring is legit (Tamriel 497/11186 cells with water; grid (0,0) itself −6070; rivers −5050…−8050; 85 high lake cells at 10624).
2. **Missing distance fog on the far water.** water.frag's `fog_near`/`fog_far` are the water column's own refraction ramp, not atmospheric fog; the composite-pass aerial/froxel fog that fades terrain into the sky does not appear to reach the distant water surface, so where vanilla shows a hazy bright river valley we show the raw surface.
3. **The exact-zero needs one more step isolated** (next session): refraction-miss lands on `deepTint` by construction (`mix(deepTint, …, transmission)`, #548's normalized ramp), so a literal 0 implies either the LOD water's push deep/shallow colours are zero on this path, or the composite/fresnel weighting zeroes out. Needs a water.frag instrument (dump `push.deep` / `reflColor` / `fresnel` for a distant quad) or a RenderDoc capture.
4. **Secondary: the auto meter pins at the 2.0 ceiling.** With half the frame at 0, the plain average luminance meters the scene dark → `exposure fixed 1.0` drops the sky from 238→217 p95 and kills the blowout (blown 7%→0%) — the blown sky is the meter's reaction to the black void, not an independent defect.
5. **Aggravating install factor:** this machine's SE install ships **no `Skyrim - Terrain.bsa`**, so no `.btr` distant terrain fills the far field behind the water — the void is fully exposed. (The lit midground mountains shade correctly; `render.debug indirect_only` shows healthy ambient — the near/mid lighting is NOT the problem.)

## Suggested fix surface

- Water: extend the composite distance-fog / beyond-grid aerial tail to the water surface (or apply it in water.frag) so far water fades like far terrain; then isolate the exact-zero (push colours vs fresnel/refl path) with a small water.frag debug output.
- Meter: consider a percentile (e.g. ~50th-weighted) or center-weighted average instead of the plain mean, so a half-black frame doesn't pin exposure at the ceiling — the fixed-1.0 A/B shows how much of the "contrast" is this artifact.
- Interiors are unaffected (no LOD water) — exactly matching the "exteriors vs interiors" framing of the report.

## Repro

```
cd <Skyrim SE Data>
BYRO_DEBUG_SERVER=1 BYRO_DEBUG_PORT=9978 <engine> --esm Skyrim.esm --grid 0,0 --radius 2 \
  --bsa "Skyrim - Meshes0.bsa" --bsa "Skyrim - Meshes1.bsa" \
  --textures-bsa "Skyrim - Textures0.bsa" --textures-bsa "Skyrim - Textures1.bsa" --bench-hold
# byro-dbg (stdin pipe): screenshot a; render.debug water_term; screenshot b; exposure fixed 1.0; screenshot c
```

## Completeness Checks
- [ ] SIBLING: other games' distant water (FNV Lake Mead, FO3 Potomac) re-checked after the fog fix
- [ ] TESTS: a fog-on-water regression pin once the mechanism lands
