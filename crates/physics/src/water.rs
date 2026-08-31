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
//! (`add_force` / `reset_forces`), and [`apply_buoyancy`] genuinely routes
//! through it — every force this module applies goes through those wrappers
//! with `wake_up = false`, never through the Rapier body directly.
//!
//! That flag is the whole reason it can (#2889). These are *per-frame* terms,
//! re-derived from the body's own mass and velocity on every tick: a wrapper
//! that woke unconditionally would re-wake the body it had just settled and
//! pin the scene stepping forever, which is the opposite of what the rest of
//! this module exists to protect. The forces are derived under a
//! `bodies.get_mut` borrow (they need `mass()` / `linvel()`) and applied
//! after it ends, so the borrow stays short and the public API stays the
//! single application path.
//!
//! `PhysicsWorld::apply_impulse` is the third member of that family and has
//! **no consumer here** — it is the hook for the not-yet-built WATAL Phase 3
//! splash kick, not something buoyancy uses.
//!
//! [`apply_buoyancy`] runs from the live physics-sync pre-step (phase 2.5,
//! before the step integrates what it applies), combines lift with current
//! drag, and preserves the exterior-reroute wake/sleep discipline
//! (WATAL §7 Phase 2).

use byroredux_core::ecs::components::collision::{MotionType, RigidBodyData};
use byroredux_core::ecs::components::groundcover::WindField;
use byroredux_core::ecs::components::water::{
    WaterContact, WaterCurrentVolume, WaterFlow, WaterMaterial, WaterPlane, WaterVolume,
    WATERLINE_HYSTERESIS,
};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::resources::TotalTime;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_core::math::Vec3;
use rapier3d::prelude::RigidBodyType;

use crate::components::{Ragdoll, RapierHandles};
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
    /// First-order response rate (1/s) that pulls the body's velocity along
    /// a [`WaterFlow`] axis toward the authored current speed. This is an
    /// engine constant, not WATR data; the WATR supplies direction + speed.
    /// A response force (instead of a constant acceleration) converges on a
    /// bounded terminal speed and cannot accelerate clutter without limit.
    pub current_drag: f32,
    /// First-order response rate for atmospheric wind acting on the exposed
    /// portion of a body at the water surface. Wind is weaker than authored
    /// current drag and vanishes when the body is fully submerged.
    pub wind_drag: f32,
}

impl Default for PhysicsWaterConstants {
    fn default() -> Self {
        Self {
            buoyancy_density_ratio: 1.67,
            linear_damping_in: 1.5,
            angular_damping_in: 1.5,
            current_drag: 4.0,
            wind_drag: 0.35,
        }
    }
}

impl Resource for PhysicsWaterConstants {}

#[derive(Clone, Copy)]
struct BuoyancyTarget {
    entity: EntityId,
    handles: RapierHandles,
    authored_lin: f32,
    authored_ang: f32,
    prior_wet: bool,
    prior_depth: f32,
}

/// Reusable staging storage for the complete per-tick buoyancy scan.
///
/// The physics system temporarily takes these vectors, clears and refills
/// them, then returns their allocations after every exit path. This keeps
/// water surfaces, placed currents, dynamic-body targets, and contact writes
/// allocation-free after their high-water marks (#3135).
#[derive(Default)]
pub struct WaterContactScratch {
    surfaces: Vec<WaterSurface>,
    current_volumes: Vec<WaterCurrentVolume>,
    targets: Vec<BuoyancyTarget>,
    writes: Vec<(EntityId, WaterContact)>,
    /// Bodies an authored current acted on during the previous per-body scan,
    /// sorted by id (#3268).
    ///
    /// This is a latch, not staging: unlike the four buffers above it is
    /// *not* cleared at the top of the scan, because it is the current-volume
    /// branch's equivalent of the surface branch's `prior_wet` bit. The
    /// surface branch wakes a body once on the dry→wet edge; the current
    /// branch, which deliberately applies its per-frame force with
    /// `wake_up = false`, needs the same one-shot edge or a body that streams
    /// in asleep inside a marker with no overlapping `WaterPlane` is never
    /// woken by anything and its `!is_sleeping()` gate stays false forever.
    ///
    /// Only the frames on which the per-body scan actually runs update it.
    /// The quiesced-scene fast path deliberately leaves it alone: nothing
    /// moved, so a body asleep in a current is still in that current and must
    /// not be re-woken when the scan next runs. Every early return that means
    /// "no body can be in a current" clears it instead.
    in_current_prev: Vec<EntityId>,
    /// Scratch half of the double buffer above — filled during the scan and
    /// swapped into `in_current_prev` at the end of it.
    in_current_now: Vec<EntityId>,
}

impl Resource for WaterContactScratch {}

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
pub fn buoyancy_force(
    mass: f32,
    submerged_fraction: f32,
    gravity_y: f32,
    density_ratio: f32,
) -> Vec3 {
    let f = submerged_fraction.clamp(0.0, 1.0) * density_ratio * mass * gravity_y.abs();
    Vec3::new(0.0, f, 0.0)
}

/// Force that makes a submerged body converge on a water current's velocity.
///
/// Only velocity **along the current axis** is matched. Ordinary submerged
/// linear damping already resists cross-current and vertical motion; applying
/// a second full-vector drag here would double-damp bobbing and player-thrown
/// lateral motion. The mass term makes bodies of different mass acquire the
/// same current-driven acceleration:
///
/// ```text
/// F_current = unit(flow.direction)
///           · (flow.speed - dot(body_velocity, unit_direction))
///           · mass · current_drag · submerged_fraction
/// ```
///
/// Invalid/zero directions and non-positive tuning resolve to zero force at
/// this final safety boundary. Valid canonical [`WaterFlow`] values are
/// finite, unit-length, and carry a non-negative speed.
#[inline]
pub fn current_force(
    flow: WaterFlow,
    body_velocity: Vec3,
    mass: f32,
    submerged_fraction: f32,
    current_drag: f32,
) -> Vec3 {
    let direction = Vec3::from_array(flow.direction);
    if !direction.is_finite()
        || !body_velocity.is_finite()
        || !mass.is_finite()
        || !flow.speed.is_finite()
        || !current_drag.is_finite()
    {
        return Vec3::ZERO;
    }

    let direction = direction.normalize_or_zero();
    if direction == Vec3::ZERO {
        return Vec3::ZERO;
    }

    let fraction = submerged_fraction.clamp(0.0, 1.0);
    let response = current_drag.max(0.0);
    let target_speed = flow.speed.max(0.0);
    let speed_error = target_speed - body_velocity.dot(direction);
    direction * (speed_error * mass.max(0.0) * response * fraction)
}

