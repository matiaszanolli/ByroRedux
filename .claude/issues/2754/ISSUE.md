# REN-D13-01: taa.comp masks bit 31 off before comparing mesh IDs, cross-comparing two different ID namespaces

- **Severity**: MEDIUM
- **Dimension**: 13 — TAA. See Cluster A.
- **Location**: `crates/renderer/shaders/taa.comp` — the `disocclusion` predicate and the 5-tap dilation loop's `candidateSurface` test
- **Description**: (1) `bool disocclusion = currSurface != (prevMid & 0x7FFFFFFFu);` — an opaque pixel whose `entity_id + 1` numerically equals some previous-frame blended fragment's `draw_index + 1` is accepted as "same surface", pulling a transparent fragment's resolved colour into the opaque surface. The `alphaBlend` early-out inspects only the current fragment. (2) `uint candidateSurface = texelFetch(uCurrMeshId, p, 0).r & 0x7FFFFFFFu;` in the surface-constrained motion dilation admits a colliding blended neighbour as "the same stable surface", letting its motion vector win — the exact leak the dilation's own comment says the constraint exists to prevent.
- **Evidence**: The shader's `#904` comment still justifies the mask with "a same-surface opacity transition (alpha-blended ↔ opaque)", true only when both encodings were the instance index; `git show 883f57cd -- crates/renderer/shaders/taa.comp` is comment-only. Because the blend pipeline also overwrites the normal attachment, `surfaceMismatch`'s 0.85 cone reads the billboard's normal, not the occluded wall's, so the two guards fail together.
- **Impact**: Translucent double images / trailing ghosts on opaque geometry a particle/effect card crossed on the previous frame; occasional wrong motion vectors at particle silhouettes. Visual only. Candidate contributing mechanism for the unresolved "diagonal double-image ghost in Skyrim interiors" note (mechanism, not diagnosis). Reduced exposure since `UpscalerMode::default()` is `Fsr3(Quality)`, so TAA is opt-in.
- **Related**: `883f57cd`, #904, #992, #2466, REN-D8-NEW-01 (identical predicate shape in svgf_temporal.comp/svgf_atrous.comp — fix both together).
- **Suggested Fix**: Treat `(prevID & 0x80000000u) != 0u` as an outright non-match rather than masking the bit away.

## Completeness Checks
- [ ] SIBLING: svgf_temporal.comp / svgf_atrous.comp same predicate shape (REN-D8-NEW-01)
- [ ] TESTS: A regression test pins this specific fix; must not regress `taa_comp_keeps_history_bounded_and_rejects_unstable_surfaces` or #904's motivating case

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2754
