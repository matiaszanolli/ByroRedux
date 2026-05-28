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

/// LIGH `radius` → renderer effective range multiplier.
///
/// Bethesda's LIGH `radius` is a "design value" where the light is
/// fully effective. The runtime contributes visible light beyond that
/// (a 1024 BU torch is ~10-30% at d=r, fading to ~0 by d=3-4r). This
/// constant captures the engine-policy choice for how far past the
/// authored radius our renderer extends the visible contribution. Set
/// in this file (the translator) — NOT in the shader, which consumes
/// only the post-translation `effective_range`. Same separation as the
/// BGSM → PBR translation: source-format quirks resolved at the
/// boundary, renderer-side code stays format-agnostic.
///
/// `2.5` — tuned against densely-lit FO4 interiors (Institute,
/// Bioscience) where smaller authored radii need a narrower reach to
/// preserve directional feel, vs Skyrim's larger radii that work fine
/// at this multiplier. Pre-2026-05-24 audit: shader hardcoded the
/// `1/(1 + 0.01d)` linear absolute-distance term plus a `radius * 4.0`
/// cull; that mixed source data with engine policy in the shader.
pub const LIGHT_RANGE_EXTENSION: f32 = 2.5;

/// LIGH `falloff_exponent` default applied when the source field is
/// `0.0` (the engine sentinel for "unset" — pre-Skyrim LIGH records
/// without the field, or NIF-direct lights). `1.0` reproduces the
/// near-linear shape Skyrim authors as default. Same translator
/// principle: defaults applied CPU-side so the shader never sees a
/// sentinel value.
pub const FALLOFF_EXPONENT_DEFAULT: f32 = 1.0;

/// Collect both the cell directional light and all placed point lights
/// into `gpu_lights`, appending — the caller is responsible for
/// clearing the Vec before invoking.
///
/// **Order matters** for the renderer's per-frame upload contract:
/// directional first (slot 0 if present), then point lights. The
/// shader-side cluster builder doesn't care about ordering, but the
/// once-per-session info log below references the first three slots,
/// so a re-order would change diagnostic output.
pub(super) fn collect_lights(world: &World, gpu_lights: &mut Vec<byroredux_renderer::GpuLight>) {
    // Cell directional light. For interior cells the XCLL directional
    // acts as a subtle fill light (not a physical sun), so we scale it
    // down to avoid hard shadow leakage through unsealed interior walls.
    if let Some(cell_lit) = world.try_resource::<CellLightingRes>() {
        let sun_intensity = world
            .try_resource::<SkyParamsRes>()
            .map(|sky| sky.sun_intensity)
            .unwrap_or(SUN_INTENSITY_PEAK);
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