/// Force from the live atmospheric field on the portion of a floating body
/// above the waterline. WindField's X/Z direction is the shared renderer and
/// SpeedTree convention. Reusing [`current_force`] keeps the response
/// mass-correct and bounded at a terminal air velocity.
#[inline]
pub fn wind_force(
    wind: WindField,
    time_secs: f32,
    body_velocity: Vec3,
    mass: f32,
    exposed_fraction: f32,
    wind_drag: f32,
) -> Vec3 {
    let gust = wind.speed
        + wind.gust_amplitude * (time_secs * wind.gust_frequency * std::f32::consts::TAU).sin();
    let gust = if gust.is_finite() { gust.max(0.0) } else { 0.0 };
    if gust <= 0.0 || !wind.direction.iter().all(|value| value.is_finite()) {
        return Vec3::ZERO;
    }
    current_force(
        WaterFlow {
            direction: [wind.direction[0], 0.0, wind.direction[1]],
            speed: gust,
        },
        body_velocity,
        mass,
        exposed_fraction,
        wind_drag,
    )
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
/// shifts the rest height by a few BU — imperceptible. (Ragdoll bones
/// really do reach this path as of #3492; before that the scan had no
/// `Ragdoll` arm and this sentence described a case the code could not
/// produce.)
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

/// One water plane's volume + surface height + material (+ optional flow),
/// snapshotted for the per-body buoyancy scan.
struct WaterSurface {
    entity: EntityId,
    volume: WaterVolume,
    surface_y: f32,
    material: WaterMaterial,
    flow: Option<WaterFlow>,
    damage_per_second: f32,
}

/// Absolute vertical distance from a reference point to a water surface.
///
/// This is the canonical tie-break metric for overlapping water volumes.
/// Camera submersion and rigid-body contact both select the eligible surface
/// with the smallest returned distance.
#[inline]
pub fn nearest_surface_distance(surface_y: f32, reference_y: f32) -> f32 {
    (surface_y - reference_y).abs()
}

/// CPU counterpart of the bounded low-frequency displacement in
/// `renderer/shaders/water.vert`. Dynamic bodies use this only for their
/// contact fraction; the authored volume remains the broadphase column.
/// Add the same live-atmosphere contribution used by `water.vert` to the CPU
/// contact surface. The renderer adds this scroll after applying the authored
/// flow-map scale and boosts amplitude by at most 50% in the strongest wind;
/// keeping the solver on that contract prevents a floating body from tracking
/// a different crest than the one visible under it.
/// CPU evaluation of the bounded low-frequency water displacement used by
/// `water.vert`. `weather_scroll` and `wind_wave_scale` come from
/// [`weather_wave_adjustment`] so gameplay, camera effects, and buoyancy all
/// agree with the rendered crest.
pub fn authored_wave_height_with_weather(
    material: &WaterMaterial,
    position: Vec3,
    time_secs: f32,
    weather_scroll: [f32; 2],
    wind_wave_scale: f32,
) -> f32 {
    let amplitude = material.wave_amplitude.clamp(0.0, 32.0) * wind_wave_scale;
    let frequency = material.wave_frequency.max(0.0);
    if !amplitude.is_finite() || !frequency.is_finite() || !time_secs.is_finite() {
        return 0.0;
    }
    let direction = |scroll: [f32; 2], fallback: [f32; 2]| {
        let len_sq = scroll[0] * scroll[0] + scroll[1] * scroll[1];
        if len_sq.is_finite() && len_sq > 1.0e-10 {
            let inv = len_sq.sqrt().recip();
            [scroll[0] * inv, scroll[1] * inv]
        } else {
            fallback
        }
    };
    let angular_rate = if material.angular_velocity.is_finite() {
        material.angular_velocity.clamp(-32.0, 32.0)
    } else {
        0.0
    };
    let (sin_angle, cos_angle) = (angular_rate * time_secs).sin_cos();
    let rotate_scroll = |scroll: [f32; 2]| {
        [
            scroll[0] * cos_angle - scroll[1] * sin_angle,
            scroll[0] * sin_angle + scroll[1] * cos_angle,
        ]
    };
    let authored_a = rotate_scroll(material.scroll_a);
    let authored_b = rotate_scroll(material.scroll_b);
    let spatial_a = material.uv_scale_a.abs().max(1.0 / 2048.0);
    let spatial_b = material.uv_scale_b.abs().max(1.0 / 4096.0);
    let flowmap_scale = if material.flowmap_scale.is_finite() && material.flowmap_scale > 0.0 {
        material.flowmap_scale.clamp(0.05, 8.0)
    } else {
        1.0
    };
    let scroll_a = [
        authored_a[0] * flowmap_scale + weather_scroll[0],
        authored_a[1] * flowmap_scale + weather_scroll[1],
    ];
    let scroll_b = [
        authored_b[0] * flowmap_scale + weather_scroll[0] * 0.65,
        authored_b[1] * flowmap_scale + weather_scroll[1] * 0.65,
    ];
    // The renderer normalizes these same post-weather vectors in
    // `water.vert`. Deriving the CPU directions before adding weather would
    // make buoyancy/submersion sample a different crest orientation whenever
    // atmospheric wind is active.
    let dir_a = direction(scroll_a, [1.0, 0.35]);
    let dir_b = direction(scroll_b, [-0.4, 1.0]);
    let rate_a = ((scroll_a[0] * scroll_a[0] + scroll_a[1] * scroll_a[1]).sqrt() / 0.0228254)
        .clamp(0.25, 4.0);
    let rate_b = ((scroll_b[0] * scroll_b[0] + scroll_b[1] * scroll_b[1]).sqrt() / 0.0286531)
        .clamp(0.25, 4.0);
    let phase_a =
        (position.x * dir_a[0] + position.z * dir_a[1]) * spatial_a * std::f32::consts::TAU
            + time_secs * frequency * rate_a * std::f32::consts::TAU;
    let phase_b =
        (position.x * dir_b[0] + position.z * dir_b[1]) * spatial_b * std::f32::consts::TAU
            - time_secs * frequency * rate_b * (std::f32::consts::TAU * 0.75);
    amplitude * (phase_a.sin() * 0.60 + phase_b.sin() * 0.40)
}

/// Return the live weather contribution used by the water vertex shader:
/// directional scroll in world units per second and a bounded amplitude
/// multiplier. A missing `WindField` is the calm-water sentinel.
pub fn weather_wave_adjustment(world: &World, time_secs: f32) -> ([f32; 2], f32) {
    let wind = world
        .try_resource::<WindField>()
        .map(|field| *field)
        .unwrap_or_default();
    let gust = wind.speed
        + wind.gust_amplitude * (time_secs * wind.gust_frequency * std::f32::consts::TAU).sin();
    // Match the renderer and SpeedTree wind contract: a negative
    // instantaneous gust is a calm trough, not a reversed current. Keeping
    // the CPU scroll one-sided prevents buoyancy/submersion sampling from
    // tracking a crest that the visible water and vegetation do not show.
    let gust = if gust.is_finite() { gust.max(0.0) } else { 0.0 };
    let len_sq = wind.direction[0] * wind.direction[0] + wind.direction[1] * wind.direction[1];
    let direction = if len_sq.is_finite() && len_sq > 1.0e-6 {
        let inv_len = len_sq.sqrt().recip();
        [wind.direction[0] * inv_len, wind.direction[1] * inv_len]
    } else {
        [1.0, 0.0]
    };
    let scroll = [
        direction[0] * gust * byroredux_core::ecs::components::water::WEATHER_SCROLL_PER_BU_PER_S,
        direction[1] * gust * byroredux_core::ecs::components::water::WEATHER_SCROLL_PER_BU_PER_S,
    ];
    let scale = 1.0
        + (gust / byroredux_core::ecs::components::groundcover::MAX_WIND_SPEED).clamp(0.0, 1.0)
            * 0.5;
    (scroll, scale)
}

/// Collect every active water plane's volume + surface height + material
/// (+ optional flow). Linear scan — cells carry 1–3 planes; a broadphase
/// would only matter at dozens (mirrors `submersion_system`).
fn collect_water_surfaces(world: &World, out: &mut Vec<WaterSurface>) {
    out.clear();
    let (Some(wq), Some(vq)) = (world.query::<WaterPlane>(), world.query::<WaterVolume>()) else {
        return;
    };
    let flow_q = world.query::<WaterFlow>();
    for (entity, plane) in wq.iter() {
        let Some(volume) = vq.get(entity) else {
            continue;
        };
        let flow = flow_q.as_ref().and_then(|fq| fq.get(entity).copied());
        out.push(WaterSurface {
            entity,
            volume: *volume,
            surface_y: volume.max[1],
            material: plane.material,
            flow,
            damage_per_second: plane.damage_per_second,
        });
    }
}

fn collect_water_current_volumes(world: &World, out: &mut Vec<WaterCurrentVolume>) {
    out.clear();
    if let Some(query) = world.query::<WaterCurrentVolume>() {
        out.extend(query.iter().map(|(_, current)| *current));
    }
}

/// Restore dynamic bodies whose source water plane streamed out before the
/// body did. The normal buoyancy loop handles this transition when another
/// surface remains resident; this helper covers the all-surfaces-empty fast
/// path without leaving submerged damping, forces, or `WaterContact` latched.
fn clear_stale_water_contacts(world: &World) {
    let Some(handles_q) = world.query::<RapierHandles>() else {
        return;
    };
    let Some(body_q) = world.query::<RigidBodyData>() else {
        return;
    };
    let Some(contact_q) = world.query::<WaterContact>() else {
        return;
    };

    let mut restore = Vec::new();
    for (entity, contact) in contact_q.iter() {
        if contact.submerged_fraction <= 0.0 {
            continue;
        }
        let Some(handles) = handles_q.get(entity).copied() else {
            continue;
        };
        let Some(body) = body_q.get(entity) else {
            continue;
        };
        if body.motion_type == MotionType::Dynamic {
            restore.push((entity, handles, body.linear_damping, body.angular_damping));
        }
    }
    // #3492 — ragdoll bones reach the buoyancy sink through the `Ragdoll`
    // pass in `apply_buoyancy_with_scratch`, so they can hold a live
    // `WaterContact` here; but they have no `RapierHandles` / `RigidBodyData`
    // row, so the loop above cannot see them. Without this arm a corpse whose
    // water plane streamed out kept submerged damping and a latched contact
    // for the rest of its life — the very state this helper exists to clear.
    if let Some(ragdoll_q) = world.query::<Ragdoll>() {
        for (_, ragdoll) in ragdoll_q.iter() {
            for ((bone, body, _), buoyancy) in ragdoll.bodies.iter().zip(&ragdoll.buoyancy) {
                let submerged = contact_q
                    .get(*bone)
                    .is_some_and(|contact| contact.submerged_fraction > 0.0);
                if !submerged {
                    continue;
                }
                restore.push((
                    *bone,
                    RapierHandles {
                        body: *body,
                        collider: buoyancy.collider,
                    },
                    buoyancy.linear_damping,
                    buoyancy.angular_damping,
                ));
            }
        }
    }
    drop(contact_q);
    drop(body_q);
    drop(handles_q);

    if restore.is_empty() {
        return;
    }
    {
        let mut pw = world.resource_mut::<PhysicsWorld>();
        for (_, handles, linear_damping, angular_damping) in &restore {
            if let Some(body) = pw.bodies.get_mut(handles.body) {
                body.set_linear_damping(*linear_damping);
                body.set_angular_damping(*angular_damping);
                body.reset_forces(false);
            }
        }
    }
    if let Some(mut q) = world.query_mut::<WaterContact>() {
        for (entity, _, _, _) in restore {
            q.insert(entity, WaterContact::default());
        }
    }
}

/// Whether animated surface displacement can change an existing body's
/// contact state while the rigid-body scene itself is otherwise quiescent.
///
/// Authored waves are common (including on the canonical default material),
/// but they matter to buoyancy only when a body already has a live contact
/// close enough to the surface for a crest or trough to change immersion.
/// Treating the mere presence of any non-zero wave amplitude as activity
/// makes the settled-scene fast path unreachable in virtually every water
/// cell (#3135).
fn waves_require_contact_rescan(world: &World, surfaces: &[WaterSurface]) -> bool {
    let max_amplitude = surfaces
        .iter()
        .map(|surface| surface.material.wave_amplitude.abs())
        .filter(|amplitude| amplitude.is_finite())
        .fold(0.0_f32, f32::max);
    if max_amplitude <= 1.0e-4 {
        return false;
    }
    let Some(contacts) = world.query::<WaterContact>() else {
        return false;
    };
    let needs_rescan = contacts.iter().any(|(_, contact)| {
        contact.surface_entity.is_some()
            && (contact.submerged_fraction > 0.0
                || contact.depth.abs() <= max_amplitude + WATERLINE_HYSTERESIS)
    });
    needs_rescan
}

/// WATAL Phase 2 — the buoyancy phase of [`physics_sync_system`]
/// (`crate::sync`). Runs BEFORE the Rapier step so the lift it adds
/// integrates this tick.
///
/// For every **dynamic** body whose representative collider AABB overlaps a
/// [`WaterVolume`], it writes a [`WaterContact`] and applies Archimedes
/// buoyancy ([`buoyancy_force`]), [`WaterFlow`] current force
/// ([`current_force`]), and submerged viscous damping so dropped clutter
/// rises, drifts, and settles instead of sinking. Bodies that have never
/// touched water carry no component and cost one AABB test.
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
    // Taken whole rather than field-by-field (#3268): `in_current_prev` is a
    // latch that must survive the round trip, so "move the struct out, hand
    // it back" is the shape that cannot silently drop one of its fields.
    let (mut scratch_buffers, reuse_scratch) = match world.try_resource_mut::<WaterContactScratch>()
    {
        Some(mut scratch) => (std::mem::take(&mut *scratch), true),
        None => (WaterContactScratch::default(), false),
    };

    apply_buoyancy_with_scratch(world, had_newcomers, &mut scratch_buffers);

    if reuse_scratch {
        if let Some(mut scratch) = world.try_resource_mut::<WaterContactScratch>() {
            *scratch = scratch_buffers;
        } else {
            log::error!("WaterContactScratch disappeared during physics_sync_system");
        }
    }
}

