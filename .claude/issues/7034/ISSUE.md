# NIFAL-2026-11-16-01: MaterialTextureHandles.normal_has_alpha is read before the texture upload flush

Labels: medium,renderer,nifal,game:skyrim,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/7034

**Severity**: MEDIUM
**Source**: found during the #7023 kind-1 reflection A/B capture (2026-11-16); not from an audit report.
**Location**: `build_material_texture_handles` (`byroredux/src/asset_provider/texture.rs`) reading `TextureRegistry::handle_has_alpha` (`crates/renderer/src/texture_registry/lookup.rs`); the queued-slot placeholder `has_alpha: false` and its fill in `flush_pending_uploads` (`crates/renderer/src/texture_registry/upload.rs`); consumers `normal_alpha_spec_binding_applies` (`byroredux/src/material_translate.rs`) and `spawn_normal_has_alpha` (`byroredux/src/render/static_meshes.rs`)
**Status**: NEW (validated live; no matching open or closed issue)

## Description
`MaterialTextureHandles::normal_has_alpha` is computed once at spawn from `handle_has_alpha(normal)`. The cell loader only *enqueues* DDS uploads (`enqueue_dds_*`), which reserve the slot with `has_alpha: false` ("`false`/`None` until then"); the real value is written by `flush_pending_uploads`, which runs at the end of the cell load, after the spawn read. Nothing refreshes the component afterwards. So the flag is **false for every texture first seen in a cell load, and true only when the texture was already flushed** (a cache hit from an earlier cell).

**Evidence** (`mat.dump <id>` on a running Skyrim SE engine, 2026-11-16):
- Cold-load `MarkarthWizardsQuarters01`: **0 of 2,743** materials had `normal_has_alpha=true`, although the normals are DXT5 (`dwemetalsheet01_n.dds`, `irondoor2_n.dds` extracted from `Skyrim - Textures*.bsa` and checked: FourCC `DXT5`).
- `door.teleport` into `MarkarthUnderStoneKeep` (textures now flushed): **2,804 kind-0 and 380 kind-1** materials read true; the same population reads false on a cold start.
- The `MaterialTextureHandles` component is never rewritten after the flush (no refresh in `cell_loader`, `scene` or `systems`).

## Impact
`normal_alpha_spec_binding_applies` requires the flag, so the whole normal-alpha-as-specular path (the `NORMAL_ALPHA_SPEC_BIT` gloss re-point, `normalAlphaSpecMask`, and the env-reflection fallback in `triangle.frag`) is **off for first-seen textures and on for cached ones**: the look of the same asset depends on load order. It also means the audit's #7023 population measurement (3,915 kind-1 meshes on the fallback) describes the warm-cache case only, and any earlier conclusion drawn from a cold single-cell capture (e.g. ice reflection) was made with this path off. `tint_has_alpha` (#4423) is built the same way and has the same code shape (not verified live).

## Suggested Fix
Make the alpha presence available at spawn: e.g. fill `has_alpha` from the parsed DDS header when the slot is reserved in `queue_or_hit_for_view` (the bytes are already in hand) instead of at flush, or refresh the component after the flush. Pick whichever keeps the NIFAL single-boundary rule (the answer must be decided before `translate_material`/spawn, not at render time). Add a regression test that a freshly enqueued DXT5 texture reports `handle_has_alpha == true` before any flush (the existing `handle_has_alpha_reads_the_seeded_flag_and_defaults_false_out_of_range` only covers a seeded flag).

After the fix, re-run the #7023 kind-1 A/B from a **cold** start (`~/Downloads/byro-kind1-ab/ab_run.py` documents the warm-cache recipe used so far) and re-check the Skyrim ice look.

## Completeness Checks
- [ ] **SIBLING**: `tint_has_alpha`, `height_has_alpha` and the flip-texture lane (`anim_convert.rs`) checked for the same pre-flush read
- [ ] **CANONICAL-BOUNDARY**: the alpha-presence answer is resolved before spawn, not re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
