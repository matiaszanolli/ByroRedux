# NIFAL-D8-2026-10-27-01: The race-skin override's skin-mesh filter copies the whole StringPool for every body piece and only works because an earlier pre-parse interned the NIF's paths

Labels: low, nifal, gameplay, performance, bug, game:oblivion, game:fo3, game:fnv
Source: docs/audits/AUDIT_NIFAL_2026-10-27.md

# NIFAL-D8-2026-10-27-01: The race-skin override's skin-mesh filter copies the whole StringPool for every body piece and only works because an earlier pre-parse interned the NIF's paths

**Source**: `docs/audits/AUDIT_NIFAL_2026-10-27.md` finding `NIFAL-D8-2026-10-27-01` (severity LOW, status NEW, CONFIRMED against HEAD `fa6be4a6b`).
**Location (re-mapped to current paths)**: `byroredux/src/npc_spawn/loot_appearance.rs:78-100` (`skin_texture_symbols` / `skin_texture_symbols_from`); call sites `byroredux/src/npc_spawn/resumable/runtime.rs:726-727` and `loot_appearance.rs:914`.

## Description
#6090 (9d1a262ed) limits `set_body_base_color` -> `override_base_color` to skin meshes. A mesh counts as skin when its authored `base_color` symbol is in a `HashSet` built by `pool.dump()` over the entire world `StringPool`, keeping every string that starts with `textures\characters\`. Two problems:
1. **Cost paid for nothing in most cases.** The snapshot is taken unconditionally for every body piece, before the code checks `body_texture.is_some()` - including creatures (`body_textures: vec![None; ...]`, runtime.rs:614) and every NPC with no race ICON. The hook it serves is attached only when `body_texture.is_some()` or a hand seam applies (runtime.rs:757-764). `dump()` allocates a `String` per interned symbol, each lowercased afterwards: O(pool size) on the main-thread spawn unit, three or four times per NPC. The corpse-restore path (`loot_appearance.rs:914`) does the same.
2. **Order dependence.** The snapshot is taken *before* `load_nif_extracting_with_skeleton`. On a cache miss that call's import (`cached_scene_lazy` -> `import_nif_scene_with_resolver` with the world pool) is what interns the body NIF's own skin path. The filter currently gets the right answer only because `build_seam_context` already ran in the Skeleton phase (on Oblivion/FO3/FNV humanoids with a skeleton it calls `peek_or_parse_scene` on every body path, `seam_blend.rs:94-125`, interning those paths earlier). If that pre-parse does not happen (no skeleton root, `blend_skin_seams` false, or a later reordering) the snapshot lacks the NIF's skin symbol on that NIF's first import, every mesh reads as non-skin and the race skin is silently not applied. No test covers the ordering; `race_skin_overrides_only_the_skin_meshes` builds the pool by hand.

## Evidence
`skin_texture_symbols_from` iterates `pool.dump()` and lowercases each path (loot_appearance.rs:84-96); runtime.rs:719-727 interns the race texture then snapshots, and the load follows at runtime.rs:767; `grep skin_texture_symbols` confirms only runtime.rs:727 and loot_appearance.rs:914 call it.

## Impact
Today a CPU cost per body piece in NPC spawn units. If the spawn order ever changes, the #6090 fix silently stops working (Ghoul and child races would render with the default human skin on the first spawn per body NIF). Violates the single-boundary tier (role-override decision depends on hidden spawn ordering).

## Related
#6090 (the fix this hardens), #6131.

## Suggested Fix
Decide skin-ness from the scene being overridden: inside the hook resolve each mesh's `base_color` symbol and test the prefix (give the hook a pool reader), or record the resolved prefix on `ImportedMesh` at import. Skip all of the work when `body_texture` is `None`. Add a test that exercises the cache-miss order without the seam pre-parse.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser->`Material` boundary - never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **SIBLING**: Same pattern checked in related files / sibling call sites
- [ ] **TESTS**: A regression test pins this specific fix

