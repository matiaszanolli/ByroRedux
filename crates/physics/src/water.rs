//! Water physics — buoyancy + flow constants and force math (WATAL Phase 2).
//!
//! This is the *physics sink* of the Water Abstraction Layer
//! ([`docs/engine/watal.md`](../../../docs/engine/watal.md)). Where the
//! renderer consumes `WaterMaterial` for shading, the solver consumes these
//! engine-defined constants + the force helpers here to make dynamic bodies
//! float, drift on currents, and settle on the surface.
//!
//! **Game-invariant by construction.** No game's WATR record authors any
//! physics parameter — Bethesda's buoyancy is engine-internal (WATAL §9 Q2).
//! So [`PhysicsWaterConstants`] is a single engine-canonical resource shared
//! by every game; the per-game translate-up cost for water physics is zero.
//!
//! The force *application* path lives on [`crate::world::PhysicsWorld`]
//! (`add_force` / `apply_impulse` / `reset_forces`); this module supplies the
//! *magnitudes*. Wiring a live `buoyancy_system` into the frame schedule is
//! sequenced with the exterior-reroute physics-freeze fix (shared Rapier
//! wake/sleep discipline — WATAL §7 Phase 2), so it is intentionally absent
//! here; the math below is validated against the real solver in the tests.

use byroredux_core::ecs::resource::Resource;
use byroredux_core::math::Vec3;

/// Engine-canonical water-physics tunables. One instance, game-invariant
/// (see module docs). Inserted as an ECS resource; the buoyancy / flow /
/// swim systems read it instead of hard-coding magnitudes.
#[derive(Debug, Clone, Copy)]
pub struct PhysicsWaterConstants {
    /// Fluid-to-body density ratio (ρ_fluid / ρ_body) for typical clutter.
    /// `> 1` floats, `< 1` sinks, `== 1` is neutral. Default **1.67** from
    /// physical densities — fresh water ≈ 1000 kg/m³ over average wood
    /// ≈ 600 kg/m³ (most Bethesda clutter — crates, bowls, bones — is
    /// wood/cloth/ceramic and floats). An engine tunable, **not** decoded
    /// from any record. Folding volume into this ratio lets
    /// [`buoyancy_force`] stay mass-correct without a separate volume term.
    pub buoyancy_density_ratio: f32,
    /// Linear damping (1/s) applied to a body while submerged, so a
    /// floating body's bob decays and it can sleep at the surface instead
    /// of oscillating forever (water is viscous vs air). Out of water the
    /// body keeps its own authored damping.
    pub linear_damping_in: f32,
    /// Angular damping (1/s) while submerged — water resists tumbling.
    pub angular_damping_in: f32,
}

impl Default for PhysicsWaterConstants {
    fn default() -> Self {
        Self {
            buoyancy_density_ratio: 1.67,
            linear_damping_in: 1.5,
            angular_damping_in: 1.5,
        }
    }
}

impl Resource for PhysicsWaterConstants {}

