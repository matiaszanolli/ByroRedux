//! World cell streaming (M40 Phase 1a).
//!
//! Owns the live (gx, gy) → cell_root map and the streaming control
//! parameters. The App-level driver (`app_step.rs`) reads the active
//! camera position each frame, asks
//! [`compute_streaming_deltas`] which cells need to enter or leave the
//! loaded set, and dispatches to
//! [`crate::cell_loader::load_one_exterior_cell`] / [`crate::cell_loader::unload_cell`].
//!
//! ## Hysteresis
//!
//! Cells load at `radius_load` and unload at `radius_unload`
//! (= `radius_load + 1`). A player walking the boundary doesn't thrash
//! a cell every frame: the cell loads as the player crosses into the
//! load radius, stays loaded for one extra cell of travel, and only
//! unloads once the player is genuinely past the boundary.
//!
//! NIF extraction and parsing run on the worker below. The main thread
//! applies completed payloads under a per-frame spawn budget; exterior
//! bootstrap uses the same request and payload path instead of maintaining
//! a second synchronous loader.

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_core::math::coord::EXTERIOR_CELL_UNITS;
use byroredux_renderer::VulkanContext;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::asset_provider::{MaterialProvider, TextureProvider};
use crate::cell_loader::{ExteriorWorldContext, UnloadPhaseTimings};

/// One loaded cell tracked by [`WorldStreamingState`]. The
/// `cell_root` is the `EntityId` returned by
/// `load_one_exterior_cell`; passing it to
/// `crate::cell_loader::unload_cell` tears the cell down (despawn
/// every entity stamped with this `CellRoot`, drop mesh / BLAS /
/// texture refs).
#[derive(Debug, Clone, Copy)]
pub struct LoadedCell {
    pub cell_root: EntityId,
}

/// How many recent samples each [`StreamingLatencySummary`] keeps in order to
/// answer percentile queries.
///
/// Percentiles need a distribution, but the count/total/max aggregate is
/// deliberately constant-memory so a long play session cannot grow it. A
/// fixed ring squares the two: the window is bounded (128 × 4 B = 512 B per
/// phase), and percentiles over it are *exact* rather than the approximation
/// a bucketed histogram of the same size would give.
///
/// 128 comfortably covers a boundary benchmark end to end — a `grid-cross`
/// run crosses three boundaries, so the per-crossing phases record a handful
/// of samples in total. Only the per-frame slice phases (dispatch, apply,
/// LOD) can overflow it, and for those a trailing window is the more useful
/// reading anyway.
const RECENT_LATENCY_SAMPLES: usize = 128;

/// Bounded latency aggregate used by [`StreamingTelemetry`].
///
/// `samples` / `total` / `max` are all-time and constant-memory. The ring
/// behind them retains the most recent [`RECENT_LATENCY_SAMPLES`] durations so
/// [`Self::percentiles_ms`] can report p50/p95 — EX-06 asks for a
/// distribution per phase, and an average hides exactly the tail that a
/// streaming deadline is meant to bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamingLatencySummary {
    pub samples: u64,
    pub total: Duration,
    pub max: Duration,
    /// Most recent sample durations in microseconds, oldest-to-newest only
    /// until the ring wraps (order is irrelevant — every reader sorts).
    recent: [u32; RECENT_LATENCY_SAMPLES],
    /// Live entries in `recent`, saturating at its capacity.
    recent_len: usize,
    /// Next write index; wraps, overwriting the oldest sample.
    recent_next: usize,
}

impl Default for StreamingLatencySummary {
    fn default() -> Self {
        Self {
            samples: 0,
            total: Duration::ZERO,
            max: Duration::ZERO,
            recent: [0; RECENT_LATENCY_SAMPLES],
            recent_len: 0,
            recent_next: 0,
        }
    }
}

impl StreamingLatencySummary {
    fn record(&mut self, elapsed: Duration) {
        self.samples = self.samples.saturating_add(1);
        self.total = self.total.saturating_add(elapsed);
        self.max = self.max.max(elapsed);

        // Microseconds in a `u32` reach ~71 minutes — orders of magnitude
        // past any streaming phase, and half the footprint of nanoseconds.
        self.recent[self.recent_next] = elapsed.as_micros().min(u32::MAX as u128) as u32;
        self.recent_next = (self.recent_next + 1) % RECENT_LATENCY_SAMPLES;
        self.recent_len = (self.recent_len + 1).min(RECENT_LATENCY_SAMPLES);
    }

    pub fn average_ms(self) -> f64 {
        if self.samples == 0 {
            0.0
        } else {
            self.total.as_secs_f64() * 1000.0 / self.samples as f64
        }
    }

    pub fn max_ms(self) -> f64 {
        self.max.as_secs_f64() * 1000.0
    }

    /// `[p50, p95, max]` in milliseconds.
    ///
    /// p50/p95 are nearest-rank over the retained window, matching
    /// `main::bench_frame_distribution`'s convention so the per-phase and
    /// whole-frame numbers in one bench line are directly comparable. `max`
    /// is the all-time maximum, not the window's — a hitch that scrolled out
    /// of the window still happened, and losing it would defeat the point of
    /// the measurement.
    pub fn percentiles_ms(&self) -> [f64; 3] {
        if self.recent_len == 0 {
            return [0.0, 0.0, self.max_ms()];
        }
        let mut sorted = [0u32; RECENT_LATENCY_SAMPLES];
        sorted[..self.recent_len].copy_from_slice(&self.recent[..self.recent_len]);
        sorted[..self.recent_len].sort_unstable();
        let window = &sorted[..self.recent_len];
        let pick = |fraction: f64| {
            let rank = (fraction * self.recent_len as f64).ceil() as usize;
            f64::from(window[rank.saturating_sub(1).min(self.recent_len - 1)]) / 1000.0
        };
        [pick(0.50), pick(0.95), self.max_ms()]
    }
}

#[derive(Debug, Clone, Copy)]
struct ActiveBoundaryTelemetry {
    grid: (i32, i32),
    started_at: Instant,
    full_detail_settled: bool,
    lod_settled: bool,
}

/// Runtime evidence for exterior streaming deadlines.
///
/// One sample begins on every real grid transition (the initial bootstrap
/// seeds `last_player_grid`, so it is not counted). Full-detail and distant
/// LOD completion are timed independently. If another boundary arrives first,
/// the unfinished phase is counted as superseded rather than silently folded
/// into the newer sample. All aggregates are bounded for normal gameplay.
#[derive(Debug, Clone, Default)]
pub struct StreamingTelemetry {
    pub boundary_crossings: u64,
    pub full_detail: StreamingLatencySummary,
    pub lod: StreamingLatencySummary,
    pub dispatch_slices: StreamingLatencySummary,
    pub unload_slices: StreamingLatencySummary,
    pub unload_ownership_index: StreamingLatencySummary,
    pub unload_handle_collection: StreamingLatencySummary,
    pub unload_gpu_release: StreamingLatencySummary,
    pub unload_owned_state_release: StreamingLatencySummary,
    pub unload_despawn: StreamingLatencySummary,
    pub unload_finalization: StreamingLatencySummary,
    pub worker_queue: StreamingLatencySummary,
    pub worker_parse: StreamingLatencySummary,
    pub apply_slices: StreamingLatencySummary,
    pub lod_slices: StreamingLatencySummary,
    pub superseded_full_detail: u64,
    pub superseded_lod: u64,
    pub queued_cells: u64,
    pub unloaded_cells: u64,
    pub worker_payloads: u64,
    pub peak_pending: usize,
    active: Option<ActiveBoundaryTelemetry>,
}

