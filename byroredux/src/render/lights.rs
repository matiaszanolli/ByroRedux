//! Light collection — extracted from `build_render_data` per #1115.
//!
//! Appends to the caller-owned `gpu_lights` Vec in two passes:
//!   1. Directional fill light (XCLL-authored, interior or exterior).
//!   2. Placed point lights (LIGH records, animated dimmer/intensity/radius).
//!
//! This module is the **translation layer** between game-format light
//! data (Bethesda LIGH records with their own `radius` semantic and
//! `falloff_exponent` field) and the renderer's standard light contract.
//! The shader consumes only the translated fields — no LIGH-specific
//! knowledge leaks into GLSL. Same directive as the BGSM → PBR
//! translation in `merge_bgsm_into_mesh`; see
//! `feedback_format_translation.md`.

use byroredux_core::ecs::{GlobalTransform, LightSource, World};

use crate::components::{CellLightingRes, SkyParamsRes};

use super::{compute_directional_upload, SUN_INTENSITY_PEAK};

/// LIGH `radius` → renderer **cull radius** multiplier.
///
/// Bethesda's LIGH `radius` is a "design value": the light is fully
/// effective at `d=0`, reads ~10–30% at `d=radius`, and fades to 0
/// shortly beyond. `effective_range = radius × LIGHT_RANGE_EXTENSION`
/// is the **cull radius** the shader receives in `position_radius.w` —
/// the distance at which attenuation reaches exactly 0, NOT the
/// authored radius.
///
/// **Why 2.0** (REND-#1451): this mirrors OpenMW's Gamebryo-lineage
/// light model, which fades a light to zero at exactly `2 × radius`
/// "to diminish pop-in" (an anti-pop-in cull window on top of the
/// physical falloff — see
/// `reference/openmw/files/shaders/lib/light/lighting_util.glsl`
/// `lcalcIllumination`). So `2.0` is correct as the **cull boundary**.
///
/// The brightness AT the authored radius is governed by the shader's
/// `pointSpotAtten` (triangle.frag), NOT by this multiplier: a physical
/// near-zone falloff keyed to the authored radius (`knee = kneeFrac ×
/// effective_range`, default `kneeFrac = 0.5 = 1/2.0`) multiplied by a
/// soft cull window from the authored radius out to `effective_range`.
/// This replaced the pre-fix model that used ONLY the cull window as
/// the entire attenuation — which read 75% at the authored radius
/// (`ratio=0.5, window=0.75`), the bright near-zone ring in Lonesome
/// Road's Ulysses Temple. The `kneeFrac` is runtime-tunable via the
/// `light.atten` console command for the controlled bench; once a value
/// is settled it can be baked as the shader default.
///
/// History: was `2.5` (tuned for FO4 dense interiors); dropped to `2.0`
/// alongside the AMBIENT_FILL additive→max() fix on 2026-06-03
/// (REND-#1452). Keeping `2.0` also preserves RT-GI reach — bounce
/// paths need light to survive to ~2× the authored radius, which the
/// cull window now provides smoothly instead of a hard cutoff.
pub const LIGHT_RANGE_EXTENSION: f32 = 2.0;

/// LIGH `falloff_exponent` default applied when the source field is
/// `0.0` (the engine sentinel for "unset" — pre-Skyrim LIGH records
/// without the field, or NIF-direct lights). `1.0` reproduces the
/// near-linear shape Skyrim authors as default. Same translator
/// principle: defaults applied CPU-side so the shader never sees a
/// sentinel value.
pub const FALLOFF_EXPONENT_DEFAULT: f32 = 1.0;

/// PERF-D5-NEW-02 / #1800 — cheap CPU-side "how much does this light
/// matter for one-bounce GI" proxy: sum of the light's RGB channels
/// (already scaled by `dimmer × intensity` at translation time) times
/// its effective range. Not physically exact — it has no idea where any
/// given GI hit point actually is — but it's a stable, frame-wide
/// ordering that favors bright, far-reaching lights over dim or
/// tightly-clamped ones, which is exactly what `giHitIrradiance`'s fixed
/// `GI_HIT_LIGHT_CAP`-sized prefix scan needs to be biased toward.
fn gi_priority_score(light: &byroredux_renderer::GpuLight) -> f32 {
    let [r, g, b, _] = light.color_type;
    let radius = light.position_radius[3];
    (r + g + b) * radius
}

