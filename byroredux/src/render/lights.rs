//! Light collection — extracted from `build_render_data` per #1115.
//!
//! Appends to the caller-owned `gpu_lights` Vec in two passes:
//!   1. Directional light (XCLL-authored interior key or exterior sun).
//!   2. Placed point lights (LIGH records, animated dimmer/intensity/radius).
//!
//! This module is the **translation layer** between game-format light
//! data (Bethesda LIGH records with their own `radius` semantic and
//! `falloff_exponent` field) and the renderer's standard light contract.
//! The shader consumes only the translated fields — no LIGH-specific
//! knowledge leaks into GLSL. Same directive as the BGSM → PBR
//! translation in `merge_external_material`; see
//! `feedback_format_translation.md`.

use byroredux_core::ecs::{GlobalTransform, LightKind, LightSource, World};
use byroredux_core::lighting::{AttenuationModel, VisibilityMask};

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
pub const LIGHT_RANGE_EXTENSION: f32 = byroredux_core::lighting::LEGACY_LIGHT_CULL_RANGE_MULTIPLIER;

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
///
/// `sort_scratch` is the caller-owned decorate-sort buffer (#2172 /
/// PERF-D1-02); its contents on entry are irrelevant — it is cleared
/// here — but its *capacity* is what the caller is preserving across
/// frames. Same contract as `gpu_lights` itself and the `AnimScratch`
/// buffers in `systems/animation.rs`.
pub(super) fn collect_lights(
    world: &World,
    gpu_lights: &mut Vec<byroredux_renderer::GpuLight>,
    fog_volumes: &[byroredux_renderer::GpuFogVolume],
    sort_scratch: &mut Vec<(f32, byroredux_renderer::GpuLight)>,
) {
    // Cell directional light. Exterior cells emit the weather/TOD sun;
    // interiors emit their separate XCLL key light after applying the authored
    // Directional Fade scalar. The six-face XCLL cube is ambient irradiance and
    // does not replace this source.
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
        let (dir_color, dir_kind) = compute_directional_upload(
            &cell_lit.directional_color,
            cell_lit.is_interior,
            sun_intensity,
            cell_lit.directional_fade,
        );
        if dir_color.iter().any(|channel| *channel > 0.0) {
            let (gpu_type, visibility) = match dir_kind {
                // XCLL's interior directional is the legacy engine's
                // unshadowed, normal-independent room fill. Type 3 keeps
                // that semantic explicit without the former radius=-1
                // sentinel; an empty visibility mask guarantees it cannot
                // enter the RT reservoir/shadow path.
                LightKind::Ambient => (3.0, VisibilityMask::NONE),
                LightKind::Directional => (2.0, VisibilityMask::FULL),
                _ => unreachable!("cell directional translation emitted a local light kind"),
            };
            gpu_lights.push(byroredux_renderer::GpuLight {
                position_radius: [0.0, 0.0, 0.0, 0.0],
                color_type: [dir_color[0], dir_color[1], dir_color[2], gpu_type],
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
                // z = explicit visibility policy. Exterior directionals trace
                // the full scene; interior ambient fill traces nothing.
                params: [
                    0.0,
                    0.0,
                    visibility.bits() as f32,
                    AttenuationModel::LegacySoftRange as u8 as f32,
                ],
            });
        }
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
                // and the value already sits on `light.emitter.range`
                // from the same code path.
                let scale = light.dimmer * light.intensity;
                // ── LIGH → standard light translation ──────────────
                // Pre-compute the renderer-standard fields here so the
                // shader consumes ready-to-use values. Raw LIGH inputs
                // (`emitter.range`, `emitter.falloff_exponent`) never
                // reach GLSL — only the post-translation `effective_
                // range` and `falloff_shape`.
                let effective_range =
                    light.emitter.range.to_bethesda_units() * LIGHT_RANGE_EXTENSION;
                let source_radius = light.emitter.source_radius.to_bethesda_units();
                let falloff_shape = light.emitter.falloff_exponent;
                // #2205 — `GpuLight.color_type.w` type code (see its doc
                // comment: 0=point, 1=spot, 2=directional). `Ambient` has
                // no radiating representation in that contract and falls
                // back to the point code — a pre-existing gap tracked
                // separately as NIFAL-D3-02, not this fix's concern.
                let light_type = match light.emitter.kind {
                    LightKind::Point | LightKind::Ambient => 0.0,
                    LightKind::Spot => 1.0,
                    LightKind::Directional => 2.0,
                };
                // `direction_angle.w` is the spot outer-angle COSINE, not
                // the authored radians — the shader never sees a raw
                // angle (same translation-layer convention as `radius` /
                // `falloff_exponent` above).
                let outer_angle_cos = if light.emitter.kind == LightKind::Spot {
                    light.emitter.outer_cone_cos
                } else {
                    0.0
                };
                let radiant_intensity = light.emitter.radiant_intensity.scaled(scale);
                gpu_lights.push(byroredux_renderer::GpuLight {
                    position_radius: [
                        t.translation.x,
                        t.translation.y,
                        t.translation.z,
                        effective_range,
                    ],
                    color_type: [
                        radiant_intensity[0],
                        radiant_intensity[1],
                        radiant_intensity[2],
                        light_type,
                    ],
                    direction_angle: [
                        light.emitter.direction[0],
                        light.emitter.direction[1],
                        light.emitter.direction[2],
                        outer_angle_cos,
                    ],
                    // x = standardized attenuation curve shape; y = finite
                    // emitter proxy used to stop shadow segments at the
                    // luminous shell instead of inside the fixture; z = the
                    // unified policy consumed by every visibility pass; w is
                    // z = explicit VisibilityMask bits; w = attenuation model.
                    // Both were resolved before this render boundary, so no
                    // game-format radius/flag interpretation reaches GLSL.
                    params: [
                        falloff_shape,
                        source_radius,
                        light.emitter.visibility.bits() as f32,
                        light.emitter.attenuation as u8 as f32,
                    ],
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
    //
    // #2034 / PERF-D1-2026-07-16-02 — precompute `gi_priority_score` once
    // per light (Schwartzian transform / decorate-sort-undecorate)
    // instead of recomputing it on both sides of every comparator call.
    //
    // #2172 / PERF-D1-02 — the decorate buffer is caller-owned and reused
    // rather than freshly allocated each frame. #2034 judged the
    // allocation negligible on size grounds and it is (point-light counts
    // are streaming-RIS-capped, typically <50), but "small" and
    // "per-frame" is the same combination every other scratch in this
    // module family already amortizes away, so it costs nothing to be
    // consistent. `clear` + `extend` keeps the backing allocation and
    // only ever grows it to the high-water light count.
    // Emissive participating media light the scene they are visible in.
    // Must precede the GI-priority sort below so a fire competes for the
    // shader's `GI_HIT_LIGHT_CAP` prefix on brightness like any other light,
    // instead of being stranded at the tail by append order.
    super::fire_lights::append_fire_lights(fog_volumes, gpu_lights);

    let suffix = &mut gpu_lights[directional_count..];
    sort_scratch.clear();
    sort_scratch.extend(suffix.iter().map(|l| (gi_priority_score(l), *l)));
    // #2680 / PERF-D1-02 — `sort_unstable_by`, not `sort_by`: the stable sort
    // heap-allocates a light-count-sized temporary above its insertion-sort
    // cutoff, which would undo the caller-owned scratch #2172 just introduced.
    // Stability buys nothing on a freshly decorated buffer, and pattern-defeating
    // quicksort is still deterministic for a given input, so the GI prefix does
    // not flicker frame to frame.
    sort_scratch.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
    for (slot, (_, light)) in suffix.iter_mut().zip(sort_scratch.iter()) {
        *slot = *light;
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
mod directional_source_contract_tests {
    //! Regression guards for directional source selection.
    //!
    //! Interior and exterior cells intentionally differ only at the
    //! source-selection boundary:
    //!
    //!   1. Interior XCLL stays independent of the exterior weather
    //!      `sun_intensity` and retains its 0.6 source calibration.
    //!   2. Exterior directional colour continues to follow the TOD ramp.
    //!   3. Interiors with a positive/missing fade keep the standard
    //!      directional path.
    //!   4. The six-face ambient cube and the discrete directional remain
    //!      separate authored inputs.
    //!   5. Interior Directional Fade scales (and at zero suppresses) the
    //!      discrete directional source.
    //!
    //! Plus a resource-level gate: `weather_system`
    //! mutations to `cell_lit.directional_color` are themselves gated
    //! on `!is_interior` (see `systems/weather.rs:561, 213`), so a
    //! prior exterior load's sun colour can't bleed into the next
    //! interior's XCLL via `weather_system` re-running on the persisted
    //! `CellLightingRes`.
    //!
    //! This module pins that split at the light-list-assembly integration
    //! level. The unit-level coverage of
    //! `compute_directional_upload` lives in
    //! [`super::super::directional_upload_tests`] (interior path:
    //! `interior_uses_fixed_source_with_standard_directional_contract`); this
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
            fog_medium: crate::fog::FogMedium::from_legacy_ramp(64.0, 4000.0, None),
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
            inheritance_flags: None,
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

    /// An interior cell with persistent exterior `SkyParamsRes` must keep
    /// its XCLL source calibration as explicit unshadowed ambient fill.
    #[test]
    fn interior_with_persistent_sky_params_uses_ambient_fill_contract() {
        let mut world = World::new();
        world.insert_resource(interior_cell_lit([0.8, 0.7, 0.5]));
        world.insert_resource(full_sun_sky_params());

        let mut lights = Vec::new();
        collect_lights(&world, &mut lights, &[], &mut Vec::new());

        assert_eq!(
            lights.len(),
            1,
            "interior with no LIGH refs must emit exactly one ambient-fill GpuLight"
        );
        let l = &lights[0];
        assert!(
            (l.color_type[3] - 3.0).abs() < 1e-6,
            "GpuLight type slot must be 3.0 (ambient-fill marker), got {}",
            l.color_type[3]
        );
        assert_eq!(l.params[2], VisibilityMask::NONE.bits() as f32);
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
    fn exterior_with_full_sun_emits_standard_directional() {
        let mut world = World::new();
        world.insert_resource(exterior_cell_lit([0.8, 0.7, 0.5]));
        world.insert_resource(full_sun_sky_params());

        let mut lights = Vec::new();
        collect_lights(&world, &mut lights, &[], &mut Vec::new());

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

    /// Skyrim-era XCLL supplies both a six-face irradiance cube and a
    /// directional key. They are distinct authoring controls, so presence of
    /// the cube alone must not discard the directional light.
    #[test]
    fn interior_ambient_cube_does_not_replace_directional_light() {
        let mut world = World::new();
        let mut cell = interior_cell_lit([0.8, 0.7, 0.5]);
        cell.directional_ambient = Some([[0.1, 0.1, 0.1]; 6]);
        cell.directional_fade = Some(1.0);
        world.insert_resource(cell);

        let mut lights = Vec::new();
        collect_lights(&world, &mut lights, &[], &mut Vec::new());

        assert_eq!(lights.len(), 1);
        assert!((lights[0].color_type[0] - 0.8).abs() < 1e-5);
    }

    /// Bannered Mare authors Directional Fade = 0. Its ambient cube remains
    /// active, but the discrete +X key must contribute no light and therefore
    /// cannot project razor-thin parallel bars through the room geometry.
    #[test]
    fn zero_interior_directional_fade_suppresses_directional_light() {
        let mut world = World::new();
        let mut cell = interior_cell_lit([0.8, 0.7, 0.5]);
        cell.directional_ambient = Some([[0.1, 0.1, 0.1]; 6]);
        cell.directional_fade = Some(0.0);
        world.insert_resource(cell);

        let mut lights = Vec::new();
        collect_lights(&world, &mut lights, &[], &mut Vec::new());

        assert!(lights.is_empty());
    }

    #[test]
    fn interior_directional_fade_scales_key_light() {
        let mut world = World::new();
        let mut cell = interior_cell_lit([0.8, 0.7, 0.5]);
        cell.directional_fade = Some(0.25);
        world.insert_resource(cell);

        let mut lights = Vec::new();
        collect_lights(&world, &mut lights, &[], &mut Vec::new());

        assert_eq!(lights.len(), 1);
        assert!((lights[0].color_type[0] - 0.2).abs() < 1e-5);
    }

    /// Interior without SkyParamsRes (no prior exterior load) keeps the
    /// same source calibration and standard directional contract.
    #[test]
    fn interior_without_sky_params_uses_unshadowed_ambient_fill() {
        let mut world = World::new();
        world.insert_resource(interior_cell_lit([1.0, 1.0, 1.0]));
        // No SkyParamsRes — fresh-boot or interior-only session.

        let mut lights = Vec::new();
        collect_lights(&world, &mut lights, &[], &mut Vec::new());

        assert_eq!(lights.len(), 1);
        let l = &lights[0];
        assert!((l.position_radius[3] - 0.0).abs() < 1e-6);
        assert!((l.color_type[3] - 3.0).abs() < 1e-6);
        assert_eq!(l.params[2], VisibilityMask::NONE.bits() as f32);
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
        collect_lights(&world, &mut lights, &[], &mut Vec::new());

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
    fn legacy_translation_derives_a_finite_bounded_source_radius() {
        let translate = |range| {
            LightSource::from_legacy_world_units(
                range,
                [1.0; 3],
                0,
                1.0,
                byroredux_core::ecs::LightKind::Point,
                [0.0; 3],
                0.0,
                0,
            )
            .emitter
            .source_radius
            .to_bethesda_units()
        };
        assert_eq!(translate(0.0), 1.0);
        assert!((translate(200.0) - 10.0).abs() < f32::EPSILON);
        assert_eq!(translate(10_000.0), 32.0);
    }

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
        spawn_point_light_with_flags(world, pos, color, radius, 0);
    }

    fn spawn_point_light_with_flags(
        world: &mut World,
        pos: [f32; 3],
        color: [f32; 3],
        radius: f32,
        shadow_flags: u32,
    ) {
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
            LightSource::from_legacy_world_units(
                radius,
                color,
                0,
                1.0,
                byroredux_core::ecs::LightKind::Point,
                [0.0; 3],
                0.0,
                shadow_flags,
            ),
        );
    }

    /// Legacy fill lights still need structural occlusion so they cannot leak
    /// through walls, but only explicitly shadow-projecting sources may trace
    /// clutter and actors. Dense interiors contain many broad fill proxies;
    /// promoting them all to full-scene visibility produces stable comb-like
    /// projections through railings that no amount of extra sampling fixes.
    #[test]
    fn collect_lights_preserves_authored_local_shadow_classification() {
        let mut world = World::new();
        spawn_point_light_with_flags(
            &mut world,
            [10.0, 0.0, 0.0],
            [0.5; 3],
            256.0,
            0, // No legacy shadow-map projection bit.
        );
        spawn_point_light_with_flags(
            &mut world,
            [20.0, 0.0, 0.0],
            [0.5; 3],
            256.0,
            byroredux_core::ecs::LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL,
        );

        let mut lights = Vec::new();
        collect_lights(&world, &mut lights, &[], &mut Vec::new());

        let no_projection_bit = lights
            .iter()
            .find(|light| light.position_radius[0] == 10.0)
            .expect("fixture light without a legacy projection bit");
        let authored_projection = lights
            .iter()
            .find(|light| light.position_radius[0] == 20.0)
            .expect("fixture light with a legacy projection bit");
        assert_eq!(
            no_projection_bit.params[2],
            VisibilityMask::ARCHITECTURE.bits() as f32
        );
        assert_eq!(no_projection_bit.params[3], 0.0);
        assert_eq!(
            authored_projection.params[2],
            VisibilityMask::FULL.bits() as f32
        );
        assert_eq!(authored_projection.params[3], 0.0);
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
        collect_lights(&world, &mut lights, &[], &mut Vec::new());

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
            fog_medium: crate::fog::FogMedium::from_legacy_ramp(64.0, 4000.0, None),
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
            inheritance_flags: None,
        });
        // A point light far brighter than the directional source.
        spawn_point_light(&mut world, [5.0, 0.0, 0.0], [1.0, 1.0, 1.0], 5000.0);

        let mut lights = Vec::new();
        collect_lights(&world, &mut lights, &[], &mut Vec::new());

        assert_eq!(lights.len(), 2);
        assert!(
            (lights[0].color_type[3] - 2.0).abs() < 1e-6,
            "index 0 must always be the directional (type 2.0), regardless \
             of point-light brightness — got type {}",
            lights[0].color_type[3]
        );
    }
}

/// #2205 — `LightKind` resolved at NIF import (`NiDirectionalLight` /
/// `NiSpotLight` / `NiPointLight`) must reach `GpuLight.color_type.w`
/// instead of every placed `LightSource` uploading as a point light.
/// Pre-fix, an Oblivion `NiDirectionalLight` (kind resolved correctly at
/// import, then discarded — the canonical `LightSource` had no field to
/// receive it) rendered as a full-white omni light over an 8,192-unit
/// sphere instead of a directional contribution.
#[cfg(test)]
mod light_kind_wiring_tests {
    use super::*;
    use byroredux_core::ecs::{LightKind, LightSource};

    fn spawn_light(world: &mut World, light: LightSource) {
        let e = world.spawn();
        world.insert(
            e,
            GlobalTransform::new(
                byroredux_core::math::Vec3::new(1.0, 2.0, 3.0),
                byroredux_core::math::Quat::IDENTITY,
                1.0,
            ),
        );
        world.insert(e, light);
    }

    #[test]
    fn directional_light_source_uploads_as_directional_gpu_light() {
        let mut world = World::new();
        spawn_light(
            &mut world,
            LightSource::from_legacy_world_units(
                0.0,
                [1.0, 1.0, 1.0],
                0,
                1.0,
                LightKind::Directional,
                [0.0, -1.0, 0.0],
                0.0,
                0,
            ),
        );

        let mut lights = Vec::new();
        collect_lights(&world, &mut lights, &[], &mut Vec::new());

        assert_eq!(lights.len(), 1);
        assert!(
            (lights[0].color_type[3] - 2.0).abs() < 1e-6,
            "a LightKind::Directional source must upload type 2.0 (directional), got {}",
            lights[0].color_type[3]
        );
        assert_eq!(&lights[0].direction_angle[0..3], &[0.0, -1.0, 0.0]);
    }

    #[test]
    fn spot_light_source_uploads_as_spot_gpu_light_with_cosine_angle() {
        let mut world = World::new();
        let outer_angle = std::f32::consts::FRAC_PI_4; // 45 degrees, radians
        spawn_light(
            &mut world,
            LightSource::from_legacy_world_units(
                500.0,
                [1.0, 1.0, 1.0],
                0,
                1.0,
                LightKind::Spot,
                [1.0, 0.0, 0.0],
                outer_angle,
                0,
            ),
        );

        let mut lights = Vec::new();
        collect_lights(&world, &mut lights, &[], &mut Vec::new());

        assert_eq!(lights.len(), 1);
        assert!(
            (lights[0].color_type[3] - 1.0).abs() < 1e-6,
            "a LightKind::Spot source must upload type 1.0 (spot), got {}",
            lights[0].color_type[3]
        );
        assert_eq!(&lights[0].direction_angle[0..3], &[1.0, 0.0, 0.0]);
        assert!(
            (lights[0].direction_angle[3] - outer_angle.cos()).abs() < 1e-6,
            "direction_angle.w must be the outer-angle COSINE the shader \
             compares against, not the raw authored radians — got {}",
            lights[0].direction_angle[3]
        );
    }

    #[test]
    fn point_light_source_still_uploads_as_point_gpu_light() {
        let mut world = World::new();
        spawn_light(
            &mut world,
            LightSource::from_legacy_world_units(
                500.0,
                [1.0, 1.0, 1.0],
                0,
                1.0,
                LightKind::Point,
                [0.0; 3],
                0.0,
                0,
            ),
        );

        let mut lights = Vec::new();
        collect_lights(&world, &mut lights, &[], &mut Vec::new());

        assert_eq!(lights.len(), 1);
        assert_eq!(
            lights[0].color_type[3], 0.0,
            "default LightKind::Point must keep uploading type 0.0 — every \
             pre-#2205 producer (ESM LIGH REFRs, procedural fill, sun \
             proxies) relies on this via ..Default::default()"
        );
    }
}

/// #2172 / PERF-D1-02 — the GI-priority decorate-sort buffer is
/// caller-owned so its allocation amortizes across frames.
#[cfg(test)]
mod sort_scratch_reuse_tests {
    use super::*;
    use byroredux_core::ecs::LightSource;

    fn spawn_point_light(world: &mut World, radius: f32, brightness: f32) {
        let e = world.spawn();
        world.insert(
            e,
            GlobalTransform::new(
                byroredux_core::math::Vec3::new(radius, 0.0, 0.0),
                byroredux_core::math::Quat::IDENTITY,
                1.0,
            ),
        );
        world.insert(
            e,
            LightSource::from_legacy_world_units(
                radius,
                [brightness; 3],
                0,
                1.0,
                byroredux_core::ecs::LightKind::Point,
                [0.0; 3],
                0.0,
                0,
            ),
        );
    }

    /// The scratch must be reused, not reallocated: after a warm-up call
    /// has grown it to the scene's light count, a second call on the same
    /// scene must not touch the allocator.
    ///
    /// `capacity()` is the observable that distinguishes "cleared and
    /// refilled" from "dropped and rebuilt" — a fresh `Vec` collected into
    /// each frame would come back with capacity grown from 0, whereas
    /// `clear` + `extend` keeps whatever the high-water mark established.
    #[test]
    fn repeated_collection_reuses_the_scratch_allocation() {
        let mut world = World::new();
        for i in 1..=12 {
            spawn_point_light(&mut world, i as f32 * 100.0, i as f32 * 0.05);
        }

        let mut lights = Vec::new();
        let mut scratch = Vec::new();

        collect_lights(&world, &mut lights, &[], &mut scratch);
        let warm_capacity = scratch.capacity();
        assert!(
            warm_capacity >= 12,
            "warm-up call must size the scratch to the light count, got {warm_capacity}",
        );

        lights.clear();
        collect_lights(&world, &mut lights, &[], &mut scratch);
        assert_eq!(
            scratch.capacity(),
            warm_capacity,
            "a second frame over the same scene must not regrow the scratch",
        );
    }

    /// Threading the scratch must not change the sort result, including
    /// when the caller hands over a buffer still holding a previous
    /// frame's (larger) contents — `collect_lights` clears it, so stale
    /// entries can neither leak into the output nor survive past the
    /// current light count.
    #[test]
    fn stale_scratch_contents_do_not_leak_into_the_sorted_output() {
        let mut world = World::new();
        spawn_point_light(&mut world, 100.0, 0.05);
        spawn_point_light(&mut world, 900.0, 0.9);

        // Pre-poison with a high-scoring entry that is in no scene.
        let mut scratch = vec![(
            f32::MAX,
            byroredux_renderer::GpuLight {
                position_radius: [7.0, 7.0, 7.0, 12_345.0],
                color_type: [9.0, 9.0, 9.0, 0.0],
                direction_angle: [0.0; 4],
                params: [1.0, 0.0, 0.0, 0.0],
            },
        )];

        let mut lights = Vec::new();
        collect_lights(&world, &mut lights, &[], &mut scratch);

        assert_eq!(lights.len(), 2, "only the two spawned lights may appear");
        assert!(
            lights
                .iter()
                .all(|l| (l.position_radius[3] - 12_345.0).abs() > 1e-3),
            "the poisoned scratch entry leaked into the output: {:?}",
            lights.iter().map(|l| l.position_radius).collect::<Vec<_>>(),
        );
        let scores: Vec<f32> = lights.iter().map(gi_priority_score).collect();
        assert!(
            scores.windows(2).all(|w| w[0] >= w[1]),
            "descending gi_priority_score order must survive scratch reuse, got {scores:?}",
        );
    }
}
