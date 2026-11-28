//! M-LIGHT reference rungs (`--cornell-reference <rung>`): Cornell scenes
//! built to be measured against an external converged reference
//! (reference-lighting PRD RLT-REF-001, RLT-REF-009, RLT-REF-010,
//! RLT-GI-010; Phase 1a, #6601).
//!
//! Unlike the `--cornell` showcase and the `--cornell-oracle` ladder, every
//! rung here obeys RLT-REF-009: no emissive surface, no glass or eye
//! material, no authored ambient, no DALC cube, no fog in range and no
//! medium. Emitters are inverse-square (`Emitter::inverse_square_point`,
//! RLT-REF-002) with a source radius of at most a tenth of the distance to
//! any measured receiver, and every material carries
//! `MAT_FLAG_LAMBERTIAN_REFERENCE`, so the engine shades it as the plain
//! Lambert lobe Mitsuba's `diffuse` BSDF is (RLT-GI-010).
//!
//! The rungs are room-scale (metres, not the showcase's 8-unit box) so the
//! scale-dependent parts of the pipeline — SSAO radius, ray offsets, the GI
//! distance fade — run in the regime game interiors do. Every rung records
//! its meshes' CPU sources so `--export-mitsuba` can serialise it.

use super::mitsuba_export::CornellMeshSources;
use super::*;
use byroredux_core::lighting::{
    Emitter, Meters, RadiantIntensityRgb, VisibilityMask, BETHESDA_UNITS_PER_METER,
};

/// One metre in world units.
const M: f32 = BETHESDA_UNITS_PER_METER;

/// Emitter source radius: 1.4 world units (2 cm), above the shader's
/// 1.0-unit sampling floor, so the engine's disk and the reference's sphere
/// have the same radius; *r*/*d* ≤ 0.07 at every surface of every rung.
const SOURCE_RADIUS_M: f32 = 0.02;

/// Room-scale box: 4 m wide and deep, 3 m tall, open at +Z.
const BOX_HALF_W: f32 = 2.0 * M;
const BOX_HEIGHT: f32 = 3.0 * M;
const BOX_WALL_T: f32 = 0.05 * M;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReferenceRung {
    /// RLT-REF-010 point calibration: one inverse-square emitter above a
    /// white Lambertian plane, nothing else.
    CalibrationPoint,
    /// The L4 / closure rung: a Lambertian Cornell room (white floor,
    /// ceiling and back wall, red left wall, green right wall) with a tall
    /// block and a short cube, lit by one point emitter under the ceiling.
    LambertBox,
    /// RLT-REF-014 material validation: one flat plane per mapped material
    /// on a black floor under one emitter, seen from three view angles
    /// (`--cornell-reference-view 0|1|2`). Phase 1a maps the Lambertian
    /// materials only (albedos 0.2 / 0.5 / 0.8 and the two wall colours);
    /// the specular rows join at Phase 4's entry (`path-tracing.md` §2.2).
    Materials,
    /// RLT-REF-010, the sun: a plane under the exterior sun alone, the
    /// sun row of A15.
    CalibrationSun,
    /// RLT-REF-010, the environment: one unit-albedo Lambertian probe
    /// under a uniform sky with no disc, glow or clouds.
    CalibrationSky,
    /// RLT-SHD-001 / -004, local light: a 15 cm inverse-square source 2 m
    /// above a white floor, a plate 0.5 m up whose straight edge casts a
    /// 10 cm full penumbra, and a 1 cm rod beside it (the thin caster A4
    /// reads). Close-up camera on the plate's shadow edge.
    Penumbra,
    /// RLT-SHD-001, the sun: the same plate under the exterior sun (the
    /// procedural sky, its directional light and angular radius), seen by a
    /// ~0.4 m-wide close-up so even the physical 0.0047 rad sun spans 10 px.
    PenumbraSun,
    /// RLT-CON-004: exterior geometry straddling the traced shadow fade.
    FarSun,
    /// RLT-SHD-010: the local-light penumbra rung with a 10 cm cube carried
    /// 1 m above the floor across the plate's shadow edge by the bench clock
    /// (`BenchMover`; `--bench-camera-length` ends its motion in a hold,
    /// `--bench-camera-freeze-at` parks it for the converged reference).
    PenumbraMoving,
    /// RLT-REF-014's normal-mapped plane: one 0.6 m Lambertian plane carrying
    /// the RLT-CON-007 normal map through the authored tangent frame, on a
    /// black floor, lit from three azimuths (`--cornell-reference-view 0|1|2`
    /// moves the emitter, not the camera). Checks the normal-map convention
    /// (`path-tracing.md` §2.2) before the contact rung relies on it.
    NormalPlane,
    /// RLT-SHD-005: the Lambertian box with six dim emitters (1/60 of the
    /// hero, about `RESERVOIR_W_CLAMP`'s 64× ratio) around the hero, the
    /// case the reservoir weight cap exists for and biases.
    ReservoirClamp,
    /// RLT-CON-001 / -003 / -006 / -007 / -008: casters resting on a
    /// normal-mapped floor under one emitter — a tall block, a small cube, a
    /// plank lying flat, an alpha-tested panel and a skinned actor proxy (a
    /// column whose bone an `AnimationPlayer` yaws on a 2 s loop, advancing
    /// under `renderer-stepped`, RLT-REF-017) — each with its contact band.
    Contact,
    /// RLT-SHD-012's flat-floor control: the contact rung with the floor's
    /// normal map left off (same casters, emitter, bands and camera). The
    /// plank's error here is what the sampler and the reference leave when
    /// no shading normal disagrees with the geometric one, so the
    /// normal-mapped plank is read against it (#6721).
    ContactFlat,
    /// RLT-REF-012 / RLT-EXP-003: the reference environment. No geometry;
    /// the exterior sky of the sky rungs with its sun disc removed (glow
    /// kept), seen by one of six 90° views (`--cornell-reference-view
    /// 0..=5`, the faces of [`SKY_ENV_FACES`]) through `--window-size
    /// 512x512`, four times the engine cube's face resolution. The composite
    /// background evaluates `sky_radiance` per pixel, so the six captures
    /// are the analytic sky, not the baked cube; `scripts/m-light/gates.py`
    /// resamples them into the lat-long map the Mitsuba scene reads.
    SkyEnvironment,
    /// RLT-REF-021: the probe-only sky rung — one probe sphere under the
    /// exterior sky, nothing else, the directional light dropped
    /// (RLT-REF-019). `--cornell-reference-view` picks the probe:
    /// 0..=4 a specular-only dielectric (diffuse albedo 0) at
    /// [`SKY_PROBE_ROUGHNESS`], 5..=9 a conductor at the same roughness,
    /// 10 a unit-albedo Lambertian probe (sky diffuse).
    SkyProbe,
    /// RLT-IBL-010 (A17): an unoccluded Lambertian plane under the same sky
    /// with the sun ON — its directional light, and the disc the probe bakes.
    /// The reference lights it with the sun as an emitter and the disc-free
    /// environment (RLT-REF-012), so the sun's energy is counted once there.
    SkySunPlane,
    /// RLT-REF-008 / RLT-REF-014's specular rows, validated at Phase 4's
    /// entry: the material rung's layout with ten planes, a Disney-branch
    /// dielectric (ρ 0.5) and a conductor (ρ 0.95) at each of
    /// [`SKY_PROBE_ROUGHNESS`], under one emitter at the same three views.
    MaterialsSpecular,
    /// RLT-IBL-004: the contact rung's static casters (block, cube, plank)
    /// and their bands on a specular-only conductor floor (ρ 0.95,
    /// roughness 0.74, which the environment term reads from the probe
    /// alone) under the probe-only sky (RLT-REF-019, RLT-REF-021). The
    /// reference's `direct` integrator lights the floor with the sky times
    /// its visibility, so each band's energy against it measures the sky
    /// specular occlusion; an open-floor region is its control.
    ContactSky,
}

