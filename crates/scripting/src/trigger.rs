//! Trigger-volume detection — the engine emit site for
//! [`OnTriggerEnterEvent`](crate::events::OnTriggerEnterEvent).
//!
//! A trigger volume is an invisible REFR (no MODL) whose geometry is an
//! `XPRM` primitive — a box or sphere placed in the world. Bethesda's
//! `default*Trigger` script family runs `OnTriggerEnter(akActionRef)` when
//! an actor crosses *into* that volume. This module is the runtime half
//! the [`quest_advance`](crate::papyrus_demo::quest_advance) recognizer
//! needs: the cell loader attaches a [`TriggerVolume`] to each trigger
//! REFR, and [`trigger_detection_system`] emits `OnTriggerEnterEvent` on
//! the volume entity the frame the player enters it.
//!
//! **Edge-triggered.** Papyrus `OnTriggerEnter` fires once per crossing,
//! not every frame the actor is inside; [`TriggerVolume::occupant_inside`]
//! carries the previous-frame state so only an outside→inside transition
//! emits. Leaving and re-entering fires again — matching the engine.
//!
//! The volume is stored in **world space** (computed once by the cell
//! loader from the REFR placement + primitive), so detection needs only
//! the player's world position — no per-frame transform composition, and
//! correct for the static volumes that triggers always are.

use crate::events::OnTriggerEnterEvent;
use crate::papyrus_demo::PlayerEntity;
use byroredux_core::ecs::components::GlobalTransform;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_core::math::{Quat, Vec3};

/// The primitive shape of a trigger volume (`XPRM` shape-type 1 / 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerShape {
    /// Oriented box — `half_extents` are per-axis, `rotation` orients it.
    Box,
    /// Sphere — `half_extents.x` is the radius; `rotation` is unused.
    Sphere,
}

/// World-space trigger volume attached to an invisible trigger REFR.
/// Built by the cell loader from the REFR's placement + `XPRM` primitive.
#[derive(Debug, Clone, Copy)]
pub struct TriggerVolume {
    /// World-space center (the REFR's placed position).
    pub center: Vec3,
    /// Box: per-axis half-extents. Sphere: `x` holds the radius.
    pub half_extents: Vec3,
    /// Box orientation (the REFR's placed rotation). Identity for spheres.
    pub rotation: Quat,
    pub shape: TriggerShape,
    /// Edge-trigger state: was the player inside on the previous tick this
    /// volume was checked? `None` until [`trigger_detection_system`] has
    /// evaluated it at least once — every volume is spawned with `None`
    /// (see the cell loader's `trigger_volume_from_primitive`), so the
    /// very first tick a volume exists for silently seeds this from
    /// whatever the player's actual position is that frame, WITHOUT
    /// emitting an enter event. This is what makes the seed correct
    /// whether the volume was just loaded on initial cell entry, a
    /// door-walk transition, or exterior streaming step-in: none of
    /// those call sites need to know the player's position, because
    /// "no prior tick to compare against" and "player loaded already
    /// standing inside" collapse to the same case — SCR-D6-NEW-02 /
    /// #1817. Fixes #1742-adjacent load-time false positives without
    /// threading player position through every `TriggerVolume` spawn site.
    pub occupant_inside: Option<bool>,
}

impl TriggerVolume {
    /// Whether world-space point `p` lies inside the volume.
    pub fn contains(&self, p: Vec3) -> bool {
        match self.shape {
            TriggerShape::Sphere => {
                let r = self.half_extents.x;
                (p - self.center).length_squared() <= r * r
            }
            TriggerShape::Box => {
                // Map the point into the box's local frame, then test
                // against the half-extents on each axis (OBB containment).
                let local = self.rotation.inverse() * (p - self.center);
                local.x.abs() <= self.half_extents.x
                    && local.y.abs() <= self.half_extents.y
                    && local.z.abs() <= self.half_extents.z
            }
        }
    }
}

