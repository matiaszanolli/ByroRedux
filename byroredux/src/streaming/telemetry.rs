//! #5092 — streaming telemetry: latency summaries, boundary tracking,
//! phase distributions and worker timings, split out of the streaming
//! driver so the state machine reads alone.

use crate::asset_provider::{PrefetchStats, ResolveExtractTotals};
use crate::cell_loader::UnloadPhaseTimings;
use std::time::{Duration, Instant};

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
    pub(crate) fn record(&mut self, elapsed: Duration) {
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
    pub unload_snapshot_capture: StreamingLatencySummary,
    pub unload_handle_collection: StreamingLatencySummary,
    pub unload_gpu_release: StreamingLatencySummary,
    pub unload_owned_state_release: StreamingLatencySummary,
    pub unload_despawn: StreamingLatencySummary,
    pub unload_finalization: StreamingLatencySummary,
    pub worker_queue: StreamingLatencySummary,
    pub worker_parse: StreamingLatencySummary,
    /// Number of NIFs skipped because an earlier request in the same worker
    /// drain already produced them. This is separate from cache hits: the
    /// batch memo closes the gap left by the frozen per-request cache snapshot
    /// (#3670).
    pub worker_batch_duplicate_skips: u64,
    pub apply_slices: StreamingLatencySummary,
    /// Per apply slice: main-thread texture archive extraction (read +
    /// inflate on the resolve miss path) inside that slice. Same samples as
    /// `apply_slices`, so the two totals give extraction's share of apply.
    pub apply_texture_extract: StreamingLatencySummary,
    /// Textures extracted inside counted apply slices.
    pub apply_texture_extracts: u64,
    /// Of `apply_texture_extracts`, how many a background prefetch had
    /// already read, so the slice paid at most a wait for a running read.
    pub apply_texture_prefetched: u64,
    /// Texture reads queued on the stream pool ahead of their resolve.
    pub texture_prefetch_queued: u64,
    /// Latest staging-store snapshot (peak staged bytes, cap drops).
    pub texture_prefetch: PrefetchStats,
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

    pub(crate) fn record_apply_slice(
        &mut self,
        elapsed: Duration,
        worked: bool,
        texture_extract: ResolveExtractTotals,
    ) {
        if self.active.is_some() && worked {
            self.apply_slices.record(elapsed);
            self.apply_texture_extract.record(texture_extract.elapsed);
            self.apply_texture_extracts = self
                .apply_texture_extracts
                .saturating_add(texture_extract.count);
            self.apply_texture_prefetched = self
                .apply_texture_prefetched
                .saturating_add(texture_extract.prefetched);
        }
    }

    pub(crate) fn record_texture_prefetch(&mut self, queued: usize, stats: PrefetchStats) {
        self.texture_prefetch_queued = self.texture_prefetch_queued.saturating_add(queued as u64);
        self.texture_prefetch = stats;
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
        self.unload_snapshot_capture.record(timings.snapshot_capture);
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
            self.worker_batch_duplicate_skips = self
                .worker_batch_duplicate_skips
                .saturating_add(timings.batch_duplicate_skips as u64);
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
             unload_index_max_ms={:.2} unload_snapshot_max_ms={:.2} unload_collect_max_ms={:.2} \
             unload_gpu_max_ms={:.2} unload_owned_max_ms={:.2} \
             unload_despawn_max_ms={:.2} unload_finalize_max_ms={:.2} \
             worker_queue_avg_ms={:.2} worker_queue_max_ms={:.2} \
             worker_avg_ms={:.2} worker_max_ms={:.2} \
             worker_batch_duplicate_skips={} \
             apply_samples={} apply_avg_ms={:.2} apply_max_ms={:.2} apply_total_ms={:.2} \
             apply_tex_extracts={} apply_tex_extract_total_ms={:.2} \
             apply_tex_extract_max_ms={:.2} apply_tex_prefetched={} \
             tex_prefetch_queued={} tex_prefetch_peak_mib={:.1} tex_prefetch_dropped={} \
             tex_prefetch_withdrawn={} tex_prefetch_unused={} \
             lod_slice_avg_ms={:.2} lod_slice_max_ms={:.2} peak_pending={} \
             unsettled_full={} unsettled_lod={} {} {} {} {} {} {} {}",
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
            self.unload_snapshot_capture.max_ms(),
            self.unload_handle_collection.max_ms(),
            self.unload_gpu_release.max_ms(),
            self.unload_owned_state_release.max_ms(),
            self.unload_despawn.max_ms(),
            self.unload_finalization.max_ms(),
            self.worker_queue.average_ms(),
            self.worker_queue.max_ms(),
            self.worker_parse.average_ms(),
            self.worker_parse.max_ms(),
            self.worker_batch_duplicate_skips,
            self.apply_slices.samples,
            self.apply_slices.average_ms(),
            self.apply_slices.max_ms(),
            self.apply_slices.total.as_secs_f64() * 1000.0,
            self.apply_texture_extracts,
            self.apply_texture_extract.total.as_secs_f64() * 1000.0,
            self.apply_texture_extract.max_ms(),
            self.apply_texture_prefetched,
            self.texture_prefetch_queued,
            self.texture_prefetch.peak_ready_bytes as f64 / (1024.0 * 1024.0),
            self.texture_prefetch.dropped_over_cap,
            self.texture_prefetch.withdrawn,
            self.texture_prefetch.unused_at_clear,
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
            phase_distribution("apply_tex_extract", &self.apply_texture_extract),
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreamingWorkerTimings {
    pub queue_wait: Duration,
    pub worker: Duration,
    /// NIFs omitted because an earlier request in the current worker queue
    /// batch already produced the same canonical key (#3670).
    pub batch_duplicate_skips: usize,
    /// Worker names observed by the parallel NIF parse fan-out. Empty for the
    /// serial branch. This is both useful telemetry and the observable that
    /// keeps #3089's dedicated-pool routing falsifiable (#3211).
    pub parallel_parse_threads: Vec<String>,
}
