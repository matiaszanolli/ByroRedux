//! Benchmark-mode contracts and deterministic scene-state fingerprints.
//!
//! A benchmark mode owns both clocks that can change the measured scene:
//! simulation delta-time and camera motion. Keeping those controls together
//! prevents a nominal renderer comparison from accidentally retaining the
//! wall-clock feedback loop that it was meant to remove.

use std::fmt;
use std::str::FromStr;

use byroredux_core::ecs::{ActiveCamera, TotalTime, Transform, World};
use byroredux_core::math::Vec3;
use byroredux_renderer::vulkan::context::DrawCommand;
use byroredux_renderer::vulkan::volumetrics::GpuFogVolume;
use byroredux_renderer::vulkan::water::WaterDrawCommand;
use byroredux_renderer::GpuLight;

use crate::bench_camera::BenchCameraPath;

/// Whether the finite benchmark still owns simulation time and camera state.
///
/// `--bench-hold` keeps the process alive after the summary is printed for
/// interactive inspection. That inspection must run like the normal engine:
/// wall-clock time and player/fly-camera input are live again.
pub(crate) const fn harness_active(summary_printed: bool) -> bool {
    !summary_printed
}

/// Complete timing/camera contract for a finite `--bench-frames` run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BenchMode {
    /// Fixed `dt = 0`, authored camera held still. Renderer attribution and
    /// deterministic regression gates only.
    RendererStatic,
    /// Fixed `dt = 1/60`, non-static frame-indexed camera path. Animation,
    /// streaming, and temporal-upscaler comparisons.
    RendererStepped,
    /// Wall-clock delta-time and no harness-owned camera. Combined system
    /// observation only; never a regression gate.
    SystemLive,
}

impl BenchMode {
    pub(crate) const FIXED_STEP_SECONDS: f32 = 1.0 / 60.0;

    pub(crate) fn delta_time(self, wall_dt: f32) -> f32 {
        match self {
            Self::RendererStatic => 0.0,
            Self::RendererStepped => Self::FIXED_STEP_SECONDS,
            Self::SystemLive => wall_dt,
        }
    }

    pub(crate) const fn gate_label(self) -> &'static str {
        match self {
            Self::RendererStatic => "renderer",
            Self::RendererStepped => "upscaler",
            Self::SystemLive => "none",
        }
    }

    pub(crate) const fn dt_label(self) -> &'static str {
        match self {
            Self::RendererStatic => "fixed-0",
            Self::RendererStepped => "fixed-1/60",
            Self::SystemLive => "wall-clock",
        }
    }
}

impl fmt::Display for BenchMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RendererStatic => "renderer-static",
            Self::RendererStepped => "renderer-stepped",
            Self::SystemLive => "system-live",
        })
    }
}

impl FromStr for BenchMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "renderer-static" => Ok(Self::RendererStatic),
            "renderer-stepped" => Ok(Self::RendererStepped),
            "system-live" => Ok(Self::SystemLive),
            other => Err(format!(
                "unknown --bench-mode '{other}'; expected one of: \
                 renderer-static, renderer-stepped, system-live"
            )),
        }
    }
}

/// Resolved mode plus its harness-owned camera. `inferred` is true only for
/// canonical legacy invocations retained during the CLI migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BenchSelection {
    pub mode: BenchMode,
    pub camera: Option<BenchCameraPath>,
    pub inferred: bool,
}

