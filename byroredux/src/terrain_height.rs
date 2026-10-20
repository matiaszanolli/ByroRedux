//! Per-terrain-layer height maps: background generation and GPU hand-off
//! (#6195).
//!
//! A terrain layer's height map is generated once per source texture
//! (`asset_provider::terrain_height` picks the source and caches the result on
//! disk) on a small dedicated pool, never on the main thread. Terrain spawn
//! binds every map that is already available; a lane whose map is still being
//! generated starts at 0 ("no height") and [`apply_ready_heights`] patches it
//! into the live tile the frame its map lands. Each patched handle is owned by
//! the tile record, so the ordinary cell-unload release
//! (`GpuTerrainTile::texture_indices`) drops it with the rest.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use byroredux_core::ecs::{Resource, World};
use byroredux_plugin::esm::reader::GameKind;
use byroredux_renderer::shader_constants::TERRAIN_SPLAT_LAYERS;
use byroredux_renderer::{TextureColorSpace, VulkanContext};

use crate::asset_provider::terrain_height::{
    default_cache_dir, normal_map_green, run_job, CachedHeight, HeightJob, LayerHeightSources,
};
use crate::asset_provider::TextureProvider;
use crate::components::{TerrainLayerHeights, TerrainTileSlot};

/// Generation threads. Two keeps first-visit generation (an FFT per new
/// layer texture) well clear of the cell-stream pool and the frame's
/// parallel stages; a layer that misses still renders, only without height.
const WORKER_THREADS: usize = 2;

/// Where a layer's map stands this session.
#[derive(Debug, Clone)]
pub(crate) enum HeightState {
    Pending,
    Ready(CachedHeight),
    Failed(String),
}

/// Session state for every layer source seen so far, plus the pool the jobs
/// run on.
pub(crate) struct TerrainHeightService {
    pool: rayon::ThreadPool,
    cache_dir: std::path::PathBuf,
    states: HashMap<LayerHeightSources, HeightState>,
    /// Finished jobs, filled by the pool and emptied by [`Self::drain`].
    finished: Arc<Mutex<Vec<(LayerHeightSources, Result<CachedHeight, String>)>>>,
}
impl Resource for TerrainHeightService {}

impl TerrainHeightService {
    pub(crate) fn new(cache_dir: std::path::PathBuf) -> Self {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(WORKER_THREADS)
            .thread_name(|i| format!("byro-terrain-height-{i}"))
            .build()
            .expect("terrain-height pool");
        Self {
            pool,
            cache_dir,
            states: HashMap::new(),
            finished: Arc::default(),
        }
    }

    pub(crate) fn state(&self, sources: &LayerHeightSources) -> Option<&HeightState> {
        self.states.get(sources)
    }

    /// The map for `sources` if it is ready; otherwise make sure a job is
    /// queued for it and return `None`.
    pub(crate) fn request(
        &mut self,
        sources: &LayerHeightSources,
        game: GameKind,
        tex_provider: &TextureProvider,
    ) -> Option<CachedHeight> {
        match self.states.get(sources) {
            Some(HeightState::Ready(cached)) => return Some(cached.clone()),
            Some(_) => return None,
            None => {}
        }
        self.states.insert(sources.clone(), HeightState::Pending);
        let job = HeightJob {
            sources: sources.clone(),
            green: normal_map_green(game),
            archives: tex_provider.texture_archive_snapshot(),
            cache_dir: self.cache_dir.clone(),
        };
        let finished = Arc::clone(&self.finished);
        self.pool.spawn(move || {
            let result = run_job(&job);
            finished
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((job.sources, result));
        });
        None
    }

    /// Fold finished jobs into the state map; returns the ones now ready.
    fn drain(&mut self) -> Vec<(LayerHeightSources, CachedHeight)> {
        let finished =
            std::mem::take(&mut *self.finished.lock().unwrap_or_else(|e| e.into_inner()));
        let mut ready = Vec::new();
        for (sources, result) in finished {
            match result {
                Ok(cached) => {
                    log::debug!(
                        target: "engine::terrain_height",
                        "height ready: {} ({:?}, relief {:.1})",
                        cached.source_path,
                        cached.meta.source,
                        cached.meta.relief
                    );
                    self.states
                        .insert(sources.clone(), HeightState::Ready(cached.clone()));
                    ready.push((sources, cached));
                }
                Err(reason) => {
                    log::warn!(target: "engine::terrain_height", "no height map for {sources:?}: {reason}");
                    self.states.insert(sources, HeightState::Failed(reason));
                }
            }
        }
        ready
    }
}

/// The session's service, created on first use.
fn service(world: &mut World) -> byroredux_core::ecs::ResourceWrite<'_, TerrainHeightService> {
    if world.try_resource::<TerrainHeightService>().is_none() {
        world.insert_resource(TerrainHeightService::new(default_cache_dir()));
    }
    world.resource_mut::<TerrainHeightService>()
}

