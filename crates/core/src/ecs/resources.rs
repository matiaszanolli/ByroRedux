//! Built-in engine resources.

use super::resource::Resource;

/// System names stored as a resource for debug and console queries.
pub struct SystemList(pub Vec<String>);
impl Resource for SystemList {}

/// Snapshot of [`crate::ecs::scheduler::AccessReport`] captured at
/// scheduler-build time and stored as a resource so console commands
/// (`sys.accesses`) can read the per-stage declared-access map without
/// a live `Scheduler` reference. R7.
pub struct SchedulerAccessReport(pub crate::ecs::scheduler::AccessReport);
impl Resource for SchedulerAccessReport {}

/// Bridge for requesting screenshots from the renderer.
/// Set `requested` to true; the renderer will capture the next frame and
/// place the PNG bytes in `result`.
pub struct ScreenshotBridge {
    pub requested: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub result: std::sync::Arc<std::sync::Mutex<Option<Vec<u8>>>>,
}

impl ScreenshotBridge {
    pub fn request(&self) {
        self.requested
            .store(true, std::sync::atomic::Ordering::Release);
    }

    pub fn take_result(&self) -> Option<Vec<u8>> {
        self.result.lock().unwrap().take()
    }
}

impl Resource for ScreenshotBridge {}

/// Per-frame delta time in seconds.
pub struct DeltaTime(pub f32);
impl Resource for DeltaTime {}

/// Accumulated wall-clock time since engine start, in seconds.
pub struct TotalTime(pub f32);
impl Resource for TotalTime {}

/// Global engine configuration.
pub struct EngineConfig {
    pub vsync: bool,
    pub target_fps: Option<u32>,
    pub debug_logging: bool,
}

impl Resource for EngineConfig {}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            vsync: true,
            target_fps: None,
            debug_logging: cfg!(debug_assertions),
        }
    }
}

const FRAME_HISTORY_SIZE: usize = 128;

/// Per-frame engine diagnostics, updated by the main loop.
///
/// Contains a rolling window of frame times for FPS calculation,
/// plus counts of entities, meshes, textures, and draw calls.
pub struct DebugStats {
    /// Current instantaneous FPS (1.0 / last dt).
    pub fps: f32,
    /// Current frame time in milliseconds.
    pub frame_time_ms: f32,
    /// Rolling buffer of frame times in seconds.
    frame_times: [f32; FRAME_HISTORY_SIZE],
    /// Write cursor into frame_times.
    frame_index: usize,
    /// Number of frames recorded (saturates at FRAME_HISTORY_SIZE).
    frame_count: usize,
    /// Total entities in the world.
    pub entity_count: u32,
    /// GPU meshes in the MeshRegistry.
    pub mesh_count: u32,
    /// GPU textures in the TextureRegistry.
    pub texture_count: u32,
    /// Draw calls last frame.
    pub draw_call_count: u32,
}

impl Resource for DebugStats {}

impl Default for DebugStats {
    fn default() -> Self {
        Self {
            fps: 0.0,
            frame_time_ms: 0.0,
            frame_times: [0.0; FRAME_HISTORY_SIZE],
            frame_index: 0,
            frame_count: 0,
            entity_count: 0,
            mesh_count: 0,
            texture_count: 0,
            draw_call_count: 0,
        }
    }
}

impl DebugStats {
    /// Record a frame's delta time and update FPS.
    pub fn push_frame_time(&mut self, dt: f32) {
        self.frame_times[self.frame_index] = dt;
        self.frame_index = (self.frame_index + 1) % FRAME_HISTORY_SIZE;
        if self.frame_count < FRAME_HISTORY_SIZE {
            self.frame_count += 1;
        }
        self.frame_time_ms = dt * 1000.0;
        self.fps = if dt > 0.0 { 1.0 / dt } else { 0.0 };
    }

    /// Average FPS over the rolling window.
    pub fn avg_fps(&self) -> f32 {
        if self.frame_count == 0 {
            return 0.0;
        }
        let sum: f32 = self.frame_times[..self.frame_count].iter().sum();
        let avg_dt = sum / self.frame_count as f32;
        if avg_dt > 0.0 {
            1.0 / avg_dt
        } else {
            0.0
        }
    }

    /// Min and max frame times (seconds) over the rolling window.
    pub fn min_max_frame_time(&self) -> (f32, f32) {
        if self.frame_count == 0 {
            return (0.0, 0.0);
        }
        let slice = &self.frame_times[..self.frame_count];
        let min = slice.iter().cloned().fold(f32::INFINITY, f32::min);
        let max = slice.iter().cloned().fold(0.0f32, f32::max);
        (min, max)
    }

    /// Current write index (useful for throttling title bar updates).
    pub fn frame_index(&self) -> usize {
        self.frame_index
    }
}

/// One row of renderer-side scratch-buffer telemetry: name, current
/// `len`, current `capacity`, and the size in bytes of one element so
/// consumers can compute the heap footprint without knowing the
/// renderer's element types.
///
/// Used by R6 (ROADMAP) to catch unbounded `Vec` growth in the
/// renderer's per-frame scratch buffers, particularly across M40 cell
/// streaming where the high-water mark would otherwise grow silently.
#[derive(Debug, Clone, Copy)]
pub struct ScratchRow {
    pub name: &'static str,
    pub len: usize,
    pub capacity: usize,
    pub elem_size_bytes: usize,
}

