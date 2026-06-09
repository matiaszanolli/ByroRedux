//! EXAL (Exterior Abstraction Layer) — the exterior **environment**
//! translation boundary.
//!
//! This module is to outdoors rendering what [`crate::material_translate`]
//! is to materials: the single home where per-game [`byroredux_plugin`]
//! ESM records (WRLD / CLMT / WTHR / LAND / WATR / CELL lighting) are
//! resolved into the engine's canonical, game-agnostic representation.
//! Everything downstream — the sky pass, the terrain pass, the water
//! pass, the sun directional light, the LOD ring — consumes the canonical
//! resources identically for every game, with no per-game branches and no
//! render-time fallbacks.
//!
//! Architecture + rollout: see `docs/engine/exal.md`. Per the canonical-
//! type rule it shares with NIFAL (`docs/engine/nifal.md`), the canonical
//! tier is the ECS resource/component that already serves the renderer-
//! facing role ([`WaterMaterial`], `SkyParamsRes`, `WeatherDataRes`,
//! `CellLightingRes`, …); this module is the `translate()` step, not a new
//! type.
//!
//! Step 1 (this slice) establishes the module and gathers the two
//! already-single-site **water** translates here verbatim:
//! [`default_water_for_worldspace`] (worldspace-default water height +
//! type) and [`resolve_water_material`] (WATR → [`WaterMaterial`]). Later
//! steps fold in the scattered sky / sun / weather producers (see the
//! `docs/engine/exal.md` §7 rollout).

use std::collections::HashMap;

use byroredux_core::ecs::components::water::{
    SubmersionState, WaterFlow, WaterKind, WaterMaterial,
};
use byroredux_plugin::esm;
use byroredux_plugin::esm::cell::WorldspaceRecord;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::{ClimateRecord, WeatherRecord};

use crate::components::{CellLightingRes, DalcCubeYup, SkyParamsRes, WeatherDataRes};

/// Resolve the worldspace-default water for exterior cells with no XCLW.
/// Returns `(default height, default water-type form)`.
///
/// Two sources, by game:
/// - **Oblivion**: WRLD carries no DNAM (verified: 0 across all 84 WRLD in
///   `Oblivion.esm`), so the default height is the global Tamriel sea level
///   Z=0 (user-confirmed), gated on the worldspace having a NAM2 water form.
/// - **FO3/FNV/Skyrim+/FO4**: the default height comes from the WRLD DNAM
///   "Land Data" second f32 (`WorldspaceRecord::default_water_height`) — it
///   is game/worldspace-specific and NOT 0 (e.g. WastelandNV -2300, Skyrim
///   Tamriel -14000), so Z=0 would be wrong. The 8-byte DNAM layout is
///   stable across Gamebryo (FNV) and Creation (Skyrim) — verified against
///   both masters. The NAM2 `water_form` supplies the water type/appearance.
///
/// Both `None` when the worldspace has no default water (no NAM2 on
/// Oblivion / no DNAM elsewhere). #1305 / OBL-D6-NEW-02.
///
/// This is the prototype of the EXAL GameVariant table (`docs/engine/exal.md`
/// §4): the one place the per-`GameKind` default-water decision lives.
pub(crate) fn default_water_for_worldspace(
    wrld: Option<&WorldspaceRecord>,
    game: GameKind,
) -> (Option<f32>, Option<u32>) {
    let Some(w) = wrld else {
        return (None, None);
    };
    if game == GameKind::Oblivion {
        // No DNAM on Oblivion WRLD → sea level Z=0, only where the
        // worldspace advertises default water via NAM2.
        return match w.water_form {
            Some(water_form) => (Some(0.0), Some(water_form)),
            None => (None, None),
        };
    }
    // FO3/FNV/Skyrim+/FO4: the DNAM default water height is the signal that
    // the worldspace has default water; pair it with the NAM2 type form.
    match w.default_water_height {
        Some(height) => (Some(height), w.water_form),
        None => (None, None),
    }
}

