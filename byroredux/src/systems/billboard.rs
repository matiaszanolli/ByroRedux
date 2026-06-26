//! Billboard orientation — face-camera mode for sprite-like nodes.

use byroredux_core::ecs::{ActiveCamera, Billboard, BillboardMode, GlobalTransform, World};
use byroredux_core::math::{Quat, Vec3};

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

    move |world: &World, _dt: f32| {
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
        if last_cam == Some((cam_pos, cam_forward)) {
            return;
        }
        last_cam = Some((cam_pos, cam_forward));

        let Some(bq) = world.query::<Billboard>() else {
            return;
        };

        for (entity, billboard) in bq.iter() {
            let Some(global) = gq.get_mut(entity) else {
                continue;
            };

            let new_rot = compute_billboard_rotation(
                billboard.mode,
                global.translation,
                cam_pos,
                cam_forward,
            );
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