impl StreamingTelemetry {
    pub(crate) fn boundary_in_progress(&self) -> bool {
        self.active.is_some()
    }

    pub(crate) fn begin_boundary(&mut self, grid: (i32, i32), now: Instant) {
        if let Some(previous) = self.active {
            if !previous.full_detail_settled {
                self.superseded_full_detail = self.superseded_full_detail.saturating_add(1);
            }
            if !previous.lod_settled {
                self.superseded_lod = self.superseded_lod.saturating_add(1);
            }
        }
        self.boundary_crossings = self.boundary_crossings.saturating_add(1);
        self.active = Some(ActiveBoundaryTelemetry {
            grid,
            started_at: now,
            full_detail_settled: false,
            lod_settled: false,
        });
    }

    pub(crate) fn observe_pending(&mut self, pending: usize) {
        if self.active.is_none() {
            return;
        }
        self.peak_pending = self.peak_pending.max(pending);
    }

    pub(crate) fn record_apply_slice(&mut self, elapsed: Duration, worked: bool) {
        if self.active.is_some() && worked {
            self.apply_slices.record(elapsed);
        }
    }

    pub(crate) fn record_dispatch_slice(&mut self, elapsed: Duration) {
        self.dispatch_slices.record(elapsed);
    }

    pub(crate) fn record_queued_cells(&mut self, queued: usize) {
        if self.active.is_some() {
            self.queued_cells = self.queued_cells.saturating_add(queued as u64);
        }
    }

    pub(crate) fn record_unload_slice(&mut self, elapsed: Duration, unloaded: usize) {
        if self.active.is_some() && unloaded > 0 {
            self.unload_slices.record(elapsed);
            self.unloaded_cells = self.unloaded_cells.saturating_add(unloaded as u64);
        }
    }

    pub(crate) fn record_unload_phases(&mut self, timings: UnloadPhaseTimings) {
        if self.active.is_none() {
            return;
        }
        self.unload_ownership_index.record(timings.ownership_index);
        self.unload_handle_collection
            .record(timings.handle_collection);
        self.unload_gpu_release.record(timings.gpu_release);
        self.unload_owned_state_release
            .record(timings.owned_state_release);
        self.unload_despawn.record(timings.despawn);
        self.unload_finalization.record(timings.finalization);
    }

    pub(crate) fn record_worker(&mut self, timings: StreamingWorkerTimings) {
        if self.active.is_some() {
            self.worker_payloads = self.worker_payloads.saturating_add(1);
            self.worker_queue.record(timings.queue_wait);
            self.worker_parse.record(timings.worker);
        }
    }

    pub(crate) fn record_lod_slice(&mut self, elapsed: Duration, attempts: usize) {
        if self.active.is_some() && attempts > 0 {
            self.lod_slices.record(elapsed);
        }
    }

    pub(crate) fn settle_full_detail(&mut self, now: Instant) -> Option<((i32, i32), Duration)> {
        let (grid, elapsed) = {
            let active = self.active.as_mut()?;
            if active.full_detail_settled {
                return None;
            }
            active.full_detail_settled = true;
            (
                active.grid,
                now.saturating_duration_since(active.started_at),
            )
        };
        self.full_detail.record(elapsed);
        self.clear_completed_boundary();
        Some((grid, elapsed))
    }

    pub(crate) fn settle_lod(&mut self, now: Instant) -> Option<((i32, i32), Duration)> {
        let (grid, elapsed) = {
            let active = self.active.as_mut()?;
            if active.lod_settled {
                return None;
            }
            active.lod_settled = true;
            (
                active.grid,
                now.saturating_duration_since(active.started_at),
            )
        };
        self.lod.record(elapsed);
        self.clear_completed_boundary();
        Some((grid, elapsed))
    }

    fn clear_completed_boundary(&mut self) {
        if self
            .active
            .is_some_and(|sample| sample.full_detail_settled && sample.lod_settled)
        {
            self.active = None;
        }
    }

    pub fn bench_line(&self) -> String {
        let unsettled_full = self
            .active
            .is_some_and(|sample| !sample.full_detail_settled);
        let unsettled_lod = self.active.is_some_and(|sample| !sample.lod_settled);
        format!(
            "streaming: crossings={} full_samples={} full_avg_ms={:.2} full_max_ms={:.2} \
             full_superseded={} lod_samples={} lod_avg_ms={:.2} lod_max_ms={:.2} \
             lod_superseded={} queued={} unloaded={} worker_payloads={} \
             dispatch_avg_ms={:.2} dispatch_max_ms={:.2} unload_max_ms={:.2} \
             unload_index_max_ms={:.2} unload_collect_max_ms={:.2} \
             unload_gpu_max_ms={:.2} unload_owned_max_ms={:.2} \
             unload_despawn_max_ms={:.2} unload_finalize_max_ms={:.2} \
             worker_queue_avg_ms={:.2} worker_queue_max_ms={:.2} \
             worker_avg_ms={:.2} worker_max_ms={:.2} \
             apply_samples={} apply_avg_ms={:.2} apply_max_ms={:.2} \
             lod_slice_avg_ms={:.2} lod_slice_max_ms={:.2} peak_pending={} \
             unsettled_full={} unsettled_lod={} {} {} {} {} {} {}",
            self.boundary_crossings,
            self.full_detail.samples,
            self.full_detail.average_ms(),
            self.full_detail.max_ms(),
            self.superseded_full_detail,
            self.lod.samples,
            self.lod.average_ms(),
            self.lod.max_ms(),
            self.superseded_lod,
            self.queued_cells,
            self.unloaded_cells,
            self.worker_payloads,
            self.dispatch_slices.average_ms(),
            self.dispatch_slices.max_ms(),
            self.unload_slices.max_ms(),
            self.unload_ownership_index.max_ms(),
            self.unload_handle_collection.max_ms(),
            self.unload_gpu_release.max_ms(),
            self.unload_owned_state_release.max_ms(),
            self.unload_despawn.max_ms(),
            self.unload_finalization.max_ms(),
            self.worker_queue.average_ms(),
            self.worker_queue.max_ms(),
            self.worker_parse.average_ms(),
            self.worker_parse.max_ms(),
            self.apply_slices.samples,
            self.apply_slices.average_ms(),
            self.apply_slices.max_ms(),
            self.lod_slices.average_ms(),
            self.lod_slices.max_ms(),
            self.peak_pending,
            u8::from(unsettled_full),
            u8::from(unsettled_lod),
            // EX-06 per-phase distributions. An average cannot show the tail
            // a streaming deadline exists to bound, so every phase the plan
            // names reports p50/p95/max alongside it. Whole-frame p50/p95/max
            // are emitted by `main`'s own bench line from the CPU frame times.
            phase_distribution("queue_wait", &self.worker_queue),
            phase_distribution("worker_parse", &self.worker_parse),
            phase_distribution("apply", &self.apply_slices),
            phase_distribution("unload", &self.unload_slices),
            phase_distribution("lod_slice", &self.lod_slices),
            phase_distribution("full_detail", &self.full_detail),
        )
    }
}

/// Format one phase's `p50/p95/max` triple for [`StreamingTelemetry::bench_line`].
fn phase_distribution(label: &str, summary: &StreamingLatencySummary) -> String {
    let [p50, p95, max] = summary.percentiles_ms();
    format!("{label}_p50_ms={p50:.2} {label}_p95_ms={p95:.2} {label}_p100_ms={max:.2}")
}

