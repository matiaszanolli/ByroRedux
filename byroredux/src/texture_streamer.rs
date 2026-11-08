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

use rustc_hash::FxHashMap;
use std::collections::HashSet;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use byroredux_core::ecs::TextureResidencyTelemetry;
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
    wanted: FxHashMap<TextureHandle, WantedMip>,
    /// What the last wanted pass found.
    pub(crate) last_wanted: WantedStats,
    /// PRD §8 settle time.
    settle: SettleClock,
}

/// Camera position, forward and vertical field of view: what the wanted-mip
/// pass reads, so a change of any of them restarts the settle clock.
pub(crate) type StreamPose = ([f32; 3], [f32; 3], f32);

/// A camera within this many world units of the pose its settle clock
/// started at has not moved. A pinned bench camera wobbles by ~0.002 units
/// between frames (MedTekResearch01, 2026-11-04), far below anything that
/// moves a wanted level; comparing against the clock's start pose rather
/// than last frame's lets a slow drift still add up to a move.
const SETTLE_MOVE_UNITS: f32 = 1.0;
/// cos 0.1°: a forward vector turned less than this has not turned.
const SETTLE_TURN_COS: f32 = 0.999_998_5;

/// M39 — PRD §8 settle time: "camera stop → no promotion queued or in
/// flight". The clock starts when the camera moves or turns past the
/// tolerances above, its field of view changes, or a cell load's textures
/// land (the load is a camera stop for the textures it brings), and stops on
/// the first frame with no promotion requested, queued or being read.
#[derive(Debug, Default)]
pub(crate) struct SettleClock {
    pose: Option<StreamPose>,
    loads_seen: u64,
    since: Option<Instant>,
    last: Option<Duration>,
    /// Times the clock restarted: a reading is only meaningful when the
    /// camera stopped, which a count near the frame count disproves.
    restarts: u64,
}

/// What the `tex-stream:` line prints for settle: whether the clock is
/// stopped, and its last reading (or, while it runs, the time so far).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SettleReading {
    pub(crate) settled: bool,
    pub(crate) ms: f64,
    pub(crate) restarts: u64,
}

impl SettleClock {
    pub(crate) fn observe(&mut self, pose: StreamPose, loads: u64, idle: bool, now: Instant) {
        let moved = self.pose.is_none_or(|start| {
            Vec3::from_array(start.0).distance(Vec3::from_array(pose.0)) > SETTLE_MOVE_UNITS
                || Vec3::from_array(start.1).dot(Vec3::from_array(pose.1)) < SETTLE_TURN_COS
                || start.2 != pose.2
        });
        if moved || self.loads_seen != loads {
            self.pose = Some(pose);
            self.loads_seen = loads;
            self.since = Some(now);
            self.restarts += 1;
        }
        if idle {
            if let Some(since) = self.since.take() {
                self.last = Some(now.saturating_duration_since(since));
            }
        }
    }

    pub(crate) fn reading(&self, now: Instant) -> SettleReading {
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        match self.since {
            Some(since) => SettleReading {
                settled: false,
                ms: ms(now.saturating_duration_since(since)),
                restarts: self.restarts,
            },
            None => SettleReading {
                settled: true,
                ms: self.last.map_or(0.0, ms),
                restarts: self.restarts,
            },
        }
    }
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
            wanted: FxHashMap::default(),
            last_wanted: WantedStats::default(),
            settle: SettleClock::default(),
        })
    }

    /// Advance the settle clock after this frame's reads and flush.
    pub(crate) fn observe_settle(&mut self, registry: &TextureRegistry, pose: StreamPose) {
        let idle = self.in_flight.is_empty() && registry.promotions_idle();
        self.settle
            .observe(pose, registry.cell_loads_timed(), idle, Instant::now());
    }

    pub(crate) fn settle_reading(&self) -> SettleReading {
        self.settle.reading(Instant::now())
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
            self.spawn_read(request.handle, request.target, move || {
                provider.extract_texture_from(&request.path, request.target)
            });
            started += 1;
        }
        (landed, started)
    }

    /// Run one archive `read` on the pool and report it over the done
    /// channel. The read runs under `catch_unwind` (#6810): the pool has no
    /// panic handler, and rayon aborts the process on an unhandled spawned-job
    /// panic. The read inflates untrusted archive bytes, the class #6119 and
    /// #6233 guard in the pre-parse and terrain-height pools, so a panic must
    /// land as an unreadable entry (`bytes: None`) and still send its
    /// `FinishedRead`, or `step` would keep the handle in `in_flight` forever.
    fn spawn_read(
        &self,
        handle: TextureHandle,
        target: u32,
        read: impl FnOnce() -> Option<Vec<u8>> + Send + 'static,
    ) {
        let done = self.done_tx.clone();
        self.pool.spawn(move || {
            let bytes = std::panic::catch_unwind(std::panic::AssertUnwindSafe(read))
                .unwrap_or_else(|_| {
                    log::error!(
                        "texture promotion read for handle {handle} (level {target}) panicked"
                    );
                    None
                });
            // A closed channel means the streamer is gone; nothing to land.
            let _ = done.send(FinishedRead {
                handle,
                target,
                bytes,
            });
        });
    }
}

