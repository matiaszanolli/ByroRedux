//! M39 — the host half of texture promotion (docs/engine/texture-streaming.md
//! §4, §10): the renderer's registry asks for a streamed texture's missing
//! levels (`TextureRegistry::take_promotion_requests`), this reads them from
//! the archives on its own small pool, and hands the bytes back
//! (`TextureRegistry::queue_promotion_bytes`), which queues the promotion
//! the per-frame flush uploads.
//!
//! It owns its provider, built from the same archive arguments as every
//! other one, so promotions work in interiors (whose cell-load provider is
//! dropped after the load) as well as exteriors. A BA2 re-read inflates only
//! the chunks the promotion needs (`Ba2Archive::extract_dx10_mips`); a BSA
//! or loose re-read is whole, the cost decision Q2 (b) accepted.

use std::collections::{HashMap, HashSet};
use std::sync::{mpsc, Arc};

use byroredux_core::math::{Mat4, Vec3};
use byroredux_renderer::texture_registry::{
    TextureHandle, TextureRegistry, WantedCase, WantedMip, WantedStats,
};
use byroredux_renderer::vulkan::context::DrawCommand;
use byroredux_renderer::VulkanContext;

use crate::asset_provider::TextureProvider;

/// Reads in flight at once. The pool has two threads, so two queued reads
/// per thread keep it busy without letting a burst of promotion requests
/// stage an unbounded amount of inflated bytes. Phase 3's per-frame cap
/// (TXS-BUD-006) replaces this interim bound with a measured one.
const MAX_READS_IN_FLIGHT: usize = 4;

struct FinishedRead {
    handle: TextureHandle,
    target: u32,
    bytes: Option<Vec<u8>>,
}

pub(crate) struct TextureStreamer {
    provider: Arc<TextureProvider>,
    pool: rayon::ThreadPool,
    in_flight: HashSet<TextureHandle>,
    done_tx: mpsc::Sender<FinishedRead>,
    done_rx: mpsc::Receiver<FinishedRead>,
    /// This frame's wanted mips, kept across frames so the pass allocates
    /// nothing in steady state.
    wanted: HashMap<TextureHandle, WantedMip>,
    /// What the last wanted pass found.
    pub(crate) last_wanted: WantedStats,
}

impl TextureStreamer {
    pub(crate) fn new(provider: TextureProvider) -> Option<Self> {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .thread_name(|i| format!("texture-stream-{i}"))
            .build()
            .map_err(|e| log::warn!("texture streamer pool: {e}"))
            .ok()?;
        let (done_tx, done_rx) = mpsc::channel();
        Some(Self {
            provider: Arc::new(provider),
            pool,
            in_flight: HashSet::new(),
            done_tx,
            done_rx,
            wanted: HashMap::new(),
            last_wanted: WantedStats::default(),
        })
    }

    /// M39 — the wanted-mip pass over this frame's draws ([`compute_wanted`]),
    /// applied to the registry: promotion requests in priority order and
    /// hysteresis demotions (TXS-WANT-001…005).
    pub(crate) fn update_wanted(
        &mut self,
        draws: &[DrawCommand],
        ctx: &mut VulkanContext,
        camera_pos: [f32; 3],
        fov_y: f32,
    ) {
        compute_wanted(draws, ctx, camera_pos, fov_y, &mut self.wanted);
        self.last_wanted = ctx.texture_registry.apply_wanted(&self.wanted);
    }

    /// Land the reads that finished since the last frame, then start new
    /// ones for the registry's queued requests, up to the in-flight bound.
    /// Returns (landed, started).
    pub(crate) fn step(&mut self, registry: &mut TextureRegistry) -> (usize, usize) {
        let mut landed = 0;
        while let Ok(read) = self.done_rx.try_recv() {
            self.in_flight.remove(&read.handle);
            let Some(bytes) = read.bytes else {
                log::warn!(
                    "texture promotion read for handle {} found no archive entry",
                    read.handle
                );
                continue;
            };
            if registry.queue_promotion_bytes(read.handle, read.target, bytes) {
                landed += 1;
            } else {
                log::debug!(
                    "texture promotion for handle {} to level {} was not queued (released or already there)",
                    read.handle,
                    read.target,
                );
            }
        }
        let room = MAX_READS_IN_FLIGHT.saturating_sub(self.in_flight.len());
        let mut started = 0;
        for request in registry.take_promotion_requests(room) {
            if !self.in_flight.insert(request.handle) {
                continue;
            }
            let provider = Arc::clone(&self.provider);
            let done = self.done_tx.clone();
            self.pool.spawn(move || {
                let bytes = provider.extract_texture_from(&request.path, request.target);
                // A closed channel means the streamer is gone; nothing to land.
                let _ = done.send(FinishedRead {
                    handle: request.handle,
                    target: request.target,
                    bytes,
                });
            });
            started += 1;
        }
        (landed, started)
    }
}

