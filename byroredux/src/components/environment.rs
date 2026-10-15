//! Cell lighting, weather, sky and cloud resources. (#5467 split from `components.rs`)

use byroredux_core::ecs::Resource;
// #5173 — the per-lane ground-cover arrays below size from the same splat
// constant the terrain tile + shader loops size from, so a lane bump cannot
// leave this component silently 8-wide while the rest of the chain moves.
use std::collections::HashMap;

/// Cell lighting from the ESM (ambient + directional + fog).
///
/// The base block (`ambient`/`directional_*`/`fog_*`) is populated for
/// every cell — interior XCLL on `load_cell_with_masters`, exterior
/// WTHR on `apply_worldspace_weather`, or engine-default constants
/// when no plugin data is available.
///
/// The optional Skyrim/FNV extended block (`directional_fade` ..
/// `fresnel_power`) is populated only when the source cell carries an
/// XCLL with the matching tail length. Pre-#861 every consumer dropped
/// these on the floor at the renderer-facing boundary even though the
/// plugin parser had been extracting them since #379 (FNV) / #367
/// (Skyrim). Now flowed through end-to-end on the CPU side; shader
/// consumption follows in #865 (fog curve) and a future Skyrim
/// ambient-cube uniform. Per-field `#[allow(dead_code)]` is the
/// staged-rollout marker — removed in lockstep with each shader-side
/// consumer landing, so an unused-after-shader-lands field surfaces
/// as a warning instead of silently growing dead.
pub(crate) struct CellLightingRes {
    pub(crate) ambient: [f32; 3],
    pub(crate) directional_color: [f32; 3],
    /// Direction vector in Y-up space (computed from rotation).
    pub(crate) directional_dir: [f32; 3],
    /// True when the cell is interior. Interior XCLL directional is a
    /// subtle tint, not a physical sun — we skip it as a scene light to
    /// avoid leak artifacts on walls that shouldn't see the sky.
    pub(crate) is_interior: bool,
    /// Fog color (RGB 0-1).
    pub(crate) fog_color: [f32; 3],
    /// Fog near distance (game units).
    pub(crate) fog_near: f32,
    /// Fog far distance (game units).
    pub(crate) fog_far: f32,
    /// Engine-native participating medium fitted once from the authored
    /// XCLL ramp. Volumetric rendering consumes this instead of evaluating
    /// `fog_near..fog_far` at runtime.
    pub(crate) fog_medium: crate::fog::FogMedium,
    // ── Extended XCLL fields (FNV 40-byte tail + Skyrim 92-byte tail) ──
    // All of these are now read by the `cell.lighting` console command
    // and/or the lighting plumbing (see #865 for the fog curve), so the
    // former per-field `#[allow(dead_code)]` guards have been removed.
    /// Directional light fade multiplier — bytes 28-31 of the XCLL.
    /// FNV+ XCLL.
    pub(crate) directional_fade: Option<f32>,
    /// Cubic-fog clip distance — bytes 32-35. FNV+ XCLL. Retained with
    /// `fog_power` for diagnostics and explicit legacy compatibility;
    /// the physical volumetric path consumes `fog_medium`.
    pub(crate) fog_clip: Option<f32>,
    /// Cubic-fog falloff exponent — bytes 36-39. FNV+ XCLL.
    pub(crate) fog_power: Option<f32>,
    /// Fog far color (RGB 0-1) — bytes 72-74. Skyrim+ XCLL. Distinct
    /// from `fog_color` (which is the near-distance fog tint).
    pub(crate) fog_far_color: Option<[f32; 3]>,
    /// Maximum fog opacity — bytes 76-79. Skyrim+ XCLL.
    pub(crate) fog_max: Option<f32>,
    /// Light fade begin distance — bytes 80-83. Skyrim+ XCLL.
    pub(crate) light_fade_begin: Option<f32>,
    /// Light fade end distance — bytes 84-87. Skyrim+ XCLL.
    pub(crate) light_fade_end: Option<f32>,
    /// Directional ambient cube — `[+X, -X, +Y, -Y, +Z, -Z]` RGB
    /// triplets from bytes 40-63. Drives the per-cell ambient probe;
    /// ±Z asymmetry is what makes Skyrim cave floors read warm while
    /// ceilings read cool without a dedicated IBL pass. Skyrim+ XCLL.
    pub(crate) directional_ambient: Option<[[f32; 3]; 6]>,
    /// Specular tint (RGB) — bytes 64-66. Skyrim+ XCLL.
    pub(crate) specular_color: Option<[f32; 3]>,
    /// Specular alpha — byte 67. Stored separately so consumers can
    /// decide whether the RGBA packing was intentional or padding.
    pub(crate) specular_alpha: Option<f32>,
    /// Fresnel power exponent — bytes 68-71. Skyrim+ XCLL.
    pub(crate) fresnel_power: Option<f32>,
    /// Raw Skyrim+ XCLL per-field LTMP inheritance mask. Values in this
    /// resource are already resolved; the mask is retained as provenance
    /// for `light.dump` and fixture diagnostics.
    pub(crate) inheritance_flags: Option<u32>,
}

