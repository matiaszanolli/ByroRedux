//! Water-plane re-emit — extracted from `build_render_data` per #1115.
//!
//! Walks every `WaterPlane` entity, locates its already-emitted
//! `DrawCommand` (the main mesh-iteration loop above produced it
//! because water entities carry `MeshHandle`), flips its `is_water`
//! flag so the regular triangle path skips it, and emits a parallel
//! `WaterDrawCommand` whose `instance_index` matches the SSBO slot
//! the renderer will assign to that draw.
//!
//! The slot-index ↔ Vec position map relies on the renderer's 1:1
//! contract: `gpu_instances` is populated by iterating `draw_commands`
//! in order, and frustum-culled draws keep their SSBO slot per #516.
//! So the index into `draw_commands` equals `gl_InstanceIndex` after
//! upload.
//!
//! ⚠ **No-resort contract** (#1026 / F-WAT-05) — once the
//! `instance_index` captured below is written, `draw_commands` MUST
//! NOT be re-ordered before the renderer consumes it. The
//! defensive `debug_assert!` lives in `VulkanContext::draw_frame`
//! (using `byroredux_renderer::vulkan::water::water_commands_match_draw_slots`).
//! This function must run AFTER the draw_commands sort and BEFORE
//! the renderer consumes them.

use byroredux_core::ecs::components::groundcover::WindField;
use byroredux_core::ecs::components::water::{WaterFlow, WaterKind, WaterPlane};
use byroredux_core::ecs::{TotalTime, World};
use byroredux_renderer::vulkan::context::DrawCommand;
use byroredux_renderer::vulkan::water::{GpuWaterParams, WaterDrawCommand};
use byroredux_scripting::RippleEvent;

