//! `ContactConfig` — engine-wide physics tunables that previously lived
//! as inline literals at three sites:
//!
//! 1. `convert.rs::flatten_to_parts` (TriMesh flags)
//! 2. `sync.rs::register_newcomers` (per-collider contact skin, defaults)
//! 3. `world/queries.rs`'s `PhysicsWorld::move_character` (KCC offset, autostep mins)
//!
//! Promoting these to a `Resource` keeps the rule "all TriMesh statics
//! get the same contact-generation treatment" enforceable in one place.
//! Bumping the character-controller offset for a wider-clearance test
//! becomes a single field write, not a hunt through three crates.
//!
//! Defaults match the values that were inline before the unification:
//! - `trimesh_flags = FIX_INTERNAL_EDGES | ORIENTED` (plus the
//!   `MERGE_DUPLICATE_VERTICES` that `FIX_INTERNAL_EDGES` implies) — the set
//!   parry 0.17's `FIX_INTERNAL_EDGES` stood for on its own; see
//!   [`TriMeshFlagBits::DEFAULT`].
//! - `default_contact_skin_bu = 1.0` (Rapier collider margin — was 0
//!   implicitly; now explicit so the narrow phase has a stable gap to
//!   resolve from).
//! - `kcc_offset_bu = 4.0` (was `controller.offset`; the cite predates the
//!   #5311 world split).

use byroredux_core::ecs::resource::Resource;
use rapier3d::prelude::CoefficientCombineRule;

/// TriMesh flag set as plain bits so `core` types don't need to alias
/// rapier types. Matches `rapier3d::parry::shape::TriMeshFlags` (u16)
/// 1:1 — the physics crate consumes this via
/// `TriMeshFlags::from_bits_truncate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TriMeshFlagBits(pub u16);

impl TriMeshFlagBits {
    /// Mirrors `rapier3d::parry::shape::TriMeshFlags::ORIENTED`.
    pub const ORIENTED: u16 = 1 << 3;
    /// Mirrors `rapier3d::parry::shape::TriMeshFlags::MERGE_DUPLICATE_VERTICES`.
    pub const MERGE_DUPLICATE_VERTICES: u16 = 1 << 4;
    /// Mirrors `rapier3d::parry::shape::TriMeshFlags::FIX_INTERNAL_EDGES`,
    /// which transitively ORs in `MERGE_DUPLICATE_VERTICES`.
    pub const FIX_INTERNAL_EDGES: u16 = (1 << 7) | Self::MERGE_DUPLICATE_VERTICES;

    /// The engine default: `FIX_INTERNAL_EDGES | ORIENTED`. Until parry 0.19
    /// `FIX_INTERNAL_EDGES` implied `ORIENTED`; it no longer does (the
    /// pseudo-normals are still computed, but the mesh stops being treated
    /// as closed and outward-facing for inside/outside tests). Naming
    /// `ORIENTED` explicitly keeps every collision mesh exactly as it was
    /// built under rapier 0.22, rather than letting the upgrade change it.
    pub const DEFAULT: Self = Self(Self::FIX_INTERNAL_EDGES | Self::ORIENTED);
}

impl Default for TriMeshFlagBits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// How a collider's friction / restitution combine with the other side's
/// when two colliders touch. Havok combines both coefficients as a
/// geometric mean, `sqrt(a * b)` — `hkpMaterial`'s
/// `getFrictionCombineRule` / `getRestitutionCombineRule`
/// (`havok-2013/Physics2012/Dynamics/Common/hkpMaterial.inl:49-57`; the
/// 2007 SDK's copy agrees at `hkpMaterial.inl:38-46`). Rapier's default is
/// `Average`, under which a restitution-0 object bounces off a
/// restitution-0.8 floor at an effective 0.4 — Havok never bounces it. The
/// same holds for friction-0 surfaces. `GeometricMean` also carries
/// rapier's highest combine-rule priority, so any pair with one
/// engine-built collider uses it whatever the other collider asks for.
///
/// Applied at both collider producers — `sync.rs::register_newcomers`
/// (every streamed or imported collider, including the synthesized terrain
/// and architecture trimeshes) and the ragdoll bone builders in
/// `ragdoll.rs` — so they cannot drift apart. Pinned by
/// `restitution_zero_body_does_not_bounce_on_restitution_floor` in
/// `world/mod.rs`.
pub const CONTACT_COEFFICIENT_COMBINE_RULE: CoefficientCombineRule =
    CoefficientCombineRule::GeometricMean;

/// Engine-wide physics tunables. A bug fix to TriMesh contact
/// generation, KCC offset, or default collider margin lands here and
/// propagates through every spawn path uniformly.
#[derive(Debug, Clone, Copy)]
pub struct ContactConfig {
    /// Flags applied to every `CollisionShape::TriMesh` at collider
    /// creation. Default: [`TriMeshFlagBits::DEFAULT`].
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
    ///
    /// **Invariant (#2885): `kcc_offset_bu > 2.0 * default_contact_skin_bu`.**
    /// Two colliders each contribute their own skin, so a player capsule
    /// resting on a floor is separated by `2 * default_contact_skin_bu`; the
    /// KCC offset must clear that or the controller starts every step already
    /// inside the skin-inflated floor — the #2193 "blocked but permanently
    /// ungrounded" configuration. `capsule_center_y_on_surface`
    /// (`byroredux/src/scene.rs`) derives spawn height from this field alone
    /// and does not account for the skin, so the relation is what keeps spawn
    /// placement sound. Pinned by
    /// `kcc_offset_clears_the_combined_contact_skin`.
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
            "FIX_INTERNAL_EDGES (and its transitive MERGE_DUPLICATE_VERTICES) must be on by default"
        );
        // parry 0.19 stopped implying ORIENTED from FIX_INTERNAL_EDGES; the
        // default names it so the meshes built before the rapier 0.36
        // upgrade are built the same way after it.
        assert_eq!(
            f.0 & TriMeshFlagBits::ORIENTED,
            TriMeshFlagBits::ORIENTED,
            "ORIENTED must stay on by default"
        );
    }

    #[test]
    fn default_contact_config_matches_previous_inline_values() {
        let c = ContactConfig::default();
        assert_eq!(c.kcc_offset_bu, 4.0, "must match the pre-split world.rs:285 value");
        assert!(c.default_contact_skin_bu >= 0.0);
        // #2884 — the damping dial is documented as the biggest "less floppy
        // than Havok" lever, so a stray non-zero default would change every
        // ragdoll's feel with nothing failing. Pin the inert default.
        assert_eq!(
            c.ragdoll_extra_angular_damping, 0.0,
            "default must stay inert — non-zero alters every ragdoll's settle \
             behaviour (physal.md §4); opt in per-config instead"
        );
    }

    /// #2885 — the defaults are consistent today, but nothing enforced the
    /// relation, and `ContactConfig`'s own doc invites single-field re-tuning.
    /// Raising `default_contact_skin_bu` to or above half the KCC offset
    /// reproduces #2193 (blocked but permanently ungrounded) with no test
    /// failure anywhere.
    #[test]
    fn kcc_offset_clears_the_combined_contact_skin() {
        let c = ContactConfig::default();
        assert!(
            c.kcc_offset_bu > 2.0 * c.default_contact_skin_bu,
            "kcc_offset_bu ({}) must exceed the combined two-collider skin \
             ({} = 2 x {}); otherwise the character controller begins each \
             step inside the skin-inflated floor (#2193)",
            c.kcc_offset_bu,
            2.0 * c.default_contact_skin_bu,
            c.default_contact_skin_bu,
        );
    }
}