/// Queue and worker-service time carried across the worker channel with each
/// payload. Durations avoid comparing wall clocks and remain valid if worker
/// execution moves to a pool later.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StreamingWorkerTimings {
    pub queue_wait: Duration,
    pub worker: Duration,
}

/// One distant-terrain LOD block tracked by [`WorldStreamingState`]
/// (#1373), keyed by block-coord. `hole_mask` is the 16-bit per-cell
/// hole pattern — bit `dy * LOD_BLOCK_CELLS + dx` is set when that cell
/// is holed (inside the full-detail radius, or missing landscape). When
/// the player moves and a boundary block's mask changes, the block is
/// regenerated so its hole-out tracks the streamed near terrain instead
/// of staying anchored to the spawn cell. Unloading a block calls
/// `drop_mesh(mesh_handle)` (frees its global-SSBO range on the next
/// rebuild) + `World::despawn(entity)`.
#[derive(Debug, Clone, Copy)]
pub struct LodBlock {
    pub entity: EntityId,
    pub mesh_handle: u32,
    /// Base ground `TextureHandle` acquired via `resolve_texture` at spawn
    /// (refcount bump). `World::despawn` has no GPU side effects, so
    /// `unload_lod_block` must `drop_texture` this explicitly or the
    /// refcount never reaches 0 and the VkImage + bindless slot pin for the
    /// session (#1537). `0` = fallback/placeholder, never per-block
    /// refcounted (skip the drop).
    pub texture_handle: u32,
    /// Per-quad tangent-space normal-map `TextureHandle` for a prebaked
    /// `.btr` block (#2371), or `0` when the block has none — the synth
    /// path, FO4's model-space `_msn` variant, and any quad whose `_n`
    /// sibling is missing. Refcounted and dropped exactly like
    /// `texture_handle`.
    pub normal_texture_handle: u32,
    pub hole_mask: u16,
}

/// Distant, worldspace-wide LOD water quad (#2449 / EXAL-01) — the `NAM3`/
/// `NAM4` counterpart of a cell's full-detail `spawn_water_plane`. Unlike
/// [`LodBlock`], this is a SINGLE entity per worldspace, not a
/// per-ring-block streaming set: spawned once at worldspace entry
/// (`cell_loader::water::spawn_lod_water_plane`) and reclaimed once on
/// worldspace exit (`streaming_helpers::drain_streaming_state`).
///
/// Its hole (cut out around the full-detail streamed area, so it doesn't
/// double-blend against the near per-cell water) is a snapshot centered on
/// the player's worldspace-entry grid position — it does NOT re-center
/// continuously as the player walks, the way [`LodBlock`]'s hole mask
/// tracks the moving full-detail boundary every reconcile. Continuous
/// per-block water LOD (mirroring `LodBlock` exactly) was assessed
/// too high-risk for this pass: `LodBlock`'s global-pool-only mesh upload
/// makes it eligible for the renderer's shadow-caster-range auto-BLAS-build
/// path (`build_global_blas_for_draws`), which would make a water quad
/// RT-hittable — breaking the water pipeline's terminate-on-hit ray
/// policy — and that interaction can't be verified without a live/RenderDoc
/// pass. This entity instead uses the SAME safe per-mesh-buffer upload path
/// (`rt_enabled: false`) every full-detail `WaterPlane` already uses, which
/// is proven never to reach the TLAS regardless of `in_tlas`'s computed
/// value (see `cell_loader::water`'s module doc).
#[derive(Debug, Clone, Copy)]
pub struct LodWaterPlane {
    pub entity: EntityId,
    pub mesh_handle: u32,
    /// Normal-map `TextureHandle` acquired via `resolve_texture` at spawn,
    /// mirroring `spawn_water_plane`'s `NormalMapHandle` refcount contract
    /// (#1338). `None` when the procedural-fallback normal is used (no
    /// texture acquired, nothing to release).
    pub normal_map_handle: Option<u32>,
}

/// Worker request — main thread asks the worker to pre-parse a cell.
/// Carries everything the worker needs to extract NIF bytes from BSA
/// and run the pool-free portion of the import pipeline.
pub struct LoadCellRequest {
    pub gx: i32,
    pub gy: i32,
    /// Generation counter snapshot at request time. The drain step
    /// compares against the current generation for `(gx, gy)` and drops
    /// stale payloads — the player may have moved out of range and back
    /// while the worker was busy.
    pub generation: u64,
    /// Monotonic enqueue stamp used to separate worker queueing from actual
    /// extract/parse service time in the boundary benchmark.
    pub queued_at: Instant,
    pub wctx: Arc<ExteriorWorldContext>,
    pub tex_provider: Arc<TextureProvider>,
    /// Snapshot of `NifImportRegistry`'s cached keys at request-build
    /// time. The worker skips BSA-extract + parse for any model path
    /// already in this set — main-thread cache will spawn it through
    /// [`crate::cell_loader::load_one_exterior_cell`] without needing
    /// the worker to re-produce the import. See #862. Includes
    /// negative-cache entries so known-failed parses aren't re-tried.
    /// May lag the registry by a few ms (more cache entries can land
    /// between snapshot and worker dispatch); that's harmless — at
    /// worst the worker over-extracts, never under-skips.
    pub cached_keys: Arc<std::collections::HashSet<String>>,
}

/// Worker output — pre-parsed scenes for every NIF the cell references.
/// `parsed` keys are lowercased model paths (matching the
/// `NifImportRegistry` key shape). The main-thread drain step finishes
/// the import (string interning + BGSM merge) and inserts into the
/// process-lifetime cache before calling `load_one_exterior_cell`.
pub struct LoadCellPayload {
    pub gx: i32,
    pub gy: i32,
    pub generation: u64,
    pub timings: StreamingWorkerTimings,
    /// `Some(scene)` = parsed cleanly. `None` = extraction or parse
    /// failed; the entry is still emitted so the cache records the
    /// negative result and a future placement of the same model
    /// doesn't re-attempt the parse.
    pub parsed: HashMap<String, Option<PartialNifImport>>,
}

/// Main-thread continuation for one worker payload.
///
/// Only one is active at a time. Keeping the original generation in the job
/// lets boundary-crossing cancellation reuse the same `pending` generation
/// gate as payload arrival.
pub(crate) struct StreamingCellApplyJob {
    pub(crate) coord: (i32, i32),
    pub(crate) generation: u64,
    pub(crate) phase: StreamingCellApplyPhase,
}

pub(crate) enum StreamingCellApplyPhase {
    /// Finish pool/material-dependent import work one NIF at a time.
    FinishImports(std::collections::hash_map::IntoIter<String, Option<PartialNifImport>>),
    /// All worker imports are resident; the exterior cell has not begun.
    BeginExterior,
    /// Terrain/root are resident and the placed-reference walk is resumable.
    Spawn(crate::cell_loader::ExteriorCellApplyJob),
}

impl StreamingCellApplyJob {
    pub(crate) fn from_payload(payload: LoadCellPayload) -> Self {
        Self {
            coord: (payload.gx, payload.gy),
            generation: payload.generation,
            phase: StreamingCellApplyPhase::FinishImports(payload.parsed.into_iter()),
        }
    }
}