/// Resolve a finite benchmark onto one of the three valid mode contracts.
///
/// Explicit modes reject `BYROREDUX_FIXED_DT`: one named mode must be the sole
/// owner of timing. Legacy invocations are accepted only when they already
/// match a complete contract (`fixed 0 + static`, `fixed 1/60 + moving path`,
/// or no overrides at all). Mixed states fail instead of receiving a false
/// mode label in the benchmark record.
pub(crate) fn resolve_bench_selection(
    has_bench_frames: bool,
    mode: Option<BenchMode>,
    camera: Option<BenchCameraPath>,
    legacy_fixed_dt: Option<&str>,
) -> Result<Option<BenchSelection>, String> {
    if !has_bench_frames {
        if mode.is_some() {
            return Err("--bench-mode requires --bench-frames N".to_owned());
        }
        return Ok(None);
    }

    if let Some(mode) = mode {
        if legacy_fixed_dt.is_some() {
            return Err(format!(
                "--bench-mode {mode} owns delta-time; unset BYROREDUX_FIXED_DT"
            ));
        }
        let camera = camera_for_mode(mode, camera)?;
        return Ok(Some(BenchSelection {
            mode,
            camera,
            inferred: false,
        }));
    }

    match legacy_fixed_dt {
        Some(raw) => {
            let fixed_dt = raw.parse::<f32>().map_err(|_| {
                format!(
                    "BYROREDUX_FIXED_DT='{raw}' is not a valid number; prefer an explicit --bench-mode"
                )
            })?;
            if fixed_dt == 0.0 && matches!(camera, None | Some(BenchCameraPath::Static)) {
                Ok(Some(BenchSelection {
                    mode: BenchMode::RendererStatic,
                    camera: Some(BenchCameraPath::Static),
                    inferred: true,
                }))
            } else if (fixed_dt - BenchMode::FIXED_STEP_SECONDS).abs() <= 1.0e-5
                && camera.is_some_and(|path| path != BenchCameraPath::Static)
            {
                Ok(Some(BenchSelection {
                    mode: BenchMode::RendererStepped,
                    camera,
                    inferred: true,
                }))
            } else {
                Err(format!(
                    "legacy benchmark controls do not match a named mode \
                     (BYROREDUX_FIXED_DT={raw}, camera={}); pass --bench-mode explicitly",
                    camera.map_or_else(|| "free".to_owned(), |path| path.to_string())
                ))
            }
        }
        None if camera.is_none() => Ok(Some(BenchSelection {
            mode: BenchMode::SystemLive,
            camera: None,
            inferred: true,
        })),
        None => Err(
            "--bench-camera without --bench-mode is an unnamed wall-clock/fixed-camera hybrid; \
             use --bench-mode renderer-stepped"
                .to_owned(),
        ),
    }
}

fn camera_for_mode(
    mode: BenchMode,
    camera: Option<BenchCameraPath>,
) -> Result<Option<BenchCameraPath>, String> {
    match mode {
        BenchMode::RendererStatic => match camera {
            None | Some(BenchCameraPath::Static) => Ok(Some(BenchCameraPath::Static)),
            Some(path) => Err(format!(
                "--bench-mode renderer-static requires a static camera, not '{path}'"
            )),
        },
        BenchMode::RendererStepped => match camera {
            Some(path) if path != BenchCameraPath::Static => Ok(Some(path)),
            _ => Err(
                "--bench-mode renderer-stepped requires a non-static --bench-camera path"
                    .to_owned(),
            ),
        },
        BenchMode::SystemLive => {
            if let Some(path) = camera {
                Err(format!(
                    "--bench-mode system-live leaves the camera free; remove --bench-camera '{path}'"
                ))
            } else {
                Ok(None)
            }
        }
    }
}

/// Stable fingerprint and forensic columns captured at the measured frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BenchSceneState {
    pub camera_position: [f32; 3],
    pub camera_forward: [f32; 3],
    pub simulated_time_s: f32,
    pub entities: u32,
    pub draws: u32,
    pub lights: u32,
    pub tlas_eligible: u32,
    pub state_hash: u64,
}