impl Component for TriggerVolume {
    type Storage = SparseSetStorage<Self>;
}

/// Register the [`TriggerVolume`] storage. Called from [`crate::register`].
pub fn register(world: &mut World) {
    world.register::<TriggerVolume>();
}

/// Per-frame detection: for every [`TriggerVolume`], test the player's
/// world position and emit an [`OnTriggerEnterEvent`] on the volume
/// entity the frame the player transitions from outside to inside. The
/// `OnTriggerEnterEvent` marker is drained by `event_cleanup_system` at
/// end-of-frame, so the quest-advance dispatch sees exactly one enter per
/// crossing.
///
/// No-ops gracefully when there's no player, no player transform, or no
/// trigger volumes in the world.
pub fn trigger_detection_system(world: &World) {
    let Some(player) = world.try_resource::<PlayerEntity>().map(|p| p.0) else {
        return;
    };
    // The player's world position — the only spatial input detection needs.
    let Some(player_pos) = player_world_position(world, player) else {
        return;
    };

    // Phase 1 (read+update): flip each volume's occupancy and record the
    // entities that just entered. Mutating `occupant_inside` here keeps
    // the edge-trigger state on the component itself.
    let mut entered: Vec<EntityId> = Vec::new();
    {
        let Some(mut vols) = world.query_mut::<TriggerVolume>() else {
            return;
        };
        for (entity, vol) in vols.iter_mut() {
            let inside = vol.contains(player_pos);
            // SCR-D6-NEW-02 / #1817 — `None` means this volume has never
            // been checked before (just spawned, this tick). Seed the
            // state silently instead of comparing against a synthetic
            // "was outside" default — a player who loads already standing
            // inside the volume must not see a spurious enter on its
            // first tick.
            if let Some(was_inside) = vol.occupant_inside {
                if inside && !was_inside {
                    entered.push(entity);
                }
            }
            vol.occupant_inside = Some(inside);
        }
    }

    // Phase 2 (write): emit the enter markers. Separate borrow so the
    // TriggerVolume write lock is released first.
    if entered.is_empty() {
        return;
    }
    let Some(mut events) = world.query_mut::<OnTriggerEnterEvent>() else {
        return;
    };
    for entity in entered {
        events.insert(entity, OnTriggerEnterEvent { triggerer: player });
    }
}

