//! `ContactConfig` — engine-wide physics tunables that previously lived
//! as inline literals at three sites:
//!
//! 1. `convert.rs::flatten_to_parts` (TriMesh flags)
//! 2. `sync.rs::register_newcomers` (per-collider contact skin, defaults)
//! 3. `world.rs::PhysicsWorld::move_character` (KCC offset, autostep mins)
//!
//! Promoting these to a `Resource` keeps the rule "all TriMesh statics
//! get the same contact-generation treatment" enforceable in one place.
//! Bumping the character-controller offset for a wider-clearance test
//! becomes a single field write, not a hunt through three crates.
//!
//! Defaults match the values that were inline before the unification:
//! - `trimesh_flags = FIX_INTERNAL_EDGES` (which transitively ORs in
//!   `ORIENTED | MERGE_DUPLICATE_VERTICES` — see parry3d-0.17.6/src/shape/trimesh.rs:276).
//! - `default_contact_skin_bu = 1.0` (Rapier collider margin — was 0
//!   implicitly; now explicit so the narrow phase has a stable gap to
//!   resolve from).
//! - `kcc_offset_bu = 4.0` (was `controller.offset` at `world.rs:285`).

use byroredux_core::ecs::resource::Resource;

/// TriMesh flag set as plain bits so `core` types don't need to alias
/// rapier types. Matches `rapier3d::parry::shape::TriMeshFlags` (u16)
/// 1:1 — the physics crate consumes this via
/// `TriMeshFlags::from_bits_truncate`.
///
/// `FIX_INTERNAL_EDGES`'s definition includes `ORIENTED` and
/// `MERGE_DUPLICATE_VERTICES` transitively (parry3d:276), so the default
/// here gives all three.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TriMeshFlagBits(pub u16);

impl TriMeshFlagBits {
    /// Mirrors `rapier3d::parry::shape::TriMeshFlags::ORIENTED`.
    pub const ORIENTED: u16 = 1 << 3;
    /// Mirrors `rapier3d::parry::shape::TriMeshFlags::MERGE_DUPLICATE_VERTICES`.
    pub const MERGE_DUPLICATE_VERTICES: u16 = 1 << 4;
    /// Mirrors `rapier3d::parry::shape::TriMeshFlags::FIX_INTERNAL_EDGES`,
    /// which transitively ORs in `ORIENTED | MERGE_DUPLICATE_VERTICES`.
    pub const FIX_INTERNAL_EDGES: u16 = (1 << 7) | Self::ORIENTED | Self::MERGE_DUPLICATE_VERTICES;

    pub const DEFAULT: Self = Self(Self::FIX_INTERNAL_EDGES);
}

impl Default for TriMeshFlagBits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Engine-wide physics tunables. A bug fix to TriMesh contact
/// generation, KCC offset, or default collider margin lands here and
/// propagates through every spawn path uniformly.
#[derive(Debug, Clone, Copy)]
pub struct ContactConfig {
    /// Flags applied to every `CollisionShape::TriMesh` at collider
    /// creation. Default: `FIX_INTERNAL_EDGES` (which transitively ORs
    /// in `ORIENTED | MERGE_DUPLICATE_VERTICES`).
    pub trimesh_flags: TriMeshFlagBits,

    /// Per-collider contact skin (Rapier collider margin), in BU. The
    /// narrow phase resolves contacts within this distance; a non-zero
    /// value gives the solver a stable gap to push out of penetration
    /// instead of teleporting on first overlap. 1 BU ≈ 1.4 cm at the
    /// Bethesda-unit scale, narrow enough that visible geometry still
    /// touches but wide enough to keep TriMesh seams from leaking the
    /// kinematic player through.
    pub default_contact_skin_bu: f32,

    /// `KinematicCharacterController.offset` distance in BU. Was 4.0
    /// before unification.
    pub kcc_offset_bu: f32,

    /// Extra angular damping added to every ragdoll body on top of the
    /// authored Havok value (M41.x). The single biggest "less floppy /
    /// less clunky than the original Havok ragdoll" lever — raise it to
    /// settle limbs faster. `0.0` = pure Havok-authored damping (inert
    /// default); ~1–3 gives a noticeably calmer death/hit ragdoll.
    pub ragdoll_extra_angular_damping: f32,
}

impl ContactConfig {
    pub const DEFAULT: Self = Self {
        trimesh_flags: TriMeshFlagBits::DEFAULT,
        default_contact_skin_bu: 1.0,
        kcc_offset_bu: 4.0,
        ragdoll_extra_angular_damping: 0.0,
    };
}

impl Default for ContactConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Resource for ContactConfig {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trimesh_flag_bits_match_rapier_definitions() {
        // Pin the bit values against rapier's TriMeshFlags so a rapier
        // upgrade that reorders the flags doesn't silently change what
        // we apply at collider creation.
        use rapier3d::parry::shape::TriMeshFlags;
        assert_eq!(TriMeshFlagBits::ORIENTED, TriMeshFlags::ORIENTED.bits());
        assert_eq!(
            TriMeshFlagBits::MERGE_DUPLICATE_VERTICES,
            TriMeshFlags::MERGE_DUPLICATE_VERTICES.bits()
        );
        assert_eq!(
            TriMeshFlagBits::FIX_INTERNAL_EDGES,
            TriMeshFlags::FIX_INTERNAL_EDGES.bits()
        );
    }

    #[test]
    fn default_trimesh_flags_include_fix_internal_edges() {
        let f = TriMeshFlagBits::default();
        assert_eq!(
            f.0 & TriMeshFlagBits::FIX_INTERNAL_EDGES,
            TriMeshFlagBits::FIX_INTERNAL_EDGES,
            "FIX_INTERNAL_EDGES (and its transitive ORIENTED + MERGE_DUPLICATE_VERTICES) must be on by default"
        );
    }

    #[test]
    fn default_contact_config_matches_previous_inline_values() {
        let c = ContactConfig::default();
        assert_eq!(c.kcc_offset_bu, 4.0, "must match world.rs:285 value");
        assert!(c.default_contact_skin_bu >= 0.0);
    }
}