/// RLT-REF-021: the L5 sweep's five roughness values (the showcase's
/// `metal_rough_*` probes, `cornell.rs`).
pub(crate) const SKY_PROBE_ROUGHNESS: [f32; 5] = [0.02, 0.26, 0.50, 0.74, 0.98];
/// Views of [`ReferenceRung::SkyProbe`]: five dielectric, five conductor,
/// one Lambertian.
pub(crate) const SKY_PROBE_VIEWS: u32 = 11;
/// The six view directions of [`ReferenceRung::SkyEnvironment`].
pub(crate) const SKY_ENV_FACES: [[f32; 3]; 6] = [
    [1.0, 0.0, 0.0],
    [-1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, -1.0, 0.0],
    [0.0, 0.0, 1.0],
    [0.0, 0.0, -1.0],
];

impl ReferenceRung {
    /// How many `--cornell-reference-view` values this rung has (#7387): a view at or past it is
    /// refused rather than aliased to another view. An exhaustive match, so a new rung states its
    /// count.
    pub(crate) const fn view_count(self) -> u32 {
        match self {
            ReferenceRung::SkyEnvironment => SKY_ENV_FACES.len() as u32,
            ReferenceRung::SkyProbe => SKY_PROBE_VIEWS,
            // Three view angles (materials), light azimuths (normal plane) or caster poses (far sun).
            ReferenceRung::Materials
            | ReferenceRung::MaterialsSpecular
            | ReferenceRung::NormalPlane
            | ReferenceRung::FarSun => 3,
            // The front view and the close-up of the tall block's base (A8).
            ReferenceRung::LambertBox | ReferenceRung::ReservoirClamp => 2,
            // One fixed camera.
            ReferenceRung::CalibrationPoint
            | ReferenceRung::CalibrationSun
            | ReferenceRung::CalibrationSky
            | ReferenceRung::Penumbra
            | ReferenceRung::PenumbraSun
            | ReferenceRung::PenumbraMoving
            | ReferenceRung::Contact
            | ReferenceRung::ContactFlat
            | ReferenceRung::SkySunPlane
            | ReferenceRung::ContactSky => 1,
        }
    }
}
/// Toward the sun on the sky rungs: about 37° up, off every axis, so its
/// glow falls inside one face of the environment capture and away from the
/// probe's silhouette.
const SKY_SUN_DIR: [f32; 3] = [0.5, 0.6, -0.62];
/// Probe sphere radius and the camera's distance from its centre.
const SKY_PROBE_RADIUS_M: f32 = 0.45;
const SKY_PROBE_CAMERA_M: f32 = 1.8;
/// `sky_radiance` draws the disc where cos(angle to the sun) exceeds
/// `sun_size - 0.002`; past 1.002 no direction does (`include/sky.glsl`).
const SUN_DISC_OFF: f32 = 2.0;

/// RLT-REF-019: present on the sky-only rungs. `collect_lights` drops the
/// directional light after `compute_directional_upload` and the exporter
/// exports no sun; the sky dome, which reads the sun intensity separately
/// (`dome.sun_dir.w`), keeps its glow and, on the probe rung, its disc.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ReferenceSkyOnly;
impl byroredux_core::ecs::Resource for ReferenceSkyOnly {}

/// A rung's vertical field of view in radians, applied to the harness camera
/// by `spawn_initial_camera` (the environment rung's 90° faces).
#[derive(Clone, Copy, Debug)]
pub(crate) struct HarnessCameraFov(pub f32);
impl byroredux_core::ecs::Resource for HarnessCameraFov {}

/// A penumbra profile line (RLT-SHD-001): on `receiver`, perpendicular to a
/// shadow edge, from the umbra side to the lit side, averaged across a strip
/// `half_width` either side. World units.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProfileLine {
    pub name: String,
    pub receiver: String,
    pub start: Vec3,
    pub end: Vec3,
    pub half_width: f32,
}

/// A fixed receiver region, an axis-aligned world box (A4's thin-rod shadow).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MeasuredRegion {
    pub name: String,
    pub receiver: String,
    pub min: Vec3,
    pub max: Vec3,
}

/// A caster's contact band (§2): the receiver strip `width` wide bordering
/// the caster's footprint, a rectangle `half_extents` (x, z) about
/// `centre` turned by `yaw` about +Y. World units.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ContactBand {
    pub name: String,
    pub receiver: String,
    pub centre: Vec3,
    pub half_extents: [f32; 2],
    pub yaw: f32,
    pub width: f32,
    /// A moving caster's band follows this named entity: the exporter
    /// takes `centre` and `yaw` from its `GlobalTransform` at the exported
    /// frame (the skinned proxy's bone, RLT-CON-003).
    pub follow: Option<String>,
}

/// World-space measurement geometry a rung fixes in its own code, exported
/// beside the scene as `rung.tsv` for `scripts/m-light/gates.py`.
#[derive(Default, Debug, Clone, PartialEq)]
pub(crate) struct RungMeasurements {
    pub profile_lines: Vec<ProfileLine>,
    pub regions: Vec<MeasuredRegion>,
    pub bands: Vec<ContactBand>,
    /// Shape names a visibility reference removes (§2 visibility).
    pub casters: Vec<String>,
    /// The smooth step fitted to each profile (RLT-SHD-001). `disk_segment`:
    /// the uncovered fraction of a disk source behind a straight edge.
    pub step_form: &'static str,
    /// Pixels whose unshadowed direct is below this fraction of the
    /// region's brightest are excluded from visibility (§2).
    pub min_unshadowed: f32,
    /// Scene-linear radiance of a uniform sky, used by the calibration rung.
    pub constant_sky: Option<[f32; 3]>,
    /// RLT-REF-012: the reference environment is the analytic sky without
    /// its disc, which the gate runner writes as `sky_env.exr` beside the
    /// scene from the [`ReferenceRung::SkyEnvironment`] captures.
    pub analytic_sky: bool,
}
impl byroredux_core::ecs::Resource for RungMeasurements {}

impl ReferenceRung {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::CalibrationPoint => "cal-point",
            Self::LambertBox => "lambert-box",
            Self::Materials => "materials",
            Self::CalibrationSun => "cal-sun",
            Self::CalibrationSky => "cal-sky",
            Self::Penumbra => "penumbra",
            Self::PenumbraSun => "penumbra-sun",
            Self::FarSun => "far-sun",
            Self::PenumbraMoving => "penumbra-moving",
            Self::ReservoirClamp => "restir-clamp",
            Self::NormalPlane => "normal-plane",
            Self::Contact => "contact",
            Self::ContactFlat => "contact-flat",
            Self::SkyEnvironment => "sky-env",
            Self::SkyProbe => "sky-probe",
            Self::SkySunPlane => "sky-sun-plane",
            Self::MaterialsSpecular => "materials-spec",
            Self::ContactSky => "contact-sky",
        }
    }
}

