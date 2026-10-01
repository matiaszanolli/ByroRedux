//! The combustion-lab harness (#5090 split of `cornell.rs`): the metre-
//! contracted fire lab, its manifest, and its `--combustion-lab` mode
//! parsing.
use super::*;

pub(super) const LAB_UNITS_PER_METER: f32 = byroredux_core::lighting::BETHESDA_UNITS_PER_METER;

const fn lab_metres(value: f32) -> f32 {
    value * LAB_UNITS_PER_METER
}

/// Meter-calibrated transported-combustion validation scene. Keeping these
/// dimensions in one manifest makes scene construction and regression tests
/// share the same physical contract instead of scattering scale literals.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CombustionLabManifest {
    pub room_half_width: f32,
    pub room_half_depth: f32,
    pub room_height: f32,
    pub wall_half_thickness: f32,
    pub flame_position: Vec3,
    pub flame_half_extents: Vec3,
    pub explosion_position: Vec3,
    pub explosion_radius: f32,
    pub explosion_start_delay_seconds: f32,
    pub explosion_lifetime_seconds: f32,
    pub baffle_center: Vec3,
    pub baffle_half_extents: Vec3,
    pub camera_position: Vec3,
    pub camera_target: Vec3,
}

pub(crate) const fn combustion_lab_manifest() -> CombustionLabManifest {
    CombustionLabManifest {
        room_half_width: lab_metres(3.0),
        room_half_depth: lab_metres(4.0),
        room_height: lab_metres(4.0),
        wall_half_thickness: lab_metres(0.06),
        flame_position: Vec3::new(lab_metres(-0.55), lab_metres(0.48), 0.0),
        flame_half_extents: Vec3::new(lab_metres(0.28), lab_metres(0.42), lab_metres(0.28)),
        explosion_position: Vec3::new(lab_metres(0.65), lab_metres(0.55), 0.0),
        explosion_radius: lab_metres(0.45),
        // Leave enough time for validation tooling to attach and capture a
        // true pre-ignition baseline before the one-shot enters the field.
        explosion_start_delay_seconds: 8.0,
        explosion_lifetime_seconds: 8.0,
        // Hood-scale clearance: the 0.90 m flame-source top sits 0.34 m
        // below the underside, so the transported plume—not the source
        // primitive—must reach and spread along the solid.
        baffle_center: Vec3::new(0.0, lab_metres(1.30), 0.0),
        baffle_half_extents: Vec3::new(lab_metres(1.25), lab_metres(0.06), lab_metres(0.8)),
        camera_position: Vec3::new(0.0, lab_metres(2.0), lab_metres(5.5)),
        camera_target: Vec3::new(0.0, lab_metres(1.10), 0.0),
    }
}

/// Exact opt-in flag for the game-data-independent combustion harness.
pub(crate) fn combustion_lab_mode(args: &[String]) -> bool {
    args.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "--combustion-lab" | "--combustion-lab-nuclear"
        )
    })
}

/// Select the high-yield variant of the combustion lab while retaining the
/// same camera, materials, and reference flame for an apples-to-apples A/B.
pub(crate) fn combustion_lab_nuclear_mode(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--combustion-lab-nuclear")
}