/// Hash the final renderer-facing scene state after the timed window closes.
///
/// The fingerprint covers the simulation clock, full active-camera transform,
/// sorted draw stream (including model matrices and material hashes), water
/// push constants, light/fog buffers, and skinned bone-world matrices. It is
/// intentionally computed only at benchmark exit, after wall timing has been
/// sampled, so the assertion itself cannot contaminate the measured frame.
pub(crate) fn capture_scene_state(
    world: &World,
    draw_commands: &[DrawCommand],
    water_commands: &[WaterDrawCommand],
    gpu_lights: &[GpuLight],
    gpu_fog_volumes: &[GpuFogVolume],
    bone_world: &[[[f32; 4]; 4]],
) -> BenchSceneState {
    let simulated_time_s = world.try_resource::<TotalTime>().map_or(0.0, |time| time.0);
    let entities = world.next_entity_id();
    let active_camera = world.try_resource::<ActiveCamera>().map(|active| active.0);
    let camera_transform = active_camera.and_then(|entity| {
        world
            .query::<Transform>()
            .and_then(|query| query.get(entity).copied())
    });
    let (camera_position, camera_forward, camera_rotation, camera_scale) = camera_transform
        .map(|transform| {
            (
                transform.translation.to_array(),
                (transform.rotation * -Vec3::Z).to_array(),
                transform.rotation.to_array(),
                transform.scale,
            )
        })
        .unwrap_or(([0.0; 3], [0.0, 0.0, -1.0], [0.0, 0.0, 0.0, 1.0], 1.0));

    let tlas_eligible = draw_commands
        .iter()
        .filter(|draw| draw.in_tlas && !draw.is_water)
        .count() as u32;

    let mut hash = StableStateHasher::new();
    hash.bytes(b"byro-bench-scene-v1");
    hash.f32(simulated_time_s);
    hash.f32_array(camera_position);
    hash.f32_array(camera_forward);
    hash.f32_array(camera_rotation);
    hash.f32(camera_scale);
    hash.u32(entities);
    hash.u32(draw_commands.len() as u32);
    hash.u32(water_commands.len() as u32);
    hash.u32(gpu_lights.len() as u32);
    hash.u32(gpu_fog_volumes.len() as u32);
    hash.u32(tlas_eligible);

    for draw in draw_commands {
        hash.u32(draw.entity_id);
        hash.u32(draw.mesh_handle);
        hash.u64(draw.material_hash());
        hash.f32_array(draw.model_matrix);
        hash.u32(draw.vertex_offset);
        hash.u32(draw.index_offset);
        hash.u32(draw.vertex_count);
        hash.u32(draw.sort_depth);
        hash.u32(draw.bone_offset);
        hash.u32(draw.material_id);
        hash.u32(draw.terrain_tile_index.unwrap_or(u32::MAX));
        hash.u8(draw.render_layer as u8);
        hash.u8(draw.src_blend);
        hash.u8(draw.dst_blend);
        hash.u8(draw.z_function);
        hash.bool(draw.alpha_blend);
        hash.bool(draw.two_sided);
        hash.bool(draw.wireframe);
        hash.bool(draw.flat_shading);
        hash.bool(draw.is_decal);
        hash.bool(draw.z_test);
        hash.bool(draw.z_write);
        hash.bool(draw.in_tlas);
        hash.bool(draw.in_raster);
        hash.bool(draw.is_water);
    }

    for water in water_commands {
        hash.u32(water.mesh_handle);
        hash.u32(water.instance_index);
        hash.f32_array(water.params.timing);
        hash.f32_array(water.params.flow);
        hash.f32_array(water.params.shallow);
        hash.f32_array(water.params.deep);
        hash.f32_array(water.params.scroll);
        hash.f32_array(water.params.tune);
        hash.f32_array(water.params.misc);
        hash.f32_array(water.params.tint_reflect);
        for &idx in &water.params.noise_indices {
            hash.u32(idx);
        }
    }
    for light in gpu_lights {
        hash.f32_array(light.position_radius);
        hash.f32_array(light.color_type);
        hash.f32_array(light.direction_angle);
        hash.f32_array(light.params);
    }
    for volume in gpu_fog_volumes {
        hash.f32_array(volume.center_shape);
        hash.f32_array(volume.half_extents_extinction);
        hash.f32_array(volume.inverse_rotation);
        hash.f32_array(volume.albedo_edge);
        hash.f32_array(volume.emission_temperature);
    }
    for matrix in bone_world {
        for column in matrix {
            hash.f32_array(*column);
        }
    }

    BenchSceneState {
        camera_position,
        camera_forward,
        simulated_time_s,
        entities,
        draws: draw_commands.len() as u32,
        lights: gpu_lights.len() as u32,
        tlas_eligible,
        state_hash: hash.finish(),
    }
}

/// Small, versioned FNV-1a implementation. Unlike `DefaultHasher`, its byte
/// contract is explicit and stable across compiler/toolchain updates.
struct StableStateHasher(u64);

impl StableStateHasher {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    fn new() -> Self {
        Self(Self::OFFSET)
    }

