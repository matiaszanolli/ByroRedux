//! The native-scale Skyrim glass-dragon experiment (#5090 split of
//! `cornell.rs`): room constants, material override, and the
//! `--cornell-glass-dragon` mode parsing.
use super::*;

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
pub(super) const DRAGON_ROOM_HALF_X: f32 = 1_100.0;
pub(super) const DRAGON_ROOM_HALF_Z: f32 = 1_200.0;
pub(super) const DRAGON_ROOM_HEIGHT: f32 = 650.0;
pub(super) const DRAGON_ROOM_T: f32 = 4.0;
pub(super) const DRAGON_FLOOR_LIFT: f32 = 10.0;
pub(super) const DRAGON_PRESENTATION_YAW: f32 = 35.0_f32.to_radians();
pub(super) const DRAGON_GLASS_TINT: [f32; 3] = [0.72, 0.88, 1.0];
pub(super) const DRAGON_GLASS_ROUGHNESS: f32 = 0.04;

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
pub(super) const SUN_DIR_RAW: Vec3 = Vec3::new(0.6, 0.84, 0.4);

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
pub(super) fn force_glass_dragon_material(source: &mut ImportedMaterial) {
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

pub(super) fn force_glass_dragon_scene(imported: &mut byroredux_nif::import::ImportedScene) {
    for mesh in &mut imported.meshes {
        force_glass_dragon_material(&mut mesh.material);
    }
}

pub(super) fn place_glass_dragon(transform: &mut Transform) {
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