impl CellLightingRes {
    /// #5814 — the seven-field "no extended XCLL tail" build: every
    /// optional tail field `None`, the participating medium fitted once
    /// from the near/far ramp. Five production sites and eleven test
    /// fixtures spelled all 25 fields by hand (each ending in the same
    /// ten `None`s); a forgotten field compiles and silently means
    /// `None`. Sites with authored tail values use this as the
    /// `..bare(..)` base and override.
    pub(crate) fn bare(
        ambient: [f32; 3],
        directional_color: [f32; 3],
        directional_dir: [f32; 3],
        is_interior: bool,
        fog_color: [f32; 3],
        fog_near: f32,
        fog_far: f32,
    ) -> Self {
        Self {
            ambient,
            directional_color,
            directional_dir,
            is_interior,
            fog_color,
            fog_near,
            fog_far,
            fog_medium: crate::fog::FogMedium::from_legacy_ramp(fog_near, fog_far, None),
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

    /// Construct a `CellLightingRes` from a fully-resolved
    /// [`byroredux_plugin::esm::cell::CellLighting`] (the plugin
    /// layer's parser output) plus the renderer-facing direction
    /// vector and the interior/exterior flag. Carries the 9 extended
    /// XCLL fields through verbatim — see #861. Producers in
    /// `scene.rs` use this helper for the interior `--esm --cell`
    /// arm; exterior weather (`apply_worldspace_weather`) and the
    /// engine-default constant path build the struct directly with
    /// `extended_*` set to `None`.
    pub(crate) fn from_cell_lighting(
        lit: &byroredux_plugin::esm::cell::CellLighting,
        directional_dir: [f32; 3],
        is_interior: bool,
    ) -> Self {
        Self {
            ambient: lit.ambient,
            directional_color: lit.directional_color,
            directional_dir,
            is_interior,
            fog_color: lit.fog_color,
            fog_near: lit.fog_near,
            fog_far: lit.fog_far,
            fog_medium: crate::fog::FogMedium::from_legacy_ramp(
                lit.fog_near,
                lit.fog_far,
                lit.fog_max,
            ),
            directional_fade: lit.directional_fade,
            fog_clip: lit.fog_clip,
            fog_power: lit.fog_power,
            fog_far_color: lit.fog_far_color,
            fog_max: lit.fog_max,
            light_fade_begin: lit.light_fade_begin,
            light_fade_end: lit.light_fade_end,
            directional_ambient: lit.directional_ambient,
            specular_color: lit.specular_color,
            specular_alpha: lit.specular_alpha,
            fresnel_power: lit.fresnel_power,
            inheritance_flags: lit.inheritance_flags,
        }
    }
}

impl Resource for CellLightingRes {}

/// The current interior CELL explicitly allows outdoor sky through open
/// geometry. Kept separate from XCLL lighting: sky visibility must not turn
/// the cell's authored ambient and directional values into exterior ones.
pub(crate) struct InteriorSkyExposureRes(pub(crate) bool);
impl Resource for InteriorSkyExposureRes {}

/// #4902 — the outdoor environment (`SkyParamsRes`, `WeatherDataRes`, …) was
/// installed as the canonical procedural default by an interior-only boot,
/// not resolved from a worldspace. `weather_system` advances it like any
/// weather; the first real worldspace replaces it outright instead of
/// cross-fading from a sky the player never saw outdoors. Removed on that
/// replacement.
pub(crate) struct ProvisionalOutdoorEnvironment;
impl Resource for ProvisionalOutdoorEnvironment {}

/// Resolved ambient-audio directive for the currently resident cell's
/// highest-priority `REGN` `Sound` entry (EX-16 item 1, #2372). CPU-only —
/// carries FormIDs, not decoded audio; `asset_provider::audio`'s
/// `resolve_sound_path`/`sound_archive_path` resolve them to archive
/// paths, and a consumer — `dispatch_region_ambient_music`
/// (`byroredux/src/asset_provider/audio.rs`), shipped 2026-08-23 (`ede48ffb`/
/// `3ef05d1b`) — dispatches actual playback via `AudioWorld::play_music`.
/// Mirrors `CellLightingRes`'s shape
/// and lifecycle: computed once at cell-apply time from data already
/// parsed into `EsmIndex`, not recomputed per-frame.
///
/// Deliberately carries only `music`/`incidental` — both are single
/// authored FormIDs (`RDMD`/`RDMO`/`RDSB` and `RDSI`). The RDAT
/// `sounds: Vec<RegionSound>` ambient-loop list is NOT surfaced here:
/// each entry's `chance_raw` selection probability has an unresolved
/// fixed-point scale (see `RegionSound`'s doc in `misc/world.rs`), and
/// picking one without a verified scale would be exactly the kind of
/// guessed threshold this project's no-guessing policy exists to
/// prevent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RegionAmbientRes {
    /// `RDMD` (Oblivion) / `RDMO` (Skyrim) / `RDSB` (FNV) — the winning
    /// entry's raw background-music word. Not a `SOUN` FormID on any of the
    /// three eras: Oblivion's `RDMD` is a `uint32` music-category enum (not
    /// a FormID at all), Skyrim's `RDMO` is a FormID but targets `MUSC`,
    /// and FNV's `RDSB` targets `MSET` (Media Set) — see
    /// `RegionDataPayload::Sound`'s doc in
    /// `crates/plugin/src/esm/records/misc/world.rs` for the full
    /// per-era census (#3787 FNV, #3811 Oblivion + Skyrim — both closed
    /// doc-only; the runtime for all three target types is tracked by
    /// #3816, FNV's `MSET` included since #3915).
    /// `dispatch_region_ambient_music` (`asset_provider::audio`) currently
    /// resolves this against the `SounRecord` map regardless, which is why
    /// region ambient music is a structural no-op on every tested game via
    /// this path.
    pub music_form: Option<u32>,
    /// `RDSI` — FNV-only incidental FormID. Same caveat: census-confirmed
    /// `MSET`, not `SOUN` (#3787; runtime tracked by #3816).
    pub incidental_form: Option<u32>,
}

impl RegionAmbientRes {
    /// Resolve from a resident cell's `regions` (XCLR) FormID list against
    /// the parsed `REGN` map. Empty/no-match input yields `Self::default()`
    /// (both fields `None`) rather than an `Option<Self>` — an unambiguous
    /// "no ambient directive" state a consumer can read unconditionally,
    /// same posture as `CellLightingRes`'s always-present-resource design.
    pub(crate) fn resolve(
        region_form_ids: &[u32],
        regions: &HashMap<u32, byroredux_plugin::esm::records::RegnRecord>,
    ) -> Self {
        use byroredux_plugin::esm::records::{select_active_region_sound, RegionDataPayload};
        let Some(entry) = select_active_region_sound(region_form_ids, regions) else {
            return Self::default();
        };
        match &entry.payload {
            RegionDataPayload::Sound {
                music, incidental, ..
            } => Self {
                music_form: *music,
                incidental_form: *incidental,
            },
            // `select_active_region_sound` only ever returns a `Sound`-kind
            // entry, so this arm is unreachable in practice; kept instead
            // of `unreachable!()` so a future payload-shape change fails
            // soft (no ambient directive) rather than panicking mid-load.
            _ => Self::default(),
        }
    }
}

impl Resource for RegionAmbientRes {}

#[cfg(test)]
mod region_ambient_res_tests {
    use super::*;
    use byroredux_plugin::esm::records::{
        RegionDataEntry, RegionDataKind, RegionDataPayload, RegnRecord,
    };
    use std::collections::HashMap;

    fn regn(entries: Vec<RegionDataEntry>) -> RegnRecord {
        RegnRecord {
            entries,
            ..Default::default()
        }
    }

    fn sound_entry(priority: u8, music: Option<u32>, incidental: Option<u32>) -> RegionDataEntry {
        RegionDataEntry {
            kind: RegionDataKind::Sound,
            flags: 0,
            priority,
            payload: RegionDataPayload::Sound {
                music,
                incidental,
                sounds: Vec::new(),
            },
        }
    }

    #[test]
    fn resolves_music_and_incidental_from_the_winning_entry() {
        let mut regions = HashMap::new();
        regions.insert(
            0x10,
            regn(vec![sound_entry(50, Some(0xAAAA), Some(0xBBBB))]),
        );
        let res = RegionAmbientRes::resolve(&[0x10], &regions);
        assert_eq!(res.music_form, Some(0xAAAA));
        assert_eq!(res.incidental_form, Some(0xBBBB));
    }