/// Resolve a cell's `XCWT` FormID to an engine [`WaterMaterial`]
/// plus a [`WaterKind`] (currently always `Calm`) plus an optional
/// [`WaterFlow`] and an optional normal-texture path the cell loader
/// should attempt to bind.
///
/// `xcwt_form == None` (no WATR reference on the cell) falls back to
/// engine defaults — same shape Skyrim uses for unmodded cells that
/// rely on the worldspace water-default cascade.
pub(crate) fn resolve_water_material(
    waters: &HashMap<u32, esm::records::misc::WatrRecord>,
    xcwt_form: Option<u32>,
) -> (WaterMaterial, WaterKind, Option<WaterFlow>, Option<String>) {
    let mut mat = WaterMaterial::default();
    let mut kind = WaterKind::Calm;
    let mut flow: Option<WaterFlow> = None;
    let mut normal_path: Option<String> = None;

    if let Some(form) = xcwt_form {
        if let Some(rec) = waters.get(&form) {
            mat.shallow_color = rec.params.shallow_color;
            mat.deep_color = rec.params.deep_color;
            mat.fog_near = rec.params.fog_near;
            mat.fog_far = rec.params.fog_far;
            mat.fresnel_f0 = rec.params.fresnel.clamp(0.001, 0.20);
            mat.reflectivity = rec.params.reflectivity;
            mat.reflection_tint = rec.params.reflection_color;
            mat.source_form = rec.form_id;

            // ── WaterKind heuristic from EDID naming convention ──
            //
            // Cell-level water planes are **always horizontal**
            // (XCLW provides a Y height; the mesh is a flat quad).
            // The `Waterfall` kind in the shader is for vertical
            // sheet geometry (cliff-side falling water), which the
            // cell loader does NOT spawn — those land as standalone
            // mesh refs through the regular NIF import path. So
            // any EDID match that would otherwise promote a cell
            // plane to `Waterfall` is demoted to `River` here: the
            // horizontal plane below a waterfall is a fast,
            // turbulent pool, not a falling sheet, and the River
            // shader path is the correct visual.
            //
            // Skyrim has many WATR records whose names contain
            // "fall"/"waterfall" but are applied to horizontal
            // bodies of water (e.g. `DLC2WaterFallingStream`,
            // `WaterFallingPool`, `WaterRiverFallingSlow`). The
            // pre-fix heuristic mis-classified these and the
            // shader's Waterfall mode painted heavy fizz foam
            // across whole exterior cells — see the May 2026
            // smoke-test screenshot reported alongside this
            // change.
            let lowered = rec.editor_id.to_ascii_lowercase();
            if lowered.contains("rapid") {
                kind = WaterKind::Rapids;
                mat.foam_strength = 0.85;
            } else if lowered.contains("waterfall")
                || lowered.contains("falls")
                || lowered.contains("river")
                || lowered.contains("stream")
            {
                kind = WaterKind::River;
                mat.foam_strength = 0.20;
            }
            // Synthesise a flow vector from WATR's wind speed +
            // direction when the kind implies flow. Bethesda's
            // wind_direction is in radians from north (UESP).
            if !matches!(kind, WaterKind::Calm) {
                let theta = rec.params.wind_direction;
                // Compute once — cos/sin were duplicated pre-#1068 (F-WAT-06).
                let (sin_theta, cos_theta) = theta.sin_cos();
                let speed = rec.params.wind_speed.abs().max(0.5);
                flow = Some(WaterFlow {
                    direction: [cos_theta, 0.0, sin_theta],
                    speed,
                });
                // Rebuild scroll vectors to bias along the flow axis.
                mat.scroll_a = [cos_theta * speed * 0.5, sin_theta * speed * 0.5];
                // Perpendicular shear at half speed for the second layer.
                mat.scroll_b = [-sin_theta * speed * 0.25, cos_theta * speed * 0.25];
            }
            // TNAM is the diffuse / noise texture — used as the
            // bindless normal map for the shader. Empty path =
            // procedural fallback.
            if !rec.texture_path.is_empty() {
                normal_path = Some(rec.texture_path.clone());
            }
        }
    }

    // SubmersionState is per-actor, not per-plane — but seed a
    // sentinel value on the material itself so debug overlays can
    // see "water without a parsed XCWT" cells.
    let _ = SubmersionState::default();

    (mat, kind, flow, normal_path)
}