/// Collect both the cell directional light and all placed point lights
/// into `gpu_lights`, appending — the caller is responsible for
/// clearing the Vec before invoking.
///
/// **Order matters** for the renderer's per-frame upload contract:
/// directional first (slot 0 if present), then point lights sorted by
/// descending [`gi_priority_score`] (#1800 — see the sort call below for
/// why). The shader-side cluster builder doesn't care about ordering
/// (it indexes lights by ID from its own per-cluster lists), but
/// `giHitIrradiance`'s fixed-prefix GI scan does, and the once-per-session
/// info log below references the first three slots post-sort.
pub(super) fn collect_lights(world: &World, gpu_lights: &mut Vec<byroredux_renderer::GpuLight>) {
    // Cell directional light. For interior cells the XCLL directional
    // acts as a subtle fill light (not a physical sun), so we scale it
    // down to avoid hard shadow leakage through unsealed interior walls.
    // Snapshot `sun_intensity` BEFORE acquiring `CellLightingRes` so the
    // two resource locks are never held simultaneously. The weather path
    // touches the same pair in the opposite (Sky→Cell) order, so nesting
    // them here (Cell→Sky) is a cross-thread ABBA deadlock risk under the
    // parallel scheduler. See invariant #4, #313, and #1410 (the global
    // BYRO_LOCK_ORDER_CHECK detector flags this exact pair).
    let sun_intensity = world
        .try_resource::<SkyParamsRes>()
        .map(|sky| sky.sun_intensity)
        .unwrap_or(SUN_INTENSITY_PEAK);
    if let Some(cell_lit) = world.try_resource::<CellLightingRes>() {
        let (dir_color, dir_radius) = compute_directional_upload(
            &cell_lit.directional_color,
            cell_lit.is_interior,
            sun_intensity,
        );
        gpu_lights.push(byroredux_renderer::GpuLight {
            position_radius: [0.0, 0.0, 0.0, dir_radius],
            color_type: [dir_color[0], dir_color[1], dir_color[2], 2.0],
            direction_angle: [
                cell_lit.directional_dir[0],
                cell_lit.directional_dir[1],
                cell_lit.directional_dir[2],
                0.0,
            ],
            // Directional lights aren't distance-attenuated; falloff
            // is a no-op for them but the shader still reads `params.x`
            // unconditionally — pass `0.0` (the "use default" sentinel)
            // so the shader's directional branch ignores it cleanly.
            params: [0.0, 0.0, 0.0, 0.0],
        });
    }

    // PERF-D5-NEW-02 / #1800 — `giHitIrradiance` (lighting.glsl) only
    // scans the first `GI_HIT_LIGHT_CAP` (8) entries of this array in
    // upload order for the one-bounce GI shadow-ray pass; the
    // directional light (if present) is always exactly one entry and
    // always pushed first, so everything from here on is the
    // point-light suffix that needs priority-sorting below.
    let directional_count = gpu_lights.len();

    // Placed point lights from LIGH records. Read-only — no write
    // needed on either component. Previously used query_2_mut (#290 P4-04).
    let light_gt_q = world.query::<GlobalTransform>();
    let light_q = world.query::<LightSource>();
    if let (Some(tq), Some(lq)) = (light_gt_q, light_q) {
        for (entity, light) in lq.iter() {
            if let Some(t) = tq.get(entity) {
                // #983 — `dimmer` and `intensity` are mutated by the
                // animation system when the source NIF carries
                // `NiLight{Dimmer,Intensity}Controller`. The product
                // is the per-frame multiplicative scalar on the
                // diffuse color; the renderer doesn't see the curves
                // directly, just the resolved factor here. `radius`
                // is similarly animated by `NiLightRadiusController`
                // and the value already sits on `light.radius` from
                // the same code path.
                let scale = light.dimmer * light.intensity;
                // ── LIGH → standard light translation ──────────────
                // Pre-compute the renderer-standard fields here so the
                // shader consumes ready-to-use values. Raw LIGH inputs
                // (`light.radius`, `light.falloff_exponent`) never
                // reach GLSL — only the post-translation `effective_
                // range` and `falloff_shape`.
                let effective_range = light.radius * LIGHT_RANGE_EXTENSION;
                let falloff_shape = if light.falloff_exponent > 0.0 {
                    light.falloff_exponent
                } else {
                    FALLOFF_EXPONENT_DEFAULT
                };
                gpu_lights.push(byroredux_renderer::GpuLight {
                    position_radius: [
                        t.translation.x,
                        t.translation.y,
                        t.translation.z,
                        effective_range,
                    ],
                    color_type: [
                        light.color[0] * scale,
                        light.color[1] * scale,
                        light.color[2] * scale,
                        0.0,
                    ], // 0 = point
                    direction_angle: [0.0, 0.0, 0.0, 0.0],
                    // `params.x` = standardized attenuation curve shape
                    // (defaulted CPU-side). Shader uses verbatim with
                    // no sentinel handling.
                    params: [falloff_shape, 0.0, 0.0, 0.0],
                });
            }
        }
    }

    // PERF-D5-NEW-02 / #1800 — the one-bounce GI hit-irradiance pass
    // (`giHitIrradiance` in lighting.glsl) evaluates only the first
    // `GI_HIT_LIGHT_CAP` entries of this same array, in whatever order
    // they land here — the shader has no per-hit-point light selection,
    // it just walks a fixed prefix. Left as arbitrary ECS sparse-set
    // iteration order, that prefix has nothing to do with which lights
    // actually matter for GI: a cell with, say, 20 point lights would
    // permanently exclude 12 of them from the bounce term (and could
    // flicker across cell reloads as ECS iteration order shuffles which
    // 8 "win"), while still paying up to 8 shadow-ray traces against
    // lights that might be nowhere near the hit point.
    //
    // Sorting the point-light suffix once per frame by descending
    // `gi_priority_score` (a cheap CPU-side "intensity × radius" proxy)
    // makes "first 8" approximate "8 most influential" scene-wide,
    // without touching the shader's per-hit ray-query logic or the
    // primary-fragment path's clustered culling (which indexes lights
    // by ID from its own per-cluster lists and doesn't care about array
    // order). The directional light, if present, is never part of this
    // sort — it stays pinned at index 0.
    gpu_lights[directional_count..]
        .sort_by(|a, b| gi_priority_score(b).total_cmp(&gi_priority_score(a)));

    // Log light count once per session.
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        static LOGGED: AtomicBool = AtomicBool::new(false);
        if !LOGGED.swap(true, Ordering::Relaxed) {
            log::info!(
                "Lights collected: {} (first 3: {:?})",
                gpu_lights.len(),
                gpu_lights
                    .iter()
                    .take(3)
                    .map(|l| (l.position_radius, l.color_type))
                    .collect::<Vec<_>>(),
            );
        }
    }
}