    #[test]
    fn no_tagging_regions_yields_default() {
        let regions = HashMap::new();
        assert_eq!(
            RegionAmbientRes::resolve(&[], &regions),
            RegionAmbientRes::default()
        );
    }

    #[test]
    fn a_tagging_region_with_no_sound_entry_yields_default() {
        let mut regions = HashMap::new();
        regions.insert(
            0x10,
            regn(vec![RegionDataEntry {
                kind: RegionDataKind::Weather,
                flags: 0,
                priority: 100,
                payload: RegionDataPayload::Weather(Vec::new()),
            }]),
        );
        assert_eq!(
            RegionAmbientRes::resolve(&[0x10], &regions),
            RegionAmbientRes::default()
        );
    }
}

#[cfg(test)]
mod cell_lighting_res_tests {
    //! Regression for #861 — extended XCLL fields propagate through the
    //! `CellLighting → CellLightingRes` boundary (FNV 40-byte tail and
    //! Skyrim 92-byte tail). Pre-fix every Optional past the 3 base
    //! fog fields was dropped on the floor at the renderer-facing
    //! resource layer.
    use super::*;
    use byroredux_plugin::esm::cell::CellLighting;

    fn fnv_xcll_with_fog_curve() -> CellLighting {
        // Mirrors the FNV 40-byte fixture in
        // crates/plugin/src/esm/cell/tests.rs:1683-1740 — directional_fade,
        // fog_clip, fog_power populated; Skyrim-only fields stay None.
        CellLighting {
            ambient: [0.10, 0.10, 0.12],
            directional_color: [1.0, 0.95, 0.80],
            directional_azimuth: 0.0,
            directional_elevation: 0.0,
            fog_color: [0.50, 0.45, 0.30],
            fog_near: 100.0,
            fog_far: 8000.0,
            directional_fade: Some(0.80),
            fog_clip: Some(7500.0),
            fog_power: Some(2.0),
            fog_far_color: None,
            fog_max: None,
            light_fade_begin: None,
            light_fade_end: None,
            directional_ambient: None,
            specular_color: None,
            specular_alpha: None,
            fresnel_power: None,
            inheritance_flags: None,
            starfield: None,
        }
    }

    fn skyrim_xcll_with_full_extension() -> CellLighting {
        CellLighting {
            ambient: [0.05, 0.05, 0.06],
            directional_color: [0.8, 0.85, 1.0],
            directional_azimuth: 0.5,
            directional_elevation: 0.3,
            fog_color: [0.20, 0.25, 0.35],
            fog_near: 50.0,
            fog_far: 5000.0,
            directional_fade: Some(0.65),
            fog_clip: Some(4500.0),
            fog_power: Some(1.5),
            fog_far_color: Some([0.10, 0.10, 0.15]),
            fog_max: Some(0.85),
            light_fade_begin: Some(800.0),
            light_fade_end: Some(2400.0),
            directional_ambient: Some([
                [0.20, 0.18, 0.15], // +X
                [0.18, 0.16, 0.13], // -X
                [0.10, 0.10, 0.10], // +Y
                [0.05, 0.05, 0.05], // -Y
                [0.25, 0.25, 0.30], // +Z (warmer ceiling)
                [0.15, 0.13, 0.10], // -Z (cooler floor)
            ]),
            specular_color: Some([0.30, 0.30, 0.32]),
            specular_alpha: Some(1.0),
            fresnel_power: Some(2.5),
            inheritance_flags: Some(0),
            starfield: None,
        }
    }

    #[test]
    fn from_cell_lighting_propagates_fnv_fog_curve_fields() {
        let lit = fnv_xcll_with_fog_curve();
        let res = CellLightingRes::from_cell_lighting(&lit, [0.0, 1.0, 0.0], true);

        // Base block.
        assert_eq!(res.ambient, [0.10, 0.10, 0.12]);
        assert_eq!(res.directional_color, [1.0, 0.95, 0.80]);
        assert_eq!(res.directional_dir, [0.0, 1.0, 0.0]);
        assert!(res.is_interior);
        assert_eq!(res.fog_color, [0.50, 0.45, 0.30]);
        assert_eq!(res.fog_near, 100.0);
        assert_eq!(res.fog_far, 8000.0);

        // FNV 40-byte tail — must be Some(...) and round-trip exactly.
        assert_eq!(res.directional_fade, Some(0.80));
        assert_eq!(res.fog_clip, Some(7500.0));
        assert_eq!(res.fog_power, Some(2.0));

        // Skyrim 92-byte tail — must stay None on a 40-byte FNV XCLL.
        assert!(res.fog_far_color.is_none());
        assert!(res.fog_max.is_none());
        assert!(res.light_fade_begin.is_none());
        assert!(res.light_fade_end.is_none());
        assert!(res.directional_ambient.is_none());
        assert!(res.specular_color.is_none());
        assert!(res.specular_alpha.is_none());
        assert!(res.fresnel_power.is_none());
    }

    #[test]
    fn from_cell_lighting_propagates_skyrim_92byte_extension() {
        let lit = skyrim_xcll_with_full_extension();
        let res = CellLightingRes::from_cell_lighting(&lit, [0.1, 0.9, -0.3], true);

        // FNV 40-byte tail.
        assert_eq!(res.directional_fade, Some(0.65));
        assert_eq!(res.fog_clip, Some(4500.0));
        assert_eq!(res.fog_power, Some(1.5));

        // Skyrim 92-byte tail.
        assert_eq!(res.fog_far_color, Some([0.10, 0.10, 0.15]));
        assert_eq!(res.fog_max, Some(0.85));
        assert_eq!(res.light_fade_begin, Some(800.0));
        assert_eq!(res.light_fade_end, Some(2400.0));
        let cube = res.directional_ambient.expect("Skyrim XCLL ambient cube");
        assert_eq!(cube[4], [0.25, 0.25, 0.30], "+Z (ceiling) face");
        assert_eq!(cube[5], [0.15, 0.13, 0.10], "-Z (floor) face");
        assert_eq!(res.specular_color, Some([0.30, 0.30, 0.32]));
        assert_eq!(res.specular_alpha, Some(1.0));
        assert_eq!(res.fresnel_power, Some(2.5));
    }

