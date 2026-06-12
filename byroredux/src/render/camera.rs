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

/// Cell-grid snap for the camera-relative render origin. Keeping the origin
/// on the 4096-unit cell grid means it only moves when the camera crosses a
/// cell boundary. A crossing does NOT reset TAA/SVGF temporal history —
/// instead the renderer tracks `prev_render_origin` and uploads an
/// origin-corrected previous view-projection
/// (`prev_vp · translation(O₂ − O₁)`, see
/// `origin_corrected_prev_view_proj` in `vulkan/context/draw.rs`), so motion
/// vectors stay valid across crossings (#1489 / REN2-04).
/// MUST match `RENDER_ORIGIN_SNAP` in `vulkan/context/draw.rs` (#1494
/// tracks hoisting the duplicate into a shared constant).
const RENDER_ORIGIN_SNAP: f32 = 4096.0;

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

        // Absolute view-projection (full-magnitude camera translation).
        // Consumed by the CPU-side frustum cull + sort-depth, which compare
        // against ABSOLUTE world positions (`WorldBound.center`), so this
        // must stay absolute — only the GPU-uploaded matrix goes relative.
        let vp_abs = match (cam_q, transform_q) {
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
        // Camera-relative view-projection (#markarth-precision): snap the
        // origin to the cell grid and rebuild the view with the camera near 0,
        // so the GPU clip-space f32 math avoids the precision cliff at large
        // worldspace offsets (Markarth at world X ≈ -176000, where f32 carries
        // only ~0.015-unit precision → fine carved detail collapses to spikes).
        // This matrix is uploaded to `GpuCamera.view_proj`; per-instance model
        // translations are rebased by the same origin in `draw_frame`, and the
        // vertex shader reconstructs absolute world position as
        // `worldPos_rel + render_origin`.
        let o = (cam_pos / RENDER_ORIGIN_SNAP).floor() * RENDER_ORIGIN_SNAP;
        let eye_rel = cam_pos - o;
        let vp_rel = proj_mat * Mat4::look_at_rh(eye_rel, eye_rel + cam_forward, cam_up);
        let frustum = FrustumPlanes::from_view_proj(vp_abs);
        (vp_rel.to_cols_array(), frustum, vp_abs)
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