#[cfg(test)]
mod interior_sun_gate_tests {
    //! Regression guard for #1282 — interior sun-shaft leak.
    //!
    //! The architecture has three independent gates that together
    //! prevent the exterior sun from creating a hard-edged light shaft
    //! on an interior cell's floor:
    //!
    //!   1. `compute_directional_upload(_, is_interior=true, _)` returns
    //!      0.6× scale of `cell_lit.directional_color` (independent of
    //!      `sun_intensity` — the XCLL value is the fill source, NOT
    //!      the weather sun) and the `radius = -1` sentinel.
    //!   2. The shader (`triangle.frag::isInteriorFill = radius < 0.0`)
    //!      treats the directional as an ISOTROPIC fill — no Lambert,
    //!      no N·L term — so the surface receives `directional × 0.24 ×
    //!      albedo` uniformly with no shadow boundary possible.
    //!   3. The RT shadow-ray loop skips the directional when
    //!      `isInteriorFill` is true (no `vkRayQuery` cast for
    //!      sealed-wall protection).
    //!
    //! Plus a fourth gate at the resource level: `weather_system`
    //! mutations to `cell_lit.directional_color` are themselves gated
    //! on `!is_interior` (see `systems/weather.rs:561, 213`), so a
    //! prior exterior load's sun colour can't bleed into the next
    //! interior's XCLL via `weather_system` re-running on the persisted
    //! `CellLightingRes`.
    //!
    //! This module pins the FIRST gate at the light-list-assembly
    //! integration level. The unit-level coverage of
    //! `compute_directional_upload` lives in
    //! [`super::super::directional_upload_tests`] (interior path:
    //! `interior_uses_fixed_fill_independent_of_sun_intensity`); this
    //! file complements it by verifying the full `collect_lights`
    //! pipeline respects `is_interior` even when a `SkyParamsRes` from
    //! a prior exterior load persists with a high `sun_intensity` —
    //! the #1199 scenario the original issue body called out.

    use super::*;
    use crate::components::{CellLightingRes, SkyParamsRes};

