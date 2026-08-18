//! Cornell-box test harness — a self-contained reference scene for
//! validating ray-traced materials and lighting without on-disk game
//! data. Activated with the `--cornell` CLI flag (handled in
//! [`crate::scene::setup_scene`]).
//!
//! # Two lighting variants
//!
//! `--cornell` is **interior / point-light only**: a closed box lit by a
//! ceiling panel + a camera-side fill, `CellLightingRes.directional_color`
//! zeroed and no `SkyParamsRes`. Every sun-driven path — directional
//! BRDF + RT sun shadows, the volumetric froxel sun injection, the
//! Effect_Lit sun shading, the composite sky — is therefore **inert**, and
//! a "sun looks wrong" regression bisected against it returns a false
//! all-clear (#1942).
//!
//! `--cornell-sun` is the exterior counterpart: same probe set, ceiling
//! removed, *all* local lights dropped, and the canonical procedural
//! exterior environment installed ([`procedural_fallback_cell_lighting`] +
//! [`procedural_fallback_sky`], the same constructors a plugin-less
//! exterior load uses) with a fixed [`SUN_DIR_RAW`]. The sun is then the only
//! light in the scene, so any sign flip / axis swap / dropped term in the
//! directional chain shows up as a moved or missing shadow rather than a
//! plausible-looking image. No `WeatherDataRes` is inserted, so
//! `weather_system` stays inert and the direction does not drift with TOD.
//!
//! The scene is the classic Cornell box (white floor/ceiling/back wall,
//! red left wall, green right wall, a ceiling area light) populated with
//! probe objects chosen to exercise specific RT behaviours:
//!
//!   * a tall matte block + a matte sphere — GI color bleeding, soft
//!     contact shadows;
//!   * a 5-sphere **roughness sweep** (metal) and a 5-sphere
//!     **metalness sweep** — GGX highlight shape, RT reflections, and
//!     the renderer's roughness reflection-gate;
//!   * a glass sphere + glass cube — `MATERIAL_KIND_GLASS` IOR
//!     refraction / transmission;
//!   * an emissive cube — emissive contribution to GI + bloom.
//!
//! Every probe carries a [`Name`] and a live-mutable [`Material`], so the
//! `mat.*` console commands (see [`crate::commands`]) can sweep material
//! parameters at runtime and watch the RT response — no rebuild needed.
//! All geometry uses a flat-white vertex color; surface color is driven
//! entirely through `Material::diffuse_color` so a single
//! `mat.set <id> color r g b` tweak fully recolors a probe.

use byroredux_core::ecs::{
    CombustionState, FogBounds, FogProfile, FogShape, FogSource, FogVolume, GlobalTransform,
    LightSource, Material, MeshHandle, TextureHandle, TotalTime, Transform, World,
};
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::StringPool;
use byroredux_nif::import::ImportedMaterial;
use byroredux_renderer::vulkan::GpuUploadCtx;
use byroredux_renderer::{
    box_vertices_colored, uv_sphere, RenderDebugMode, VulkanContext, MATERIAL_KIND_FIRE_REFRACTION,
    MATERIAL_KIND_GLASS,
};

use crate::components::{CellLightingRes, MaterialTextureHandles};
use crate::env_translate::{procedural_fallback_cell_lighting, procedural_fallback_sky};

/// Classic Cornell wall albedos (linear). Gamebryo colors are raw
/// monitor-space floats and must NOT be sRGB-decoded (see the
/// `feedback_color_space` memory), so these are used verbatim as
/// `Material::diffuse_color`.
const WHITE: [f32; 3] = [0.73, 0.73, 0.73];
const RED: [f32; 3] = [0.65, 0.05, 0.05];
const GREEN: [f32; 3] = [0.12, 0.45, 0.15];

/// Room half-extents (world units). The box spans `x,z ∈ [-HALF_W, HALF_W]`
/// and `y ∈ [0, HEIGHT]`; the front (`+Z`) is left open for the camera.
const HALF_W: f32 = 4.0;
const HEIGHT: f32 = 5.0;
/// Wall slab half-thickness.
const T: f32 = 0.05;

/// Vanilla Skyrim SE bronze-dragon display mesh used by the large glass-
/// material experiment. Unlike the actor NIF, this asset carries an authored
/// static pose and therefore does not depend on creature HKX pose conversion.
/// The game profile opens the numbered Skyrim mesh archives as siblings, so
/// the caller only needs `--game skyrim_se` rather than an archive path.
pub(crate) const SKYRIM_GLASS_DRAGON_NIF: &str = r"meshes\loadscreenart\loadscreenbronzedragon.nif";

// Start with the actor-dragon envelope; the capture validation below makes the
// authored display mesh's actual framing visible before this scene becomes a
// fixture. This is intentionally separate from the compact correctness box:
// the oracle stays synthetic and redistributable, while this experiment
// exercises a real Bethesda asset and material import.
const DRAGON_ROOM_HALF_X: f32 = 1_100.0;
const DRAGON_ROOM_HALF_Z: f32 = 1_200.0;
const DRAGON_ROOM_HEIGHT: f32 = 650.0;
const DRAGON_ROOM_T: f32 = 4.0;
const DRAGON_FLOOR_LIFT: f32 = 10.0;
const DRAGON_PRESENTATION_YAW: f32 = 35.0_f32.to_radians();
const DRAGON_GLASS_TINT: [f32; 3] = [0.72, 0.88, 1.0];
const DRAGON_GLASS_ROUGHNESS: f32 = 0.04;

/// Un-normalised sun direction for `--cornell-sun`, in the engine's
/// canonical convention: the vector points **from the scene toward the
/// sun**, in Y-up world space. That is what `systems::weather`'s
/// `compute_sun_arc` produces (`y = sin(arc angle)`, positive while the
/// sun is up) and what `env_translate` copies verbatim into *both*
/// `CellLightingRes::directional_dir` and `SkyParamsRes::sun_direction`.
///
/// Deliberately asymmetric with three distinct component magnitudes and
/// all-positive signs: the tall block's floor shadow then falls toward
/// `-X/-Z` at an angle no axis swap or sign flip reproduces, so a
/// convention regression relocates it visibly instead of yielding a
/// different-but-plausible image. Normalised at use — `SkyParams`
/// consumers (`draw.rs`, `water.frag`'s caustic synthesis) assume a unit
/// vector.
const SUN_DIR_RAW: Vec3 = Vec3::new(0.6, 0.84, 0.4);

/// First hardware-independent rungs of the renderer correctness ladder.
///
/// These are deliberately separate from the material-showcase variants above:
/// every rung adds exactly one variable, so a failed capture names the first
/// broken contract instead of producing another plausible-looking Cornell
/// image with several possible causes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CornellOracleRung {
    L0,
    L1,
    L2,
    L3,
    L4,
}

/// Data contract shared by scene construction, analytic tests, and capture
/// tooling. Later rungs can extend this table without adding another
/// constructor.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CornellOracleManifest {
    pub name: &'static str,
    pub directional_radiance: [f32; 3],
    /// World-space unit vector from the surface toward the source.
    pub direction_toward_source: [f32; 3],
    pub blocker: bool,
    /// Local point-light + fog-volume transport is active. L3 is the open
    /// control and L4 changes only by adding an opaque partition.
    pub volumetric_probe: bool,
    pub camera_position: Vec3,
    pub camera_target: Vec3,
    pub primary_debug_view: &'static str,
    pub max_linear_error: f32,
}

// Normalized (1, 1, 2). The receiver faces +Z; the asymmetric source makes
// L2's hard shadow visible below-left of the blocker while retaining a
// hand-derivable N.L.
const ORACLE_LIGHT_DIRECTION: [f32; 3] = [0.408_248_3, 0.408_248_3, 0.816_496_6];
const ORACLE_CAMERA_POSITION: Vec3 = Vec3::new(0.0, 4.0, 10.0);
const ORACLE_CAMERA_TARGET: Vec3 = Vec3::new(0.0, 4.0, 0.0);
/// The surface-shadow oracle is intentionally unit-scale, but volumetrics use
/// Bethesda-world distances (70 units/metre) and a first froxel slab roughly
/// 44 units deep. Scaling L3/L4 gives the medium multiple depth samples while
/// preserving exactly the same projected scene and optical depth.
const ORACLE_VOLUMETRIC_SCALE: f32 = 100.0;

pub(crate) fn cornell_oracle_manifest(rung: CornellOracleRung) -> CornellOracleManifest {
    let (name, directional_radiance, blocker, volumetric_probe, primary_debug_view) = match rung {
        CornellOracleRung::L0 => ("l0_dark_plane", [0.0; 3], false, false, "direct"),
        CornellOracleRung::L1 => ("l1_directional_lambert", [1.0; 3], false, false, "direct"),
        CornellOracleRung::L2 => (
            "l2_opaque_blocker",
            [1.0; 3],
            true,
            false,
            "shadow_visibility",
        ),
        CornellOracleRung::L3 => ("l3_point_fog_open", [0.0; 3], false, true, "composite_term"),
        CornellOracleRung::L4 => (
            "l4_point_fog_partition",
            [0.0; 3],
            true,
            true,
            "composite_term",
        ),
    };
    CornellOracleManifest {
        name,
        directional_radiance,
        direction_toward_source: ORACLE_LIGHT_DIRECTION,
        blocker,
        volumetric_probe,
        camera_position: ORACLE_CAMERA_POSITION
            * if volumetric_probe {
                ORACLE_VOLUMETRIC_SCALE
            } else {
                1.0
            },
        camera_target: ORACLE_CAMERA_TARGET
            * if volumetric_probe {
                ORACLE_VOLUMETRIC_SCALE
            } else {
                1.0
            },
        primary_debug_view,
        // Linear-light probe tolerance. Image-level thresholds remain owned by
        // the capture runner rather than being hidden in this scene builder.
        max_linear_error: 0.015,
    }
}