/// Pool-free portion of NIF import — everything the worker can do
/// off-thread. The main-thread drain step takes a `PartialNifImport`,
/// runs `import_nif_with_collision` (string interning, needs the
/// world's `StringPool`) and `merge_external_material` (needs the
/// `MaterialProvider`), and assembles the full
/// `cell_loader::CachedNifImport`.
pub struct PartialNifImport {
    /// Parsed scene — needed by the main-thread import step
    /// (`import_nif_with_collision` walks this).
    pub scene: byroredux_nif::scene::NifScene,
    /// BSXFlags bit-set extracted from the scene root. The drain step
    /// honours the `0x20` bit, game-era gated via [`Self::bsver`] — see
    /// that field's doc comment (#2046 / TD2-103).
    pub bsx: u32,
    /// `NifHeader.user_version_2` (BSVER) — needed alongside `bsx` to
    /// interpret BSXFlags bit 5 correctly: it means `EditorMarker` on
    /// Oblivion/FO3/FNV but was re-purposed to `MultiBoundNode` on
    /// Skyrim+/FO4/FO76/Starfield (see `references::import::parse_and_import_nif`,
    /// fix commit `6feac029`). Without this field the drain step can't
    /// apply the same game-era gate and silently drops legitimate
    /// architecture NIFs with bit 5 set on those games.
    pub bsver: u32,
    /// Root NiNode `NiAVObject.flags` (SELECTIVE_UPDATE / DISABLE_SORTING
    /// / DISPLAY_OBJECT / IS_NODE / …) for placement-root SceneFlags
    /// parity with the loose-NIF loader. See #1235 / LC-D1-NEW-01.
    pub root_flags: u32,
    /// Lights — pool-free import path.
    pub lights: Vec<byroredux_nif::import::ImportedLight>,
    /// Particle emitters — pool-free import path.
    pub particle_emitters: Vec<byroredux_nif::import::ImportedParticleEmitterFlat>,
    /// Embedded animation clip — pool-free import path.
    pub embedded_clip: Option<byroredux_nif::anim::AnimationClip>,
}

// #1171 / CONC-D6-NEW-05 — compile-time guarantee that
// `PartialNifImport: Send`. The cell-stream worker emits these across
// `mpsc::Sender<LoadCellPayload>`, which requires `Send`. If a future
// contributor adds a non-`Send` field to `NifScene` (e.g. an `Rc<…>`
// for some compositional reason) or to any nested type, this fires at
// the struct's declaration site rather than at the distant channel-
// send call deep inside `cell_pre_parse_worker`.
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<PartialNifImport>();
};

/// World-streaming state. Owned by `App` (not an ECS resource — needs
/// to coexist on the same struct as `VulkanContext` and the texture /
/// material providers, all of which the streaming driver borrows
/// mutably each frame).
pub struct WorldStreamingState {
    /// Once-per-session parsed plugin snapshot + chosen worldspace +
    /// resolved climate / default weather. Cheap to clone the `Arc`
    /// into the worker thread per request.
    pub wctx: Arc<ExteriorWorldContext>,
    /// Long-lived texture archive provider (BSA / BA2 readers). Behind
    /// `Arc` so the worker thread can extract NIF bytes off-thread —
    /// `BsaArchive` / `Ba2Archive` already serialise their inner `File`
    /// access via `Mutex`, so concurrent extracts are safe.
    pub tex_provider: Arc<TextureProvider>,
    /// Long-lived BGSM material provider. Stays main-thread only —
    /// `merge_external_material` needs `&mut MaterialProvider` (writes to
    /// `bgsm_cache` / `bgem_cache` / `failed_paths`), and serialising
    /// every drain-step BGSM resolve through a Mutex would put the
    /// main thread on the slow path. Worker doesn't touch BGSM.
    pub mat_provider: MaterialProvider,
    /// Currently-loaded cells.
    pub loaded: HashMap<(i32, i32), LoadedCell>,
    /// Root owning the active worldspace's persistent CELL. Unlike `loaded`,
    /// this is not keyed by a grid coordinate and never participates in
    /// radius eviction; it is reclaimed only when the worldspace drains.
    pub persistent_root: Option<EntityId>,
    /// Resumable persistent-CELL spawn. It shares the main-thread apply
    /// deadline with ordinary exterior tiles and is cleared on completion.
    pub(crate) persistent_apply: Option<crate::cell_loader::PersistentCellApplyJob>,
    /// Distant-terrain LOD blocks, keyed by block-coord (#1373). Streamed
    /// each cell-boundary crossing alongside the full-detail cells: blocks
    /// entering the LOD radius spawn, blocks leaving unload, and boundary
    /// blocks whose hole mask changed regenerate. The Slice-1 ring spawned
    /// these once and never tracked them — re-entry leaked ~600 blocks and
    /// the hole-out went stale as the player walked.
    pub lod_blocks: HashMap<(i32, i32, i32), LodBlock>,
    /// Terrain-LOD coordinates that were reconciled but produced no mesh,
    /// keyed to the hole mask that was attempted. This is the terrain
    /// equivalent of the empty sentinels stored directly in the object and
    /// placement maps: incremental initialization must not retry the same
    /// absent asset every frame and starve all later coordinates.
    pub lod_missing_blocks: HashMap<(i32, i32, i32), u16>,
    /// Distant **object** LOD quads, keyed by `(level, qx, qy)` — the quad's
    /// LOD band plus its SW-corner cell (EXAL step 6). Skyrim+/FO4 only —
    /// each entry is the baked `.bto` macro-mesh's spawned sub-meshes (or an
    /// empty sentinel for a quad with no baked LOD). Reconciled progressively
    /// alongside `lod_blocks`; quads load only outside the full-detail ring.
    ///
    /// The `level` is part of the key because the same ground is covered by a
    /// different quad in every band (#2371): a band switch is an unload of
    /// the old `(level, …)` entry plus a load of the new one, which is what
    /// keeps two levels from ever double-drawing it.
    pub object_lod_blocks: HashMap<(i32, i32, i32), crate::cell_loader::ObjectLodBlock>,
    /// Distant **object** LOD cells for Oblivion's placement scheme, keyed
    /// by cell `(x, y)`. Each entry is the cell's
    /// `DistantLOD\*.lod` instanced `_far.nif` meshes (or an empty sentinel
    /// for a cell with no `.lod`). Streamed alongside `object_lod_blocks`;
    /// only one of the two ever populates per game (the gate is by
    /// `GameKind`). Cells load only outside the full-detail ring.
    pub placement_lod_blocks: HashMap<(i32, i32), crate::cell_loader::PlacementLodBlock>,
    /// Distant worldspace-wide LOD water quad (`NAM3`/`NAM4`, #2449 /
    /// EXAL-01). `None` when the worldspace authors no LOD water, or the
    /// mesh upload failed at spawn. Set once at worldspace entry (see
    /// [`LodWaterPlane`]'s doc for why this isn't reconciled per-block like
    /// `lod_blocks`), reclaimed once in `drain_streaming_state`.
    pub lod_water: Option<crate::streaming::LodWaterPlane>,
    /// Cells whose load request is in flight on the worker. Maps
    /// `(gx, gy)` to the generation of the outstanding request.
    /// Drain compares the payload's generation against this map's
    /// entry — mismatch ⇒ payload is stale, drop it.
    pub pending: HashMap<(i32, i32), u64>,
    /// Generation counter — bumped per request so a "load → unload →
    /// reload" sequence on the same `(gx, gy)` cell can distinguish
    /// the outstanding payload from the new one. Drains never apply
    /// payloads whose generation doesn't match `pending[(gx, gy)]`.
    pub next_generation: u64,
    /// Load radius — cells within this Chebyshev distance of the player
    /// are loaded. `1` = 3×3 grid, `2` = 5×5, `3` = 7×7.
    pub radius_load: i32,
    /// Unload radius — cells outside this Chebyshev distance are
    /// unloaded. Must be `>= radius_load + 1` to avoid load-unload
    /// thrash at the boundary.
    pub radius_unload: i32,
    /// Last (gx, gy) the player was in. Used by the App driver to
    /// suppress no-op streaming work when the player hasn't crossed a
    /// cell boundary.
    pub last_player_grid: Option<(i32, i32)>,
    /// Whether the three distant-LOD rings still have deferred reconcile
    /// work. Foreground-first bootstrap and cell-boundary movement set this;
    /// idle frames clear it progressively through the shared LOD budget.
    pub lod_reconcile_pending: bool,
    /// Boundary-to-ready and apply-slice aggregates. Read by the benchmark
    /// summary; otherwise passive, bounded runtime diagnostics.
    pub telemetry: StreamingTelemetry,
    /// Worker thread handle. Held so the thread isn't detached. On
    /// graceful shutdown [`WorldStreamingState::shutdown`] drops
    /// `request_tx` (so the worker's recv loop exits) and joins this
    /// handle with a bounded timeout (#856). Kept inside `Option` so
    /// `shutdown` can move the handle out of `self` by destructure
    /// without `JoinHandle: Default`. The [`Drop`] impl on
    /// `WorldStreamingState` (#1167) mirrors that shutdown handshake
    /// for any exit path that bypasses the explicit call.
    pub worker: Option<JoinHandle<()>>,
    /// mpsc channel sending requests to the worker. Wrapped in
    /// `Option` so [`Drop`] (#1167) can `take()` it and drop the
    /// sender BEFORE the worker `JoinHandle` is dropped — Rust's
    /// declaration-order field-drop would otherwise drop the worker
    /// (= detach) before the channel close, defeating the join. Send
    /// sites go through [`WorldStreamingState::send_request`].
    pub request_tx: Option<mpsc::Sender<LoadCellRequest>>,
    /// mpsc receiver for completed payloads. Drained each frame by the
    /// App driver; non-blocking via `try_recv`.
    pub payload_rx: mpsc::Receiver<LoadCellPayload>,
    /// Current resumable main-thread apply. The matching `pending` entry stays
    /// live until this completes, suppressing duplicate requests and making a
    /// boundary-crossing removal an immediate cancellation signal.
    pub(crate) active_apply: Option<StreamingCellApplyJob>,
}

