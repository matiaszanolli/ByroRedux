//! Drain a streaming-worker [`PartialNifImport`] into the
//! [`NifImportRegistry`].
//!
//! The streaming worker (see `crate::streaming`) parses NIFs off the
//! main thread and ships a [`PartialNifImport`] back; this function
//! finishes the import — merges BGSM materials, registers any
//! embedded animation clip, and caches the resulting
//! `Arc<CachedNifImport>` in `NifImportRegistry` so subsequent
//! placements of the same model hit cache.

use byroredux_core::ecs::World;
use std::sync::Arc;

use crate::asset_provider::{merge_bgsm_into_mesh, MaterialProvider};

use super::nif_import_registry::{CachedNifImport, NifImportRegistry};

pub(crate) fn finish_partial_import(
    world: &mut World,
    mat_provider: Option<&mut MaterialProvider>,
    mesh_resolver: Option<&dyn byroredux_nif::import::MeshResolver>,
    model_path: &str,
    partial: crate::streaming::PartialNifImport,
) {
    let cache_key = model_path.to_ascii_lowercase();
    // Already-cached early-out (#864). The streaming worker
    // pre-filters its model_paths against `NifImportRegistry`'s
    // cached-keys snapshot (#862), but the snapshot is captured at
    // request-build time and can lag the registry by a few ms — a
    // payload from request A finishing while request B is in flight
    // can populate the cache before B's worker runs, so B's payload
    // still arrives carrying paths that are now cached. Skipping
    // here prevents:
    //   * a redundant `import_nif_with_collision` walk + BGSM merge,
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
    // Editor markers — pre-warmed scene gets cached as `None` so future
    // placements skip silently. Matches the `parse_and_import_nif` skip
    // semantics.
    if partial.bsx & 0x20 != 0 {
        log::debug!("[stream-drain] Skipping editor marker NIF '{}'", model_path);
        let freed = {
            let mut reg = world.resource_mut::<NifImportRegistry>();
            reg.insert(cache_key, None)
        };
        // #863 — release any LRU-evicted clip handles. Negative-cache
        // insert can still trigger eviction of pre-existing entries
        // when `BYRO_NIF_CACHE_MAX > 0`.
        if !freed.is_empty() {
            let mut clip_reg =
                world.resource_mut::<byroredux_core::animation::AnimationClipRegistry>();
            for h in freed {
                clip_reg.release(h);
            }
        }
        return;
    }

    let crate::streaming::PartialNifImport {
        scene,
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

    let (mut meshes, collisions) = {
        let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
        byroredux_nif::import::import_nif_with_collision_and_resolver(
            &scene,
            &mut pool,
            mesh_resolver,
        )
    };
    if let Some(provider) = mat_provider {
        let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
        for mesh in &mut meshes {
            merge_bgsm_into_mesh(mesh, provider, &mut pool);
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

    // Phase 18 — flame-marker offset is left `None` on the
    // streaming-partial path. The helper takes
    // `&ImportedScene` (post-import node array); partial.rs
    // works on the raw `NifScene`. Running the full
    // `import_nif_scene` again here just to get the node
    // names would double the per-NIF parse cost.
    // Streamed-cell candles fall back to the placement-root
    // position (pre-Phase-18 behaviour) until a focused
    // raw-NifScene flame-walker lands as a follow-up.

    let cached = Arc::new(CachedNifImport {
        meshes,
        collisions,
        lights,
        particle_emitters,
        embedded_clip,
        // Partial NIFs are decoded from streamed bytes — no SpeedTree
        // placeholder path runs through here, so no billboard mode.
        placement_root_billboard: None,
        // #1214 — BSXFlags surfaced from the streaming partial. The
        // editor-marker bit (0x20) is filtered upstream at line 53;
        // any cached entry reaching here either has the bit clear OR
        // the partial reader skipped the filter (mod content).
        bsx_flags: bsx,
        // #1235 / LC-D1-NEW-01 — root NiAVObject.flags surfaced from
        // the streaming partial for placement-root SceneFlags parity.
        root_flags,
        // Phase 18 — see note above; streamed-partial path keeps
        // None, sync parse path fills it.
        flame_attach_offset: None,
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