    #[test]
    fn from_cell_lighting_handles_pre_skyrim_xcll_with_no_extension() {
        // Oblivion-shape XCLL (28-byte head, no tail at all) — every
        // optional must stay None even though the parser emits the
        // CellLighting struct with the same field set.
        let lit = CellLighting {
            ambient: [0.30, 0.30, 0.30],
            directional_color: [0.90, 0.90, 0.85],
            directional_azimuth: 0.0,
            directional_elevation: 0.0,
            fog_color: [0.40, 0.40, 0.50],
            fog_near: 200.0,
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
            inheritance_flags: None,
            starfield: None,
        };
        let res = CellLightingRes::from_cell_lighting(&lit, [0.0, 1.0, 0.0], true);
        assert!(res.directional_fade.is_none());
        assert!(res.fog_clip.is_none());
        assert!(res.fog_power.is_none());
        assert!(res.directional_ambient.is_none());
    }
}

#[cfg(test)]
mod dalc_cube_tests {
    //! Regression for #993 — Skyrim DALC 6-axis ambient cube
    //! conversion and per-component lerp. Pinned distinctive colours
    //! per axis so a future refactor that swaps two axes can't
    //! silently land.
    use super::*;
    use byroredux_plugin::esm::records::weather::{SkyColor, SkyrimAmbientCube};

    fn rgb(r: u8, g: u8, b: u8) -> SkyColor {
        SkyColor { r, g, b, a: 0 }
    }

    fn distinctive_cube() -> SkyrimAmbientCube {
        // Each axis flagged with a unique R channel so the Z-up→Y-up
        // remap can't silently scramble axes.
        SkyrimAmbientCube {
            pos_x: rgb(0x10, 0, 0), // east
            neg_x: rgb(0x20, 0, 0), // west
            pos_y: rgb(0x30, 0, 0), // bethesda north
            neg_y: rgb(0x40, 0, 0), // bethesda south
            pos_z: rgb(0x50, 0, 0), // sky (bethesda up)
            neg_z: rgb(0x60, 0, 0), // ground (bethesda down)
            specular: rgb(0x70, 0, 0),
            fresnel_power: 1.0,
        }
    }

    #[test]
    fn from_xcll_zup_maps_ordered_faces_to_engine_axes() {
        let faces = [
            [1.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [3.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
            [5.0, 0.0, 0.0],
            [6.0, 0.0, 0.0],
        ];
        let yup = DalcCubeYup::from_xcll_zup(&faces, [0.2, 0.4, 0.6], 1.5);

        assert_eq!(yup.pos_x, faces[0]);
        assert_eq!(yup.neg_x, faces[1]);
        assert_eq!(yup.neg_z, faces[2]);
        assert_eq!(yup.pos_z, faces[3]);
        assert_eq!(yup.pos_y, faces[4]);
        assert_eq!(yup.neg_y, faces[5]);
        assert_eq!(yup.specular, [0.2, 0.4, 0.6]);
        assert_eq!(yup.fresnel_power, 1.5);
    }

    #[test]
    fn from_skyrim_zup_maps_bethesda_up_to_engine_pos_y() {
        let yup = DalcCubeYup::from_skyrim_zup(&distinctive_cube());
        // Bethesda +Z (sky-fill) lands on engine +Y.
        let pos_y_r = (yup.pos_y[0] * 255.0).round() as u8;
        assert_eq!(pos_y_r, 0x50, "engine +Y must carry bethesda +Z sky-fill");
        // Bethesda -Z (ground-bounce) lands on engine -Y.
        let neg_y_r = (yup.neg_y[0] * 255.0).round() as u8;
        assert_eq!(neg_y_r, 0x60, "engine -Y must carry bethesda -Z ground");
    }

    #[test]
    fn from_skyrim_zup_swaps_lateral_north_south_to_engine_z() {
        let yup = DalcCubeYup::from_skyrim_zup(&distinctive_cube());
        // Bethesda +Y (north) → engine -Z per the project-wide
        // `(x, y, z) → (x, z, -y)` swap. Verified against
        // `crates/nif/src/import/coord.rs:18`.
        let neg_z_r = (yup.neg_z[0] * 255.0).round() as u8;
        let pos_z_r = (yup.pos_z[0] * 255.0).round() as u8;
        assert_eq!(neg_z_r, 0x30, "engine -Z must carry bethesda +Y north");
        assert_eq!(pos_z_r, 0x40, "engine +Z must carry bethesda -Y south");
    }

    #[test]
    fn from_skyrim_zup_preserves_lateral_x() {
        let yup = DalcCubeYup::from_skyrim_zup(&distinctive_cube());
        let pos_x_r = (yup.pos_x[0] * 255.0).round() as u8;
        let neg_x_r = (yup.neg_x[0] * 255.0).round() as u8;
        assert_eq!(pos_x_r, 0x10, "X axis is identical across the swap");
        assert_eq!(neg_x_r, 0x20);
    }

    #[test]
    fn from_skyrim_zup_carries_specular_and_fresnel() {
        let yup = DalcCubeYup::from_skyrim_zup(&distinctive_cube());
        let spec_r = (yup.specular[0] * 255.0).round() as u8;
        assert_eq!(spec_r, 0x70);
        assert!((yup.fresnel_power - 1.0).abs() < 1e-6);
    }

    #[test]
    fn lerp_at_half_returns_midpoint_per_axis() {
        let a = DalcCubeYup {
            pos_x: [0.0, 0.0, 0.0],
            neg_x: [1.0, 1.0, 1.0],
            pos_y: [0.0, 0.5, 1.0],
            neg_y: [0.0; 3],
            pos_z: [0.0; 3],
            neg_z: [0.0; 3],
            specular: [0.0, 0.0, 0.0],
            fresnel_power: 1.0,
        };
        let b = DalcCubeYup {
            pos_x: [1.0, 1.0, 1.0],
            neg_x: [0.0, 0.0, 0.0],
            pos_y: [1.0, 0.5, 0.0],
            neg_y: [1.0; 3],
            pos_z: [1.0; 3],
            neg_z: [1.0; 3],
            specular: [1.0, 1.0, 1.0],
            fresnel_power: 3.0,
        };
        let m = DalcCubeYup::lerp(&a, &b, 0.5);
        assert_eq!(m.pos_x, [0.5; 3]);
        assert_eq!(m.neg_x, [0.5; 3]);
        assert_eq!(m.pos_y, [0.5, 0.5, 0.5]);
        assert_eq!(m.neg_y, [0.5; 3]);
        assert_eq!(m.specular, [0.5; 3]);
        assert!((m.fresnel_power - 2.0).abs() < 1e-6);
    }

    #[test]
    fn lerp_endpoints_return_inputs_exactly() {
        let a = DalcCubeYup {
            pos_y: [0.1, 0.2, 0.3],
            fresnel_power: 1.5,
            ..Default::default()
        };
        let b = DalcCubeYup {
            pos_y: [0.7, 0.8, 0.9],
            fresnel_power: 2.5,
            ..Default::default()
        };
        let at_zero = DalcCubeYup::lerp(&a, &b, 0.0);
        assert_eq!(at_zero.pos_y, a.pos_y);
        assert!((at_zero.fresnel_power - a.fresnel_power).abs() < 1e-6);
        let at_one = DalcCubeYup::lerp(&a, &b, 1.0);
        assert_eq!(at_one.pos_y, b.pos_y);
        assert!((at_one.fresnel_power - b.fresnel_power).abs() < 1e-6);
    }
}

/// 6-axis directional ambient cube interpolated for the current TOD,
/// stored in **engine Y-up** coordinates so the renderer can sample
/// along a fragment normal without coordinate conversion.
///
/// Sourced from Skyrim WTHR `DALC` sub-records (4 entries per record,
/// one per TOD slot — sunrise / day / sunset / night). The original
/// authoring is Bethesda Z-up; the converter
/// [`DalcCubeYup::from_skyrim_zup`] applies the same `(x, y, z) →
/// (x, z, -y)` permutation as
/// [`byroredux_core::math::coord::zup_to_yup_pos`] (also wrapped by
/// `crates/nif/src/import/coord.rs:18` for every other importer) — just
/// spread across 6 named cube-face fields instead of one `[f32; 3]`, so
/// it isn't a `zup_to_yup_pos` *call site* and won't turn up by grepping
/// for its callers (#2062).
///
/// Engine sampling semantics:
/// - `pos_y` = sky-fill (was Bethesda +Z, the "up" axis)
/// - `neg_y` = ground-bounce / cavity-fill (was Bethesda -Z, "down")
/// - `pos_x` / `neg_x` = lateral east / west
/// - `pos_z` / `neg_z` = lateral south / north (Bethesda's ±Y after
///   the swap collapses to engine ∓Z)
///
/// Future GPU consumer pushes this through the per-frame UBO; the
/// shader samples with weights `max(N, 0)` / `max(-N, 0)`. See #993
/// for the renderer-side wiring.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct DalcCubeYup {
    pub(crate) pos_x: [f32; 3],
    pub(crate) neg_x: [f32; 3],
    /// Engine +Y (Bethesda +Z) — sky fill.
    pub(crate) pos_y: [f32; 3],
    /// Engine -Y (Bethesda -Z) — ground-bounce / cavity fill.
    pub(crate) neg_y: [f32; 3],
    pub(crate) pos_z: [f32; 3],
    pub(crate) neg_z: [f32; 3],
    /// DALC specular tint (raw monitor-space RGB).
    pub(crate) specular: [f32; 3],
    /// DALC fresnel power tail — vanilla Skyrim ships `1.0`.
    pub(crate) fresnel_power: f32,
}

impl DalcCubeYup {
    /// Convert the Skyrim XCLL directional-ambient face array from
    /// Bethesda Z-up into engine Y-up axes.
    ///
    /// XCLL stores faces in `[+X, -X, +Y, -Y, +Z, -Z]` order. This is
    /// the same axis permutation as [`Self::from_skyrim_zup`], but the
    /// source values have already been decoded to linear RGB floats.
    pub(crate) fn from_xcll_zup(
        faces: &[[f32; 3]; 6],
        specular: [f32; 3],
        fresnel_power: f32,
    ) -> Self {
        Self {
            pos_x: faces[0],
            neg_x: faces[1],
            // Z-up "up" → Y-up "up".
            pos_y: faces[4],
            // Z-up "down" → Y-up "down".
            neg_y: faces[5],
            // Z-up "north" (+Y) → Y-up "south" (-Z).
            neg_z: faces[2],
            // Z-up "south" (-Y) → Y-up "north" (+Z).
            pos_z: faces[3],
            specular,
            fresnel_power,
        }
    }

