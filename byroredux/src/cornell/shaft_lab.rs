//! The exterior shaft lab (#6726, VLM-SH-001): a reproducible, game-data-free exterior for the
//! exterior sunlight-shaft path, plus its `--shaft-lab` / `--shaft-lab-solid` mode parsing. The
//! outdoor sibling of the interior [`godray_lab`](super::godray_lab).
//!
//! A low sun stands behind a long wall in open sky. In the open variant the wall is a row of slats
//! with gaps between them, so the sun's light passes the gaps and lights slanted slabs of air in
//! front of the wall: the shafts. `--shaft-lab-solid` replaces the slats with one unbroken wall, the
//! matched negative control. Everything else, camera included, is identical, so a pixel difference
//! between the two captures is the shaft and nothing else (`exterior-shafts.sh`, VLM-SH-002).
//!
//! The medium is whatever the engine's procedural-fallback exterior supplies (`0.000777 / m` before the
//! base-fog scale, about 1.8 times FNV's clear weather, read from `env.health`), not a lab-thick fog: the
//! scene exists to show what a clear exterior does, which is what the exterior haze floor (VLM-SH-010)
//! changes. The weather system rewrites the cell's medium every frame, so the lab does not set one of its
//! own. Sizes are metres, converted at the use sites.
use super::*;

/// Sun elevation of the lab, degrees. Low enough that the shafts are long and the floor's horizon
/// gain is in play, high enough that the sun clears the wall's far side.
pub(crate) const SHAFT_LAB_SUN_ELEVATION_DEG: f32 = 10.0;
/// Number of gaps in the open wall's row of slats.
pub(crate) const SHAFT_LAB_SLATS: usize = 12;

const SLAT_HALF_WIDTH_M: f32 = 0.6;
const SLAT_PITCH_M: f32 = 3.0;
const WALL_HEIGHT_M: f32 = 14.0;
const WALL_THICKNESS_M: f32 = 1.0;
const GROUND_COLOR: [f32; 3] = [0.34, 0.32, 0.28];
const WALL_COLOR: [f32; 3] = [0.55, 0.5, 0.45];

/// The alpha-tested card (VLM-SH-009): a thin panel centred on the sun's axis, [`CARD_STRIPES`] vertical
/// stripes wide. Its texture is opaque on the even stripes and cut out on the odd ones. Three matched
/// variants share the geometry of the see-through stripes, so the only thing that differs is whether the
/// sun's ray is stopped there:
///
/// - [`ShaftLabCard::Cutout`]: one panel, the odd stripes cut out by alpha (what content ships);
/// - [`ShaftLabCard::Geometry`]: four slats at the even stripes, the odd stripes real holes (the ground
///   truth for "the sun passes there");
/// - [`ShaftLabCard::Solid`]: one unbroken panel (the floor of "the sun is stopped").
///
/// A froxel sun trace that honours alpha coverage reads the cutout panel like the geometry one; one that
/// treats every triangle as opaque reads it like the solid one. `exterior-shafts.sh` reads the difference as
/// `(cutout - solid) / (geometry - solid)` of the in-scatter term over the card: 1 is alpha-aware, 0 is opaque.
pub(crate) const CARD_STRIPES: u32 = 7;
/// Width of one stripe, metres. Wide enough that the camera's near-axial view rays stay inside one lit
/// or one shadowed slab for most of their length.
pub(crate) const CARD_STRIPE_WIDTH_M: f32 = 2.0;
pub(crate) const CARD_HEIGHT_M: f32 = 14.0;
const CARD_THICKNESS_M: f32 = 0.05;
const CARD_STRIPE_TEXELS: u32 = 9;

/// The three matched card variants (`--shaft-lab-card`, `--shaft-lab-card-geo`, `--shaft-lab-card-solid`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShaftLabCard {
    Cutout,
    Geometry,
    Solid,
}

/// Which card variant the arguments select. Exact matches only.
pub(crate) fn shaft_lab_card_mode(args: &[String]) -> Option<ShaftLabCard> {
    [
        ("--shaft-lab-card", ShaftLabCard::Cutout),
        ("--shaft-lab-card-geo", ShaftLabCard::Geometry),
        ("--shaft-lab-card-solid", ShaftLabCard::Solid),
    ]
    .into_iter()
    .find(|(flag, _)| args.iter().any(|arg| arg == flag))
    .map(|(_, variant)| variant)
}

