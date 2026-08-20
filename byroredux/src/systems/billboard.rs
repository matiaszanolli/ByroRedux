//! Billboard orientation — face-camera mode for sprite-like nodes.

use byroredux_core::ecs::components::groundcover::WindField;
use byroredux_core::ecs::{
    ActiveCamera, Billboard, BillboardMode, GlobalTransform, SpeedTreeWind, World,
};
use byroredux_core::math::{Quat, Vec2, Vec3};

/// Billboard system factory — returns a closure with a cached camera pose.
///
/// Each frame the closure checks whether the camera position or forward
/// direction changed since last frame. When neither moved (static scene,
/// camera parked) the entire billboard loop is skipped, preventing
/// `get_mut` from arming `GlobalTransform`'s TRACK_CHANGES dirty set for
/// every billboard entity. Without this gate, `world_bound_propagation`'s
/// incremental-bounds fast path was defeated every frame in billboard-heavy
/// cells (vegetation impostors, sprite quads) — see #1374.
///
/// Behavior is identical to a per-frame plain function when the camera does
/// move: all billboard rotations are recomputed and written exactly as
/// before.
///
/// Mirrors Gamebryo's `NiBillboardNode::UpdateWorldBound`. See #225.
pub(crate) fn make_billboard_system() -> impl FnMut(&World, f32) + Send + Sync {
    // Sentinel: `None` on first frame so the loop always runs once.
    let mut last_cam: Option<(Vec3, Vec3)> = None;
    let mut elapsed = 0.0_f32;
    let mut last_wind_active = false;

    move |world: &World, dt: f32| {
        elapsed = (elapsed + dt.max(0.0)).max(0.0);
        let wind = world
            .try_resource::<WindField>()
            .map(|w| *w)
            .unwrap_or_default();
        let wind_active = wind.speed > 1.0e-4 || wind.gust_amplitude > 1.0e-4;
        let wind_state_changed = wind_active != last_wind_active;
        last_wind_active = wind_active;
        // Active camera lookup (position + forward).
        let Some(active) = world.try_resource::<ActiveCamera>() else {
            return;
        };
        let cam_entity = active.0;
        drop(active);

        // Single GlobalTransform write query — `get` reads the camera GT
        // through the same handle that drives the billboard writes below.
        // Pre-#829 the system cycled a read lock + write lock on the same
        // storage every frame; the read-then-write pair burned ~50–100 ns
        // and a Vec allocation in release (compounding with #823) plus
        // opened a window for a future deadlock if the prelude grew
        // another acquisition between the two.
        let Some(mut gq) = world.query_mut::<GlobalTransform>() else {
            return;
        };
        let Some(cam_global) = gq.get(cam_entity).copied() else {
            return;
        };
        let cam_pos = cam_global.translation;
        // Camera forward = rotation * -Z (see Camera::view_matrix).
        let cam_forward = cam_global.rotation * -Vec3::Z;

        // Camera-motion gate (#1374): when neither camera position nor
        // forward direction changed since last frame, every billboard
        // rotation is still correct from the prior frame — skip the loop.
        // Exact equality is appropriate here: the camera transform is
        // written by camera_follow_system / fly_camera_system with no
        // floating-point accumulation.
        if last_cam == Some((cam_pos, cam_forward)) && !wind_active && !wind_state_changed {
            return;
        }
        last_cam = Some((cam_pos, cam_forward));

        let Some(bq) = world.query::<Billboard>() else {
            return;
        };
        let swq = world.query::<SpeedTreeWind>();

        for (entity, billboard) in bq.iter() {
            let Some(global) = gq.get_mut(entity) else {
                continue;
            };

            let mut new_rot = compute_billboard_rotation(
                billboard.mode,
                global.translation,
                cam_pos,
                cam_forward,
            );
            // SpeedTree placements carry the authoritative `SpeedTreeWind`
            // marker, but their source NIFs are not uniform about which
            // billboard enum they use (some author `RotateAboutUp`, others
            // Bethesda's `BsRotateAboutUp`). Key the canopy response off the
            // marker rather than one enum value so every imported tree gets
            // the shared weather wind while ordinary billboards remain
            // unaffected. Phase is world-position seeded, keeping nearby
            // trees in sync while avoiding lockstep.
            let tree_wind = swq.as_ref().and_then(|q| q.get(entity).copied());
            if wind_active && tree_wind.is_some() {
                let gust = wind.speed
                    + wind.gust_amplitude
                        * (elapsed * wind.gust_frequency * std::f32::consts::TAU).sin();
                let strength = (gust
                    / byroredux_core::ecs::components::groundcover::MAX_WIND_SPEED)
                    .clamp(0.0, 1.0);
                // Wind direction is the horizontal X/Z vector used by the
                // weather and water paths. Project the tree position onto it
                // so trees downwind share a coherent travelling wave rather
                // than merely oscillating at a speed-dependent frequency.
                let wind_dir = Vec2::new(wind.direction[0], wind.direction[1]);
                let wind_dir = if wind_dir.length_squared() > 1.0e-6 {
                    wind_dir.normalize()
                } else {
                    Vec2::X
                };
                let along_wind =
                    global.translation.x * wind_dir.x + global.translation.z * wind_dir.y;
                let phase = along_wind * 0.017;
                let wave = (elapsed * (0.35 + strength * 0.85) + phase).sin();
                let cross = (elapsed * (0.47 + strength * 0.65) + phase * 1.7).cos();
                let (response, stiffness) = tree_wind
                    .map(|wind| {
                        (
                            wind.response.clamp(0.0, 4.0),
                            wind.stiffness.clamp(0.0, 1.0),
                        )
                    })
                    .unwrap_or((1.0, 0.0));
                let bend = strength * 0.16 * response * (1.0 - stiffness);
                let along_weight = wind_dir.x.abs().max(0.25);
                let cross_weight = wind_dir.y.abs().max(0.25);
                new_rot = new_rot
                    * Quat::from_rotation_z(wave * bend * along_weight)
                    * Quat::from_rotation_x(cross * bend * 0.65 * cross_weight);
            }
            global.rotation = new_rot;
        }
    }
}