    fn interior_cell_lit(directional_color: [f32; 3]) -> CellLightingRes {
        CellLightingRes {
            ambient: [0.1, 0.1, 0.1],
            directional_color,
            directional_dir: [0.0, -1.0, 0.0],
            is_interior: true,
            fog_color: [0.05, 0.06, 0.08],
            fog_near: 64.0,
            fog_far: 4000.0,
            directional_fade: None,
            fog_clip: None,
            fog_power: None,
            fog_far_color: None,
            fog_max: None,
            light_fade_begin: None,
            light_fade_end: None,
            directional_ambient: None,
            specular_color: None,
            specular_alpha: None,
            fresnel_power: None,
        }
    }

    fn exterior_cell_lit(directional_color: [f32; 3]) -> CellLightingRes {
        let mut lit = interior_cell_lit(directional_color);
        lit.is_interior = false;
        lit
    }

    fn full_sun_sky_params() -> SkyParamsRes {
        // Simulates a `SkyParamsRes` left over from a prior exterior
        // worldspace load: full daytime sun, full intensity. Pre-#1282
        // the worry was this resource leaking into interior lighting.
        SkyParamsRes {
            zenith_color: [0.3, 0.5, 0.9],
            horizon_color: [0.8, 0.8, 0.9],
            lower_color: [0.4, 0.4, 0.45],
            sun_direction: [0.5, -0.8, 0.3],
            sun_color: [1.0, 0.95, 0.85],
            sun_size: 0.02,
            sun_intensity: super::super::SUN_INTENSITY_PEAK, // 4.0 = daytime
            sun_angular_radius: 0.020,
            is_exterior: true,
            cloud_tile_scale: 0.0,
            cloud_texture_index: 0,
            sun_texture_index: 0,
            cloud_tile_scale_1: 0.0,
            cloud_texture_index_1: 0,
            cloud_tile_scale_2: 0.0,
            cloud_texture_index_2: 0,
            cloud_tile_scale_3: 0.0,
            cloud_texture_index_3: 0,
            current_dalc_cube: None,
        }
    }

    /// The headline regression guard: an interior cell with
    /// `SkyParamsRes` present (high sun_intensity, simulating a prior
    /// exterior load) must produce a SCALED + UNSHADOWED directional —
    /// not a full-intensity sun. Pre-gate the XCLL directional would
    /// have been pushed at full strength because the interior arm
    /// wouldn't fire.
    #[test]
    fn interior_with_persistent_sky_params_does_not_emit_full_sun() {
        let mut world = World::new();
        world.insert_resource(interior_cell_lit([0.8, 0.7, 0.5]));
        world.insert_resource(full_sun_sky_params());

        let mut lights = Vec::new();
        collect_lights(&world, &mut lights);

        assert_eq!(
            lights.len(),
            1,
            "interior with no LIGH refs must emit exactly one GpuLight (the directional fill)"
        );
        let l = &lights[0];
        assert!(
            (l.color_type[3] - 2.0).abs() < 1e-6,
            "GpuLight type slot must be 2.0 (directional marker), got {}",
            l.color_type[3]
        );
        assert!(
            (l.position_radius[3] - (-1.0)).abs() < 1e-6,
            "interior fill must use radius=-1 sentinel so the shader's \
             `isInteriorFill` branch fires (skipping RT shadow + using \
             isotropic fill instead of N·L Lambert). got {}",
            l.position_radius[3]
        );
        // Color must be SCALED — 0.6× the authored directional_color,
        // INDEPENDENT of sun_intensity (the XCLL value is the fill source).
        // 0.8 * 0.6 = 0.48; 0.7 * 0.6 = 0.42; 0.5 * 0.6 = 0.30.
        assert!(
            (l.color_type[0] - 0.48).abs() < 1e-5,
            "interior R must be 0.6× authored (= 0.48), got {}",
            l.color_type[0]
        );
        assert!(
            (l.color_type[1] - 0.42).abs() < 1e-5,
            "interior G must be 0.6× authored (= 0.42), got {}",
            l.color_type[1]
        );
        assert!(
            (l.color_type[2] - 0.30).abs() < 1e-5,
            "interior B must be 0.6× authored (= 0.30), got {}",
            l.color_type[2]
        );
    }