/// Archimedes buoyancy force on a submerged dynamic body — engine
/// world-space (Y-up), Bethesda-unit "Newtons".
///
/// The upward force is the weight of displaced fluid, expressed relative to
/// the body's own weight so no separate volume term is needed:
///
/// ```text
/// F_up = clamp(submerged_fraction, 0, 1) · density_ratio · mass · |gravity|
/// ```
///
/// - `mass` — body mass (BU³ × density), from [`crate::world::PhysicsWorld::body_mass`].
/// - `submerged_fraction` — 0 (dry) … 1 (fully under), from the displaced
///   collider volume vs the water column.
/// - `gravity_y` — the world gravity Y (negative); only its magnitude is used.
/// - `density_ratio` — ρ_fluid / ρ_body ([`PhysicsWaterConstants::buoyancy_density_ratio`]).
///
/// With `density_ratio > 1` a fully-submerged body rises until it is partly
/// out of the water, dropping `submerged_fraction` to the neutral point
/// `density_ratio · fraction == 1` — a stable floating equilibrium. At
/// `density_ratio · fraction == 1` the returned force exactly equals the
/// body's weight (`mass · |gravity|`), cancelling gravity.
#[inline]
pub fn buoyancy_force(mass: f32, submerged_fraction: f32, gravity_y: f32, density_ratio: f32) -> Vec3 {
    let f = submerged_fraction.clamp(0.0, 1.0) * density_ratio * mass * gravity_y.abs();
    Vec3::new(0.0, f, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::iso_from_trs;
    use crate::world::{PhysicsWorld, PHYSICS_DT};
    use byroredux_core::math::{Quat, Vec3};
    use rapier3d::prelude::*;

    const G_Y: f32 = -686.7; // PhysicsWorld gravity, BU/s².

    #[test]
    fn buoyancy_force_is_zero_when_dry() {
        let f = buoyancy_force(10.0, 0.0, G_Y, 1.67);
        assert_eq!(f.y, 0.0);
    }

    #[test]
    fn buoyancy_force_scales_with_displacement_and_ratio() {
        // Fully submerged, ratio 2 → twice the body's weight, upward.
        let weight = 10.0 * G_Y.abs();
        let f = buoyancy_force(10.0, 1.0, G_Y, 2.0);
        assert!((f.y - 2.0 * weight).abs() < 1e-3, "F = {}", f.y);
        // Half-submerged at ratio 2 → exactly cancels gravity (neutral).
        let f_neutral = buoyancy_force(10.0, 0.5, G_Y, 2.0);
        assert!((f_neutral.y - weight).abs() < 1e-3, "neutral F = {f_neutral:?}");
        // submerged_fraction clamps — 1.5 must not exceed full submersion.
        assert_eq!(buoyancy_force(10.0, 1.5, G_Y, 2.0).y, buoyancy_force(10.0, 1.0, G_Y, 2.0).y);
    }

    /// End-to-end against the real Rapier solver: a dense-clutter-shaped
    /// body released deep below a water surface must rise under buoyancy and
    /// settle near the surface (not sink, not launch into orbit). This
    /// validates the Archimedes magnitude + the `add_force` application path
    /// + submerged-damping settling together.
    #[test]
    fn body_floats_up_and_settles_at_surface() {
        let consts = PhysicsWaterConstants::default();
        let surface = 0.0_f32;
        let radius = 10.0_f32;

        let mut w = PhysicsWorld::new();
        // Start submerged but near the surface — a long runway lets the
        // (lightly-damped, ζ≈0.1) float build hundreds of BU/s and bob for
        // many seconds; releasing it near equilibrium settles quickly while
        // still proving it rises (it starts fully under at y=-50).
        let h = w.bodies.insert(
            RigidBodyBuilder::dynamic()
                .position(iso_from_trs(Vec3::new(0.0, -50.0, 0.0), Quat::IDENTITY))
                .build(),
        );
        w.colliders.insert_with_parent(
            ColliderBuilder::ball(radius).build(),
            h,
            &mut w.bodies,
        );
        // Submerged drag so the float settles instead of bobbing forever.
        w.bodies[h].set_linear_damping(consts.linear_damping_in);
        let mass = w.body_mass(h).expect("mass");

        // Helper: fraction of the ball below the surface for centre `y`.
        let submerged = |y: f32| -> f32 {
            let top = y + radius;
            let bottom = y - radius;
            if bottom >= surface {
                0.0
            } else if top <= surface {
                1.0
            } else {
                (surface - bottom) / (2.0 * radius)
            }
        };

        let y_start = w.bodies[h].translation().y;
        assert_eq!(submerged(y_start), 1.0, "starts fully submerged");

        // ~10 s — long enough for the underdamped float (decay e^(-0.75 t))
        // to settle at the neutral point, re-deriving buoyancy each frame.
        for _ in 0..600 {
            let y = w.bodies[h].translation().y;
            let frac = submerged(y);
            w.reset_forces(h);
            let f = buoyancy_force(mass, frac, G_Y, consts.buoyancy_density_ratio);
            w.add_force(h, f);
            w.step(PHYSICS_DT);
        }

        let y = w.bodies[h].translation().y;
        assert!(y > y_start, "body must rise from its release depth; {y} !> {y_start}");
        // Neutral point for ratio 1.67: density_ratio · frac == 1 → frac ≈ 0.6,
        // centre ≈ 2.5 BU below the surface. Confirm it floats there, neither
        // sunk (frac == 1, still deep) nor launched out (frac == 0, airborne).
        let frac = submerged(y);
        assert!(
            (0.3..0.9).contains(&frac),
            "must rest partly submerged at the surface; y = {y}, frac = {frac}"
        );
    }
}