const RUNGS: &[ReferenceRung] = &[
    ReferenceRung::CalibrationPoint,
    ReferenceRung::LambertBox,
    ReferenceRung::Materials,
    ReferenceRung::CalibrationSun,
    ReferenceRung::CalibrationSky,
    ReferenceRung::Penumbra,
    ReferenceRung::PenumbraSun,
    ReferenceRung::FarSun,
    ReferenceRung::PenumbraMoving,
    ReferenceRung::ReservoirClamp,
    ReferenceRung::NormalPlane,
    ReferenceRung::Contact,
    ReferenceRung::ContactFlat,
    ReferenceRung::SkyEnvironment,
    ReferenceRung::SkyProbe,
    ReferenceRung::SkySunPlane,
    ReferenceRung::MaterialsSpecular,
    ReferenceRung::ContactSky,
];

/// Penumbra rungs: plate half-extent and height above the floor.
const PLATE_HALF_M: f32 = 0.3;
const PLATE_HEIGHT_M: f32 = 0.5;
/// The local light's height and source radius (r/d <= 0.075 at the floor).
const PENUMBRA_LIGHT_HEIGHT_M: f32 = 2.0;
const PENUMBRA_SOURCE_RADIUS_M: f32 = 0.15;
/// Unit vector toward the sun on the sun rung.
const PENUMBRA_SUN_DIR: [f32; 3] = [0.3, 0.95, 0.0];
/// Toward the sun on its calibration rung: off-axis, so the cosine law is
/// tested, not only the intensity.
const CALIBRATION_SUN_DIR: [f32; 3] = [0.35, 0.85, 0.25];

/// Present on every reference scene that installs the procedural sky: the reference render lights its probes
/// with that sky as it is, with no ground under it, so the raster's diffuse sky term must not add one
/// ([`byroredux_renderer::SkyParams::ground_albedo`]).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ReferenceEnvironment;
impl byroredux_core::ecs::Resource for ReferenceEnvironment {}

/// The install-less exterior sun and sky at `dir` (the procedural fallback
/// a plugin-less world gets), with the reference rungs' no-medium,
/// fog-beyond-the-scene lighting (RLT-REF-009).
fn insert_reference_sun(world: &mut World, dir: Vec3) {
    let dir = dir.normalize().to_array();
    let lighting = crate::env_translate::procedural_fallback_cell_lighting(dir);
    world.insert_resource(CellLightingRes {
        directional_fade: Some(1.0),
        fog_medium: crate::fog::FogMedium::DISABLED,
        ..CellLightingRes::bare(
            lighting.ambient,
            lighting.directional_color,
            dir,
            false,
            [0.0; 3],
            100_000.0,
            1_000_000.0,
        )
    });
    world.insert_resource(crate::env_translate::procedural_fallback_sky(dir));
    world.insert_resource(ReferenceEnvironment);
}

/// The exterior sky of the sky rungs (RLT-REF-012, RLT-REF-019, RLT-REF-021):
/// the procedural sky toward [`SKY_SUN_DIR`] with no cloud deck, moon or
/// stars, so it is static and identical on both sides; the directional
/// light is dropped ([`ReferenceSkyOnly`]) unless `sun` is set. `disc:
/// false` removes the sun disc and keeps its glow, which is sky radiance
/// (RLT-REF-012).
fn insert_reference_sky(world: &mut World, disc: bool, sun: bool) {
    insert_reference_sun(world, Vec3::from_array(SKY_SUN_DIR));
    {
        let mut sky = world.resource_mut::<crate::components::SkyParamsRes>();
        sky.weather.cloud_coverage = 0.0;
        sky.weather.moon_glare = 0.0;
        sky.weather.stars_color = [0.0; 3];
        if !disc {
            sky.sun_size = SUN_DISC_OFF;
        }
    }
    // RLT-REF-009: no medium. The exterior haze floor (VLM-SH-010) is an
    // engine-supplied medium on every exterior; along a sky ray it adds
    // in-scatter the probe's lighting never sees, so the environment
    // capture would carry it into the reference.
    world.insert_resource(crate::fog::ExteriorHazeFloor::OFF);
    // `weather_system` rewrites the exterior cell's fog medium every frame
    // from the weather table, so the rung's `FogMedium::DISABLED` holds only
    // if the table itself carries none: the procedural weather fits a
    // medium from its fog ramp, which fogs every sky ray toward the horizon.
    let mut weather = crate::env_translate::procedural_fallback_weather();
    weather.fog_media = [crate::fog::FogMedium::DISABLED, crate::fog::FogMedium::DISABLED];
    world.insert_resource(weather);
    if !sun {
        world.insert_resource(ReferenceSkyOnly);
    }
}

/// The probe of [`ReferenceRung::SkyProbe`] view `view`.
fn sky_probe_material(view: u32) -> Material {
    let roughness = SKY_PROBE_ROUGHNESS[(view % 5) as usize];
    match view {
        // Specular only: no diffuse lobe for the sky to light (§3.4).
        0..=4 => pbr([0.0; 3], 0.0, roughness),
        5..=9 => pbr([0.95; 3], 1.0, roughness),
        _ => lambert([1.0; 3]),
    }
}

/// The Lambertian materials Phase 1a maps (RLT-REF-008, RLT-REF-014), by
/// plane name; the material rung lays them out left to right.
pub(crate) const MATERIAL_PLANES: &[(&str, [f32; 3])] = &[
    ("mat_lambert_020", [0.2; 3]),
    ("mat_lambert_050", [0.5; 3]),
    ("mat_lambert_080", [0.8; 3]),
    ("mat_lambert_red", [0.65, 0.05, 0.05]),
    ("mat_lambert_green", [0.12, 0.45, 0.15]),
];

/// Parse `--cornell-reference-view N` (default 0) for `rung`: the material rungs' view
/// angle, the normal plane's light azimuth and the far-sun caster pose (0..=2), the box
/// rungs' close-up (1), the environment rung's face (0..=5) or the sky probe (0..=10); a rung
/// with one fixed camera takes only 0. A view past the rung's [`ReferenceRung::view_count`]
/// is an error (#7387): before, the parser checked one global bound and the rung's own
/// arm clamped or fell through to its last view, so a mistyped gate invocation produced a
/// plausible duplicate capture instead of a refusal.
pub(crate) fn cornell_reference_view(args: &[String], rung: ReferenceRung) -> Result<u32, String> {
    let Some(index) = args.iter().position(|a| a == "--cornell-reference-view") else {
        return Ok(0);
    };
    let count = rung.view_count();
    match args.get(index + 1).map(|v| v.parse::<u32>()) {
        Some(Ok(view)) if view < count => Ok(view),
        _ => Err(format!(
            "--cornell-reference-view requires 0..={} for the {rung:?} rung",
            count - 1
        )),
    }
}

/// Parse `--cornell-reference <rung>`; a typo is an error, never a fallback.
pub(crate) fn cornell_reference_rung(args: &[String]) -> Result<Option<ReferenceRung>, String> {
    let Some(index) = args.iter().position(|a| a == "--cornell-reference") else {
        return Ok(None);
    };
    let names: Vec<&str> = RUNGS.iter().map(|r| r.name()).collect();
    let value = args
        .get(index + 1)
        .ok_or_else(|| format!("--cornell-reference requires one of: {}", names.join(", ")))?;
    RUNGS
        .iter()
        .copied()
        .find(|r| r.name() == value.to_ascii_lowercase())
        .map(Some)
        .ok_or_else(|| {
            format!(
                "unknown reference rung '{value}'; expected one of: {}",
                names.join(", ")
            )
        })
}