impl ScratchRow {
    /// Heap footprint of the buffer at its current `capacity` (not `len`).
    pub fn bytes_used(&self) -> usize {
        self.capacity.saturating_mul(self.elem_size_bytes)
    }

    /// Bytes of headroom — `capacity - len` × element size. Sustained
    /// non-zero values across many frames mean the high-water mark
    /// drifted up at some point and never came back down.
    pub fn wasted_bytes(&self) -> usize {
        self.capacity
            .saturating_sub(self.len)
            .saturating_mul(self.elem_size_bytes)
    }
}

/// Snapshot of every renderer-side persistent `Vec` scratch's capacity.
///
/// Refreshed each frame by the engine binary (after `Scheduler::run`,
/// alongside `mesh_count` / `texture_count` on `DebugStats`). Read by
/// the `ctx.scratch` console command.
///
/// `rows` is a reused `Vec` — stabilises at the count of registered
/// scratches (5 today) after the first frame, and is the *only*
/// per-frame heap allocation in the telemetry path. Bounded by the
/// number of declared scratches at the call site
/// (`VulkanContext::fill_scratch_telemetry`), so it cannot itself
/// exhibit the unbounded-growth pattern this resource is designed
/// to catch.
#[derive(Debug, Default)]
pub struct ScratchTelemetry {
    pub rows: Vec<ScratchRow>,
    /// R1 / #780 — unique materials at end of last `build_render_data`
    /// (== `MaterialTable::len()`). Pairs with `materials_interned` to
    /// compute the dedup ratio.
    pub materials_unique: usize,
    /// R1 / #780 — total `intern()` calls during last
    /// `build_render_data` (== `MaterialTable::interned_count()`,
    /// one tick per emitted `DrawCommand`). Dedup ratio =
    /// `materials_unique / materials_interned`. A drop here flags a
    /// regression (alignment hole, non-deterministic float in the
    /// producer) that breaks byte-equality dedup before VRAM
    /// pressure shows it.
    pub materials_interned: usize,
}

impl Resource for ScratchTelemetry {}

impl ScratchTelemetry {
    pub fn total_bytes(&self) -> usize {
        self.rows.iter().map(ScratchRow::bytes_used).sum()
    }

    pub fn total_wasted(&self) -> usize {
        self.rows.iter().map(ScratchRow::wasted_bytes).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_stats_default_is_zero() {
        let stats = DebugStats::default();
        assert_eq!(stats.fps, 0.0);
        assert_eq!(stats.avg_fps(), 0.0);
        assert_eq!(stats.min_max_frame_time(), (0.0, 0.0));
    }

    #[test]
    fn push_frame_time_updates_fps() {
        let mut stats = DebugStats::default();
        stats.push_frame_time(1.0 / 60.0); // 60 FPS
        assert!((stats.fps - 60.0).abs() < 0.5);
        assert!((stats.frame_time_ms - 16.67).abs() < 0.1);
    }

    #[test]
    fn avg_fps_over_multiple_frames() {
        let mut stats = DebugStats::default();
        for _ in 0..10 {
            stats.push_frame_time(1.0 / 30.0); // 30 FPS
        }
        assert!((stats.avg_fps() - 30.0).abs() < 0.5);
    }

    #[test]
    fn min_max_frame_time_correct() {
        let mut stats = DebugStats::default();
        stats.push_frame_time(0.010); // 10ms
        stats.push_frame_time(0.020); // 20ms
        stats.push_frame_time(0.005); // 5ms
        let (min, max) = stats.min_max_frame_time();
        assert!((min - 0.005).abs() < 1e-6);
        assert!((max - 0.020).abs() < 1e-6);
    }

    #[test]
    fn circular_buffer_wraps() {
        let mut stats = DebugStats::default();
        // Fill more than 128 frames
        for i in 0..200 {
            stats.push_frame_time(0.016);
            assert!(stats.frame_count <= FRAME_HISTORY_SIZE);
            if i >= FRAME_HISTORY_SIZE {
                assert_eq!(stats.frame_count, FRAME_HISTORY_SIZE);
            }
        }
        assert_eq!(stats.frame_index, 200 % FRAME_HISTORY_SIZE);
        assert!((stats.avg_fps() - 62.5).abs() < 1.0);
    }

    #[test]
    fn scratch_row_bytes_used_is_capacity_times_elem_size() {
        let row = ScratchRow {
            name: "x",
            len: 10,
            capacity: 100,
            elem_size_bytes: 32,
        };
        assert_eq!(row.bytes_used(), 100 * 32);
        assert_eq!(row.wasted_bytes(), (100 - 10) * 32);
    }

    #[test]
    fn scratch_row_wasted_is_zero_when_full() {
        let row = ScratchRow {
            name: "full",
            len: 50,
            capacity: 50,
            elem_size_bytes: 8,
        };
        assert_eq!(row.wasted_bytes(), 0);
    }

    #[test]
    fn scratch_telemetry_aggregates_rows() {
        let mut tlm = ScratchTelemetry::default();
        tlm.rows.push(ScratchRow {
            name: "a",
            len: 1,
            capacity: 10,
            elem_size_bytes: 4,
        });
        tlm.rows.push(ScratchRow {
            name: "b",
            len: 5,
            capacity: 5,
            elem_size_bytes: 16,
        });
        assert_eq!(tlm.total_bytes(), 10 * 4 + 5 * 16);
        assert_eq!(tlm.total_wasted(), (10 - 1) * 4 + 0);
    }
}