/// RGBA texels of the card: opaque white on even stripes, alpha 0 on odd ones.
pub(crate) fn card_stripe_pixels() -> (u32, u32, Vec<u8>) {
    let (width, height) = (CARD_STRIPES * CARD_STRIPE_TEXELS, 4);
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..height {
        for x in 0..width {
            let cut = (x / CARD_STRIPE_TEXELS) % 2 == 1;
            pixels.extend_from_slice(&[255, 255, 255, if cut { 0 } else { 255 }]);
        }
    }
    (width, height, pixels)
}

/// `--shaft-lab` (`Some(false)`, the open variant) or `--shaft-lab-solid` (`Some(true)`, the control).
/// Exact matches only, so `--shaft-lab-extra` selects nothing.
pub(crate) fn shaft_lab_mode(args: &[String]) -> Option<bool> {
    if args.iter().any(|arg| arg == "--shaft-lab-solid") {
        Some(true)
    } else if args.iter().any(|arg| arg == "--shaft-lab") {
        Some(false)
    } else {
        None
    }
}

/// Camera distance from the wall, metres, when `--shaft-lab-distance` is absent.
pub(crate) const SHAFT_LAB_DISTANCE_M: f32 = 55.0;

/// `--shaft-lab-distance <metres>`: how far the camera stands from the wall, for the range sweep
/// (VLM-SH-005). Absent, unparsable, non-finite or non-positive values fall back to the default, so a
/// typo never moves the pinned camera of the gate and the golden frame.
pub(crate) fn shaft_lab_distance_m(args: &[String]) -> f32 {
    args.windows(2)
        .find(|pair| pair[0] == "--shaft-lab-distance")
        .and_then(|pair| pair[1].parse::<f32>().ok())
        .filter(|metres| metres.is_finite() && *metres > 0.0)
        .unwrap_or(SHAFT_LAB_DISTANCE_M)
}

/// The unit direction TO the sun, in the engine's Y-up convention: elevation
/// [`SHAFT_LAB_SUN_ELEVATION_DEG`], toward `-Z` so the sun stands behind the wall for a camera on `+Z`.
pub(crate) fn shaft_lab_sun_direction() -> [f32; 3] {
    let elevation = SHAFT_LAB_SUN_ELEVATION_DEG.to_radians();
    [0.0, elevation.sin(), -elevation.cos()]
}

/// What stands in the sun's path.
#[derive(Clone, Copy)]
enum Wall {
    Slats,
    Solid,
    Card(ShaftLabCard),
}

pub(crate) fn setup_shaft_lab_scene(
    world: &mut World,
    ctx: &mut VulkanContext,
    solid: bool,
    distance_m: f32,
) -> (Vec3, Vec3) {
    build_shaft_lab_scene(world, ctx, if solid { Wall::Solid } else { Wall::Slats }, distance_m)
}

/// The alpha-card lab (VLM-SH-009): the same sky, ground and camera, with the striped card of the chosen
/// variant in place of the slat wall.
pub(crate) fn setup_shaft_card_lab_scene(
    world: &mut World,
    ctx: &mut VulkanContext,
    card: ShaftLabCard,
    distance_m: f32,
) -> (Vec3, Vec3) {
    build_shaft_lab_scene(world, ctx, Wall::Card(card), distance_m)
}

