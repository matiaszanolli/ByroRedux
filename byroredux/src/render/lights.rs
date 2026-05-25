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