    /// Convert a Bethesda-Z-up `SkyrimAmbientCube` into engine Y-up
    /// axes. The byte→field mapping in the WTHR parser is literal
    /// (no axis swap), so we apply the swap here once per TOD slot,
    /// not per fragment. Same `(x, y, z) → (x, z, -y)` permutation as
    /// [`byroredux_core::math::coord::zup_to_yup_pos`] (see the struct
    /// doc above for why this can't just call it directly).
    ///
    /// Mapping:
    /// - Bethesda +Z (sky) → engine +Y
    /// - Bethesda -Z (ground) → engine -Y
    /// - Bethesda +Y (north) → engine -Z
    /// - Bethesda -Y (south) → engine +Z
    /// - Bethesda ±X unchanged
    pub(crate) fn from_skyrim_zup(
        cube: &byroredux_plugin::esm::records::weather::SkyrimAmbientCube,
    ) -> Self {
        Self {
            pos_x: cube.pos_x.to_rgb_f32(),
            neg_x: cube.neg_x.to_rgb_f32(),
            // Z-up "up" → Y-up "up".
            pos_y: cube.pos_z.to_rgb_f32(),
            // Z-up "down" → Y-up "down".
            neg_y: cube.neg_z.to_rgb_f32(),
            // Z-up "north" (+Y) → Y-up "south" (-Z), so engine `neg_z`
            // sees Bethesda's "north" fill.
            neg_z: cube.pos_y.to_rgb_f32(),
            // Z-up "south" (-Y) → Y-up "north" (+Z).
            pos_z: cube.neg_y.to_rgb_f32(),
            specular: cube.specular.to_rgb_f32(),
            fresnel_power: cube.fresnel_power,
        }
    }