/// M39 TXS-WANT-001/002 — this frame's wanted mip for every streamed texture
/// a draw references (docs/engine/texture-streaming.md §5.2–§5.3), into
/// `out` (cleared first; kept by the caller so steady state allocates
/// nothing). Per draw: the mesh's object-space bound and UV texel density,
/// scaled by the largest axis of the model matrix; the distance from the
/// camera to the bound's surface; and
/// `wanted = floor(log2(width / px_per_uv))`, with
/// `px_per_uv = viewport_height / (2·tan(fov_y/2)) · world_per_uv · scale / distance`.
/// A texture's wanted mip is the finest over its instances; one seen only
/// through the TLAS is wanted one level coarser (the secondary-ray rule,
/// §5.3). A mesh with degenerate UVs wants no level finer than the floor,
/// so it contributes nothing. A drawn terrain tile wants its layer textures
/// at level 0: layer UVs tile by a per-layer scale this estimate does not
/// model yet.
pub(crate) fn compute_wanted(
    draws: &[DrawCommand],
    ctx: &VulkanContext,
    camera_pos: [f32; 3],
    fov_y: f32,
    out: &mut HashMap<TextureHandle, WantedMip>,
) {
    out.clear();
    let registry = &ctx.texture_registry;
    let (_, viewport_height) = ctx.swapchain_extent();
    let px_per_unit_at_unit_depth = viewport_height as f32 * 0.5 / (fov_y * 0.5).tan().max(1.0e-4);
    let camera = Vec3::from_array(camera_pos);
    let mut want = |handle: TextureHandle, level: u32, case: WantedCase, distance: f32| {
        if handle == 0 || registry.stream_width(handle).is_none() {
            return;
        }
        let candidate = WantedMip {
            level,
            case,
            distance,
        };
        out.entry(handle)
            .and_modify(|w| {
                if (level, case) < (w.level, w.case) {
                    w.level = level;
                    w.case = case;
                }
                w.distance = w.distance.min(distance);
            })
            .or_insert(candidate);
    };
    for draw in draws {
        if !(draw.in_raster || draw.in_tlas) {
            continue;
        }
        let case = if draw.in_raster {
            WantedCase::Drawn
        } else {
            WantedCase::TlasOnly
        };
        if let Some(tile) = draw
            .terrain_tile_index
            .and_then(|slot| ctx.terrain_tile(slot))
        {
            for &handle in tile
                .layer_diffuse_index
                .iter()
                .chain(&tile.layer_normal_index)
                .chain(&tile.layer_specular_index)
                .chain(std::iter::once(&tile.base_diffuse_index))
            {
                want(handle, 0, case, 0.0);
            }
            continue;
        }
        let meshes = &ctx.mesh_registry;
        let (Some(world_per_uv), Some(bound)) = (
            meshes.world_per_uv(draw.mesh_handle),
            meshes.local_bound(draw.mesh_handle),
        ) else {
            continue;
        };
        let model = Mat4::from_cols_array(&draw.model_matrix);
        let scale = model
            .x_axis
            .truncate()
            .length()
            .max(model.y_axis.truncate().length())
            .max(model.z_axis.truncate().length());
        let centre = model.transform_point3(Vec3::new(bound[0], bound[1], bound[2]));
        let distance = (camera.distance(centre) - bound[3] * scale).max(0.1);
        let px_per_uv = px_per_unit_at_unit_depth * world_per_uv * scale / distance;
        for handle in [
            draw.texture_handle,
            draw.normal_map_index,
            draw.dark_map_index,
            draw.glow_map_index,
            draw.detail_map_index,
            draw.gloss_map_index,
            draw.parallax_map_index,
            draw.env_mask_index,
        ] {
            let Some(width) = registry.stream_width(handle) else {
                continue;
            };
            let ratio = width as f32 / px_per_uv.max(1.0e-6);
            let mut level = if ratio > 1.0 {
                ratio.log2().floor() as u32
            } else {
                0
            };
            if case == WantedCase::TlasOnly {
                level += 1;
            }
            want(handle, level, case, distance);
        }
    }
}
