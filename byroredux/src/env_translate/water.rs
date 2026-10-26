//! EXAL `WATR` → [`WaterMaterial`]: colours, specular, noise and rain
//! response, normal-layer motion, the water kind / physics flow
//! classification, and the composed flow scrolls.

use std::collections::HashMap;

use byroredux_core::ecs::components::water::{
    SubmersionState, WaterFlow, WaterKind, WaterMaterial,
    STARFIELD_WATER_CONCENTRATION_REFERENCE,
};
use byroredux_plugin::esm;
use byroredux_plugin::esm::reader::GameKind;

/// Shader UV-scroll rate produced by 1 BU/s of canonical [`WaterFlow`]
/// (#2872).
///
/// Calibrated against the two constants the engine already documents, not
/// invented: `WaterMaterial::default().scroll_a == [0.020, 0.011]` — a
/// magnitude of `hypot(0.020, 0.011) ≈ 0.02283` UV/s — is the authored look
/// of the slowest current [`WaterFlow::speed`] documents, its
/// `SPEED_MIN` "calm river" anchor of 0.5 BU/s. So one BU/s of flow scrolls
/// the wave layer `0.02283 / 0.5 ≈ 0.04565` UV/s, and a `River` plane
/// reproduces the default appearance exactly while `Rapids` / `Waterfall`
/// scale up in proportion to the current the physics sink is simulating.
pub(crate) const WATER_SCROLL_UV_PER_BU_PER_S: f32 = 0.045_651;

/// Fraction of the downstream rate at which scroll vector 1 crosses the
/// current — the canonical perpendicular shear the `WaterMaterial.scroll_*`
/// contract documents.
///
/// #4727 restores the pre-#4544 value of 0.5: #4544 halved it to 0.25 to
/// offset a ~90° frame error in the per-layer wind angles that slid the
/// dominant layer sideways across the current. With that frame corrected
/// at the boundary ([`watr_angle_to_engine_xz`]) the authored layers run
/// downstream and compose verbatim, so the compensating attenuation — and
/// the old `WATER_CROSS_STREAM_SCROLL` confinement — are gone.
pub(crate) const WATER_PERPENDICULAR_SHEAR_SCROLL: f32 = 0.5;

/// #4727 / #4910 — WATR's authored noise-wind angles (the per-layer DNAM
/// offsets, and the `wind_direction` alias Skyrim promotes from DNAM) are
/// wind-FROM compass bearings β in the record's Z-up frame: β = 0 blows from
/// game north (the pattern travels south), 90° from east, clockwise seen from
/// above. The engine XZ angle φ (from +X, toward +Z = game south, since
/// Z-up (x, y, z) maps to Y-up (x, z, −y)) is the direction of travel,
/// β + 180° as a bearing, which lands at φ = β + 90°. Census over the
/// shipped masters:
/// layer-minus-flow circular mean −84.1° raw on Skyrim's 51 non-zero
/// layers (R = 0.72, 48/51 negative, Rayleigh p ≈ 6e-12) and −87.7° on
/// FO4's 108 (R = 0.65, 96/108 negative, p ≈ 1e-19); under this conversion
/// the means land at +5.9° / +2.3° — the layers run downstream — while
/// every mirror shape collapses to R ≤ 0.36. The direction-named Skyrim
/// editor IDs confirm per record (RiverWaterFlowNE layer 0: 82° raw,
/// 8° corrected against its NE current).
///
/// #4910 — the conversion is scoped per game, because the census supports
/// only some layouts:
///
/// - **Skyrim / FO4** convert (the census above; R = 0.74 / 0.65).
/// - **FO76** converts *tentatively*: the re-run census gives +12.5° mean
///   offset but R = 0.36 — closer to converted than any mirror, yet weak.
/// - **Oblivion** does not convert: its layer 0 is the angle of the
///   editor's (x, y) scroll *pair*, not a bearing — the +90° read turned
///   (x, y) into (−y, x) with no evidence for any Oblivion record
///   (`DefaultWater` among them).
/// - **FO3 / FNV / Starfield** do not convert either: 71/78 FNV and 47/53
///   FO3 records carry non-zero layer speeds but none has a NAM0 to census
///   against, and Starfield's R = 0.21 supports neither frame. Both frames
///   are recorded OPEN in watal.md §2 — restoring the un-rotated read is
///   the pre-2026-09-24 status quo, not a claim about the authored frame.
///
/// Per the no-guessing rule the Skyrim conversion is pinned to the census by
/// `riverwater_flowne_layers_run_downstream_under_the_corrected_frame`
/// (and its ignored real-data sibling) in this module's tests; the per-game
/// scope is pinned by `wind_angle_conversion_is_scoped_per_game`.
fn watr_angle_to_engine_xz(game: GameKind, beta_radians: f32) -> f32 {
    match game {
        GameKind::Skyrim | GameKind::Fallout4 | GameKind::Fallout76 => {
            beta_radians + core::f32::consts::FRAC_PI_2
        }
        GameKind::Oblivion | GameKind::Fallout3NV | GameKind::Starfield => beta_radians,
    }
}

fn resolve_water_colors(
    waters: &HashMap<u32, esm::records::misc::WatrRecord>,
    rec: &esm::records::misc::WatrRecord,
    mat: &mut WaterMaterial,
) {
    mat.normal_encoding = rec.normal_encoding;
    mat.shallow_color = rec.params.shallow_color;
    mat.deep_color = rec.params.deep_color;
    mat.underwater_color = rec.params.underwater_color;
    mat.fog_near = rec.params.fog_near;
    mat.fog_far = rec.params.fog_far;
    mat.depth_amount = rec.params.depth_amount;
    mat.underwater_fog_near = rec.params.underwater_fog_near;
    mat.underwater_fog_far = rec.params.underwater_fog_far;
    mat.underwater_fog_amount = rec.params.underwater_fog_amount.clamp(0.0, 8.0);

    // GNAM's third related-water link is the authored underwater variant.
    // Resolve only one hop and reject self-links so malformed mod chains
    // cannot recurse or replace the parent surface optics.
    if let Some(underwater_form) = rec
        .related_waters
        .get(2)
        .copied()
        .filter(|form| *form != 0 && *form != rec.form_id)
    {
        if let Some(underwater) = waters.get(&underwater_form) {
            mat.underwater_color = underwater.params.underwater_color;
            mat.underwater_fog_near = underwater.params.underwater_fog_near;
            mat.underwater_fog_far = underwater.params.underwater_fog_far;
            mat.underwater_fog_amount = underwater.params.underwater_fog_amount.clamp(0.0, 8.0);
        }
    }

    // FO4/FO76 suspended silt is a water-column contribution. Resolve it
    // once so the surface, refraction, and underwater sinks share a palette.
    if rec.params.silt_amount.is_finite() && rec.params.silt_amount > 0.0 {
        let silt = rec.params.silt_amount.clamp(0.0, 1.0);
        for (dst, src) in mat
            .shallow_color
            .iter_mut()
            .zip(rec.params.silt_light_color)
        {
            *dst = (*dst * (1.0 - silt * 0.25) + src * (silt * 0.25)).clamp(0.0, 1.0);
        }
        for (dst, src) in mat.deep_color.iter_mut().zip(rec.params.silt_dark_color) {
            *dst = (*dst * (1.0 - silt) + src * silt).clamp(0.0, 1.0);
        }
        for (dst, src) in mat
            .underwater_color
            .iter_mut()
            .zip(rec.params.silt_dark_color)
        {
            *dst = (*dst * (1.0 - silt) + src * silt).clamp(0.0, 1.0);
        }
    }

    mat.day_shallow_color = mat.shallow_color;
    mat.day_deep_color = mat.deep_color;
    mat.day_fog_near = mat.fog_near;
    mat.day_fog_far = mat.fog_far;
    mat.day_reflection_tint = rec.params.reflection_color;
    mat.night_shallow_color = mat.shallow_color;
    mat.night_deep_color = mat.deep_color;
    mat.night_fog_near = mat.fog_near;
    mat.night_fog_far = mat.fog_far;
    mat.night_reflection_tint = rec.params.reflection_color;

    let variant_shallow = |base: [f32; 3], light: [f32; 3], amount: f32| {
        if amount.is_finite() && amount > 0.0 {
            let silt = amount.clamp(0.0, 1.0);
            std::array::from_fn(|i| {
                (base[i] * (1.0 - silt * 0.25) + light[i] * (silt * 0.25)).clamp(0.0, 1.0)
            })
        } else {
            base
        }
    };
    let variant_deep = |base: [f32; 3], dark: [f32; 3], amount: f32| {
        if amount.is_finite() && amount > 0.0 {
            let silt = amount.clamp(0.0, 1.0);
            std::array::from_fn(|i| (base[i] * (1.0 - silt) + dark[i] * silt).clamp(0.0, 1.0))
        } else {
            base
        }
    };
    for (slot, target) in [
        (
            0usize,
            (
                &mut mat.day_shallow_color,
                &mut mat.day_deep_color,
                &mut mat.day_fog_near,
                &mut mat.day_fog_far,
                &mut mat.day_reflection_tint,
            ),
        ),
        (
            1usize,
            (
                &mut mat.night_shallow_color,
                &mut mat.night_deep_color,
                &mut mat.night_fog_near,
                &mut mat.night_fog_far,
                &mut mat.night_reflection_tint,
            ),
        ),
    ] {
        let Some(form) = rec
            .related_waters
            .get(slot)
            .copied()
            .filter(|form| *form != 0 && *form != rec.form_id)
        else {
            continue;
        };
        let Some(variant) = waters.get(&form) else {
            continue;
        };
        *target.0 = variant_shallow(
            variant.params.shallow_color,
            variant.params.silt_light_color,
            variant.params.silt_amount,
        );
        *target.1 = variant_deep(
            variant.params.deep_color,
            variant.params.silt_dark_color,
            variant.params.silt_amount,
        );
        *target.2 = variant.params.fog_near;
        *target.3 = variant.params.fog_far;
        *target.4 = variant.params.reflection_color;
    }
}

fn resolve_water_specular(rec: &esm::records::misc::WatrRecord, mat: &mut WaterMaterial) {
    if rec.opacity_authored && rec.opacity.is_finite() {
        mat.opacity = rec.opacity.clamp(0.0, 1.0);
    }
    if rec
        .params
        .alpha_controls
        .iter()
        .any(|value| value.is_finite() && *value > 0.0)
    {
        mat.alpha_controls = rec.params.alpha_controls.map(|value| {
            if value.is_finite() {
                value.max(0.0)
            } else {
                0.0
            }
        });
        mat.alpha_controls[0] = mat.alpha_controls[0].clamp(0.0, 1.0);
        mat.alpha_controls[1] = mat.alpha_controls[1].clamp(0.0, 1.0);
        mat.alpha_controls[2] = mat.alpha_controls[2].clamp(0.0, 100_000.0);
        mat.alpha_controls[3] = mat.alpha_controls[3].clamp(mat.alpha_controls[2] + 1.0, 100_000.0);
    }
    mat.fresnel_f0 = rec.params.fresnel.clamp(0.001, 0.20);
    // FO3/FNV encode disabled reflection as an authored zero, not an FNAM
    // bit. Preserve the scalar verbatim (#3196).
    mat.reflectivity = rec.params.reflectivity;
    mat.reflection_tint = rec.params.reflection_color;
    if rec.params.reflection_hdr_multiplier.is_finite()
        && rec.params.reflection_hdr_multiplier > 0.0
    {
        mat.reflection_hdr_multiplier = rec.params.reflection_hdr_multiplier.clamp(0.0, 16.0);
    }
    mat.sun_specular_power = rec.params.sun_specular_power;
    if rec.params.roughness.is_finite() && rec.params.roughness > 0.0 {
        let roughness = rec.params.roughness.clamp(0.02, 1.0);
        mat.roughness = roughness;
        mat.sun_specular_power = (2.0 / (roughness * roughness) - 2.0).clamp(1.0, 2048.0);
    }
    if rec.params.specular_magnitude.is_finite() && rec.params.specular_magnitude > 0.0 {
        mat.specular_magnitude = rec.params.specular_magnitude.clamp(0.0, 8.0);
    }
    if rec.params.specular_radius.is_finite() && rec.params.specular_radius > 0.0 {
        mat.specular_radius = rec.params.specular_radius.clamp(0.0, 10_000.0);
    }
}