/// Bind `cached` as a texture: the registry's live entry if there is one
/// (refcount bump), else the cached `.dds` enqueued for the next upload flush.
/// Heights are data, so linear; terrain tiles, so wrap (`clamp_mode` 3).
fn acquire_height_texture(ctx: &mut VulkanContext, cached: &CachedHeight) -> u32 {
    if let Some(handle) = ctx
        .texture_registry
        .acquire_by_path_with_clamp_and_color_space(
            &cached.registry_key,
            3,
            TextureColorSpace::Linear,
        )
    {
        return handle;
    }
    let bytes = match std::fs::read(&cached.dds_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            log::warn!(target: "engine::terrain_height", "read {}: {e}", cached.dds_path.display());
            return 0;
        }
    };
    match ctx.texture_registry.enqueue_dds_with_clamp_and_color_space(
        &ctx.device,
        &cached.registry_key,
        bytes,
        3,
        TextureColorSpace::Linear,
    ) {
        Ok(handle) => handle,
        Err(e) => {
            log::warn!(target: "engine::terrain_height", "upload {}: {e}", cached.dds_path.display());
            0
        }
    }
}

/// Terrain spawn for a cell with no splat tile (BTXT only): queue its base
/// map so it exists — and shows in `terrain.height` — even though there is no
/// tile lane to bind it to yet.
pub(crate) fn request_heights(
    world: &mut World,
    tex_provider: &TextureProvider,
    game: GameKind,
    lanes: &TerrainLayerHeights,
) {
    for sources in std::iter::once(&lanes.base)
        .chain(lanes.layers.iter())
        .flatten()
    {
        service(world).request(sources, game, tex_provider);
    }
}

/// Terrain spawn: the height handles for a tile's base and splat lanes. A
/// lane whose map is ready is bound now (one acquire each, owned by the tile);
/// the rest are queued and stay 0 until [`apply_ready_heights`].
pub(crate) fn acquire_spawn_heights(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    game: GameKind,
    lanes: &TerrainLayerHeights,
) -> (u32, [u32; TERRAIN_SPLAT_LAYERS as usize]) {
    let mut ready = |sources: &Option<LayerHeightSources>| -> Option<CachedHeight> {
        service(world).request(sources.as_ref()?, game, tex_provider)
    };
    let base = ready(&lanes.base);
    let layers: Vec<Option<CachedHeight>> = lanes.layers.iter().map(&mut ready).collect();
    let mut layer_indices = [0u32; TERRAIN_SPLAT_LAYERS as usize];
    for (index, cached) in layer_indices.iter_mut().zip(&layers) {
        if let Some(cached) = cached {
            *index = acquire_height_texture(ctx, cached);
        }
    }
    let base_index = base.map_or(0, |cached| acquire_height_texture(ctx, &cached));
    (base_index, layer_indices)
}

/// Per frame: patch every finished map into the live tiles still waiting on
/// it. Cheap when nothing finished (one lock, one empty swap).
pub(crate) fn apply_ready_heights(world: &World, ctx: &mut VulkanContext) {
    // Take the finished list and release the service before touching any
    // component storage.
    let ready = match world.try_resource_mut::<TerrainHeightService>() {
        Some(mut service) => service.drain(),
        None => return,
    };
    if ready.is_empty() {
        return;
    }
    let ready: HashMap<&LayerHeightSources, &CachedHeight> = ready
        .iter()
        .map(|(sources, cached)| (sources, cached))
        .collect();
    let mut patches: Vec<(u32, Option<usize>, &CachedHeight)> = Vec::new();
    {
        let (Some(slots), Some(lanes)) = (
            world.query::<TerrainTileSlot>(),
            world.query::<TerrainLayerHeights>(),
        ) else {
            return;
        };
        for (entity, lanes) in lanes.iter() {
            let Some(slot) = slots.get(entity) else {
                continue;
            };
            let base = std::iter::once((None, &lanes.base));
            let layers = lanes.layers.iter().enumerate().map(|(i, s)| (Some(i), s));
            for (lane, sources) in base.chain(layers) {
                if let Some(cached) = sources.as_ref().and_then(|s| ready.get(s)) {
                    patches.push((slot.0, lane, cached));
                }
            }
        }
    }
    let mut patched = 0usize;
    for (slot, lane, cached) in patches {
        let current = ctx.terrain_tile(slot).map(|tile| match lane {
            None => tile.base_height_index,
            Some(i) => tile.layer_height_index[i],
        });
        // Only an empty lane: a non-zero one was bound at spawn and already
        // owns its reference.
        if current != Some(0) {
            continue;
        }
        let handle = acquire_height_texture(ctx, cached);
        if handle == 0 {
            continue;
        }
        ctx.update_terrain_tile(slot, |tile| match lane {
            None => tile.base_height_index = handle,
            Some(i) => tile.layer_height_index[i] = handle,
        });
        patched += 1;
    }
    if patched > 0 {
        crate::scene::flush_pending_loose_textures(ctx);
        log::debug!(target: "engine::terrain_height", "patched {patched} terrain height lanes");
    }
}