fn build_shaft_lab_scene(
    world: &mut World,
    ctx: &mut VulkanContext,
    wall: Wall,
    distance_m: f32,
) -> (Vec3, Vec3) {
    use crate::fog::WORLD_UNITS_PER_METER as UPM;
    install_cornell_lighting(world, true);
    let sun = shaft_lab_sun_direction();
    {
        let mut cell = world.resource_mut::<CellLightingRes>();
        cell.is_interior = false;
        cell.directional_dir = sun;
    }
    world.resource_mut::<crate::components::SkyParamsRes>().sun_direction = sun;

    let neutral = TextureHandle(ctx.texture_registry.neutral_fallback());
    let mut builder = MeshBuilder::new(ctx);
    let mut slabs: Vec<([f32; 3], Vec3, [f32; 3], String)> = vec![(
        // Roughly 200 m a side: past the 128 m froxel range in every direction.
        [100.0 * UPM, 0.05 * UPM, 100.0 * UPM],
        Vec3::new(0.0, -0.05 * UPM, 0.0),
        GROUND_COLOR,
        "shaft_ground".to_string(),
    )];
    let half_height = WALL_HEIGHT_M * 0.5 * UPM;
    let half_thickness = WALL_THICKNESS_M * 0.5 * UPM;
    let mut cutout_card = false;
    if let Wall::Card(card) = wall {
        let stripe_half = CARD_STRIPE_WIDTH_M * 0.5 * UPM;
        let card_half_height = CARD_HEIGHT_M * 0.5 * UPM;
        let card_half_thickness = CARD_THICKNESS_M * 0.5 * UPM;
        let first = -(CARD_STRIPES as f32 - 1.0) * 0.5 * CARD_STRIPE_WIDTH_M;
        match card {
            ShaftLabCard::Cutout => cutout_card = true,
            ShaftLabCard::Geometry => {
                // The even stripes stand, the odd ones are holes.
                for stripe in (0..CARD_STRIPES).step_by(2) {
                    slabs.push((
                        [stripe_half, card_half_height, card_half_thickness],
                        Vec3::new((first + stripe as f32 * CARD_STRIPE_WIDTH_M) * UPM, card_half_height, 0.0),
                        WALL_COLOR,
                        format!("shaft_card_stripe_{stripe}"),
                    ));
                }
            }
            ShaftLabCard::Solid => slabs.push((
                [stripe_half * CARD_STRIPES as f32, card_half_height, card_half_thickness],
                Vec3::new(0.0, card_half_height, 0.0),
                WALL_COLOR,
                "shaft_card_solid".to_string(),
            )),
        }
    } else if matches!(wall, Wall::Solid) {
        let half_width = (SHAFT_LAB_SLATS as f32 * SLAT_PITCH_M) * 0.5 * UPM;
        slabs.push((
            [half_width, half_height, half_thickness],
            Vec3::new(0.0, half_height, 0.0),
            WALL_COLOR,
            "shaft_wall_solid".to_string(),
        ));
    } else {
        let first = -(SHAFT_LAB_SLATS as f32 - 1.0) * 0.5 * SLAT_PITCH_M;
        for slat in 0..SHAFT_LAB_SLATS {
            let x = first + slat as f32 * SLAT_PITCH_M;
            slabs.push((
                [SLAT_HALF_WIDTH_M * UPM, half_height, half_thickness],
                Vec3::new(x * UPM, half_height, 0.0),
                WALL_COLOR,
                format!("shaft_slat_{slat}"),
            ));
        }
        // A lintel along the top so the slats read as one structure and the sun cannot pass over it.
        let half_width = (SHAFT_LAB_SLATS as f32 * SLAT_PITCH_M) * 0.5 * UPM;
        slabs.push((
            [half_width, 0.5 * UPM, half_thickness],
            Vec3::new(0.0, WALL_HEIGHT_M * UPM + 0.5 * UPM, 0.0),
            WALL_COLOR,
            "shaft_lintel".to_string(),
        ));
    }
    for (half_extents, position, color, name) in slabs {
        // One upload per entity: unload releases one mesh reference per holder.
        let mesh = builder.box_mesh(half_extents);
        spawn_object(world, mesh, neutral, position, Quat::IDENTITY, matte(color), &name);
    }
    if cutout_card {
        let (width, height, pixels) = card_stripe_pixels();
        let texture = TextureHandle(builder.register_texture(width, height, pixels));
        let mesh = builder.box_mesh([
            CARD_STRIPE_WIDTH_M * CARD_STRIPES as f32 * 0.5 * UPM,
            CARD_HEIGHT_M * 0.5 * UPM,
            CARD_THICKNESS_M * 0.5 * UPM,
        ]);
        let mut material = matte(WALL_COLOR);
        material.alpha_test = true;
        material.alpha_threshold = 0.5;
        material.alpha_test_func = 6;
        spawn_object(
            world,
            mesh,
            texture,
            Vec3::new(0.0, CARD_HEIGHT_M * 0.5 * UPM, 0.0),
            Quat::IDENTITY,
            material,
            "shaft_card",
        );
    }
    builder.finish();

    // The camera stands 55 m out on the shadow side, ON the sun's axis and looking along it (up the
    // sun's elevation), so every view ray runs nearly parallel to the beams and stays inside one lit
    // or one shadowed slab for its whole length: that is where a shaft is a crisp stripe. Off axis, a
    // ray crosses the 3 m slat pitch every few metres and averages the stripes away. The wall's top
    // (14 m) clears the sun at this range, so the sun disc is hidden and only the beams remain.
    let eye_height = 1.7;
    let distance = distance_m;
    let rise = distance * SHAFT_LAB_SUN_ELEVATION_DEG.to_radians().tan();
    (
        Vec3::new(0.0, eye_height * UPM, distance * UPM),
        Vec3::new(0.0, (eye_height + rise) * UPM, 0.0),
    )
}
