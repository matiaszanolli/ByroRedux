//! Camera component and ActiveCamera resource.

use crate::ecs::resource::Resource;
use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::{Component, EntityId};
use crate::math::{Mat4, Vec3};

use super::transform::Transform;

/// Shared geometry render distance for every scene type.
///
/// Interior cells do not get a shorter projection: they use the same camera
/// and frustum contract as streamed exterior worldspaces. Exterior LOD
/// production decides what geometry exists at long range; it does not change
/// the camera far plane.
///
/// **It must, however, reach as far as that geometry.** The distant-LOD ring
/// is a square centred on the player, so its furthest visible point is a
/// corner at `reach · √2`, and the frustum's far plane is extracted from the
/// view-projection matrix — geometry past it is *culled*, not merely clipped,
/// so whole LOD blocks wink out at the diagonals. The binding constraint is
/// the widest ring any supported game streams:
///
/// | Ring | Reach | Corner (`·√2`) |
/// |---|---|---|
/// | Synthesized (Oblivion / FO3 / FNV) | 48 cells = 196 608 BU | 278 046 BU |
/// | Baked ladder (Skyrim / FO4) | 61 cells = 249 856 BU | 353 350 BU |
///
/// 400 000 covers the 353 350 corner with ~13% left for terrain relief above
/// and below the camera. `cell_loader::terrain_lod` const-asserts this
/// against its own `MAX_LOD_RING_REACH_CELLS`, so retuning either side fails
/// the build rather than silently clipping the horizon.
///
/// ## Depth-precision policy
///
/// This far plane against the 0.1 near plane is a 4 000 000:1 depth range on
/// a `D32_SFLOAT` buffer with a conventional (non-reversed) 0→1 mapping —
/// the arrangement that wastes float depth most thoroughly, because distant
/// samples crowd into the region near 1.0 where f32 steps are coarsest.
/// [`Camera::depth_resolution_at`] quantifies it: **~37 250 world units per
/// depth step out at the 250 000 BU LOD ring** (and ~23 000 already at the
/// old synth ring's 196 608 BU), i.e. effectively no depth discrimination
/// between distant terrain and the object LOD standing on it. Raising the
/// near plane to 5.0 measures at 745 units over the same span — the ~50×
/// noted below.
///
/// Two ways out, and only one of them is available cheaply:
///
/// * **Raising the near plane** is the usual first lever, and every shipped
///   Gamebryo game does exactly that (`fNearDistance` is 5 in
///   `Fallout_default.ini`, 10 in `Oblivion_default.ini`, versus 0.1 here) —
///   it would buy ~50×. It cannot be done as a single global constant while
///   one camera contract serves every scene: `scene.rs`'s no-content fallback
///   parks the camera 4 units from the origin, so a 5-unit near plane would
///   swallow the whole demo scene. Making `near` scale-aware is a real design
///   change, not a constant edit.
/// * **Reversed-Z** (far→0 plus `GREATER_OR_EQUAL` depth compare) is the
///   actual fix, and pairs with `D32_SFLOAT` to give near-uniform world-space
///   resolution. It is deliberately *not* done here: it touches the
///   projection, the depth clear, pipeline compare state, and every depth
///   consumer (SSAO, SVGF reprojection, TAA, composite, water, FSR3
///   linearization), and none of those failure modes are visible to
///   `cargo test`. It needs a GPU capture to land safely — the same gate
///   EX-11 still owes. See #2371.
pub const DEFAULT_RENDER_DISTANCE: f32 = 400_000.0;

/// Perspective camera parameters.
///
/// Attach to an entity that also has a [`Transform`] component.
/// The entity's Transform determines the camera's position and orientation.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct Camera {
    /// Vertical field of view in radians.
    pub fov_y: f32,
    /// Near clipping plane distance.
    pub near: f32,
    /// Far clipping plane distance.
    pub far: f32,
    /// Viewport aspect ratio (width / height). Updated on window resize.
    pub aspect: f32,
    /// Lens aperture half-radius in world units. `0.0` = pinhole camera (DOF disabled).
    /// The renderer jitters the camera position within a disk of this radius each frame;
    /// TAA accumulates the samples to produce a spatially-varying blur — surfaces at
    /// `focus_dist` are sharp, surfaces at other depths are progressively blurred.
    pub aperture: f32,
    /// Distance to the focal plane in world units. Surfaces at this depth are in sharp focus.
    /// Ignored when `aperture == 0.0`.
    pub focus_dist: f32,
}

impl Camera {
    pub fn new(fov_y: f32, aspect: f32, near: f32, far: f32) -> Self {
        Self {
            fov_y,
            near,
            far,
            aspect,
            aperture: 0.0,
            focus_dist: 20.0,
        }
    }

