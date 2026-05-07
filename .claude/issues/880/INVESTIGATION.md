# Investigation — #880 (CELL-PERF-02)

## Domain
nif-parser (NIF parsing + import) + binary (npc_spawn / load_nif_bytes_with_skeleton)

## Hot path

`spawn_npc_entity` (`byroredux/src/npc_spawn.rs:307`) calls
`load_nif_bytes_with_skeleton` (`byroredux/src/scene.rs:1274`) for:
  * skeleton  (`:368`)
  * body      (`:432`)
  * head      (`:592`)
  * (and FaceGen-recipe path at `:761`/`:797`)

`load_nif_bytes_with_skeleton` always does:
  1. `byroredux_nif::parse_nif(data)` (BSA bytes → NifScene)
  2. `byroredux_nif::import::import_nif_scene_with_resolver(&scene, ...)`
     → `ImportedScene { nodes, meshes, particle_emitters, ...}`
  3. Optional BGSM merge across `imported.meshes`
  4. Optional `pre_spawn_hook` callback (FaceGen morph)
  5. Spawn ECS entities (node hierarchy + meshes + materials)

Pre-fix steps 1–3 run once PER NPC, even though skeleton + body +
hand NIFs are SHARED across the cell's NPCs. For Megaton's ~40 NPCs
× ~7 NIFs each ≈ 280 redundant parses.

## Why the existing `NifImportRegistry` doesn't help

`NifImportRegistry` (#381, `cell_loader_nif_import_registry.rs`)
caches `Arc<CachedNifImport>` keyed by lowercased model path. But
`CachedNifImport` is the FLAT-import shape (Vec<ImportedMesh> +
collisions + lights + particle_emitters + embedded_clip) produced
by `import_nif_with_collision_and_resolver` — a different import
function from the hierarchical `import_nif_scene_with_resolver`
that NPC spawn uses. NPC spawn needs the full `ImportedScene` with
its `nodes: Vec<ImportedNode>` to spawn the bone hierarchy.

The fix needs a **separate cache for hierarchical
`ImportedScene` data**, mirroring `NifImportRegistry`'s shape.

## `pre_spawn_hook` complication

The head-NIF spawn (only) passes `pre_spawn_hook: Some(...)` to
mutate `imported.meshes[head].positions` for FaceGen morphs. Each
NPC has unique morphs → caching with the hook applied would give
every NPC the same face. So the cache is consulted **only when
`pre_spawn_hook` is None** — skeleton, body, hand, and head-without-
morph spawns are cached; head-with-morph is left on the legacy
parse-per-call path.

This still captures ≥ 6 of the 7 NIF loads per NPC (the audit's
"~280 redundant parses" is dominated by the shared paths).

## ImportedScene is not Clone

The `ImportedScene` struct doesn't derive `Clone` — its nested
`CollisionShape` / `RigidBodyData` types from parry3d don't either.
So we can't trivially deep-clone the cached scene to apply a
per-instance hook on top. Therefore: bypass cache when hook is Some.

## Files affected

1. `byroredux/src/scene.rs` — add `SceneImportCache` resource +
   refactor `load_nif_bytes_with_skeleton` to consult it on the
   `hook == None` path. Extract `parse_and_import_scene` helper.
2. `byroredux/src/main.rs` — register the new resource at App init
   alongside `NifImportRegistry`.
3. (optional) `byroredux/src/streaming.rs` — verify no
   cross-contamination with the existing streaming snapshot path.

Likely 2-3 files, well within scope.

## Test approach

Add a `parses` counter to `SceneImportCache`. Pin in unit tests
that:
  * Populating an empty cache via `get_or_insert` increments parses
    by 1 on first call, 0 on second call (same key).
  * Cache miss with negative entry (Some(None)) doesn't re-attempt.

Live integration with NPC spawn: the existing
`m41_phase1bx_skinning` integration test exercises NPC spawn end-
to-end; the cache routing is verified by tests + the existing
test suite continuing to pass.