/// Build a meter-scaled room for the transported combustion solver.
///
/// The ordinary Cornell showcase is intentionally only a few world units
/// wide, while combustion velocity, buoyancy, extinction, and source size are
/// physical metre contracts. This harness uses Bethesda's canonical 70
/// units/metre conversion and places a thin rigid baffle above the source so
/// one run exposes no-through transport and source-side tangential slip.
pub(crate) fn setup_combustion_lab_scene(
    world: &mut World,
    ctx: &mut VulkanContext,
    nuclear: bool,
) -> (Vec3, Vec3) {
    let manifest = combustion_lab_manifest();
    install_cornell_lighting(world, false);
    // Give the standalone harness a stable crosswind so the same scene
    // exposes external advection, plume shear, and blast roll-up without
    // depending on a loaded WTHR record. The game path replaces this resource
    // from the active weather just as it does for foliage and water.
    world.insert_resource(WindField {
        direction: [1.0, 0.0],
        speed: 48.0,
        gust_amplitude: 12.0,
        gust_frequency: 0.18,
    });
    let neutral = TextureHandle(ctx.texture_registry.neutral_fallback());
    let mut builder = MeshBuilder::new(ctx);

    let horizontal = builder.box_mesh([
        manifest.room_half_width,
        manifest.wall_half_thickness,
        manifest.room_half_depth,
    ]);
    let back = builder.box_mesh([
        manifest.room_half_width,
        manifest.room_height * 0.5,
        manifest.wall_half_thickness,
    ]);
    let side = builder.box_mesh([
        manifest.wall_half_thickness,
        manifest.room_height * 0.5,
        manifest.room_half_depth,
    ]);
    let room_mid_y = manifest.room_height * 0.5;
    for (mesh, position, color, name) in [
        (
            horizontal,
            Vec3::new(0.0, -manifest.wall_half_thickness, 0.0),
            WHITE,
            "combustion_lab_floor",
        ),
        (
            horizontal,
            Vec3::new(
                0.0,
                manifest.room_height + manifest.wall_half_thickness,
                0.0,
            ),
            WHITE,
            "combustion_lab_ceiling",
        ),
        (
            back,
            Vec3::new(
                0.0,
                room_mid_y,
                -manifest.room_half_depth - manifest.wall_half_thickness,
            ),
            WHITE,
            "combustion_lab_back_wall",
        ),
        (
            side,
            Vec3::new(
                -manifest.room_half_width - manifest.wall_half_thickness,
                room_mid_y,
                0.0,
            ),
            RED,
            "combustion_lab_left_wall",
        ),
        (
            side,
            Vec3::new(
                manifest.room_half_width + manifest.wall_half_thickness,
                room_mid_y,
                0.0,
            ),
            GREEN,
            "combustion_lab_right_wall",
        ),
    ] {
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

    if !nuclear {
        // The oil case keeps the hood so the same capture validates a solid
        // boundary and tangential reappearance. A nuclear cloud is an open,
        // high-yield event in this harness; leaving the hood out exposes its
        // cap/stem silhouette instead of hiding it behind the fixture.
        let baffle = builder.box_mesh(manifest.baffle_half_extents.to_array());
        spawn_object(
            world,
            baffle,
            neutral,
            manifest.baffle_center,
            Quat::IDENTITY,
            matte([0.55, 0.58, 0.62]),
            "combustion_lab_overhead_baffle",
        );
    }

    // A low, cool reference fill keeps the room legible before ignition. The
    // transported field's delayed radiant moments remain the dominant warm
    // source once the explosion starts, so surface-light coupling is visible
    // instead of being hidden by a bright authored key.
    spawn_point_light(
        world,
        Vec3::new(lab_metres(2.0), lab_metres(2.7), lab_metres(2.7)),
        lab_metres(8.0),
        [0.12, 0.15, 0.2],
        "combustion_lab_reference_fill",
    );
    spawn_combustion_probe(
        world,
        CombustionProbeSpec {
            kind: CombustionProbeKind::Flame,
            position: manifest.flame_position,
            half_extents: manifest.flame_half_extents,
            name: "combustion_lab_flame",
        },
    );
    let explosion_kind = if nuclear {
        CombustionProbeKind::NuclearExplosion {
            start_delay_seconds: manifest.explosion_start_delay_seconds,
            lifetime_seconds: manifest.explosion_lifetime_seconds,
        }
    } else {
        CombustionProbeKind::Explosion {
            start_delay_seconds: manifest.explosion_start_delay_seconds,
            lifetime_seconds: manifest.explosion_lifetime_seconds,
        }
    };
    let explosion_radius = if nuclear {
        manifest.explosion_radius * 1.6
    } else {
        manifest.explosion_radius
    };
    spawn_combustion_probe(
        world,
        CombustionProbeSpec {
            kind: explosion_kind,
            position: manifest.explosion_position,
            half_extents: Vec3::splat(explosion_radius),
            name: "combustion_lab_explosion",
        },
    );

    builder.finish();
    log::info!(
        "Combustion lab ready: room=6x4x8 m, flame+{} explosion sources, containment={}",
        if nuclear { "nuclear" } else { "oil" },
        if nuclear {
            "open high-yield plume"
        } else {
            "rigid hood underside=1.24 m"
        },
    );
    (manifest.camera_position, manifest.camera_target)
}