fn apply_buoyancy_with_scratch(
    world: &World,
    had_newcomers: bool,
    scratch: &mut WaterContactScratch,
) {
    // Independent field bindings so the staging buffers and the `#3268` latch
    // can be borrowed at once inside the per-body loop.
    let WaterContactScratch {
        surfaces,
        current_volumes,
        targets,
        writes,
        in_current_prev,
        in_current_now,
    } = scratch;
    surfaces.clear();
    current_volumes.clear();
    targets.clear();
    writes.clear();
    in_current_now.clear();
    if world.try_resource::<PhysicsWorld>().is_none() {
        in_current_prev.clear();
        return;
    }
    let consts = world
        .try_resource::<PhysicsWaterConstants>()
        .map(|r| *r)
        .unwrap_or_default();

    collect_water_surfaces(world, surfaces);
    collect_water_current_volumes(world, current_volumes);
    let time_secs = world.try_resource::<TotalTime>().map(|time| time.0);
    let atmospheric_wind = world
        .try_resource::<WindField>()
        .map(|wind| *wind)
        .unwrap_or_default();
    let (weather_scroll, wind_wave_scale) = time_secs
        .map(|time| weather_wave_adjustment(world, time))
        .unwrap_or(([0.0; 2], 1.0));
    let waves_require_rescan = time_secs.is_some() && waves_require_contact_rescan(world, surfaces);
    // Fast path: without a surface no body can remain buoyant. Always clear
    // stale contacts first because a placed current marker may outlive the
    // plane during streaming; only return immediately when it is absent too.
    if surfaces.is_empty() {
        clear_stale_water_contacts(world);
        if current_volumes.is_empty() {
            // No markers at all — no body can be inside one, so the #3268
            // latch is stale by definition. Dropping it here is what lets a
            // re-streamed marker wake its bodies again.
            in_current_prev.clear();
            return;
        }
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
        if pw.awake_counts().0 == 0 && !pw.pending_wake() && !had_newcomers && !waves_require_rescan
        {
            // #3268 — `in_current_prev` is deliberately NOT touched here.
            // Nothing moved, so a body asleep inside a current volume is
            // still inside it; clearing the latch would make the next real
            // scan read it as a fresh entry and re-wake a body that has
            // legitimately settled.
            return;
        }
    }

    // Union XZ footprint of every water plane — a cheap per-body reject that
    // avoids the collider AABB compute for the (vast) majority of bodies in a
    // large exterior cell with one small lake. The per-surface test below
    // still refines XZ + the vertical band; this only prunes far-away bodies.
    let (mut ux0, mut uz0, mut ux1, mut uz1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for s in surfaces.iter() {
        ux0 = ux0.min(s.volume.min[0]);
        uz0 = uz0.min(s.volume.min[2]);
        ux1 = ux1.max(s.volume.max[0]);
        uz1 = uz1.max(s.volume.max[2]);
    }
    for current in current_volumes.iter() {
        ux0 = ux0.min(current.volume.min[0]);
        uz0 = uz0.min(current.volume.min[2]);
        ux1 = ux1.max(current.volume.max[0]);
        uz1 = uz1.max(current.volume.max[2]);
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

    for (entity, handles) in handles_q.iter() {
        let Some(bd) = body_q.get(entity) else {
            continue;
        };
        if bd.motion_type != MotionType::Dynamic {
            continue;
        }
        let prior_contact = contact_q.as_ref().and_then(|cq| cq.get(entity)).copied();
        targets.push(BuoyancyTarget {
            entity,
            handles: *handles,
            authored_lin: bd.linear_damping,
            authored_ang: bd.angular_damping,
            prior_wet: prior_contact
                .map(|contact| contact.submerged_fraction > 0.0)
                .unwrap_or(false),
            prior_depth: prior_contact.map(|contact| contact.depth).unwrap_or(0.0),
        });
    }
    // #3492 — second source: ragdoll bodies. They are structurally invisible
    // to the pass above — `build_ragdoll` stores their handles only on the
    // `Ragdoll` component (no `RapierHandles` row is ever written for them),
    // and `activate_ragdoll`'s #1772 teardown deletes the bone entities' own
    // `RapierHandles` + `RigidBodyData` rows so the keyframed followers stop
    // fighting the multibody. Both halves of that are correct for their own
    // purposes; the gap was that this scan keys off exactly those two
    // components and had no `Ragdoll` arm, so a corpse in water sank at full
    // gravity with air damping and emitted no `WaterContact` for the splash /
    // audio / FX consumers downstream. The release path has had a `Ragdoll`
    // arm since #1531 — this mirrors it.
    //
    // Gated on the storage existing and being non-empty so scenes with no
    // actors pay nothing.
    if let Some(ragdoll_q) = world.query::<Ragdoll>() {
        for (_, ragdoll) in ragdoll_q.iter() {
            for ((bone, body, _), buoyancy) in ragdoll.bodies.iter().zip(&ragdoll.buoyancy) {
                let prior_contact = contact_q.as_ref().and_then(|cq| cq.get(*bone)).copied();
                targets.push(BuoyancyTarget {
                    entity: *bone,
                    handles: RapierHandles {
                        body: *body,
                        collider: buoyancy.collider,
                    },
                    authored_lin: buoyancy.linear_damping,
                    authored_ang: buoyancy.angular_damping,
                    prior_wet: prior_contact
                        .map(|contact| contact.submerged_fraction > 0.0)
                        .unwrap_or(false),
                    prior_depth: prior_contact.map(|contact| contact.depth).unwrap_or(0.0),
                });
            }
        }
    }

    drop(handles_q);
    drop(body_q);
    drop(contact_q);

    if targets.is_empty() {
        // No dynamic bodies — same reasoning as the no-marker return above.
        in_current_prev.clear();
        return;
    }

    // Buoyancy + damping under the write lock; collect the `WaterContact`
    // writes to apply after the lock drops.
    {
        let mut pw = world.resource_mut::<PhysicsWorld>();
        let gravity_y = pw.gravity.y;
        // Set once if any body is woken on a dry→wet transition this frame
        // (see the `pw.wake()` after the loop).
        let mut woke_any = false;
        for t in targets.iter() {
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

            let current_flow = if pos.x < ux0 || pos.x > ux1 || pos.z < uz0 || pos.z > uz1 {
                None
            } else {
                current_volumes
                    .iter()
                    .find(|current| {
                        let v = &current.volume;
                        pos.x >= v.min[0]
                            && pos.x <= v.max[0]
                            && pos.y >= v.min[1]
                            && pos.y <= v.max[1]
                            && pos.z >= v.min[2]
                            && pos.z <= v.max[2]
                    })
                    .map(|current| current.flow)
            };

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
                // #2887 — the collider AABB centre, NOT `pos.y` (the rigid
                // body's ORIGIN). They coincide only for a shape centred on
                // its body, which is exactly what this module's test balls
                // are and exactly what the bhk import path is not:
                // `collision_shape_to_parts` attaches every compound part at
                // its own local isometry, and ragdoll bones are offset by
                // construction. `submerged_fraction` already reads the AABB
                // span, so sorting and `depth` were the odd ones out.
                let center_y = 0.5 * (min_y + max_y);
                surfaces
                    .iter()
                    .filter_map(|s| {
                        let v = &s.volume;
                        if !(pos.x >= v.min[0]
                            && pos.x <= v.max[0]
                            && pos.z >= v.min[2]
                            && pos.z <= v.max[2]
                            && max_y >= v.min[1])
                        {
                            return None;
                        }
                        let surface_y = time_secs
                            .map(|time| {
                                authored_wave_height_with_weather(
                                    &s.material,
                                    Vec3::new(pos.x, pos.y, pos.z),
                                    time,
                                    weather_scroll,
                                    wind_wave_scale,
                                )
                            })
                            .unwrap_or(0.0)
                            + s.surface_y;
                        (min_y <= surface_y + WATERLINE_HYSTERESIS).then_some((s, surface_y))
                    })
                    .min_by(|a, b| {
                        nearest_surface_distance(a.1, center_y)
                            .total_cmp(&nearest_surface_distance(b.1, center_y))
                    })
                    .map(|(s, surface_y)| (s, min_y, max_y, surface_y))
            };

            // #3114 — ONE owner of the force clear per body per frame, before
            // any branch applies. Rapier's `add_force` accumulates into
            // `forces.user_force`, which persists across `pipeline.step()` and
            // is cleared only by `reset_forces`. The current-volume branch at
            // the bottom of this loop deliberately runs *after* the surface
            // branch so a plane's force reset cannot discard the marker's
            // current — correct for a body floating in a river, where the
            // surface branch resets first. But when the surface branch never
            // runs (`surface == None && !prior_wet`, or `frac == 0.0 &&
            // !prior_wet` — a body above the waterline inside the marker's
            // vertical extent, or in a marker that overlaps no plane in XZ)
            // nothing zeroed `user_force` and the per-frame term compounded.
            // For a body held at rest by friction the term is a constant
            // `m·c·s`, so `user_force` grew as `n·m·c·s` until it exceeded
            // static friction and launched the body. Resetting up here makes
            // the "marker after plane" ordering safe by construction.
            //
            // Scoped to bodies water is acting on rather than made
            // unconditional: `targets` is EVERY dynamic body, and
            // `PhysicsWorld::add_force`'s documented contract is that an
            // external force "accumulates across frames until cleared with
            // reset_forces". Clearing every dynamic body every frame would
            // silently turn that public API into a one-frame impulse.

            if current_flow.is_some() || surface.is_some() || t.prior_wet {
                pw.reset_forces(t.handles.body, false);
            }

            match surface {
                Some((s, min_y, max_y, surface_y)) => {
                    // Same AABB centre the surface search sorted by (#2887).
                    let center_y = 0.5 * (min_y + max_y);
                    // Carries the derived force out of the `bodies.get_mut`
                    // borrow so it can be applied through `PhysicsWorld`'s
                    // own API rather than the raw Rapier body (#2889).
                    let mut pending_force: Option<Vec3> = None;
                    let frac = submerged_fraction(min_y, max_y, surface_y);
                    if frac > 0.0 {
                        if let Some(b) = pw.bodies.get_mut(t.handles.body) {
                            // Submerged viscous damping so the float settles
                            // (and can then sleep) rather than bobbing forever.
                            b.set_linear_damping(consts.linear_damping_in);
                            b.set_angular_damping(consts.angular_damping_in);
                            // Wake ONCE on entry so a body that streamed in
                            // already submerged (spawned asleep) floats up.
                            if !t.prior_wet
                                || (b.is_sleeping()
                                    && (surface_y - center_y - t.prior_depth).abs() > 0.1)
                            {
                                b.wake_up(true);
                                woke_any = true;
                            }
                            // Apply lift + current only while awake — a settled,
                            // sleeping float is left at rest (gravity is also
                            // not integrated while asleep, so equilibrium
                            // holds). Entry wakes the body once above; a real
                            // current then keeps it moving naturally, while a
                            // body pinned against a bank may sleep again.
                            if !b.is_sleeping() {
                                let mass = b.mass();
                                let mut f = buoyancy_force(
                                    mass,
                                    frac,
                                    gravity_y,
                                    consts.buoyancy_density_ratio,
                                );
                                if let Some(flow) = s.flow {
                                    let velocity = b.linvel();
                                    f += current_force(
                                        flow,
                                        Vec3::new(velocity.x, velocity.y, velocity.z),
                                        mass,
                                        frac,
                                        consts.current_drag,
                                    );
                                }
                                let velocity = b.linvel();
                                f += wind_force(
                                    atmospheric_wind,
                                    time_secs.unwrap_or(0.0),
                                    Vec3::new(velocity.x, velocity.y, velocity.z),
                                    mass,
                                    1.0 - frac,
                                    consts.wind_drag,
                                );
                                // Derived under the borrow (it needs mass +
                                // linvel), applied through the wrapper after
                                // it ends — `wake_up = false` because this is
                                // re-derived every tick and waking here would
                                // pin the scene awake forever (#2889).
                                pending_force = Some(f);
                            }
                        }
                        if let Some(f) = pending_force.take() {
                            pw.add_force(t.handles.body, f, false);
                        }
                        writes.push((
                            t.entity,
                            WaterContact {
                                surface_entity: Some(s.entity),
                                depth: surface_y - center_y,
                                submerged_fraction: frac,
                                head_submerged: max_y <= surface_y,
                                flow: s.flow,
                                damage_per_second: s.damage_per_second,
                            },
                        ));
                    } else if t.prior_wet {
                        // The hysteresis band keeps a body eligible for the
                        // surface query a few units above the waterline, but
                        // it is already dry there. Restore the authored state
                        // at this boundary instead of clearing `prior_wet`
                        // while leaving damping/forces latched (#2870).
                        if let Some(b) = pw.bodies.get_mut(t.handles.body) {
                            b.set_linear_damping(t.authored_lin);
                            b.set_angular_damping(t.authored_ang);
                        }
                        writes.push((t.entity, WaterContact::default()));
                    }
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
                        }
                        writes.push((t.entity, WaterContact::default()));
                    }
                }
            }

            // Placed XWCU markers are current volumes, not water surfaces.
            // Apply their bounded drag after the surface branch so a
            // co-located water plane's force does not discard the marker's
            // current. Safe to add without resetting here because the single
            // reset above already ran for this body this frame (#3114).
            if let Some(flow) = current_flow {
                in_current_now.push(t.entity);
                // #3268 — one-shot wake on entering a current volume from
                // rest, mirroring the surface branch's `!prior_wet` edge.
                // Without it a body whose marker overlaps no `WaterPlane`
                // never passes through the surface branch at all, so nothing
                // ever wakes it and the `!is_sleeping()` gate below is
                // permanently false — the authored flow is silently inert.
                // Gated on the latch rather than fired every frame so a body
                // pinned against a bank is allowed to settle and stay
                // settled, which is the reason the force itself is applied
                // with `wake_up = false`.
                if in_current_prev.binary_search(&t.entity).is_err() {
                    if let Some(b) = pw.bodies.get_mut(t.handles.body) {
                        if b.is_sleeping() {
                            b.wake_up(true);
                            woke_any = true;
                        }
                    }
                }
                let current = pw.bodies.get(t.handles.body).and_then(|b| {
                    (!b.is_sleeping()).then(|| {
                        let velocity = b.linvel();
                        current_force(
                            flow,
                            Vec3::new(velocity.x, velocity.y, velocity.z),
                            b.mass(),
                            1.0,
                            consts.current_drag,
                        )
                    })
                });
                if let Some(f) = current {
                    // Per-frame term, so no wake — see the surface branch.
                    pw.add_force(t.handles.body, f, false);
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
        // #3268 — publish this scan's membership as the next one's prior.
        // Sorted so the per-body lookup above stays a binary search; both
        // halves keep their allocations across the swap.
        in_current_now.sort_unstable();
        std::mem::swap(in_current_prev, in_current_now);
    }

    if !writes.is_empty() {
        match world.query_mut::<WaterContact>() {
            Some(mut wq) => {
                for (entity, contact) in writes.drain(..) {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::iso_from_trs;
    use crate::world::{PhysicsWorld, PHYSICS_DT};
    use byroredux_core::ecs::components::water::WaterCurrentVolume;
    use byroredux_core::math::{Quat, Vec3};
    use rapier3d::prelude::*;

    const G_Y: f32 = -686.7; // PhysicsWorld gravity, BU/s².

    #[test]
    fn placed_current_volume_is_collected_without_becoming_a_water_surface() {
        let mut world = World::new();
        let entity = world.spawn();
        world.insert(
            entity,
            WaterCurrentVolume {
                volume: WaterVolume {
                    min: [-10.0, -4.0, -10.0],
                    max: [10.0, 4.0, 10.0],
                },
                flow: WaterFlow::new([1.0, 0.0, 0.0], 2.0),
            },
        );

        let mut currents = Vec::new();
        collect_water_current_volumes(&world, &mut currents);
        assert_eq!(currents.len(), 1);
        assert_eq!(currents[0].flow.direction, [1.0, 0.0, 0.0]);
        assert!(world.query::<WaterPlane>().is_none());
    }

    #[test]
    fn default_authored_waves_only_rescan_nearby_live_contacts() {
        let mut world = World::new();
        world.register::<WaterContact>();
        let surface_entity = world.spawn();
        let surfaces = [WaterSurface {
            entity: surface_entity,
            volume: WaterVolume {
                min: [-10.0, -10.0, -10.0],
                max: [10.0, 0.0, 10.0],
            },
            surface_y: 0.0,
            material: WaterMaterial::default(),
            flow: None,
            damage_per_second: 0.0,
        }];

        assert!(
            !waves_require_contact_rescan(&world, &surfaces),
            "the canonical non-zero wave default must not disarm quiescence by itself"
        );

        let body = world.spawn();
        world.insert(
            body,
            WaterContact {
                surface_entity: Some(surface_entity),
                depth: WaterMaterial::default().wave_amplitude * 0.5,
                submerged_fraction: 0.0,
                ..WaterContact::default()
            },
        );
        assert!(
            waves_require_contact_rescan(&world, &surfaces),
            "a live contact inside the crest band must continue to track authored waves"
        );
    }

    #[test]
    fn overlapping_water_volumes_choose_nearest_surface() {
        use crate::physics_sync_system;
        use byroredux_core::ecs::components::collision::{
            CollisionShape, MotionType, RigidBodyData,
        };
        use byroredux_core::ecs::components::{GlobalTransform, Transform};

        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        world.insert_resource(PhysicsWaterConstants::default());
        world.insert_resource(WaterContactScratch::default());
        world.register::<RapierHandles>();
        world.register::<WaterContact>();

        let mut upper_surface = None;
        for (surface_y, damage_per_second) in [(0.0, 1.0), (3.0, 7.0)] {
            let water = world.spawn();
            world.insert(
                water,
                WaterPlane {
                    kind: byroredux_core::ecs::components::water::WaterKind::Calm,
                    material: WaterMaterial::default(),
                    damage_per_second,
                },
            );
            world.insert(
                water,
                WaterVolume {
                    min: [-50.0, -100.0, -50.0],
                    max: [50.0, surface_y, 50.0],
                },
            );
            if surface_y == 3.0 {
                upper_surface = Some(water);
            }
        }
        let current = world.spawn();
        world.insert(
            current,
            WaterCurrentVolume {
                volume: WaterVolume {
                    min: [100.0, -10.0, 100.0],
                    max: [110.0, 10.0, 110.0],
                },
                flow: WaterFlow::new([1.0, 0.0, 0.0], 1.0),
            },
        );

        let body = world.spawn();
        world.insert(body, CollisionShape::Ball { radius: 1.0 });
        world.insert(
            body,
            RigidBodyData {
                motion_type: MotionType::Dynamic,
                mass: 20.0,
                friction: 0.5,
                restitution: 0.0,
                linear_damping: 0.0,
                angular_damping: 0.0,
                collidable: true,
            },
        );
        let position = Vec3::new(0.0, 2.0, 0.0);
        world.insert(body, GlobalTransform::new(position, Quat::IDENTITY, 1.0));
        world.insert(body, Transform::from_translation(position));

        physics_sync_system(&world, PHYSICS_DT);

        let first_capacities = {
            let scratch = world.resource::<WaterContactScratch>();
            (
                scratch.surfaces.capacity(),
                scratch.current_volumes.capacity(),
                scratch.targets.capacity(),
                scratch.writes.capacity(),
            )
        };
        assert!(first_capacities.0 >= 2);
        assert!(first_capacities.1 >= 1);
        assert!(first_capacities.2 >= 1);
        assert!(first_capacities.3 >= 1);
        let contact = world
            .get::<WaterContact>(body)
            .expect("nearest surface writes a contact");
        assert_eq!(contact.surface_entity, upper_surface);
        assert_eq!(contact.depth, 1.0);
        assert_eq!(contact.damage_per_second, 7.0);
        drop(contact);

        physics_sync_system(&world, PHYSICS_DT);
        let scratch = world.resource::<WaterContactScratch>();
        assert_eq!(
            (
                scratch.surfaces.capacity(),
                scratch.current_volumes.capacity(),
                scratch.targets.capacity(),
                scratch.writes.capacity(),
            ),
            first_capacities,
            "every buoyancy staging allocation must survive across physics ticks"
        );
    }

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
        assert!(
            (f_neutral.y - weight).abs() < 1e-3,
            "neutral F = {f_neutral:?}"
        );
        // submerged_fraction clamps — 1.5 must not exceed full submersion.
        assert_eq!(
            buoyancy_force(10.0, 1.5, G_Y, 2.0).y,
            buoyancy_force(10.0, 1.0, G_Y, 2.0).y
        );
    }

    #[test]
    fn authored_wave_height_is_finite_bounded_and_time_varying() {
        let material = WaterMaterial {
            wave_amplitude: 4.0,
            wave_frequency: 1.0,
            scroll_a: [0.0228254, 0.0],
            scroll_b: [0.0, 0.0286531],
            ..WaterMaterial::default()
        };
        let position = Vec3::new(113.0, 0.0, -47.0);
        let start = authored_wave_height_with_weather(&material, position, 0.0, [0.0; 2], 1.0);
        let later = authored_wave_height_with_weather(&material, position, 0.37, [0.0; 2], 1.0);
        assert!(start.is_finite() && later.is_finite());
        assert!(start.abs() <= 4.0 && later.abs() <= 4.0);
        assert!((start - later).abs() > 1.0e-4);
    }

    #[test]
    fn authored_wave_height_tracks_weather_scroll_and_amplitude() {
        let material = WaterMaterial {
            wave_amplitude: 4.0,
            wave_frequency: 0.7,
            scroll_a: [0.0228254, 0.0],
            scroll_b: [0.0, 0.0286531],
            ..WaterMaterial::default()
        };
        let position = Vec3::new(37.0, 0.0, 91.0);
        let calm = authored_wave_height_with_weather(&material, position, 2.3, [0.0, 0.0], 1.0);
        let wind_x = authored_wave_height_with_weather(&material, position, 2.3, [0.2, 0.0], 1.0);
        let amplified =
            authored_wave_height_with_weather(&material, position, 2.3, [0.0, 0.0], 1.5);
        assert!((calm - wind_x).abs() > 1.0e-4);
        assert!((amplified - calm * 1.5).abs() < 1.0e-5);
    }

    #[test]
    fn authored_wave_height_uses_weather_for_crest_direction() {
        // With zero temporal frequency, any change here must come from the
        // post-weather spatial direction (the same normalized scroll vector
        // used by water.vert), not from a rate-only adjustment.
        let material = WaterMaterial {
            wave_amplitude: 4.0,
            wave_frequency: 0.0,
            scroll_a: [1.0, 0.0],
            scroll_b: [0.0, 0.0],
            uv_scale_a: 0.01,
            uv_scale_b: 0.01,
            ..WaterMaterial::default()
        };
        let position = Vec3::new(0.0, 0.0, 100.0);
        let calm = authored_wave_height_with_weather(&material, position, 0.0, [0.0, 0.0], 1.0);
        let wind = authored_wave_height_with_weather(&material, position, 0.0, [0.0, 1.0], 1.0);
        assert!(calm.abs() < 1.0e-5);
        assert!(wind.abs() > 1.0e-3);
    }

    #[test]
    fn negative_weather_gust_keeps_cpu_water_surface_calm() {
        let mut world = World::new();
        world.insert_resource(WindField {
            direction: [1.0, 0.0],
            speed: 0.0,
            gust_amplitude: -100.0,
            gust_frequency: 0.25,
        });

        let (scroll, scale) = weather_wave_adjustment(&world, 1.0);
        assert_eq!(scroll, [0.0, 0.0]);
        assert_eq!(scale, 1.0);
    }

    #[test]
    fn authored_wave_height_tracks_nam1_surface_rotation() {
        let mut static_surface = WaterMaterial {
            wave_amplitude: 4.0,
            wave_frequency: 0.7,
            scroll_a: [0.0228254, 0.0],
            scroll_b: [0.0, 0.0286531],
            ..WaterMaterial::default()
        };
        let position = Vec3::new(37.0, 0.0, 91.0);
        let static_height =
            authored_wave_height_with_weather(&static_surface, position, 1.0, [0.0; 2], 1.0);
        static_surface.angular_velocity = std::f32::consts::FRAC_PI_2;
        let rotating_height =
            authored_wave_height_with_weather(&static_surface, position, 1.0, [0.0; 2], 1.0);
        assert!((static_height - rotating_height).abs() > 1.0e-4);
    }

    #[test]
    fn current_force_matches_velocity_along_normalized_flow_axis() {
        let flow = WaterFlow {
            // Deliberately non-unit: the helper owns the final normalization.
            direction: [2.0, 0.0, 0.0],
            speed: 8.0,
        };
        let mass = 10.0;
        let drag = 4.0;

        let from_rest = current_force(flow, Vec3::ZERO, mass, 0.5, drag);
        assert_eq!(from_rest, Vec3::new(160.0, 0.0, 0.0));

        let at_target = current_force(flow, Vec3::new(8.0, 20.0, -3.0), mass, 1.0, drag);
        assert_eq!(at_target, Vec3::ZERO);

        let faster_than_current = current_force(flow, Vec3::new(10.0, 0.0, 0.0), mass, 1.0, drag);
        assert_eq!(faster_than_current, Vec3::new(-80.0, 0.0, 0.0));
    }

    #[test]
    fn current_force_is_zero_when_dry_or_flow_is_invalid() {
        let flow = WaterFlow {
            direction: [1.0, 0.0, 0.0],
            speed: 8.0,
        };
        assert_eq!(current_force(flow, Vec3::ZERO, 10.0, 0.0, 4.0), Vec3::ZERO);
        assert_eq!(
            current_force(
                WaterFlow {
                    direction: [0.0; 3],
                    speed: 8.0,
                },
                Vec3::ZERO,
                10.0,
                1.0,
                4.0,
            ),
            Vec3::ZERO
        );
        assert_eq!(
            current_force(
                WaterFlow {
                    direction: [f32::NAN, 0.0, 0.0],
                    speed: 8.0,
                },
                Vec3::ZERO,
                10.0,
                1.0,
                4.0,
            ),
            Vec3::ZERO
        );
    }

    #[test]
    fn wind_force_targets_exposed_fraction_in_shared_xz_direction() {
        let wind = WindField {
            direction: [0.0, 1.0],
            speed: 10.0,
            gust_amplitude: 0.0,
            gust_frequency: 0.0,
        };
        let force = wind_force(wind, 0.0, Vec3::ZERO, 10.0, 0.5, 0.35);
        assert_eq!(force, Vec3::new(0.0, 0.0, 17.5));
        assert_eq!(
            wind_force(wind, 0.0, Vec3::new(0.0, 0.0, 10.0), 10.0, 1.0, 0.35),
            Vec3::ZERO
        );
        assert_eq!(
            wind_force(wind, 0.0, Vec3::ZERO, 10.0, 0.0, 0.35),
            Vec3::ZERO
        );
    }

    #[test]
    fn wind_force_clamps_negative_gust_like_water_and_speedtree() {
        let wind = WindField {
            direction: [1.0, 0.0],
            speed: 0.0,
            gust_amplitude: -100.0,
            gust_frequency: 0.25,
        };
        assert_eq!(
            wind_force(wind, 1.0, Vec3::ZERO, 10.0, 1.0, 0.35),
            Vec3::ZERO
        );
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
        w.colliders
            .insert_with_parent(ColliderBuilder::ball(radius).build(), h, &mut w.bodies);
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
            w.reset_forces(h, true);
            let f = buoyancy_force(mass, frac, G_Y, consts.buoyancy_density_ratio);
            w.add_force(h, f, true);
            w.step(PHYSICS_DT);
        }

        let y = w.bodies[h].translation().y;
        assert!(
            y > y_start,
            "body must rise from its release depth; {y} !> {y_start}"
        );
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
    /// Shared fixture for the two live buoyancy tests: a deep calm column
    /// with a +X current, surface at `y = 0`, and one dynamic ball released
    /// fully submerged at `start_y`. Returns `(world, water, body)`.
    ///
    /// Factored out under #2871 so the sub-tick companion below exercises
    /// byte-identical physics content and differs only in the `frame_dt` it
    /// drives `physics_sync_system` with — the one variable under test.
    fn submerged_ball_world(start_y: f32) -> (World, EntityId, EntityId) {
        use crate::RapierHandles;
        use byroredux_core::ecs::components::collision::{
            CollisionShape, MotionType, RigidBodyData,
        };
        use byroredux_core::ecs::components::water::{
            WaterContact, WaterFlow, WaterKind, WaterMaterial, WaterPlane, WaterVolume,
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
                damage_per_second: 9.0,
            },
        );
        world.insert(
            water,
            WaterVolume {
                min: [-500.0, -200.0, -500.0],
                max: [500.0, surface_y, 500.0],
            },
        );
        world.insert(
            water,
            WaterFlow {
                direction: [1.0, 0.0, 0.0],
                speed: 8.0,
            },
        );

        let radius = 10.0_f32;
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
                collidable: true,
            },
        );
        world.insert(
            body,
            GlobalTransform::new(Vec3::new(0.0, start_y, 0.0), Quat::IDENTITY, 1.0),
        );
        world.insert(
            body,
            Transform::from_translation(Vec3::new(0.0, start_y, 0.0)),
        );
        (world, water, body)
    }

    /// Regression for #2887. `WaterContact::depth` is documented as "Surface
    /// Y minus the body's **centre** Y", but it was computed from
    /// `body.translation().y` — the rigid body's ORIGIN. Those coincide only
    /// for a shape centred on its body, which is what every test ball in this
    /// module is and what the bhk import path is not:
    /// `collision_shape_to_parts` attaches each compound part at its own
    /// local isometry, and ragdoll bones are offset by construction.
    ///
    /// A compound whose only leaf sits well below the origin makes the two
    /// disagree by a known amount, so the assertion can name which one it got.
    #[test]
    fn depth_is_measured_from_the_collider_aabb_centre_not_the_body_origin() {
        use crate::physics_sync_system;
        use byroredux_core::ecs::components::collision::CollisionShape;
        use byroredux_core::ecs::components::water::WaterContact;
        use byroredux_core::ecs::components::Transform;

        // Reuse the calm-column fixture, then swap the centred ball for a
        // compound whose leaf hangs 40 BU below the body origin.
        const OFFSET: f32 = -40.0;
        const RADIUS: f32 = 10.0;
        let body_origin_y = -30.0_f32;
        let (mut world, _water, body) = submerged_ball_world(body_origin_y);
        world.insert(
            body,
            CollisionShape::Compound {
                children: vec![(
                    Vec3::new(0.0, OFFSET, 0.0),
                    Quat::IDENTITY,
                    Box::new(CollisionShape::Ball { radius: RADIUS }),
                )],
            },
        );

        // One frame: registers the newcomer and writes the first contact.
        // Read before the solver has moved anything, so the geometry under
        // test is exactly what was authored.
        physics_sync_system(&world, PHYSICS_DT);

        let placed_y = world
            .get::<Transform>(body)
            .expect("transform present")
            .translation
            .y;
        assert!(
            (placed_y - body_origin_y).abs() < 1.0,
            "fixture precondition: the body has not moved far yet ({placed_y})"
        );
        let contact = world
            .get::<WaterContact>(body)
            .expect("WaterContact written");
        assert!(
            contact.submerged_fraction > 0.0,
            "fixture precondition: the offset collider is under the surface"
        );

        // Surface is y=0, so depth is just -centre_y. The AABB centre sits at
        // `origin + OFFSET`; the body origin is 40 BU higher.
        let from_aabb_centre = -(body_origin_y + OFFSET);
        let from_body_origin = -body_origin_y;
        assert!(
            (contact.depth - from_aabb_centre).abs() < 1.5,
            "depth must be measured from the collider AABB centre \
             (expected ≈{from_aabb_centre}); got {} — {} is the body-origin \
             value the doc does not promise",
            contact.depth,
            from_body_origin
        );
    }

    /// Regression for #2871 / #2856. Every other live buoyancy test drives
    /// `physics_sync_system` with `dt` exactly `PHYSICS_DT`, so the
    /// accumulator always reaches one substep on the very first call — the
    /// suite was structurally blind to the above-60 fps case.
    ///
    /// At `frame_dt < PHYSICS_DT` the dry→wet transition frame runs **zero**
    /// substeps. Pre-#2856 `step` consumed `pending_wake` anyway; the next
    /// frame then saw `prior_wet == true` (so buoyancy raised no new wake),
    /// an empty `active_dynamic_bodies` (the island lists only update inside
    /// `pipeline.step`), and `pending_wake == false` — which satisfies BOTH
    /// the buoyancy quiesced guard and the static-scene step fast path. The
    /// latter zeroes the accumulator, so the banked sub-tick time could never
    /// reach `PHYSICS_DT` and the body hung mid-water-column indefinitely.
    ///
    /// This drives the same fixture at ~240 fps and requires the float to
    /// rise: the wake must survive however many sub-tick frames it takes to
    /// bank one full tick.
    #[test]
    fn buoyancy_survives_sub_tick_frames_above_sixty_fps() {
        use crate::physics_sync_system;
        use byroredux_core::ecs::components::water::WaterContact;
        use byroredux_core::ecs::components::Transform;

        let start_y = -60.0_f32;
        let (world, _water, body) = submerged_ball_world(start_y);

        // ~240 fps: four frames per physics tick, so the transition frame
        // itself runs zero substeps.
        let frame_dt = PHYSICS_DT / 4.0;
        assert!(frame_dt < PHYSICS_DT, "fixture must be a sub-tick frame");

        // One frame: registers the newcomer and takes the dry→wet
        // transition. No substep can have run yet.
        physics_sync_system(&world, frame_dt);
        assert!(
            world
                .get::<WaterContact>(body)
                .is_some_and(|contact| contact.submerged_fraction > 0.0),
            "the newcomer escape hatch must see the body as submerged on \
             its first frame"
        );

        // ~10 s of wall clock at 240 fps.
        for _ in 0..2400 {
            physics_sync_system(&world, frame_dt);
        }

        let y = world
            .get::<Transform>(body)
            .expect("transform present")
            .translation
            .y;
        assert!(
            y > start_y + 1.0,
            "a body that streams in submerged must float up at above-60 fps \
             frame rates too — it hung at its release depth (y={y}, released \
             at {start_y}), which is the #2871 wake/quiesce latch"
        );
    }

    #[test]
    fn body_in_water_volume_floats_and_drifts_via_physics_sync() {
        use crate::physics_sync_system;
        use byroredux_core::ecs::components::water::{
            WaterContact, WaterFlow, WaterPlane, WaterVolume,
        };
        use byroredux_core::ecs::components::Transform;

        let surface_y = 0.0_f32;
        let radius = 10.0_f32;
        let start_y = -5.0_f32;
        let (mut world, water, body) = submerged_ball_world(start_y);
        // ~10 s through the live system (registers on the first call, then
        // wakes-on-entry + applies buoyancy every tick until it sleeps).
        for _ in 0..600 {
            physics_sync_system(&world, PHYSICS_DT);
        }

        let position = world
            .get::<Transform>(body)
            .expect("transform present")
            .translation;
        let y = position.y;
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
        assert!(
            position.x > 5.0,
            "flowing water must carry the body downstream; x={}",
            position.x
        );

        let contact = world
            .get::<WaterContact>(body)
            .expect("WaterContact written");
        assert!(
            contact.submerged_fraction > 0.0 && contact.submerged_fraction <= 1.0,
            "submerged_fraction out of range: {}",
            contact.submerged_fraction
        );
        assert_eq!(contact.surface_entity, Some(water));
        assert_eq!(
            contact.flow.map(|flow| flow.direction),
            Some([1.0, 0.0, 0.0]),
            "contact carries the same current consumed by the solver"
        );
        assert_eq!(contact.damage_per_second, 9.0);
        drop(contact);

        // Streaming can remove the water plane before the dynamic body is
        // reclaimed while an independent current marker remains resident.
        // The no-surface path must still clear the canonical contact instead
        // of returning through the quiesced current-only fast path (#3127).
        let marker = world.spawn();
        world.insert(
            marker,
            WaterCurrentVolume {
                volume: WaterVolume {
                    min: [-500.0, -500.0, -500.0],
                    max: [500.0, 500.0, 500.0],
                },
                flow: WaterFlow::new([1.0, 0.0, 0.0], 8.0),
            },
        );
        world.remove::<WaterPlane>(water);
        world.remove::<WaterVolume>(water);
        world.remove::<WaterFlow>(water);
        physics_sync_system(&world, PHYSICS_DT);
        let cleared = world
            .get::<WaterContact>(body)
            .expect("dry transition keeps a contact breadcrumb");
        assert_eq!(cleared.submerged_fraction, 0.0);
        assert!(cleared.surface_entity.is_none());
    }

    /// A buoyant body settling at equilibrium must fall asleep, and once
    /// asleep the static-scene fast path (`PhysicsWorld::step` returning 0
    /// substeps) must re-engage — buoyancy's continuous per-frame force
    /// application must not keep the body (and thus the whole sim) awake
    /// forever.
    #[test]
    fn buoyant_body_sleeps_so_static_fast_path_re_engages() {
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
        world.insert_resource(PhysicsWaterConstants {
            buoyancy_density_ratio: 1.02, // near-neutral: equilibrium frac ~ 0.98, AABB top hugs the surface band
            ..PhysicsWaterConstants::default()
        });
        world.register::<RapierHandles>();
        world.register::<WaterContact>();

        let surface_y = 0.0_f32;
        let water = world.spawn();
        world.insert(
            water,
            WaterPlane {
                kind: WaterKind::Calm,
                material: WaterMaterial::default(),
                damage_per_second: 0.0,
            },
        );
        world.insert(
            water,
            WaterVolume {
                min: [-500.0, -200.0, -500.0],
                max: [500.0, surface_y, 500.0],
            },
        );

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
                collidable: true,
            },
        );
        world.insert(
            body,
            GlobalTransform::new(Vec3::new(0.0, start_y, 0.0), Quat::IDENTITY, 1.0),
        );
        world.insert(
            body,
            Transform::from_translation(Vec3::new(0.0, start_y, 0.0)),
        );

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
        let steps = {
            let mut pw = world.resource_mut::<PhysicsWorld>();
            pw.step(PHYSICS_DT)
        };
        assert_eq!(
            ad, 0,
            "buoyant body must sleep at equilibrium so the fast path re-engages"
        );
        assert_eq!(
            steps, 0,
            "static-scene fast path must skip stepping once the float sleeps"
        );
    }

    /// #3114 — `add_force` accumulates into `forces.user_force`, which persists
    /// across `pipeline.step()` and is cleared only by `reset_forces`. The
    /// placed-`XWCU` current branch ran *after* the surface branch and did not
    /// reset, which is correct only when the surface branch reset first. With a
    /// current marker that overlaps no water plane the surface branch never
    /// runs, so the per-frame term compounded: for a body held near rest the
    /// term is a constant `m·c·s`, giving `user_force ~ n·m·c·s` — unbounded
    /// linear growth until the body is launched ("havok explosion"), and a body
    /// under a growing force never sleeps, pinning the static-scene fast path.
    #[test]
    /// #3268 — sibling of the wind-up test below, with its manual
    /// per-iteration `wake_up` removed. That workaround was the bug: the
    /// current branch is gated on `!is_sleeping()` and only the SURFACE
    /// branch ever woke anything, so a body resting inside a marker that
    /// overlaps no `WaterPlane` was never woken by anything and the authored
    /// flow was silently inert.
    ///
    /// Two worlds, identical but for the marker. The control proves the
    /// trigger condition is real — a freshly streamed dynamic body spawns
    /// asleep (EXTERIOR-FREEZE, `sync.rs`) and nothing else in this fixture
    /// moves it along X — and doubles as a guard against "fix" it by waking
    /// every body every frame.
    #[test]
    fn current_volume_without_a_water_plane_wakes_a_body_resting_in_it() {
        use crate::{physics_sync_system, RapierHandles};
        use byroredux_core::ecs::components::collision::{
            CollisionShape, MotionType, RigidBodyData,
        };
        use byroredux_core::ecs::components::{GlobalTransform, Transform};
        use byroredux_core::ecs::{EntityId, World};

        fn fixture(with_current: bool) -> (World, EntityId) {
            let mut world = World::new();
            world.insert_resource(PhysicsWorld::new());
            world.insert_resource(PhysicsWaterConstants::default());
            // The production shape: with the resource present the in-current
            // latch persists across ticks, so the wake is genuinely one-shot
            // rather than re-fired every frame by a fresh default scratch.
            world.insert_resource(WaterContactScratch::default());
            world.register::<RapierHandles>();
            world.register::<WaterContact>();

            if with_current {
                let marker = world.spawn();
                world.insert(
                    marker,
                    WaterCurrentVolume {
                        volume: WaterVolume {
                            min: [-500.0, -500.0, -500.0],
                            max: [500.0, 500.0, 500.0],
                        },
                        flow: WaterFlow::new([1.0, 0.0, 0.0], 8.0),
                    },
                );
            }
            assert!(
                world.query::<WaterPlane>().is_none(),
                "the whole point of this fixture is that no water surface exists"
            );

            let floor = world.spawn();
            world.insert(
                floor,
                CollisionShape::Cuboid {
                    half_extents: Vec3::new(400.0, 10.0, 400.0),
                },
            );
            world.insert(
                floor,
                RigidBodyData {
                    motion_type: MotionType::Static,
                    mass: 0.0,
                    friction: 1.0,
                    restitution: 0.0,
                    linear_damping: 0.0,
                    angular_damping: 0.0,
                    collidable: true,
                },
            );
            world.insert(
                floor,
                GlobalTransform::new(Vec3::new(0.0, -30.0, 0.0), Quat::IDENTITY, 1.0),
            );
            world.insert(
                floor,
                Transform::from_translation(Vec3::new(0.0, -30.0, 0.0)),
            );

            let body = world.spawn();
            world.insert(body, CollisionShape::Ball { radius: 10.0 });
            world.insert(
                body,
                RigidBodyData {
                    motion_type: MotionType::Dynamic,
                    mass: 20.0,
                    friction: 1.0,
                    restitution: 0.0,
                    linear_damping: 0.0,
                    angular_damping: 0.0,
                    collidable: true,
                },
            );
            world.insert(
                body,
                GlobalTransform::new(Vec3::new(0.0, -9.0, 0.0), Quat::IDENTITY, 1.0),
            );
            world.insert(body, Transform::from_translation(Vec3::new(0.0, -9.0, 0.0)));
            (world, body)
        }

        fn velocity_x(world: &World, body: EntityId) -> f32 {
            let handles = *world.query::<RapierHandles>().unwrap().get(body).unwrap();
            let pw = world.resource::<PhysicsWorld>();
            pw.bodies
                .get(handles.body)
                .map(|b| b.linvel().x)
                .unwrap_or(0.0)
        }

        // No manual wake in either loop — that workaround is what this test
        // exists to remove.
        let (control, control_body) = fixture(false);
        for _ in 0..60 {
            physics_sync_system(&control, PHYSICS_DT);
        }
        assert_eq!(
            velocity_x(&control, control_body),
            0.0,
            "control: without a current marker nothing may push the body along X"
        );

        let (world, body) = fixture(true);
        for _ in 0..60 {
            physics_sync_system(&world, PHYSICS_DT);
        }
        assert!(
            velocity_x(&world, body) > 1.0,
            "body never accelerated under the authored 8.0 BU/s current \
             (vx = {}) — nothing woke it, so the current branch never ran \
             (#3268)",
            velocity_x(&world, body)
        );
    }

    #[test]
    fn current_volume_without_a_water_plane_does_not_wind_up_user_force() {
        use crate::{physics_sync_system, RapierHandles};
        use byroredux_core::ecs::components::collision::{
            CollisionShape, MotionType, RigidBodyData,
        };
        use byroredux_core::ecs::components::{GlobalTransform, Transform};
        use byroredux_core::ecs::World;

        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        world.insert_resource(PhysicsWaterConstants::default());
        world.register::<RapierHandles>();
        world.register::<WaterContact>();

        // An authored current marker and NO WaterPlane anywhere — the exact
        // configuration in which the surface branch cannot run.
        let marker = world.spawn();
        world.insert(
            marker,
            WaterCurrentVolume {
                volume: WaterVolume {
                    min: [-500.0, -500.0, -500.0],
                    max: [500.0, 500.0, 500.0],
                },
                flow: WaterFlow::new([1.0, 0.0, 0.0], 8.0),
            },
        );
        assert!(
            world.query::<WaterPlane>().is_none(),
            "the whole point of this fixture is that no water surface exists"
        );

        // A static floor is load-bearing for this test, not scenery. The
        // wind-up only diverges while the body is held near rest: then the
        // per-frame term `m·c·(s - v·d)` is a CONSTANT `m·c·s` and the sum
        // grows linearly. A free-falling body accelerates, `v·d` overshoots
        // `s`, the term flips sign and the accumulator self-corrects — so a
        // fixture without a floor passes against the bug and proves nothing.
        let floor = world.spawn();
        world.insert(
            floor,
            CollisionShape::Cuboid {
                half_extents: Vec3::new(400.0, 10.0, 400.0),
            },
        );
        world.insert(
            floor,
            RigidBodyData {
                motion_type: MotionType::Static,
                mass: 0.0,
                friction: 1.0,
                restitution: 0.0,
                linear_damping: 0.0,
                angular_damping: 0.0,
                collidable: true,
            },
        );
        world.insert(
            floor,
            GlobalTransform::new(Vec3::new(0.0, -30.0, 0.0), Quat::IDENTITY, 1.0),
        );
        world.insert(
            floor,
            Transform::from_translation(Vec3::new(0.0, -30.0, 0.0)),
        );

        let body = world.spawn();
        world.insert(body, CollisionShape::Ball { radius: 10.0 });
        world.insert(
            body,
            RigidBodyData {
                motion_type: MotionType::Dynamic,
                mass: 20.0,
                friction: 1.0,
                restitution: 0.0,
                linear_damping: 0.0,
                angular_damping: 0.0,
                collidable: true,
            },
        );
        world.insert(
            body,
            GlobalTransform::new(Vec3::new(0.0, -9.0, 0.0), Quat::IDENTITY, 1.0),
        );
        world.insert(body, Transform::from_translation(Vec3::new(0.0, -9.0, 0.0)));

        // Register first so there is a handle to hold awake.
        physics_sync_system(&world, PHYSICS_DT);

        let force_of = |world: &World| -> (f32, f32) {
            let handles = *world.query::<RapierHandles>().unwrap().get(body).unwrap();
            let pw = world.resource::<PhysicsWorld>();
            let b = pw.bodies.get(handles.body).expect("body is registered");
            let f = b.user_force();
            let v = b.linvel();
            ((f.x * f.x + f.y * f.y + f.z * f.z).sqrt(), v.x)
        };

        let mut peak = 0.0_f32;
        for _ in 0..120 {
            // Hold the body awake and armed. Both are load-bearing and neither
            // is incidental to the defect: the current branch is gated on
            // `!is_sleeping()`, and `apply_buoyancy`'s quiesced-scene fast path
            // returns before the per-body scan unless something is awake or
            // `pending_wake` is armed. Nothing in the current-volume path wakes
            // a body itself — only the SURFACE branch calls `wake_up` — so a
            // marker with no overlapping plane needs an independent
            // disturbance, which is exactly this issue's trigger condition.
            let handles = *world.query::<RapierHandles>().unwrap().get(body).unwrap();
            {
                let mut pw = world.resource_mut::<PhysicsWorld>();
                if let Some(b) = pw.bodies.get_mut(handles.body) {
                    b.wake_up(true);
                }
                pw.wake();
            }
            physics_sync_system(&world, PHYSICS_DT);
            let (magnitude, _) = force_of(&world);
            assert!(magnitude.is_finite(), "user_force went non-finite");
            peak = peak.max(magnitude);
        }

        // `current_force` documents a bounded first-order response: with the
        // body at rest the term is at most `m·c·s` = 20 × 4 × 8 = 640, and it
        // decays as the body's velocity approaches the authored current.
        let bound = 20.0 * 4.0 * 8.0;
        assert!(
            peak <= bound * 1.05,
            "user_force peaked at {peak}, above the bounded response {bound} — \
             the per-frame current term is compounding across steps (#3114). \
             Pre-fix this fixture peaks around 2900 and oscillates."
        );

        // Convergence, not oscillation: an accumulating force is an integral
        // controller where a proportional one was intended, so it overshoots
        // the authored speed and rings instead of settling.
        let (final_force, final_vx) = force_of(&world);
        assert!(
            final_force < peak * 0.05,
            "user_force settled at {final_force} against a {peak} peak — a \
             first-order response should have decayed to near zero (#3114)"
        );
        assert!(
            final_vx <= 8.0 * 1.05 && final_vx > 8.0 * 0.9,
            "body settled at {final_vx} BU/s against an authored current of 8.0 \
             — overshoot or undershoot means the response is not proportional"
        );
    }
}