#[inline]
fn lerp_color(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Pack three non-negative legacy rain controls into the reserved fourth
/// normal-falloff lane. Each channel uses a 10-bit reciprocal encoding so
/// small authored values retain useful precision while large malformed
/// values remain bounded; zero is the compatibility sentinel.
#[inline]
fn pack_rain_controls(velocity: f32, falloff: f32, dampener: f32) -> f32 {
    let quantize = |value: f32| {
        let value = if value.is_finite() { value.max(0.0) } else { 0.0 };
        ((value / (value + 1.0)).clamp(0.0, 0.999) * 1023.0).round() as u32
    };
    let packed = quantize(velocity) | (quantize(falloff) << 10) | (quantize(dampener) << 20);
    f32::from_bits(packed)
}

/// Re-emit water planes: flip the `is_water` flag on each plane's
/// already-emitted draw command and produce a matching
/// `WaterDrawCommand` referencing the same SSBO slot.
///
/// Linear scan over `draw_commands` per water entity is O(N×W);
/// typical N is ~thousands of draws and W is ≤ ~3 water planes per
/// cell, so this is well under a microsecond. A
/// `HashMap<EntityId, usize>` would be premature for the expected
/// scale.
pub(super) fn reemit_water_planes(
    world: &World,
    draw_commands: &mut [DrawCommand],
    water_commands: &mut Vec<WaterDrawCommand>,
) {
    let time_secs = world
        .try_resource::<TotalTime>()
        .map(|t| t.0)
        .unwrap_or(0.0);
    // The same weather wind that drives SpeedTree canopy sway also perturbs
    // water's surface normals. WATR wind remains water-local authored motion;
    // this resource is the live atmosphere shared by both surfaces.
    let weather_wind = world
        .try_resource::<WindField>()
        .map(|w| *w)
        .unwrap_or_default();
    let precipitation = world
        .try_resource::<crate::components::WeatherDataRes>()
        .map(|weather| weather.precipitation.clamp(0.0, 1.0))
        .unwrap_or(0.0);
    let game_hour = world
        .try_resource::<crate::components::GameTimeRes>()
        .map(|clock| clock.hour);
    let tod_hours = world
        .try_resource::<crate::components::WeatherDataRes>()
        .map(|weather| weather.tod_hours);
    let night_factor = match (game_hour, tod_hours) {
        (Some(hour), Some(hours)) => crate::systems::weather::night_factor_for_hour(hour, hours),
        // Interior and synthetic worlds without a weather resource retain
        // the daytime/base palette rather than inventing a second clock.
        _ => 0.0,
    };
    let gust = weather_wind.speed
        + weather_wind.gust_amplitude
            * (time_secs * weather_wind.gust_frequency * std::f32::consts::TAU).sin();
    // SpeedTree treats a negative instantaneous gust as calm weather by
    // clamping its bend strength to zero. Keep water's UV drift on that same
    // one-sided magnitude contract; otherwise a gust trough briefly reverses
    // the surface while vegetation has stopped responding.
    let gust = if gust.is_finite() { gust.max(0.0) } else { 0.0 };
    // SpeedTree sway treats the field as a direction, not a magnitude. Keep
    // water's weather scroll on that same contract so a hand-authored or
    // malformed resource cannot make water travel faster than the foliage it
    // is meant to share wind with.
    let wind_direction = {
        let len_sq = weather_wind.direction[0] * weather_wind.direction[0]
            + weather_wind.direction[1] * weather_wind.direction[1];
        if len_sq.is_finite() && len_sq > 1.0e-6 {
            let inv_len = len_sq.sqrt().recip();
            [
                weather_wind.direction[0] * inv_len,
                weather_wind.direction[1] * inv_len,
            ]
        } else {
            // Match the billboard path's deterministic fallback for a zero
            // or non-finite direction.
            [1.0, 0.0]
        }
    };
    // Use the same instantaneous gust magnitude that drives SpeedTree sway.
    // Keep authored WATR amplitude as the baseline, then add at most 50% in
    // the strongest weather so calm water remains calm and storm water gains
    // a visible silhouette response without runaway displacement.
    let wind_wave_scale = 1.0
        + (gust / byroredux_core::ecs::components::groundcover::MAX_WIND_SPEED)
            .clamp(0.0, 1.0)
            * 0.5;
    let weather_scroll = [
        wind_direction[0]
            * gust
            * byroredux_core::ecs::components::water::WEATHER_SCROLL_PER_BU_PER_S,
        wind_direction[1]
            * gust
            * byroredux_core::ecs::components::water::WEATHER_SCROLL_PER_BU_PER_S,
    ];
    let Some(wq) = world.query::<WaterPlane>() else {
        return;
    };
    let fq = world.query::<WaterFlow>();
    let rq = world.query::<RippleEvent>();
    for (entity, plane) in wq.iter() {
        let Some(idx) = draw_commands.iter().position(|c| c.entity_id == entity) else {
            // Entity has WaterPlane but no DrawCommand was emitted —
            // typically because the cell loader spawned the water
            // entity but the mesh wasn't yet uploaded, or the
            // entity is frustum-culled out of the regular emit
            // path. Skip silently.
            continue;
        };
        draw_commands[idx].is_water = true;

        let flow = fq.as_ref().and_then(|q| q.get(entity).copied());
        let ripple = rq
            .as_ref()
            .and_then(|q| q.get(entity).copied())
            .map(|event| {
                [
                    event.position[0],
                    event.position[2],
                    event.intensity.clamp(0.0, 1.0),
                    4.0 + event.intensity.clamp(0.0, 1.0) * 20.0,
                ]
            })
            .unwrap_or([0.0; 4]);
        let (flow_dir, flow_speed) = match flow {
            Some(f) => (f.direction, f.speed),
            None => ([1.0, 0.0, 0.0], 0.0),
        };

        // ABI: matches `WaterParams` in `shaders/water.frag`.
        // Each vec4 maps to one std140 slot — see
        // `crates/renderer/src/vulkan/water.rs::GpuWaterParams` for
        // the layout contract.
        let mut mat = plane.material;
        // GNAM daytime/nighttime related-water variants are kept on the
        // canonical material and blended here at the same climate-authored
        // TOD factor used by the sky/fog system. This keeps the GPU ABI
        // unchanged while avoiding a hard 06:00/18:00 water transition.
        mat.shallow_color =
            lerp_color(mat.day_shallow_color, mat.night_shallow_color, night_factor);
        mat.deep_color = lerp_color(mat.day_deep_color, mat.night_deep_color, night_factor);
        mat.fog_near = mat.day_fog_near + (mat.night_fog_near - mat.day_fog_near) * night_factor;
        mat.fog_far = mat.day_fog_far + (mat.night_fog_far - mat.day_fog_far) * night_factor;
        mat.reflection_tint = lerp_color(
            mat.day_reflection_tint,
            mat.night_reflection_tint,
            night_factor,
        );
        mat.reflection_tint = mat
            .reflection_tint
            .map(|channel| channel * mat.reflection_hdr_multiplier.max(0.0));
        // Starfield's flow-map tile scale is a visual UV-rate control, not a
        // physics velocity. Keep the canonical `WaterFlow` speed bounded and
        // scale only the authored wave scroll vectors here.
        let flowmap_scale = if mat.flowmap_scale.is_finite() && mat.flowmap_scale > 0.0 {
            mat.flowmap_scale.clamp(0.05, 8.0)
        } else {
            1.0
        };
        // Skyrim-family WATR.NAM1 supplies an angular velocity vector. Rotate
        // only the authored surface scroll here; atmospheric weather wind is
        // an external world-space field and must keep its direction shared
        // with SpeedTree sway. A zero/invalid rate is the legacy no-op.
        let angular_rate = if mat.angular_velocity.is_finite() {
            mat.angular_velocity.clamp(-32.0, 32.0)
        } else {
            0.0
        };
        let (sin_angle, cos_angle) = (angular_rate * time_secs).sin_cos();
        let rotate_scroll = |scroll: [f32; 2], scale: f32| {
            let x = scroll[0] * scale;
            let y = scroll[1] * scale;
            [x * cos_angle - y * sin_angle, x * sin_angle + y * cos_angle]
        };
        let authored_scroll_a = rotate_scroll(mat.scroll_a, flowmap_scale);
        let authored_scroll_b = rotate_scroll(mat.scroll_b, flowmap_scale);
        let authored_scroll_c = rotate_scroll(mat.scroll_c, flowmap_scale);
        let params = GpuWaterParams {
            timing: [
                time_secs,
                plane.kind as u8 as f32,
                mat.foam_strength,
                mat.ior,
            ],
            flow: [flow_dir[0], flow_dir[1], flow_dir[2], flow_speed],
            shallow: [
                mat.shallow_color[0],
                mat.shallow_color[1],
                mat.shallow_color[2],
                mat.fog_near,
            ],
            deep: [
                mat.deep_color[0],
                mat.deep_color[1],
                mat.deep_color[2],
                mat.fog_far,
            ],
            scroll: [
                authored_scroll_a[0] + weather_scroll[0],
                authored_scroll_a[1] + weather_scroll[1],
                authored_scroll_b[0] + weather_scroll[0] * 0.65,
                authored_scroll_b[1] + weather_scroll[1] * 0.65,
            ],
            scroll_c: [
                authored_scroll_c[0] + weather_scroll[0] * 0.45,
                authored_scroll_c[1] + weather_scroll[1] * 0.45,
                mat.underwater_fog_near,
                mat.underwater_fog_far,
            ],
            tune: [
                mat.uv_scale_a,
                mat.uv_scale_b,
                mat.shoreline_width,
                mat.wave_amplitude * wind_wave_scale, // #2240 — WATR + shared wind
            ],
            misc: [
                mat.fresnel_f0,
                mat.wave_frequency, // #2240 — WATR-authored, consumed by water.frag
                GpuWaterParams::pack_normal_index(mat.normal_map_index),
                mat.sun_specular_power,
            ],
            tint_reflect: [
                mat.reflection_tint[0],
                mat.reflection_tint[1],
                mat.reflection_tint[2],
                mat.reflectivity,
            ],
            noise_indices: [
                mat.noise_map_indices[0],
                mat.noise_map_indices[1],
                mat.noise_map_indices[2],
                mat.opacity.to_bits(),
            ],
            detail: [
                mat.uv_scale_c,
                mat.noise_amplitude_scales[0],
                mat.noise_amplitude_scales[1],
                mat.noise_amplitude_scales[2],
            ],
            // `.y` carries Skyrim WATR.FNAM's Blend Normals gate; keeping it
            // in this existing vec4 preserves the fixed water UBO ABI.
            noise_falloff: [
                mat.noise_falloff,
                if mat.blend_normals { 1.0 } else { 0.0 },
                mat.roughness.clamp(0.0, 1.0),
                mat.specular_radius.max(0.0),
            ],
            normal_falloff: [
                mat.normal_falloff[0],
                mat.normal_falloff[1],
                mat.normal_falloff[2],
                pack_rain_controls(mat.rain_velocity, mat.rain_falloff, mat.rain_dampener),
            ],
            displacement: [
                mat.displacement[0],
                mat.displacement[1],
                mat.displacement[2],
                mat.rain_start_size.max(0.0),
            ],
            depth: mat.depth_weights,
            effects: [
                mat.effect_controls[0],
                mat.effect_controls[1],
                mat.effect_controls[2],
                mat.effect_controls[3] * mat.specular_magnitude,
            ],
            absorption: [
                mat.absorption_ranges[0],
                mat.absorption_ranges[1],
                mat.absorption_ranges[2],
                precipitation
                    * if mat.rain_response.is_finite() {
                        mat.rain_response.clamp(0.0, 4.0)
                    } else {
                        1.0
                    },
            ],
            concentration: mat.concentration,
            ripple,
            underwater: [
                mat.underwater_color[0],
                mat.underwater_color[1],
                mat.underwater_color[2],
                mat.underwater_fog_amount,
            ],
        };
        water_commands.push(WaterDrawCommand {
            mesh_handle: draw_commands[idx].mesh_handle,
            instance_index: idx as u32,
            params,
        });
        // Silence WaterKind-unused warning on builds where the
        // enum is only consumed by the f32 cast above.
        let _ = WaterKind::Calm;
    }
}