    /// Per-component lerp between two cubes. Used by `weather_system`
    /// to interpolate between adjacent TOD slot pairs (sunrise→day,
    /// day→sunset, …).
    pub(crate) fn lerp(a: &Self, b: &Self, t: f32) -> Self {
        Self {
            pos_x: crate::helpers::lerp_f32x3(a.pos_x, b.pos_x, t),
            neg_x: crate::helpers::lerp_f32x3(a.neg_x, b.neg_x, t),
            pos_y: crate::helpers::lerp_f32x3(a.pos_y, b.pos_y, t),
            neg_y: crate::helpers::lerp_f32x3(a.neg_y, b.neg_y, t),
            pos_z: crate::helpers::lerp_f32x3(a.pos_z, b.pos_z, t),
            neg_z: crate::helpers::lerp_f32x3(a.neg_z, b.neg_z, t),
            specular: crate::helpers::lerp_f32x3(a.specular, b.specular, t),
            fresnel_power: a.fresnel_power + (b.fresnel_power - a.fresnel_power) * t,
        }
    }
}

/// Runtime weather controls that affect the sky and screen-space atmosphere.
/// Kept as one small copyable value so weather transitions can blend all
/// authored effects together and every SkyParamsRes literal has one stable
/// extension point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WeatherSkyState {
    /// Broad procedural cloud occupancy in `[0, 1]`, derived from the WTHR
    /// weather classification. Authored cloud textures remain detail masks;
    /// this value supplies the continuous cloud body between their sprites.
    pub(crate) cloud_coverage: f32,
    /// Current TOD-sampled cloud tint and alpha multiplier per rendered layer.
    /// `[r, g, b, alpha]`, in raw monitor-space authoring units.
    pub(crate) cloud_tints: [[f32; 4]; 4],
    /// Rain and snow screen-space precipitation intensity.
    pub(crate) precipitation: [f32; 2],
    /// Thunder/lightning frequency, normalized from WTHR DATA.
    pub(crate) thunder_frequency: f32,
    /// RGB lightning flash colour.
    pub(crate) lightning_color: [f32; 3],
    /// Current TOD-sampled star colour from WTHR SKY_STARS.
    pub(crate) stars_color: [f32; 3],
    /// Current TOD-sampled, dimmer-scaled WTHR sunlight colour (`SKY_SUNLIGHT`),
    /// cross-faded with the weather transition. `weather_system` writes the
    /// same value to an exterior's `CellLightingRes::directional_color`, but
    /// that resource belongs to the active interior once one is loaded, so
    /// it cannot supply the exterior sun to the interior portal sky
    /// (#4839). This is the exterior sun colour that survives the transition.
    pub(crate) sunlight_color: [f32; 3],
    /// Sun glare multiplier from WTHR DATA.
    pub(crate) sun_glare: f32,
    /// Moon glare multiplier from Skyrim NAM3 / Moon Glare colour data.
    pub(crate) moon_glare: f32,
    /// Skyrim aurora enable/intensity derived from WTHR classification.
    pub(crate) aurora_intensity: f32,
    pub(crate) aurora_follows_sun: bool,
    /// Authored horizontal wind direction in renderer X/Z coordinates.
    pub(crate) wind_direction: [f32; 2],
    /// True when the source weather actually authored a directional-wind
    /// value. Legacy records retain the deterministic worldspace fallback.
    pub(crate) wind_direction_authored: bool,
    /// Normalized WTHR wind speed for precipitation drift.
    pub(crate) wind_speed: f32,
}

impl Default for WeatherSkyState {
    /// Engine-chosen neutral weather state for "exterior, no authored WTHR"
    /// (the procedural-fallback path) and the pre-first-frame seed. The
    /// constants that also serve the *authored* translate path alias their
    /// one declaration in `env_translate` — the `FB_TOD_HOURS`/
    /// `weather::DEFAULT_TOD_HOURS` arrangement (#2812) — so the duplicated
    /// values cannot drift (#4494); the rest are documented engine choices,
    /// not translated data.
    fn default() -> Self {
        Self {
            // Engine default occupancy for the no-record state. It is NOT
            // one of `env_translate::fog_coverage_from_weather`'s
            // classification-driven values (skyal.md §2 provenance note) —
            // those apply only once a WTHR is resolved.
            cloud_coverage: 0.35,
            cloud_tints: [[1.0, 1.0, 1.0, 1.0]; 4],
            precipitation: [0.0; 2],
            thunder_frequency: 0.0,
            lightning_color: [1.0; 3],
            stars_color: crate::env_translate::FB_STARS_COLOR,
            // The procedural fallback's sunlight, aliased from the one
            // declaration `procedural_fallback_cell_lighting` also uses.
            sunlight_color: crate::env_translate::FB_SUNLIGHT,
            // Aliased from the fallback constants `weather_sky_state`
            // substitutes for unauthored glare data — one declaration each.
            sun_glare: crate::env_translate::NEUTRAL_SUN_GLARE,
            moon_glare: crate::env_translate::FALLBACK_MOON_GLARE,
            aurora_intensity: 0.0,
            aurora_follows_sun: false,
            wind_direction: [1.0, 0.0],
            wind_direction_authored: false,
            wind_speed: 0.0,
        }
    }
}

/// Sky rendering parameters from WTHR records (exterior cells).
/// Stored as an ECS resource so the render loop can read it per-frame.
pub(crate) struct SkyParamsRes {
    pub(crate) zenith_color: [f32; 3],
    pub(crate) horizon_color: [f32; 3],
    /// Below-horizon ground / lower-hemisphere tint from WTHR's
    /// `SKY_LOWER` group (real Sky-Lower at NAM0 slot 7 per nif.xml,
    /// post-#729). Per-frame `weather_system` interpolates the
    /// authored TOD slots; the renderer's `compute_sky` branches on
    /// negative elevation and uses this colour instead of the pre-#541
    /// `horizon * 0.3` fake.
    pub(crate) lower_color: [f32; 3],
    pub(crate) sun_direction: [f32; 3],
    pub(crate) sun_color: [f32; 3],
    pub(crate) sun_size: f32,
    pub(crate) sun_intensity: f32,
    /// Tangent-plane half-radius of the directional-light disk in
    /// radians. Drives PCSS-lite shadow jitter in triangle.frag (#1023
    /// / REN-D20-NEW-01). Default 0.020 (~1.15°) matches the pre-fix
    /// hardcoded shader const; future per-cell / per-TOD tuning will
    /// drive this from CLMT / weather metadata.
    pub(crate) sun_angular_radius: f32,
    pub(crate) is_exterior: bool,
    /// Cloud layer 0 UV tile scale. `0.0` disables clouds (shader skips the sample).
    pub(crate) cloud_tile_scale: f32,
    /// Bindless texture handle for cloud_textures[0]. Only meaningful when
    /// `cloud_tile_scale > 0.0`.
    pub(crate) cloud_texture_index: u32,
    /// Bindless texture handle for the CLMT FNAM sun sprite. `0` = use
    /// the composite shader's procedural sun disc (pre-#478 behaviour).
    /// Populated at cell load when the worldspace has a CLMT with a
    /// resolvable FNAM path. See #478.
    pub(crate) sun_texture_index: u32,
    /// Cloud layer 1 UV tile scale. `0.0` disables the layer (shader
    /// branch-skips the sample). Set to `0.0` when no CNAM texture
    /// is available for the current weather.
    pub(crate) cloud_tile_scale_1: f32,
    /// Bindless texture handle for cloud_textures[1] (WTHR CNAM).
    /// Only meaningful when `cloud_tile_scale_1 > 0.0`.
    pub(crate) cloud_texture_index_1: u32,
    /// Cloud layer 2 UV tile scale. `0.0` disables the layer.
    /// Set to `0.0` when no ANAM texture is available.
    pub(crate) cloud_tile_scale_2: f32,
    /// Bindless texture handle for cloud_textures[2] (WTHR ANAM).
    pub(crate) cloud_texture_index_2: u32,
    /// Cloud layer 3 UV tile scale. `0.0` disables the layer.
    /// Set to `0.0` when no BNAM texture is available.
    pub(crate) cloud_tile_scale_3: f32,
    /// Bindless texture handle for cloud_textures[3] (WTHR BNAM).
    pub(crate) cloud_texture_index_3: u32,
    /// Current TOD-interpolated Skyrim DALC ambient cube in engine
    /// Y-up coordinates. `Some` only when a Skyrim WTHR record drove
    /// the cell load (FNV/FO3/Oblivion stay `None`). The renderer's
    /// future GPU consumer (#993) uploads this through the per-frame
    /// UBO and replaces the temporary `AMBIENT_AO_FLOOR` constant in
    /// `triangle.frag` with a normal-driven cube sample.
    pub(crate) current_dalc_cube: Option<DalcCubeYup>,
    /// The effective TOD breakpoints in force this frame — the climate's
    /// CLMT TNAM quad, or the blend of the source and target breakpoints
    /// while a WTHR cross-fade is in flight (#5179). `weather_system`
    /// republishes it every tick from the same value `compute_sun_arc` runs
    /// on, so consumers that need "which dawn/dusk bands are live" (the GNAM
    /// day/night surface blend, the `time` command's phase label) read this
    /// one copy instead of re-deriving from `WeatherDataRes`, which still
    /// holds the *source* climate until the fade promotes its target.
    pub(crate) tod_hours: [f32; 4],
    /// Current TOD/transition-sampled precipitation, lightning, glare,
    /// aurora, cloud tint, and wind-direction controls.
    pub(crate) weather: WeatherSkyState,
}
impl Resource for SkyParamsRes {}

/// Continuous-simulation cloud scroll accumulators — survive cell
/// transitions because the player exiting an exterior cell to an
/// interior shouldn't snap the cloud frame back to origin on
/// re-entry. Mirrors the `GameTimeRes` survives-transitions pattern.
///
/// Pre-#803 the four scroll fields lived on `SkyParamsRes`, which
/// `cell_loader::unload_cell` removes on every cell unload; the next
/// `apply_worldspace_weather` rebuilt the resource with `[0, 0]`
/// scroll, producing a visible cloud snap-back on every exterior
/// re-entry (~0.5 UV per 30 s of interior time, hard-cap at 1.0 via
/// the `weather_system` `rem_euclid(1.0)` wrap). Lifting the
/// accumulator into its own resource means `unload_cell` leaves it
/// alone, the renderer reads the live values per-frame, and
/// `weather_system` advances them across cell boundaries.
#[derive(Debug, Default)]
pub(crate) struct CloudSimState {
    /// Cloud layer 0 scroll offset (matches the scroll vector that
    /// formerly lived on `SkyParamsRes.cloud_scroll`).
    pub(crate) cloud_scroll: [f32; 2],
    /// Cloud layer 1 scroll offset (WTHR CNAM).
    pub(crate) cloud_scroll_1: [f32; 2],
    /// Cloud layer 2 scroll offset (WTHR ANAM).
    pub(crate) cloud_scroll_2: [f32; 2],
    /// Cloud layer 3 scroll offset (WTHR BNAM).
    pub(crate) cloud_scroll_3: [f32; 2],
}
impl Resource for CloudSimState {}

/// Continuous exterior-ground response to the current weather.
///
/// This is intentionally separate from [`WeatherSkyState`]: the latter is
/// authored weather input sampled from WTHR, while these values are history-
/// dependent surface state. Rain first raises a thin wet film and then makes
/// low spots read as standing water in the terrain shader; snow accumulates
/// independently and hides the wet response until it melts or compacts.
///
/// The state currently follows the active worldspace rather than individual
/// cells. That keeps it alive across exterior streaming and interior visits;
/// per-cell exposure/retention maps can refine it later without changing the
/// weather-to-renderer contract.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WeatherSurfaceState {
    /// Absorbed rain film on exposed ground, normalized to `[0, 1]`.
    pub(crate) wetness: f32,
    /// Snow coverage/depth at the renderer's visual accumulation cap,
    /// normalized to `[0, 1]`.
    pub(crate) snow: f32,
}