    /// Parity: an EXTERIOR cell with the same SkyParamsRes pushes the
    /// directional at the FULL sun_intensity ramp (no scale-down, no
    /// sentinel radius). Pins that the gate hasn't slipped to "always
    /// scaled" — exterior cells must still receive proper sun.
    #[test]
    fn exterior_with_full_sun_emits_unshadowed_full_directional() {
        let mut world = World::new();
        world.insert_resource(exterior_cell_lit([0.8, 0.7, 0.5]));
        world.insert_resource(full_sun_sky_params());

        let mut lights = Vec::new();
        collect_lights(&world, &mut lights);

        assert_eq!(lights.len(), 1);
        let l = &lights[0];
        assert!(
            (l.position_radius[3] - 0.0).abs() < 1e-6,
            "exterior radius must be 0 (standard directional, shader \
             casts RT shadow), got {}",
            l.position_radius[3]
        );
        // sun_intensity == PEAK → ramp == 1.0 → color = authored as-is.
        assert!((l.color_type[0] - 0.8).abs() < 1e-5);
        assert!((l.color_type[1] - 0.7).abs() < 1e-5);
        assert!((l.color_type[2] - 0.5).abs() < 1e-5);
    }

    /// Interior without SkyParamsRes (no prior exterior load): same
    /// behavior as the persistent-SkyParams case — `sun_intensity`
    /// falls back to `SUN_INTENSITY_PEAK` but the interior arm ignores
    /// it anyway. Pins that the absent-resource fallback doesn't
    /// accidentally promote the cell to "exterior" semantics.
    #[test]
    fn interior_without_sky_params_still_uses_fixed_fill() {
        let mut world = World::new();
        world.insert_resource(interior_cell_lit([1.0, 1.0, 1.0]));
        // No SkyParamsRes — fresh-boot or interior-only session.

        let mut lights = Vec::new();
        collect_lights(&world, &mut lights);

        assert_eq!(lights.len(), 1);
        let l = &lights[0];
        assert!((l.position_radius[3] - (-1.0)).abs() < 1e-6);
        // 1.0 × 0.6 = 0.6.
        assert!((l.color_type[0] - 0.6).abs() < 1e-5);
        assert!((l.color_type[1] - 0.6).abs() < 1e-5);
        assert!((l.color_type[2] - 0.6).abs() < 1e-5);
    }

    /// No `CellLightingRes` at all (engine pre-cell-load) — no
    /// directional emitted. Guards against a future code path that
    /// would conjure a directional from `SkyParamsRes` alone.
    #[test]
    fn no_cell_lighting_emits_no_directional() {
        let mut world = World::new();
        world.insert_resource(full_sun_sky_params());

        let mut lights = Vec::new();
        collect_lights(&world, &mut lights);

        assert_eq!(
            lights.len(),
            0,
            "directional must come from CellLightingRes — SkyParamsRes \
             alone must NOT conjure a sun light. {} lights pushed.",
            lights.len()
        );
    }
}

/// PERF-D5-NEW-02 / #1800 — `giHitIrradiance` (lighting.glsl) only scans
/// the first `GI_HIT_LIGHT_CAP` entries of the uploaded light array;
/// `collect_lights` must order the point-light suffix by descending
/// [`gi_priority_score`] so that fixed prefix approximates "the most
/// influential lights" rather than arbitrary ECS iteration order.
#[cfg(test)]
mod gi_light_priority_tests {
    use super::*;
    use byroredux_core::ecs::LightSource;

    #[test]
    fn priority_score_favors_brighter_and_farther_reaching_lights() {
        let dim_small = byroredux_renderer::GpuLight {
            position_radius: [0.0, 0.0, 0.0, 100.0],
            color_type: [0.05, 0.05, 0.05, 0.0],
            direction_angle: [0.0; 4],
            params: [1.0, 0.0, 0.0, 0.0],
        };
        let bright_large = byroredux_renderer::GpuLight {
            position_radius: [0.0, 0.0, 0.0, 1000.0],
            color_type: [0.9, 0.8, 0.7, 0.0],
            direction_angle: [0.0; 4],
            params: [1.0, 0.0, 0.0, 0.0],
        };
        assert!(
            gi_priority_score(&bright_large) > gi_priority_score(&dim_small),
            "a bright, far-reaching light must score higher than a dim, \
             tightly-clamped one"
        );
    }