fn resolve_water_noise_and_rain(
    rec: &esm::records::misc::WatrRecord,
    mat: &mut WaterMaterial,
    game: GameKind,
) {
    mat.wave_amplitude = rec.params.wave_amplitude;
    mat.wave_frequency = rec.params.wave_frequency;
    mat.blend_normals = rec.blend_normals.unwrap_or(true);
    mat.angular_velocity = if rec.params.angular_velocity[2].is_finite() {
        rec.params.angular_velocity[2]
    } else {
        0.0
    }
    .clamp(
        -byroredux_core::ecs::components::water::WATER_SCROLL_ANGULAR_RATE_MAX,
        byroredux_core::ecs::components::water::WATER_SCROLL_ANGULAR_RATE_MAX,
    );
    mat.rain_response = if rec.params.rain_response.is_finite() {
        rec.params.rain_response.clamp(0.0, 4.0)
    } else {
        1.0
    };
    for (dst, src) in [
        (&mut mat.uv_scale_a, rec.params.noise_uv_scale_a),
        (&mut mat.uv_scale_b, rec.params.noise_uv_scale_b),
        (&mut mat.uv_scale_c, rec.params.noise_uv_scale_c),
    ] {
        if src.is_finite() && src > 0.0 {
            *dst = src.clamp(1.0 / 4096.0, 1.0 / 8.0);
        }
    }
    for (dst, src) in mat
        .noise_amplitude_scales
        .iter_mut()
        .zip(rec.params.noise_amplitude_scales)
    {
        if src.is_finite() && src > 0.0 {
            *dst = src.clamp(0.05, 4.0);
        }
    }
    if rec.params.noise_falloff.is_finite() && rec.params.noise_falloff > 0.0 {
        mat.noise_falloff = rec.params.noise_falloff.clamp(1.0, 100_000.0);
    }
    for (dst, src) in mat.normal_falloff.iter_mut().zip(rec.params.normal_falloff) {
        if src.is_finite() && src > 0.0 {
            *dst = src.clamp(0.0, 4.0);
        }
    }
    for (dst, src) in mat.displacement.iter_mut().zip(rec.params.displacement) {
        if src.is_finite() && src > 0.0 {
            *dst = src.clamp(0.0, 10_000.0);
        }
    }
    for (dst, src, min, max) in [
        (
            &mut mat.rain_start_size,
            rec.params.rain_start_size,
            0.05,
            10_000.0,
        ),
        (&mut mat.rain_velocity, rec.params.rain_velocity, 0.05, 16.0),
        (&mut mat.rain_falloff, rec.params.rain_falloff, 0.0, 64.0),
        (&mut mat.rain_dampener, rec.params.rain_dampener, 0.0, 64.0),
    ] {
        if src.is_finite() && src > 0.0 {
            *dst = src.clamp(min, max);
        }
    }
    let normal_magnitude =
        if rec.params.normal_magnitude.is_finite() && rec.params.normal_magnitude > 0.0 {
            rec.params.normal_magnitude.clamp(0.01, 8.0)
        } else {
            1.0
        };
    for amplitude in &mut mat.noise_amplitude_scales {
        *amplitude = (*amplitude * normal_magnitude).clamp(0.0, 8.0);
    }
    mat.above_water_fog_amount = if rec.params.above_water_fog_amount.is_finite() {
        rec.params.above_water_fog_amount.clamp(0.0, 8.0)
    } else {
        1.0
    };
    for (dst, src) in mat.depth_weights.iter_mut().zip(rec.params.depth_weights) {
        if src.is_finite() && src > 0.0 {
            *dst = src.clamp(0.0, 4.0);
        }
    }
    mat.depth_weights[1] = (mat.depth_weights[1] * mat.above_water_fog_amount).clamp(0.0, 8.0);
    for (index, (dst, src)) in mat
        .effect_controls
        .iter_mut()
        .zip(rec.params.effect_controls)
        .enumerate()
    {
        if src.is_finite() && src > 0.0 {
            *dst = if index == 1 {
                src.clamp(1.0, 2048.0)
            } else {
                src.clamp(0.0, 8.0)
            };
        }
    }
    if rec.params.flowmap_scale.is_finite() && rec.params.flowmap_scale > 0.0 {
        mat.flowmap_scale = rec.params.flowmap_scale.clamp(
            byroredux_core::ecs::components::water::WATER_FLOWMAP_SCALE_MIN,
            byroredux_core::ecs::components::water::WATER_FLOWMAP_SCALE_MAX,
        );
    }
    for (dst, src) in mat
        .absorption_coefficients
        .iter_mut()
        .zip(rec.params.absorption_coefficients)
    {
        // Per-BU extinction. Starfield's per-metre values arrive ÷70 from
        // the parse boundary (#5151: clear water's blue lane is ~0.0011/BU),
        // so the floor only guards against denormal noise, never clips data.
        if src.is_finite() && src > 0.0 {
            *dst = src.clamp(1.0e-6, 100_000.0);
        }
    }
    // #4285 — Starfield's authored pigment concentrations (RGB lanes)
    // are normalized to the canonical 0..1 pigment fraction HERE, at the
    // WATAL translate boundary, not in `water.frag`: per-game unit
    // conventions belong at the parser→canonical boundary. The fourth
    // `oceanness` lane is authored natively in 0..1 (vanilla 0 / 0.514 /
    // 0.699 / 1.0, #3227) and must NOT take the pigment division (#4837):
    // it passes through with only the canonical clamp. The zero sentinel
    // is preserved on every lane (0 / reference = 0).
    let [pigments @ .., oceanness] = rec.params.concentration;
    if concentration_pigments_are_starfield_scale(game) {
        for (dst, src) in mat.concentration.iter_mut().zip(pigments) {
            if src.is_finite() && src > 0.0 {
                *dst = (src / STARFIELD_WATER_CONCENTRATION_REFERENCE).clamp(0.0, 1.0);
            }
        }
    }
    if concentration_lane3_is_oceanness(game) && oceanness.is_finite() && oceanness > 0.0 {
        mat.concentration[3] = oceanness.clamp(0.0, 1.0);
    }
}

/// Whether WATR DNAM's three leading concentration lanes are on Starfield's
/// 0..20 pigment scale, so `/ STARFIELD_WATER_CONCENTRATION_REFERENCE` yields
/// the canonical 0..1 fraction. FO76 shares the decoder but authors them at
/// 9e-5–0.52 (census, #5169): dividing that by 20 applied Starfield's meaning
/// at an unmeasured scale, and its pigment term reached `water.frag` as a
/// 0–0.026 value nobody had decided. They keep the zero sentinel until a
/// source settles what they control — the same rule lane 3 follows below
/// (#5343). No other game authors them.
fn concentration_pigments_are_starfield_scale(game: GameKind) -> bool {
    matches!(game, GameKind::Starfield)
}

/// Whether WATR DNAM's fourth concentration lane is Starfield's 0..1
/// `oceanness` control. FO76 shares the decoder but authors that lane
/// 0.16–75.7 (39 of its 47 vanilla records above 1) — a different quantity
/// whose meaning is unestablished. Clamping it into `oceanness` turned most
/// FO76 water, puddles included, into full ocean, so it keeps the zero
/// sentinel until a source settles it (#5169). No other game authors it.
fn concentration_lane3_is_oceanness(game: GameKind) -> bool {
    matches!(game, GameKind::Starfield)
}

fn resolve_water_layer_motion(
    rec: &esm::records::misc::WatrRecord,
    layer: usize,
    game: GameKind,
) -> [f32; 2] {
    let speed = rec.params.noise_wind_speeds[layer];
    let direction = rec.params.noise_wind_directions[layer];
    if speed.is_finite() && speed > 0.0 && direction.is_finite() {
        // #4727 — the authored angle is a Z-up wind-FROM bearing; the
        // Z-up→Y-up frame conversion happens HERE, at the parse→canonical
        // boundary, never re-derived at render time. #4910 — scoped per
        // game; see watr_angle_to_engine_xz for the census basis.
        let (sin_theta, cos_theta) = watr_angle_to_engine_xz(game, direction).sin_cos();
        [cos_theta * speed, sin_theta * speed]
    } else {
        [0.0, 0.0]
    }
}

fn classify_water_kind_and_flow(
    rec: &esm::records::misc::WatrRecord,
    mat: &mut WaterMaterial,
    game: GameKind,
) -> (WaterKind, Option<WaterFlow>) {
    let named_kind = crate::material_translate::water_kind_from_cell_record_name(&rec.editor_id);
    let authored_flow_speed = rec
        .linear_velocity
        .map(|velocity| velocity[0].hypot(velocity[1]))
        .unwrap_or(0.0);
    let has_authored_linear_flow = rec.linear_velocity.is_some_and(|velocity| {
        velocity.iter().all(|component| component.is_finite()) && authored_flow_speed > 1.0e-5
    });
    let kind = if rec.material_name.eq_ignore_ascii_case("lava") {
        WaterKind::Lava
    } else if matches!(named_kind, WaterKind::Rapids)
        || (has_authored_linear_flow && authored_flow_speed >= WaterFlow::SPEED_RAPIDS)
    {
        WaterKind::Rapids
    } else if matches!(named_kind, WaterKind::River)
        || rec.flow_noise_texture_path_is_enabled()
        || has_authored_linear_flow
    {
        WaterKind::River
    } else {
        WaterKind::Calm
    };
    mat.foam_strength = kind.canonical_foam_strength();

    let mut flow = None;
    if kind.has_directional_flow() {
        // #4734 — FO3/FNV River-by-name creeks (CreekWater01,
        // RockCreekEstatesWater, …) carry no NAM0/XWCU and the dead editor
        // default (90°) in `wind_direction`. #2872 ruled a zero-variance
        // field cannot be authored velocity and #3185 ruled a name
        // establishes the water's kind but not its axis — this is that
        // same defect class, axis-fallback edition. Emit no physics flow
        // rather than fabricate a due-south current; the surface still
        // scrolls from its authored layers. #4931 — the parser decides
        // authorship per layout (a missing field or short DNAM is not a
        // heading either); nothing here re-tests the value.
        let authored_heading_available = rec.params.wind_direction_authored;
        let canonical = rec
            .linear_velocity
            .filter(|velocity| {
                let magnitude = velocity[0].hypot(velocity[1]);
                magnitude.is_finite() && magnitude > 1.0e-5
            })
            .map(|velocity| {
                let magnitude = velocity[0].hypot(velocity[1]);
                WaterFlow::new(
                    [velocity[0] / magnitude, 0.0, velocity[1] / magnitude],
                    magnitude,
                )
            })
            .or_else(|| {
                authored_heading_available.then(|| {
                    // #4727 — the same frame conversion as the layers: on
                    // Skyrim this field carries the promoted DNAM layer
                    // angle, so an un-converted read aims the physics
                    // current ~90° off course.
                    let (sin_theta, cos_theta) =
                        watr_angle_to_engine_xz(game, rec.params.wind_direction).sin_cos();
                    WaterFlow::for_kind(kind, [cos_theta, 0.0, sin_theta])
                })
            });
        if let Some(canonical) = canonical {
            // #4911 — the composition lives in one helper so the WATR
            // translation and `merge_placed_water` (a REFR XWCU current
            // replacing the WATR flow) cannot let the pattern term and the
            // physics current diverge.
            compose_flow_scrolls(
                mat,
                rec,
                game,
                [canonical.direction[0], canonical.direction[2]],
                canonical.speed,
            );
            flow = Some(canonical);
        } else {
            apply_authored_layer_scrolls(rec, mat, game);
        }
    } else {
        apply_authored_layer_scrolls(rec, mat, game);
    }
    (kind, flow)
}

/// #4911 — compose the flow scroll term onto the authored layer motion:
/// layer A rides the flow axis, layer B the perpendicular shear, layer C
/// keeps its authored motion or mirrors A. Shared by the WATR translation
/// above and `merge_placed_water`, which recomposes the term when a REFR
/// `XWCU` current replaces the WATR flow — across Skyrim's 128 XWCU REFRs
/// two run up to ~79° off their WATR, and the old shape let ripples run
/// across the current that carried floating bodies.
///
/// `direction_xz` is a unit vector in engine XZ; `speed` is world units per
/// second.
pub(crate) fn compose_flow_scrolls(
    mat: &mut WaterMaterial,
    rec: &esm::records::misc::WatrRecord,
    game: GameKind,
    direction_xz: [f32; 2],
    speed: f32,
) {
    let scroll = speed * WATER_SCROLL_UV_PER_BU_PER_S;
    // #4727 — the per-layer angles are converted to the engine XZ
    // frame at the boundary (resolve_water_layer_motion), so the
    // authored layers run downstream and compose with the flow term
    // verbatim. The #4544 cross-stream confinement is gone: it
    // compensated exactly the frame error the conversion fixes and
    // discarded ~70% of the authored speed profile while what
    // survived still pointed mostly sideways.
    let flow_x = direction_xz[0];
    let flow_z = direction_xz[1];
    let authored_a = resolve_water_layer_motion(rec, 0, game);
    let authored_b = resolve_water_layer_motion(rec, 1, game);
    let authored_c = resolve_water_layer_motion(rec, 2, game);
    mat.scroll_a = [
        flow_x * scroll + authored_a[0],
        flow_z * scroll + authored_a[1],
    ];
    mat.scroll_b = [
        -flow_z * scroll * WATER_PERPENDICULAR_SHEAR_SCROLL + authored_b[0],
        flow_x * scroll * WATER_PERPENDICULAR_SHEAR_SCROLL + authored_b[1],
    ];
    mat.scroll_c = if authored_c != [0.0, 0.0] {
        authored_c
    } else {
        mat.scroll_a
    };
}

/// The no-current scroll arm: authored noise layers scroll the surface
/// verbatim (no synthesized flow term). Shared by the calm arm and
/// #4734's dead-default-creek arm, which keeps its authored layer motion
/// but refuses to fabricate a physics current.
fn apply_authored_layer_scrolls(
    rec: &esm::records::misc::WatrRecord,
    mat: &mut WaterMaterial,
    game: GameKind,
) {
    for (dst, authored) in [
        (&mut mat.scroll_a, resolve_water_layer_motion(rec, 0, game)),
        (&mut mat.scroll_b, resolve_water_layer_motion(rec, 1, game)),
        (&mut mat.scroll_c, resolve_water_layer_motion(rec, 2, game)),
    ] {
        if authored != [0.0, 0.0] {
            *dst = authored;
        }
    }
}