/// M39 TXS-TEL-002 — the per-session `tex-stream:` bench line, from the
/// residency telemetry and the streamer's settle clock (`None` when no
/// streamer runs). With streaming off nothing promotes, so the run is
/// settled at zero and `floor_ready_ms` times the full-chain control (A6).
/// Evictions and declines are the budget's (TXS-BUD): they read zero while
/// the budget is not exceeded.
pub(crate) fn tex_stream_bench_line(
    t: &TextureResidencyTelemetry,
    settle: Option<SettleReading>,
) -> String {
    let settle = settle.unwrap_or(SettleReading {
        settled: !t.streaming,
        ms: 0.0,
        restarts: 0,
    });
    format!(
        "tex-stream: streaming={} resident_bytes={} peak_bytes={} floor_bytes={} \
         promotions={} demotions={} evictions={} declines={} churn={} \
         hysteresis_frames={} cell_loads={} floor_ready_ms={:.2} reserved={} \
         deficits={} settled={} settle_ms={:.2} settle_restarts={} budget_bytes={} \
         peak_above_floor_bytes={}",
        u8::from(t.streaming),
        t.resident_bytes,
        t.peak_resident_bytes,
        t.floor_bytes,
        t.promotions,
        t.demotions,
        t.evictions,
        t.declines,
        t.churn,
        t.hysteresis_frames,
        t.cell_loads_timed,
        t.floor_ready_ms,
        t.reserved,
        t.deficits,
        u8::from(settle.settled),
        settle.ms,
        settle.restarts,
        t.budget_bytes,
        t.peak_above_floor_bytes,
    )
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
    out: &mut FxHashMap<TextureHandle, WantedMip>,
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

#[cfg(test)]
mod tests {
    use super::*;

    const POSE: StreamPose = ([0.0, 1.0, 2.0], [0.0, 0.0, 1.0], 1.2);

    /// A pool read that panics must not abort the process (#6810): the job
    /// reports an unreadable entry and the `FinishedRead` still arrives, so
    /// `step` clears `in_flight` and the handle can be requested again.
    #[test]
    fn a_panicking_read_is_reported_unreadable_not_fatal() {
        let streamer = TextureStreamer::new(TextureProvider::new()).expect("pool builds");
        streamer.spawn_read(7, 2, || panic!("decoder blew up"));
        let read = streamer
            .done_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("the panicking read never reported");
        assert_eq!((read.handle, read.target), (7, 2));
        assert!(read.bytes.is_none());
        // The pool survived the panic and still serves reads.
        streamer.spawn_read(8, 1, || Some(vec![1, 2, 3]));
        let read = streamer
            .done_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("the pool died with the panicking job");
        assert_eq!(read.bytes.as_deref(), Some(&[1u8, 2, 3][..]));
    }

    /// The clock starts on the first pose, stops on the first idle frame,
    /// and restarts on a pose change or a newly timed cell load.
    #[test]
    fn settle_clock_restarts_on_a_pose_change_or_a_cell_load() {
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let mut clock = SettleClock::default();
        clock.observe(POSE, 1, false, at(0));
        assert_eq!(clock.reading(at(40)), SettleReading { settled: false, ms: 40.0, restarts: 1 });
        clock.observe(POSE, 1, false, at(50));
        clock.observe(POSE, 1, true, at(120));
        assert_eq!(clock.reading(at(500)), SettleReading { settled: true, ms: 120.0, restarts: 1 });
        // Idle frames after settling leave the reading alone.
        clock.observe(POSE, 1, true, at(600));
        assert_eq!(clock.reading(at(700)).ms, 120.0);
        // The camera turns: a new run.
        let turned = (POSE.0, [1.0, 0.0, 0.0], POSE.2);
        clock.observe(turned, 1, false, at(1_000));
        clock.observe(turned, 1, true, at(1_030));
        assert_eq!(clock.reading(at(1_100)), SettleReading { settled: true, ms: 30.0, restarts: 2 });
        // Sub-unit wobble is not a move: no restart.
        let wobble = ([0.0, 1.002, 2.0], turned.1, POSE.2);
        clock.observe(wobble, 1, false, at(1_200));
        assert_eq!(clock.reading(at(1_300)), SettleReading { settled: true, ms: 30.0, restarts: 2 });
        // A cell load's textures land with the camera still: a new run, and
        // already idle on the frame it starts.
        clock.observe(turned, 2, true, at(2_000));
        assert_eq!(clock.reading(at(2_100)), SettleReading { settled: true, ms: 0.0, restarts: 3 });
    }

    /// The line carries every TXS-TEL-002 field; with streaming off and no
    /// streamer it reads settled at zero.
    #[test]
    fn bench_line_carries_the_tel_002_fields() {
        let t = TextureResidencyTelemetry {
            resident_bytes: 100,
            peak_resident_bytes: 200,
            floor_bytes: 30,
            promotions: 4,
            demotions: 1,
            evictions: 2,
            declines: 3,
            budget_bytes: 4096,
            peak_above_floor_bytes: 77,
            churn: 0,
            hysteresis_frames: 120,
            cell_loads_timed: 1,
            floor_ready_ms: 812.345,
            deficits: 0,
            ..Default::default()
        };
        let line = tex_stream_bench_line(&t, None);
        assert_eq!(
            line,
            "tex-stream: streaming=0 resident_bytes=100 peak_bytes=200 floor_bytes=30 \
             promotions=4 demotions=1 evictions=2 declines=3 churn=0 \
             hysteresis_frames=120 cell_loads=1 floor_ready_ms=812.35 reserved=0 \
             deficits=0 settled=1 settle_ms=0.00 settle_restarts=0 budget_bytes=4096 \
             peak_above_floor_bytes=77"
        );
        let on = TextureResidencyTelemetry {
            streaming: true,
            ..Default::default()
        };
        assert!(tex_stream_bench_line(&on, None).ends_with("settle_restarts=0 budget_bytes=0 peak_above_floor_bytes=0"));
        assert!(tex_stream_bench_line(
            &on,
            Some(SettleReading { settled: true, ms: 1.5, restarts: 2 })
        )
        .ends_with("settle_restarts=2 budget_bytes=0 peak_above_floor_bytes=0"));
    }
}