/// A Lambertian reference material (RLT-GI-010): metalness 0, no specular,
/// ior 1, and the flag that removes every GGX lobe and (1 − F) weight.
pub(super) fn lambert(color: [f32; 3]) -> Material {
    let mut material = matte(color);
    material.metalness = 0.0;
    material.specular_strength = 0.0;
    material.ior = 1.0;
    material.effect_shader_flags |=
        byroredux_renderer::vulkan::material::material_flag::LAMBERTIAN_REFERENCE;
    material
}

fn spawn_reference_emitter(world: &mut World, pos: Vec3, intensity: f32, name: &str) {
    spawn_reference_emitter_sized(world, pos, intensity, SOURCE_RADIUS_M, name);
}

fn spawn_reference_emitter_sized(world: &mut World, pos: Vec3, intensity: f32, radius_m: f32, name: &str) {
    let light = world.spawn();
    world.insert(light, Transform::new(pos, Quat::IDENTITY, 1.0));
    world.insert(light, GlobalTransform::new(pos, Quat::IDENTITY, 1.0));
    world.insert(
        light,
        LightSource {
            emitter: Emitter::inverse_square_point(
                RadiantIntensityRgb::new([intensity; 3]),
                // Cull window starts at the range: 10 m keeps every surface
                // of every rung inside it, so the law is pure 1/d².
                Meters::new(10.0),
                Meters::new(radius_m),
                VisibilityMask::for_legacy_local_light(),
            ),
            shadow_flags: byroredux_core::ecs::LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL,
            ..Default::default()
        },
    );
    name_entity(world, light, name);
}

/// The contact rung (RLT-CON-001, -006, -007, -008). Each caster's band
/// width obeys §2: an upright caster's is at most a tenth of its height, the
/// plank's is a fixed width no less than its thickness.
///
/// `normal_mapped` is RLT-CON-007's floor; `false` builds the flat-floor
/// control (`ReferenceRung::ContactFlat`), the same scene with the map off.
fn setup_contact_rung(
    world: &mut World,
    builder: &mut MeshBuilder,
    neutral: TextureHandle,
    normal_mapped: bool,
) -> (Vec3, Vec3) {
    // RLT-CON-007: the floor carries the Cornell wavy normal map through an
    // authored tangent frame (T = dP/du, w = +1), the frame Mitsuba's
    // `normalmap` builds from the UVs (`path-tracing.md` §2.2).
    let floor = builder.box_mesh_with_uv_tangents([1.5 * M, BOX_WALL_T, 1.5 * M]);
    let floor_entity = spawn_object(world, floor, neutral, Vec3::new(0.0, -BOX_WALL_T, 0.0), Quat::IDENTITY,
        lambert([0.8; 3]), "con_floor");
    if normal_mapped {
        let normal_map = builder.register_texture(16, 16, super::builders::wavy_normal_map_pixels(16));
        insert_normal_map(world, floor_entity, normal_map);
    }

    let (mut bands, mut caster_names) = spawn_static_contact_casters(world, builder, neutral);

    // RLT-CON-008: an upright 0.6 m panel, 1 cm thick, cut by a cross of
    // slots symmetric in u so its front and back faces (whose u runs
    // opposite ways) open the same holes. Three layers at most along any
    // ray, under MAX_ALPHA_SKIP_LAYERS = 8. Band <= 0.06 m.
    let cutout = builder.register_texture(64, 64, cutout_pixels(64));
    let panel = builder.box_mesh([0.2 * M, 0.3 * M, 0.005 * M]);
    let mut material = lambert([0.8; 3]);
    material.alpha_test = true;
    material.alpha_threshold = 0.5;
    material.alpha_test_func = 6;
    spawn_object(world, panel, TextureHandle(cutout), Vec3::new(0.45 * M, 0.3 * M, 0.45 * M), Quat::IDENTITY,
        material, "con_panel");
    bands.push(ContactBand {
        name: "con_panel_band".into(),
        receiver: "con_floor".into(),
        centre: Vec3::new(0.45 * M, 0.0, 0.45 * M),
        half_extents: [0.2 * M, 0.005 * M],
        yaw: 0.0,
        width: 0.04 * M,
        follow: None,
    });
    caster_names.push("con_panel".into());

    // RLT-CON-003: the skinned proxy, a 0.8 m column standing at (0.05,
    // 0.55) m, every vertex on one bone whose yaw an AnimationPlayer sweeps
    // 0 -> 0.6 -> 0 rad over 2 s. Under `renderer-static` (dt 0) it holds
    // its t = 0 pose; under `renderer-stepped` it moves, so a capture and
    // `--export-mitsuba-frame` at the same frame see the same pose
    // (RLT-REF-017, RLT-EXP-001). Band <= 0.08 m.
    spawn_skinned_proxy(world, builder, neutral, Vec3::new(0.05 * M, 0.0, 0.55 * M), [0.075, 0.4, 0.075]);
    bands.push(ContactBand {
        name: "con_proxy_band".into(),
        receiver: "con_floor".into(),
        centre: Vec3::new(0.05 * M, 0.0, 0.55 * M),
        half_extents: [0.075 * M, 0.075 * M],
        yaw: 0.0,
        width: 0.05 * M,
        follow: Some(PROXY_BONE.into()),
    });
    caster_names.push("con_proxy".into());

    // A 10 cm source, 2.4 m up and to one side, so every caster has a lit
    // and a shadowed side of its band.
    spawn_reference_emitter_sized(world, Vec3::new(0.6 * M, 2.4 * M, 0.9 * M), 3.0, 0.1, "con_emitter");
    world.insert_resource(RungMeasurements {
        bands,
        casters: caster_names,
        min_unshadowed: 0.05,
        ..Default::default()
    });
    // ~2 m from the casters: a 3 cm band is ~13 px at 720p.
    (Vec3::new(0.0, 1.6 * M, 1.2 * M), Vec3::new(0.0, 0.0, 0.05 * M))
}

/// The contact rung's three static casters resting on `con_floor` (a tall
/// block, a small cube and RLT-CON-006's plank), each with its contact band;
/// shared by [`ReferenceRung::Contact`] and [`ReferenceRung::ContactSky`].
fn spawn_static_contact_casters(
    world: &mut World,
    builder: &mut MeshBuilder,
    neutral: TextureHandle,
) -> (Vec<ContactBand>, Vec<String>) {
    // (name, half extents in metres, centre x/z, yaw, band width in metres)
    #[allow(clippy::type_complexity)] // the comment above names each element; an alias would only move the naming
    let casters: [(&str, [f32; 3], [f32; 2], f32, f32); 3] = [
        // 1.2 m tall: band <= 0.12 m.
        ("con_block", [0.15, 0.6, 0.15], [-0.55, -0.35], 0.3, 0.06),
        // 0.3 m cube: band <= 0.03 m.
        ("con_cube", [0.15, 0.15, 0.15], [0.5, -0.4], -0.4, 0.03),
        // RLT-CON-006: a 2 cm plank lying flat, a fixed 3 cm band.
        ("con_plank", [0.3, 0.01, 0.075], [-0.4, 0.45], 0.2, 0.03),
    ];
    let mut bands = Vec::new();
    let mut caster_names = Vec::new();
    for (name, half, [x, z], yaw, band) in casters {
        let mesh = builder.box_mesh([half[0] * M, half[1] * M, half[2] * M]);
        spawn_object(world, mesh, neutral, Vec3::new(x * M, half[1] * M, z * M), Quat::from_rotation_y(yaw),
            lambert([0.8; 3]), name);
        bands.push(ContactBand {
            name: format!("{name}_band"),
            receiver: "con_floor".into(),
            centre: Vec3::new(x * M, 0.0, z * M),
            half_extents: [half[0] * M, half[2] * M],
            yaw,
            width: band * M,
            follow: None,
        });
        caster_names.push(name.to_string());
    }
    (bands, caster_names)
}