fn resolve_water_texture_paths(
    rec: &esm::records::misc::WatrRecord,
    kind: WaterKind,
) -> (Option<String>, [Option<String>; 3]) {
    // The parser has already resolved the per-game texture role: this field
    // is normal/noise only. Oblivion TNAM diffuse art is preserved separately
    // and intentionally reaches the procedural normal fallback (#3222).
    let normal_path = (!rec.texture_path.is_empty()).then(|| rec.texture_path.clone());
    let mut noise_paths = std::array::from_fn(|index| {
        (!rec.noise_texture_paths[index].is_empty()).then(|| rec.noise_texture_paths[index].clone())
    });
    // Skyrim SE's NAM5 is a flow-normal texture. Preserve the compact
    // three-layer GPU ABI by promoting it over NAM4 only for flowing bodies;
    // calm water keeps its authored NAM4 layer.
    if kind.has_directional_flow() && rec.flow_noise_texture_path_is_enabled() {
        noise_paths[2] = Some(rec.flow_noise_texture_path.clone());
    }
    (normal_path, noise_paths)
}

/// Resolve a cell's `XCWT` FormID to an engine [`WaterMaterial`]
/// plus its classified [`WaterKind`], optional canonical [`WaterFlow`], and
/// resolved normal/noise texture paths for the cell loader to bind.
///
/// `xcwt_form == None` (no WATR reference on the cell) falls back to
/// engine defaults — same shape Skyrim uses for unmodded cells that
/// rely on the worldspace water-default cascade.
pub(crate) fn resolve_water_material(
    waters: &HashMap<u32, esm::records::misc::WatrRecord>,
    xcwt_form: Option<u32>,
    game: GameKind,
) -> (
    WaterMaterial,
    WaterKind,
    Option<WaterFlow>,
    Option<String>,
    [Option<String>; 3],
) {
    let mut mat = WaterMaterial::default();
    let mut kind = WaterKind::Calm;
    let mut flow: Option<WaterFlow> = None;
    let mut normal_path: Option<String> = None;
    let mut noise_paths: [Option<String>; 3] = [None, None, None];

    if let Some(form) = xcwt_form {
        if let Some(rec) = waters.get(&form) {
            resolve_water_colors(waters, rec, &mut mat);
            resolve_water_specular(rec, &mut mat);
            resolve_water_noise_and_rain(rec, &mut mat, game);
            mat.source_form = rec.form_id;
            (kind, flow) = classify_water_kind_and_flow(rec, &mut mat, game);
            (normal_path, noise_paths) = resolve_water_texture_paths(rec, kind);
        }
    }

    // SubmersionState is per-actor, not per-plane — but seed a
    // sentinel value on the material itself so debug overlays can
    // see "water without a parsed XCWT" cells.
    let _ = SubmersionState::default();

    // Calm water's pattern drifts with the weather wind, not at the authored
    // layer rate (`WaterMaterial::wave_scrolls`); decided here, once, so the
    // renderer and the CPU crest sampler cannot disagree.
    // A `River` whose current sits at the engine's calm-river floor (or that
    // has none at all) is placeholder motion on still water — Skyrim's
    // `DefaultWater` lakes and the unauthored defaults — so it follows the wind
    // too. A real authored current keeps its flow-driven scroll.
    let still_river = kind == WaterKind::River
        && flow.is_none_or(|flow| flow.speed <= WaterFlow::SPEED_MIN + 1.0e-3);
    mat.wind_driven = kind == WaterKind::Calm || still_river;
    (mat, kind, flow, normal_path, noise_paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::misc::{WaterParams, WatrRecord};

    // ── resolve_water_material ────────────────────────────────────

    /// Regression for #1069 / F-WAT-09 — `reflection_color` parsed from
    /// WATR DATA must reach `WaterMaterial.reflection_tint` via
    /// `resolve_water_material`. Pre-fix the field was silently dropped.
    #[test]
    fn resolve_water_material_transfers_reflection_color() {
        let lava_tint = [0.85_f32, 0.30, 0.10]; // orange-red lava pool

        let rec = WatrRecord {
            form_id: 0x000A_BCDE,
            editor_id: "LavaPool01".to_string(),
            full_name: "Lava Pool".to_string(),
            opacity: 0.75,
            opacity_authored: false,
            legacy_flags: None,
            legacy_damage: None,
            water_flags: None,
            blend_normals: None,
            texture_path: String::new(),
            diffuse_texture_path: String::new(),
            material_name: String::new(),
            sound_form: 0,
            noise_texture_paths: Default::default(),
            flow_noise_texture_path: String::new(),
            linear_velocity: None,
            related_waters: [0; 3],
            effect_form: 0,
            material_type_form: 0,
            normal_encoding: Default::default(),
            params: WaterParams {
                shallow_color: [1.0, 0.4, 0.1],
                deep_color: [0.6, 0.1, 0.0],
                underwater_color: [0.6, 0.1, 0.0],
                alpha_controls: [0.0; 4],
                reflection_color: lava_tint,
                reflection_hdr_multiplier: 2.5,
                fog_near: 20.0,
                fog_far: 80.0,
                depth_amount: 0.0,
                underwater_fog_near: 0.0,
                underwater_fog_far: 0.0,
                underwater_fog_amount: 1.0,
                reflectivity: 0.40,
                fresnel: 0.04,
                wind_speed: 0.0,
                wind_direction: 0.0,
                wind_direction_authored: false,
                angular_velocity: [0.0; 3],
                wave_amplitude: 0.0,
                wave_frequency: 0.0,
                rain_response: 1.0,
                sun_specular_power: 90.0,
                noise_uv_scale_a: 0.0,
                noise_uv_scale_b: 0.0,
                noise_uv_scale_c: 0.0,
                noise_amplitude_scales: [0.0; 3],
                noise_falloff: 0.0,
                normal_falloff: [0.0; 3],
                displacement: [0.0; 3],
                rain_start_size: 0.0,
                rain_velocity: 0.0,
                rain_falloff: 0.0,
                rain_dampener: 0.0,
                normal_magnitude: 1.0,
                above_water_fog_amount: 1.0,
                depth_weights: [0.0; 4],
                effect_controls: [0.0; 4],
                flowmap_scale: 0.0,
                specular_magnitude: 0.0,
                specular_radius: 0.0,
                noise_wind_directions: [0.0; 3],
                noise_wind_speeds: [0.0; 3],
                absorption_coefficients: [0.0; 3],
                concentration: [0.0; 4],
                roughness: 0.0,
                silt_amount: 0.0,
                silt_light_color: [0.0; 3],
                silt_dark_color: [0.0; 3],
            },
            raw_dnam: Vec::new(),
            raw_data: Vec::new(),
        };

        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);

        let (mat, _kind, _flow, _normal, _noise) =
            resolve_water_material(&waters, Some(0x000A_BCDE), GameKind::Skyrim);

        assert_eq!(
            mat.reflection_tint, lava_tint,
            "reflection_tint must round-trip from WATR DATA reflection_color"
        );
        assert_eq!(mat.sun_specular_power, 90.0);
        assert_eq!(mat.rain_response, 1.0);
        assert_eq!(mat.reflection_hdr_multiplier, 2.5);
    }

    /// #3196 — the reference title's own counterexample pair. `NVCleanWater`
    /// (`001009CA`) and `NVCleanWaterNoReflect` (`0017B612`) ship
    /// byte-identical 196-byte DNAM payloads except for the reflectivity float
    /// at offset 20, and **both** carry `FNAM == 0x02`. The author who needed a
    /// non-reflective variant zeroed the float and left the bit set, so the bit
    /// is not the reflectivity channel on FO3/FNV.
    ///
    /// This test **documents the disproof**; it is deliberately not the
    /// regression guard, and cannot be. Both records set the bit, so the
    /// removed gate never fired on either — restoring the gate leaves this
    /// green. The test that fails against the bug is
    /// `legacy_fnam_without_reflective_bit_keeps_authored_reflectivity` (the
    /// 44-of-78 majority), and the decode half is pinned by
    /// `reflectivity_comes_from_dnam_offset_20_not_from_the_fnam_bit` in
    /// `crates/plugin/src/esm/records/misc/water.rs`.
    #[test]
    fn legacy_fnam_reflective_bit_does_not_gate_authored_reflectivity() {
        let mut waters = HashMap::new();
        for (form_id, edid, reflectivity) in [
            (0x0010_09CA_u32, "NVCleanWater", 0.6_f32),
            (0x0017_B612, "NVCleanWaterNoReflect", 0.0),
        ] {
            let mut rec = calm_watr(
                form_id,
                edid,
                WaterParams {
                    reflectivity,
                    ..WaterParams::default()
                },
            );
            rec.legacy_flags = Some(0x02);
            waters.insert(form_id, rec);
        }

        let (reflective, _, _, _, _) = resolve_water_material(&waters, Some(0x0010_09CA), GameKind::Skyrim);
        let (matte, _, _, _, _) = resolve_water_material(&waters, Some(0x0017_B612), GameKind::Skyrim);
        // Same flag byte on both — only the authored float separates them.
        assert_eq!(reflective.reflectivity, 0.6);
        assert_eq!(matte.reflectivity, 0.0);
    }

    /// The 44-of-78 majority case: bit 0x02 clear, reflectivity authored.
    /// `WaterTypeUtility` (`000B03A7`, FNAM 0x00, 0.100) is referenced by 18
    /// vanilla cells and lost its reflection entirely under the removed gate.
    #[test]
    fn legacy_fnam_without_reflective_bit_keeps_authored_reflectivity() {
        let mut rec = calm_watr(
            0x000B_03A7,
            "WaterTypeUtility",
            WaterParams {
                reflectivity: 0.1,
                ..WaterParams::default()
            },
        );
        rec.legacy_flags = Some(0x00);
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);

        let (mat, _, _, _, _) = resolve_water_material(&waters, Some(0x000B_03A7), GameKind::Skyrim);
        assert_eq!(mat.reflectivity, 0.1);
    }

    #[test]
    fn resolve_water_material_uses_gnam_underwater_variant() {
        let parent_id = 0x000A_1000;
        let underwater_id = 0x000A_1001;
        let mut parent = calm_watr(parent_id, "LakeSurface", WaterParams::default());
        parent.related_waters[2] = underwater_id;
        let underwater = calm_watr(
            underwater_id,
            "LakeUnderwater",
            WaterParams {
                underwater_color: [0.08, 0.16, 0.30],
                underwater_fog_near: 12.0,
                underwater_fog_far: 340.0,
                underwater_fog_amount: 0.65,
                ..WaterParams::default()
            },
        );
        let mut waters = HashMap::new();
        waters.insert(parent_id, parent);
        waters.insert(underwater_id, underwater);

        let (mat, _, _, _, _) = resolve_water_material(&waters, Some(parent_id), GameKind::Skyrim);
        assert_eq!(mat.underwater_color, [0.08, 0.16, 0.30]);
        assert_eq!(mat.underwater_fog_near, 12.0);
        assert_eq!(mat.underwater_fog_far, 340.0);
        assert_eq!(mat.underwater_fog_amount, 0.65);
    }

    #[test]
    fn resolve_water_material_preserves_gnam_day_night_surface_variants() {
        let parent_id = 0x000A_1100;
        let day_id = 0x000A_1101;
        let night_id = 0x000A_1102;
        let mut parent = calm_watr(parent_id, "LakeSurface", WaterParams::default());
        parent.related_waters[0] = day_id;
        parent.related_waters[1] = night_id;
        let day = calm_watr(
            day_id,
            "LakeDay",
            WaterParams {
                shallow_color: [0.20, 0.45, 0.55],
                deep_color: [0.03, 0.10, 0.16],
                reflection_color: [0.75, 0.80, 0.85],
                fog_near: 30.0,
                fog_far: 300.0,
                ..WaterParams::default()
            },
        );
        let night = calm_watr(
            night_id,
            "LakeNight",
            WaterParams {
                shallow_color: [0.02, 0.05, 0.10],
                deep_color: [0.005, 0.01, 0.03],
                reflection_color: [0.12, 0.16, 0.24],
                fog_near: 8.0,
                fog_far: 90.0,
                ..WaterParams::default()
            },
        );
        let mut waters = HashMap::new();
        waters.insert(parent_id, parent);
        waters.insert(day_id, day);
        waters.insert(night_id, night);

        let (mat, _, _, _, _) = resolve_water_material(&waters, Some(parent_id), GameKind::Skyrim);
        assert_eq!(mat.day_shallow_color, [0.20, 0.45, 0.55]);
        assert_eq!(mat.day_fog_far, 300.0);
        assert_eq!(mat.night_deep_color, [0.005, 0.01, 0.03]);
        assert_eq!(mat.night_reflection_tint, [0.12, 0.16, 0.24]);
        assert_eq!(mat.night_fog_near, 8.0);
    }

    /// Default WaterMaterial (no XCWT / no WATR record) uses the neutral
    /// grey that matches the pre-#1069 hard-coded shader value.
    #[test]
    fn default_water_material_has_neutral_reflection_tint() {
        let (mat, _, _, _, _) = resolve_water_material(&HashMap::new(), None, GameKind::Skyrim);
        assert_eq!(
            mat.reflection_tint,
            [0.65, 0.70, 0.75],
            "default reflection_tint must match the pre-fix shader hard-code"
        );
    }

    /// Minimal calm WATR with only the named appearance fields authored;
    /// everything else left at `WaterParams::default`. Helper for the
    /// WATAL translate-up tests below.
    fn calm_watr(form: u32, edid: &str, params: WaterParams) -> WatrRecord {
        WatrRecord {
            form_id: form,
            editor_id: edid.to_string(),
            full_name: String::new(),
            opacity: 0.75,
            opacity_authored: false,
            legacy_flags: None,
            legacy_damage: None,
            water_flags: None,
            blend_normals: None,
            texture_path: String::new(),
            diffuse_texture_path: String::new(),
            material_name: String::new(),
            sound_form: 0,
            noise_texture_paths: Default::default(),
            flow_noise_texture_path: String::new(),
            linear_velocity: None,
            related_waters: [0; 3],
            effect_form: 0,
            material_type_form: 0,
            normal_encoding: Default::default(),
            params,
            raw_dnam: Vec::new(),
            raw_data: Vec::new(),
        }
    }

    #[test]
    fn oblivion_lava_uses_authored_material_and_keeps_diffuse_out_of_normal_role() {
        let mut rec = calm_watr(0x000A_0000, "LocalizedSurfaceName", WaterParams::default());
        rec.material_name = "lava".to_string();
        rec.diffuse_texture_path = "Water\\OblivionLava06.dds".to_string();
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);

        let (material, kind, flow, normal_path, noise_paths) =
            resolve_water_material(&waters, Some(0x000A_0000), GameKind::Skyrim);
        assert_eq!(kind, WaterKind::Lava);
        assert_eq!(material.foam_strength, 0.0);
        assert!(flow.is_none());
        assert!(normal_path.is_none());
        assert_eq!(noise_paths, [None, None, None]);
    }

    #[test]
    fn lava_editor_id_without_authored_material_does_not_guess_the_medium() {
        let rec = calm_watr(0x000A_0001, "OblivionLavaTest01", WaterParams::default());
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (_, kind, _, _, _) = resolve_water_material(&waters, Some(0x000A_0001), GameKind::Skyrim);
        assert_eq!(kind, WaterKind::Calm);
    }

    /// WATAL Phase 1: `wave_amplitude` / `wave_frequency` are parsed into
    /// `WaterParams` for every era but were dropped at the translate
    /// boundary pre-WATAL. They must now reach the canonical material.
    #[test]
    fn resolve_water_material_carries_wave_params() {
        let rec = calm_watr(
            0x000A_0001,
            "DefaultWater",
            WaterParams {
                wave_amplitude: 1.5,
                wave_frequency: 2.0,
                effect_controls: [7.5, 500.0, 0.0, 0.0],
                ..WaterParams::default()
            },
        );
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);

        let (mat, kind, _flow, _normal, _noise) =
            resolve_water_material(&waters, Some(0x000A_0001), GameKind::Skyrim);
        assert_eq!(
            mat.wind_driven,
            kind == WaterKind::Calm,
            "this record authors no current, so only a calm classification is wind-driven"
        );
        assert_eq!(
            mat.wave_amplitude, 1.5,
            "wave_amplitude must round-trip from WATR"
        );
        assert_eq!(
            mat.wave_frequency, 2.0,
            "wave_frequency must round-trip from WATR"
        );
        assert_eq!(mat.effect_controls[..2], [7.5, 500.0]);
        assert!(matches!(kind, WaterKind::Calm));
    }

    /// WATAL: the authored rain-simulator response must survive the WATR
    /// translation boundary so precipitation-driven ripples retain each
    /// game's water-specific intensity instead of silently using the default.
    #[test]
    fn resolve_water_material_carries_rain_response() {
        let rec = calm_watr(
            0x000A_0004,
            "RainSensitiveWater",
            WaterParams {
                rain_response: 2.75,
                ..WaterParams::default()
            },
        );
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);

        let (mat, _, _, _, _) = resolve_water_material(&waters, Some(0x000A_0004), GameKind::Skyrim);
        assert_eq!(
            mat.rain_response, 2.75,
            "rain_response must round-trip from WATR into WaterMaterial"
        );
    }

    /// #4734 (restored by #4931 — `a6a210eb9` deleted it) — a River-by-name
    /// FO3/FNV creek with no NAM0/XWCU and no authored heading must produce
    /// **no** physics current: #2872 ruled the zero-variance field cannot be
    /// authored velocity, #3185 ruled a name establishes kind but not axis.
    /// Pre-#4734 the fallback fabricated a due-south current for every such
    /// creek. Its authored layer motion still scrolls (through the #4727
    /// frame conversion), and a record with an authored NAM0 keeps its flow.
    #[test]
    fn dead_default_wind_direction_yields_no_physics_flow_for_named_creeks() {
        let mut rec = calm_watr(
            0x000A_0005,
            "CreekWater01",
            WaterParams {
                wind_direction: 90.0f32.to_radians(),
                wind_direction_authored: false,
                noise_wind_directions: [0.25, 0.0, 1.2],
                noise_wind_speeds: [0.10, 0.0, 0.20],
                ..WaterParams::default()
            },
        );
        // `CreekWater01` classifies River by name alone.
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec.clone());

        let (mat, kind, flow, _, _) =
            resolve_water_material(&waters, Some(rec.form_id), GameKind::Skyrim);
        assert!(matches!(kind, WaterKind::River), "the name alone classifies the kind");
        assert!(
            mat.wind_driven,
            "a creek with no current is still water: it follows the wind"
        );
        assert!(
            flow.is_none(),
            "an unauthored heading must not fabricate a physics current (#4734)"
        );
        // Authored layer motion still scrolls the surface verbatim, in the
        // engine frame (A and C here; layer B's zero speed leaves the
        // sentinel default in place).
        let layer = |speed: f32, bearing: f32| {
            let (sin, cos) = watr_angle_to_engine_xz(GameKind::Skyrim, bearing).sin_cos();
            [cos * speed, sin * speed]
        };
        let close = |got: [f32; 2], want: [f32; 2]| {
            (got[0] - want[0]).abs() < 1e-6 && (got[1] - want[1]).abs() < 1e-6
        };
        assert!(close(mat.scroll_a, layer(0.10, 0.25)), "{:?}", mat.scroll_a);
        assert_eq!(mat.scroll_b, WaterMaterial::default().scroll_b);
        assert!(close(mat.scroll_c, layer(0.20, 1.2)), "{:?}", mat.scroll_c);

        // The contrast: an authored NAM0 keeps the flow (and its scroll
        // synthesis) — the refusal is specifically about the missing heading.
        rec.linear_velocity = Some([1.0, 0.0]);
        waters.insert(rec.form_id, rec);
        let (_, _, flow, _, _) = resolve_water_material(&waters, Some(0x000A_0005), GameKind::Skyrim);
        let flow = flow.expect("an authored NAM0 current must survive");
        assert!((flow.speed - 1.0).abs() < 1.0e-6);
        // ...and a real current keeps the flow-driven scroll, unlike the
        // currentless creek above.
        let (mat, ..) = resolve_water_material(&waters, Some(0x000A_0005), GameKind::Skyrim);
        assert!(!mat.wind_driven, "an authored current is not wind-driven");
    }

    /// #4931 — the flag, not the value, gates the fallback: a NAM0-less
    /// River whose parser marked the heading authored keeps a current even
    /// at exactly 90° (FO4 authors that bearing), and an unauthored 0° (a
    /// missing/short DNAM's default) emits none.
    #[test]
    fn nam0_less_flow_fallback_follows_the_parse_side_authored_flag() {
        for (degrees, authored) in [(90.0f32, true), (0.0, false), (0.0, true)] {
            let rec = calm_watr(
                0x000A_0006,
                "CreekWater02",
                WaterParams {
                    wind_direction: degrees.to_radians(),
                    wind_direction_authored: authored,
                    ..WaterParams::default()
                },
            );
            let waters = HashMap::from([(rec.form_id, rec)]);
            let (_, kind, flow, _, _) = resolve_water_material(&waters, Some(0x000A_0006), GameKind::Skyrim);
            assert!(matches!(kind, WaterKind::River));
            assert_eq!(
                flow.is_some(),
                authored,
                "{degrees}° authored={authored}: the parse-side flag alone decides"
            );
        }
    }

    #[test]
    fn authored_layer_motion_runs_downstream_and_composes_verbatim() {
        let mut rec = calm_watr(
            0x000A_0002,
            "LocalizedWater",
            WaterParams {
                wind_direction: 0.0,
                // An authored 0° bearing (e.g. an Oblivion record) — the
                // NAM0-less fallback may use it (#4931).
                wind_direction_authored: true,
                noise_wind_directions: [0.0, std::f32::consts::FRAC_PI_2, 0.25],
                noise_wind_speeds: [0.10, 0.20, 0.30],
                ..WaterParams::default()
            },
        );
        // NAM5 is an authored flow signal even when the EDID is localized or
        // otherwise gives no English river/stream hint.
        rec.flow_noise_texture_path = "textures\\water\\flow.dds".to_string();
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);

        let (mat, kind, flow, _, _) = resolve_water_material(&waters, Some(0x000A_0002), GameKind::Skyrim);
        assert!(matches!(kind, WaterKind::River));
        let flow = flow.expect("flowing water must carry its canonical current");
        // wind_direction 0 is a raw Z-up wind-FROM bearing (from game
        // north); under the corrected frame (#4727: φ = β + 90°) it reads
        // as engine +Z — game SOUTH, since Z-up (x, y, z) maps to Y-up
        // (x, z, −y) — so the current runs south and the perpendicular
        // shear is the scroll-x lane.
        let scroll = flow.speed * WATER_SCROLL_UV_PER_BU_PER_S;
        // Layer A: raw bearing 0 converts to (−sin 0, cos 0) = +Z — fully
        // along-flow — and composes on top of the current-driven term.
        assert!((mat.scroll_a[0] - 0.0).abs() < 1e-6);
        assert!((mat.scroll_a[1] - (scroll + 0.10)).abs() < 1e-6);
        // Layer B: raw π/2 converts to (−1, 0) — pure cross-stream toward
        // −X — and survives VERBATIM now (the #4544 quartering is gone);
        // the deliberate perpendicular shear also points −X here (flow is
        // +Z, the shear term is (−flow_z, flow_x)) and returns to the
        // documented half rate.
        assert!(
            (mat.scroll_b[0] - (-0.20 - scroll * WATER_PERPENDICULAR_SHEAR_SCROLL)).abs() < 1e-6
        );
        assert!(mat.scroll_b[1].abs() < 1e-6);
        // Layer C: raw 0.25 converts to (−sin 0.25, cos 0.25)·0.30 verbatim.
        assert!((mat.scroll_c[0] - -0.30 * 0.25_f32.sin()).abs() < 1e-6);
        assert!((mat.scroll_c[1] - 0.30 * 0.25_f32.cos()).abs() < 1e-6);
    }

    /// #4727 — the census relation pinned against the vanilla
    /// `RiverWaterFlowNE` authoring (NAM0 (2.54, -1.35), layer dirs
    /// 4.067/4.660/4.398 rad, speeds 0.09/0.04/0.30): under the corrected
    /// engine frame every authored layer runs downstream — layer 0 within
    /// ~10° of the record's own NAM0 current (raw it sat 82° off) — and
    /// the dominant scroll drifts sideways by only a fraction of its
    /// downstream rate. Pre-#4544 the raw angles slid the dominant layer
    /// ~80% sideways; #4544 masked that by discarding ~70% of the authored
    /// speed profile; this is the frame fix that removes the cause.
    /// #4910 — the +90° bearing conversion is scoped per game: census-backed
    /// on Skyrim/FO4 (and tentatively FO76), while Oblivion's layer angles
    /// are the editor's (x, y) scroll pair — unrotated — and FO3/FNV and
    /// Starfield stay on the un-rotated status quo with both frames recorded
    /// OPEN in watal.md §2 (no NAM0 exists there to census against).
    #[test]
    fn wind_angle_conversion_is_scoped_per_game() {
        use byroredux_plugin::esm::reader::GameKind;

        let mut rec = esm::records::misc::WatrRecord::default();
        rec.params.noise_wind_speeds[0] = 1.0;
        rec.params.noise_wind_directions[0] = 0.0;

        // A due-north wind-FROM bearing (β = 0) travels south (+Z) once
        // converted — the Skyrim/FO4/FO76 census frame.
        for game in [GameKind::Skyrim, GameKind::Fallout4, GameKind::Fallout76] {
            let motion = resolve_water_layer_motion(&rec, 0, game);
            assert!(
                (motion[0].abs() < 1.0e-6) && ((motion[1] - 1.0).abs() < 1.0e-6),
                "{game:?}: the bearing converts (got {motion:?})"
            );
        }
        // Un-rotated: the same angle reads as the +X unit pair — Oblivion's
        // Cartesian scroll direction, and the pre-2026-09-24 read the
        // undetermined frames keep.
        for game in [
            GameKind::Oblivion,
            GameKind::Fallout3NV,
            GameKind::Starfield,
        ] {
            assert_eq!(
                resolve_water_layer_motion(&rec, 0, game),
                [1.0, 0.0],
                "{game:?}: the angle is not a converted bearing"
            );
        }
    }

    #[test]
    fn riverwater_flowne_layers_run_downstream_under_the_corrected_frame() {
        let rec = calm_watr(
            0x000A_0009,
            "RiverWaterFlowNE",
            WaterParams {
                noise_wind_directions: [4.066_617, 4.660_029, 4.398_229_6],
                noise_wind_speeds: [0.09, 0.04, 0.30],
                ..WaterParams::default()
            },
        );
        let layer_dirs = rec.params.noise_wind_directions;
        let layer_speeds = rec.params.noise_wind_speeds;
        let rec = esm::records::misc::WatrRecord {
            linear_velocity: Some([2.54, -1.35]),
            ..rec
        };
        let waters = HashMap::from([(rec.form_id, rec)]);
        let (mat, kind, flow, _, _) = resolve_water_material(&waters, Some(0x000A_0009), GameKind::Skyrim);
        assert!(matches!(kind, WaterKind::River));
        let flow = flow.expect("NE river must carry its NAM0 current");
        let f = flow.direction;
        let axis = [f[0], f[2]];
        assert!(axis[0] > 0.8 && axis[1] < -0.3, "flow must point ENE in engine XZ");
        let flow_bearing = axis[1].atan2(axis[0]);
        // Every non-zero authored layer runs within 10° of the current.
        for (layer, &speed) in layer_speeds.iter().enumerate() {
            if !(speed.is_finite() && speed > 0.0) {
                continue;
            }
            let angle = watr_angle_to_engine_xz(GameKind::Skyrim, layer_dirs[layer]);
            let delta = (angle - flow_bearing).abs();
            let delta = delta.min(std::f32::consts::TAU - delta);
            assert!(
                delta.to_degrees() <= 10.0,
                "layer {layer} runs {:.1}° off the NAM0 current under the \
                 corrected frame — the authored layers must run downstream",
                delta.to_degrees()
            );
        }
        // The dominant scroll rides the current, not sideways across it.
        let (along, cross) = (mat.scroll_a[0] * axis[0] + mat.scroll_a[1] * axis[1],
                              -mat.scroll_a[0] * axis[1] + mat.scroll_a[1] * axis[0]);
        assert!(along > 0.0, "the dominant layer must travel downstream");
        let cross_ratio = (cross.abs() / along.max(1e-6)).abs();
        assert!(
            cross_ratio < 0.35,
            "dominant layer drifts {cross_ratio:.2}× its downstream rate — \
             sideways current motion must stay well under half"
        );
    }

    /// #4727 — the same census relation against the shipped record itself
    /// (the synthetic pin above freezes the vanilla authoring; this one
    /// re-reads it live, so re-authored angles still get the frame checked
    /// against their own NAM0). Opt-in like the other real-data tests:
    /// needs `Skyrim.esm` on disk.
    #[test]
    #[ignore = "needs Skyrim SE game data on disk"]
    fn riverwater_flowne_real_record_layers_run_downstream() {
        let data_dir = std::env::var_os(byroredux_plugin::esm::test_paths::SKYRIM_SE_ENV)
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(
                    "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data",
                )
            });
        let esm_path = data_dir.join("Skyrim.esm");
        if !esm_path.is_file() {
            return;
        }
        let index = byroredux_plugin::esm::parse_esm(&std::fs::read(esm_path).unwrap()).unwrap();
        let rec = index
            .waters
            .values()
            .find(|rec| rec.editor_id.eq_ignore_ascii_case("RiverWaterFlowNE"))
            .expect("Skyrim.esm must ship RiverWaterFlowNE");
        let velocity = rec
            .linear_velocity
            .expect("RiverWaterFlowNE authors a NAM0 current");
        let flow_bearing = velocity[1].atan2(velocity[0]);
        for (layer, &speed) in rec.params.noise_wind_speeds.iter().enumerate() {
            if !(speed.is_finite() && speed > 0.0) {
                continue;
            }
            let angle = watr_angle_to_engine_xz(GameKind::Skyrim, rec.params.noise_wind_directions[layer]);
            let delta = (angle - flow_bearing).abs();
            let delta = delta.min(std::f32::consts::TAU - delta);
            assert!(
                delta.to_degrees() <= 10.0,
                "layer {layer} runs {:.1}° off the record's own NAM0 current",
                delta.to_degrees()
            );
        }
    }

    /// #4900 — parse→translate: a NAM0 water whose DNAM leaves a layer at
    /// zero speed (FO4 `IntOldGulletWaterSlow`: NAM0 (0.08, 0, 0), layer
    /// speeds (0.0072, 0.0, 0.0145)). The parser used to fill that layer
    /// from NAM0 in the engine frame; the translate then rotated it +90°
    /// again, so it slid across the current at 4× the flow term. Only the
    /// synthesized perpendicular shear may cross the current.
    #[test]
    fn nam0_water_layers_never_run_across_the_current_after_parse() {
        use byroredux_plugin::esm::reader::SubRecord;
        let mut dnam = vec![0u8; 228];
        // Skyrim DNAM layer bearings (100/104/108, degrees) and speeds
        // (112/116/120). 270° in the record frame is +X in engine XZ,
        // parallel to the NAM0 current below.
        for (offset, value) in [(100, 270.0f32), (108, 270.0), (112, 0.0072), (120, 0.0145)] {
            dnam[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        let mut nam0 = Vec::new();
        for component in [0.08f32, 0.0, 0.0] {
            nam0.extend_from_slice(&component.to_le_bytes());
        }
        let rec = byroredux_plugin::esm::records::parse_watr(
            0x001E_214D,
            &[
                SubRecord { sub_type: *b"DNAM", data: dnam },
                SubRecord { sub_type: *b"NAM0", data: nam0 },
            ],
            GameKind::Skyrim,
            &None,
        );
        let waters = HashMap::from([(rec.form_id, rec)]);
        let (mat, kind, flow, _, _) = resolve_water_material(&waters, Some(0x001E_214D), GameKind::Skyrim);
        assert!(matches!(kind, WaterKind::River));
        let flow = flow.expect("a nonzero NAM0 is an authored current");
        assert!((flow.direction[0] - 1.0).abs() < 1e-6 && flow.direction[2].abs() < 1e-6);

        // Layers 0 and 2 are authored along the current: no cross-stream part.
        assert!(mat.scroll_a[1].abs() < 1e-6, "scroll_a crosses the current: {:?}", mat.scroll_a);
        assert!(mat.scroll_c[1].abs() < 1e-6, "scroll_c crosses the current: {:?}", mat.scroll_c);
        // Layer 1 is unauthored: exactly the canonical perpendicular shear
        // of the (SPEED_MIN-clamped) canonical current.
        let shear = flow.speed * WATER_SCROLL_UV_PER_BU_PER_S * WATER_PERPENDICULAR_SHEAR_SCROLL;
        assert!(mat.scroll_b[0].abs() < 1e-6);
        assert!(
            (mat.scroll_b[1] - shear).abs() < 1e-6,
            "scroll_b {:?} is not the bare shear {shear}",
            mat.scroll_b
        );
    }

    #[test]
    fn modern_fnam_flowmap_flag_gates_nam5_but_keeps_authored_current() {
        let mut disabled = calm_watr(
            0x000A_0003,
            "LocalizedWater",
            WaterParams {
                // An authored heading, so the NAM0-less fallback runs (#4931).
                wind_direction_authored: true,
                ..WaterParams::default()
            },
        );
        disabled.flow_noise_texture_path = "textures\\water\\flow.dds".to_string();
        disabled.water_flags = Some(0x00);
        disabled.blend_normals = Some(false);
        let mut enabled = disabled.clone();
        enabled.form_id += 1;
        enabled.water_flags = Some(0x08);
        enabled.blend_normals = Some(true);
        let waters = HashMap::from([(disabled.form_id, disabled), (enabled.form_id, enabled)]);

        let (disabled_mat, disabled_kind, disabled_flow, _, disabled_noise) =
            resolve_water_material(&waters, Some(0x000A_0003), GameKind::Skyrim);
        assert!(matches!(disabled_kind, WaterKind::Calm));
        assert!(disabled_flow.is_none());
        assert!(disabled_noise[2].is_none());
        assert!(!disabled_mat.blend_normals);

        let (enabled_mat, enabled_kind, enabled_flow, _, enabled_noise) =
            resolve_water_material(&waters, Some(0x000A_0004), GameKind::Skyrim);
        assert!(matches!(enabled_kind, WaterKind::River));
        assert!(enabled_flow.is_some());
        assert_eq!(
            enabled_noise[2].as_deref(),
            Some("textures\\water\\flow.dds")
        );
        assert!(enabled_mat.blend_normals);
    }

    #[test]
    fn flowmap_scale_is_not_baked_into_canonical_flow_scroll() {
        let mut rec = calm_watr(
            0x000A_0004,
            "LocalizedRiver",
            WaterParams {
                flowmap_scale: 3.0,
                // Raw −90° reads as engine +X under the corrected frame
                // (#4727), keeping this test's along-X current.
                wind_direction: -std::f32::consts::FRAC_PI_2,
                wind_direction_authored: true,
                ..WaterParams::default()
            },
        );
        rec.flow_noise_texture_path = "textures\\water\\flow.dds".to_string();
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);

        let (mat, kind, flow, _, _) = resolve_water_material(&waters, Some(0x000A_0004), GameKind::Skyrim);
        let speed = flow
            .expect("flowing water must have a canonical current")
            .speed;
        assert!(matches!(kind, WaterKind::River));
        assert_eq!(mat.flowmap_scale, 3.0);
        assert!((mat.scroll_a[0] - speed * WATER_SCROLL_UV_PER_BU_PER_S).abs() < 1.0e-6);
    }

    #[test]
    fn resolve_water_material_carries_starfield_optical_controls_with_canonical_concentration() {
        let rec = calm_watr(
            0x000A_0008,
            "StarfieldOcean",
            WaterParams {
                absorption_coefficients: [0.16558, 0.09624, 0.07627],
                concentration: [8.840, 6.594, 4.710, 0.514],
                noise_falloff: 300.0,
                normal_falloff: [0.9, 0.7, 0.8],
                displacement: [0.05, 0.985, 10.0],
                ..WaterParams::default()
            },
        );
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (mat, _, _, _, _) = resolve_water_material(&waters, Some(0x000A_0008), GameKind::Starfield);
        assert_eq!(mat.absorption_coefficients, [0.16558, 0.09624, 0.07627]);
        // #5151 — the per-BU values the parse boundary produces for the same
        // vanilla record must pass the translate floor unclipped.
        let per_bu = [0.16558 / 70.0, 0.09624 / 70.0, 0.07627 / 70.0];
        let rec = calm_watr(
            0x000A_0009,
            "StarfieldOceanPerBu",
            WaterParams {
                absorption_coefficients: per_bu,
                ..WaterParams::default()
            },
        );
        let mut per_bu_waters = HashMap::new();
        per_bu_waters.insert(rec.form_id, rec);
        let (per_bu_mat, _, _, _, _) =
            resolve_water_material(&per_bu_waters, Some(0x000A_0009), GameKind::Skyrim);
        assert_eq!(per_bu_mat.absorption_coefficients, per_bu);
        // #4285 — pigment concentrations are normalized to canonical 0..1
        // fractions at this boundary (RGB lanes ÷ the Starfield authoring
        // upper bound). The natively-0..1 `oceanness` lane is NOT divided
        // (#4837 — dividing it weakened both shader consumers ~20×). The
        // shader no longer carries a per-game unit constant.
        assert_eq!(
            mat.concentration,
            [8.840 / 20.0, 6.594 / 20.0, 4.710 / 20.0, 0.514]
        );
        assert_eq!(mat.noise_falloff, 300.0);
        assert_eq!(mat.normal_falloff, [0.9, 0.7, 0.8]);
        assert_eq!(mat.displacement, [0.05, 0.985, 10.0]);
    }

    /// #4837 — `oceanness` is authored natively in 0..1 and must not take
    /// the RGB pigment division; an out-of-range authored value (one
    /// Starfield test record carries 1.63) takes only the canonical clamp.
    #[test]
    fn starfield_oceanness_lane_passes_through_without_the_pigment_division() {
        for (authored, expected) in [(1.0, 1.0), (0.699, 0.699), (1.63, 1.0), (0.0, 0.0)] {
            let rec = calm_watr(
                0x000A_000A,
                "StarfieldOceanness",
                WaterParams {
                    concentration: [20.0, 0.0, 0.0, authored],
                    ..WaterParams::default()
                },
            );
            let mut waters = HashMap::new();
            waters.insert(rec.form_id, rec);
            let (mat, _, _, _, _) = resolve_water_material(&waters, Some(0x000A_000A), GameKind::Starfield);
            assert_eq!(mat.concentration, [1.0, 0.0, 0.0, expected], "oceanness {authored}");
        }
    }

    /// #5169 — FO76 shares Starfield's DNAM decoder but authors the fourth
    /// concentration lane 0.16–75.7, not a 0..1 oceanness. It keeps the zero
    /// sentinel there instead of clamping most FO76 water to full ocean; the
    /// same record under Starfield still promotes, so the gate is the game.
    #[test]
    fn fo76_fourth_concentration_lane_is_not_promoted_to_oceanness() {
        let rec = calm_watr(
            0x000A_000B,
            "ExtMurkyWaterDefault",
            WaterParams {
                concentration: [0.00237, 0.0018, 0.00098, 8.0],
                ..WaterParams::default()
            },
        );
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (fo76, _, _, _, _) =
            resolve_water_material(&waters, Some(0x000A_000B), GameKind::Fallout76);
        assert_eq!(fo76.concentration[3], 0.0);
        let (sf, _, _, _, _) =
            resolve_water_material(&waters, Some(0x000A_000B), GameKind::Starfield);
        assert_eq!(sf.concentration[3], 1.0);
        // #5343 — the pigment lanes are gated too: FO76 authors them at
        // 9e-5–0.52, not Starfield's 0–20, so the Starfield division is not
        // applied to them (zero sentinel), where Starfield still normalises.
        assert_eq!(fo76.concentration[..3], [0.0, 0.0, 0.0]);
        assert!(
            sf.concentration[..3].iter().all(|lane| *lane > 0.0),
            "fixture: the same record gives Starfield a pigment contribution"
        );
    }

    #[test]
    fn resolve_water_material_maps_starfield_roughness_to_sun_exponent() {
        let rec = calm_watr(
            0x000A_0009,
            "StarfieldOcean",
            WaterParams {
                roughness: 0.5,
                ..WaterParams::default()
            },
        );
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (mat, _, _, _, _) = resolve_water_material(&waters, Some(0x000A_0009), GameKind::Skyrim);
        assert!((mat.roughness - 0.5).abs() < 1e-6);
        assert!((mat.sun_specular_power - 6.0).abs() < 1e-6);
    }

    #[test]
    fn resolve_water_material_blends_fo4_silt_into_water_palette() {
        let rec = calm_watr(
            0x000A_000A,
            "MuddyWater",
            WaterParams {
                shallow_color: [0.8, 0.8, 0.8],
                deep_color: [0.8, 0.8, 0.8],
                underwater_color: [0.8, 0.8, 0.8],
                alpha_controls: [0.35, 0.9, 12.0, 240.0],
                silt_amount: 0.5,
                silt_light_color: [1.0, 0.0, 0.0],
                silt_dark_color: [0.0, 0.0, 0.0],
                ..WaterParams::default()
            },
        );
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (mat, _, _, _, _) = resolve_water_material(&waters, Some(0x000A_000A), GameKind::Skyrim);
        assert!(mat.shallow_color[0] > mat.shallow_color[1]);
        assert!(mat.deep_color.iter().all(|channel| *channel < 0.8));
        assert_eq!(mat.deep_color, mat.underwater_color);
        assert_eq!(mat.alpha_controls, [0.35, 0.9, 12.0, 240.0]);
    }

    #[test]
    fn resolve_water_material_carries_authored_noise_uv_scales() {
        let rec = calm_watr(
            0x000A_0003,
            "DefaultWater",
            WaterParams {
                noise_uv_scale_a: 1.0 / 320.0,
                noise_uv_scale_b: 1.0 / 760.0,
                ..WaterParams::default()
            },
        );
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);

        let (mat, _, _, _, _) = resolve_water_material(&waters, Some(0x000A_0003), GameKind::Skyrim);
        assert!((mat.uv_scale_a - 1.0 / 320.0).abs() < 1e-6);
        assert!((mat.uv_scale_b - 1.0 / 760.0).abs() < 1e-6);
    }

    #[test]
    fn resolve_water_material_applies_authored_normal_magnitude() {
        let rec = calm_watr(
            0x000A_0004,
            "SkyrimWater",
            WaterParams {
                normal_magnitude: 0.5,
                above_water_fog_amount: 0.5,
                noise_amplitude_scales: [0.8, 0.6, 0.4],
                depth_weights: [1.0, 1.0, 1.0, 1.0],
                ..WaterParams::default()
            },
        );
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);

        let (mat, _, _, _, _) = resolve_water_material(&waters, Some(0x000A_0004), GameKind::Skyrim);
        assert_eq!(mat.noise_amplitude_scales, [0.4, 0.3, 0.2]);
        assert_eq!(mat.depth_weights[1], 0.5);
    }

    #[test]
    fn resolve_water_material_carries_authored_opacity() {
        let mut rec = calm_watr(0x000A_0005, "OpaqueWater", WaterParams::default());
        rec.opacity = 0.62;
        rec.opacity_authored = true;
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (mat, _, _, _, _) = resolve_water_material(&waters, Some(0x000A_0005), GameKind::Skyrim);
        assert!((mat.opacity - 0.62).abs() < 1e-6);
    }

    #[test]
    fn resolve_water_material_uses_canonical_opacity_when_anam_is_absent() {
        let rec = calm_watr(0x000A_0008, "DefaultWater", WaterParams::default());
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (mat, _, _, _, _) = resolve_water_material(&waters, Some(0x000A_0008), GameKind::Skyrim);
        assert_eq!(mat.opacity, WaterMaterial::default().opacity);
    }

    #[test]
    fn resolve_water_material_honors_authored_zero_opacity() {
        let mut rec = calm_watr(0x000A_0006, "InvisibleWater", WaterParams::default());
        rec.opacity = 0.0;
        rec.opacity_authored = true;
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (mat, _, _, _, _) = resolve_water_material(&waters, Some(0x000A_0006), GameKind::Skyrim);
        assert_eq!(mat.opacity, 0.0);
    }

    #[test]
    fn resolve_water_material_preserves_low_authored_opacity() {
        let mut rec = calm_watr(0x000A_0007, "ThinWater", WaterParams::default());
        rec.opacity = 0.01;
        rec.opacity_authored = true;
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (mat, _, _, _, _) = resolve_water_material(&waters, Some(0x000A_0007), GameKind::Skyrim);
        assert!((mat.opacity - 0.01).abs() < 1.0e-6);
    }

    #[test]
    fn resolve_water_material_carries_authored_noise_paths() {
        let mut rec = calm_watr(0x000A_0002, "DefaultWater", WaterParams::default());
        rec.noise_texture_paths = [
            "textures/water/noise_a.dds".into(),
            String::new(),
            "textures/water/noise_c.dds".into(),
        ];
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (_, _, _, _, noise) = resolve_water_material(&waters, Some(0x000A_0002), GameKind::Skyrim);
        assert_eq!(noise[0].as_deref(), Some("textures/water/noise_a.dds"));
        assert!(noise[1].is_none());
        assert_eq!(noise[2].as_deref(), Some("textures/water/noise_c.dds"));
    }

    #[test]
    fn flowing_water_promotes_skyrim_flow_normal_over_nam4() {
        let mut rec = calm_watr(0x000A_0006, "RiverWater", WaterParams::default());
        rec.noise_texture_paths[2] = "textures/water/noise_c.dds".into();
        rec.flow_noise_texture_path = "textures/water/flow.dds".into();
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (_, kind, _, _, noise) = resolve_water_material(&waters, Some(0x000A_0006), GameKind::Skyrim);
        assert!(matches!(kind, WaterKind::River));
        assert_eq!(noise[2].as_deref(), Some("textures/water/flow.dds"));
    }

    #[test]
    fn authored_flow_normal_promotes_neutral_water_to_river() {
        let mut rec = calm_watr(0x000A_0007, "DefaultWater", WaterParams::default());
        rec.noise_texture_paths[2] = "textures/water/noise_c.dds".into();
        rec.flow_noise_texture_path = "textures/water/flow.dds".into();
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (_, kind, _, _, noise) = resolve_water_material(&waters, Some(0x000A_0007), GameKind::Skyrim);
        assert!(matches!(kind, WaterKind::River));
        assert_eq!(noise[2].as_deref(), Some("textures/water/flow.dds"));
    }

    #[test]
    fn authored_nam0_velocity_promotes_localized_water_to_river() {
        let mut rec = calm_watr(0x000A_0008, "AguaPrincipal", WaterParams::default());
        rec.linear_velocity = Some([3.0, 0.0]);
        // The authored current points along renderer +X; weather wind points
        // along +Z. Translation must preserve NAM0 rather than substitute the
        // atmospheric direction.
        rec.params.wind_direction = std::f32::consts::FRAC_PI_2;
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (material, kind, flow, _, _) = resolve_water_material(&waters, Some(0x000A_0008), GameKind::Skyrim);
        assert!(matches!(kind, WaterKind::River));
        let flow = flow.expect("NAM0 velocity must produce a canonical current");
        assert!(flow.direction[0] > 0.99);
        assert!((flow.speed - 3.0).abs() < 1.0e-6);
        assert!(material.scroll_a[0] > 0.0);
        assert!(material.scroll_a[1].abs() < 1.0e-6);
        assert!(material.scroll_b[0].abs() < 1.0e-6);
        assert!(material.scroll_b[1] > 0.0);
    }

    #[test]
    fn zero_nam0_velocity_keeps_neutral_water_calm() {
        let mut rec = calm_watr(0x000A_000A, "DefaultWater", WaterParams::default());
        // FO76/Starfield can serialize NAM0 as an explicit all-zero
        // sentinel. It must not manufacture a river profile or a zero-speed
        // current merely because the subrecord exists.
        rec.linear_velocity = Some([0.0, 0.0]);
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);

        let (material, kind, flow, _, _) = resolve_water_material(&waters, Some(0x000A_000A), GameKind::Skyrim);
        assert!(matches!(kind, WaterKind::Calm));
        assert!(flow.is_none());
        assert_eq!(
            material.foam_strength,
            WaterMaterial::default().foam_strength
        );
    }

    #[test]
    fn zero_nam0_velocity_on_named_river_uses_kind_current() {
        let mut rec = calm_watr(
            0x000A_000B,
            "RiverWater",
            WaterParams {
                // An authored heading, so the NAM0-less fallback runs (#4931).
                wind_direction_authored: true,
                ..WaterParams::default()
            },
        );
        // A zero NAM0 is an explicit sentinel on newer records; the EDID
        // still identifies this surface as a flowing body, and its authored
        // heading supplies the axis (#4734 — the name alone does not).
        rec.linear_velocity = Some([0.0, 0.0]);
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);

        let (material, kind, flow, _, _) = resolve_water_material(&waters, Some(0x000A_000B), GameKind::Skyrim);
        assert!(matches!(kind, WaterKind::River));
        let flow = flow.expect("named river must retain its fallback current");
        assert!(flow.speed > 0.0);
        assert!(material.scroll_a[0].hypot(material.scroll_a[1]) > 0.0);
    }

    #[test]
    fn authored_nam0_rapid_velocity_selects_rapids_profile() {
        let mut rec = calm_watr(0x000A_0009, "AguaRapida", WaterParams::default());
        rec.linear_velocity = Some([0.0, 12.0]);
        rec.params.wind_direction = std::f32::consts::FRAC_PI_2;
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (mat, kind, flow, _, _) = resolve_water_material(&waters, Some(0x000A_0009), GameKind::Skyrim);
        assert!(matches!(kind, WaterKind::Rapids));
        assert_eq!(mat.foam_strength, 0.85);
        assert!((flow.expect("rapid NAM0 flow").speed - 12.0).abs() < 1.0e-6);
    }

    #[test]
    fn calm_water_uses_authored_normal_layer_wind_without_touching_flow() {
        let params = WaterParams {
            noise_wind_directions: [std::f32::consts::FRAC_PI_2, 0.0, 0.0],
            noise_wind_speeds: [0.03, 0.02, 0.0],
            ..Default::default()
        };
        let rec = calm_watr(0x000A_0003, "DefaultWater", params);
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (mat, kind, flow, _, _) = resolve_water_material(&waters, Some(0x000A_0003), GameKind::Skyrim);
        assert!(matches!(kind, WaterKind::Calm));
        assert!(flow.is_none());
        // #4727 — raw bearing π/2 reads as engine −X under the corrected
        // frame (φ = β + 90°), and raw 0 reads as +Z.
        assert!((mat.scroll_a[0] - -0.03).abs() < 1e-6);
        assert!(mat.scroll_a[1].abs() < 1e-6);
        assert!(mat.scroll_b[0].abs() < 1e-6);
        assert!((mat.scroll_b[1] - 0.02).abs() < 1e-6);
    }

    // ── #2872 — WaterFlow.speed unit ──────────────────────────────

    /// The regression itself. Every vanilla FO3 / FNV / Skyrim WATR whose
    /// DATA is 196 bytes (or DNAM 228) carries `90.0` in the float the
    /// parser reads as `wind_speed` — a constant, the same value the shorter
    /// legacy layouts put in the *direction* slot. Feeding it straight into
    /// `WaterFlow::speed` made the physics current's terminal velocity 90
    /// BU/s (3.6× the documented ceiling) and blew `scroll_a` out to ~45
    /// against a documented default of `[0.020, 0.011]`.
    ///
    /// The canonical speed must now come from the `WaterKind` band and be
    /// completely independent of that field.
    #[test]
    fn flow_speed_ignores_the_watr_wind_field_and_stays_in_band() {
        // The real vanilla value, and two adversarial ones.
        for wind_speed in [90.0_f32, 0.0, -1e9] {
            let rec = calm_watr(
                0x000B_0001,
                "WaterRiverFallingSlow",
                WaterParams {
                    wind_speed,
                    wind_direction: 0.0,
                    // An authored heading, so the fallback runs (#4931).
                    wind_direction_authored: true,
                    ..WaterParams::default()
                },
            );
            let mut waters = HashMap::new();
            waters.insert(rec.form_id, rec);

            let (mat, kind, flow, _, _) = resolve_water_material(&waters, Some(0x000B_0001), GameKind::Skyrim);
            let flow = flow.expect("a river EDID must synthesize a flow");
            assert!(matches!(kind, WaterKind::River));
            assert_eq!(
                flow.speed,
                WaterFlow::speed_for_kind(WaterKind::River),
                "speed must come from the kind, not from wind_speed={wind_speed}"
            );
            assert!(
                (WaterFlow::SPEED_MIN..=WaterFlow::SPEED_MAX).contains(&flow.speed),
                "speed {} escaped the documented band",
                flow.speed
            );
            // The shader scroll is derived from the same canonical scalar,
            // so it can no longer diverge from the physics current.
            let scroll = (mat.scroll_a[0].powi(2) + mat.scroll_a[1].powi(2)).sqrt();
            assert!(
                scroll < 0.05,
                "scroll_a magnitude {scroll} is nowhere near the documented \
                 [0.020, 0.011] default — the pre-fix value was ~45"
            );
        }
    }

    /// Rapids are faster than rivers, and the whole ladder stays inside the
    /// band `WaterFlow::speed` documents — so `current_force`'s terminal
    /// velocity target is bounded for every kind the translate site emits.
    #[test]
    fn flow_speed_ladder_is_ordered_and_bounded() {
        let mut waters = HashMap::new();
        for (form, edid) in [(0x000C_0001, "WhiteRapidsFast"), (0x000C_0002, "RiverSlow")] {
            let params = WaterParams {
                // An authored heading, so the NAM0-less fallback runs (#4931).
                wind_direction_authored: true,
                ..WaterParams::default()
            };
            waters.insert(form, calm_watr(form, edid, params));
        }
        let (_, rapids_kind, rapids, _, _) = resolve_water_material(&waters, Some(0x000C_0001), GameKind::Skyrim);
        let (_, river_kind, river, _, _) = resolve_water_material(&waters, Some(0x000C_0002), GameKind::Skyrim);
        assert!(matches!(rapids_kind, WaterKind::Rapids));
        assert!(matches!(river_kind, WaterKind::River));

        let rapids = rapids.expect("rapids flow");
        let river = river.expect("river flow");
        assert!(
            rapids.speed > river.speed,
            "rapids ({}) must run faster than a river ({})",
            rapids.speed,
            river.speed
        );
        for speed in [rapids.speed, river.speed] {
            assert!((WaterFlow::SPEED_MIN..=WaterFlow::SPEED_MAX).contains(&speed));
        }
    }

    /// Calm water carries no flow at all, so the physics current never
    /// engages on a lake and the material keeps its default scroll.
    #[test]
    fn calm_water_carries_no_flow_and_default_scroll() {
        let rec = calm_watr(0x000D_0001, "DefaultWater", WaterParams::default());
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);

        let (mat, kind, flow, _, _) = resolve_water_material(&waters, Some(0x000D_0001), GameKind::Skyrim);
        assert!(matches!(kind, WaterKind::Calm));
        assert!(flow.is_none());
        assert_eq!(mat.scroll_a, WaterMaterial::default().scroll_a);
    }

    /// WATAL translate-up contract (docs/engine/watal.md §3/§4): a poorer
    /// (Oblivion-shaped) record and a richer (Skyrim-shaped) record that
    /// author *different* appearance values must still resolve to
    /// **identical SENTINEL fields** — the engine-default fields no game
    /// authors. They may differ ONLY in AUTHORED fields. This is what lets
    /// the renderer + solver consume one `WaterMaterial` regardless of the
    /// source game.
    ///
    /// #4486 — this synthetic guard now pins the **full** §4 sentinel list,
    /// not a 5-scalar selection (pre-fix `noise_map_indices`, the wave
    /// defaults, the underwater-fog/depth/absorption zeros,
    /// `blend_normals`, the specular sentinels and the GNAM-less deep-tint
    /// mirror were all unpinned). What a synthetic literal structurally
    /// cannot see — the sentinel set the *parser* decides per game
    /// (FO3/FNV `OffsetNoise`, Oblivion NNAM-vs-Skyrim TNAM texture roles,
    /// the pre-Skyrim formats' missing tail bytes) — is covered by
    /// `water_sentinels_hold_on_real_watr_per_game` below.
    ///
    /// #5184 — since #4910 the translate takes a `GameKind`, so a future
    /// per-game `match` that sets a sentinel field (`ior`, `uv_scale_*`,
    /// `shoreline_width`, …) is now expressible at this boundary. The guard
    /// resolves **both records under every `GameKind`**: within a game the
    /// two shapes must agree, and every game must land on the canonical
    /// default — which transitively pins cross-game invariance.
    #[test]
    fn resolve_water_material_sentinels_are_game_invariant() {
        // "Oblivion-shaped": sparse DATA — colours + a short fog only.
        let oblivion = calm_watr(
            0x0001_0000,
            "OblivionLake",
            WaterParams {
                shallow_color: [0.08, 0.20, 0.24],
                deep_color: [0.01, 0.04, 0.06],
                fog_near: 80.0,
                fog_far: 600.0,
                ..WaterParams::default()
            },
        );
        // "Skyrim-shaped": richer authored colours/fog/reflectivity.
        let skyrim = calm_watr(
            0x0002_0000,
            "SkyrimLake",
            WaterParams {
                shallow_color: [0.12, 0.36, 0.42],
                deep_color: [0.02, 0.07, 0.11],
                fog_near: 120.0,
                fog_far: 1200.0,
                reflectivity: 0.92,
                ..WaterParams::default()
            },
        );

        let mut waters = HashMap::new();
        waters.insert(oblivion.form_id, oblivion);
        waters.insert(skyrim.form_id, skyrim);

        for game in [
            GameKind::Oblivion,
            GameKind::Fallout3NV,
            GameKind::Skyrim,
            GameKind::Fallout4,
            GameKind::Fallout76,
            GameKind::Starfield,
        ] {
            let (ob, ob_kind, ob_flow, _, _) =
                resolve_water_material(&waters, Some(0x0001_0000), game);
            let (sk, _, _, _, _) = resolve_water_material(&waters, Some(0x0002_0000), game);
            let def = WaterMaterial::default();

            // AUTHORED fields differ (proves the two records are distinct).
            assert_ne!(
                ob.fog_far, sk.fog_far,
                "authored fog must differ between the two records ({game:?})"
            );

            // SENTINEL fields no game authors must be identical across games
            // AND equal to the canonical default — the translate-up invariant.
            // #4486: the loop used to pin 5 scalars; it now walks the full §4
            // sentinel list, including the rows the unpinned-list audit named
            // (underwater fog/depth zeros, specular sentinels, the wave
            // defaults, absorption/concentration Starfield columns).
            for (label, a, b, d) in [
                ("ior", ob.ior, sk.ior, def.ior),
                (
                    "shoreline_width",
                    ob.shoreline_width,
                    sk.shoreline_width,
                    def.shoreline_width,
                ),
                ("uv_scale_a", ob.uv_scale_a, sk.uv_scale_a, def.uv_scale_a),
                ("uv_scale_b", ob.uv_scale_b, sk.uv_scale_b, def.uv_scale_b),
                ("uv_scale_c", ob.uv_scale_c, sk.uv_scale_c, def.uv_scale_c),
                ("depth_amount", ob.depth_amount, sk.depth_amount, def.depth_amount),
                (
                    "underwater_fog_near",
                    ob.underwater_fog_near,
                    sk.underwater_fog_near,
                    def.underwater_fog_near,
                ),
                (
                    "underwater_fog_far",
                    ob.underwater_fog_far,
                    sk.underwater_fog_far,
                    def.underwater_fog_far,
                ),
                (
                    "underwater_fog_amount",
                    ob.underwater_fog_amount,
                    sk.underwater_fog_amount,
                    def.underwater_fog_amount,
                ),
                (
                    "specular_magnitude",
                    ob.specular_magnitude,
                    sk.specular_magnitude,
                    def.specular_magnitude,
                ),
                (
                    "specular_radius",
                    ob.specular_radius,
                    sk.specular_radius,
                    def.specular_radius,
                ),
                (
                    "normal_magnitude",
                    ob.normal_magnitude,
                    sk.normal_magnitude,
                    def.normal_magnitude,
                ),
                (
                    "above_water_fog_amount",
                    ob.above_water_fog_amount,
                    sk.above_water_fog_amount,
                    def.above_water_fog_amount,
                ),
                ("flowmap_scale", ob.flowmap_scale, sk.flowmap_scale, def.flowmap_scale),
                ("noise_falloff", ob.noise_falloff, sk.noise_falloff, def.noise_falloff),
                (
                    "angular_velocity",
                    ob.angular_velocity,
                    sk.angular_velocity,
                    def.angular_velocity,
                ),
                // WATAL Phase 1 defaults — the unauthored wave shape is the
                // named canonical constant, not a local literal.
                (
                    "wave_amplitude",
                    ob.wave_amplitude,
                    sk.wave_amplitude,
                    byroredux_core::ecs::components::water::DEFAULT_WATER_WAVE_AMPLITUDE,
                ),
                (
                    "wave_frequency",
                    ob.wave_frequency,
                    sk.wave_frequency,
                    byroredux_core::ecs::components::water::DEFAULT_WATER_WAVE_FREQUENCY,
                ),
                (
                    "foam_strength",
                    ob.foam_strength,
                    sk.foam_strength,
                    def.foam_strength,
                ),
            ] {
                assert_eq!(
                    a, b,
                    "SENTINEL `{label}` must be game-invariant ({game:?})"
                );
                assert_eq!(
                    a, d,
                    "SENTINEL `{label}` must equal the canonical default ({game:?})"
                );
            }
            // Array sentinels — each shape checked as its own pair.
            assert_eq!(
                ob.alpha_controls, sk.alpha_controls,
                "SENTINEL must be game-invariant ({game:?})"
            );
            assert_eq!(
                ob.alpha_controls, def.alpha_controls,
                "SENTINEL must equal the default ({game:?})"
            );
            assert_eq!(
                ob.depth_weights, sk.depth_weights,
                "SENTINEL must be game-invariant ({game:?})"
            );
            assert_eq!(
                ob.depth_weights, def.depth_weights,
                "SENTINEL must equal the default ({game:?})"
            );
            assert_eq!(
                ob.effect_controls, sk.effect_controls,
                "SENTINEL must be game-invariant ({game:?})"
            );
            assert_eq!(
                ob.effect_controls, def.effect_controls,
                "SENTINEL must equal the default ({game:?})"
            );
            assert_eq!(
                ob.noise_amplitude_scales, sk.noise_amplitude_scales,
                "SENTINEL must be game-invariant ({game:?})"
            );
            assert_eq!(
                ob.noise_amplitude_scales, def.noise_amplitude_scales,
                "SENTINEL must equal the default ({game:?})"
            );
            assert_eq!(
                ob.normal_falloff, sk.normal_falloff,
                "SENTINEL must be game-invariant ({game:?})"
            );
            assert_eq!(
                ob.normal_falloff, def.normal_falloff,
                "SENTINEL must equal the default ({game:?})"
            );
            assert_eq!(
                ob.absorption_coefficients, sk.absorption_coefficients,
                "SENTINEL must be game-invariant ({game:?})"
            );
            assert_eq!(
                ob.absorption_coefficients, def.absorption_coefficients,
                "SENTINEL must equal the default ({game:?})"
            );
            assert_eq!(
                ob.concentration, sk.concentration,
                "SENTINEL must be game-invariant ({game:?})"
            );
            assert_eq!(
                ob.concentration, def.concentration,
                "SENTINEL must equal the default ({game:?})"
            );
            assert_eq!(
                ob.uv_offset, sk.uv_offset,
                "SENTINEL must be game-invariant ({game:?})"
            );
            assert_eq!(
                ob.uv_offset, def.uv_offset,
                "SENTINEL must equal the default ({game:?})"
            );
            assert_eq!(
                ob.blend_normals, sk.blend_normals,
                "SENTINEL must be game-invariant ({game:?})"
            );
            assert!(
                ob.blend_normals,
                "no FNAM blend bit authored → compatibility default ({game:?})"
            );
            // Procedural bindless sentinels — handles are assigned downstream of
            // this boundary, so `resolve` must leave them unset.
            assert_eq!(
                ob.normal_map_index,
                u32::MAX,
                "no texture authored → procedural sentinel ({game:?})"
            );
            assert_eq!(
                ob.flow_map_index,
                u32::MAX,
                "cell WATR → no mesh flow map ({game:?})"
            );
            assert_eq!(
                ob.noise_map_indices,
                [u32::MAX; 3],
                "no noise layers authored → procedural sentinel ({game:?})"
            );
            // Deep-tint fallback: no GNAM variant on either record, so the
            // day/night palettes mirror the record's own base deep tint instead
            // of inventing a per-variant value.
            assert_eq!(
                ob.day_deep_color, ob.deep_color,
                "day palette mirrors base without GNAM ({game:?})"
            );
            assert_eq!(
                ob.night_deep_color, ob.deep_color,
                "night palette mirrors base without GNAM ({game:?})"
            );
            assert_eq!(
                sk.day_deep_color, sk.deep_color,
                "day palette mirrors base without GNAM ({game:?})"
            );
            assert_eq!(
                sk.night_deep_color, sk.deep_color,
                "night palette mirrors base without GNAM ({game:?})"
            );
            // Calm water authors no flow — a real distinction, not a leak.
            assert!(matches!(ob_kind, WaterKind::Calm));
            assert!(
                ob_flow.is_none(),
                "calm water has no synthesized flow ({game:?})"
            );
        }
    }

    /// #4486 / EXT-D5-2026-09-19-02 — the §4 sentinel invariant against
    /// **real parsed records**. The synthetic guard above cannot see what
    /// the parser decides per game, so this harness decodes one real WATR
    /// from each vanilla master behind the repo's `BYROREDUX_*_DATA`
    /// fixture env vars (same convention as the sound-candidate harness in
    /// `asset_provider::texture`) and resolves it through the boundary,
    /// pinning per game:
    ///
    /// - the parse-decided `WaterNormalEncoding` (FO3/FNV `OffsetNoise`,
    ///   everyone else `TangentNormal`) — the SIBLING completeness check;
    /// - the engine-invariant scalars no game authors (`ior`,
    ///   `shoreline_width`, foam-by-kind) and the procedural bindless
    ///   sentinels (handles are assigned downstream of this boundary);
    /// - the Starfield-only zero columns (`absorption_coefficients`,
    ///   `concentration`);
    /// - for the pre-Skyrim formats, the §4 SENTINEL rows their DATA/DNAM
    ///   layout has no bytes for (depth ramp, below-water fog split,
    ///   specular controls, depth-response weights, noise UV scales, …).
    ///
    /// Each game is skipped independently when its master is not on disk.
    /// Run with:
    /// ```sh
    /// BYROREDUX_OBLIVION_DATA=<path> BYROREDUX_FNV_DATA=<path> \
    /// BYROREDUX_FO3_DATA=<path> BYROREDUX_SKYRIMSE_DATA=<path> \
    /// BYROREDUX_FO4_DATA=<path> \
    ///     cargo test -p byroredux --bin byroredux \
    ///     water_sentinels_hold_on_real_watr_per_game -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "needs vanilla game masters on disk (parses one ESM per game)"]
    fn water_sentinels_hold_on_real_watr_per_game() {
        use byroredux_core::ecs::components::water::WaterNormalEncoding;
        use std::path::PathBuf;

        const STEAM: &str = "/mnt/data/SteamLibrary/steamapps/common";

        /// Which §4 SENTINEL rows the game's WATR payload has no bytes for.
        /// Grounded in the per-era decoders (`crates/plugin/.../misc/water.rs`),
        /// not the §4 table's column labels: the FO3/FNV long-DATA layout
        /// DOES author the underwater-fog / specular / noise-UV tail, so
        /// only the depth-response, flow-map, normal-falloff, noise-falloff
        /// and Starfield columns stay sentinel there.
        #[derive(Clone, Copy)]
        enum Layout {
            /// TES4 short DATA (+ NAM1): no modern-tail bytes at all.
            Tes4Short,
            /// FO3/FNV 186/196-byte long DATA/DNAM.
            FalloutLong,
            /// Skyrim 228/232-byte DNAM / FO4 201-byte DNAM — the canonical,
            /// most-authored tail.
            Modern,
        }

        // (label, env var, default Data dir, master, parse-decided normal
        // encoding, layout, engine GameKind — #4910: the wind-angle frame
        // is scoped per game, so the real-data sweep must resolve each
        // master under its own frame)
        const GAMES: [(&str, &str, &str, &str, WaterNormalEncoding, Layout, GameKind); 5] = [
            (
                "oblivion",
                "BYROREDUX_OBLIVION_DATA",
                "Oblivion/Data",
                "Oblivion.esm",
                WaterNormalEncoding::TangentNormal,
                Layout::Tes4Short,
                GameKind::Oblivion,
            ),
            (
                "fnv",
                "BYROREDUX_FNV_DATA",
                "Fallout New Vegas/Data",
                "FalloutNV.esm",
                WaterNormalEncoding::OffsetNoise,
                Layout::FalloutLong,
                GameKind::Fallout3NV,
            ),
            (
                "fo3",
                "BYROREDUX_FO3_DATA",
                "Fallout 3 goty/Data",
                "Fallout3.esm",
                WaterNormalEncoding::OffsetNoise,
                Layout::FalloutLong,
                GameKind::Fallout3NV,
            ),
            (
                "skyrimse",
                "BYROREDUX_SKYRIMSE_DATA",
                "Skyrim Special Edition/Data",
                "Skyrim.esm",
                WaterNormalEncoding::TangentNormal,
                Layout::Modern,
                GameKind::Skyrim,
            ),
            (
                "fo4",
                "BYROREDUX_FO4_DATA",
                "Fallout 4/Data",
                "Fallout4.esm",
                WaterNormalEncoding::TangentNormal,
                Layout::Modern,
                GameKind::Fallout4,
            ),
        ];

        let mut games_checked = 0usize;
        for (label, env_var, default_dir, master, encoding, layout, game) in GAMES {
            // An explicitly-set override is binding (#3850 convention); the
            // documented Steam layout is only the fallback.
            let dir = std::env::var_os(env_var)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(STEAM).join(default_dir));
            let master_path = dir.join(master);
            if !master_path.is_file() {
                eprintln!("skipping {label}: {} not found", master_path.display());
                continue;
            }
            let Ok(bytes) = std::fs::read(&master_path) else {
                eprintln!("skipping {label}: {} unreadable", master_path.display());
                continue;
            };
            let index = byroredux_plugin::esm::parse_esm(&bytes)
                .unwrap_or_else(|err| panic!("{label}: parsing {} failed: {err}", master_path.display()));

            // Deterministic pick: the lowest FormID the master actually
            // ships. The invariant is per-format, not per-record, so any
            // real WATR must hold it.
            let Some((&form, _)) = index.waters.iter().min_by_key(|(&fid, _)| fid) else {
                eprintln!(
                    "skipping {label}: no WATR records parsed from {}",
                    master_path.display()
                );
                continue;
            };

            let (mat, kind, _, _, _) = resolve_water_material(&index.waters, Some(form), game);
            let def = WaterMaterial::default();

            // Parse-decided per-game normal decoding.
            assert_eq!(
                mat.normal_encoding, encoding,
                "{label}: WaterNormalEncoding is decided at parse, not at translate"
            );

            // Bindless texture handles are assigned downstream of this
            // boundary — resolve must leave the procedural sentinels.
            assert_eq!(mat.normal_map_index, u32::MAX, "{label}");
            assert_eq!(mat.flow_map_index, u32::MAX, "{label}");
            assert_eq!(mat.noise_map_indices, [u32::MAX; 3], "{label}");
            // Mesh-water-authored UV offsets never come from a cell WATR.
            assert_eq!(mat.uv_offset, [0.0; 2], "{label}");

            // Engine-invariant shading constants (§4 "engine-invariant" row).
            assert_eq!(mat.ior, def.ior, "{label}");
            assert_eq!(mat.shoreline_width, def.shoreline_width, "{label}");
            assert_eq!(
                mat.foam_strength,
                kind.canonical_foam_strength(),
                "{label}: foam must follow the resolved kind"
            );

            // Starfield-only columns: every pre-Starfield record leaves
            // them at the zero sentinel.
            assert_eq!(mat.absorption_coefficients, [0.0; 3], "{label}");
            assert_eq!(mat.concentration, [0.0; 4], "{label}");

            match layout {
                Layout::Tes4Short => {
                    // The pre-Skyrim FNAM contracts have no blend-normals bit.
                    assert!(
                        mat.blend_normals,
                        "{label}: pre-Skyrim FNAM has no blend-normals bit"
                    );
                    // TES4's short DATA has no bytes for any of these, so a
                    // value off the canonical default means the decoder
                    // started promoting a sentinel — the exact whole-game
                    // rendering change this harness exists to stop.
                    for (field, value, expected) in [
                        ("depth_amount", mat.depth_amount, def.depth_amount),
                        (
                            "underwater_fog_near",
                            mat.underwater_fog_near,
                            def.underwater_fog_near,
                        ),
                        ("underwater_fog_far", mat.underwater_fog_far, def.underwater_fog_far),
                        (
                            "underwater_fog_amount",
                            mat.underwater_fog_amount,
                            def.underwater_fog_amount,
                        ),
                        ("above_water_fog_amount", mat.above_water_fog_amount, def.above_water_fog_amount),
                        ("specular_magnitude", mat.specular_magnitude, def.specular_magnitude),
                        ("specular_radius", mat.specular_radius, def.specular_radius),
                        ("normal_magnitude", mat.normal_magnitude, def.normal_magnitude),
                        ("flowmap_scale", mat.flowmap_scale, def.flowmap_scale),
                        ("noise_falloff", mat.noise_falloff, def.noise_falloff),
                        ("roughness", mat.roughness, def.roughness),
                        ("uv_scale_a", mat.uv_scale_a, def.uv_scale_a),
                        ("uv_scale_b", mat.uv_scale_b, def.uv_scale_b),
                        ("uv_scale_c", mat.uv_scale_c, def.uv_scale_c),
                    ] {
                        assert_eq!(
                            value, expected,
                            "{label}: TES4 DATA cannot author `{field}` — a §4 \
                             SENTINEL row leaked into the decode"
                        );
                    }
                    assert_eq!(mat.effect_controls, def.effect_controls, "{label}");
                    assert_eq!(mat.depth_weights, def.depth_weights, "{label}");
                    assert_eq!(mat.noise_amplitude_scales, def.noise_amplitude_scales, "{label}");
                    assert_eq!(mat.normal_falloff, def.normal_falloff, "{label}");
                }
                Layout::FalloutLong => {
                    // The FO3/FNV FNAM contract has no blend-normals bit.
                    assert!(
                        mat.blend_normals,
                        "{label}: FO3/FNV FNAM has no blend-normals bit"
                    );
                    // The FO3/FNV long DATA/DNAM authors the underwater-fog
                    // pair (140/144/148), the specular Light Radius/Brightness
                    // pair (164/168), the effect-control head (152/156), the
                    // noise UV scales and amplitudes — those are AUTHORED here
                    // and deliberately NOT pinned. The depth-response,
                    // flow-map, normal-falloff and noise-falloff columns have
                    // no Fallout bytes and must stay sentinel.
                    for (field, value, expected) in [
                        ("depth_amount", mat.depth_amount, def.depth_amount),
                        ("normal_magnitude", mat.normal_magnitude, def.normal_magnitude),
                        ("flowmap_scale", mat.flowmap_scale, def.flowmap_scale),
                        ("noise_falloff", mat.noise_falloff, def.noise_falloff),
                        ("roughness", mat.roughness, def.roughness),
                    ] {
                        assert_eq!(
                            value, expected,
                            "{label}: the FO3/FNV layout cannot author `{field}` — \
                             a §4 SENTINEL row leaked into the decode"
                        );
                    }
                    assert_eq!(mat.depth_weights, def.depth_weights, "{label}");
                    assert_eq!(mat.normal_falloff, def.normal_falloff, "{label}");
                }
                Layout::Modern => {
                    // The Skyrim/FO4 decoders author the full canonical tail
                    // except `depth_amount`, which only the FO4/FO76 DNAM
                    // layouts carry — Skyrim's 228/232-byte records leave it
                    // at the zero sentinel even though §4 groups them in the
                    // canonical column.
                    if label == "skyrimse" {
                        assert_eq!(
                            mat.depth_amount, def.depth_amount,
                            "{label}: Skyrim DNAM has no depth-amount byte — a §4 \
                             SENTINEL row leaked into the decode"
                        );
                    }
                }
            }

            games_checked += 1;
        }
        assert!(
            games_checked > 0,
            "no game master was found — set BYROREDUX_*_DATA or install the \
             vanilla games under {STEAM}"
        );
    }

    /// Regression pin for #1997 (REN-D15-01) — the returned `normal_path`
    /// (4th tuple element) is what the caller (`spawn_water_plane`) uses
    /// to decide whether a water plane takes the procedural shader branch
    /// vs. the bound-texture branch. Three ways a plane can fall through
    /// to the procedural default, all of which the shader-side fix
    /// (render-origin-relative hashing) now assumes is the COMMON case,
    /// not an edge case:
    ///
    /// 1. no XCWT on the cell at all (`xcwt_form = None`)
    /// 2. XCWT present but the form doesn't resolve in `waters`
    /// 3. XCWT resolves but the WATR's `texture_path` is empty (e.g. a
    ///    lava pool — mirrors the `LavaPool01` fixture used above)
    ///
    /// A fourth case with a populated `texture_path` proves the positive
    /// side: normal_path IS produced when the format actually authors one.
    #[test]
    fn resolve_water_material_procedural_default_classification() {
        // Case 1: no XCWT at all.
        let (_, _, _, normal_none, _) = resolve_water_material(&HashMap::new(), None, GameKind::Skyrim);
        assert!(
            normal_none.is_none(),
            "no XCWT must classify as procedural default"
        );

        // Case 2: XCWT present but unresolvable form.
        let (_, _, _, normal_unresolved, _) =
            resolve_water_material(&HashMap::new(), Some(0x00DE_AD00), GameKind::Skyrim);
        assert!(
            normal_unresolved.is_none(),
            "unresolvable XCWT must classify as procedural default"
        );

        // Case 3: XCWT resolves, WATR has an empty texture_path.
        let no_tex = calm_watr(0x000B_0001, "LavaPool01", WaterParams::default());
        let mut waters = HashMap::new();
        waters.insert(no_tex.form_id, no_tex);
        let (_, _, _, normal_empty_tex, _) = resolve_water_material(&waters, Some(0x000B_0001), GameKind::Skyrim);
        assert!(
            normal_empty_tex.is_none(),
            "WATR with empty texture_path must classify as procedural default"
        );

        // Case 4 (contrast): non-empty texture_path must resolve to Some.
        let mut with_tex = calm_watr(0x000B_0002, "DefaultWater", WaterParams::default());
        with_tex.texture_path = "textures\\water\\defaultwater.dds".to_string();
        let mut waters2 = HashMap::new();
        waters2.insert(with_tex.form_id, with_tex);
        let (_, _, _, normal_with_tex, _) = resolve_water_material(&waters2, Some(0x000B_0002), GameKind::Skyrim);
        assert_eq!(
            normal_with_tex,
            Some("textures\\water\\defaultwater.dds".to_string()),
            "WATR with a texture_path must NOT classify as procedural default"
        );
    }
}