// ───────────────────────────────────────────────────────────────────────
// Exterior sky / sun / weather / lighting translation (EXAL step 3)
//
// The WTHR-driven canonical resources (`CellLightingRes`, `SkyParamsRes`,
// `WeatherDataRes`) and the no-climate procedural fallback are built here,
// behind the single boundary. The functions are **pure** — the caller
// (`scene::world_setup::apply_worldspace_weather`) pre-resolves the
// `VulkanContext`-coupled cloud / sun textures into a [`SkyTextures`] and
// hands them in, mirroring `material_translate`'s `ResolvedPaths`. World
// insertion, the bindless-handle lifecycle, and the WTHR cross-fade-vs-insert
// decision stay in the caller (orchestration, not translation).
// ───────────────────────────────────────────────────────────────────────

/// Cos-threshold of the rendered sun-disc half-angle (~1.8°). Matches the
/// pre-EXAL hardcoded `SkyParamsRes` literal.
const SUN_SIZE_COS: f32 = 0.9995;
/// Directional-light intensity at full day. The per-frame `weather_system`
/// re-derives the live value from the TOD arc; this is the bootstrap seed.
const SUN_INTENSITY: f32 = 4.0;
/// Tangent-plane half-radius of the directional disk in radians (~1.15°);
/// drives PCSS-lite shadow jitter (#1023). Pre-EXAL hardcoded constant.
const SUN_ANGULAR_RADIUS: f32 = 0.020;

/// Pre-resolved cloud + sun-sprite bindless handles for [`translate_sky`].
/// The caller resolves these through the texture registry (the only
/// `VulkanContext`-coupled step); the translate stays pure.
pub(crate) struct SkyTextures {
    /// `(bindless handle, tile_scale)` per WTHR cloud layer 0..=3.
    /// `(0, 0.0)` = layer disabled (the shader branch-skips it).
    pub(crate) cloud_layers: [(u32, f32); 4],
    /// CLMT FNAM sun-sprite handle. `0` = composite shader's procedural disc.
    pub(crate) sun_sprite: u32,
}