/// RLT-IBL-004's rung ([`ReferenceRung::ContactSky`]): the static contact
/// casters on a specular-only conductor floor under the probe-only sky, the
/// same camera as the contact rung.
fn setup_contact_sky_rung(world: &mut World, builder: &mut MeshBuilder, neutral: TextureHandle) -> (Vec3, Vec3) {
    insert_reference_sky(world, true, false);
    let floor = builder.box_mesh([1.5 * M, BOX_WALL_T, 1.5 * M]);
    spawn_object(world, floor, neutral, Vec3::new(0.0, -BOX_WALL_T, 0.0), Quat::IDENTITY,
        pbr([0.95; 3], 1.0, CONTACT_SKY_FLOOR_ROUGHNESS), "con_floor");
    let (bands, casters) = spawn_static_contact_casters(world, builder, neutral);
    // The control: open floor 0.85 m or more from every caster.
    let regions = vec![MeasuredRegion {
        name: "con_open_floor".into(),
        receiver: "con_floor".into(),
        min: Vec3::new(0.35 * M, -0.01 * M, 0.3 * M),
        max: Vec3::new(0.6 * M, 0.01 * M, 0.6 * M),
    }];
    world.insert_resource(RungMeasurements {
        bands,
        regions,
        casters,
        analytic_sky: true,
        ..Default::default()
    });
    (Vec3::new(0.0, 1.6 * M, 1.2 * M), Vec3::new(0.0, 0.0, 0.05 * M))
}

/// [`ReferenceRung::ContactSky`]'s floor roughness: one of
/// [`SKY_PROBE_ROUGHNESS`] at or above the traced-reflection cut (0.6), so a
/// conductor's environment term is the probe's share alone.
const CONTACT_SKY_FLOOR_ROUGHNESS: f32 = SKY_PROBE_ROUGHNESS[3];

/// The contact rung's skinned proxy bone (RLT-CON-003).
pub(crate) const PROXY_BONE: &str = "con_proxy_bone";

/// A box column skinned to one bone at `base` (its foot on the floor), the
/// bone driven by a looping yaw clip. The bind pose is the bone at the
/// origin, so the bind inverse is the identity.
fn spawn_skinned_proxy(world: &mut World, builder: &mut MeshBuilder, neutral: TextureHandle, base: Vec3, half_m: [f32; 3]) {
    use byroredux_core::animation::{
        AnimationClip, AnimationClipRegistry, AnimationPlayer, CycleType, KeyType, RotationKey, TransformChannel,
    };
    use byroredux_core::ecs::{RenderLayer, SkinnedMesh};
    let half = [half_m[0] * M, half_m[1] * M, half_m[2] * M];
    let (mut vertices, indices) = box_vertices_colored(half, [1.0; 3]);
    for vertex in &mut vertices {
        vertex.position[1] += half[1]; // foot at the bone, y = 0
        vertex.bone_indices = [0; 4];
        vertex.bone_weights = [1.0, 0.0, 0.0, 0.0];
    }
    let mesh = builder.upload(&vertices, &indices);
    let entity = spawn_object(world, mesh, neutral, base, Quat::IDENTITY, lambert([0.8; 3]), "con_proxy");
    let bone = world.spawn();
    world.insert(bone, Transform::new(base, Quat::IDENTITY, 1.0));
    world.insert(bone, GlobalTransform::new(base, Quat::IDENTITY, 1.0));
    name_entity(world, bone, PROXY_BONE);
    world.insert(
        entity,
        SkinnedMesh {
            skeleton_root: Some(bone),
            bones: vec![Some(bone)],
            bind_inverses: vec![byroredux_core::math::Mat4::IDENTITY],
            global_skin_transform: byroredux_core::math::Mat4::IDENTITY,
        },
    );
    world.insert(entity, RenderLayer::Actor);
    let key = |time: f32, yaw: f32| RotationKey { time, value: Quat::from_rotation_y(yaw), tbc: None };
    let channel = TransformChannel {
        translation_keys: Vec::new(),
        translation_type: KeyType::Linear,
        rotation_keys: vec![key(0.0, 0.0), key(1.0, 0.6), key(2.0, 0.0)],
        rotation_type: KeyType::Linear,
        scale_keys: Vec::new(),
        scale_type: KeyType::Linear,
        priority: 0,
    };
    let bone_name = world.resource_mut::<byroredux_core::string::StringPool>().intern(PROXY_BONE);
    let mut channels = rustc_hash::FxHashMap::default();
    channels.insert(bone_name, channel);
    let handle = world.resource_mut::<AnimationClipRegistry>().add(AnimationClip {
        name: "m-light proxy sway".into(),
        duration: 2.0,
        cycle_type: CycleType::Loop,
        frequency: 1.0,
        phase: 0.0,
        weight: 1.0,
        accum_root_name: None,
        channels,
        float_channels: Vec::new(),
        color_channels: Vec::new(),
        bool_channels: Vec::new(),
        texture_flip_channels: Vec::new(),
        text_keys: Vec::new(),
        root_yaw: Vec::new(),
    });
    world.insert(bone, AnimationPlayer::new(handle).with_root(bone));
}

/// Bind `normal_map` as `entity`'s tangent-space normal map (RLT-EXP-002).
fn insert_normal_map(world: &mut World, entity: byroredux_core::ecs::EntityId, normal_map: u32) {
    world.insert(
        entity,
        crate::components::MaterialTextureHandles {
            textures: byroredux_nif::import::MaterialTextureSet {
                normal: normal_map,
                ..Default::default()
            },
            normal_has_alpha: false,
            tint_has_alpha: false,
            parallax_height_scale: byroredux_core::ecs::components::material::DEFAULT_PARALLAX_HEIGHT_SCALE,
            parallax_max_passes: byroredux_core::ecs::components::material::DEFAULT_PARALLAX_MAX_PASSES,
        },
    );
}

/// RGBA8 cut-out for the alpha-tested panel: white, with a vertical and a
/// horizontal slot (alpha 0) through the centre, symmetric in u.
pub(super) fn cutout_pixels(size: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    let slot = |i: u32| {
        let t = (i as f32 + 0.5) / size as f32;
        (t - 0.5).abs() < 0.08
    };
    for y in 0..size {
        for x in 0..size {
            let hole = slot(x) || slot(y);
            pixels.extend_from_slice(&[255, 255, 255, if hole { 0 } else { 255 }]);
        }
    }
    pixels
}

