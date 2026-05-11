//! Render-layer classification for depth-bias ladder.
//!
//! Bethesda content carries three implicit content layers — *Architecture*
//! (large fixtures: walls, floors, fireplaces), *Clutter* (small items
//! resting on architecture: papers, books, ammo), *Actors* (NPCs / creatures
//! standing on floors). Gamebryo's renderer enforced layer priority via
//! render-order tricks the unified RT pipeline doesn't replicate, so
//! coplanar-stacked content z-fights against the surfaces underneath
//! (rugs zebra-striped on hardwood, papers clipping desks, NPC feet
//! sinking into floors).
//!
//! [`RenderLayer`] is the explicit classification, attached as a sparse
//! component to every renderable entity at cell-load time. The renderer
//! reads it and applies the per-layer depth-bias ladder via
//! `vkCmdSetDepthBias`, replacing the ad-hoc `is_decal || alpha_test_func != 0`
//! heuristic from commits `0f13ff5` / `ee3cb13`. Single source of truth,
//! game-invariant (Oblivion through Starfield use the same record-type →
//! layer mapping; per-game record availability differs but the mapping
//! itself doesn't).
//!
//! See `crates/plugin/src/record.rs::RecordType::render_layer` for the
//! base-record classifier, and `byroredux/src/cell_loader.rs` for the
//! spawn-site escalation rule (alpha-tested or NIF-decal-flagged meshes
//! escalate to [`RenderLayer::Decal`] regardless of base record).

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// Content-layer classification for depth-bias purposes. Lower numeric
/// value = lower priority (drawn deeper); higher value = wins more
/// z-fights. Default [`RenderLayer::Architecture`] yields zero bias —
/// identical to pre-#renderlayer behaviour for any entity that never
/// gets the component attached.
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum RenderLayer {
    /// Walls, floors, fireplaces, doors, plants rooted in ground,
    /// terrain, vending machines, wall-mounted lamps, terminals,
    /// containers/footlockers built into the level. The base layer that
    /// owns the depth buffer; zero bias.
    Architecture = 0,
    /// Player-pickup-able items: weapons, armor, ammo, misc clutter,
    /// keys, alchemy, ingredients, books, notes. Small things that
    /// rest on top of architecture and need a tiny depth bias to win
    /// the coplanar z-fight against the surface beneath.
    Clutter = 1,
    /// NPCs and creatures. Their feet plant on floors at exactly the
    /// floor's Y; without bias every standing NPC z-fights the floor at
    /// the foot-plant patch.
    Actor = 2,
    /// True decals (NIF-flagged blood splats, scorch marks, bullet holes)
    /// AND alpha-tested overlays (rugs, posters, fences, cutout foliage)
    /// — anything authored to lie flat against another surface. Strongest
    /// bias. The escalation rule lives at the cell-loader spawn site:
    /// `mesh.is_decal || mesh.alpha_test_func != 0` → `RenderLayer::Decal`
    /// regardless of base record type.
    Decal = 3,
}

impl RenderLayer {
    /// The Vulkan `vkCmdSetDepthBias` triple `(constant_factor, clamp,
    /// slope_factor)` per layer. Conservative ladder — `Decal` is the
    /// proven anchor (`(-64, 0, -2)` from commit `0f13ff5`); the
    /// intermediate layers ramp linearly between zero and the Decal
    /// anchor.
    ///
    /// The Vulkan formula is `bias = constant_factor × r + slope_factor
    /// × |max_dz/dxy|` where `r ≈ 2⁻²⁴ ≈ 6e-8` for D32_SFLOAT at typical
    /// depth values. With `Decal = (-64, 0, -2)` the offset works out to
    /// roughly `4e-6` of normalised depth — same scale Bethesda's D3D
    /// engines use for decal polygon offset, big enough to win every
    /// coplanar tie, small enough that distant content doesn't poke
    /// through occluders.
    pub const fn depth_bias(self) -> (f32, f32, f32) {
        match self {
            RenderLayer::Architecture => (0.0, 0.0, 0.0),
            RenderLayer::Clutter => (-16.0, 0.0, -1.0),
            RenderLayer::Actor => (-32.0, 0.0, -1.5),
            RenderLayer::Decal => (-64.0, 0.0, -2.0),
        }
    }
}

impl Default for RenderLayer {
    fn default() -> Self {
        Self::Architecture
    }
}

