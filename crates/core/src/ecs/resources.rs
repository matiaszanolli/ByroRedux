//! Built-in engine resources.

use super::resource::Resource;

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
}