/// Build the rung. Returns the camera pose (position, target).
pub(crate) fn setup_reference_scene(
    world: &mut World,
    ctx: &mut VulkanContext,
    rung: ReferenceRung,
    view: u32,
) -> (Vec3, Vec3) {
    // RLT-REF-009: no ambient, no directional, interior (no sky probe, no
    // DALC), fog far beyond the scene and no medium. `bare` fits a medium
    // from even an out-of-range legacy ramp, which the froxel integral then
    // applies as a ~0.4 %/m extinction, so the rung disables it outright.
    world.insert_resource(CellLightingRes {
        directional_fade: Some(1.0),
        fog_medium: crate::fog::FogMedium::DISABLED,
        ..CellLightingRes::bare(
            [0.0; 3],
            [0.0; 3],
            [0.0, 1.0, 0.0],
            true,
            [0.0; 3],
            100_000.0,
            1_000_000.0,
        )
    });
    world.insert_resource(RungMeasurements::default());
    let neutral = TextureHandle(ctx.texture_registry.neutral_fallback());
    let mut builder = MeshBuilder::new(ctx);
    let camera = match rung {
        ReferenceRung::CalibrationPoint => {
            let plane = builder.box_mesh([2.0 * M, BOX_WALL_T, 2.0 * M]);
            spawn_object(
                world,
                plane,
                neutral,
                Vec3::new(0.0, -BOX_WALL_T, 0.0),
                Quat::IDENTITY,
                lambert([0.8; 3]),
                "cal_plane",
            );
            spawn_reference_emitter(world, Vec3::new(0.0, 1.5 * M, 0.0), 2.0, "cal_emitter");
            (Vec3::new(0.0, 2.6 * M, 2.6 * M), Vec3::ZERO)
        }
        ReferenceRung::CalibrationSun => {
            let plane = builder.box_mesh([2.0 * M, BOX_WALL_T, 2.0 * M]);
            spawn_object(world, plane, neutral, Vec3::new(0.0, -BOX_WALL_T, 0.0), Quat::IDENTITY,
                lambert([0.8; 3]), "cal_plane");
            insert_reference_sun(world, Vec3::from_array(CALIBRATION_SUN_DIR));
            (Vec3::new(0.0, 2.6 * M, 2.6 * M), Vec3::ZERO)
        }
        ReferenceRung::SkyEnvironment => {
            insert_reference_sky(world, false, false);
            world.insert_resource(HarnessCameraFov(std::f32::consts::FRAC_PI_2));
            // `cornell_reference_view` refuses a view past the six faces (#7387): no clamp here.
            let face = Vec3::from_array(SKY_ENV_FACES[view as usize]);
            (Vec3::ZERO, face * M)
        }
        ReferenceRung::SkyProbe => {
            insert_reference_sky(world, true, false);
            let probe = builder.sphere(SKY_PROBE_RADIUS_M * M);
            spawn_object(world, probe, neutral, Vec3::ZERO, Quat::IDENTITY,
                sky_probe_material(view), "sky_probe");
            world.insert_resource(RungMeasurements {
                analytic_sky: true,
                ..RungMeasurements::default()
            });
            (Vec3::new(0.0, 0.0, SKY_PROBE_CAMERA_M * M), Vec3::ZERO)
        }
        ReferenceRung::SkySunPlane => {
            insert_reference_sky(world, true, true);
            let plane = builder.box_mesh([2.0 * M, BOX_WALL_T, 2.0 * M]);
            spawn_object(world, plane, neutral, Vec3::new(0.0, -BOX_WALL_T, 0.0), Quat::IDENTITY,
                lambert([0.8; 3]), "sun_plane");
            world.insert_resource(RungMeasurements {
                analytic_sky: true,
                ..RungMeasurements::default()
            });
            (Vec3::new(0.0, 2.6 * M, 2.6 * M), Vec3::ZERO)
        }
        ReferenceRung::CalibrationSky => {
            let plane = builder.box_mesh([2.0 * M, BOX_WALL_T, 2.0 * M]);
            spawn_object(
                world,
                plane,
                neutral,
                Vec3::new(0.0, -BOX_WALL_T, 0.0),
                Quat::IDENTITY,
                lambert([1.0; 3]),
                "cal_plane",
            );
            insert_reference_sun(world, Vec3::Y);
            {
                let mut sky = world.resource_mut::<crate::components::SkyParamsRes>();
                let color = [0.35, 0.5, 0.75];
                sky.zenith_color = color;
                sky.horizon_color = color;
                sky.lower_color = color;
                sky.sun_intensity = 0.0;
                sky.weather.cloud_coverage = 0.0;
                sky.weather.sun_glare = 0.0;
                sky.weather.moon_glare = 0.0;
                sky.weather.stars_color = [0.0; 3];
            }
            let sky = crate::render::sky::build_sky_params(world);
            world.insert_resource(RungMeasurements {
                constant_sky: Some(sky.zenith_color),
                ..RungMeasurements::default()
            });
            (Vec3::new(0.0, 2.6 * M, 2.6 * M), Vec3::ZERO)
        }
        ReferenceRung::LambertBox | ReferenceRung::ReservoirClamp => {
            let h = BOX_HALF_W;
            let floor = builder.box_mesh([h, BOX_WALL_T, h]);
            let side = builder.box_mesh([BOX_WALL_T, BOX_HEIGHT * 0.5, h]);
            let back = builder.box_mesh([h, BOX_HEIGHT * 0.5, BOX_WALL_T]);
            let tall = builder.box_mesh([0.4 * M, 0.9 * M, 0.4 * M]);
            let short = builder.box_mesh([0.4 * M, 0.4 * M, 0.4 * M]);
            let white = [0.73; 3];
            let walls = [
                (floor, Vec3::new(0.0, -BOX_WALL_T, 0.0), white, "box_floor"),
                (floor, Vec3::new(0.0, BOX_HEIGHT + BOX_WALL_T, 0.0), white, "box_ceiling"),
                (back, Vec3::new(0.0, BOX_HEIGHT * 0.5, -h - BOX_WALL_T), white, "box_back"),
                (side, Vec3::new(-h - BOX_WALL_T, BOX_HEIGHT * 0.5, 0.0), [0.65, 0.05, 0.05], "box_left_red"),
                (side, Vec3::new(h + BOX_WALL_T, BOX_HEIGHT * 0.5, 0.0), [0.12, 0.45, 0.15], "box_right_green"),
            ];
            for (mesh, pos, color, name) in walls {
                spawn_object(world, mesh, neutral, pos, Quat::IDENTITY, lambert(color), name);
            }
            spawn_object(
                world,
                tall,
                neutral,
                Vec3::new(-0.6 * M, 0.9 * M, -0.5 * M),
                Quat::from_rotation_y(0.3),
                lambert(white),
                "box_tall_block",
            );
            spawn_object(
                world,
                short,
                neutral,
                Vec3::new(0.7 * M, 0.4 * M, 0.4 * M),
                Quat::from_rotation_y(-0.3),
                lambert(white),
                "box_short_cube",
            );
            spawn_reference_emitter(
                world,
                Vec3::new(0.0, BOX_HEIGHT - 0.3 * M, 0.0),
                3.0,
                "box_emitter",
            );
            if rung == ReferenceRung::ReservoirClamp {
                for i in 0..6 {
                    let angle = i as f32 * std::f32::consts::TAU / 6.0;
                    // 1.5 m out: clear of both blocks and 0.5 m inside the walls.
                    let pos = Vec3::new(1.5 * angle.cos() * M, 1.2 * M, 1.5 * angle.sin() * M);
                    spawn_reference_emitter(world, pos, 0.05, &format!("box_dim_emitter_{i}"));
                }
            }
            if view == 1 {
                // Close-up of the tall block's base, ~1.1 m away: SSAO's
                // screen radius shrinks with distance (16 px / (d / 100 u)),
                // so only a camera this close sees it act on the contact and
                // the floor-wall junction (A8, RLT-GI-004).
                (
                    Vec3::new(0.1 * M, 0.6 * M, 0.4 * M),
                    Vec3::new(-0.6 * M, 0.1 * M, -0.5 * M),
                )
            } else {
                (
                    Vec3::new(0.0, BOX_HEIGHT * 0.5, h + 3.4 * M),
                    Vec3::new(0.0, BOX_HEIGHT * 0.5, 0.0),
                )
            }
        }
        ReferenceRung::Penumbra | ReferenceRung::PenumbraSun | ReferenceRung::PenumbraMoving => {
            let floor = builder.box_mesh([1.5 * M, BOX_WALL_T, 1.5 * M]);
            spawn_object(world, floor, neutral, Vec3::new(0.0, -BOX_WALL_T, 0.0), Quat::IDENTITY,
                lambert([0.8; 3]), "pen_floor");
            let plate = builder.box_mesh([PLATE_HALF_M * M, 0.01 * M, PLATE_HALF_M * M]);
            spawn_object(world, plate, neutral, Vec3::new(0.0, PLATE_HEIGHT_M * M, 0.0), Quat::IDENTITY,
                lambert([0.8; 3]), "pen_plate");
            let (edge_x, camera) = if rung != ReferenceRung::PenumbraSun {
                let rod = builder.box_mesh([0.005 * M, 0.005 * M, 0.3 * M]);
                spawn_object(world, rod, neutral, Vec3::new(0.45 * M, PLATE_HEIGHT_M * M, 0.0),
                    Quat::IDENTITY, lambert([0.8; 3]), "pen_rod");
                spawn_reference_emitter_sized(world, Vec3::new(0.0, PENUMBRA_LIGHT_HEIGHT_M * M, 0.0), 3.0,
                    PENUMBRA_SOURCE_RADIUS_M, "pen_emitter");
                // Similar triangles: the plate edge's shadow centre on the floor.
                let edge = PLATE_HALF_M * PENUMBRA_LIGHT_HEIGHT_M / (PENUMBRA_LIGHT_HEIGHT_M - PLATE_HEIGHT_M);
                (edge, (Vec3::new(edge * M, 0.41 * M, 0.12 * M), Vec3::new(edge * M, 0.0, 0.0)))
            } else {
                let dir = Vec3::from_array(PENUMBRA_SUN_DIR).normalize();
                insert_reference_sun(world, dir);
                let edge = PLATE_HALF_M - PLATE_HEIGHT_M * dir.x / dir.y;
                // 0.2 m above the floor, a 45° field 1280 px wide spans
                // ~0.31 m: the physical sun's ~3.2 mm 10-90 % width is ~13 px.
                (edge, (Vec3::new(edge * M, 0.2 * M, 0.06 * M), Vec3::new(edge * M, 0.0, 0.0)))
            };
            let reach = if rung != ReferenceRung::PenumbraSun { 0.12 } else { 0.03 };
            let profile_lines = [-0.4f32, 0.0, 0.4]
                .iter()
                .map(|f| {
                    let z = f * reach;
                    ProfileLine {
                        name: format!("edge_z{:+.0}mm", z * 1000.0),
                        receiver: "pen_floor".into(),
                        start: Vec3::new((edge_x - reach) * M, 0.0, z * M),
                        end: Vec3::new((edge_x + reach) * M, 0.0, z * M),
                        half_width: 0.25 * reach * M,
                    }
                })
                .collect();
            let mut regions = Vec::new();
            let mut casters = vec!["pen_plate".to_string()];
            if rung != ReferenceRung::PenumbraSun {
                // The rod's shadow centre, by similar triangles, and its
                // ~0.1 m penumbra either side.
                let x = 0.45 * PENUMBRA_LIGHT_HEIGHT_M / (PENUMBRA_LIGHT_HEIGHT_M - PLATE_HEIGHT_M);
                regions.push(MeasuredRegion {
                    name: "rod_shadow".into(),
                    receiver: "pen_floor".into(),
                    min: Vec3::new((x - 0.06) * M, -1.0, -0.1 * M),
                    max: Vec3::new((x + 0.06) * M, 1.0, 0.1 * M),
                });
                casters.push("pen_rod".into());
            }
            if rung == ReferenceRung::PenumbraMoving {
                let cube = builder.box_mesh([0.05 * M, 0.05 * M, 0.05 * M]);
                let start = Vec3::new(0.1 * M, 1.0 * M, 0.0);
                let mover = spawn_object(world, cube, neutral, start, Quat::IDENTITY, lambert([0.8; 3]), "pen_mover");
                // Its shadow (x doubled at the floor) sweeps 0.2 -> 1.2 m,
                // across the plate's edge penumbra and the rod's shadow.
                world.insert(
                    mover,
                    crate::bench_camera::BenchMover { start, end: Vec3::new(0.6 * M, 1.0 * M, 0.0) },
                );
                casters.push("pen_mover".into());
            }
            world.insert_resource(RungMeasurements {
                profile_lines,
                regions,
                casters,
                step_form: "disk_segment",
                min_unshadowed: 0.05,
                ..Default::default()
            });
            camera
        }
        ReferenceRung::FarSun => {
            // Low sun rakes across a matte ground plane. View 1 pulls back
            // past the traced fade, and view 2 turns to put the caster out
            // of frame while keeping receivers in the scene.
            insert_reference_sun(world, Vec3::new(-1.0, 0.24, 0.15));
            let ground = builder.box_mesh([4500.0, 20.0, 3200.0]);
            spawn_object(world, ground, neutral, Vec3::new(0.0, -20.0, 0.0),
                Quat::IDENTITY, lambert([0.5; 3]), "far_sun_ground");
            let caster = builder.box_mesh([220.0, 450.0, 550.0]);
            spawn_object(world, caster, neutral, Vec3::new(-500.0, 450.0, 0.0),
                Quat::IDENTITY, lambert([0.7; 3]), "far_sun_caster");
            // Blue fill overlaps the sun shadow on the side opposite the
            // caster, making accidental attenuation of another light visible.
            let fill = world.spawn();
            let fill_pos = Vec3::new(1800.0, 750.0, 120.0);
            world.insert(fill, Transform::new(fill_pos, Quat::IDENTITY, 1.0));
            world.insert(fill, GlobalTransform::new(fill_pos, Quat::IDENTITY, 1.0));
            world.insert(fill, LightSource {
                emitter: Emitter::inverse_square_point(
                    RadiantIntensityRgb::new([0.0, 0.0, 160.0]),
                    Meters::new(80.0), Meters::new(0.0), VisibilityMask::NONE,
                ),
                ..Default::default()
            });
            name_entity(world, fill, "far_sun_fill");
            match view {
                0 => (Vec3::new(0.0, 2800.0, 6800.0), Vec3::ZERO),
                1 => (Vec3::new(0.0, 3800.0, 15500.0), Vec3::ZERO),
                _ => (Vec3::new(3000.0, 3800.0, 15500.0), Vec3::new(12500.0, 0.0, 0.0)),
            }
        }
        ReferenceRung::Contact => setup_contact_rung(world, &mut builder, neutral, true),
        ReferenceRung::ContactFlat => setup_contact_rung(world, &mut builder, neutral, false),
        ReferenceRung::ContactSky => setup_contact_sky_rung(world, &mut builder, neutral),
        ReferenceRung::NormalPlane => {
            let floor = builder.box_mesh([2.0 * M, BOX_WALL_T, 2.0 * M]);
            spawn_object(world, floor, neutral, Vec3::new(0.0, -BOX_WALL_T, 0.0), Quat::IDENTITY,
                lambert([0.0; 3]), "nm_floor_black");
            let plane = builder.box_mesh_with_uv_tangents([0.3 * M, 0.005 * M, 0.3 * M]);
            let entity = spawn_object(world, plane, neutral, Vec3::new(0.0, 0.01 * M, 0.0), Quat::IDENTITY,
                lambert([0.8; 3]), "nm_plane");
            let normal_map = builder.register_texture(16, 16, super::builders::wavy_normal_map_pixels(16));
            insert_normal_map(world, entity, normal_map);
            // 50° elevation, 1.5 m away, at azimuth 0°, 120° or 240°.
            let azimuth = view as f32 * std::f32::consts::TAU / 3.0;
            let (elevation, distance) = (50.0f32.to_radians(), 1.5);
            let dir = Vec3::new(
                elevation.cos() * azimuth.cos(),
                elevation.sin(),
                elevation.cos() * azimuth.sin(),
            );
            spawn_reference_emitter(world, dir * distance * M, 2.0, "nm_emitter");
            (Vec3::new(0.0, 1.2 * M, 1.1 * M), Vec3::ZERO)
        }
        ReferenceRung::MaterialsSpecular => {
            let floor = builder.box_mesh([3.0 * M, BOX_WALL_T, 2.0 * M]);
            spawn_object(world, floor, neutral, Vec3::new(0.0, -BOX_WALL_T, 0.0), Quat::IDENTITY,
                lambert([0.0; 3]), "mat_floor_black");
            let plane = builder.box_mesh([0.3 * M, 0.005 * M, 0.3 * M]);
            for (i, &roughness) in SKY_PROBE_ROUGHNESS.iter().enumerate() {
                let x = (i as f32 - 2.0) * 0.8 * M;
                let tag = (roughness * 100.0).round() as u32;
                spawn_object(world, plane, neutral, Vec3::new(x, 0.01 * M, -0.4 * M), Quat::IDENTITY,
                    pbr_bsdf([0.5; 3], 0.0, roughness), &format!("mat_spec_dielectric_{tag:03}"));
                spawn_object(world, plane, neutral, Vec3::new(x, 0.01 * M, 0.4 * M), Quat::IDENTITY,
                    pbr_bsdf([0.95; 3], 1.0, roughness), &format!("mat_spec_conductor_{tag:03}"));
            }
            spawn_reference_emitter(world, Vec3::new(0.0, 1.5 * M, 0.3 * M), 2.0, "mat_emitter");
            let (height, back) = match view {
                0 => (3.2 * M, 1.2 * M),
                1 => (2.0 * M, 3.0 * M),
                _ => (1.0 * M, 3.6 * M),
            };
            (Vec3::new(0.0, height, back), Vec3::ZERO)
        }
        ReferenceRung::Materials => {
            // Planes 0.6 m square, 0.8 m apart, 1 cm above a black floor
            // (zero albedo: no bounce reaches the planes, so direct-only and
            // total agree up to the planes' own interreflection, which is
            // none for coplanar planes).
            let floor = builder.box_mesh([3.0 * M, BOX_WALL_T, 2.0 * M]);
            spawn_object(
                world,
                floor,
                neutral,
                Vec3::new(0.0, -BOX_WALL_T, 0.0),
                Quat::IDENTITY,
                lambert([0.0; 3]),
                "mat_floor_black",
            );
            let plane = builder.box_mesh([0.3 * M, 0.005 * M, 0.3 * M]);
            let count = MATERIAL_PLANES.len() as f32;
            for (i, (name, color)) in MATERIAL_PLANES.iter().enumerate() {
                let x = (i as f32 - (count - 1.0) * 0.5) * 0.8 * M;
                spawn_object(
                    world,
                    plane,
                    neutral,
                    Vec3::new(x, 0.01 * M, 0.0),
                    Quat::IDENTITY,
                    lambert(*color),
                    name,
                );
            }
            spawn_reference_emitter(world, Vec3::new(0.0, 1.5 * M, 0.3 * M), 2.0, "mat_emitter");
            // Three view angles over the row: steep, oblique, grazing-ish.
            let (height, back) = match view {
                0 => (3.2 * M, 1.2 * M),
                1 => (2.0 * M, 3.0 * M),
                _ => (1.0 * M, 3.6 * M),
            };
            (Vec3::new(0.0, height, back), Vec3::ZERO)
        }
    };
    let (sources, textures) = builder.finish_recording_sources();
    world.insert_resource(CornellMeshSources(sources));
    world.insert_resource(super::mitsuba_export::CornellTextureSources(textures));
    camera
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_every_rung_and_rejects_typos() {
        assert_eq!(cornell_reference_rung(&args(&[])).unwrap(), None);
        for rung in RUNGS {
            assert_eq!(
                cornell_reference_rung(&args(&["--cornell-reference", rung.name()])).unwrap(),
                Some(*rung)
            );
        }
        assert!(cornell_reference_rung(&args(&["--cornell-reference"])).is_err());
        assert!(cornell_reference_rung(&args(&["--cornell-reference", "cal-pt"])).is_err());
    }

    /// #7387 — each rung takes exactly its own views. The parser used to accept 0..=10 for every
    /// rung, and an out-of-range view was clamped (sky-env re-captured face 5) or fell through to
    /// the rung's last view (views 3..=10 became view 2), so a mistyped gate invocation produced a
    /// plausible duplicate capture.
    #[test]
    fn view_selector_defaults_to_zero_and_rejects_a_view_past_the_rungs_own_count() {
        use ReferenceRung::*;
        let view = |rung, v: &str| cornell_reference_view(&args(&["--cornell-reference-view", v]), rung);
        assert_eq!(cornell_reference_view(&args(&[]), SkyEnvironment).unwrap(), 0, "default 0");
        for (rung, count) in [
            (SkyEnvironment, 6),
            (SkyProbe, 11),
            (Materials, 3),
            (MaterialsSpecular, 3),
            (NormalPlane, 3),
            (FarSun, 3),
            (LambertBox, 2),
            (ReservoirClamp, 2),
            (CalibrationPoint, 1),
            (Contact, 1),
            (ContactSky, 1),
        ] {
            assert_eq!(rung.view_count(), count, "{rung:?}");
            assert_eq!(view(rung, &(count - 1).to_string()).unwrap(), count - 1, "{rung:?} takes its last view");
            assert!(view(rung, &count.to_string()).is_err(), "{rung:?} refuses view {count}");
        }
        assert!(view(Materials, "x").is_err(), "a non-number is an error");
        // The six sky-env faces are the six views; nothing past them is reachable now.
        assert_eq!(SkyEnvironment.view_count() as usize, SKY_ENV_FACES.len());
    }

    #[test]
    fn lambert_material_carries_the_reference_flag_and_no_specular() {
        let m = lambert([0.5; 3]);
        assert_ne!(
            m.effect_shader_flags & byroredux_renderer::vulkan::material::material_flag::LAMBERTIAN_REFERENCE,
            0
        );
        assert_eq!((m.metalness, m.specular_strength, m.ior), (0.0, 0.0, 1.0));
    }

    #[allow(clippy::assertions_on_constants)] // a retuned SOURCE_RADIUS_M that breaks r/d <= 0.1 or the 1-unit floor must fail here
    #[test]
    fn source_radius_stays_below_a_tenth_of_the_nearest_receiver() {
        // Nearest measured surfaces: the box ceiling 0.3 m above the emitter,
        // the calibration plane 1.5 m below it (RLT-REF-002, r/d ≤ 0.1).
        assert!(SOURCE_RADIUS_M / 0.3 <= 0.1);
        assert!(SOURCE_RADIUS_M * M >= 1.0, "below the shader's 1-unit sampling floor");
    }
}