/// Parse `--cornell-oracle l0|l1|l2|l3|l4` without silently falling back to
/// the demo scene on a typo. Later rungs intentionally remain errors until
/// their full scene and assertions exist.
pub(crate) fn cornell_oracle_rung(args: &[String]) -> Result<Option<CornellOracleRung>, String> {
    let Some(index) = args.iter().position(|arg| arg == "--cornell-oracle") else {
        return Ok(None);
    };
    let value = args
        .get(index + 1)
        .ok_or_else(|| "--cornell-oracle requires one of: l0, l1, l2, l3, l4".to_string())?;
    let rung = match value.to_ascii_lowercase().as_str() {
        "l0" => CornellOracleRung::L0,
        "l1" => CornellOracleRung::L1,
        "l2" => CornellOracleRung::L2,
        "l3" => CornellOracleRung::L3,
        "l4" => CornellOracleRung::L4,
        _ => {
            return Err(format!(
                "unknown Cornell oracle rung '{value}'; expected one of: l0, l1, l2, l3, l4"
            ));
        }
    };
    Ok(Some(rung))
}

/// Parse the diagnostic world translation applied to every Cornell oracle
/// object and its fixed camera. This keeps the analytic scene identical while
/// exercising camera-relative rendering and absolute ray-query coordinates.
pub(crate) fn cornell_oracle_world_offset(args: &[String]) -> Result<Vec3, String> {
    let Some(index) = args
        .iter()
        .position(|arg| arg == "--cornell-oracle-world-offset")
    else {
        return Ok(Vec3::ZERO);
    };
    let value = args.get(index + 1).ok_or_else(|| {
        "--cornell-oracle-world-offset requires finite comma-separated x,y,z".to_string()
    })?;
    let parts: Vec<_> = value.split(',').collect();
    if parts.len() != 3 {
        return Err(format!(
            "--cornell-oracle-world-offset requires finite comma-separated x,y,z, got '{value}'"
        ));
    }
    let mut coordinates = [0.0; 3];
    for (slot, part) in coordinates.iter_mut().zip(parts) {
        *slot = part.parse::<f32>().map_err(|_| {
            format!(
                "--cornell-oracle-world-offset requires finite comma-separated x,y,z, got '{value}'"
            )
        })?;
        if !slot.is_finite() {
            return Err(format!(
                "--cornell-oracle-world-offset requires finite comma-separated x,y,z, got '{value}'"
            ));
        }
    }
    Ok(Vec3::from_array(coordinates))
}

impl CornellOracleManifest {
    /// Expected legacy-Lambert direct term on the +Z receiver. Oracle materials
    /// set IOR=1 and specular strength=0, so Fresnel and specular are exactly
    /// absent and the clustered-light arm reduces to albedo * Li * N.L.
    pub(crate) fn expected_unshadowed_direct(self, albedo: [f32; 3]) -> [f32; 3] {
        let n_dot_l = self.direction_toward_source[2].max(0.0);
        [
            albedo[0] * self.directional_radiance[0] * n_dot_l,
            albedo[1] * self.directional_radiance[1] * n_dot_l,
            albedo[2] * self.directional_radiance[2] * n_dot_l,
        ]
    }
}

/// Unit-length [`SUN_DIR_RAW`].
fn sun_dir() -> [f32; 3] {
    SUN_DIR_RAW.normalize().to_array()
}