impl WorldStreamingState {
    /// New scene geometry is appended throughout a resumable cell/LOD load.
    /// Rebuilding the whole global SSBO after every atomic REFR would turn a
    /// large FO4 crossing into dozens of 600–900 MiB copies. The frame driver
    /// defers that rebuild while this returns true and masks appended ranges
    /// from raster/TLAS through `MeshRegistry::is_geometry_resident`.
    pub fn geometry_batch_in_progress(&self) -> bool {
        !self.pending.is_empty()
            || self.active_apply.is_some()
            || self.persistent_apply.is_some()
            || self.lod_reconcile_pending
    }

    /// Construct from an already-resolved [`ExteriorWorldContext`] and
    /// the long-lived providers. Spawns the cell-pre-parse worker
    /// thread; first request can be sent immediately.
    pub fn new(
        wctx: ExteriorWorldContext,
        tex_provider: TextureProvider,
        mat_provider: MaterialProvider,
        radius_load: i32,
    ) -> Self {
        // Hysteresis: unload at load + 1. Pre-fix any value would
        // accept; clamping here means a future caller passing
        // `radius_unload = radius_load` doesn't cause boundary thrash.
        let radius_load = radius_load.max(0);
        let (request_tx, request_rx) = mpsc::channel::<LoadCellRequest>();
        let (payload_tx, payload_rx) = mpsc::channel::<LoadCellPayload>();
        let worker = std::thread::Builder::new()
            .name("byro-cell-stream".into())
            .spawn(move || cell_pre_parse_worker(request_rx, payload_tx))
            .expect("failed to spawn cell-stream worker thread");
        Self {
            wctx: Arc::new(wctx),
            tex_provider: Arc::new(tex_provider),
            mat_provider,
            loaded: HashMap::new(),
            persistent_root: None,
            persistent_apply: None,
            lod_blocks: HashMap::new(),
            lod_missing_blocks: HashMap::new(),
            object_lod_blocks: HashMap::new(),
            placement_lod_blocks: HashMap::new(),
            lod_water: None,
            pending: HashMap::new(),
            next_generation: 0,
            radius_load,
            radius_unload: radius_load + 1,
            last_player_grid: None,
            lod_reconcile_pending: false,
            telemetry: StreamingTelemetry::default(),
            worker: Some(worker),
            request_tx: Some(request_tx),
            payload_rx,
            active_apply: None,
        }
    }

    /// Spawn the worldspace-wide distant LOD water quad for this streaming
    /// state's worldspace, if it authors one (`NAM3`/`NAM4`) and a player
    /// grid position has already been set (#2449 / EXAL-01). Leaves
    /// `lod_water` untouched (stays `None`) if either precondition isn't
    /// met, so callers can invoke this unconditionally right after
    /// `last_player_grid` is set, mirroring how `apply_worldspace_weather`
    /// is called unconditionally at every worldspace-entry call site.
    pub fn spawn_lod_water(&mut self, world: &mut World, ctx: &mut VulkanContext) {
        let Some(player_grid) = self.last_player_grid else {
            return;
        };
        let (Some(height), lod_water_form) = crate::env_translate::translate_lod_water(
            &self.wctx.record_index.cells.worldspaces,
            &self.wctx.worldspace_key,
        ) else {
            return;
        };
        self.lod_water = crate::cell_loader::spawn_lod_water_plane(
            world,
            ctx,
            &self.tex_provider,
            &self.wctx.record_index.waters,
            height,
            lod_water_form,
            player_grid,
            self.radius_unload,
            self.wctx.record_index.game,
        );
    }

    /// Send a load request to the worker. Returns `Err` if the worker
    /// channel has already been closed (Drop / shutdown). Hides the
    /// `Option<Sender>` field shape introduced for the #1167 Drop fix.
    pub fn send_request(
        &self,
        req: LoadCellRequest,
    ) -> Result<(), mpsc::SendError<LoadCellRequest>> {
        match self.request_tx.as_ref() {
            Some(tx) => tx.send(req),
            None => Err(mpsc::SendError(req)),
        }
    }

    /// Queue cells through the canonical exterior worker path.
    ///
    /// Generation allocation, duplicate suppression, pending bookkeeping,
    /// request construction, and closed-channel rollback live here so the
    /// initial bootstrap and steady-state boundary crossing cannot drift.
    /// Returns the number of requests successfully queued.
    pub fn queue_loads(
        &mut self,
        coords: impl IntoIterator<Item = (i32, i32)>,
        cached_keys: Arc<HashSet<String>>,
    ) -> usize {
        let mut queued = 0usize;
        for (gx, gy) in coords {
            let coord = (gx, gy);
            if self.loaded.contains_key(&coord) || self.pending.contains_key(&coord) {
                continue;
            }

            let generation = self.next_generation;
            self.next_generation = self.next_generation.wrapping_add(1);
            self.pending.insert(coord, generation);
            let req = LoadCellRequest {
                gx,
                gy,
                generation,
                queued_at: Instant::now(),
                wctx: self.wctx.clone(),
                tex_provider: self.tex_provider.clone(),
                cached_keys: cached_keys.clone(),
            };
            if self.send_request(req).is_err() {
                log::error!("Streaming worker channel closed; cell ({gx},{gy}) cannot be loaded");
                self.pending.remove(&coord);
            } else {
                queued += 1;
            }
        }
        queued
    }