/// #3492 — ragdoll bodies must reach the buoyancy sink.
///
/// They are invisible to the `RapierHandles` + `RigidBodyData` scan by
/// construction: `build_ragdoll` records their handles only on the
/// `Ragdoll` component, and `activate_ragdoll`'s #1772 teardown deletes the
/// bone entities' own rows of both selectors. Before the second pass, a
/// corpse in water sank at full gravity with air damping and emitted no
/// `WaterContact` at all.
#[cfg(test)]
pub(crate) mod ragdoll_buoyancy_tests {
    use super::*;
    use crate::config::ContactConfig;
    use crate::ragdoll::{build_ragdoll, RagdollBodySpec, RagdollSpec};
    use byroredux_core::ecs::components::collision::CollisionShape;
    use byroredux_core::ecs::components::water::{
        WaterContact, WaterKind, WaterMaterial, WaterPlane, WaterVolume,
    };
    use byroredux_core::math::{Quat, Vec3};

    /// A world whose only dynamic body is one ragdoll limb, seeded below a
    /// water surface at y=0. Deliberately registers `RapierHandles` and
    /// `RigidBodyData` but writes **no rows** for the bone — exactly the
    /// state `activate_ragdoll` leaves behind (#1772).
    pub(crate) fn submerged_ragdoll_world(start_y: f32) -> (World, EntityId) {
        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        world.insert_resource(PhysicsWaterConstants::default());
        world.register::<RapierHandles>();
        world.register::<RigidBodyData>();
        world.register::<WaterContact>();
        world.register::<Ragdoll>();

        let water = world.spawn();
        world.insert(
            water,
            WaterPlane {
                kind: WaterKind::Calm,
                material: WaterMaterial::default(),
                damage_per_second: 0.0,
            },
        );
        world.insert(
            water,
            WaterVolume {
                min: [-500.0, -200.0, -500.0],
                max: [500.0, 0.0, 500.0],
            },
        );

        let bone = world.spawn();
        let spec = RagdollSpec {
            bodies: vec![RagdollBodySpec {
                entity: bone,
                translation: Vec3::new(0.0, start_y, 0.0),
                rotation: Quat::IDENTITY,
                scale: 1.0,
                shape: CollisionShape::Ball { radius: 5.0 },
                mass: 4.0,
                linear_damping: 0.05,
                angular_damping: 0.05,
                friction: 0.5,
                restitution: 0.0,
            }],
            constraints: Vec::new(),
        };
        let ragdoll = {
            let mut pw = world.resource_mut::<PhysicsWorld>();
            build_ragdoll(&mut pw, &spec, &ContactConfig::DEFAULT)
        };
        let actor = world.spawn();
        world.insert(actor, ragdoll);
        (world, bone)
    }

