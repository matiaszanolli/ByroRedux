# #4427: NIFAL-D8-2026-09-16-02: Flipbook frame handles are acquired with a registry refcount that no unload path ever releases

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4427
- **Labels**: medium,nifal,renderer,memory,game:oblivion,bug
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: MEDIUM. This is a resource leak that grows with every cell load (not every frame). The resident VRAM is bounded by the number of distinct flipbook textures.
- **Dimension**: Shader-flags/Effects (texture-role lifecycle)
- **Tier Violated**: single-boundary. `MaterialTextureSet::values()` / `secondary_values()` is the declared exhaustive lifecycle walk for role textures, and flip frames are role textures held outside it.
- **Game Affected**: Oblivion (60 vanilla flip controllers). Any content with `NiFlipController`.
- **Location**: `byroredux/src/anim_convert.rs:246-253` (acquire). `byroredux/src/cell_loader/unload.rs:494-531` (release walk covers `TextureHandle`, `NormalMapHandle`, `WaterNoiseMapHandles`, `MaterialTextureHandles`, but not `AnimatedTextureFlip`). The cell attach site is `byroredux/src/cell_loader/spawn.rs:833-843`.
- **Status**: NEW (present since #2221, `7fbc5bafb`)
- **Description**: `resolve_texture_view_with_clamp` deliberately uses `acquire_by_path_*`, so "each resolve pairs with one drop_texture on cell unload" (#524). The flipbook path calls it once per frame per placement and stores the handles in `TextureFlipEntry.handles` on the `AnimatedTextureFlip` component. `collect_unload_drops` never queries `AnimatedTextureFlip`, and `grep drop_texture` finds no other consumer of those handles. Every acquired reference therefore outlives the entities that hold it.
- **Evidence**:
  - `byroredux/src/cell_loader/unload.rs:494-499` lists the six queried component types; `AnimatedTextureFlip` is not one of them.
  - The cell path passes `Some(ctx)` / `Some(tex_provider)` into `attach_animation_sinks` for every placement (`byroredux/src/cell_loader/spawn.rs:833-843`), so each placed Oblivion gate acquires 3 × 16 = 48 references per cell load.
  - The census from D8-01 finds 60 flip controllers in the vanilla Oblivion archives (up to 16 frames each).
- **Impact**:
  - Flipbook textures (Oblivion Gate portals, fire and water flipbooks) are never freed once a cell that places them has loaded. Their refcounts grow by the frame count on every re-entry.
  - The registry's deferred-destroy backlog, `texture_pending_destroy_count` (published since `e3131f5ef`), cannot see this because nothing is ever queued.
  - Long exterior sessions keep every flipbook texture ever seen resident.
- **Related**: #524, #2221, #3901, #4117, NIFAL-D8-2026-09-16-01
- **Suggested Fix**:
  1. Add an `AnimatedTextureFlip` arm to `collect_unload_drops` that pushes every `handles` entry through `push_tex_drop`.
  2. Better, give `AnimatedTextureFlip` a `values()`-style walk and extend the unload test that pins `MaterialTextureHandles` release, so the next flipbook-held handle cannot be missed.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
- [ ] **TESTS**: A regression test pins this specific fix
