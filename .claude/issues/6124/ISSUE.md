# PERF-SD-2026-10-19-02: NPC spawn extracts every shared NIF's bytes before asking the import cache whether it already has the scene

Labels: low,performance,gameplay,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6124

**Severity**: LOW (measured 1.31 ms per FNV humanoid; ~50 ms per 40-NPC cell; negligible on Skyrim SE) · **Dimension**: Streaming & Cells (NPC spawn)

## Location
- `byroredux/src/npc_spawn/resumable/mod.rs` (`spawn_armor_unit`, `spawn_skeleton_unit`)
- `byroredux/src/npc_spawn/resumable/runtime.rs` (body parts, head, `spawn_shared_skeleton_part`: hair, eyes, brows, teeth, tongue, mouth)
- `byroredux/src/npc_spawn/resumable/prebaked.rs` (fallback head parts)
- The consumer: `byroredux/src/scene/nif_loader.rs` (`load_nif_bytes_with_skeleton` -> `cached_scene`), which reads `data` only on a `SceneImportCache` miss.

## Description
Each spawn unit calls `tex_provider.extract_mesh(path)` (archive read + inflate + a `Vec` copy) and passes the bytes in. `cached_scene` then looks the lower-cased path up in `SceneImportCache` and returns the cached `Arc<ImportedScene>` without touching the bytes. For the shared parts (skeleton, body, hands, feet, head parts, hair, armor) every NPC after the first is a cache hit, so the extraction is discarded. `cached_scene` already accepts `data: Option<&[u8]>` and extracts lazily when `None`, so the mechanism for a lazy extraction exists.

## Evidence
Measured with a scratch crate on the repo's `byroredux-bsa` (release, warm page cache, 20 repetitions per file):

| Archive | Parts | Decoded | Time |
|---|---|---:|---:|
| FNV `Fallout - Meshes.bsa` (v104, zlib) | skeleton, upperbody, 2 hands, headhuman + 6 head parts | 799 KB | 1.31 ms per NPC |
| Skyrim SE `Skyrim - Meshes0/1.bsa` (v105, LZ4) | skeleton, malebody_0, malehands_0, malefeet_0 | 301 KB | 0.07 ms per NPC |

`TextureProvider::extract_mesh` is `extract_first` over the archives, no memo. Skyrim/FO4 per-NPC FaceGeom heads are unique per NPC, so their extract is a genuine miss and is correctly needed.

## Impact
Wasted main-thread time inside the budgeted apply slices of every NPC cell on the zlib games: ~1.3 ms x NPCs (FNV Megaton-class ~40 NPCs ~ 50 ms; 10-25 % of the 5-12 ms per-NPC spawn estimate in `AUDIT_PERFORMANCE_2026-09-23b.md`). It lengthens apply completion; it does not stall a frame (units yield).

## Related
Not a regression of #880 / #4794 (parses and textures). NIF-SD-01 (different cache).

## Suggested Fix
Take a lazy extractor (`impl FnOnce() -> Option<Vec<u8>>`) or `Option<&[u8]>` in `load_nif_bytes_with_skeleton` and have the NPC units probe `SceneImportCache` first; keep the eager extract only for the unique FaceGeom head. Pin: a second NPC of the same race performs zero `extract_mesh` calls for skeleton/body/hands.

Source: docs/audits/AUDIT_PERFORMANCE_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