    /// Same brightness, different radius: the farther-reaching light
    /// must win — it's a better candidate for illuminating an arbitrary
    /// GI hit point.
    #[test]
    fn priority_score_orders_by_radius_at_equal_brightness() {
        let near = byroredux_renderer::GpuLight {
            position_radius: [0.0, 0.0, 0.0, 200.0],
            color_type: [0.5, 0.5, 0.5, 0.0],
            direction_angle: [0.0; 4],
            params: [1.0, 0.0, 0.0, 0.0],
        };
        let far = byroredux_renderer::GpuLight {
            position_radius: [0.0, 0.0, 0.0, 800.0],
            color_type: [0.5, 0.5, 0.5, 0.0],
            direction_angle: [0.0; 4],
            params: [1.0, 0.0, 0.0, 0.0],
        };
        assert!(gi_priority_score(&far) > gi_priority_score(&near));
    }

    fn spawn_point_light(world: &mut World, pos: [f32; 3], color: [f32; 3], radius: f32) {
        let e = world.spawn();
        world.insert(
            e,
            GlobalTransform::new(
                byroredux_core::math::Vec3::new(pos[0], pos[1], pos[2]),
                byroredux_core::math::Quat::IDENTITY,
                1.0,
            ),
        );
        world.insert(
            e,
            LightSource {
                radius,
                color,
                ..Default::default()
            },
        );
    }

    /// Integration-level regression: three point lights inserted in an
    /// order that (pre-fix) would have survived verbatim as ECS
    /// iteration order — dimmest first, brightest last — must come out
    /// of `collect_lights` sorted brightest/farthest-reaching first.
    /// This is the exact bug: pre-fix, `giHitIrradiance`'s fixed 8-light
    /// prefix would have hit the dim light first and the bright one last
    /// (or not at all, in a >8-light cell), regardless of which one
    /// actually matters for GI.
    #[test]
    fn collect_lights_sorts_point_lights_brightest_first() {
        let mut world = World::new();
        // Insertion order: dim, medium, bright — deliberately the
        // opposite of the desired output order.
        spawn_point_light(&mut world, [0.0, 0.0, 0.0], [0.05, 0.05, 0.05], 100.0);
        spawn_point_light(&mut world, [10.0, 0.0, 0.0], [0.4, 0.4, 0.4], 400.0);
        spawn_point_light(&mut world, [20.0, 0.0, 0.0], [0.9, 0.9, 0.9], 900.0);

        let mut lights = Vec::new();
        collect_lights(&world, &mut lights);

        assert_eq!(lights.len(), 3, "all three point lights must be collected");
        let scores: Vec<f32> = lights.iter().map(gi_priority_score).collect();
        assert!(
            scores.windows(2).all(|w| w[0] >= w[1]),
            "point lights must be sorted by descending gi_priority_score, got {scores:?}"
        );
        // The brightest/farthest-reaching light (authored radius 900,
        // color 0.9 — effective range = 900 * LIGHT_RANGE_EXTENSION)
        // must land first — inside GI_HIT_LIGHT_CAP even in a cell with
        // more lights than the cap.
        let expected_effective_range = 900.0 * super::LIGHT_RANGE_EXTENSION;
        assert!(
            (lights[0].position_radius[3] - expected_effective_range).abs() < 1e-3,
            "brightest light must sort first, got effective range {} (expected {})",
            lights[0].position_radius[3],
            expected_effective_range,
        );
    }

    /// The directional light must stay pinned at index 0 regardless of
    /// how bright the point lights are — it is never part of the
    /// priority sort (see the doc comment on the sort call site).
    #[test]
    fn directional_light_stays_pinned_at_index_zero() {
        use crate::components::CellLightingRes;

        let mut world = World::new();
        world.insert_resource(CellLightingRes {
            ambient: [0.1, 0.1, 0.1],
            directional_color: [0.8, 0.7, 0.5],
            directional_dir: [0.0, -1.0, 0.0],
            is_interior: false,
            fog_color: [0.05, 0.06, 0.08],
            fog_near: 64.0,
            fog_far: 4000.0,
            directional_fade: None,
            fog_clip: None,
            fog_power: None,
            fog_far_color: None,
            fog_max: None,
            light_fade_begin: None,
            light_fade_end: None,
            directional_ambient: None,
            specular_color: None,
            specular_alpha: None,
            fresnel_power: None,
        });
        // A point light far brighter than the directional fill.
        spawn_point_light(&mut world, [5.0, 0.0, 0.0], [1.0, 1.0, 1.0], 5000.0);

        let mut lights = Vec::new();
        collect_lights(&world, &mut lights);

        assert_eq!(lights.len(), 2);
        assert!(
            (lights[0].color_type[3] - 2.0).abs() < 1e-6,
            "index 0 must always be the directional (type 2.0), regardless \
             of point-light brightness — got type {}",
            lights[0].color_type[3]
        );
    }
}