/// Apply the spawn-site Decal escalation: any mesh that's NIF-flagged
/// as a decal OR carries an active alpha test (NiAlphaProperty
/// `alpha_test` bit set on the source record) is meant to lie flat
/// against another surface. Bias it as [`RenderLayer::Decal`]
/// regardless of the base record's classification. Otherwise pass
/// `base` through unchanged.
///
/// This is the single rule used at every cell-loader / scene spawn
/// site, so the FNV-rug fix from commit `ee3cb13` (alpha-test → strong
/// bias) is preserved end-to-end while the per-base-record classification
/// from [`RenderLayer::Architecture`] / [`RenderLayer::Clutter`] /
/// [`RenderLayer::Actor`] takes over for everything else.
///
/// Important — this gate uses the `alpha_test: bool` from
/// [`crate::ecs::components::Material`] (or `ImportedMesh::alpha_test`
/// at spawn time), NOT `alpha_test_func: u8`. The Gamebryo default
/// for `alpha_test_func` is `6` (GREATEREQUAL) and lands on every
/// imported material regardless of whether alpha-testing is actually
/// enabled — using the func value as the gate would escalate every
/// architectural mesh in the cell. The `alpha_test` bool is the
/// authoritative "is testing actually on" signal.
pub const fn render_layer_with_decal_escalation(
    base: RenderLayer,
    mesh_is_decal: bool,
    mesh_alpha_test_enabled: bool,
) -> RenderLayer {
    if mesh_is_decal || mesh_alpha_test_enabled {
        RenderLayer::Decal
    } else {
        base
    }
}

/// World-space bounding-sphere radius (in Bethesda units) under which a
/// [`RenderLayer::Architecture`] mesh is reclassified as
/// [`RenderLayer::Clutter`]. See [`escalate_small_static_to_clutter`].
///
/// Calibrated against vanilla FNV content. 1 Bethesda unit ≈ 1.43 cm
/// (1 yard = 64 units), so 50 units ≈ 71 cm — a sphere that comfortably
/// encloses every desktop-clutter STAT (paper piles, folders, clipboards,
/// books, photo frames, ashtrays, lamps; bounding-sphere radii observed
/// in the 5-25-unit range) while staying below the smallest architectural
/// pieces (door panels ≈ 48 units, wall sections ≥ 128, railing posts
/// ≈ 35 are the only borderline candidates and tolerate a -16 / -1.0
/// nudge against the floor without visual impact).
pub const SMALL_STATIC_RADIUS_UNITS: f32 = 50.0;

/// Reclassify small STAT meshes from Architecture to Clutter so they win
/// the coplanar z-fight against the surface they rest on. In vanilla
/// content, decorative props authored as `STAT` records (papers, folders,
/// clipboards, ashtrays, photo frames, etc.) inherit
/// [`RenderLayer::Architecture`] from the [`RecordType`] classifier — but
/// they sit on top of desks / shelves / floors at exactly that surface's
/// Y, so they z-fight the way pickup-clutter MISC items used to before
/// the layer ladder. The base-record classifier can't tell pickup-clutter
/// (`MISC`) from decorative-clutter (`STAT`) because Bethesda authored
/// both forms — only the spatial extent does.
///
/// Threshold lives in [`SMALL_STATIC_RADIUS_UNITS`] so the calibration
/// is one tweakable constant.
///
/// Only escalates Architecture; Clutter / Actor / Decal pass through
/// unchanged (we never want to demote, and a small Actor — child NPC,
/// small creature — should keep its Actor bias against the floor).
///
/// `world_bound_radius` should already include the REFR's `ref_scale`
/// applied to `ImportedMesh::local_bound_radius`. The call site doing
/// `mesh.local_bound_radius * ref_scale` is the contract.
///
/// [`RecordType`]: crate::ecs::components::RenderLayer
pub fn escalate_small_static_to_clutter(base: RenderLayer, world_bound_radius: f32) -> RenderLayer {
    if matches!(base, RenderLayer::Architecture)
        && world_bound_radius > 0.0
        && world_bound_radius < SMALL_STATIC_RADIUS_UNITS
    {
        RenderLayer::Clutter
    } else {
        base
    }
}

impl Component for RenderLayer {
    type Storage = SparseSetStorage<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bias-table monotonicity guard. Any future reordering that puts
    /// (e.g.) Actor's constant factor below Decal's would break the
    /// "stronger layer wins more z-fights" invariant the rest of the
    /// system depends on.
    #[test]
    fn depth_bias_constants_monotone() {
        let arch = RenderLayer::Architecture.depth_bias().0;
        let clutter = RenderLayer::Clutter.depth_bias().0;
        let actor = RenderLayer::Actor.depth_bias().0;
        let decal = RenderLayer::Decal.depth_bias().0;
        assert!(
            arch > clutter && clutter > actor && actor > decal,
            "bias ladder must descend Architecture > Clutter > Actor > Decal: \
             got {arch} > {clutter} > {actor} > {decal}"
        );
    }