impl Default for WeatherSurfaceState {
    fn default() -> Self {
        Self {
            wetness: 0.0,
            snow: 0.0,
        }
    }
}

impl Resource for WeatherSurfaceState {}

impl SkyParamsRes {
    /// Bindless texture handles owned by this resource.
    ///
    /// Acquired in `scene/world_setup.rs` via `texture_registry.load_dds`
    /// (sun) and `acquire_by_path` (cloud layers); each call bumps the
    /// registry refcount once. Resource is worldspace-scoped (#1199) —
    /// the per-cell drop loop was removed from `unload_cell`. Update this
    /// list whenever a new bindless slot is added to the struct.
    ///
    /// The matching release is `apply_worldspace_weather`'s prologue in
    /// `scene/world_setup.rs`, which snapshots these handles before
    /// installing the next worldspace's `SkyParamsRes` and releases them
    /// after — the #1339 / #1770 acquire-new-then-release-old handoff. That
    /// is the worldspace-transition hook this doc used to describe as
    /// unbuilt, and it is the only release point (#3887).
    pub(crate) fn texture_indices(&self) -> [u32; 5] {
        [
            self.cloud_texture_index,
            self.cloud_texture_index_1,
            self.cloud_texture_index_2,
            self.cloud_texture_index_3,
            self.sun_texture_index,
        ]
    }
}

