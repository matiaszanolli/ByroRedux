# #6202 — INC-2026-10-20-01: `byroTerrainHeightBlend` is discontinuous where a layer's painted share reaches 0 — hard, triangle-edge-aligned seams of up to 50 % layer coverage at every painted-layer rim

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6202
- **Labels**: high,terrain-exterior,shaders,renderer,bug
- **Filed**: 2026-10-20 by /audit-publish

From `docs/audits/AUDIT_INCREMENTAL_2026-10-20.md` (`/audit-suite --preset quick`, HEAD c4df4c91e).

- **Severity**: HIGH (risk floor of the `bindings.glsl` / `triangle.frag` rows; on impact alone this is a visual artifact with a workaround, `terrain.blend 0`)
- **Dimension**: exterior Dim 2 (height blend) / renderer
- **Location**: `crates/renderer/shaders/include/bindings.glsl:595` (`hasHeight[i] = idx != 0u && share[i] > 0.0`), `:603` (`count < 2u` early return), `:609-619` (scoring)
- **Changed in**: `crates/renderer/shaders/include/bindings.glsl` (commit ea45af067)
- **Status**: NEW (implementation defect inside open epic #6196)
- **Description**: A height-mapped lane scores `h_i + share_i / S`. As its painted share tends to 0 its score tends to `h_i`, not to "nothing", so a lane with a high height texel keeps a large blended share for arbitrarily small `share_i`. At exactly `share_i == 0` the lane is excluded (`hasHeight` false), and with fewer than two lanes in play the function returns the unblended weights. The blended share therefore jumps from up to 50 % to 0 across the zero-weight boundary. Splat weights are per-vertex UNORM8 interpolated, so that boundary is the triangle edge between a triangle with one non-zero vertex and an all-zero triangle: the seam is polygonal and follows the LAND grid. The same jump occurs on the base lane where a painted layer reaches full opacity (base share → 0).
- **Evidence**: CPU mirror of the function (audit scratch script, not retained), one painted lane over the base: band 0.2, `h_layer=1.0, h_base=0.0` → share' **0.500 at w=1e-4, 0.000 at w=0**; `h_layer=0.95, h_base=0.1` → 0.201 → 0.000; band 1.0, `h_layer=0.7, h_base=0.3` → 0.286 → 0.000. In Mishkinis' two-layer original the weights are complementary and no lane is ever excluded, so the formula never had this edge; the N-lane generalisation added the `share > 0` gate without making the score vanish with the share.
- **Impact**: Every terrain cell with at least two height-mapped lanes (the BTXT base always has one once generated) shows speckled bleed of high-height texels that cuts off along triangle edges at each painted layer's outer rim and at fully opaque cores. The seam gets wider as the band grows (at band 1.0 it appears wherever `h_layer > h_base`). It also feeds the specular/normal loops, the blade ground colour, and the Tier-3/receiver density (see -02). It violates #6196's requirement that the blend respect the painted weights.
- **Related**: #6196 (open; acceptance "Review by Matías" pending), #6197 (POM driven by the blended height inherits the seam).
- **Suggested Fix**: Make each lane's blended share go to 0 continuously with its painted share. For example, multiply each lane's `max(score − floor, 0)` by its `share_i` (or by `smoothstep(0, ε, share_i)`) before normalising, or score with `h_i · share_i`-weighted terms. Then remove the hard `share > 0` and `count < 2` cut-offs, or make them no-ops at the limit. Add a CPU-mirror test asserting continuity as `share_i → 0`.

## Completeness Checks
- [ ] **SIBLING**: `groundcover_blade.frag` also calls `byroTerrainHeightBlend` (blade ground colour) — the fix must cover both call sites; #6197 POM consumes the blended height
- [ ] **TESTS**: A CPU-mirror regression test asserts each lane's blended share → 0 continuously as its painted share → 0