    /// Slope factors should also descend (or at least not invert) so
    /// angled surfaces honour the same priority order.
    #[test]
    fn depth_bias_slopes_monotone() {
        let s_arch = RenderLayer::Architecture.depth_bias().2;
        let s_clutter = RenderLayer::Clutter.depth_bias().2;
        let s_actor = RenderLayer::Actor.depth_bias().2;
        let s_decal = RenderLayer::Decal.depth_bias().2;
        assert!(s_arch >= s_clutter && s_clutter >= s_actor && s_actor >= s_decal);
    }

    /// `Architecture` is zero so absent-component fallback (via
    /// `unwrap_or_default()` in the renderer) is identical to pre-#renderlayer
    /// behaviour for any entity that never gets the component.
    #[test]
    fn architecture_is_zero_bias() {
        assert_eq!(RenderLayer::Architecture.depth_bias(), (0.0, 0.0, 0.0));
    }

    /// `Decal` matches the proven-working `(-64, 0, -2)` anchor from
    /// commit `0f13ff5` so the rug-z-fight fix is preserved byte-for-byte.
    #[test]
    fn decal_matches_proven_anchor() {
        assert_eq!(RenderLayer::Decal.depth_bias(), (-64.0, 0.0, -2.0));
    }

    #[test]
    fn default_is_architecture() {
        assert_eq!(RenderLayer::default(), RenderLayer::Architecture);
    }

    /// `repr(u8)` so the variant can be packed into shader-side flag
    /// words for the debug-viz bit `0x40`. Locks the layout.
    #[test]
    fn repr_is_u8() {
        assert_eq!(std::mem::size_of::<RenderLayer>(), 1);
        assert_eq!(RenderLayer::Architecture as u8, 0);
        assert_eq!(RenderLayer::Clutter as u8, 1);
        assert_eq!(RenderLayer::Actor as u8, 2);
        assert_eq!(RenderLayer::Decal as u8, 3);
    }

    // ── Spawn-site escalation rule (#renderlayer) ─────────────────────
    //
    // Single rule called at every cell-loader / scene spawn site so
    // the alpha-test-on-clutter case (the FNV rug regression that
    // motivated commit `ee3cb13`) is preserved without having to
    // re-derive it per-spawn-site.

    #[test]
    fn nif_decal_flag_escalates_clutter_to_decal() {
        let r = render_layer_with_decal_escalation(RenderLayer::Clutter, true, false);
        assert_eq!(r, RenderLayer::Decal);
    }

    #[test]
    fn alpha_test_escalates_architecture_to_decal() {
        // FNV `rugsmall01.nif` is a STAT (Architecture base) with
        // `NiAlphaProperty.alpha_test = true`. The escalation must
        // fire even when the base is Architecture so the rug wins its
        // z-fight against the floor.
        let r = render_layer_with_decal_escalation(RenderLayer::Architecture, false, true);
        assert_eq!(r, RenderLayer::Decal);
    }

    #[test]
    fn no_decal_no_alpha_test_passes_base_through() {
        // Plain MISC clutter on a desk — no NIF decal, no alpha-test.
        // Stays at Clutter (small bias for the desktop z-fight).
        let r = render_layer_with_decal_escalation(RenderLayer::Clutter, false, false);
        assert_eq!(r, RenderLayer::Clutter);
        // Plain wall — Architecture.
        let r = render_layer_with_decal_escalation(RenderLayer::Architecture, false, false);
        assert_eq!(r, RenderLayer::Architecture);
        // Plain NPC body part — Actor.
        let r = render_layer_with_decal_escalation(RenderLayer::Actor, false, false);
        assert_eq!(r, RenderLayer::Actor);
    }

    /// Critical regression guard — the early implementation gated on
    /// `alpha_test_func != 0` instead of the bool, but the
    /// Gamebryo default for `alpha_test_func` is `6` (GREATEREQUAL)
    /// regardless of whether alpha-testing is actually enabled. That
    /// bug escalated every mesh in the cell to Decal at spawn time,
    /// caught by the live FNV viz screenshot showing all-yellow.
    /// Pin the correct gate: only the `alpha_test_enabled` bool
    /// triggers escalation; `alpha_test_func` is irrelevant here.
    #[test]
    fn alpha_test_disabled_does_not_escalate_regardless_of_default_func() {
        // FNV importer ships `MaterialInfo::default().alpha_test_func = 6`
        // (GREATEREQUAL — Gamebryo default). That default value rides
        // through to every mesh whose source NIF lacks a
        // `NiAlphaProperty`. Those meshes must NOT escalate to Decal —
        // they're plain opaque architecture.
        let r = render_layer_with_decal_escalation(RenderLayer::Architecture, false, false);
        assert_eq!(r, RenderLayer::Architecture);
    }

