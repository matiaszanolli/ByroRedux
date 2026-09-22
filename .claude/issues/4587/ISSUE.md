# REN-D7-2026-09-21-01: `docs/engine/renderer.md` still describes retired renderer states — composite ACES, volumetrics ×0.0, pre-#3572 TAA/bloom order, `rebind_hdr_views`/`fall_back_to_raw_hdr`, "exposure + ACES" presentation

**Labels**: low, renderer, documentation, doc-rot

Filed via /audit-publish from docs/audits/AUDIT_RENDERER_2026-09-21.md.

**Severity**: LOW (reference-doc drift) · **Dimension**: Denoiser/Composite
**Location**: `docs/engine/renderer.md`:
- the pipeline bullet "Composite pass … with ACES tone mapping" (~:42-46);
- the file-tree entry "swapchain_extent, rebind_hdr_views" (~:216);
- frame-flow steps 17-23 (~:323-343);
- TAA section, step 3 (~:508-510).

**Status**: NEW. These sites were missed by the sweeps for #3573, #3608 and #4524.
**Verified against**: HEAD `f97775ca8`

## Description

`renderer.md` still describes several renderer states that no longer exist:
1. **Composite tone-maps.** The pipeline bullet says composite reassembles "… + sky, with ACES tone mapping". Since the FSR split, composite emits linear HDR and tone mapping lives in presentation (#4202).
2. **Volumetrics gated off.** Frame-flow step 17 says "the output is multiplied by 0.0 in composite until Phase 2 lands". `VOLUMETRIC_OUTPUT_CONSUMED` is `true` (`vulkan/volumetrics.rs`). #3573 fixed the § Volumetric section and the pipeline bullet, but not this step.
3. **Pre-#3572 order.** Frame-flow steps 18-21 run TAA (18) before bloom (20) and composite (21), and composite computes "`direct + indirect * albedo + caustic + bloom`". The live order is composite → bloom (`bloom_apply.comp` adds in place, #2796) → exposure meter → TAA → upscale (#3572). #3608 fixed the § Bloom section and the pipeline bullet, but not the frame flow.
4. **Retired rebind/fallback.** TAA § step 3 says `CompositePipeline::rebind_hdr_views()` swaps composite's input to the TAA output, and that composite falls back via `fall_back_to_raw_hdr`. The file tree also lists `rebind_hdr_views`. Both functions are gone. #4524 swept six code-comment sites, not this doc.
5. **Presentation.** Step 23 reads "exposure + ACES tone map". Presentation now applies `tonemap(graded * exposureTex)` with an ACES|AgX switch and a meter-produced exposure texel (Stage 1).

## Evidence

- `grep -rn "fn rebind_hdr_views\|fn fall_back_to_raw_hdr" crates/renderer/src` finds no match.
- `crates/renderer/src/vulkan/volumetrics.rs` has `pub const VOLUMETRIC_OUTPUT_CONSUMED: bool = true;`.
- `taa_resolves_the_post_bloom_scene_tap` (`crates/renderer/src/vulkan/context/post_passes.rs`) pins the live frame-tail order: composite → bloom → exposure meter → TAA → upscale.

## Impact

The renderer overview doc presents the wrong frame graph: tone-map location, TAA position, volumetrics status, and a fallback mechanism that no longer exists. Contributors and auditors who rely on it reason about the wrong pipeline. No runtime effect.

## Related

- #3573, #3608 and #4524 (closed): earlier sweeps of these retired states. Their named sites are fixed; these `renderer.md` sites were outside their scope.
- #4202 (closed): moved ACES out of composite.
- REN-D4-2026-09-21-01 (#4586): the same frame-flow drift in `shader-pipeline.md` / `memory-budget.md`.

## Suggested Fix

In `renderer.md`:
- Rewrite frame-flow steps 17-23 to the live order: composite → bloom → exposure meter → TAA → upscale → presentation.
- Fix the composite bullet: linear HDR, no tone map.
- Drop the ×0.0 volumetrics note.
- Replace TAA step 3 and the file-tree mention with the current resolve wiring.

A `renderer.md` ↔ `record_post_passes` order pin, of the kind #3573 proposed for the volumetrics claim, would prevent another round.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D7-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: `shader-pipeline.md` (REN-D4-2026-09-21-01 (#4586)) and the audit-renderer skill's frame-order line updated in the same sweep
- [ ] **TESTS**: optional doc-order pin against `record_post_passes`