    #[test]
    fn ragdoll_bone_with_no_rapier_handles_row_still_gets_water_contact() {
        let (world, bone) = submerged_ragdoll_world(-40.0);

        // Precondition: the bone really is invisible to the first pass.
        assert!(
            world.get::<RapierHandles>(bone).is_none(),
            "fixture must mirror activate_ragdoll's #1772 teardown",
        );
        assert!(world.get::<RigidBodyData>(bone).is_none());

        apply_buoyancy(&world, true);

        let contact = world
            .get::<WaterContact>(bone)
            .map(|contact| *contact)
            .expect(
                "a submerged ragdoll bone must emit WaterContact — without \
                 the Ragdoll pass it is structurally invisible to the scan, \
                 so splash/ripple, underwater audio and the FX transition \
                 edge all see nothing for a corpse in water (#3492)",
            );
        assert!(
            contact.submerged_fraction > 0.0,
            "bone seeded 40 BU under the surface must read as submerged, \
             got {contact:?}",
        );
    }

    /// The authored damping the pass restores on exit is the **effective**
    /// figure `build_ragdoll` used, not the raw spec value — otherwise
    /// leaving the water would strip `ragdoll_extra_angular_damping`.
    #[test]
    fn recorded_damping_is_the_effective_value_build_ragdoll_applied() {
        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        let bone = world.spawn();
        let spec = RagdollSpec {
            bodies: vec![RagdollBodySpec {
                entity: bone,
                translation: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                scale: 1.0,
                shape: CollisionShape::Ball { radius: 5.0 },
                mass: 4.0,
                linear_damping: 0.05,
                angular_damping: 0.05,
                friction: 0.5,
                restitution: 0.0,
            }],
            constraints: Vec::new(),
        };
        let mut cfg = ContactConfig::DEFAULT;
        cfg.ragdoll_extra_angular_damping = 0.25;

        let ragdoll = {
            let mut pw = world.resource_mut::<PhysicsWorld>();
            build_ragdoll(&mut pw, &spec, &cfg)
        };
        let recorded = ragdoll.buoyancy.first().expect("one body recorded");
        assert_eq!(recorded.linear_damping, 0.05);
        assert!(
            (recorded.angular_damping - 0.30).abs() < 1e-6,
            "expected authored 0.05 + extra 0.25, got {}",
            recorded.angular_damping,
        );

        let pw = world.resource::<PhysicsWorld>();
        let body = pw.bodies.get(ragdoll.bodies[0].1).expect("body");
        assert!(
            (body.angular_damping() - recorded.angular_damping).abs() < 1e-6,
            "the recorded value must be what the live body runs with",
        );
    }
}

