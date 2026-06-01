//! Camera view-projection + frustum setup — extracted from
//! `build_render_data` per #1115.
//!
//! Assembles the per-frame camera matrices from the `ActiveCamera`
//! resource + the camera entity's `Camera` and `Transform` components.
//! Falls back to identity matrices when any of those are missing
//! (engine just opened, no active camera, missing components).
//!
//! Also owns the `FrustumPlanes` type — derived from the assembled
//! view-projection matrix and consumed by the static-mesh loop for
//! per-entity sphere-vs-frustum culling.

use byroredux_core::ecs::{ActiveCamera, Camera, Transform, World};
use byroredux_core::math::{Mat4, Vec3, Vec4};

/// 6-plane camera frustum, normalized so a plane-distance comparison
/// against radius is direct. Built by [`assemble_camera`] from the
/// per-frame view-projection.
///
/// Stored unnormalized at extraction; we normalize once at construction
/// so the sphere test can compare directly against the entity's
/// `WorldBound::radius`.
pub(crate) struct FrustumPlanes {
    planes: [Vec4; 6],
}

impl FrustumPlanes {
    pub(crate) fn from_view_proj(m: Mat4) -> Self {
        let r0 = m.row(0);
        let r1 = m.row(1);
        let r2 = m.row(2);
        let r3 = m.row(3);

        let mut planes = [
            r3 + r0, // left
            r3 - r0, // right
            r3 + r1, // bottom
            r3 - r1, // top
            r3 + r2, // near
            r3 - r2, // far
        ];

        for p in &mut planes {
            let len = Vec3::new(p.x, p.y, p.z).length();
            if len > 1e-10 {
                *p /= len;
            }
        }

        Self { planes }
    }

    pub(crate) fn contains_sphere(&self, center: Vec3, radius: f32) -> bool {
        for p in &self.planes {
            let dist = p.x * center.x + p.y * center.y + p.z * center.z + p.w;
            if dist < -radius {
                return false;
            }
        }
        true
    }
}

/// Per-frame camera matrices + culling frustum, ready for the main
/// render loop.
pub(super) struct CameraView {
    /// Column-major flat array for the renderer-side UBO upload.
    pub view_proj: [f32; 16],
    /// 6-plane frustum derived from `view_proj`.
    pub frustum: FrustumPlanes,
    /// Square matrix for shader-side clip-space math (sort_depth,
    /// particle pos_clip, …).
    pub vp_mat: Mat4,
    /// World-space camera position — captured for particle billboard
    /// face-camera rotation.
    pub cam_pos: Vec3,
    /// Camera right vector in world space (unit length).
    pub cam_right: Vec3,
    /// Camera up vector in world space (unit length).
    pub cam_up: Vec3,
    /// Camera forward vector in world space (unit length, points into the scene).
    pub cam_forward: Vec3,
    /// Perspective projection matrix (column-major, Vulkan clip space with Y-flip).
    /// Stored separately so the renderer can apply a DOF-jittered view matrix and
    /// recompute view_proj without re-running the full camera assembly.
    pub proj_mat: Mat4,
    /// Lens aperture half-radius (world units). `0.0` = pinhole / DOF disabled.
    pub aperture: f32,
    /// Focal distance (world units). Surfaces at this depth are in sharp focus.
    pub focus_dist: f32,
}

/// Assemble the active camera's view-projection matrices + frustum.
///
/// Returns identity matrices and a degenerate frustum when no active
/// camera is set (engine just opened, between cell loads, or missing
/// `Transform` / `Camera` components on the active entity).
pub(super) fn assemble_camera(world: &World) -> CameraView {
    let mut cam_pos = Vec3::ZERO;
    let mut cam_right = Vec3::X;
    let mut cam_up = Vec3::Y;
    let mut cam_forward = -Vec3::Z;
    let mut proj_mat = Mat4::IDENTITY;
    let mut aperture = 0.0f32;
    let mut focus_dist = 20.0f32;

    let (view_proj, frustum, vp_mat) = if let Some(active) = world.try_resource::<ActiveCamera>() {
        let cam_entity = active.0;
        drop(active);

        let cam_q = world.query::<Camera>();
        let transform_q = world.query::<Transform>();

        let vp = match (cam_q, transform_q) {
            (Some(cq), Some(tq)) => {
                let cam = cq.get(cam_entity);
                let t = tq.get(cam_entity);
                match (cam, t) {
                    (Some(c), Some(t)) => {
                        cam_pos = t.translation;
                        // Extract world-space basis from the Transform rotation.
                        // Camera local axes: X=right, Y=up, -Z=forward (look direction).
                        let rot = t.rotation;
                        cam_right = rot * Vec3::X;
                        cam_up = rot * Vec3::Y;
                        cam_forward = rot * (-Vec3::Z);
                        proj_mat = c.projection_matrix();
                        aperture = c.aperture;
                        focus_dist = c.focus_dist;
                        proj_mat * Camera::view_matrix(t)
                    }
                    _ => Mat4::IDENTITY,
                }
            }
            _ => Mat4::IDENTITY,
        };
        let frustum = FrustumPlanes::from_view_proj(vp);
        (vp.to_cols_array(), frustum, vp)
    } else {
        (
            Mat4::IDENTITY.to_cols_array(),
            FrustumPlanes::from_view_proj(Mat4::IDENTITY),
            Mat4::IDENTITY,
        )
    };
    CameraView {
        view_proj,
        frustum,
        vp_mat,
        cam_pos,
        cam_right,
        cam_up,
        cam_forward,
        proj_mat,
        aperture,
        focus_dist,
    }
}