    /// Graceful shutdown — close the request channel so the worker's
    /// recv loop exits, then join the worker with a bounded timeout.
    /// On timeout the worker is detached (matches the pre-#856
    /// unconditional-detach behaviour as a fallback). Replaces the
    /// previous `self.streaming.take()` pattern at the
    /// `WindowEvent::CloseRequested` handler in `main.rs`.
    ///
    /// The bound is necessary because the worker may be mid-
    /// `BsaArchive::extract()` (~100–300 ms typical, much longer on
    /// network filesystems or contended spinning disks); a slow
    /// extract should not block process teardown indefinitely. See
    /// AUDIT_CONCURRENCY_2026-05-05.md / C6-NEW-03.
    ///
    /// Takes `&mut self` (#1167) — the [`Drop`] safety-net calls into
    /// this same method, so both paths share one implementation. After
    /// `shutdown` returns, subsequent calls (including the eventual
    /// `Drop`) observe `worker: None` and short-circuit, so the join
    /// runs exactly once.
    pub fn shutdown(&mut self, timeout: std::time::Duration) {
        // Take the worker handle so the eventual `Drop` skips the
        // detaching path; the join below is the only place we wait on
        // this thread.
        let Some(handle) = self.worker.take() else {
            return;
        };
        // Close the request channel BEFORE the join. The worker's
        // `request_rx.recv()` returns Err on its next loop iteration
        // and the thread exits. The matching `payload_rx` will be
        // dropped automatically when `self` is dropped — if the worker
        // is currently inside `payload_tx.send(payload)` it observes
        // the closed receiver and bails via the existing post-#854
        // break path.
        let _ = self.request_tx.take();
        match join_with_timeout(handle, timeout) {
            Ok(()) => log::info!("cell-stream worker joined cleanly on shutdown"),
            Err(JoinTimeout) => log::warn!(
                "cell-stream worker did not exit within {:?} — detaching (#856). \
                 The worker thread will exit shortly after `request_tx` drop, but the \
                 process teardown won't block on it.",
                timeout
            ),
        }
    }
}

/// Safety-net teardown for every exit path that doesn't go through the
/// explicit [`WorldStreamingState::shutdown`] handshake (e.g. the
/// `--bench-frames` natural exit at `main.rs` and the panic / error
/// exits that call `event_loop.exit()` without first taking the
/// streaming state out of `App`). See #1167 / CONC-D6-NEW-01.
///
/// Delegates to `shutdown` with a fixed 1 s timeout. If `shutdown` was
/// already called explicitly, the take()'s inside it have set
/// `worker = None` / `request_tx = None`, so this re-entry observes
/// the short-circuit and is a no-op — the join runs exactly once.
impl Drop for WorldStreamingState {
    fn drop(&mut self) {
        self.shutdown(std::time::Duration::from_secs(1));
    }
}

/// Sentinel returned by [`join_with_timeout`] when the joined thread
/// outlives the timeout. Body is unit since the caller doesn't need
/// to recover any state from the thread — its purpose is to signal
/// "detach, log, move on."
#[derive(Debug, PartialEq, Eq)]
pub struct JoinTimeout;

/// `JoinHandle::join` with a wall-clock timeout. Poll-based on
/// [`std::thread::JoinHandle::is_finished`] (stabilised in Rust
/// 1.61) — no auxiliary watcher thread, no `Arc`-held-resource leak
/// on the timeout path. The previous `mpsc::channel` + watcher-
/// thread pattern (#1169) leaked one watcher thread per timeout,
/// each holding the joined `JoinHandle` indefinitely; reaped by the
/// OS at process exit but a real leak on any future non-terminal
/// caller.
///
/// On `Ok(())`, the joined thread has terminated and `join()` has
/// been called (consumes the handle). On `Err(JoinTimeout)`, the
/// handle has been dropped — equivalent to detaching the thread,
/// matching the contract of the old API.
///
/// Poll cadence: 10 ms. With a 1 s timeout (the production caller)
/// that's ≤100 wakeups during shutdown — negligible CPU, and the
/// fast path (worker exits within the first poll) is one extra
/// `is_finished` check vs. an unconditional join.
///
/// Unit-testable without a full streaming setup — see the
/// `join_with_timeout_*` tests below.
pub fn join_with_timeout(
    handle: JoinHandle<()>,
    timeout: std::time::Duration,
) -> Result<(), JoinTimeout> {
    const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(10);
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if handle.is_finished() {
            // Swallow a panic in the joined thread — the caller's
            // contract is "thread is done," not "thread succeeded."
            // Panics in worker threads are already surfaced by the
            // worker itself (see `pre_parse_cell_panic_safe`).
            let _ = handle.join();
            return Ok(());
        }
        let now = std::time::Instant::now();
        if now >= deadline {
            // Drop the handle here — detaches the thread, which will
            // exit naturally once its current unit completes. Matches
            // the prior contract: caller can move on.
            drop(handle);
            return Err(JoinTimeout);
        }
        // Sleep up to POLL_INTERVAL but never past the deadline so a
        // short remaining window doesn't overshoot.
        let remaining = deadline.saturating_duration_since(now);
        std::thread::sleep(POLL_INTERVAL.min(remaining));
    }
}

/// Cell pre-parse worker loop. Pulls requests off the channel, does
/// the off-thread work for every NIF the cell references, and emits a
/// single `LoadCellPayload` per request.
///
/// Exits when `request_rx` returns `Err` (sender dropped on
/// `WorldStreamingState` shutdown). Panics inside `pre_parse_cell`
/// are caught and converted into an empty payload — without this
/// guard a single parser-level panic would tear down the worker
/// thread, drop `request_rx`, and silently disable exterior streaming
/// for the rest of the session (#854).
fn cell_pre_parse_worker(
    request_rx: mpsc::Receiver<LoadCellRequest>,
    payload_tx: mpsc::Sender<LoadCellPayload>,
) {
    log::info!("cell-stream worker thread started");
    while let Ok(req) = request_rx.recv() {
        let LoadCellRequest {
            gx,
            gy,
            generation,
            queued_at,
            wctx,
            tex_provider,
            cached_keys,
        } = req;
        let worker_started = Instant::now();
        let mut payload = pre_parse_cell_panic_safe(gx, gy, generation, || {
            pre_parse_cell(gx, gy, generation, &wctx, &tex_provider, &cached_keys)
        });
        payload.timings = StreamingWorkerTimings {
            queue_wait: worker_started.saturating_duration_since(queued_at),
            worker: worker_started.elapsed(),
        };
        if payload_tx.send(payload).is_err() {
            // Receiver dropped — main thread is shutting down; exit cleanly.
            break;
        }
    }
    log::info!("cell-stream worker thread exiting");
}