/// #3492 SIBLING — `clear_stale_water_contacts` selects on the same two
/// components the buoyancy scan does, so giving the scan a `Ragdoll` arm
/// without giving the cleanup one would leave a corpse holding submerged
/// damping and a latched `WaterContact` forever once its water plane
/// streamed out.
#[cfg(test)]
mod ragdoll_stale_contact_tests {
    use super::ragdoll_buoyancy_tests::*;
    use super::*;
    use byroredux_core::ecs::components::water::{WaterContact, WaterPlane, WaterVolume};

    #[test]
    fn ragdoll_contact_clears_when_its_water_plane_streams_out() {
        let (world, bone) = submerged_ragdoll_world(-40.0);
        apply_buoyancy(&world, true);
        assert!(
            world
                .get::<WaterContact>(bone)
                .is_some_and(|contact| contact.submerged_fraction > 0.0),
            "precondition: the bone is wet",
        );

        // Stream the only water surface out, exactly as an unload would.
        let water = world
            .query::<WaterPlane>()
            .expect("water storage")
            .iter()
            .map(|(entity, _)| entity)
            .next()
            .expect("one water entity");
        if let Some(mut q) = world.query_mut::<WaterPlane>() {
            q.remove(water);
        }
        if let Some(mut q) = world.query_mut::<WaterVolume>() {
            q.remove(water);
        }

        apply_buoyancy(&world, true);

        let contact = world
            .get::<WaterContact>(bone)
            .map(|contact| *contact)
            .expect("contact component still present");
        assert_eq!(
            contact.submerged_fraction, 0.0,
            "a ragdoll bone whose water plane streamed out must have its \
             contact cleared and its authored damping restored — the \
             cleanup pass keys off RapierHandles/RigidBodyData, which a \
             ragdoll bone does not have (#3492)",
        );
    }
}
