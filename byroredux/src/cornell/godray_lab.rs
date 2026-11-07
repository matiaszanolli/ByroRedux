//! The godray lab scene (#5090 split of `cornell.rs`): a reproducible,
//! game-data-free interior for the unmarked-hole godray path, plus its
//! `--godray-lab` / `--godray-lab-sealed` mode parsing.
use super::*;

/// Reproducible, game-data-free interior for the unmarked-hole godray path.
/// `--godray-lab-sealed` changes only the roof, giving captures a matched
/// negative control. The 200-BU hole is enclosed by a coplanar roof beyond
/// the shader's 192-BU diagonal rim probes.
pub(crate) fn godray_lab_mode(args: &[String]) -> Option<bool> {
    if args.iter().any(|arg| arg == "--godray-lab-sealed") {
        Some(true)
    } else if args.iter().any(|arg| arg == "--godray-lab") {
        Some(false)
    } else {
        None
    }
}

pub(crate) fn setup_godray_lab_scene(
    world: &mut World,
    ctx: &mut VulkanContext,
    sealed: bool,
) -> (Vec3, Vec3) {
    install_cornell_lighting(world, true);
    {
        let mut cell = world.resource_mut::<CellLightingRes>();
        cell.is_interior = true;
        cell.ambient = [0.02; 3];
        cell.directional_color = [0.0; 3];
        cell.fog_color = [0.0; 3];
        cell.fog_medium = crate::fog::FogMedium::DISABLED;
    }
    {
        let mut sky = world.resource_mut::<crate::components::SkyParamsRes>();
        sky.sun_direction = [0.0, 1.0, 0.0];
        sky.sun_color = [1.0, 0.95, 0.85];
        sky.sun_intensity = 6.0;
        // An interior's volumetric sun is the outdoor palette's `sun_illuminance`, built from the
        // weather's sunlight colour (#4909); a lab has no weather system, so give it one. The value is
        // the radiance the gate's thresholds were written against (the old `sun_color x sun_intensity`,
        // about 6), NOT a real weather colour: authored sunlight colours are around 0..1, at which this
        // room's shaft reads centre delta 8 instead of 45 (#6730). A real cell is lit by the real one.
        sky.weather.sunlight_color = [6.0, 5.7, 5.1];
    }
    // The froxel pass reads the interior's Show Sky flag as its "open sky" gate; real cells set it from
    // the CELL record. #4902 removed the render-loop fallback that supplied it, so the lab states it.
    world.insert_resource(crate::components::InteriorSkyExposureRes(true));

    let neutral = TextureHandle(ctx.texture_registry.neutral_fallback());
    let mut builder = MeshBuilder::new(ctx);
    let roof_y = 284.0;
    let mut slabs: Vec<([f32; 3], Vec3, [f32; 3], &str)> = vec![
        ([350.0, 4.0, 350.0], Vec3::new(0.0, -4.0, 0.0), WHITE, "godray_floor"),
        ([350.0, 140.0, 4.0], Vec3::new(0.0, 140.0, -354.0), WHITE, "godray_back"),
        ([350.0, 140.0, 4.0], Vec3::new(0.0, 140.0, 354.0), WHITE, "godray_front"),
        ([4.0, 140.0, 350.0], Vec3::new(-354.0, 140.0, 0.0), RED, "godray_left"),
        ([4.0, 140.0, 350.0], Vec3::new(354.0, 140.0, 0.0), GREEN, "godray_right"),
    ];
    if sealed {
        slabs.push((
            [350.0, 4.0, 350.0],
            Vec3::new(0.0, roof_y, 0.0),
            WHITE,
            "godray_sealed_roof",
        ));
    } else {
        slabs.extend([
            ([125.0, 4.0, 350.0], Vec3::new(-225.0, roof_y, 0.0), WHITE, "godray_roof_left"),
            ([125.0, 4.0, 350.0], Vec3::new(225.0, roof_y, 0.0), WHITE, "godray_roof_right"),
            ([100.0, 4.0, 125.0], Vec3::new(0.0, roof_y, -225.0), WHITE, "godray_roof_back"),
            ([100.0, 4.0, 125.0], Vec3::new(0.0, roof_y, 225.0), WHITE, "godray_roof_front"),
        ]);
    }
    for (half_extents, position, color, name) in slabs {
        // One upload per entity: unload releases one mesh reference per
        // holder, as in the Studio room above.
        let mesh = builder.box_mesh(half_extents);
        spawn_object(world, mesh, neutral, position, Quat::IDENTITY, matte(color), name);
    }
    builder.finish();
    spawn_fog_volume_with_extinction(
        world,
        Vec3::new(0.0, 140.0, 0.0),
        Vec3::new(330.0, 136.0, 330.0),
        0.08,
        "godray_room_dust",
    );
    (Vec3::new(0.0, 150.0, 270.0), Vec3::new(0.0, 180.0, 0.0))
}