/// Run `f` (the cell pre-parse) inside a panic guard. If `f` panics,
/// log and return an empty payload tagged with the request's
/// coordinates and generation. The drain step still observes the
/// (empty) payload, clears the pending entry, and the streaming loop
/// stays live for the next cell crossing — unlike the pre-#854
/// behaviour where the worker thread died and every subsequent send
/// failed.
fn pre_parse_cell_panic_safe<F>(gx: i32, gy: i32, generation: u64, f: F) -> LoadCellPayload
where
    F: FnOnce() -> LoadCellPayload,
{
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|_| {
        log::error!(
            "[stream-worker] panic in pre_parse_cell({}, {}) gen={} — recovered with empty payload (#854)",
            gx,
            gy,
            generation
        );
        LoadCellPayload {
            gx,
            gy,
            generation,
            timings: StreamingWorkerTimings::default(),
            parsed: HashMap::new(),
        }
    })
}

/// Per-cell pre-parse: walk references, resolve unique model paths,
/// extract NIF bytes from the texture provider's mesh archives, and
/// run the pool-free portion of the NIF import pipeline.
///
/// `cached_keys` is the main-thread snapshot of
/// [`crate::cell_loader::NifImportRegistry`] at request-build time;
/// any model path it contains is skipped here — the drain step's
/// `load_one_exterior_cell` will spawn the cell's REFRs against the
/// cached entries directly, no re-parse needed. See #862.
///
/// Returns a populated [`LoadCellPayload`] (which may have an empty
/// Parse + import a single (path, Option<bytes>) pair. Shared between
/// the serial and parallel branches of `pre_parse_cell` so both paths
/// stay byte-identical — no logic drift between code paths.
///
/// Per-NIF panic guard — converts a parser-level panic into the same
/// `None` failure marker used by the regular `Err` path. Without this,
/// a panic would propagate through rayon's `collect()` and tear down
/// the worker thread (#854). Preserved verbatim across the #877
/// refactor; extracted in #1262 (NIF-D5-NEW-02) to avoid duplicating
/// the closure between the serial / parallel branches.
fn parse_one_nif((path, bytes): (String, Option<Vec<u8>>)) -> (String, Option<PartialNifImport>) {
    let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(bytes) = bytes else {
            log::debug!("[stream-worker] NIF not in BSA: '{}'", path);
            return None;
        };
        let scene = match byroredux_nif::parse_nif(&bytes) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("[stream-worker] NIF parse failed '{}': {}", path, e);
                return None;
            }
        };
        let bsx = byroredux_nif::import::extract_bsx_flags(&scene);
        // `NifScene` already retains BSVER as `scene.bsver` (set from the
        // header during the `parse_nif` call above) — no need to re-parse
        // the header. #2111.
        let bsver = scene.bsver;
        let root_flags = byroredux_nif::import::extract_root_flags(&scene);
        let lights = byroredux_nif::import::import_nif_lights(&scene);
        let particle_emitters = byroredux_nif::import::import_nif_particle_emitters(&scene);
        let embedded_clip = byroredux_nif::anim::import_embedded_animations(&scene);
        Some(PartialNifImport {
            scene,
            bsx,
            bsver,
            root_flags,
            lights,
            particle_emitters,
            embedded_clip,
        })
    }))
    .unwrap_or_else(|_| {
        log::error!(
            "[stream-worker] panic parsing NIF '{}' — recording None (#854)",
            path
        );
        None
    });
    (path, parsed)
}

/// `parsed` map if the cell doesn't exist, has no references, or
/// every model path was already cached — the main-thread drain still
/// applies the empty payload so the pending entry is cleared).
#[tracing::instrument(
    name = "pre_parse_cell",
    skip_all,
    fields(gx = gx, gy = gy, generation = generation, cached_count = cached_keys.len()),
)]
fn pre_parse_cell(
    gx: i32,
    gy: i32,
    generation: u64,
    wctx: &ExteriorWorldContext,
    tex_provider: &TextureProvider,
    cached_keys: &HashSet<String>,
) -> LoadCellPayload {
    let mut parsed: HashMap<String, Option<PartialNifImport>> = HashMap::new();
    let cells_map = match wctx
        .record_index
        .cells
        .exterior_cells
        .get(&wctx.worldspace_key)
    {
        Some(m) => m,
        None => {
            return LoadCellPayload {
                gx,
                gy,
                generation,
                timings: StreamingWorkerTimings::default(),
                parsed,
            }
        }
    };
    let Some(cell) = cells_map.get(&(gx, gy)) else {
        return LoadCellPayload {
            gx,
            gy,
            generation,
            timings: StreamingWorkerTimings::default(),
            parsed,
        };
    };

    // Unique lowercased model paths in this cell. Reuse across
    // duplicate placements — chairs, lanterns, rocks all share one
    // model path each. Filter out paths already in the main-thread
    // cache snapshot — the drain's `load_one_exterior_cell` spawns
    // them directly from cache without needing the worker to
    // re-produce the import (#862). 7×7 grid traversal in WastelandNV
    // typically sees ~95% cache hits on shared statics, so this slash
    // is dominant for the steady-state workload.
    let mut model_paths: HashSet<String> = HashSet::new();
    let mut skipped_cached = 0usize;
    for refr in &cell.references {
        let Some(model_path) = wctx
            .record_index
            .cells
            .statics
            .get(&refr.base_form_id)
            .map(|s| s.model_path.as_str())
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let key = model_path.to_ascii_lowercase();
        if cached_keys.contains(&key) {
            skipped_cached += 1;
            continue;
        }
        model_paths.insert(key);
    }
    if skipped_cached > 0 {
        log::debug!(
            "[stream-worker] cell ({},{}): {} cached models skipped, {} unique to parse",
            gx,
            gy,
            skipped_cached,
            model_paths.len(),
        );
    }

    // Two-phase pre-parse (#877 / NIF-PERF-13):
    //   Phase 1 — SERIAL BSA extract on one thread. The BSA / BA2
    //     readers wrap `File` in `Mutex<File>` (`bsa/archive.rs:119`,
    //     `bsa/ba2.rs:78`), so concurrent `extract_mesh` calls would
    //     queue on the mutex and pay both the lock-acquire overhead
    //     and a context switch per worker — the worst case shape for
    //     a short-blob hot path. Doing the I/O serially on one thread
    //     pays zero lock contention.
    //   Phase 2 — PARALLEL parse + import on the `(path, bytes)` pairs.
    //     The CPU-bound parse / import work fans out cleanly across
    //     rayon workers without any shared-mutex bottleneck.
    //
    // Pre-#877 the entire pipeline ran inside the rayon closure,
    // including the BSA mutex acquire — workers spent most of their
    // wall-clock queued on the mutex on small-NIF-heavy interior
    // cells. Original #830 / NIF-PERF-06 closeout already shipped the
    // ~6-7× single-core → multi-core speedup; this lift on top is the
    // remaining ~10-20% the mutex was eating.
    //
    // Errors are recorded as `None` entries so the drain step caches
    // the negative result and downstream placements skip silently.
    let model_paths: Vec<String> = model_paths.into_iter().collect();

    // Phase 1: serial extract. One BSA mutex acquire per NIF, no
    // contention. `None` for paths the BSA doesn't carry (skipped
    // silently — same semantics as the pre-#877 inline check).
    let extracted: Vec<(String, Option<Vec<u8>>)> = model_paths
        .into_iter()
        .map(|p| {
            let bytes = tex_provider.extract_mesh(&p);
            (p, bytes)
        })
        .collect();

    // Phase 2: parse + import. Each worker owns its `Vec<u8>` for the
    // whole closure — no shared mutex on the hot path.
    //
    // #1262 / NIF-D5-NEW-02 — rayon's worker-wake + join overhead
    // (~50-200 µs typical) dominates at small N. Post-#862 the NIF
    // import cache absorbs most cell-load work and the typical fresh-
    // parse count is 0-6 per cell (the Riverwood log confirms "6 new
    // unique meshes parsed, NIF cache hits/misses 156/6 this cell").
    // Drop to serial iteration below the threshold; keep rayon for
    // session-start fresh-cell bursts where N is genuinely large.
    //
    // Threshold: 8. Empirically chosen against the steady-state
    // streaming pattern — at N≤7 the parallel dispatch is net-loss
    // or break-even; N≥8 the parallel speedup outpaces wake-overhead.
    const PRE_PARSE_RAYON_MIN: usize = 8;
    let results: Vec<(String, Option<PartialNifImport>)> = if extracted.len() < PRE_PARSE_RAYON_MIN
    {
        extracted.into_iter().map(parse_one_nif).collect()
    } else {
        extracted.into_par_iter().map(parse_one_nif).collect()
    };
    parsed.extend(results);

    LoadCellPayload {
        gx,
        gy,
        generation,
        timings: StreamingWorkerTimings::default(),
        parsed,
    }
}