    // ── Small-STAT escalation rule (#renderlayer follow-up) ────────────
    //
    // Bethesda authors decorative desk-clutter (paper piles, folders,
    // clipboards) as STAT records — the base classifier puts them in
    // Architecture (zero bias) and they z-fight the desk surface
    // beneath. Spatial extent is the only signal that distinguishes
    // them from real architecture; `escalate_small_static_to_clutter`
    // moves Architecture meshes whose world-space bounding-sphere
    // radius is below `SMALL_STATIC_RADIUS_UNITS` into the Clutter
    // bias band.

    #[test]
    fn small_static_escalates_to_clutter() {
        // 12-unit radius — ~17 cm in real-world terms; matches a folder
        // / clipboard / paper-pile bounding sphere observed in vanilla
        // FNV STAT meshes.
        let r = escalate_small_static_to_clutter(RenderLayer::Architecture, 12.0);
        assert_eq!(r, RenderLayer::Clutter);
    }

    #[test]
    fn large_static_stays_architecture() {
        // 200-unit radius — comfortably bigger than any clutter prop;
        // matches a wall section or floor tile.
        let r = escalate_small_static_to_clutter(RenderLayer::Architecture, 200.0);
        assert_eq!(r, RenderLayer::Architecture);
    }

    #[test]
    fn small_static_threshold_is_strict_lower_bound() {
        // Exactly at the threshold must NOT escalate (strictly less).
        // Pinning the comparator so a future `>=` slip doesn't let
        // borderline architecture leak into Clutter.
        let r =
            escalate_small_static_to_clutter(RenderLayer::Architecture, SMALL_STATIC_RADIUS_UNITS);
        assert_eq!(r, RenderLayer::Architecture);
    }

    #[test]
    fn small_static_zero_radius_does_not_escalate() {
        // A zero radius means the mesh has no extracted bounds (NIF
        // bound was zero AND the vertex-position fallback produced
        // nothing). Don't pretend that's "small" — leave it as
        // Architecture so a real STAT with missing bounds doesn't
        // start drawing in front of its neighbors.
        let r = escalate_small_static_to_clutter(RenderLayer::Architecture, 0.0);
        assert_eq!(r, RenderLayer::Architecture);
    }

    #[test]
    fn small_radius_does_not_demote_higher_layers() {
        // Idempotent for non-Architecture bases: a small Actor
        // (child NPC, small creature) keeps its Actor bias; small
        // Decal stays Decal; already-Clutter stays Clutter.
        let r = escalate_small_static_to_clutter(RenderLayer::Clutter, 5.0);
        assert_eq!(r, RenderLayer::Clutter);
        let r = escalate_small_static_to_clutter(RenderLayer::Actor, 5.0);
        assert_eq!(r, RenderLayer::Actor);
        let r = escalate_small_static_to_clutter(RenderLayer::Decal, 5.0);
        assert_eq!(r, RenderLayer::Decal);
    }

    /// Composition order at the spawn site: small-STAT escalation runs
    /// first, decal escalation second. Verify the two orderings produce
    /// the right final layer:
    ///   small + alpha-test STAT → Architecture → Clutter (size) → Decal
    ///   large + alpha-test STAT (rug) → Architecture → Architecture → Decal
    #[test]
    fn size_then_decal_composition_order() {
        // Small alpha-tested STAT → Decal wins over Clutter.
        let l = escalate_small_static_to_clutter(RenderLayer::Architecture, 10.0);
        let l = render_layer_with_decal_escalation(l, false, true);
        assert_eq!(l, RenderLayer::Decal);

        // Large alpha-tested STAT (the rug case from `ee3cb13`) →
        // skips Clutter, lands on Decal anyway.
        let l = escalate_small_static_to_clutter(RenderLayer::Architecture, 200.0);
        let l = render_layer_with_decal_escalation(l, false, true);
        assert_eq!(l, RenderLayer::Decal);
    }

    #[test]
    fn already_decal_stays_decal() {
        // Idempotent: passing `RenderLayer::Decal` as the base with
        // any escalation flag still returns Decal.
        let r = render_layer_with_decal_escalation(RenderLayer::Decal, true, true);
        assert_eq!(r, RenderLayer::Decal);
        let r = render_layer_with_decal_escalation(RenderLayer::Decal, false, false);
        assert_eq!(r, RenderLayer::Decal);
    }
}