    /// Build a perspective projection matrix (Vulkan clip space: Y-down, Z 0..1).
    pub fn projection_matrix(&self) -> Mat4 {
        // glam's perspective_rh already maps Z to [0, 1] (Vulkan/D3D convention).
        // Only the Y-flip is needed for Vulkan's inverted Y axis.
        // Note: the Y-flip reverses apparent triangle winding in clip space —
        // CW triangles (NIF/D3D) appear CCW after this, matching our front face setting.
        let mut proj = Mat4::perspective_rh(self.fov_y, self.aspect, self.near, self.far);
        proj.col_mut(1).y *= -1.0;
        proj
    }

    /// Smallest world-space distance this camera's depth buffer can still
    /// resolve at `distance` in front of the eye, in world units.
    ///
    /// Assumes the pipeline's actual configuration: a `D32_SFLOAT` buffer
    /// with the conventional near→0 / far→1 mapping [`Self::projection_matrix`]
    /// produces. Two surfaces closer together than the returned value land on
    /// the same depth sample and z-fight.
    ///
    /// Derivation — for that mapping, `z_ndc(d) = f/(f-n) · (1 - n/d)`, whose
    /// slope is `f·n / ((f-n)·d²)`. Inverting it against one f32 step (taken
    /// at the actual encoded value, so the exponent is the real one rather
    /// than an assumed worst case) gives the world-space span of a single
    /// depth increment.
    ///
    /// This is the measurement behind the reversed-Z note on
    /// [`DEFAULT_RENDER_DISTANCE`]. Returns `0.0` for degenerate inputs
    /// (`distance` at or inside the near plane, or a non-positive depth
    /// range) — there is no meaningful resolution to report there.
    pub fn depth_resolution_at(&self, distance: f32) -> f32 {
        let (n, f) = (self.near, self.far);
        // Reject NaN/inf first so the ordering comparisons below are total.
        if !(distance.is_finite() && n.is_finite() && f.is_finite()) {
            return 0.0;
        }
        if n <= 0.0 || f <= n || distance <= n {
            return 0.0;
        }
        let ndc = (f / (f - n)) * (1.0 - n / distance);
        // One f32 step at the encoded depth value.
        let ulp = f32::from_bits(ndc.to_bits() + 1) - ndc;
        ulp * (f - n) * distance * distance / (f * n)
    }

    /// Build a view matrix from the camera entity's transform.
    ///
    /// The transform's translation is the camera position.
    /// The transform's rotation determines the look direction
    /// (forward is -Z in the camera's local space).
    pub fn view_matrix(transform: &Transform) -> Mat4 {
        let position = transform.translation;
        let forward = transform.rotation * -Vec3::Z;
        let up = transform.rotation * Vec3::Y;
        Mat4::look_at_rh(position, position + forward, up)
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            fov_y: std::f32::consts::FRAC_PI_4, // 45°
            // 0.1 is far nearer than any shipped Gamebryo game clips at,
            // and it is what makes the depth range so lopsided — but one
            // camera contract serves both BU-scale worldspaces and the
            // unit-scale demo/loose-NIF scenes, and the latter sit ~4 units
            // from the origin. See `DEFAULT_RENDER_DISTANCE`'s depth-precision
            // policy for the full reasoning and the measured numbers.
            near: 0.1,
            // Sized to clear the widest distant-LOD ring's far-corner
            // diagonal; `cell_loader::terrain_lod` const-asserts the
            // relation. Rationale + depth-precision policy live on the
            // constant.
            far: DEFAULT_RENDER_DISTANCE,
            aspect: 16.0 / 9.0,
            aperture: 0.0,
            focus_dist: 20.0,
        }
    }
}

impl Component for Camera {
    type Storage = SparseSetStorage<Self>;
}

/// Resource indicating which entity is the active camera.
pub struct ActiveCamera(pub EntityId);
impl Resource for ActiveCamera {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{Quat, Vec3, Vec4};
    use std::f32::consts::FRAC_PI_4;

    #[test]
    fn default_camera() {
        let cam = Camera::default();
        assert!((cam.fov_y - FRAC_PI_4).abs() < 1e-6);
        assert!((cam.near - 0.1).abs() < 1e-6);
        assert!((cam.far - DEFAULT_RENDER_DISTANCE).abs() < 1e-6);
    }