/// Compute a world-space rotation for a billboard.
///
/// `ALWAYS_FACE_CENTER` / `RIGID_FACE_CENTER` point the billboard's forward
/// axis at the camera position (per-billboard look-at). `ALWAYS_FACE_CAMERA`
/// / `RIGID_FACE_CAMERA` use the camera's forward direction for every
/// billboard (parallel planes — cheaper, no per-billboard yaw changes when
/// walking sideways past a sprite). Up-locked modes keep world Y fixed and
/// only rotate around it.
fn compute_billboard_rotation(
    mode: BillboardMode,
    billboard_pos: Vec3,
    cam_pos: Vec3,
    cam_forward: Vec3,
) -> Quat {
    // Direction the billboard needs to LOOK toward (in world space).
    // "Face camera" rules want the billboard to look at the camera, so its
    // local -Z (forward) should point toward `cam_pos` (or along the
    // camera's forward plane).
    let look_dir = match mode {
        BillboardMode::AlwaysFaceCamera
        | BillboardMode::RigidFaceCamera
        | BillboardMode::AlwaysFaceCenter
        | BillboardMode::RigidFaceCenter => {
            let to_cam = cam_pos - billboard_pos;
            if to_cam.length_squared() < 1.0e-6 {
                // Billboard at camera origin — fall back to camera forward.
                -cam_forward
            } else {
                to_cam.normalize()
            }
        }
        BillboardMode::RotateAboutUp | BillboardMode::RotateAboutUp2 => {
            // Rotate only around world Y. Project the to-camera vector onto
            // the XZ plane, normalize, and use it as the horizontal look
            // direction.
            let mut to_cam = cam_pos - billboard_pos;
            to_cam.y = 0.0;
            if to_cam.length_squared() < 1.0e-6 {
                Vec3::Z
            } else {
                to_cam.normalize()
            }
        }
        BillboardMode::BsRotateAboutUp => {
            // NIF intent is rotation about the node's *local* up; we don't
            // have the local frame here, so this locks the world up (Y) —
            // identical to RotateAboutUp above and visually indistinguishable
            // for the grass/foliage BsRotateAboutUp is authored on (SPT-NEW-04:
            // the doc previously claimed a local-Z rotation the code never
            // performs).
            let mut to_cam = cam_pos - billboard_pos;
            to_cam.y = 0.0;
            if to_cam.length_squared() < 1.0e-6 {
                Vec3::Z
            } else {
                to_cam.normalize()
            }
        }
    };

    // Build a look-at rotation: forward = look_dir, up = world Y.
    // `Quat::from_rotation_arc(-Z, look_dir)` handles the short-path rotation
    // and keeps roll stable when up is parallel to look_dir.
    let from = -Vec3::Z;
    if (look_dir - from).length_squared() < 1.0e-6 {
        return Quat::IDENTITY;
    }
    Quat::from_rotation_arc(from, look_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::groundcover::WindField;
    use byroredux_core::ecs::{Camera, Transform};

    #[test]
    fn speedtree_billboard_bends_with_shared_weather_wind() {
        fn rotation_after_wind(
            mode: BillboardMode,
            direction: [f32; 2],
            speed: f32,
        ) -> Quat {
            let mut world = World::new();
            let camera = world.spawn();
            world.insert(camera, Transform::IDENTITY);
            world.insert(camera, GlobalTransform::IDENTITY);
            world.insert(camera, Camera::default());
            world.insert_resource(ActiveCamera(camera));

            let tree = world.spawn();
            world.insert(
                tree,
                GlobalTransform::new(Vec3::new(12.0, 0.0, 8.0), Quat::IDENTITY, 1.0),
            );
            world.insert(tree, Billboard::new(mode));
            world.insert(tree, SpeedTreeWind::new(1.0, 0.0));
            world.insert_resource(WindField {
                direction,
                speed,
                gust_amplitude: 0.0,
                gust_frequency: 0.0,
            });

            let mut system = make_billboard_system();
            system(&world, 0.0);
            system(&world, 0.5);
            let rotation = world
                .query::<GlobalTransform>()
                .unwrap()
                .get(tree)
                .unwrap()
                .rotation;
            rotation
        }

        let calm = rotation_after_wind(BillboardMode::BsRotateAboutUp, [1.0, 0.0], 0.0);
        let first = rotation_after_wind(BillboardMode::BsRotateAboutUp, [1.0, 0.0], 220.0);
        let second = rotation_after_wind(BillboardMode::BsRotateAboutUp, [0.0, 1.0], 220.0);
        assert_ne!(
            calm, first,
            "SpeedTree billboard must respond to weather wind"
        );
        assert_ne!(
            first, second,
            "SpeedTree sway must follow the atmospheric wind direction"
        );

        let rotate_about_up =
            rotation_after_wind(BillboardMode::RotateAboutUp, [1.0, 0.0], 220.0);
        assert_ne!(
            calm, rotate_about_up,
            "SpeedTreeWind marker must drive trees using the legacy RotateAboutUp mode"
        );
    }
}
