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

use byroredux_core::ecs::components::collision::{MotionType, RigidBodyData};
use byroredux_core::ecs::components::water::{
    WaterContact, WaterFlow, WaterMaterial, WaterPlane, WaterVolume,
};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_core::math::Vec3;
use rapier3d::prelude::{vector, RigidBodyType};

use crate::components::RapierHandles;
use crate::world::PhysicsWorld;

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

/// Fraction (0..1) of a body's vertical span below the water surface,
/// derived from its world-space collider AABB.
///
/// This is a deliberately crude proxy for displaced volume: the exact
/// displacement needs the body's shape integrated against the plane, but
/// the AABB-span fraction is monotonic in depth, cheap, and sufficient to
/// reach the buoyancy equilibrium (`density_ratio · fraction == 1`). For a
/// symmetric shape (sphere / box) it is exact; for an irregular ragdoll
/// bone it slightly over/under-estimates near the surface, which only
/// shifts the rest height by a few BU — imperceptible.
///
/// - `aabb_min_y` / `aabb_max_y` — the body collider AABB's world-space
///   vertical extent (`aabb_max_y >= aabb_min_y`).
/// - `surface_y` — the water plane height (the `WaterVolume`'s `max.y`).
///
/// Returns `0.0` when the whole body is above the surface, `1.0` when the
/// whole body is under it, and the partial fraction in between.
#[inline]
pub fn submerged_fraction(aabb_min_y: f32, aabb_max_y: f32, surface_y: f32) -> f32 {
    // `max(eps)` guards a degenerate flat AABB (height 0) from dividing by
    // zero — such a body is treated as a thin sheet that is either fully
    // dry or fully wet via the clamp.
    let height = (aabb_max_y - aabb_min_y).max(1e-6);
    ((surface_y - aabb_min_y) / height).clamp(0.0, 1.0)
}

/// Near-surface acceptance band (Bethesda world units) for the body↔water
/// containment test, so a body bobbing right at the waterline doesn't
/// flicker in and out of contact. Mirrors the camera submersion system's
/// `WATERLINE_HYSTERESIS` (`byroredux::systems::water`, #1450) — kept as a
/// local constant because that one is private to the binary crate.
const WATERLINE_HYSTERESIS: f32 = 4.0;

/// One water plane's volume + surface height + material (+ optional flow),
/// snapshotted for the per-body buoyancy scan.
struct WaterSurface {
    volume: WaterVolume,
    surface_y: f32,
    material: WaterMaterial,
    flow: Option<WaterFlow>,
}

/// Collect every active water plane's volume + surface height + material
/// (+ optional flow). Linear scan — cells carry 1–3 planes; a broadphase
/// would only matter at dozens (mirrors `submersion_system`).
fn collect_water_surfaces(world: &World) -> Vec<WaterSurface> {
    let mut out = Vec::new();
    let (Some(wq), Some(vq)) = (world.query::<WaterPlane>(), world.query::<WaterVolume>()) else {
        return out;
    };
    let flow_q = world.query::<WaterFlow>();
    for (entity, plane) in wq.iter() {
        let Some(volume) = vq.get(entity) else {
            continue;
        };
        let flow = flow_q.as_ref().and_then(|fq| fq.get(entity).copied());
        out.push(WaterSurface {
            volume: *volume,
            surface_y: volume.max[1],
            material: plane.material,
            flow,
        });
    }
    out
}