/// Full WTHR NAM0 sky color data stored for per-frame time-of-day interpolation.
/// Inserted alongside SkyParamsRes when loading an exterior cell with weather.
pub(crate) struct WeatherDataRes {
    /// 10 color groups × 6 time-of-day slots, raw monitor-space f32 per 0e8efc6.
    /// Indexed by `weather::SKY_*` and `weather::TOD_*` constants.
    pub(crate) sky_colors: [[[f32; 3]; 6]; 10],
    /// Fog distances: [day_near, day_far, night_near, night_far].
    pub(crate) fog: [f32; 4],
    /// Engine-native day/night media fitted once at WTHR translation.
    /// `weather_system` interpolates these coefficients directly and never
    /// reconstructs a medium from the legacy distance ramps.
    pub(crate) fog_media: [crate::fog::FogMedium; 2],
    /// Per-climate sunrise/sunset hour breakpoints — `weather_system`
    /// uses these to drive the TOD slot interpolator so Capital
    /// Wasteland and Mojave run on their own schedules (FO3 sunrise
    /// is ~0.3 hr earlier than FNV). Sourced from CLMT TNAM bytes
    /// (10-minute units converted to floating hours: `hour = byte / 6`).
    /// See #463.
    ///
    /// `[sunrise_begin, sunrise_end, sunset_begin, sunset_end]` in hours.
    /// Defaults (6.0, 10.0, 18.0, 22.0) match the pre-#463 hardcoded
    /// values so synthetic test cells and non-climate content keep
    /// their old behaviour.
    pub(crate) tod_hours: [f32; 4],
    /// Skyrim DALC 6-axis ambient cube, four entries
    /// (sunrise / day / sunset / night) already converted to engine
    /// Y-up. `None` on FNV / FO3 / Oblivion (different ambient model).
    /// `weather_system` interpolates between the (slot_a, slot_b)
    /// DALC pair the TOD picker chose for `sky_colors`; TOD slots 4/5
    /// (high_noon / midnight) fold into day / night per the WTHR
    /// parser's padding convention. See #993.
    pub(crate) skyrim_dalc_per_tod: Option<[DalcCubeYup; 4]>,
    /// WTHR DATA `wind_speed` byte (#1033 / REN-D15-NEW-12). Drives
    /// the cloud-layer scroll rate in `weather_system` so calm vs
    /// storm weather animates at different rates. Pre-#1033 the byte
    /// was parsed onto `WeatherRecord` but never projected into the
    /// runtime resource; the cloud animation used a hardcoded
    /// `0.018 UV/sec` literal regardless of weather. `0` on the
    /// synthetic-fallback path (no WTHR record loaded) — produces a
    /// static cloud layer, which is the safe default.
    pub(crate) wind_speed: u8,
    /// WTHR precipitation intensity: rain is `1`, snow is a subtle wet-surface
    /// contribution, and clear/cloudy/unclassified weather is `0`.
    pub(crate) precipitation: f32,
    /// Authored X/Y cloud motion controls for the four compatibility layers.
    pub(crate) cloud_layer_velocities: [[f32; 2]; 4],
    /// Whether the source WTHR record actually authored a value for each
    /// layer (#3985) — an authored `0` and "no ONAM/RNAM/QNAM at all" are
    /// both `[0.0, 0.0]` in [`Self::cloud_layer_velocities`] with nothing
    /// else to tell them apart. See
    /// [`byroredux_plugin::esm::records::weather::WeatherRecord::cloud_layer_velocities_authored`].
    pub(crate) cloud_layer_velocities_authored: [bool; 4],
    /// Authored PNAM cloud tint tables, four TOD samples per layer.
    pub(crate) cloud_layer_colors: [[[f32; 3]; 4]; 4],
    /// Authored JNAM cloud alpha tables, four TOD samples per layer.
    pub(crate) cloud_layer_alphas: [[f32; 4]; 4],
    /// Non-colour weather controls from DATA/NAM2/NAM3/classification.
    pub(crate) weather: WeatherSkyState,
    /// Oblivion `WTHR.HNAM.grassDimmer` — the per-weather ground-cover colour
    /// multiplier (#4057, `exal-groundcover.md` §12 Phase 5 remainder).
    ///
    /// `1.0` for every game that does not ship `HNAM`, which is all of them
    /// except Oblivion. `weather_system` cross-fades this alongside
    /// `precipitation` and publishes it as
    /// [`byroredux_core::ecs::components::groundcover::GroundCoverDimmer`], so
    /// the sward's tint follows a `WTHR` change instead of freezing at the
    /// weather that happened to be active when the worldspace was entered.
    pub(crate) grass_dimmer: f32,
    /// Oblivion `HNAM` "Sunlight Dimmer" — the HDR block's
    /// directional-sunlight multiplier, translated once at the EXAL
    /// boundary and multiplied into the sampled `SKY_SUNLIGHT` colour in
    /// `weather_system` so an authored dim/bright sun reaches
    /// `CellLightingRes.directional_color`. Parsed since #537 but never
    /// consumed — every Oblivion exterior rendered at the palette's own
    /// sun colour regardless of the worldspace's HDR dimmer. `1.0` for
    /// every game that does not ship `HNAM`, and when the cross-fade
    /// blends two weathers this fades with the palette like
    /// [`Self::grass_dimmer`] does.
    pub(crate) sunlight_dimmer: f32,
    /// #4416 — the exterior's base image space per sky-colour time-of-day
    /// slot (Sunrise, Day, Sunset, Night, High Noon, Midnight — the
    /// `sky_colors` order), resolved at the EXAL boundary by
    /// [`crate::env_translate::translate_weather`] through
    /// [`crate::env_translate::ImageSpaceSources`]: the weather's own
    /// `IMSP` on Skyrim/FO4, else the worldspace's inherited `INAM`
    /// (FO3/FNV), else the identity grade — with FO3/FNV's per-slot weather
    /// IMAD folded on top. Sampled and cross-faded with the sky colours by
    /// `weather_system`, which publishes it as `ImageSpaceBase` in exteriors.
    pub(crate) image_space: [byroredux_scripting::ImageSpace;
        byroredux_plugin::esm::records::weather::SKY_TIME_SLOTS],
}
impl Resource for WeatherDataRes {}

/// In-flight cross-fade between two `WeatherDataRes` snapshots (M33.1).
///
/// When a cell load encounters a different weather while one is already
/// active, `scene.rs` keeps the current `WeatherDataRes` in place and
/// inserts this resource carrying the new target plus the WTHR TNAM
/// transition duration. `weather_system` advances `elapsed_secs` each
/// frame, blends the post-TOD-sample colours by `t = elapsed/duration`,
/// and on completion swaps the live `WeatherDataRes` to `target` and
/// removes the transition resource.
///
/// Interpolation happens after each side runs its own TOD-slot pick so
/// the transition stays correct across the midnight wrap (each weather
/// can be on a different slot).
pub(crate) struct WeatherTransitionRes {
    pub(crate) target: WeatherDataRes,
    pub(crate) elapsed_secs: f32,
    pub(crate) duration_secs: f32,
    /// `true` once `weather_system` has promoted `target` into the
    /// live `WeatherDataRes`. Subsequent frames skip the timer
    /// advance + blend path, so the resource stays resident as a
    /// dormant idempotent record of "the last cross-fade landed
    /// successfully" without further state mutation.
    ///
    /// Pre-fix the dormant state was encoded as
    /// `duration_secs = f32::INFINITY`, which (a) used float
    /// arithmetic as a state machine — the `elapsed/duration`
    /// computation produced 0 by chance — and (b) let
    /// `elapsed_secs += dt` accumulate every frame forever until it
    /// saturated to INFINITY itself and made the ratio NaN. The
    /// explicit bool drops both hazards. See REN-D15-NEW-07 (audit
    /// 2026-05-09).
    pub(crate) done: bool,
}
impl Resource for WeatherTransitionRes {}
