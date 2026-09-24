# #4794: PERF-D7-2026-09-23b-01: Seam blending (`5570c221c`) adds ~2.9 MB of archive inflate and four hand-NIF re-parses to every Oblivion/FO3/FNV NPC spawn on the main thread, with nothing reused across NPCs

**Severity**: MEDIUM
**Labels**: medium, performance, import-pipeline, game:oblivion, game:fo3, game:fnv, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23b.md (PERF-D7-2026-09-23b-01)

- **Severity**: MEDIUM
- **Dimension**: Streaming & Cells (NPC spawn)
- **Location**:
  - `byroredux/src/npc_spawn/seam_blend.rs`: `:84-118` (`ToneSampler`), `:281-307` (brute-force match), `:365-385` (fade).
  - `byroredux/src/npc_spawn/resumable.rs`: `:846-852`, `:863-913` (hand hook), `:1313-1336` (head hook), `:1510-1560` (`build_seam_context`), `:1708-1720` (`yup_egm_morphs`).
  - `byroredux/src/scene/nif_loader.rs`: `:201-214` (`peek_or_parse_scene`), `:440-471` (the hook bypass; the comment at `:446-447` is now false).
- **Status**: NEW. Partial Regression of #880 (the hand-NIF cache hit; see Eroded guards).
- **Description**: For every Oblivion or FO3/FNV humanoid (`resumable.rs:677`):
  - **(a) Skin textures are re-extracted per part.** Each part builds its own `ToneSampler`, whose whole-DDS cache dies with the part, and `TextureProvider::extract` has no cache.
    - Vanilla FO3 sizes: head + body-skin neighbour 1.40 MB; each hand + neighbour 0.74 MB; ≈ **2.88 MB per NPC**. The same bytes repeat for every NPC of that race and gender.
    - `Fallout - Textures.bsa` has flags 0x107, so these are zlib inflates.
  - **(b) Hands lost the import cache.** The hook path parses and imports without inserting into `SceneImportCache`, and `peek_or_parse_scene` only *peeks*. Each 86.7 KB hand NIF is therefore parsed and imported twice per NPC: 4 parses, where it used to be 0 after the first NPC. Peek-miss parses are also missing from the cache's `parses` counter.
  - **(c) Matching is brute force.** Head boundary vertices × ~2,000–3,000 neighbour vertices, with no spatial index.
  - **(d) The EGM is copied whole.** `yup_egm_morphs` makes a Y-up copy of the full EGM per NPC: 80 morphs × 1,449 verts × 12 B = 1.39 MB in 82 allocations, although the EGM is shared per head mesh.
- **Evidence**:
  - `resumable.rs:873` creates `ToneSampler::new(tex_provider)` per hand part.
  - `nif_loader.rs:206-213`: peek, then `extract_mesh` + `parse_import_and_merge`, with no insert. The orchestrator confirmed this.
- **Impact**:
  - *est.* 5–12 ms per NPC in release, inside `FrameTimeBudget` units. A Megaton-scale cell (~30 humanoids) adds ~150–350 ms of apply work, about 10–20 more budgeted frames.
  - Boot, save-load and debug loads are unbudgeted and take all of it in one frame. Debug builds are several times worse.
- **Related**: #880 / CELL-PERF-02, #4617
- **Suggested Fix**:
  - Add a shared per-texture tone cache resource (the 256-wide mip is enough).
  - Have the hook path clone the cached `Arc<ImportedScene>`, inserting on a miss, and make `peek_or_parse_scene` insert too.
  - Convert the EGM in place, or cache it Y-up per head path.
  - Put a bucketed grid over the neighbour vertices.
  - Pin "a second NPC with the same hand path performs 0 hand parses".

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
