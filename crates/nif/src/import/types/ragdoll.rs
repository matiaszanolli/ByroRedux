//! Imported Havok ragdoll articulations.

use super::*;

/// A Havok ragdoll articulation, engine-native (Y-up, havok-scaled).
///
/// `bodies` and `constraints` form a kinematic tree: each constraint
/// links two `bodies` by array index. Built by
/// [`crate::import::collision::extract_ragdoll`] from the NIF's
/// `BhkRigidBody` + `BhkConstraint` blocks. See M41.x.
#[derive(Debug, Clone)]
pub struct ImportedRagdoll {
    pub bodies: Vec<ImportedRagdollBody>,
    pub constraints: Vec<ImportedRagdollConstraint>,
}

/// One rigid body of a ragdoll, hosted on a skeleton bone.
#[derive(Debug, Clone)]
pub struct ImportedRagdollBody {
    /// Host bone name (the NiNode that carries this body's collision
    /// object). The engine resolves it to the bone `EntityId` to seed and
    /// write back the simulated pose.
    pub bone_name: Arc<str>,
    pub mass: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub friction: f32,
    pub restitution: f32,
    /// Collider shape in body-local space (Y-up, havok-scaled).
    pub shape: CollisionShape,
    /// Rigid-body origin **relative to the host bone's NiNode** (Y-up,
    /// scaled) — the same node-local reading `extract_from_classic` applies
    /// to architecture colliders. Used verbatim as the body's bone-local
    /// offset when building the runtime ragdoll template. Only meaningful
    /// when [`Self::is_t`] is `true` — see that field.
    ///
    /// #3318 corrected this: it was previously documented (and consumed) as a
    /// skeleton-root/rest-space pose per #2336, which the authored data
    /// falsifies — FNV's median authored magnitude is 8.6 units, limb-scale,
    /// not whole-skeleton-scale.
    pub translation: Vec3,
    /// Rigid-body orientation in skeleton-root/rest space (Y-up). Only
    /// meaningful when [`Self::is_t`] is `true` — see that field.
    pub rotation: Quat,
    /// FO4+: the ragdoll-skeleton bone id this body drives, which an `XRGD`
    /// corpse pose names its entries by (#5015). `None` for the classic
    /// `bhkRigidBody` path, whose pose entries follow body order.
    pub pose_bone_id: Option<u8>,
    /// `true` only if the source block was `bhkRigidBodyT`. Mirrors
    /// `BhkRigidBody::is_t` (#2316): plain `bhkRigidBody` carries the same
    /// wire-format `translation`/`rotation` fields, but Gamebryo treats them
    /// as identity — the engine must not resolve [`Self::translation`] /
    /// [`Self::rotation`] against the bone rest pose when this is `false`,
    /// instead treating the body as coincident with its host bone's own
    /// rest transform (zero local offset). #2447 / PHYS-01.
    pub is_t: bool,
}

/// One joint linking two ragdoll bodies (indices into
/// [`ImportedRagdoll::bodies`]).
#[derive(Debug, Clone)]
pub struct ImportedRagdollConstraint {
    pub body_a: usize,
    pub body_b: usize,
    pub kind: ImportedJointKind,
}

/// Joint geometry, converted to engine space. Pivots are positions
/// (Y-up, scaled); axes are unit directions (Y-up, unscaled). Angles are
/// radians, passed through from Havok.
#[derive(Debug, Clone)]
pub enum ImportedJointKind {
    /// 3-DOF cone/twist ball joint (`bhkRagdollConstraint`).
    Ragdoll {
        twist_a: Vec3,
        plane_a: Vec3,
        pivot_a: Vec3,
        twist_b: Vec3,
        plane_b: Vec3,
        pivot_b: Vec3,
        cone_max: f32,
        plane_min: f32,
        plane_max: f32,
        twist_min: f32,
        twist_max: f32,
    },
    /// 1-DOF angle-limited hinge (`bhkLimitedHingeConstraint`).
    LimitedHinge {
        axis_a: Vec3,
        /// Authored zero-angle reference direction for side A
        /// (`Perp Axis In A1`), orthogonalised against `axis_a` at the
        /// solver boundary. Defines the plane `min_angle`/`max_angle` are
        /// measured from — without it the solver must synthesize an
        /// arbitrary reference frame, rotating the enforced angle window
        /// by an uncontrolled amount from what the content author
        /// intended (#2448 / PHYS-02).
        perp_a: Vec3,
        pivot_a: Vec3,
        axis_b: Vec3,
        /// Authored zero-angle reference direction for side B
        /// (`Perp Axis In B1`). Zeroed (not authored) on Oblivion/
        /// Morrowind content — the solver boundary's `frame_rot` falls
        /// back to a synthesized perpendicular for a degenerate/zero
        /// input, so this stays safe either way.
        perp_b: Vec3,
        pivot_b: Vec3,
        min_angle: f32,
        max_angle: f32,
    },
    /// 1-DOF sliding rail (`bhkPrismaticConstraint`, #3792). All three
    /// rotation axes and the two non-sliding translation axes are fixed;
    /// the body can only translate along `axis_a`/`axis_b` between
    /// `min_distance`/`max_distance`.
    Prismatic {
        axis_a: Vec3,
        /// Authored zero-angle reference direction for side A
        /// (`Rotation A`) — orthogonalised against `axis_a` at the solver
        /// boundary, same role as `LimitedHinge::perp_a`.
        perp_a: Vec3,
        pivot_a: Vec3,
        axis_b: Vec3,
        perp_b: Vec3,
        pivot_b: Vec3,
        min_distance: f32,
        max_distance: f32,
    },
}