/// The player's world-space translation, via its [`GlobalTransform`].
fn player_world_position(world: &World, player: EntityId) -> Option<Vec3> {
    let q = world.query::<GlobalTransform>()?;
    q.get(player).map(|gt| gt.translation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::OnTriggerEnterEvent;
    use byroredux_core::ecs::components::{GlobalTransform, Transform};
    use byroredux_core::ecs::world::World;

    /// Builds an already-primed volume (known outside on the previous
    /// tick) — the steady-state shape most tests below want. Tests
    /// exercising the fresh-spawn seeding behavior construct
    /// `TriggerVolume` directly with `occupant_inside: None`.
    fn axis_box(center: Vec3, half: Vec3) -> TriggerVolume {
        TriggerVolume {
            center,
            half_extents: half,
            rotation: Quat::IDENTITY,
            shape: TriggerShape::Box,
            occupant_inside: Some(false),
        }
    }

    #[test]
    fn aabb_contains_interior_and_rejects_exterior() {
        let v = axis_box(Vec3::ZERO, Vec3::new(2.0, 1.0, 3.0));
        assert!(v.contains(Vec3::ZERO));
        assert!(v.contains(Vec3::new(1.9, -0.9, 2.9)));
        assert!(v.contains(Vec3::new(2.0, 1.0, 3.0))); // on the face
        assert!(!v.contains(Vec3::new(2.1, 0.0, 0.0)));
        assert!(!v.contains(Vec3::new(0.0, 0.0, 3.5)));
    }

    #[test]
    fn obb_rotation_is_respected() {
        // A box rotated 45° about Y: a point that's outside the AABB on
        // the X axis but inside the rotated box must register inside, and
        // vice-versa.
        let rot = Quat::from_rotation_y(std::f32::consts::FRAC_PI_4);
        let v = TriggerVolume {
            center: Vec3::ZERO,
            half_extents: Vec3::new(1.0, 1.0, 1.0),
            rotation: rot,
            shape: TriggerShape::Box,
            occupant_inside: Some(false),
        };
        // Along the box-local X (rotated), a diagonal world point lands
        // inside; the same distance along world X is past the corner.
        let local_x_dir = rot * Vec3::X; // box's +X in world space
        assert!(v.contains(local_x_dir * 0.9));
        // World-space corner direction at the same radius is outside.
        assert!(!v.contains(Vec3::new(1.4, 0.0, 1.4)));
    }

    #[test]
    fn sphere_contains_by_radius() {
        let v = TriggerVolume {
            center: Vec3::new(5.0, 0.0, 0.0),
            half_extents: Vec3::new(2.0, 0.0, 0.0), // radius 2
            rotation: Quat::IDENTITY,
            shape: TriggerShape::Sphere,
            occupant_inside: Some(false),
        };
        assert!(v.contains(Vec3::new(5.0, 1.9, 0.0)));
        assert!(!v.contains(Vec3::new(5.0, 2.1, 0.0)));
    }

    /// Build a world with a player at `player_pos` and one box trigger,
    /// run the detection system once, and report whether an enter fired.
    fn run_once(player_pos: Vec3, vol: TriggerVolume) -> (World, EntityId, bool) {
        let mut world = World::new();
        crate::register(&mut world);
        let player = world.spawn();
        world.insert(player, Transform::from_translation(player_pos));
        world.insert(
            player,
            GlobalTransform::new(player_pos, Quat::IDENTITY, 1.0),
        );
        world.insert_resource(PlayerEntity(player));
        let trigger = world.spawn();
        world.insert(trigger, vol);

        trigger_detection_system(&world);
        let fired = world.has::<OnTriggerEnterEvent>(trigger);
        (world, trigger, fired)
    }

    #[test]
    fn emits_event_when_player_inside() {
        let (world, trigger, fired) = run_once(Vec3::ZERO, axis_box(Vec3::ZERO, Vec3::splat(1.0)));
        assert!(fired, "player inside an unoccupied volume must emit enter");
        let ev = world.get::<OnTriggerEnterEvent>(trigger).unwrap();
        assert_eq!(ev.triggerer, world.resource::<PlayerEntity>().0);
    }

    #[test]
    fn no_event_when_player_outside() {
        let (_w, _t, fired) =
            run_once(Vec3::new(10.0, 0.0, 0.0), axis_box(Vec3::ZERO, Vec3::splat(1.0)));
        assert!(!fired, "player outside the volume must not emit");
    }

    #[test]
    fn edge_triggered_not_level_triggered() {
        // Steady state: the volume was already known-occupied on the
        // previous tick (not a fresh spawn — see the None-sentinel tests
        // below for that case) and the player is still inside: no re-fire.
        let mut vol = axis_box(Vec3::ZERO, Vec3::splat(1.0));
        vol.occupant_inside = Some(true);
        let (_w, _t, fired) = run_once(Vec3::ZERO, vol);
        assert!(
            !fired,
            "a volume already occupied must not re-fire while still inside",
        );
    }

    /// SCR-D6-NEW-02 / #1817 — a freshly-spawned volume (`occupant_inside:
    /// None`, exactly what the cell loader's `trigger_volume_from_primitive`
    /// produces) with the player already standing inside must NOT fire on
    /// its very first detection tick — that first tick only seeds the
    /// state. This is the actual load-already-inside bug: pre-fix, spawning
    /// with a bare `false` made this indistinguishable from "known outside,
    /// player just crossed in," firing a spurious enter.
    #[test]
    fn fresh_spawn_seeds_silently_even_when_player_already_inside() {
        let vol = TriggerVolume {
            center: Vec3::ZERO,
            half_extents: Vec3::splat(1.0),
            rotation: Quat::IDENTITY,
            shape: TriggerShape::Box,
            occupant_inside: None,
        };
        let (_w, _t, fired) = run_once(Vec3::ZERO, vol);
        assert!(
            !fired,
            "a volume's first-ever detection tick must seed occupant_inside, \
             not fire — a player loading already inside a trigger volume \
             must not see a spurious OnTriggerEnter (#1817)"
        );
    }

    /// Counterpart: a freshly-spawned volume with the player OUTSIDE also
    /// doesn't fire on the seed tick (nothing surprising there), but the
    /// subsequent genuine crossing still fires normally — the seed doesn't
    /// permanently wedge the volume into a non-firing state.
    #[test]
    fn fresh_spawn_still_fires_on_a_later_genuine_crossing() {
        let mut world = World::new();
        crate::register(&mut world);
        let player = world.spawn();
        let outside = Vec3::new(10.0, 0.0, 0.0);
        world.insert(player, Transform::from_translation(outside));
        world.insert(player, GlobalTransform::new(outside, Quat::IDENTITY, 1.0));
        world.insert_resource(PlayerEntity(player));
        let trigger = world.spawn();
        world.insert(
            trigger,
            TriggerVolume {
                center: Vec3::ZERO,
                half_extents: Vec3::splat(1.0),
                rotation: Quat::IDENTITY,
                shape: TriggerShape::Box,
                occupant_inside: None,
            },
        );

        // Seed tick — player outside, nothing fires.
        trigger_detection_system(&world);
        assert!(!world.has::<OnTriggerEnterEvent>(trigger));

        // Genuine crossing — must fire.
        {
            let mut q = world.query_mut::<GlobalTransform>().unwrap();
            q.get_mut(player).unwrap().translation = Vec3::ZERO;
        }
        trigger_detection_system(&world);
        assert!(
            world.has::<OnTriggerEnterEvent>(trigger),
            "a later genuine crossing must still fire after the seed tick"
        );
    }

    #[test]
    fn re_entry_fires_again() {
        // Inside → emits and sets occupied; stays inside → silent; moves
        // out → clears; comes back → fires again.
        let mut world = World::new();
        crate::register(&mut world);
        let player = world.spawn();
        world.insert(player, Transform::from_translation(Vec3::ZERO));
        world.insert(player, GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0));
        world.insert_resource(PlayerEntity(player));
        let trigger = world.spawn();
        world.insert(trigger, axis_box(Vec3::ZERO, Vec3::splat(1.0)));

        let move_player = |world: &World, p: Vec3| {
            let mut q = world.query_mut::<GlobalTransform>().unwrap();
            q.get_mut(player).unwrap().translation = p;
        };
        let clear_event = |world: &World| {
            if world.has::<OnTriggerEnterEvent>(trigger) {
                world
                    .query_mut::<OnTriggerEnterEvent>()
                    .unwrap()
                    .remove(trigger);
            }
        };

        // Frame 1 — inside: fires.
        trigger_detection_system(&world);
        assert!(world.has::<OnTriggerEnterEvent>(trigger));
        clear_event(&world);

        // Frame 2 — still inside: no re-fire.
        trigger_detection_system(&world);
        assert!(!world.has::<OnTriggerEnterEvent>(trigger));

        // Frame 3 — moved out: no fire, occupancy clears.
        move_player(&world, Vec3::new(10.0, 0.0, 0.0));
        trigger_detection_system(&world);
        assert!(!world.has::<OnTriggerEnterEvent>(trigger));

        // Frame 4 — back inside: fires again.
        move_player(&world, Vec3::ZERO);
        trigger_detection_system(&world);
        assert!(world.has::<OnTriggerEnterEvent>(trigger));
    }
}