/// Construct the controlled L0-L4 correctness scene selected by
/// `--cornell-oracle`. The richer `--cornell` showcase remains untouched.
pub(crate) fn setup_cornell_oracle_scene(
    world: &mut World,
    ctx: &mut VulkanContext,
    rung: CornellOracleRung,
    world_offset: Vec3,
) -> (Vec3, Vec3) {
    let manifest = cornell_oracle_manifest(rung);
    if manifest.volumetric_probe {
        // Include the volumetric integral but bypass presentation exposure,
        // grading and stochastic dither. The capture then remains a direct
        // HDR-linear transport oracle.
        ctx.set_render_debug_mode(RenderDebugMode::CompositeTerm);
    }
    let expected_unshadowed = manifest.expected_unshadowed_direct([1.0; 3]);
    world.insert_resource(CellLightingRes {
        ambient: [0.0; 3],
        directional_color: manifest.directional_radiance,
        directional_dir: manifest.direction_toward_source,
        is_interior: true,
        fog_color: [0.0; 3],
        fog_near: 100_000.0,
        fog_far: 1_000_000.0,
        fog_medium: crate::fog::FogMedium::from_legacy_ramp(100_000.0, 1_000_000.0, None),
        // Preserve the manifest's radiance exactly instead of applying the
        // legacy 0.6 XCLL fallback calibration.
        directional_fade: Some(1.0),
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

    let neutral = TextureHandle(ctx.texture_registry.neutral_fallback());
    let mut builder = MeshBuilder::new(ctx);
    let oracle_scale = if manifest.volumetric_probe {
        ORACLE_VOLUMETRIC_SCALE
    } else {
        1.0
    };
    let receiver_mesh = builder.box_mesh([4.0, 4.0, 0.05].map(|v| v * oracle_scale));
    // L3/L4 use a black surface so the final capture contains only
    // in-scattered volumetric radiance; direct and indirect surface terms
    // cannot masquerade as a fog visibility result.
    let receiver_color = if manifest.volumetric_probe {
        [0.0; 3]
    } else {
        [1.0; 3]
    };
    let mut oracle_matte = matte(receiver_color);
    oracle_matte.ior = 1.0;
    oracle_matte.specular_strength = 0.0;
    spawn_object(
        world,
        receiver_mesh,
        neutral,
        Vec3::new(0.0, 4.0, -0.05) * oracle_scale + world_offset,
        Quat::IDENTITY,
        oracle_matte.clone(),
        "oracle_receiver",
    );

    if manifest.blocker && !manifest.volumetric_probe {
        let blocker_mesh = builder.box_mesh([0.75, 0.75, 0.75]);
        spawn_object(
            world,
            blocker_mesh,
            neutral,
            Vec3::new(0.0, 4.0, 0.75) + world_offset,
            Quat::IDENTITY,
            oracle_matte.clone(),
            "oracle_blocker",
        );
    }

    if manifest.volumetric_probe {
        // The local medium fills the camera-to-receiver segment. L3 is the
        // open control. L4 adds one thin, edge-on partition at x=0: points on
        // its left must be shadowed from the right-side point light while the
        // right half remains an unchanged lit control. Because the partition
        // is edge-on to the camera, its only substantial image-space effect is
        // the visibility boundary in the fog rather than a broad foreground
        // surface.
        spawn_fog_volume_with_extinction(
            world,
            Vec3::new(0.0, 4.0, 5.0) * oracle_scale + world_offset,
            Vec3::new(3.5, 3.5, 4.0) * oracle_scale,
            40.0 / oracle_scale,
            "oracle_fog_volume",
        );
        spawn_point_light(
            world,
            Vec3::new(2.5, 4.0, 5.0) * oracle_scale + world_offset,
            20.0 * oracle_scale,
            [2.0; 3],
            "oracle_point_light",
        );

        if manifest.blocker {
            // Half a native unit thick after scaling: enough for a robust
            // ray-query hit, but narrow enough in screen space that a failed
            // XY reconstruction cannot hide its halo inside a broad surface.
            let partition_mesh = builder.box_mesh([0.005, 4.0, 4.0].map(|v| v * oracle_scale));
            spawn_object(
                world,
                partition_mesh,
                neutral,
                Vec3::new(0.0, 4.0, 4.0) * oracle_scale + world_offset,
                Quat::IDENTITY,
                oracle_matte,
                "oracle_opaque_partition",
            );
        }
    }
    builder.finish();

    log::info!(
        "Cornell oracle {} ready: blocker={}, volumetric={}, debug={}, world_offset={:?}, \
         expected unshadowed direct={:?}, linear tolerance={:.4}",
        manifest.name,
        manifest.blocker,
        manifest.volumetric_probe,
        manifest.primary_debug_view,
        world_offset,
        expected_unshadowed,
        manifest.max_linear_error,
    );
    (
        manifest.camera_position + world_offset,
        manifest.camera_target + world_offset,
    )
}

/// Whether the separate native-scale Skyrim glass-dragon experiment was
/// selected. Exact matching keeps it independent from `--cornell`,
/// `--cornell-sun`, and the deterministic oracle ladder.
pub(crate) fn glass_dragon_mode(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--cornell-glass-dragon")
}

/// Replace one imported dragon submesh's authored skin material with a smooth
/// dielectric while retaining only its normal map. Clearing base color, glow,
/// masks, decals, and alpha-test coverage is load-bearing: this experiment is
/// meant to shade the entire silhouette as glass, not blend blue over the
/// original dragon skin. The alpha-blend *pipeline* remains required even for
/// thick glass: it preserves the receiver when the bounded RT path falls back,
/// carries the live instance index in mesh-ID bit 31, and therefore admits the
/// surface to `caustic_splat.comp`.
fn force_glass_dragon_material(source: &mut ImportedMaterial) {
    let normal = source.textures.normal;
    let model_space_normals = source.model_space_normals;
    let texture_clamp_mode = source.texture_clamp_mode;

    let mut glass = ImportedMaterial::default();
    glass.textures.normal = normal;
    glass.model_space_normals = model_space_normals;
    glass.texture_clamp_mode = texture_clamp_mode;
    glass.diffuse_color = DRAGON_GLASS_TINT;
    glass.ambient_color = DRAGON_GLASS_TINT;
    glass.specular_color = [1.0; 3];
    glass.specular_strength = 1.0;
    glass.mat_alpha = 0.25;
    glass.has_alpha = true;
    glass.material_kind = MATERIAL_KIND_GLASS;
    glass.metalness_override = Some(0.0);
    glass.roughness_override = Some(DRAGON_GLASS_ROUGHNESS);
    // The experiment itself is the authoritative producer of these values.
    // This prevents normal-alpha legacy fallback from replacing the forced
    // smoothness after MaterialTextureHandles have been resolved.
    glass.bgsm_pbr_scalars_authored = true;
    *source = glass;
}

fn force_glass_dragon_scene(imported: &mut byroredux_nif::import::ImportedScene) {
    for mesh in &mut imported.meshes {
        force_glass_dragon_material(&mut mesh.material);
    }
}

fn place_glass_dragon(transform: &mut Transform) {
    transform.translation.y += DRAGON_FLOOR_LIFT;
    transform.rotation = Quat::from_rotation_y(DRAGON_PRESENTATION_YAW) * transform.rotation;
}

/// Build the larger Cornell shell, load Skyrim's authored static dragon
/// display through the regular BSA/NIF path, and force every imported submesh
/// onto the canonical refractive-glass ray-query path. The asset stays at native
/// Skyrim scale.
pub(crate) fn setup_cornell_glass_dragon_scene(
    world: &mut World,
    ctx: &mut VulkanContext,
    args: &[String],
) -> Result<(Vec3, Vec3), String> {
    let tex_provider = crate::asset_provider::build_texture_provider(args);
    let dragon_bytes = tex_provider
        .extract_mesh(SKYRIM_GLASS_DRAGON_NIF)
        .ok_or_else(|| {
            format!(
                "'{SKYRIM_GLASS_DRAGON_NIF}' was not found in the configured mesh archives; \
                 launch with `--game skyrim_se --cornell-glass-dragon` or provide Skyrim's \
                 mesh archives via --bsa"
            )
        })?;
    let mut mat_provider = crate::asset_provider::build_material_provider(args);

    let camera = setup_cornell_glass_dragon_room(world, ctx);
    let mut override_hook = force_glass_dragon_scene;
    let (entity_count, root, _) = crate::scene::load_nif_bytes_with_skeleton(
        world,
        ctx,
        &dragon_bytes,
        SKYRIM_GLASS_DRAGON_NIF,
        &tex_provider,
        Some(&mut mat_provider),
        None,
        None,
        Some(&mut override_hook),
    );
    let root = root.ok_or_else(|| {
        format!("Skyrim dragon parsed but spawned no root entity from '{SKYRIM_GLASS_DRAGON_NIF}'")
    })?;

    // Lift the authored static display pose slightly above the receiver.
    let mut transforms = world
        .query_mut::<Transform>()
        .ok_or_else(|| "Transform storage unavailable after dragon spawn".to_string())?;
    let root_transform = transforms
        .get_mut(root)
        .ok_or_else(|| format!("dragon root entity {root} has no Transform"))?;
    place_glass_dragon(root_transform);
    drop(transforms);

    log::info!(
        "Large Cornell glass dragon ready: {} spawned entities from '{}', root={}, \
         tint={:?}, roughness={:.3}",
        entity_count,
        SKYRIM_GLASS_DRAGON_NIF,
        root,
        DRAGON_GLASS_TINT,
        DRAGON_GLASS_ROUGHNESS,
    );
    Ok(camera)
}

fn setup_cornell_glass_dragon_room(world: &mut World, ctx: &mut VulkanContext) -> (Vec3, Vec3) {
    install_cornell_lighting(world, false);
    let neutral = TextureHandle(ctx.texture_registry.neutral_fallback());
    let mut builder = MeshBuilder::new(ctx);

    let cy = DRAGON_ROOM_HEIGHT * 0.5;
    let horizontal = builder.box_mesh([DRAGON_ROOM_HALF_X, DRAGON_ROOM_T, DRAGON_ROOM_HALF_Z]);
    let back = builder.box_mesh([DRAGON_ROOM_HALF_X, cy, DRAGON_ROOM_T]);
    let side = builder.box_mesh([DRAGON_ROOM_T, cy, DRAGON_ROOM_HALF_Z]);
    let walls: &[(MeshHandle, Vec3, [f32; 3], &str)] = &[
        (
            horizontal,
            Vec3::new(0.0, -DRAGON_ROOM_T, 0.0),
            WHITE,
            "dragon_room_floor",
        ),
        (
            horizontal,
            Vec3::new(0.0, DRAGON_ROOM_HEIGHT + DRAGON_ROOM_T, 0.0),
            WHITE,
            "dragon_room_ceiling",
        ),
        (
            back,
            Vec3::new(0.0, cy, -DRAGON_ROOM_HALF_Z - DRAGON_ROOM_T),
            WHITE,
            "dragon_room_back",
        ),
        (
            side,
            Vec3::new(-DRAGON_ROOM_HALF_X - DRAGON_ROOM_T, cy, 0.0),
            RED,
            "dragon_room_left_red",
        ),
        (
            side,
            Vec3::new(DRAGON_ROOM_HALF_X + DRAGON_ROOM_T, cy, 0.0),
            GREEN,
            "dragon_room_right_green",
        ),
    ];
    for &(mesh, position, color, name) in walls {
        spawn_object(
            world,
            mesh,
            neutral,
            position,
            Quat::IDENTITY,
            matte(color),
            name,
        );
    }

    // A broad visible emitter anchors the ceiling reflection. Keep its
    // radiance below the bloom/exposure clipping range: at 10× it occupied a
    // large solid angle in close views and turned physically-small Fresnel
    // highlights into broad white patches, hiding the refraction being tested.
    // The point light beneath it supplies direct illumination; two lower fills
    // make the silhouette and red/green refraction legible without a sun path.
    let panel = builder.box_mesh([260.0, 3.0, 220.0]);
    spawn_object(
        world,
        panel,
        neutral,
        Vec3::new(0.0, DRAGON_ROOM_HEIGHT - 5.0, -100.0),
        Quat::IDENTITY,
        emissive([1.0, 0.97, 0.9], 3.0),
        "dragon_room_ceiling_panel",
    );
    spawn_point_light(
        world,
        Vec3::new(0.0, DRAGON_ROOM_HEIGHT - 45.0, -100.0),
        4_500.0,
        [2.6, 2.5, 2.35],
        "dragon_room_key",
    );
    spawn_point_light(
        world,
        Vec3::new(650.0, 360.0, 1_050.0),
        3_200.0,
        [0.75, 0.9, 1.25],
        "dragon_room_front_fill",
    );
    spawn_point_light(
        world,
        Vec3::new(-650.0, 280.0, -850.0),
        2_800.0,
        [1.2, 0.45, 0.25],
        "dragon_room_back_rim",
    );
    builder.finish();

    // Stay just outside the open +Z wall, close enough that the authored
    // silhouette—not the empty room—is the subject of the experiment.
    let target = Vec3::new(0.0, 220.0, 0.0);
    let position = Vec3::new(0.0, 300.0, 1_700.0);
    (position, target)
}

/// Build the Cornell box into `world`, uploading all meshes + BLAS through
/// `ctx`. Returns `(camera_position, camera_target)` so the caller can
/// place the fly-camera looking into the open front of the box.
///
/// `sun` selects the exterior variant (`--cornell-sun`): see the module
/// header for what changes and why (#1942).
pub(crate) fn setup_cornell_scene(
    world: &mut World,
    ctx: &mut VulkanContext,
    sun: bool,
) -> (Vec3, Vec3) {
    install_cornell_lighting(world, sun);
    let combustion_probe = std::env::var_os("BYRO_COMBUSTION_PROBE").is_some();

    // Every probe is untextured by design — surface color comes entirely
    // from `Material::diffuse_color`. Bind the registry's white 1×1
    // neutral fallback (handle 1) so the shader's `albedo *= texColor`
    // multiply yields the authored color. Without an explicit
    // `TextureHandle` the draw loop would default to handle 0 — the
    // magenta/checker "missing texture" diagnostic — and every surface
    // would render as a tinted checkerboard. (See `asset_provider`'s F2
    // path: the NIF / cell loaders route textureless materials here too.)
    let neutral = TextureHandle(ctx.texture_registry.neutral_fallback());

    let mut builder = MeshBuilder::new(ctx);

    // ── Room shell ──────────────────────────────────────────────────
    // Walls are thin slabs; from inside the room the inner face is
    // front-facing (normal points into the room) so back-face culling
    // keeps the outer faces hidden. Color is driven by Material, so the
    // slab geometry is flat-white.
    let cy = HEIGHT * 0.5;
    let h_slab = builder.box_mesh([HALF_W, T, HALF_W]); // floor / ceiling
    let back_slab = builder.box_mesh([HALF_W, cy, T]);
    let side_slab = builder.box_mesh([T, cy, HALF_W]); // left / right

    let walls: &[(MeshHandle, Vec3, [f32; 3], &str)] = &[
        (h_slab, Vec3::new(0.0, -T, 0.0), WHITE, "floor"),
        // Skipped in sun mode — a lid would block every sun ray and
        // reduce the bisection scene to ambient.
        (h_slab, Vec3::new(0.0, HEIGHT + T, 0.0), WHITE, "ceiling"),
        (
            back_slab,
            Vec3::new(0.0, cy, -HALF_W - T),
            WHITE,
            "back_wall",
        ),
        (
            side_slab,
            Vec3::new(-HALF_W - T, cy, 0.0),
            RED,
            "left_wall_red",
        ),
        (
            side_slab,
            Vec3::new(HALF_W + T, cy, 0.0),
            GREEN,
            "right_wall_green",
        ),
    ];
    for &(mesh, pos, color, name) in walls {
        if sun && name == "ceiling" {
            continue;
        }
        spawn_object(
            world,
            mesh,
            neutral,
            pos,
            Quat::IDENTITY,
            matte(color),
            name,
        );
    }

    // ── Local fog volume probe ──────────────────────────────────────
    // #2248 (REN-D21-01) — unlike the global `CellLightingRes::fog_medium`
    // ramp below (deliberately pushed out of range to match a real
    // no-authored-fog interior cell, #1942's sibling trap for the sun
    // path), a local `FogVolume` is an explicitly-placed authored effect
    // — a designer-placed smoke/mist pocket, independent of whether the
    // cell has ambient atmospheric fog at all. Spawned in both variants:
    // local fog isn't sun-driven, so it belongs outside the `!sun` gate.
    // Extinction is authored directly in "per meter" terms and converted
    // to per-world-unit by the same `WORLD_UNITS_PER_METER` divide the
    // real import path uses (`render/fog_volumes.rs`), so a value that
    // reads as "thick smoke" at Bethesda scale also reads as thick smoke
    // here — the box's few-world-unit span is what makes it visible in a
    // handful of units instead of dozens of metres.
    spawn_fog_volume(
        world,
        Vec3::new(-1.6, 1.6, -0.4),
        Vec3::new(1.3, 1.3, 1.3),
        "fog_volume_probe",
    );
    if combustion_probe {
        spawn_combustion_probe(world, Vec3::new(0.0, 1.35, -0.4));
    }

    // ── Local lights (interior variant only) ────────────────────────
    // In sun mode every one of these is skipped: a bisection harness for
    // the directional path must not have a second light source that can
    // mask a dead or misdirected sun. What survives is the emissive cube
    // probe below, which is emissive-GI, not a `LightSource`.
    if !sun {
        // ── Ceiling area light ──────────────────────────────────────
        // An emissive panel (the visible light) plus a point LightSource
        // just below it (the actual direct illumination — emissive-only
        // GI is a known weak spot this harness is meant to expose).
        let light_panel = builder.box_mesh([1.2, 0.02, 1.2]);
        spawn_object(
            world,
            light_panel,
            neutral,
            Vec3::new(0.0, HEIGHT - 0.03, 0.0),
            Quat::IDENTITY,
            emissive([1.0, 0.97, 0.9], 8.0),
            "ceiling_light_panel",
        );
        spawn_point_light(
            world,
            Vec3::new(0.0, HEIGHT - 0.3, 0.0),
            30.0,
            [1.6, 1.55, 1.45],
            "ceiling_light",
        );

        // ── Camera-side key/fill light ──────────────────────────────
        // The ceiling light alone sits *behind* the front probe rows, so
        // their camera-facing hemispheres fall into near-shadow and no
        // material differences are visible. This second light, placed
        // high and off to one side near the camera, rakes the
        // camera-facing sides — giving each probe a GGX highlight whose
        // shape/size reveals roughness, and an albedo-tinted (vs white)
        // specular that reveals metalness. Dimmer than the key so the
        // Cornell colour-bleed look survives. (Whether GI *alone* should
        // fill these faces is a separate question tracked for a later
        // pass.)
        spawn_point_light(
            world,
            Vec3::new(2.0, HEIGHT * 0.8, HALF_W + 1.0),
            40.0,
            [1.1, 1.1, 1.15],
            "camera_fill_light",
        );
    }

    // ── Classic probes: tall matte block + matte sphere ─────────────
    let tall = builder.box_mesh([0.7, 1.5, 0.7]);
    spawn_object(
        world,
        tall,
        neutral,
        Vec3::new(-1.5, 1.5, -1.6),
        Quat::from_rotation_y(-0.3),
        matte(WHITE),
        "tall_block",
    );
    let big_sphere = builder.sphere(0.9);
    spawn_object(
        world,
        big_sphere,
        neutral,
        Vec3::new(1.6, 0.9, -1.2),
        Quat::IDENTITY,
        matte(WHITE),
        "matte_sphere",
    );

    // ── Material sweeps ─────────────────────────────────────────────
    // Two front rows of small spheres. Row near z=+1.5 sweeps roughness
    // at metalness=1.0 (GGX lobe + RT reflection across the gate); row at
    // z=+2.9 sweeps metalness at a fixed *moderate* roughness.
    //
    // The metalness row's roughness is deliberately 0.35, not mirror-
    // smooth: at low roughness both ends of a metalness sweep are
    // dominated by a sharp environment reflection, so dielectric (m=0)
    // and metal (m=1) look near-identical in a dim room — verified live
    // via `mat.set`. At 0.35 the dielectric end shows its diffuse albedo
    // while the metal end shows an albedo-tinted glossy reflection, so
    // the transition actually reads. Sweep either row at runtime with
    // `mat.set <id> roughness <v>` to probe other points.
    let probe = builder.sphere(0.45);
    let xs = [-3.0_f32, -1.5, 0.0, 1.5, 3.0];
    for (i, &x) in xs.iter().enumerate() {
        let r = 0.02 + 0.96 * (i as f32 / (xs.len() - 1) as f32);
        spawn_object(
            world,
            probe,
            neutral,
            Vec3::new(x, 0.45, 1.5),
            Quat::IDENTITY,
            pbr([0.95, 0.95, 0.95], 1.0, r),
            &format!("metal_rough_{i}"),
        );
    }
    for (i, &x) in xs.iter().enumerate() {
        let m = i as f32 / (xs.len() - 1) as f32;
        spawn_object(
            world,
            probe,
            neutral,
            Vec3::new(x, 0.45, 2.9),
            Quat::IDENTITY,
            pbr([0.9, 0.85, 0.55], m, 0.35),
            &format!("metalness_{i}"),
        );
    }
    // #2477 / REN-D21-2026-08-07-01 — same metalness sweep, one row
    // further back, but with `MAT_FLAG_PBR_BSDF` set so it renders
    // through `disneyDiffuseSplit` instead of legacy Lambert. Side by
    // side with `metalness_*` above, the two rows should read as
    // subtly different (Burley vs. Lambert diffuse falloff) rather
    // than identical — identical would mean the Disney branch is
    // silently not engaging. `mat.set <id> material_flags <bits>`
    // clears/re-sets the bit live for direct comparison.
    for (i, &x) in xs.iter().enumerate() {
        let m = i as f32 / (xs.len() - 1) as f32;
        spawn_object(
            world,
            probe,
            neutral,
            Vec3::new(x, 0.45, 4.3),
            Quat::IDENTITY,
            pbr_bsdf([0.9, 0.85, 0.55], m, 0.35),
            &format!("metalness_bsdf_{i}"),
        );
    }
    // #2514 / REN-D21-2026-08-07-02 — one row further back, sweeping
    // subsurface/sheen/sheen_tint/anisotropic together from 0 → 1 at a
    // fixed moderate metalness/roughness (same 0.35 as the row above, for
    // the same "diffuse end doesn't get swamped by a sharp reflection"
    // reason). Before this probe, no entity the harness could produce
    // ever drove these four scalars off `Material::default()`'s zero —
    // `disneyDiffuseSplit` always degenerated back to Burley-only even
    // with `MAT_FLAG_PBR_BSDF` set. Sweep with `mat.set <id> subsurface
    // <v>` / `sheen <v>` / `sheen_tint <v>` / `anisotropic <v>` to isolate
    // one lobe at a time.
    for (i, &x) in xs.iter().enumerate() {
        let t = i as f32 / (xs.len() - 1) as f32;
        spawn_object(
            world,
            probe,
            neutral,
            Vec3::new(x, 0.45, 5.7),
            Quat::IDENTITY,
            pbr_bsdf_lobes([0.9, 0.85, 0.55], 0.5, 0.35, t, t, t, t),
            &format!("bsdf_lobes_{i}"),
        );
    }

    // ── Glass probes ────────────────────────────────────────────────
    // Glass is OPAQUE (no AlphaBlend): the IOR refraction ray IS the
    // transmission — it samples the
    // scene behind and writes it in place of the background, so the bent /
    // refracted world is what you see THROUGH the glass. An alpha-blend
    // window would instead composite the *undistorted* background over the
    // glass and dilute the refraction to invisibility. The old budget /
    // jitter stipple that motivated alpha-blend is fixed (IOR budget,
    // smooth-glass deterministic refraction, deterministic metal refl).
    // Front-centre hero so its wide IOR refraction captures the colourful
    // room behind it (red/green walls, ceiling light, floor) and shows the
    // inverted/magnified scene — the classic glass-ball refraction demo.
    // Against the flat white back wall (where it sat before) the bend is
    // invisible; here the two-surface refraction reads clearly.
    let glass_sphere = builder.sphere(0.95);
    spawn_object(
        world,
        glass_sphere,
        neutral,
        Vec3::new(0.0, 1.05, 2.4),
        Quat::IDENTITY,
        glass([0.9, 0.95, 1.0]),
        "glass_sphere",
    );
    let glass_cube = builder.box_mesh([0.6, 0.6, 0.6]);
    spawn_object(
        world,
        glass_cube,
        neutral,
        Vec3::new(-2.6, 0.6, 0.6),
        Quat::from_rotation_y(0.4),
        glass([1.0, 0.95, 0.9]),
        "glass_cube",
    );

    // ── Fire-refraction probe ────────────────────────────────────────
    // #2249 (REN-D21-03) — `MATERIAL_KIND_FIRE_REFRACTION` had no Cornell
    // coverage: `mat.set` couldn't reach `ior` (the field's distortion-
    // strength overload — now fixed in `commands/scene.rs`) and no probe
    // carried a normal map, so `tangentWarp = N - macroN * dot(N, macroN)`
    // was structurally zero even at max authored strength. This probe
    // exercises both halves together.
    let fire_normal_map = synthesize_wavy_normal_map(builder.ctx);
    let fire_cube = builder.box_mesh([0.6, 0.9, 0.6]);
    let fire_entity = spawn_object(
        world,
        fire_cube,
        neutral,
        Vec3::new(2.6, 0.9, 0.6),
        Quat::from_rotation_y(-0.4),
        fire_refraction(0.6),
        "fire_refraction_probe",
    );
    world.insert(
        fire_entity,
        MaterialTextureHandles {
            textures: byroredux_nif::import::MaterialTextureSet {
                normal: fire_normal_map,
                ..Default::default()
            },
            normal_has_alpha: false,
            parallax_height_scale: 0.04,
            parallax_max_passes: 4.0,
        },
    );

    // ── Emissive probe ──────────────────────────────────────────────
    let emit_cube = builder.box_mesh([0.35, 0.35, 0.35]);
    spawn_object(
        world,
        emit_cube,
        neutral,
        Vec3::new(0.2, 0.35, 0.4),
        Quat::from_rotation_y(0.6),
        emissive([1.0, 0.4, 0.1], 4.0),
        "emissive_cube",
    );

    builder.finish();

    log::info!(
        "Cornell box ready ({}): {} entities. Tweak materials live via `mat.list` / \
         `mat.set <id> <field> <value>` over byro-dbg.",
        if sun {
            "exterior / sun-only"
        } else {
            "interior / point-light"
        },
        world.next_entity_id()
    );

    // Camera: stand outside the open front, slightly above mid-height,
    // looking at the room center.
    let target = Vec3::new(0.0, HEIGHT * 0.45, 0.0);
    let pos = Vec3::new(0.0, HEIGHT * 0.55, HALF_W + 6.0);
    (pos, target)
}

/// Install the environment resources for the selected variant.
///
/// Split out of [`setup_cornell_scene`] because it is the whole point of
/// #1942 and the only part testable without a Vulkan device: it decides
/// whether the renderer's sun paths are driven at all.
///
/// Interior (`sun == false`) keeps the classic look — near-black ambient
/// so the ceiling panel dominates, directional zeroed, fog pushed out of
/// range, and *no* `SkyParamsRes` (so `build_sky_params` returns the
/// all-default `SkyParams` and the composite pass skips the sky).
///
/// Exterior (`sun == true`) reuses the canonical plugin-less exterior
/// constructors rather than hand-rolling a second set of literals, so the
/// harness drifts with the real fallback path instead of away from it.
pub(crate) fn install_cornell_lighting(world: &mut World, sun: bool) {
    if sun {
        world.insert_resource(procedural_fallback_cell_lighting(sun_dir()));
        world.insert_resource(procedural_fallback_sky(sun_dir()));
        return;
    }
    world.insert_resource(CellLightingRes {
        ambient: [0.03, 0.03, 0.03],
        directional_color: [0.0, 0.0, 0.0],
        directional_dir: [0.0, -1.0, 0.0],
        is_interior: true,
        fog_color: [0.0, 0.0, 0.0],
        fog_near: 100_000.0,
        fog_far: 1_000_000.0,
        fog_medium: crate::fog::FogMedium::from_legacy_ramp(100_000.0, 1_000_000.0, None),
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
}

/// Matte dielectric — the diffuse Cornell surface.
fn matte(color: [f32; 3]) -> Material {
    Material {
        diffuse_color: color,
        roughness: 0.95,
        metalness: 0.0,
        ..Default::default()
    }
}

/// Explicit PBR probe with caller-chosen metalness/roughness.
///
/// #2477 / REN-D21-2026-08-07-01 — this constructor, like every other
/// probe in the harness, leaves `effect_shader_flags` at
/// `Material::default()`'s `0`, so `MAT_FLAG_PBR_BSDF` is clear and the
/// shared direct-lighting BRDF (`include/lighting.glsl`,
/// `triangle.frag`) takes the legacy Lambert diffuse branch, not the
/// Disney (`disneyDiffuseSplit`) branch every BGSM/BGEM-sourced game
/// surface takes. See [`pbr_bsdf`] for the Disney-branch sibling.
fn pbr(color: [f32; 3], metalness: f32, roughness: f32) -> Material {
    Material {
        diffuse_color: color,
        metalness,
        roughness,
        ..Default::default()
    }
}

/// Disney-BSDF sibling of [`pbr`] — sets `MAT_FLAG_PBR_BSDF` so the
/// shared direct-lighting BRDF takes the `disneyDiffuseSplit` branch
/// instead of legacy Lambert, matching every real BGSM/BGEM-sourced
/// surface (#1352 sets this bit for all `is_pbr` content). Without a
/// probe on this branch, a regression isolated to `disneyDiffuseSplit`
/// (or its sheen/subsurface lobe) bisects clean against Cornell and only
/// reproduces in-game — the false-all-clear failure mode #1942 fixed for
/// the sun path (#2477 / REN-D21-2026-08-07-01).
fn pbr_bsdf(color: [f32; 3], metalness: f32, roughness: f32) -> Material {
    let mut material = pbr(color, metalness, roughness);
    material.effect_shader_flags |= byroredux_renderer::vulkan::material::material_flag::PBR_BSDF;
    material
}

/// [`pbr_bsdf`] sibling that also drives the four Disney lobe scalars no
/// source format authors — `subsurface`/`sheen`/`sheen_tint`/
/// `anisotropic` — so a probe on this constructor actually exercises
/// `disneyDiffuseSplit`'s distinguishing parameters instead of running it
/// with all three pinned at zero (degenerating back toward Burley-only).
/// #2514 / REN-D21-2026-08-07-02 — the enabling half of the
/// REN-D21-2026-08-07-01 gap: even with `MAT_FLAG_PBR_BSDF` set, no CPU
/// producer could reach these fields before this constructor + the
/// matching `mat.set` arms.
fn pbr_bsdf_lobes(
    color: [f32; 3],
    metalness: f32,
    roughness: f32,
    subsurface: f32,
    sheen: f32,
    sheen_tint: f32,
    anisotropic: f32,
) -> Material {
    let mut material = pbr_bsdf(color, metalness, roughness);
    material.subsurface = subsurface;
    material.sheen = sheen;
    material.sheen_tint = sheen_tint;
    material.anisotropic = anisotropic;
    material
}

/// `MATERIAL_KIND_GLASS` probe — forces the glass-smooth roughness so the
/// IOR refraction path engages (the gate keys on
/// `materialKind == MATERIAL_KIND_GLASS && roughness < 0.35`, not `alpha`),
/// matching the spawn-time `classify_glass_into_material` contract.
/// `alpha: 0.25` below sets `finalAlpha` for these probes to ~0.25 (not
/// 1.0). It is unconsumed by the *composite/TAA passes* specifically
/// (`taa.comp`/`composite.frag` don't read it), and latent-fragile if a
/// future composite branch keys on alpha for glass/decal classification.
/// See #676 / DEN-6.
///
/// It is NOT inert engine-wide (#2515): the value reaches
/// `GpuMaterial.material_alpha` through `to_gpu_material` and is hashed by
/// `hash_gpu_material_fields` (`material.rs` writes
/// `mat.material_alpha.to_bits()`), which `MaterialTable::intern_by_hash`
/// keys on. So it is part of the material dedup identity, and changing it
/// splits or merges material-table slots — these glass probes already
/// occupy a slot distinct from an otherwise identical opaque dielectric
/// purely because of it. Relevant to anyone measuring dedup ratio via
/// `ctx.scratch` (#780 / PERF-N1) against the Cornell scene.
fn glass(color: [f32; 3]) -> Material {
    let mut material = Material {
        diffuse_color: color,
        material_kind: MATERIAL_KIND_GLASS,
        alpha: 0.25,
        ..Default::default()
    };
    material
        .apply_surface_behavior(byroredux_core::ecs::components::material::GLASS_SURFACE_BEHAVIOR);
    material
}

/// Self-illuminated probe. `mult` scales `emissive_color`.
fn emissive(color: [f32; 3], mult: f32) -> Material {
    use byroredux_core::ecs::components::material::EmissiveSource;
    Material {
        diffuse_color: color,
        emissive_color: color,
        emissive_mult: mult,
        emissive_source: EmissiveSource::Material,
        roughness: 0.9,
        ..Default::default()
    }
}

/// `MATERIAL_KIND_FIRE_REFRACTION` probe (#2249 / REN-D21-03). This kind
/// overloads `Material::ior` as the authored distortion strength — see
/// `triangle.frag`'s fire-refraction branch, `clamp(mat.ior, 0.0, 1.0)` —
/// not a real refractive index. The caller must also attach a
/// `MaterialTextureHandles` with a non-zero `textures.normal`
/// ([`synthesize_wavy_normal_map`]): without one, `N == macroN` at every
/// fragment and `tangentWarp = N - macroN * dot(N, macroN)` is
/// structurally zero regardless of this value.
fn fire_refraction(distortion_strength: f32) -> Material {
    Material {
        diffuse_color: [1.0, 1.0, 1.0],
        material_kind: MATERIAL_KIND_FIRE_REFRACTION,
        ior: distortion_strength,
        ..Default::default()
    }
}

/// Synthesize a small tangent-space normal map with a spatially-varying
/// wave pattern (#2249 / REN-D21-03). Cornell has no on-disk textures, so
/// without this the fire-refraction probe's normal map slot stays at the
/// bindless-0 "absent" sentinel and its distortion path is a structural
/// no-op (see [`fire_refraction`]). Flat/neutral (`(0,0,1)` everywhere)
/// would compile and shade but still leave `tangentWarp` zero since
/// `N == macroN`; the wave pattern guarantees genuine per-fragment
/// disagreement between the two.
fn synthesize_wavy_normal_map(ctx: &mut VulkanContext) -> u32 {
    const SIZE: u32 = 16;
    let pixels = wavy_normal_map_pixels(SIZE);
    let alloc = ctx.allocator.as_ref().unwrap();
    let upload_ctx = GpuUploadCtx {
        device: &ctx.device,
        allocator: alloc,
        queue: &ctx.graphics_queue,
        command_pool: ctx.transfer_pool,
    };
    ctx.texture_registry
        .register_rgba(upload_ctx, SIZE, SIZE, &pixels)
        .expect("Cornell normal-map synth upload failed")
}

/// Pixel data for [`synthesize_wavy_normal_map`], split out so the pattern
/// itself is testable without a Vulkan device. RGBA8, tangent-space
/// encoding (`channel = component * 0.5 + 0.5`), a sine wave over both
/// axes so no two rows/columns share the same normal.
fn wavy_normal_map_pixels(size: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let u = x as f32 / size as f32;
            let v = y as f32 / size as f32;
            let nx = (u * std::f32::consts::TAU * 3.0).sin() * 0.6;
            let ny = (v * std::f32::consts::TAU * 3.0).sin() * 0.6;
            let nz = (1.0 - nx * nx - ny * ny).max(0.0).sqrt();
            pixels.push(((nx * 0.5 + 0.5) * 255.0) as u8);
            pixels.push(((ny * 0.5 + 0.5) * 255.0) as u8);
            pixels.push(((nz * 0.5 + 0.5) * 255.0) as u8);
            pixels.push(255u8);
        }
    }
    pixels
}

/// Spawn a renderable probe carrying `Transform`, `GlobalTransform`,
/// `MeshHandle`, `Material`, and `Name`. `GlobalTransform` is seeded to
/// match `Transform` so the first rendered frame is correct before
/// transform propagation runs.
fn spawn_object(
    world: &mut World,
    mesh: MeshHandle,
    tex: TextureHandle,
    pos: Vec3,
    rot: Quat,
    material: Material,
    name: &str,
) -> byroredux_core::ecs::EntityId {
    let e = world.spawn();
    world.insert(e, Transform::new(pos, rot, 1.0));
    world.insert(e, GlobalTransform::new(pos, rot, 1.0));
    world.insert(e, mesh);
    world.insert(e, tex);
    world.insert(e, material);
    name_entity(world, e, name);
    e
}

/// Spawn a named point [`LightSource`] at `pos`. `radius` is the
/// influence falloff distance; `color` is the (un-tonemapped, linear)
/// radiance.
fn spawn_point_light(world: &mut World, pos: Vec3, radius: f32, color: [f32; 3], name: &str) {
    let light = world.spawn();
    world.insert(light, Transform::new(pos, Quat::IDENTITY, 1.0));
    world.insert(light, GlobalTransform::new(pos, Quat::IDENTITY, 1.0));
    world.insert(
        light,
        LightSource::from_legacy_world_units(
            radius,
            color,
            byroredux_core::ecs::LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL,
            1.0,
            byroredux_core::ecs::LightKind::Point,
            [0.0; 3],
            0.0,
            byroredux_core::ecs::LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL,
        ),
    );
    name_entity(world, light, name);
}

/// Spawn a named local [`FogVolume`] probe, centered on the entity's own
/// `GlobalTransform` (`FogBounds::center` left at the origin). `pos` and
/// `half_extents` are world units, same convention as every mesh probe in
/// this file. `extinction_per_meter` / `single_scatter_albedo` are
/// authored exactly like a real content producer would — the collection
/// path (`render/fog_volumes.rs`) applies the same `WORLD_UNITS_PER_METER`
/// conversion either way, so there's no Cornell-specific scaling here.
fn spawn_fog_volume(world: &mut World, pos: Vec3, half_extents: Vec3, name: &str) {
    spawn_fog_volume_with_extinction(world, pos, half_extents, 40.0, name);
}

fn spawn_fog_volume_with_extinction(
    world: &mut World,
    pos: Vec3,
    half_extents: Vec3,
    extinction_per_meter: f32,
    name: &str,
) {
    let e = world.spawn();
    world.insert(e, Transform::new(pos, Quat::IDENTITY, 1.0));
    world.insert(e, GlobalTransform::new(pos, Quat::IDENTITY, 1.0));
    world.insert(
        e,
        FogVolume {
            bounds: Some(FogBounds {
                center: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                half_extents,
                shape: FogShape::Box,
            }),
            extinction_per_meter,
            single_scatter_albedo: [0.92, 0.92, 0.97],
            edge_softness: 0.35,
            profile: FogProfile::Homogeneous,
            emissive_radiance: [0.0; 3],
            emission_temperature_k: 0.0,
            source: FogSource::AuthoredMesh,
        },
    );
    name_entity(world, e, name);
}

/// Opt-in one-shot used to validate the complete explosion profile without
/// depending on game data. It starts two seconds after the first Cornell frame
/// so capture tooling can observe the hot core, expansion, and cooled shell.
fn spawn_combustion_probe(world: &mut World, pos: Vec3) {
    let e = world.spawn();
    world.insert(e, Transform::new(pos, Quat::IDENTITY, 1.0));
    world.insert(e, GlobalTransform::new(pos, Quat::IDENTITY, 1.0));
    let emissive_radiance =
        byroredux_core::radiometry::blackbody_radiance_srgb(2800.0, 1850.0, 24.0)
            .expect("the finite Cornell combustion probe temperature is representable");
    world.insert(
        e,
        FogVolume {
            bounds: Some(FogBounds {
                center: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                half_extents: Vec3::splat(1.55),
                shape: FogShape::Sphere,
            }),
            extinction_per_meter: 10.0,
            single_scatter_albedo: [0.12; 3],
            edge_softness: 0.3,
            profile: FogProfile::Explosion,
            emissive_radiance,
            emission_temperature_k: 2800.0,
            source: FogSource::RuntimeEffect,
        },
    );
    let now_seconds = { world.resource::<TotalTime>().0 };
    // Keep the opt-in probe slow enough for a debugger to capture its hot,
    // transitional, and smoke-dominant phases without changing production
    // particle lifetimes.
    world.insert(e, CombustionState::one_shot(now_seconds + 2.0, 8.0));
    name_entity(world, e, "combustion_explosion_probe");
}

fn name_entity(world: &mut World, entity: byroredux_core::ecs::EntityId, name: &str) {
    let interned = {
        let mut pool = world.resource_mut::<StringPool>();
        pool.intern(name)
    };
    world.insert(entity, byroredux_core::ecs::components::Name(interned));
}

/// Accumulates uploaded meshes so their BLAS can be built in one batch,
/// matching the demo-scene upload pattern in `scene::setup_scene`.
struct MeshBuilder<'a> {
    ctx: &'a mut VulkanContext,
    pending: Vec<(u32, u32, u32)>,
}

impl<'a> MeshBuilder<'a> {
    fn new(ctx: &'a mut VulkanContext) -> Self {
        Self {
            ctx,
            pending: Vec::new(),
        }
    }

    fn box_mesh(&mut self, half: [f32; 3]) -> MeshHandle {
        let (v, i) = box_vertices_colored(half, [1.0, 1.0, 1.0]);
        self.upload(&v, &i)
    }

    fn sphere(&mut self, radius: f32) -> MeshHandle {
        let (v, i) = uv_sphere(radius, [1.0, 1.0, 1.0], 96, 128);
        self.upload(&v, &i)
    }

    fn upload(&mut self, verts: &[byroredux_renderer::Vertex], idxs: &[u32]) -> MeshHandle {
        let alloc = self.ctx.allocator.as_ref().unwrap();
        let rt = self.ctx.device_caps.ray_query_supported;
        let upload_ctx = GpuUploadCtx {
            device: &self.ctx.device,
            allocator: alloc,
            queue: &self.ctx.graphics_queue,
            command_pool: self.ctx.transfer_pool,
        };
        // Cornell geometry participates in ordinary scene rendering, even
        // when a real NIF is loaded beside it. Register it in the global
        // geometry pool as well as retaining its per-mesh buffers for BLAS.
        // A per-mesh-only upload works while Cornell is the whole scene, but
        // becomes invalid as soon as a NIF enables the global multi-draw path:
        // that path reads every batch through global offsets.
        let handle = self
            .ctx
            .mesh_registry
            .upload_scene_mesh(upload_ctx, verts, idxs, rt, None)
            .expect("Cornell scene-mesh upload failed");
        self.pending
            .push((handle, verts.len() as u32, idxs.len() as u32));
        MeshHandle(handle)
    }

    /// Build BLAS for every uploaded mesh in one batched call.
    fn finish(self) {
        self.ctx.build_blas_batched(&self.pending);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::cornell_sun_mode;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn cornell_oracle_cli_names_only_complete_rungs() {
        assert_eq!(cornell_oracle_rung(&args(&[])).unwrap(), None);
        assert_eq!(
            cornell_oracle_rung(&args(&["--cornell-oracle", "l0"])).unwrap(),
            Some(CornellOracleRung::L0)
        );
        assert_eq!(
            cornell_oracle_rung(&args(&["--cornell-oracle", "L2"])).unwrap(),
            Some(CornellOracleRung::L2)
        );
        assert_eq!(
            cornell_oracle_rung(&args(&["--cornell-oracle", "l4"])).unwrap(),
            Some(CornellOracleRung::L4)
        );
        assert!(cornell_oracle_rung(&args(&["--cornell-oracle"])).is_err());
        assert!(cornell_oracle_rung(&args(&["--cornell-oracle", "l5"])).is_err());
    }

    #[test]
    fn cornell_oracle_world_offset_is_explicit_finite_and_three_dimensional() {
        assert_eq!(cornell_oracle_world_offset(&args(&[])).unwrap(), Vec3::ZERO);
        assert_eq!(
            cornell_oracle_world_offset(&args(&[
                "--cornell-oracle-world-offset",
                "1000000,0,-1000000",
            ]))
            .unwrap(),
            Vec3::new(1_000_000.0, 0.0, -1_000_000.0)
        );
        for invalid in ["1,2", "1,2,3,4", "1,NaN,3", "far,0,0"] {
            assert!(cornell_oracle_world_offset(&args(&[
                "--cornell-oracle-world-offset",
                invalid,
            ]))
            .is_err());
        }
        assert!(cornell_oracle_world_offset(&args(&["--cornell-oracle-world-offset"])).is_err());
    }

    #[test]
    fn glass_dragon_flag_is_a_distinct_exact_scene_mode() {
        assert!(!glass_dragon_mode(&args(&[])));
        assert!(!glass_dragon_mode(&args(&["--cornell"])));
        assert!(!glass_dragon_mode(&args(&["--cornell-sun"])));
        assert!(!glass_dragon_mode(&args(&["--cornell-glass-dragon-extra"])));
        assert!(glass_dragon_mode(&args(&[
            "--game",
            "skyrim_se",
            "--cornell-glass-dragon",
        ])));
    }

    #[test]
    fn glass_dragon_placement_lifts_and_presents_the_authored_static_pose() {
        let authored_rotation = Quat::from_rotation_x(0.2);
        let mut transform = Transform::new(Vec3::new(5.0, -9.0, 7.0), authored_rotation, 1.0);

        place_glass_dragon(&mut transform);

        assert_eq!(
            transform.translation,
            Vec3::new(5.0, -9.0 + DRAGON_FLOOR_LIFT, 7.0)
        );
        let expected = Quat::from_rotation_y(DRAGON_PRESENTATION_YAW) * authored_rotation;
        assert!(transform.rotation.dot(expected).abs() > 0.999_999);
    }

    #[test]
    fn glass_dragon_override_reaches_canonical_refractive_glass() {
        let mut pool = StringPool::new();
        let base = pool.intern("textures/actors/dragon/dragon.dds");
        let normal = pool.intern("textures/actors/dragon/dragon_n.dds");
        let mut imported = ImportedMaterial {
            has_alpha: true,
            alpha_test: true,
            is_decal: true,
            is_pbr: true,
            has_translucency: true,
            model_space_normals: true,
            material_kind: byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER,
            emissive_color: [1.0, 0.2, 0.1],
            emissive_mult: 8.0,
            ..Default::default()
        };
        imported.textures.base_color = Some(base);
        imported.textures.normal = Some(normal);

        force_glass_dragon_material(&mut imported);

        assert_eq!(imported.material_kind, MATERIAL_KIND_GLASS);
        assert!(
            imported.has_alpha,
            "refractive glass needs the blend pipeline for fallback coverage and caustic source identity"
        );
        assert_eq!(imported.src_blend_mode, 6);
        assert_eq!(imported.dst_blend_mode, 7);
        assert!(!imported.alpha_test);
        assert!(!imported.is_decal);
        assert_eq!(imported.textures.base_color, None);
        assert_eq!(imported.textures.normal, Some(normal));
        assert!(imported.model_space_normals);
        assert_eq!(imported.emissive_mult, 0.0);
        assert_eq!(imported.metalness_override, Some(0.0));
        assert_eq!(imported.roughness_override, Some(DRAGON_GLASS_ROUGHNESS));
        assert!(imported.bgsm_pbr_scalars_authored);

        let translated = crate::material_translate::translate_material(
            &imported,
            Some("Dragon:0"),
            crate::material_translate::ResolvedPaths {
                textures: Default::default(),
                material_path: None,
            },
            0,
        );
        assert_eq!(translated.material_kind, MATERIAL_KIND_GLASS);
        assert_eq!(translated.metalness, 0.0);
        assert_eq!(translated.roughness, DRAGON_GLASS_ROUGHNESS);
        assert!(
            translated.ior > 1.0,
            "glass must reach the refractive IOR path"
        );
    }

    #[test]
    fn cornell_oracle_l0_l2_add_exactly_light_then_blocker() {
        let l0 = cornell_oracle_manifest(CornellOracleRung::L0);
        let l1 = cornell_oracle_manifest(CornellOracleRung::L1);
        let l2 = cornell_oracle_manifest(CornellOracleRung::L2);

        assert_eq!(l0.directional_radiance, [0.0; 3]);
        assert!(!l0.blocker);
        assert_eq!(l1.directional_radiance, [1.0; 3]);
        assert!(!l1.blocker);
        assert_eq!(l2.directional_radiance, l1.directional_radiance);
        assert_eq!(l2.direction_toward_source, l1.direction_toward_source);
        assert!(l2.blocker);
        assert_eq!(l2.primary_debug_view, "shadow_visibility");
    }

    #[test]
    fn cornell_oracle_l3_l4_add_exactly_the_opaque_partition() {
        let l3 = cornell_oracle_manifest(CornellOracleRung::L3);
        let l4 = cornell_oracle_manifest(CornellOracleRung::L4);

        assert!(l3.volumetric_probe);
        assert!(l4.volumetric_probe);
        assert_eq!(l3.directional_radiance, [0.0; 3]);
        assert_eq!(l4.directional_radiance, l3.directional_radiance);
        assert!(!l3.blocker);
        assert!(l4.blocker);
        assert_eq!(l3.camera_position, l4.camera_position);
        assert_eq!(l3.camera_target, l4.camera_target);
        assert_eq!(l3.primary_debug_view, "composite_term");
        assert_eq!(l4.primary_debug_view, "composite_term");
    }

    #[test]
    fn cornell_oracle_lambert_expectation_is_analytic() {
        let l0 = cornell_oracle_manifest(CornellOracleRung::L0);
        let l1 = cornell_oracle_manifest(CornellOracleRung::L1);
        assert_eq!(l0.expected_unshadowed_direct([1.0; 3]), [0.0; 3]);

        let direction = Vec3::from_array(l1.direction_toward_source);
        assert!((direction.length() - 1.0).abs() < 1e-6);
        let expected = l1.expected_unshadowed_direct([1.0; 3]);
        for channel in expected {
            assert!((channel - direction.z).abs() < 1e-6);
        }
    }

    #[test]
    fn cornell_oracle_l2_probe_ray_crosses_the_declared_blocker() {
        let l2 = cornell_oracle_manifest(CornellOracleRung::L2);
        let direction = Vec3::from_array(l2.direction_toward_source);
        let at_mid_depth = 0.75 / direction.z;

        // The blocker is [-0.75, 0.75] in X, [3.25, 4.75] in Y, and
        // [0, 1.5] in Z. This receiver point reaches its centre at mid-depth
        // when traced toward the source, while the control remains outside.
        let shadow_probe = Vec3::new(-0.375, 3.625, 0.0);
        let inside = shadow_probe + direction * at_mid_depth;
        assert!(inside.x.abs() < 0.75 && (inside.y - 4.0).abs() < 0.75);

        let unshadowed_probe = Vec3::new(2.5, 6.5, 0.0);
        let outside = unshadowed_probe + direction * at_mid_depth;
        assert!(outside.x.abs() > 0.75 && (outside.y - 4.0).abs() > 0.75);
    }

    /// #1942 — `--cornell-sun` selects the exterior variant, plain
    /// `--cornell` the interior one, and neither flag falls through to
    /// the ESM / NIF / demo paths. Both flags together resolve to sun
    /// mode rather than to whichever the parser happened to test first.
    #[test]
    fn cornell_flag_selects_variant() {
        assert_eq!(cornell_sun_mode(&args(&[])), None);
        assert_eq!(cornell_sun_mode(&args(&["--esm", "Skyrim.esm"])), None);
        assert_eq!(cornell_sun_mode(&args(&["--cornell"])), Some(false));
        assert_eq!(cornell_sun_mode(&args(&["--cornell-sun"])), Some(true));
        assert_eq!(
            cornell_sun_mode(&args(&["--cornell", "--cornell-sun"])),
            Some(true),
            "asking for the sun variant at all means the sun paths are what's being bisected"
        );
        assert_eq!(
            cornell_sun_mode(&args(&["--cornellsun"])),
            None,
            "no prefix matching — an unknown flag must not silently enable the harness"
        );
    }

    /// The interior variant is what #1942 reported: the sun paths are
    /// inert because the directional term is zeroed and no
    /// `SkyParamsRes` exists, so `build_sky_params` hands the renderer
    /// the all-default `SkyParams` (`is_exterior = false`). Pinned so a
    /// future "just give Cornell a sun" edit can't quietly change the
    /// interior reference scene instead of using the new variant.
    #[test]
    fn interior_variant_leaves_the_sun_paths_inert() {
        let mut world = World::new();
        install_cornell_lighting(&mut world, false);

        let lit = world.resource::<CellLightingRes>();
        assert!(lit.is_interior);
        assert_eq!(lit.directional_color, [0.0, 0.0, 0.0]);
        drop(lit);
        assert!(
            world
                .try_resource::<crate::components::SkyParamsRes>()
                .is_none(),
            "no SkyParamsRes → no sky, no volumetric sun injection, no Effect_Lit sun"
        );
    }

    /// The exterior variant drives every sun path: a non-zero
    /// directional colour on an `is_interior = false` cell (so
    /// `compute_directional_upload` scales by the full
    /// `sun_intensity / SUN_INTENSITY_PEAK` instead of the 0.6 interior
    /// constant) plus an `is_exterior` `SkyParamsRes` carrying the same
    /// direction. Both resources must agree — `directional_dir` and
    /// `sun_direction` are separately consumed (`render::lights` vs
    /// `render::sky`), and a harness where they disagree would itself
    /// be a sun-direction bug. #1942.
    #[test]
    fn sun_variant_drives_directional_and_sky_paths() {
        use crate::components::SkyParamsRes;

        let mut world = World::new();
        install_cornell_lighting(&mut world, true);

        let expected = sun_dir();
        let len =
            (expected[0] * expected[0] + expected[1] * expected[1] + expected[2] * expected[2])
                .sqrt();
        assert!(
            (len - 1.0).abs() < 1e-5,
            "sun direction must be unit-length, got {len}"
        );
        assert!(
            expected[1] > 0.0,
            "engine convention: the vector points TOWARD the sun, so +Y while the sun is up"
        );

        let lit = world.resource::<CellLightingRes>();
        assert!(!lit.is_interior, "exterior → full sun-intensity scaling");
        assert_ne!(lit.directional_color, [0.0, 0.0, 0.0]);
        assert_eq!(lit.directional_dir, expected);
        drop(lit);

        let sky = world.resource::<SkyParamsRes>();
        assert!(
            sky.is_exterior,
            "gates the composite sky + froxel sun inject"
        );
        assert_eq!(
            sky.sun_direction, expected,
            "SkyParamsRes and CellLightingRes must carry the same direction"
        );
        assert!(sky.sun_intensity > 0.0);
    }

    /// Regression for #2248 (REN-D21-01): `--cornell` must carry a local
    /// `FogVolume` probe that produces genuinely measurable optical depth
    /// at the box's own world-unit scale, not the near-zero the global
    /// fog ramp rounds to over the box's few-unit span (`fog.rs`'s
    /// `fit_legacy_fog_extinction(100_000.0, 1_000_000.0, ...)` is fit for
    /// Bethesda-cell distances, not a ~4-8-unit room).
    #[test]
    fn fog_volume_probe_is_renderable_and_visible_at_cornell_scale() {
        let mut world = World::new();
        world.insert_resource(StringPool::new());
        spawn_fog_volume(
            &mut world,
            Vec3::new(-1.6, 1.6, -0.4),
            Vec3::new(1.3, 1.3, 1.3),
            "fog_volume_probe",
        );

        let volumes = world.query::<FogVolume>().expect("FogVolume storage");
        let transforms = world
            .query::<GlobalTransform>()
            .expect("GlobalTransform storage");
        let (entity, volume) = volumes
            .iter()
            .next()
            .expect("spawn_fog_volume must insert exactly one FogVolume");
        assert!(
            transforms.get(entity).is_some(),
            "a FogVolume needs a GlobalTransform for the collection query in render/fog_volumes.rs"
        );
        assert!(
            volume.is_renderable(),
            "probe must satisfy FogVolume::is_renderable (bounds set, finite positive extinction)"
        );

        // Mirror the CPU→GPU conversion in `render/fog_volumes.rs`
        // (`extinction_per_meter / WORLD_UNITS_PER_METER`) to check the
        // resulting per-world-unit density actually produces visible
        // attenuation across the volume's own extent, instead of the ~0
        // the Bethesda-cell-scale global ramp rounds to here.
        let bounds = volume.bounds.expect("checked by is_renderable above");
        let sigma_t_per_world_unit =
            volume.extinction_per_meter / crate::fog::WORLD_UNITS_PER_METER;
        let path_length = 2.0 * bounds.half_extents.min_element();
        let optical_depth = sigma_t_per_world_unit * path_length;
        assert!(
            optical_depth > 0.5,
            "optical depth {optical_depth} across the probe must be clearly visible \
             (>0.5), not rounding to ~0 like the global fog ramp does at this scale"
        );
    }

    /// Regression for #2249 (REN-D21-03): the Cornell fire-refraction
    /// probe's normal map must actually vary spatially. A flat/neutral
    /// normal map would still compile and shade, but `N == macroN` at
    /// every fragment makes `tangentWarp = N - macroN * dot(N, macroN)`
    /// structurally zero regardless of authored distortion strength — the
    /// same silent no-op the missing `mat.set ior` case left uncaught.
    #[test]
    fn wavy_normal_map_pixels_vary_and_stay_opaque() {
        let pixels = wavy_normal_map_pixels(16);
        assert_eq!(pixels.len(), 16 * 16 * 4);

        let first_rgb = [pixels[0], pixels[1], pixels[2]];
        let varies = pixels
            .chunks_exact(4)
            .any(|p| [p[0], p[1], p[2]] != first_rgb);
        assert!(
            varies,
            "normal map must vary spatially, not be uniformly flat/neutral"
        );
        assert!(
            pixels.chunks_exact(4).all(|p| p[3] == 255),
            "normal map must be fully opaque"
        );
    }

    /// The fire-refraction probe's `Material` must carry the material
    /// kind + a non-zero authored distortion strength (`ior`) — the
    /// half of #2249 that doesn't need a normal map to check.
    #[test]
    fn fire_refraction_material_carries_kind_and_distortion_strength() {
        let material = fire_refraction(0.6);
        assert_eq!(material.material_kind, MATERIAL_KIND_FIRE_REFRACTION);
        assert_eq!(material.ior, 0.6);
    }

    /// Regression for #2477 (REN-D21-2026-08-07-01): every OTHER Cornell
    /// material constructor leaves `effect_shader_flags` at
    /// `Material::default()`'s `0`, so `MAT_FLAG_PBR_BSDF` stays clear and
    /// the shared direct-lighting BRDF takes the legacy Lambert branch —
    /// never the Disney (`disneyDiffuseSplit`) branch every real
    /// BGSM/BGEM-sourced surface takes. `pbr_bsdf` must set the bit so at
    /// least one probe row exercises that branch.
    #[test]
    fn pbr_bsdf_material_sets_the_disney_bsdf_flag() {
        use byroredux_renderer::vulkan::material::material_flag::PBR_BSDF;

        // Every other constructor: flag clear (the pre-fix, still-correct
        // state for the legacy-Lambert probes).
        assert_eq!(matte(WHITE).effect_shader_flags & PBR_BSDF, 0);
        assert_eq!(
            pbr([0.9, 0.85, 0.55], 0.5, 0.35).effect_shader_flags & PBR_BSDF,
            0
        );

        // The Disney sibling must set it, and must otherwise match `pbr`'s
        // metalness/roughness/color plumbing exactly.
        let plain = pbr([0.9, 0.85, 0.55], 0.5, 0.35);
        let bsdf = pbr_bsdf([0.9, 0.85, 0.55], 0.5, 0.35);
        assert_ne!(
            bsdf.effect_shader_flags & PBR_BSDF,
            0,
            "pbr_bsdf must set MAT_FLAG_PBR_BSDF so the Cornell harness can \
             reach the Disney diffuse branch at all (#2477)"
        );
        assert_eq!(bsdf.metalness, plain.metalness);
        assert_eq!(bsdf.roughness, plain.roughness);
        assert_eq!(bsdf.diffuse_color, plain.diffuse_color);
    }

    /// Regression for #2514 (REN-D21-2026-08-07-02): every OTHER Cornell
    /// material constructor — including `pbr_bsdf` itself — leaves
    /// `subsurface`/`sheen`/`sheen_tint`/`anisotropic` at
    /// `Material::default()`'s zero, so `disneyDiffuseSplit` runs with all
    /// three distinguishing parameters pinned off even when
    /// `MAT_FLAG_PBR_BSDF` is set. `pbr_bsdf_lobes` must drive all four
    /// non-zero while still setting the flag and preserving `pbr_bsdf`'s
    /// metalness/roughness/color plumbing.
    #[test]
    fn pbr_bsdf_lobes_material_drives_all_four_disney_scalars() {
        use byroredux_renderer::vulkan::material::material_flag::PBR_BSDF;

        // Every other constructor, `pbr_bsdf` included: all four lobes
        // stay at zero (the pre-#2514 state).
        assert_eq!(matte(WHITE).subsurface, 0.0);
        let plain_bsdf = pbr_bsdf([0.9, 0.85, 0.55], 0.5, 0.35);
        assert_eq!(plain_bsdf.subsurface, 0.0);
        assert_eq!(plain_bsdf.sheen, 0.0);
        assert_eq!(plain_bsdf.sheen_tint, 0.0);
        assert_eq!(plain_bsdf.anisotropic, 0.0);

        let lobes = pbr_bsdf_lobes([0.9, 0.85, 0.55], 0.5, 0.35, 0.4, 0.3, 0.2, 0.1);
        assert_eq!(lobes.subsurface, 0.4);
        assert_eq!(lobes.sheen, 0.3);
        assert_eq!(lobes.sheen_tint, 0.2);
        assert_eq!(lobes.anisotropic, 0.1);
        assert_ne!(
            lobes.effect_shader_flags & PBR_BSDF,
            0,
            "pbr_bsdf_lobes must still set MAT_FLAG_PBR_BSDF — driving the \
             lobe scalars without it would silently no-op (#2477)"
        );
        assert_eq!(lobes.metalness, plain_bsdf.metalness);
        assert_eq!(lobes.roughness, plain_bsdf.roughness);
        assert_eq!(lobes.diffuse_color, plain_bsdf.diffuse_color);
    }
}