/// WATAL Phase 2 — the buoyancy phase of [`physics_sync_system`]
/// (`crate::sync`). Runs BEFORE the Rapier step so the lift it adds
/// integrates this tick.
///
/// For every **dynamic** body whose representative collider AABB overlaps a
/// [`WaterVolume`], it writes a [`WaterContact`] and applies Archimedes
/// buoyancy ([`buoyancy_force`]) + submerged viscous damping so dropped
/// clutter rises and *settles* at the surface instead of sinking. Bodies
/// that have never touched water carry no component and cost one AABB test.
///
/// # Wake discipline (shared with the exterior-freeze fix — `watal.md` §0)
///
/// The exterior-freeze fix spawns dynamic bodies **asleep**, and the
/// static-scene step fast-path skips stepping while every body sleeps.
/// Buoyancy must not defeat that:
///
/// - Forces/damping are applied with `wake_up = false`, so re-deriving the
///   float each tick never resets a body's sleep timer.
/// - A body is **woken once** only on the dry→wet transition (so clutter
///   that streams in already submerged, spawned asleep, still floats up).
///   Once it reaches equilibrium (`density_ratio · fraction == 1`, partly
///   submerged) it sleeps and is never re-woken — it stays wet, so no new
///   transition fires. The sim quiesces; buoyancy can't pin it awake.
/// - On the wet→dry transition the body's authored damping is restored
///   exactly once and the buoyancy force cleared.
pub(crate) fn apply_buoyancy(world: &World, had_newcomers: bool) {
    if world.try_resource::<PhysicsWorld>().is_none() {
        return;
    }
    let consts = world
        .try_resource::<PhysicsWaterConstants>()
        .map(|r| *r)
        .unwrap_or_default();

    let surfaces = collect_water_surfaces(world);
    // Fast path: no water anywhere → no body can be buoyant, so skip the
    // whole per-body scan. Keeps interior / no-water / loose-NIF frames free
    // (the common case). A body that was wet last frame is co-unloaded with
    // its cell's water plane, so there is nothing left to restore here.
    if surfaces.is_empty() {
        return;
    }

    // Quiesced-scene fast path: a dry→wet transition can only occur if some
    // dynamic body's pose changed this frame, which requires it to be awake
    // (or pending a step, or freshly registered). With nothing awake, nothing
    // pending, and no newcomer this frame, no body moved since the last
    // buoyancy eval, so the per-body scan is pure waste — skip it and let the
    // static-scene step fast-path keep a settled water cell at ~0 cost (the
    // exterior-freeze goal, watal.md §0). The `had_newcomers` term is
    // load-bearing: a body that streams in already submerged spawns ASLEEP and
    // Phase 1 does NOT wake it (`register_newcomers`), so without this term its
    // first-frame dry→wet float-up would be skipped here.
    {
        let pw = world.resource::<PhysicsWorld>();
        if pw.awake_counts().0 == 0 && !pw.pending_wake() && !had_newcomers {
            return;
        }
    }

    // Union XZ footprint of every water plane — a cheap per-body reject that
    // avoids the collider AABB compute for the (vast) majority of bodies in a
    // large exterior cell with one small lake. The per-surface test below
    // still refines XZ + the vertical band; this only prunes far-away bodies.
    let (mut ux0, mut uz0, mut ux1, mut uz1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for s in &surfaces {
        ux0 = ux0.min(s.volume.min[0]);
        uz0 = uz0.min(s.volume.min[2]);
        ux1 = ux1.max(s.volume.max[0]);
        uz1 = uz1.max(s.volume.max[2]);
    }

    // Gather dynamic bodies + their authored damping (to restore on exit) +
    // prior wet state (a present `WaterContact` with fraction > 0). Done
    // under read locks, released before taking the `PhysicsWorld` write lock.
    let Some(handles_q) = world.query::<RapierHandles>() else {
        return;
    };
    let Some(body_q) = world.query::<RigidBodyData>() else {
        return;
    };
    let contact_q = world.query::<WaterContact>();

    struct Target {
        entity: EntityId,
        handles: RapierHandles,
        authored_lin: f32,
        authored_ang: f32,
        prior_wet: bool,
    }
    let mut targets: Vec<Target> = Vec::new();
    for (entity, handles) in handles_q.iter() {
        let Some(bd) = body_q.get(entity) else {
            continue;
        };
        if bd.motion_type != MotionType::Dynamic {
            continue;
        }
        let prior_wet = contact_q
            .as_ref()
            .and_then(|cq| cq.get(entity))
            .map(|c| c.submerged_fraction > 0.0)
            .unwrap_or(false);
        targets.push(Target {
            entity,
            handles: *handles,
            authored_lin: bd.linear_damping,
            authored_ang: bd.angular_damping,
            prior_wet,
        });
    }
    drop(handles_q);
    drop(body_q);
    drop(contact_q);

    if targets.is_empty() {
        return;
    }

    // Buoyancy + damping under the write lock; collect the `WaterContact`
    // writes to apply after the lock drops.
    let mut writes: Vec<(EntityId, WaterContact)> = Vec::new();
    {
        let mut pw = world.resource_mut::<PhysicsWorld>();
        let gravity_y = pw.gravity.y;
        // Set once if any body is woken on a dry→wet transition this frame
        // (see the `pw.wake()` after the loop).
        let mut woke_any = false;
        for t in &targets {
            // Cheap reject first (body translation only); pay for the collider
            // AABB + the per-surface vertical test ONLY for bodies inside the
            // water XZ footprint. The immutable `body` borrow ends with `pos`.
            let pos = {
                let Some(body) = pw.bodies.get(t.handles.body) else {
                    continue;
                };
                if body.body_type() != RigidBodyType::Dynamic {
                    continue;
                }
                *body.translation()
            };
            let center_y = pos.y;

            // Find the containing surface: body centre inside the volume's XZ
            // extent, and the body within the column (band-extended above the
            // surface like the camera path so the waterline is sticky). Bodies
            // outside the union footprint resolve to `None` without ever
            // touching the collider set. `(s, min_y, max_y)` borrows only the
            // local `surfaces` Vec — not `pw` — so the `get_mut` below is free.
            let surface = if pos.x < ux0 || pos.x > ux1 || pos.z < uz0 || pos.z > uz1 {
                None
            } else {
                let Some(collider) = pw.colliders.get(t.handles.collider) else {
                    continue;
                };
                let aabb = collider.compute_aabb();
                let (min_y, max_y) = (aabb.mins.y, aabb.maxs.y);
                surfaces
                    .iter()
                    .find(|s| {
                        let v = &s.volume;
                        pos.x >= v.min[0]
                            && pos.x <= v.max[0]
                            && pos.z >= v.min[2]
                            && pos.z <= v.max[2]
                            && min_y <= s.surface_y + WATERLINE_HYSTERESIS
                            && max_y >= v.min[1]
                    })
                    .map(|s| (s, min_y, max_y))
            };

            match surface {
                Some((s, min_y, max_y)) => {
                    let frac = submerged_fraction(min_y, max_y, s.surface_y);
                    if frac > 0.0 {
                        if let Some(b) = pw.bodies.get_mut(t.handles.body) {
                            // Submerged viscous damping so the float settles
                            // (and can then sleep) rather than bobbing forever.
                            b.set_linear_damping(consts.linear_damping_in);
                            b.set_angular_damping(consts.angular_damping_in);
                            // Wake ONCE on entry so a body that streamed in
                            // already submerged (spawned asleep) floats up.
                            if !t.prior_wet {
                                b.wake_up(true);
                                woke_any = true;
                            }
                            // Apply lift only while awake — a settled, sleeping
                            // float is left at rest (gravity is also not
                            // integrated while asleep, so equilibrium holds).
                            if !b.is_sleeping() {
                                let mass = b.mass();
                                let f = buoyancy_force(
                                    mass,
                                    frac,
                                    gravity_y,
                                    consts.buoyancy_density_ratio,
                                );
                                b.reset_forces(false);
                                b.add_force(vector![0.0, f.y, 0.0], false);
                            }
                        }
                    }
                    writes.push((
                        t.entity,
                        WaterContact {
                            depth: s.surface_y - center_y,
                            submerged_fraction: frac,
                            head_submerged: max_y <= s.surface_y,
                            flow: s.flow,
                            material: Some(s.material),
                        },
                    ));
                }
                None => {
                    // Exited every volume. If it was wet, restore its authored
                    // damping + clear the buoyancy force exactly once, and mark
                    // it dry. Bodies that were never wet are skipped entirely
                    // (no WaterContact churn for the whole non-water scene).
                    if t.prior_wet {
                        if let Some(b) = pw.bodies.get_mut(t.handles.body) {
                            b.set_linear_damping(t.authored_lin);
                            b.set_angular_damping(t.authored_ang);
                            b.reset_forces(false);
                        }
                        writes.push((t.entity, WaterContact::default()));
                    }
                }
            }
        }

        // A body woken on a dry→wet transition is NOT yet in
        // `active_dynamic_bodies` (that island list reflects the prior step),
        // so the static-scene step fast-path (`world.rs::step`) would skip this
        // frame unless `pending_wake` is armed — leaving the freshly-woken
        // float un-integrated until something else pokes the sim. Arm it once
        // per frame any dry→wet transition occurred. This is a one-shot tied to
        // the (also one-shot, `!prior_wet`-gated) body wake, so it CANNOT pin
        // the sim: a settled float stays wet, fires no new transition, and the
        // flag clears on the next step. (watal.md §0 wake discipline.)
        if woke_any {
            pw.wake();
        }
    }

    if writes.is_empty() {
        return;
    }
    match world.query_mut::<WaterContact>() {
        Some(mut wq) => {
            for (entity, contact) in writes {
                wq.insert(entity, contact);
            }
        }
        // Mirror `register_newcomers`' RapierHandles breadcrumb: without this,
        // a forgotten `register::<WaterContact>()` at setup would silently
        // apply buoyancy forces but never record a contact — an invisible
        // failure. Log once-ish at error so it's caught.
        None => log::error!(
            "WaterContact storage missing — call World::register::<WaterContact>() \
             during setup before running physics_sync_system; buoyancy still \
             applies but no per-body water contact is recorded"
        ),
    }
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
    fn submerged_fraction_spans_dry_to_full() {
        // Surface at 0; a body spanning y ∈ [-10, 10].
        // Fully above (min above surface) → 0.
        assert_eq!(submerged_fraction(5.0, 25.0, 0.0), 0.0);
        // Fully below (max at/under surface) → 1.
        assert_eq!(submerged_fraction(-25.0, -5.0, 0.0), 1.0);
        // Centred on the surface → exactly half.
        assert!((submerged_fraction(-10.0, 10.0, 0.0) - 0.5).abs() < 1e-6);
        // A quarter under.
        assert!((submerged_fraction(-10.0, 30.0, 0.0) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn submerged_fraction_clamps_and_survives_degenerate_aabb() {
        // Surface far above a fully-submerged body → clamps to 1, no overflow.
        assert_eq!(submerged_fraction(-100.0, -50.0, 1000.0), 1.0);
        // Surface far below a fully-dry body → clamps to 0.
        assert_eq!(submerged_fraction(50.0, 100.0, -1000.0), 0.0);
        // Degenerate flat AABB (height 0): no NaN/inf; resolves to 0 or 1.
        let dry = submerged_fraction(10.0, 10.0, 0.0);
        let wet = submerged_fraction(-10.0, -10.0, 0.0);
        assert!(dry.is_finite() && wet.is_finite());
        assert_eq!(dry, 0.0);
        assert_eq!(wet, 1.0);
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

    /// WATAL §7.3 Phase 2 test gate, end-to-end through the LIVE
    /// `physics_sync_system` buoyancy phase: a dynamic body spawned (asleep,
    /// per the exterior-freeze fix) inside a `WaterVolume` must float up,
    /// settle near the surface, and have a `WaterContact` written. This
    /// proves the whole chain — register → wake-on-entry → buoyancy +
    /// submerged damping → step → pull → WaterContact — not just the math.
    #[test]
    fn body_in_water_volume_floats_via_physics_sync() {
        use crate::{physics_sync_system, RapierHandles};
        use byroredux_core::ecs::components::collision::{
            CollisionShape, MotionType, RigidBodyData,
        };
        use byroredux_core::ecs::components::water::{
            WaterContact, WaterKind, WaterMaterial, WaterPlane, WaterVolume,
        };
        use byroredux_core::ecs::components::{GlobalTransform, Transform};
        use byroredux_core::ecs::World;

        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        world.insert_resource(PhysicsWaterConstants::default());
        world.register::<RapierHandles>();
        world.register::<WaterContact>();

        // Water: surface at y=0, a deep wide column.
        let surface_y = 0.0_f32;
        let water = world.spawn();
        world.insert(
            water,
            WaterPlane {
                kind: WaterKind::Calm,
                material: WaterMaterial::default(),
            },
        );
        world.insert(
            water,
            WaterVolume {
                min: [-500.0, -200.0, -500.0],
                max: [500.0, surface_y, 500.0],
            },
        );

        // Dynamic ball released fully submerged at y=-60 (top=-50 < surface).
        let radius = 10.0_f32;
        let start_y = -5.0_f32;
        let body = world.spawn();
        world.insert(body, CollisionShape::Ball { radius });
        world.insert(
            body,
            RigidBodyData {
                motion_type: MotionType::Dynamic,
                mass: 20.0,
                friction: 0.5,
                restitution: 0.0,
                linear_damping: 0.0,
                angular_damping: 0.0,
            },
        );
        world.insert(
            body,
            GlobalTransform::new(Vec3::new(0.0, start_y, 0.0), Quat::IDENTITY, 1.0),
        );
        world.insert(body, Transform::from_translation(Vec3::new(0.0, start_y, 0.0)));

        // ~10 s through the live system (registers on the first call, then
        // wakes-on-entry + applies buoyancy every tick until it sleeps).
        for _ in 0..600 {
            physics_sync_system(&world, PHYSICS_DT);
        }

        let y = world
            .get::<Transform>(body)
            .expect("transform present")
            .translation
            .y;
        assert!(
            y > start_y,
            "buoyant body must rise from its release depth {start_y}; y={y}"
        );
        // Settles near the surface at the ratio-1.67 equilibrium (frac ≈ 0.6,
        // centre ≈ -2 BU). The band is the real proof of buoyancy: with no
        // floor collider in this world, a NON-buoyant body would sink past the
        // column floor toward the kill plane, landing far below this band.
        assert!(
            (surface_y - 6.0..surface_y + radius).contains(&y),
            "must settle near the surface; y={y}"
        );

        let contact = world.get::<WaterContact>(body).expect("WaterContact written");
        assert!(
            contact.submerged_fraction > 0.0 && contact.submerged_fraction <= 1.0,
            "submerged_fraction out of range: {}",
            contact.submerged_fraction
        );
        assert!(contact.material.is_some(), "contact carries the plane material");
    }

    /// A buoyant body settling at equilibrium must fall asleep, and once
    /// asleep the static-scene fast path (`PhysicsWorld::step` returning 0
    /// substeps) must re-engage — buoyancy's continuous per-frame force
    /// application must not keep the body (and thus the whole sim) awake
    /// forever.
    #[test]
    fn buoyant_body_sleeps_so_static_fast_path_re_engages() {
        use crate::{physics_sync_system, RapierHandles};
        use byroredux_core::ecs::components::collision::{CollisionShape, MotionType, RigidBodyData};
        use byroredux_core::ecs::components::water::{WaterContact, WaterKind, WaterMaterial, WaterPlane, WaterVolume};
        use byroredux_core::ecs::components::{GlobalTransform, Transform};
        use byroredux_core::ecs::World;

        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        world.insert_resource(PhysicsWaterConstants {
            buoyancy_density_ratio: 1.02, // near-neutral: equilibrium frac ~ 0.98, AABB top hugs the surface band
            ..PhysicsWaterConstants::default()
        });
        world.register::<RapierHandles>();
        world.register::<WaterContact>();

        let surface_y = 0.0_f32;
        let water = world.spawn();
        world.insert(water, WaterPlane { kind: WaterKind::Calm, material: WaterMaterial::default() });
        world.insert(water, WaterVolume { min: [-500.0, -200.0, -500.0], max: [500.0, surface_y, 500.0] });

        let radius = 10.0_f32;
        let start_y = -5.0_f32;
        let body = world.spawn();
        world.insert(body, CollisionShape::Ball { radius });
        world.insert(body, RigidBodyData { motion_type: MotionType::Dynamic, mass: 20.0, friction: 0.5, restitution: 0.0, linear_damping: 0.0, angular_damping: 0.0 });
        world.insert(body, GlobalTransform::new(Vec3::new(0.0, start_y, 0.0), Quat::IDENTITY, 1.0));
        world.insert(body, Transform::from_translation(Vec3::new(0.0, start_y, 0.0)));

        // Run long enough to settle.
        for _ in 0..2000 {
            physics_sync_system(&world, PHYSICS_DT);
            let (ad, _ak) = world.resource::<PhysicsWorld>().awake_counts();
            if ad == 0 {
                break;
            }
        }
        let (ad, _ak) = world.resource::<PhysicsWorld>().awake_counts();
        // Now confirm the static-scene fast path: further steps return 0.
        let steps = { let mut pw = world.resource_mut::<PhysicsWorld>(); pw.step(PHYSICS_DT) };
        assert_eq!(ad, 0, "buoyant body must sleep at equilibrium so the fast path re-engages");
        assert_eq!(steps, 0, "static-scene fast path must skip stepping once the float sleeps");
    }
}