/// WTHR → exterior [`CellLightingRes`] (the day-TOD-slot snapshot the
/// per-frame `weather_system` then animates through the stored NAM0 table).
/// Raw monitor-space colours (commit 0e8efc6) — no sRGB decode.
pub(crate) fn translate_exterior_cell_lighting(
    wthr: &WeatherRecord,
    sun_dir: [f32; 3],
) -> CellLightingRes {
    use byroredux_plugin::esm::records::weather::{SKY_AMBIENT, SKY_FOG, SKY_SUNLIGHT, TOD_DAY};
    CellLightingRes {
        ambient: wthr.sky_colors[SKY_AMBIENT][TOD_DAY].to_rgb_f32(),
        directional_color: wthr.sky_colors[SKY_SUNLIGHT][TOD_DAY].to_rgb_f32(),
        directional_dir: sun_dir,
        is_interior: false,
        fog_color: wthr.sky_colors[SKY_FOG][TOD_DAY].to_rgb_f32(),
        fog_near: wthr.fog_day_near,
        fog_far: wthr.fog_day_far,
        // WTHR-driven exterior lighting; the extended XCLL tail applies to
        // interior cells (and not-yet-wired exterior lighting overrides). #861.
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

/// WTHR + pre-resolved [`SkyTextures`] → [`SkyParamsRes`]. `current_dalc_cube`
/// is seeded `None`; `weather_system` populates it per-frame from
/// [`WeatherDataRes::skyrim_dalc_per_tod`] (#993).
pub(crate) fn translate_sky(
    wthr: &WeatherRecord,
    sun_dir: [f32; 3],
    textures: SkyTextures,
) -> SkyParamsRes {
    use byroredux_plugin::esm::records::weather::{
        SKY_HORIZON, SKY_LOWER, SKY_SUN, SKY_UPPER, TOD_DAY,
    };
    let [(c0, s0), (c1, s1), (c2, s2), (c3, s3)] = textures.cloud_layers;
    SkyParamsRes {
        zenith_color: wthr.sky_colors[SKY_UPPER][TOD_DAY].to_rgb_f32(),
        horizon_color: wthr.sky_colors[SKY_HORIZON][TOD_DAY].to_rgb_f32(),
        // #541 — real Sky-Lower (NAM0 slot 7) drives composite.frag's
        // below-horizon branch instead of the pre-fix `horizon * 0.3` fake.
        lower_color: wthr.sky_colors[SKY_LOWER][TOD_DAY].to_rgb_f32(),
        sun_direction: sun_dir,
        sun_color: wthr.sky_colors[SKY_SUN][TOD_DAY].to_rgb_f32(),
        sun_size: SUN_SIZE_COS,
        sun_intensity: SUN_INTENSITY,
        sun_angular_radius: SUN_ANGULAR_RADIUS,
        is_exterior: true,
        cloud_tile_scale: s0,
        cloud_texture_index: c0,
        sun_texture_index: textures.sun_sprite,
        cloud_tile_scale_1: s1,
        cloud_texture_index_1: c1,
        cloud_tile_scale_2: s2,
        cloud_texture_index_2: c2,
        cloud_tile_scale_3: s3,
        cloud_texture_index_3: c3,
        current_dalc_cube: None,
    }
}

/// WTHR (+ climate for TOD breakpoints) → [`WeatherDataRes`], the full NAM0
/// table the per-frame interpolator walks. `skyrim_dalc_per_tod` is `Some`
/// only for Skyrim WTHR (converted Z-up → Y-up once here); `None` elsewhere.
pub(crate) fn translate_weather(
    wthr: &WeatherRecord,
    climate: Option<&ClimateRecord>,
) -> WeatherDataRes {
    use byroredux_plugin::esm::records::weather::{SKY_COLOR_GROUPS, SKY_TIME_SLOTS};
    let mut sky_colors = [[[0.0f32; 3]; SKY_TIME_SLOTS]; SKY_COLOR_GROUPS];
    for (dst_group, src_group) in sky_colors.iter_mut().zip(wthr.sky_colors.iter()) {
        for (dst, src) in dst_group.iter_mut().zip(src_group.iter()) {
            *dst = src.to_rgb_f32();
        }
    }
    let skyrim_dalc_per_tod = wthr.skyrim_ambient_cube.as_ref().map(|cubes| {
        [
            DalcCubeYup::from_skyrim_zup(&cubes[0]),
            DalcCubeYup::from_skyrim_zup(&cubes[1]),
            DalcCubeYup::from_skyrim_zup(&cubes[2]),
            DalcCubeYup::from_skyrim_zup(&cubes[3]),
        ]
    });
    WeatherDataRes {
        sky_colors,
        fog: [
            wthr.fog_day_near,
            wthr.fog_day_far,
            wthr.fog_night_near,
            wthr.fog_night_far,
        ],
        // #463 — per-climate sunrise/sunset breakpoints (validated helper).
        tod_hours: crate::scene::climate_tod_hours(climate),
        skyrim_dalc_per_tod,
        // #1033 — WTHR DATA wind_speed drives per-weather cloud-scroll rate.
        wind_speed: wthr.wind_speed,
    }
}

// ── Procedural fallback (no resolved climate / weather) ──
//
// Warm Mojave-style desert sky. Same values the bulk loader used pre-#M40;
// kept here as the canonical no-data default rather than an inline block in
// the render-setup path (EXAL §3: the fallback is an explicit canonical
// constructor, not a render-time heuristic).
const FB_AMBIENT: [f32; 3] = [0.15, 0.14, 0.12];
const FB_SUNLIGHT: [f32; 3] = [1.0, 0.95, 0.8];
const FB_FOG_COLOR: [f32; 3] = [0.65, 0.7, 0.8];
const FB_ZENITH: [f32; 3] = [0.15, 0.3, 0.65];
const FB_HORIZON: [f32; 3] = [0.55, 0.5, 0.42];
// Pre-#541 the `compute_sky` below-horizon branch faked the ground tint as
// `horizon * 0.3`; matching that keeps the procedural look unchanged.
const FB_LOWER: [f32; 3] = [
    FB_HORIZON[0] * 0.3,
    FB_HORIZON[1] * 0.3,
    FB_HORIZON[2] * 0.3,
];
const FB_SUN_COLOR: [f32; 3] = [1.0, 0.95, 0.8];
const FB_FOG_NEAR: f32 = 15000.0;
const FB_FOG_FAR: f32 = 80000.0;

/// Procedural-fallback exterior lighting (no plugin data → engine defaults).
pub(crate) fn procedural_fallback_cell_lighting(sun_dir: [f32; 3]) -> CellLightingRes {
    CellLightingRes {
        ambient: FB_AMBIENT,
        directional_color: FB_SUNLIGHT,
        directional_dir: sun_dir,
        is_interior: false,
        fog_color: FB_FOG_COLOR,
        fog_near: FB_FOG_NEAR,
        fog_far: FB_FOG_FAR,
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

/// Procedural-fallback sky (no clouds, procedural sun disc).
pub(crate) fn procedural_fallback_sky(sun_dir: [f32; 3]) -> SkyParamsRes {
    SkyParamsRes {
        zenith_color: FB_ZENITH,
        horizon_color: FB_HORIZON,
        lower_color: FB_LOWER,
        sun_direction: sun_dir,
        sun_color: FB_SUN_COLOR,
        sun_size: SUN_SIZE_COS,
        sun_intensity: SUN_INTENSITY,
        sun_angular_radius: SUN_ANGULAR_RADIUS,
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

/// Synthetic [`WeatherDataRes`] for the fallback: every TOD slot of the six
/// groups `weather_system` reads carries the same procedural colour, so the
/// TOD lerp re-writes the same values each frame while still advancing
/// `sun_direction` / `sun_intensity`. See #542 / M33-10.
pub(crate) fn procedural_fallback_weather() -> WeatherDataRes {
    use byroredux_plugin::esm::records::weather::{
        SKY_AMBIENT, SKY_COLOR_GROUPS, SKY_FOG, SKY_HORIZON, SKY_LOWER, SKY_SUN, SKY_SUNLIGHT,
        SKY_TIME_SLOTS, SKY_UPPER,
    };
    let mut sky_colors = [[[0.0f32; 3]; SKY_TIME_SLOTS]; SKY_COLOR_GROUPS];
    let synthetic = [
        (SKY_UPPER, FB_ZENITH),
        (SKY_FOG, FB_FOG_COLOR),
        (SKY_AMBIENT, FB_AMBIENT),
        (SKY_SUNLIGHT, FB_SUNLIGHT),
        (SKY_SUN, FB_SUN_COLOR),
        // #541 — `weather_system` also reads SKY_LOWER for the below-horizon
        // branch; synthetic value matches the procedural `FB_LOWER`.
        (SKY_LOWER, FB_LOWER),
        (SKY_HORIZON, FB_HORIZON),
    ];
    for (group, color) in synthetic {
        sky_colors[group].fill(color);
    }
    WeatherDataRes {
        sky_colors,
        // Day/night fog distances kept identical — no authored night distance.
        fog: [FB_FOG_NEAR, FB_FOG_FAR, FB_FOG_NEAR, FB_FOG_FAR],
        // Pre-#463 hardcoded TOD breakpoints.
        tod_hours: [6.0, 10.0, 18.0, 22.0],
        skyrim_dalc_per_tod: None,
        wind_speed: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::misc::{WaterParams, WatrRecord};

    // ── default_water_for_worldspace ──────────────────────────────

    /// #1305 / OBL-D6-NEW-02 — an Oblivion worldspace with a NAM2 default
    /// water form makes its no-XCLW cells default to water at the Tamriel
    /// sea level Z=0 (Oblivion WRLD has no DNAM height field, so the
    /// constant is load-bearing). Pins both the gate (only when water_form
    /// present) and the user-confirmed 0.0 height.
    #[test]
    fn oblivion_worldspace_with_water_form_defaults_to_sea_level() {
        let wrld = WorldspaceRecord {
            water_form: Some(0x0000_1234),
            ..Default::default()
        };
        assert_eq!(
            default_water_for_worldspace(Some(&wrld), GameKind::Oblivion),
            (Some(0.0), Some(0x0000_1234)),
            "Oblivion worldspace advertising default water → no-XCLW cells get Z=0 water"
        );
    }

    #[test]
    fn worldspace_without_water_form_has_no_default_water() {
        let wrld = WorldspaceRecord {
            water_form: None,
            ..Default::default()
        };
        assert_eq!(
            default_water_for_worldspace(Some(&wrld), GameKind::Oblivion),
            (None, None)
        );
        // A missing worldspace record likewise yields no default water.
        assert_eq!(
            default_water_for_worldspace(None, GameKind::Oblivion),
            (None, None)
        );
    }

    /// Non-Oblivion games must NOT be forced to Z=0: a NAM2 water_form
    /// alone (no DNAM default height parsed) yields no default water, so
    /// the loader never invents sea level for FO3/FNV/Skyrim+ where the
    /// real default lives in DNAM (e.g. WastelandNV -2300, Skyrim Tamriel
    /// -14000). Pins that the Oblivion Z=0 path does not leak to them.
    #[test]
    fn non_oblivion_without_dnam_gets_no_default() {
        let wrld = WorldspaceRecord {
            water_form: Some(0x0000_1234),
            default_water_height: None,
            ..Default::default()
        };
        for game in [
            GameKind::Fallout3NV,
            GameKind::Skyrim,
            GameKind::Fallout4,
            GameKind::Fallout76,
            GameKind::Starfield,
        ] {
            assert_eq!(
                default_water_for_worldspace(Some(&wrld), game),
                (None, None),
                "{game:?} with no DNAM default height must NOT be forced to Z=0"
            );
        }
    }

    /// Non-Oblivion games use the WRLD DNAM default water height (second
    /// f32 of "Land Data"), paired with the NAM2 type form — NOT Z=0.
    /// Pins the #1305 follow-up (FO3/FNV/Skyrim+ default-water inheritance).
    #[test]
    fn non_oblivion_uses_dnam_default_water_height() {
        let wrld = WorldspaceRecord {
            water_form: Some(0x0000_00AB),
            default_water_height: Some(-2300.0), // e.g. WastelandNV
            ..Default::default()
        };
        for game in [GameKind::Fallout3NV, GameKind::Skyrim, GameKind::Fallout4] {
            assert_eq!(
                default_water_for_worldspace(Some(&wrld), game),
                (Some(-2300.0), Some(0x0000_00AB)),
                "{game:?} no-XCLW cells inherit the DNAM default water height + NAM2 type"
            );
        }
        // Oblivion ignores DNAM (it has none) and stays on Z=0.
        assert_eq!(
            default_water_for_worldspace(Some(&wrld), GameKind::Oblivion),
            (Some(0.0), Some(0x0000_00AB))
        );
    }

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
            texture_path: String::new(),
            noise_textures: [u32::MAX; 3],
            params: WaterParams {
                shallow_color: [1.0, 0.4, 0.1],
                deep_color: [0.6, 0.1, 0.0],
                reflection_color: lava_tint,
                fog_near: 20.0,
                fog_far: 80.0,
                reflectivity: 0.40,
                fresnel: 0.04,
                wind_speed: 0.0,
                wind_direction: 0.0,
                wave_amplitude: 0.0,
                wave_frequency: 0.0,
            },
            raw_dnam: Vec::new(),
            raw_data: Vec::new(),
        };

        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);

        let (mat, _kind, _flow, _normal) = resolve_water_material(&waters, Some(0x000A_BCDE));

        assert_eq!(
            mat.reflection_tint, lava_tint,
            "reflection_tint must round-trip from WATR DATA reflection_color"
        );
    }

    /// Default WaterMaterial (no XCWT / no WATR record) uses the neutral
    /// grey that matches the pre-#1069 hard-coded shader value.
    #[test]
    fn default_water_material_has_neutral_reflection_tint() {
        let (mat, _, _, _) = resolve_water_material(&HashMap::new(), None);
        assert_eq!(
            mat.reflection_tint,
            [0.65, 0.70, 0.75],
            "default reflection_tint must match the pre-fix shader hard-code"
        );
    }

    // ── exterior sky / sun / weather translation (EXAL step 3) ────

    use byroredux_plugin::esm::records::weather::{
        SkyColor, SKY_AMBIENT, SKY_FOG, SKY_HORIZON, SKY_LOWER, SKY_SUN, SKY_SUNLIGHT, SKY_UPPER,
        TOD_DAY,
    };

    /// Build a WTHR with a distinct colour in one (group, day-slot) cell.
    fn wthr_with(group: usize, c: SkyColor) -> WeatherRecord {
        let mut w = WeatherRecord::default();
        w.sky_colors[group][TOD_DAY] = c;
        w
    }

    #[test]
    fn translate_cell_lighting_reads_day_slot_and_marks_exterior() {
        let mut w = WeatherRecord::default();
        w.sky_colors[SKY_AMBIENT][TOD_DAY] = SkyColor {
            r: 51,
            g: 0,
            b: 0,
            a: 255,
        };
        w.sky_colors[SKY_SUNLIGHT][TOD_DAY] = SkyColor {
            r: 0,
            g: 102,
            b: 0,
            a: 255,
        };
        w.sky_colors[SKY_FOG][TOD_DAY] = SkyColor {
            r: 0,
            g: 0,
            b: 204,
            a: 255,
        };
        w.fog_day_near = 1500.0;
        w.fog_day_far = 9000.0;

        let sun_dir = [0.1, 0.9, 0.2];
        let cl = translate_exterior_cell_lighting(&w, sun_dir);
        assert_eq!(cl.ambient, [51.0 / 255.0, 0.0, 0.0]);
        assert_eq!(cl.directional_color, [0.0, 102.0 / 255.0, 0.0]);
        assert_eq!(cl.fog_color, [0.0, 0.0, 204.0 / 255.0]);
        assert_eq!(cl.directional_dir, sun_dir);
        assert_eq!((cl.fog_near, cl.fog_far), (1500.0, 9000.0));
        assert!(!cl.is_interior);
    }

    #[test]
    fn translate_sky_routes_each_cloud_layer_and_sun_sprite() {
        // Distinct day-slot colours so the slot→field mapping is pinned.
        let mut w = wthr_with(
            SKY_UPPER,
            SkyColor {
                r: 10,
                g: 0,
                b: 0,
                a: 255,
            },
        );
        w.sky_colors[SKY_HORIZON][TOD_DAY] = SkyColor {
            r: 0,
            g: 20,
            b: 0,
            a: 255,
        };
        w.sky_colors[SKY_LOWER][TOD_DAY] = SkyColor {
            r: 0,
            g: 0,
            b: 30,
            a: 255,
        };
        w.sky_colors[SKY_SUN][TOD_DAY] = SkyColor {
            r: 40,
            g: 40,
            b: 0,
            a: 255,
        };

        let textures = SkyTextures {
            cloud_layers: [(11, 0.11), (22, 0.22), (33, 0.33), (44, 0.44)],
            sun_sprite: 99,
        };
        let sun_dir = [0.0, 1.0, 0.0];
        let sky = translate_sky(&w, sun_dir, textures);

        // Colour slot routing.
        assert_eq!(sky.zenith_color, [10.0 / 255.0, 0.0, 0.0]);
        assert_eq!(sky.horizon_color, [0.0, 20.0 / 255.0, 0.0]);
        assert_eq!(sky.lower_color, [0.0, 0.0, 30.0 / 255.0]);
        assert_eq!(sky.sun_color, [40.0 / 255.0, 40.0 / 255.0, 0.0]);
        // Cloud-layer handle/scale routing — each of the 4 lands in its slot.
        assert_eq!((sky.cloud_texture_index, sky.cloud_tile_scale), (11, 0.11));
        assert_eq!(
            (sky.cloud_texture_index_1, sky.cloud_tile_scale_1),
            (22, 0.22)
        );
        assert_eq!(
            (sky.cloud_texture_index_2, sky.cloud_tile_scale_2),
            (33, 0.33)
        );
        assert_eq!(
            (sky.cloud_texture_index_3, sky.cloud_tile_scale_3),
            (44, 0.44)
        );
        assert_eq!(sky.sun_texture_index, 99);
        // Canonical seeds.
        assert_eq!(sky.sun_direction, sun_dir);
        assert_eq!(sky.sun_size, 0.9995);
        assert_eq!(sky.sun_intensity, 4.0);
        assert_eq!(sky.sun_angular_radius, 0.020);
        assert!(sky.is_exterior);
        // DALC is populated per-frame by weather_system, not at translate.
        assert!(sky.current_dalc_cube.is_none());
    }

    #[test]
    fn translate_weather_copies_fog_wind_and_falls_back_tod_without_climate() {
        let mut w = WeatherRecord::default();
        w.fog_day_near = 100.0;
        w.fog_day_far = 200.0;
        w.fog_night_near = 300.0;
        w.fog_night_far = 400.0;
        w.wind_speed = 7;
        w.sky_colors[SKY_UPPER][TOD_DAY] = SkyColor {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        };

        let wd = translate_weather(&w, None);
        assert_eq!(wd.fog, [100.0, 200.0, 300.0, 400.0]);
        assert_eq!(wd.wind_speed, 7);
        // No climate → the validated `climate_tod_hours` fallback.
        assert_eq!(wd.tod_hours, [6.0, 10.0, 18.0, 22.0]);
        // FNV/FO3/Oblivion WTHR (no DALC sub-records) → None.
        assert!(wd.skyrim_dalc_per_tod.is_none());
        // The NAM0 table round-trips to f32.
        assert_eq!(wd.sky_colors[SKY_UPPER][TOD_DAY], [1.0, 0.0, 0.0]);
    }

    #[test]
    fn procedural_fallback_pins_mojave_defaults() {
        let sun_dir = [-0.4, 0.8, -0.45];
        let cl = procedural_fallback_cell_lighting(sun_dir);
        assert_eq!(cl.ambient, [0.15, 0.14, 0.12]);
        assert_eq!(cl.directional_dir, sun_dir);
        assert_eq!((cl.fog_near, cl.fog_far), (15000.0, 80000.0));
        assert!(!cl.is_interior);

        let sky = procedural_fallback_sky(sun_dir);
        assert_eq!(sky.zenith_color, [0.15, 0.3, 0.65]);
        // Below-horizon ground tint matches the pre-#541 `horizon * 0.3`.
        assert_eq!(sky.lower_color, [0.55 * 0.3, 0.5 * 0.3, 0.42 * 0.3]);
        assert_eq!(sky.cloud_tile_scale, 0.0); // no clouds in the fallback
        assert_eq!(sky.sun_texture_index, 0); // procedural disc

        let wd = procedural_fallback_weather();
        assert_eq!(wd.tod_hours, [6.0, 10.0, 18.0, 22.0]);
        assert_eq!(wd.wind_speed, 0);
        assert!(wd.skyrim_dalc_per_tod.is_none());
        // Synthetic table: the day slot of the read groups carries the
        // procedural colour, and the lerp endpoints are equal.
        assert_eq!(wd.sky_colors[SKY_AMBIENT][TOD_DAY], [0.15, 0.14, 0.12]);
        assert_eq!(
            wd.sky_colors[SKY_HORIZON][0],
            wd.sky_colors[SKY_HORIZON][TOD_DAY]
        );
    }
}