/// Diff result computed by [`compute_streaming_deltas`]. Pure
/// data — no Vulkan, no World access — so it's testable in isolation
/// of the engine's runtime.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct StreamingDeltas {
    /// Cells inside the load radius that aren't yet loaded. Sorted so
    /// the App driver loads cells in a deterministic order (closer to
    /// the player first, ties broken on (gx, gy) lexically). The
    /// closest-first ordering means the visible cell-of-arrival is
    /// always loaded before peripheral cells.
    pub to_load: Vec<(i32, i32)>,
    /// Cells outside the unload radius that are currently loaded. No
    /// inherent ordering required (the App driver unloads each via
    /// `unload_cell` independently). Sorted by (gx, gy) for
    /// deterministic output so the regression tests are stable.
    pub to_unload: Vec<(i32, i32)>,
}

/// Compute streaming deltas — which cells to load, which to unload —
/// given the player's current grid coords, the currently-loaded set,
/// and the load / unload radii.
///
/// Pure function with no I/O. The App driver consumes the deltas and
/// dispatches to the cell loader.
pub fn compute_streaming_deltas(
    loaded: &HashMap<(i32, i32), LoadedCell>,
    player_grid: (i32, i32),
    radius_load: i32,
    radius_unload: i32,
) -> StreamingDeltas {
    debug_assert!(
        radius_unload >= radius_load,
        "radius_unload ({radius_unload}) < radius_load ({radius_load}) — boundary thrash"
    );

    let (px, py) = player_grid;

    // Desired set: every cell inside the load radius (Chebyshev).
    let mut desired: HashSet<(i32, i32)> = HashSet::new();
    for dx in -radius_load..=radius_load {
        for dy in -radius_load..=radius_load {
            desired.insert((px + dx, py + dy));
        }
    }

    // Cells to load: in `desired`, not in `loaded`.
    let mut to_load: Vec<(i32, i32)> = desired
        .iter()
        .copied()
        .filter(|coord| !loaded.contains_key(coord))
        .collect();
    // Closest-first ordering by Chebyshev distance, ties on (gx, gy).
    to_load.sort_by_key(|(gx, gy)| {
        let d = (gx - px).abs().max((gy - py).abs());
        (d, *gx, *gy)
    });

    // Cells to unload: in `loaded`, outside the unload radius.
    let mut to_unload: Vec<(i32, i32)> = loaded
        .keys()
        .copied()
        .filter(|(gx, gy)| {
            let d = (gx - px).abs().max((gy - py).abs());
            d > radius_unload
        })
        .collect();
    to_unload.sort();

    StreamingDeltas { to_load, to_unload }
}

/// Cells with an in-flight worker request that have left the unload
/// radius around the player's current grid position — #2113 / D7-01.
///
/// `compute_streaming_deltas` only diffs `loaded` against the desired
/// set, so a cell dispatched to the worker but not yet spawned (still
/// only in `pending`) is invisible to it: if the player leaves before
/// the request completes, the payload would otherwise still classify
/// as [`PayloadDecision::Apply`] and pay a full main-thread spawn just
/// before the next boundary crossing unloads it again. The caller
/// removes each returned coord from `pending`, so `classify_payload`
/// sees no entry for it and returns `StaleNoPending` — the payload is
/// discarded before spawn.
pub fn stale_pending_coords(
    pending: &HashMap<(i32, i32), u64>,
    player_grid: (i32, i32),
    radius_unload: i32,
) -> Vec<(i32, i32)> {
    let (px, py) = player_grid;
    let mut stale: Vec<(i32, i32)> = pending
        .keys()
        .copied()
        .filter(|(gx, gy)| (gx - px).abs().max((gy - py).abs()) > radius_unload)
        .collect();
    stale.sort();
    stale
}

/// Convert a Y-up world-space translation into Bethesda exterior grid
/// coords. 4096 units per cell. The engine's Z-up→Y-up flip negates
/// the source-Y axis when populating world Z, so an exterior placed at
/// source `(2048, 2048, 0)` lands at world `(2048, 0, -2048)` and
/// resolves to grid `(0, 0)`.
pub fn world_pos_to_grid(world_x: f32, world_z: f32) -> (i32, i32) {
    let gx = (world_x / EXTERIOR_CELL_UNITS).floor() as i32;
    let gy = (-world_z / EXTERIOR_CELL_UNITS).floor() as i32;
    (gx, gy)
}

/// Generation-counter decision for an incoming worker payload.
///
/// The shared drain step in `streaming_helpers::consume_streaming_payload`
/// compares the payload's generation
/// against `WorldStreamingState.pending[(gx, gy)]`. A mismatch means
/// either:
///   * The cell was unloaded since the request was sent — `pending`
///     has no entry for the coord (`StaleNoPending`).
///   * The cell was unloaded and re-requested at a higher generation
///     — `pending` holds the new generation, payload's is older
///     (`StaleNewerPending`).
///
/// Both cases result in the payload being dropped without spawning;
/// the worker's pre-parse work is wasted but the world stays
/// consistent. This pure helper makes that invariant testable
/// without standing up the worker thread.
#[derive(Debug, PartialEq, Eq)]
pub enum PayloadDecision {
    /// Apply the payload — it matches the pending request for the
    /// cell.
    Apply,
    /// Drop — no pending entry for `(gx, gy)`. Cell was unloaded
    /// (or never loaded) while the payload was in flight.
    StaleNoPending,
    /// Drop — pending entry exists but at a different generation.
    /// Cell was unloaded and re-requested while the older payload was
    /// in flight.
    StaleNewerPending {
        pending_generation: u64,
        payload_generation: u64,
    },
}

/// Classify an incoming worker payload against the streaming state's
/// pending map. Returns the action the caller should take.
pub fn classify_payload(
    pending: &HashMap<(i32, i32), u64>,
    coord: (i32, i32),
    payload_generation: u64,
) -> PayloadDecision {
    match pending.get(&coord) {
        Some(&g) if g == payload_generation => PayloadDecision::Apply,
        Some(&pending_generation) => PayloadDecision::StaleNewerPending {
            pending_generation,
            payload_generation,
        },
        None => PayloadDecision::StaleNoPending,
    }
}

#[cfg(test)]
#[path = "streaming_tests.rs"]
mod tests;