    /// #2371 / EX-11 — the far plane must clear the far-corner diagonal of
    /// the widest distant-LOD ring (Skyrim/FO4's 61-cell baked ladder), or
    /// the frustum's far plane culls whole LOD blocks at the diagonals.
    /// `cell_loader::terrain_lod` const-asserts the same relation against its
    /// own reach constant; this pins the core half so the number cannot drift
    /// without a failure on both sides.
    #[test]
    fn far_plane_clears_the_widest_lod_ring_corner() {
        const CELL_UNITS: f32 = 4096.0;
        for (label, cells) in [("synthesized ring", 48.0), ("baked ladder", 61.0)] {
            let corner = cells * CELL_UNITS * 2f32.sqrt();
            assert!(
                DEFAULT_RENDER_DISTANCE > corner,
                "{label}: far plane {DEFAULT_RENDER_DISTANCE} does not reach its \
                 {corner} BU far-corner diagonal"
            );
        }
        // The 61-cell corner is the binding constraint; anything below it is
        // a regression, and the margin above covers terrain relief.
        let binding = 61.0 * CELL_UNITS * 2f32.sqrt();
        assert!((353_300.0..353_400.0).contains(&binding));
        assert!(DEFAULT_RENDER_DISTANCE / binding > 1.1);
    }

    /// Pins the depth-precision budget the reversed-Z note argues from, so
    /// the claim cannot quietly go stale. At the LOD ring the buffer resolves
    /// nothing finer than tens of kilometres — distant terrain and the object
    /// LOD standing on it share a depth sample.
    #[test]
    fn depth_resolution_collapses_at_the_lod_ring() {
        let cam = Camera::default();

        // Close up the buffer is effectively exact.
        assert!(cam.depth_resolution_at(10.0) < 0.01);

        // At the ring a single depth step spans tens of thousands of world
        // units (measured: ~37 250 BU at 250 000 BU out).
        let at_ring = cam.depth_resolution_at(250_000.0);
        assert!(
            (30_000.0..50_000.0).contains(&at_ring),
            "expected a collapsed depth budget at the LOD ring, got {at_ring}"
        );

        // Resolution degrades with the square of distance, so the vanilla
        // near-plane values would buy roughly the ratio they change: a 5.0
        // near plane (Fallout_default.ini's fNearDistance) is ~50× better.
        let raised = Camera {
            near: 5.0,
            ..Camera::default()
        };
        let ratio = at_ring / raised.depth_resolution_at(250_000.0);
        assert!(
            (25.0..100.0).contains(&ratio),
            "a 0.1 → 5.0 near plane should buy roughly 50×, got {ratio}"
        );
    }

    /// Degenerate inputs report `0.0` rather than infinities or NaN — the
    /// helper is called with whatever the active camera holds.
    #[test]
    fn depth_resolution_rejects_degenerate_inputs() {
        let cam = Camera::default();
        assert_eq!(cam.depth_resolution_at(cam.near), 0.0);
        assert_eq!(cam.depth_resolution_at(0.0), 0.0);
        assert_eq!(cam.depth_resolution_at(f32::NAN), 0.0);
        let inverted = Camera::new(FRAC_PI_4, 1.0, 10.0, 1.0);
        assert_eq!(inverted.depth_resolution_at(5.0), 0.0);
        let zero_near = Camera::new(FRAC_PI_4, 1.0, 0.0, 100.0);
        assert_eq!(zero_near.depth_resolution_at(50.0), 0.0);
    }

    #[test]
    fn projection_matrix_is_valid() {
        let cam = Camera::new(FRAC_PI_4, 16.0 / 9.0, 0.1, 100.0);
        let proj = cam.projection_matrix();

        // Near plane should map to Z=0 in Vulkan clip space.
        // Far plane should map to Z=1.
        // Check that the matrix is not all zeros.
        assert!(proj.col(0).x.abs() > 0.0);
        assert!(proj.col(1).y.abs() > 0.0);

        // Y should be flipped for Vulkan (negative).
        assert!(proj.col(1).y < 0.0);
    }

    #[test]
    fn view_matrix_at_origin_looking_forward() {
        let transform = Transform::IDENTITY;
        let view = Camera::view_matrix(&transform);

        // Camera at origin looking down -Z.
        // Point at (0, 0, -5) should be in front of the camera.
        let point = view * Vec4::new(0.0, 0.0, -5.0, 1.0);
        // In view space, the point should have negative Z (in front).
        assert!(point.z < 0.0);
    }

    #[test]
    fn view_matrix_translated() {
        let transform = Transform::from_translation(Vec3::new(0.0, 0.0, 5.0));
        let view = Camera::view_matrix(&transform);

        // Camera at (0, 0, 5) looking down -Z.
        // Origin (0, 0, 0) should be 5 units in front.
        let point = view * Vec4::new(0.0, 0.0, 0.0, 1.0);
        assert!((point.z + 5.0).abs() < 1e-4);
    }

    #[test]
    fn view_matrix_rotated() {
        // Camera rotated 90° around Y — now looking down -X.
        let rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let transform = Transform::from_rotation(rotation);
        let view = Camera::view_matrix(&transform);

        // Point at (-5, 0, 0) should be in front of the camera.
        let point = view * Vec4::new(-5.0, 0.0, 0.0, 1.0);
        assert!(point.z < 0.0);
    }
}
