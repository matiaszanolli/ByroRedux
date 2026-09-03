//! Drain a streaming-worker [`PartialNifImport`] into the
//! [`NifImportRegistry`].
//!
//! The streaming worker (see `crate::streaming`) parses and imports NIFs off
//! the main thread, then ships a [`PartialNifImport`] back; this function
//! rebinds worker-local string handles, merges BGSM materials, registers any
//! embedded animation clip, and caches the resulting
//! `Arc<CachedNifImport>` in `NifImportRegistry` so subsequent
//! placements of the same model hit cache.

use byroredux_core::ecs::World;
use byroredux_core::string::StringPool;
use std::sync::Arc;

use crate::asset_provider::{merge_external_material, MaterialProvider};

use super::nif_import_registry::{canonical_model_path_key, CachedNifImport, NifImportRegistry};
use super::references::{find_flame_attach_offset, furniture_component};

pub(crate) fn finish_partial_import(
    world: &mut World,
    mat_provider: Option<&mut MaterialProvider>,
    model_path: &str,
    partial: crate::streaming::PartialNifImport,
) {
    let cache_key = canonical_model_path_key(model_path);
    // Already-cached early-out (#864). The streaming worker
    // pre-filters its model_paths against `NifImportRegistry`'s
    // cached-keys snapshot (#862), but the snapshot is captured at
    // request-build time and can lag the registry by a few ms — a
    // payload from request A finishing while request B is in flight
    // can populate the cache before B's worker runs, so B's payload
    // still arrives carrying paths that are now cached. Skipping
    // here prevents:
    //   * a redundant mesh/collision import walk + BGSM merge,
    //   * a stale `convert_nif_clip` + `clip_reg.add` (which would
    //     leak the previous clip handle and overwrite the cache
    //     entry's clip mapping), and
    //   * an `Arc<CachedNifImport>` rebuild that ends up mostly the
    //     same content as the existing arc.
    // Both positive (`Some(Some(_))`) and negative (`Some(None)`)
    // cache hits short-circuit — re-attempting a previously-failed
    // parse is also wasted, and the worker already filters those
    // out at request time.
    if world
        .resource::<NifImportRegistry>()
        .get(&cache_key)
        .is_some()
    {
        return;
    }
    let crate::streaming::PartialNifImport {
        scene,
        mut meshes,
        collisions,
        worker_pool,
        // #1214 — surface BSXFlags onto the cache entry so the spawn
        // site can attach a `BSXFlags` ECS row on the placement root.
        // Pre-#1214 this field was discarded.
        bsx,
        // #1235 / LC-D1-NEW-01 — root NiAVObject.flags for placement-root
        // SceneFlags parity with the loose-NIF loader.
        root_flags,
        lights,
        particle_emitters,
        embedded_clip,
    } = partial;

    let collision_authoring =
        byroredux_nif::import::collision::summarize_collision_authoring(&scene);

    let mut meshes = {
        let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
        reintern_imported_meshes(&mut meshes, &worker_pool, &mut pool);
        meshes
    };
    if let Some(provider) = mat_provider {
        let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
        for mesh in &mut meshes {
            // #2709 (SF-D9-03) — outcome discarded deliberately; this
            // path has no per-cell material tally to feed it into.
            let _ = merge_external_material(&mut mesh.material, provider, &mut pool);
        }
    }

    // Embedded animation clip — register exactly once per unique NIF.
    let clip_handle = embedded_clip.as_ref().map(|nif_clip| {
        let clip = {
            let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
            crate::anim_convert::convert_nif_clip(nif_clip, &mut pool)
        };
        let mut clip_reg = world.resource_mut::<byroredux_core::animation::AnimationClipRegistry>();
        clip_reg.add(clip)
    });

    // Phase 18 — flame-marker offset, computed the same way the
    // synchronous `parse_and_import_nif` path does. #3074: the stated
    // blocker for leaving this `None` here was that the helper needs
    // `&ImportedScene` — false, it takes `&NifScene`, which `scene`
    // (destructured above) already is.
    let flame_attach_offset = find_flame_attach_offset(&scene);

    // M41.5 Phase B — lift `BSFurnitureMarker` sit/sleep/lean positions
    // to the `Furniture` ECS component, mirroring the synchronous path
    // (`references/import.rs`). #3072: this used to hardcode `None`,
    // citing the same (false) blocker as `flame_attach_offset` above —
    // `scene` is a `&NifScene` here too.
    let furniture = {
        let markers = byroredux_nif::import::extract_furniture_markers(&scene);
        if markers.is_empty() {
            None
        } else {
            Some(furniture_component(&markers))
        }
    };

    let cached = Arc::new(CachedNifImport {
        meshes,
        geometry_dedup: Vec::new(),
        collisions,
        collision_authoring,
        lights,
        particle_emitters,
        embedded_clip,
        // Partial NIFs are decoded from streamed bytes — no SpeedTree
        // placeholder path runs through here, so no billboard mode.
        placement_root_billboard: None,
        speedtree_wind: None,
        // #1214 / #3036 — BSXFlags surfaced from the streaming partial.
        // Bit 5 means marker children are present on classic content (and
        // MultiBound metadata on later content), never that the whole NIF
        // is a marker. The shared walker culls marker children individually.
        bsx_flags: bsx,
        // #1235 / LC-D1-NEW-01 — root NiAVObject.flags surfaced from
        // the streaming partial for placement-root SceneFlags parity.
        root_flags,
        flame_attach_offset,
        // #1594 — the streaming-partial path keeps the FO4+ weapon-mod
        // attach graph `None`; the sync `parse_and_import_nif` path
        // materializes it. Unlike `flame_attach_offset` / `furniture`
        // above (#3072 / #3074, both blocked on nothing — the data was
        // sitting in `scene` the whole time), this one is a deliberate
        // scope decision: cell-streamed REFRs are architecture / clutter,
        // not modular weapons, so this is a near-zero-loss follow-up
        // rather than a bug.
        attach_points: None,
        child_attach_connections: None,
        furniture,
    });

    let freed_clip_handles = {
        let mut reg = world.resource_mut::<NifImportRegistry>();
        let freed = reg.insert(cache_key.clone(), Some(cached));
        if let Some(handle) = clip_handle {
            reg.set_clip_handle(cache_key, handle);
        }
        freed
    };
    // Release the keyframes of any clip handles whose owning cache
    // entries were just LRU-evicted (#863). No-op when
    // `BYRO_NIF_CACHE_MAX=0` (default unlimited mode).
    if !freed_clip_handles.is_empty() {
        let mut clip_reg = world.resource_mut::<byroredux_core::animation::AnimationClipRegistry>();
        for h in freed_clip_handles {
            clip_reg.release(h);
        }
    }
}

/// Rebind the `FixedString` texture/material paths produced by a worker-local
/// import pool to the process-wide ECS pool. Symbols are indices into their
/// owning pool rather than self-describing strings, so carrying the pool
/// alongside the imported meshes is necessary before the worker payload is
/// dropped. `MaterialTextureSet::map_ref` keeps this pass exhaustive as new
/// semantic texture roles are added.
fn reintern_imported_meshes(
    meshes: &mut [byroredux_nif::import::ImportedMesh],
    worker_pool: &StringPool,
    world_pool: &mut StringPool,
) {
    for mesh in meshes {
        mesh.material.textures = mesh.material.textures.map_ref(|path| {
            path.as_ref()
                .and_then(|symbol| worker_pool.resolve(*symbol))
                .map(|path| world_pool.intern(path))
        });
        mesh.material.material_path = mesh
            .material
            .material_path
            .and_then(|symbol| worker_pool.resolve(symbol))
            .map(|path| world_pool.intern(path));
    }
}