    fn bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    fn u8(&mut self, value: u8) {
        self.bytes(&[value]);
    }

    fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }

    fn u32(&mut self, value: u32) {
        self.bytes(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }

    fn f32(&mut self, value: f32) {
        self.u32(value.to_bits());
    }

    fn f32_array<const N: usize>(&mut self, values: [f32; N]) {
        for value in values {
            self.f32(value);
        }
    }

    fn finish(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_names_round_trip_and_pin_clock_contracts() {
        for mode in [
            BenchMode::RendererStatic,
            BenchMode::RendererStepped,
            BenchMode::SystemLive,
        ] {
            assert_eq!(mode.to_string().parse::<BenchMode>().unwrap(), mode);
        }
        assert_eq!(BenchMode::RendererStatic.delta_time(0.25), 0.0);
        assert_eq!(
            BenchMode::RendererStepped.delta_time(0.25),
            BenchMode::FIXED_STEP_SECONDS
        );
        assert_eq!(BenchMode::SystemLive.delta_time(0.25), 0.25);
        assert!("static".parse::<BenchMode>().is_err());
    }

    #[test]
    fn held_session_releases_harness_after_summary() {
        assert!(harness_active(false));
        assert!(!harness_active(true));
    }

    #[test]
    fn explicit_modes_reject_hybrid_controls() {
        assert!(resolve_bench_selection(
            true,
            Some(BenchMode::RendererStatic),
            Some(BenchCameraPath::Pan),
            None,
        )
        .is_err());
        assert!(
            resolve_bench_selection(true, Some(BenchMode::RendererStepped), None, None,).is_err()
        );
        assert!(resolve_bench_selection(
            true,
            Some(BenchMode::SystemLive),
            Some(BenchCameraPath::Orbit),
            None,
        )
        .is_err());
        assert!(
            resolve_bench_selection(true, Some(BenchMode::RendererStatic), None, Some("0"),)
                .is_err()
        );
    }

    #[test]
    fn canonical_legacy_invocations_receive_honest_names() {
        let frozen = resolve_bench_selection(true, None, None, Some("0"))
            .unwrap()
            .unwrap();
        assert_eq!(frozen.mode, BenchMode::RendererStatic);
        assert_eq!(frozen.camera, Some(BenchCameraPath::Static));
        assert!(frozen.inferred);

        let stepped =
            resolve_bench_selection(true, None, Some(BenchCameraPath::Orbit), Some("0.0166667"))
                .unwrap()
                .unwrap();
        assert_eq!(stepped.mode, BenchMode::RendererStepped);

        let live = resolve_bench_selection(true, None, None, None)
            .unwrap()
            .unwrap();
        assert_eq!(live.mode, BenchMode::SystemLive);
        assert_eq!(live.camera, None);
    }

    #[test]
    fn empty_scene_fingerprint_is_repeatable_and_time_sensitive() {
        let mut world = World::new();
        world.insert_resource(TotalTime(0.0));
        let camera = world.spawn();
        world.insert(camera, Transform::default());
        world.insert_resource(ActiveCamera(camera));

        let first = capture_scene_state(&world, &[], &[], &[], &[], &[]);
        let repeat = capture_scene_state(&world, &[], &[], &[], &[], &[]);
        assert_eq!(first, repeat);

        world.resource_mut::<TotalTime>().0 = 1.0;
        let advanced = capture_scene_state(&world, &[], &[], &[], &[], &[]);
        assert_ne!(first.state_hash, advanced.state_hash);
        assert_eq!(advanced.simulated_time_s, 1.0);
    }
}

/// #3407 — structural guard over the checked-in runtime telemetry
/// baselines (`.claude/audit-baselines/runtime/*.tsv`).
///
/// The defect that motivated this was a hand-transcribed number: the `fo3`
/// baseline's `bench_draws_batches` was written as FNV's spike batch count
/// from an adjacent table, and its `bench_draws_gpu_calls` was left at the
/// stale pre-refresh value. Neither is structurally detectable — only a
/// re-measurement catches a wrong-but-well-formed number, and these files
/// exist precisely because that measurement needs a GPU and game data.
///
/// What IS checkable here is the file contract the README states, which is
/// what the regen path writes and what `/audit-runtime` Phase 3 diffs
/// against: the `# regenerated:` header, one occurrence of every gating
/// metric, and a parseable numeric value on each. A baseline that loses a
/// row silently stops gating that metric — the same class of failure, and
/// the one a test can actually see.
#[cfg(test)]
mod runtime_baseline_schema_tests {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    /// Every metric the README's schema block lists. `bench_fps_*` are
    /// advisory (RT-2 / #1701) but still stored, so they must still be
    /// present and parseable.
    const REQUIRED_METRICS: &[&str] = &[
        "entities_total",
        "tex_missing_unique_paths",
        "mesh_cache_failed_count",
        "light_count_directional",
        "skin_pool_live",
        "skin_pool_max",
        "skin_pool_overflow_attempts",
        "bench_fps_p50",
        "bench_fps_avg",
        "bench_draws_cmds",
        "bench_draws_batches",
        "bench_draws_gpu_calls",
    ];

    fn baseline_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("byroredux/ has a workspace parent")
            .join(".claude/audit-baselines/runtime")
    }

    fn parse(path: &Path) -> (bool, HashMap<String, String>) {
        let text = std::fs::read_to_string(path).expect("read baseline");
        let mut has_header = false;
        let mut rows = HashMap::new();
        for line in text.lines() {
            if line.starts_with("# regenerated:") {
                has_header = true;
                continue;
            }
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let mut parts = line.split_whitespace();
            let (Some(key), Some(value)) = (parts.next(), parts.next()) else {
                panic!("{}: malformed row {line:?}", path.display());
            };
            assert!(
                rows.insert(key.to_string(), value.to_string()).is_none(),
                "{}: metric {key} appears twice — a duplicate row means the \
                 diff reads whichever the parser reaches last",
                path.display(),
            );
        }
        (has_header, rows)
    }

    #[test]
    fn every_baseline_carries_the_full_gating_metric_set() {
        let dir = baseline_dir();
        let mut checked = 0usize;
        for entry in std::fs::read_dir(&dir).expect("baseline dir") {
            let path = entry.expect("dir entry").path();
            if path.extension().is_none_or(|e| e != "tsv") {
                continue;
            }
            checked += 1;
            let (has_header, rows) = parse(&path);
            assert!(
                has_header,
                "{}: missing the `# regenerated: YYYY-MM-DD` header the \
                 README requires",
                path.display(),
            );
            for metric in REQUIRED_METRICS {
                let value = rows.get(*metric).unwrap_or_else(|| {
                    panic!(
                        "{}: missing metric {metric} — /audit-runtime silently \
                         stops gating a metric it cannot find",
                        path.display(),
                    )
                });
                value.parse::<f64>().unwrap_or_else(|_| {
                    panic!(
                        "{}: metric {metric} = {value:?} is not numeric",
                        path.display()
                    )
                });
            }
        }
        assert_eq!(
            checked, 5,
            "expected the five per-game runtime baselines (fnv, fo3, fo4, \
             oblivion, skyrim_se), found {checked}"
        );
    }

    /// A draw split must be internally coherent: the three-way
    /// `cmds >= batches >= gpu_calls` ordering is what the split MEANS
    /// (commands merge into batches, batches issue as GPU calls), so a
    /// violation is a transcription error no matter what the engine
    /// measured. The `fo3` row that motivated #3407 satisfied this even
    /// while wrong — recorded so the next reader knows this guard's reach.
    #[test]
    fn draw_split_rows_are_internally_ordered() {
        for entry in std::fs::read_dir(baseline_dir()).expect("baseline dir") {
            let path = entry.expect("dir entry").path();
            if path.extension().is_none_or(|e| e != "tsv") {
                continue;
            }
            let (_, rows) = parse(&path);
            let get = |k: &str| rows[k].parse::<f64>().expect("numeric");
            let (cmds, batches, calls) = (
                get("bench_draws_cmds"),
                get("bench_draws_batches"),
                get("bench_draws_gpu_calls"),
            );
            assert!(
                cmds >= batches && batches >= calls,
                "{}: draw split {cmds}/{batches}b/{calls}c violates \
                 cmds >= batches >= gpu_calls",
                path.display(),
            );
        }
    }
}
