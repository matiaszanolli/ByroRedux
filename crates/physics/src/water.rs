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
use rustc_hash::FxHashMap;
use byroredux_core::ecs::components::groundcover::WindField;
use byroredux_core::ecs::components::water::{
    WaterBedField, WaterContact, WaterCurrentVolume, WaterFlow, WaterKind, WaterMaterial,
    WaterPlane, WaterSurfaceMesh, WaterVolume, WATERLINE_HYSTERESIS, WATER_SHORE_REFRACT_DEPTH,
    WATER_SHORE_MIN_UPHILL_LEN, WATER_SHORE_REFRACT_MAX, WATER_SHORE_RIPPLE_TURN_SHARE,
    WATER_SWELL_BREAK_RATIO, WATER_SWELL_SHORE_TOWARD_RAMP, WATER_WAVE_AMPLITUDE_MAX, WATER_WAVE_DEFAULT_SCROLL_A,
    WATER_WAVE_DEFAULT_SCROLL_B, WATER_WAVE_FALLBACK_DIR_A, WATER_WAVE_FALLBACK_DIR_B,
    WATER_WAVE_MIX_A, WATER_WAVE_MIX_B, WATER_WAVE_PHASE_B_RATE, WATER_WAVE_RATE_MAX,
    WATER_WAVE_RATE_MIN, WATER_WAVE_SPATIAL_FLOOR_A, WATER_WAVE_SPATIAL_FLOOR_B,
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
    /// The depth (`surface_y - center_y`) at which each currently-sleeping wet
    /// body settled, latched the first scan that sees it asleep (#5561). The
    /// wave re-wake compares against THIS, not against last scan's
    /// `WaterContact::depth`. Like `in_current_prev` it is a latch, not
    /// staging: it survives scans, is dropped when the body wakes or leaves
    /// the water, and is swept of despawned bodies after each full scan. A
    /// load simply re-derives it on the first asleep scan (worst case: the
    /// body settles at its current depth).
    ///
    /// Each value is `(depth, scan stamp)`: the stamp is `scan_generation`
    /// as of the last scan that saw the body asleep and wet, so the
    /// despawn sweep is one O(latched) `retain` on the stamp instead of an
    /// O(latched × targets) search (#5709). `FxHashMap`, not SipHash: the
    /// keyspace is per wet body per frame (#2923).
    rest_depth: FxHashMap<EntityId, (f32, u32)>,
    /// Bumped once per scan; the stamp `rest_depth` entries are refreshed to.
    scan_generation: u32,
}

/// How far a sleeping float's depth may drift from its settled depth before
/// the waves wake it (#5561). BU.
const WAVE_WAKE_DEPTH_DELTA: f32 = 0.1;

impl Resource for WaterContactScratch {}

/// One awake wet body's buoyancy lift, kept so the physics step can re-derive
/// it at EVERY fixed tick (#5547).
///
/// `apply_buoyancy` runs once per render frame, before the step, and used to
/// add a lift derived from that frame's pose, held constant over every
/// substep (up to `MAX_SUBSTEPS`) × rapier's internal solver steps. A stiff
/// spring (lift ∝ submerged fraction; k = 1.67·m·|g|/h for an AABB height
/// `h`) held that long over-integrates and adds energy each frame, so the
/// float-vs-sink threshold scaled with the render frame rate (≈4.8 BU at 60
/// fps, ≈11 BU at 30 fps, ≈28 BU at 12 fps) and thin clutter never settled.
/// The scan records the non-lift forces once (they stay in rapier's
/// persistent `user_force`) and registers the lift here; each substep adds
/// only the CHANGE in lift, so the placed-current marker's force and every
/// other contributor are left untouched.
#[derive(Clone, Copy)]
pub(crate) struct WaterLift {
    pub(crate) body: rapier3d::prelude::RigidBodyHandle,
    /// The collider the submerged fraction is measured on (the scan's
    /// representative AABB).
    pub(crate) collider: rapier3d::prelude::ColliderHandle,
    pub(crate) surface_y: f32,
    pub(crate) mass: f32,
    pub(crate) gravity_y: f32,
    pub(crate) density_ratio: f32,
    /// Extra vertical (heave) damping, 1/s — see [`heave_hold_damping`].
    pub(crate) heave_damping: f32,
    /// Lift + heave force currently contributed to the body's user force.
    pub(crate) applied: Vec3,
}

/// Heave damping needed to keep the lift spring stable over the K-step hold
/// rapier's internal solver substeps impose, for a body of AABB height
/// `height` (#5547). Zero for bodies tall enough that the submerged damping
/// alone (`base`) already dissipates the hold's energy gain.
///
/// The hold adds a factor ≈ `ω²·h_s²·K(K−1)/2` per tick with
/// `ω² = ρ·|g|/h` — it scales as the spring stiffness `ρ·|g|` and as `1/h`.
/// [`HEAVE_HOLD_COEFFICIENT`] was measured at the default stiffness
/// ([`HEAVE_HOLD_REFERENCE_STIFFNESS`], ≈ 1147/h), so it is rescaled by
/// `density_ratio·|gravity_y|` over that reference (#5618): at the defaults
/// the result is unchanged, away from them the damping follows the spring.
///
/// The damping is applied as an explicit force refreshed once per
/// [`crate::world::PHYSICS_DT`] tick, which is stable only while `c·dt < 2`; the result is
/// capped at [`HEAVE_STABLE_FRACTION`]`/dt` so a very thin AABB (or a very
/// dense fluid) gets a bounded, stable damping instead of one that
/// multiplies the heave velocity by `|1 − c·dt|` ≫ 1 every tick. (The lift
/// spring itself loses stability below `h ≈ 1147/(2/dt)² ≈ 0.08` BU, which
/// no damping can fix.)
pub(crate) fn heave_hold_damping(
    height: f32,
    base: f32,
    density_ratio: f32,
    gravity_y: f32,
) -> f32 {
    let stiffness_scale = density_ratio * gravity_y.abs() / HEAVE_HOLD_REFERENCE_STIFFNESS;
    let wanted = HEAVE_HOLD_COEFFICIENT * stiffness_scale / height.max(1.0e-3) - base;
    wanted.clamp(0.0, HEAVE_STABLE_FRACTION / crate::world::PHYSICS_DT)
}

/// 1/s·BU at [`HEAVE_HOLD_REFERENCE_STIFFNESS`]: see [`heave_hold_damping`].
/// Measured, not derived — the `thin_floats_settle_and_sleep_at_any_frame_rate`
/// matrix pins it.
const HEAVE_HOLD_COEFFICIENT: f32 = 11.0;

/// ρ·|g| the coefficient was measured at: the default
/// [`PhysicsWaterConstants::buoyancy_density_ratio`] (1.67) times the default
/// world gravity (686.7 BU/s², 9.81 m/s² × 70 BU/m).
const HEAVE_HOLD_REFERENCE_STIFFNESS: f32 = 1.67 * 686.7;

/// Largest `c·dt` the explicit heave damping may reach: below the `c·dt = 1`
/// dead-beat point, well inside the `c·dt < 2` stability edge.
const HEAVE_STABLE_FRACTION: f32 = 0.9;

impl PhysicsWorld {
    /// Re-derive every registered lift from the body's CURRENT pose and
    /// velocity and add the change to its user force (#5547). Called at the
    /// top of each fixed substep. Sleeping bodies keep their last value.
    pub(crate) fn refresh_water_lift(&mut self) {
        for i in 0..self.water_lift.len() {
            let lift = self.water_lift[i];
            let Some(body) = self.bodies.get(lift.body) else {
                continue;
            };
            if body.is_sleeping() || !body.is_dynamic() {
                continue;
            }
            let vy = body.linvel().y;
            let Some(collider) = self.colliders.get(lift.collider) else {
                continue;
            };
            let aabb = collider.compute_aabb();
            let frac = submerged_fraction(aabb.mins.y, aabb.maxs.y, lift.surface_y);
            let mut now =
                buoyancy_force(lift.mass, frac, lift.gravity_y, lift.density_ratio);
            now.y -= lift.heave_damping * lift.mass * vy;
            let delta = now - lift.applied;
            self.add_force(lift.body, delta, false);
            self.water_lift[i].applied = now;
        }
    }
}

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
/// - `surface_y` — the water surface height over the body
///   ([`WaterVolume::surface_y_at`]).
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
    /// The plane's kind — the weather transport the CPU crest sees depends on
    /// it (`WaterKind::weather_scroll_for`, #5354).
    kind: WaterKind,
    volume: WaterVolume,
    /// Non-planar mesh-water surface; `None` means the planar `volume.max.y`.
    surface: Option<WaterSurfaceMesh>,
    material: WaterMaterial,
    flow: Option<WaterFlow>,
    damage_per_second: f32,
    /// The bed under a cell plane (the LAND samples its mesh vertices were
    /// baked from); `None` for water with no LAND under it.
    bed: Option<WaterBedField>,
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
///
/// `weather_scroll` (already kind-damped through
/// [`WaterKind::weather_scroll_for`]) and `wind_wave_scale` come from
/// [`weather_wave_adjustment`], the same pair the renderer uploads, so
/// gameplay, camera effects, and buoyancy all agree with the rendered crest.
/// The scroll prep is [`WaterMaterial::wave_scrolls`] — the function the
/// renderer's upload calls — and every tuning value is a `WATER_WAVE_*`
/// constant `water.vert` reads from the generated header (#5582), so no
/// number here is a second copy of the shader's.
pub fn authored_wave_height_with_weather(
    material: &WaterMaterial,
    position: Vec3,
    time_secs: f32,
    weather_scroll: [f32; 2],
    wind_wave_scale: f32,
) -> f32 {
    authored_wave_height_over_bed(
        material,
        position,
        time_secs,
        weather_scroll,
        wind_wave_scale,
        None,
    )
}

/// `dir` mixed toward `shore_dir` by `share` and renormalised; `dir` itself if
/// the mix collapses. Mirrors `normalize(mix(dir, shoreDir, share))` in
/// `water.vert`.
fn turn_toward(dir: [f32; 2], shore_dir: [f32; 2], share: f32) -> [f32; 2] {
    let mixed = [
        dir[0] + (shore_dir[0] - dir[0]) * share,
        dir[1] + (shore_dir[1] - dir[1]) * share,
    ];
    let len = (mixed[0] * mixed[0] + mixed[1] * mixed[1]).sqrt();
    if len > 1.0e-6 {
        [mixed[0] / len, mixed[1] / len]
    } else {
        dir
    }
}

/// The swell's heading after shore refraction. Only a swell travelling toward
/// the shore turns toward it, fading in from perpendicular to 60° off
/// (`smoothstep(0, 0.5, dot)` in `water.vert`): one running offshore would mix
/// to a near-zero vector at half share, and normalising that flips the
/// heading from one vertex to the next.
fn refract_swell(heading: [f32; 2], shore_dir: [f32; 2], refraction: f32) -> [f32; 2] {
    let cosine = heading[0] * shore_dir[0] + heading[1] * shore_dir[1];
    let t = (cosine / WATER_SWELL_SHORE_TOWARD_RAMP).clamp(0.0, 1.0);
    turn_toward(heading, shore_dir, refraction * t * t * (3.0 - 2.0 * t))
}

/// [`authored_wave_height_with_weather`] over a known bed: the wind-driven swell
/// (`WaterMaterial::swell`) shoals, turns toward the shore and breaks exactly
/// as `water.vert` does, and the authored ripple pair is capped by the same
/// breaking limit and refracted the same way. `bed` is the plane's
/// [`WaterBedField`]; `None`, or a position outside it, is open water. The
/// bed field and the mesh vertices come from the same LAND samples, so the
/// visible crest and this one cannot disagree about the depth.
pub fn authored_wave_height_over_bed(
    material: &WaterMaterial,
    position: Vec3,
    time_secs: f32,
    weather_scroll: [f32; 2],
    wind_wave_scale: f32,
    bed: Option<&WaterBedField>,
) -> f32 {
    if !time_secs.is_finite() {
        return 0.0;
    }
    let sample = bed.and_then(|bed| bed.sample_baked(position.x, position.z));
    let bed_depth = sample.map_or(f32::INFINITY, |sample| sample.depth);
    // Refraction toward the shore: only where the bed rises, and only in the
    // zone `water.vert` bends in (`WATER_SHORE_REFRACT_DEPTH`).
    // The baked direction's length is its border fade (#6420): gate on it, then use the unit
    // heading, as `water.vert` does with the interpolated vertex colour.
    let uphill = sample.map_or([0.0, 0.0], |sample| sample.uphill);
    let shore_len = (uphill[0] * uphill[0] + uphill[1] * uphill[1]).sqrt();
    let shore_dir = if shore_len > WATER_SHORE_MIN_UPHILL_LEN {
        [uphill[0] / shore_len, uphill[1] / shore_len]
    } else {
        [0.0, 0.0]
    };
    let refraction = if sample.is_some() && shore_len > WATER_SHORE_MIN_UPHILL_LEN {
        let t = ((bed_depth - 0.25 * WATER_SHORE_REFRACT_DEPTH)
            / (0.75 * WATER_SHORE_REFRACT_DEPTH))
            .clamp(0.0, 1.0);
        (1.0 - t * t * (3.0 - 2.0 * t)) * WATER_SHORE_REFRACT_MAX
    } else {
        0.0
    };
    let turn = |dir: [f32; 2], share: f32| turn_toward(dir, shore_dir, share);

    // The swell is independent of the authored ripple amplitude: a plane with
    // none still rolls in the wind.
    let swell = material
        .swell(weather_scroll)
        .map(|mut swell| {
            if refraction > 0.0 {
                swell.direction = refract_swell(swell.direction, shore_dir, refraction);
            }
            swell.height(position.x, position.z, time_secs, bed_depth)
        })
        .unwrap_or(0.0);

    // The shader clamps the uploaded `tune.w` (amplitude × wind scale) and
    // takes `abs` of the uploaded frequency; mirror both exactly. #5582 —
    // the CPU used to clamp before the wind scale and floor a negative
    // frequency at 0, two crests the visible water never showed. The breaking
    // limit then caps it by the water under it.
    let amplitude = (material.wave_amplitude * wind_wave_scale)
        .clamp(0.0, WATER_WAVE_AMPLITUDE_MAX)
        .min(WATER_SWELL_BREAK_RATIO * bed_depth);
    let frequency = material.wave_frequency.abs();
    if !amplitude.is_finite() || !frequency.is_finite() {
        return swell;
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
    let scrolls = material.wave_scrolls(weather_scroll, time_secs);
    let (scroll_a, scroll_b) = (scrolls.a, scrolls.b);
    let spatial_a = material.uv_scale_a.abs().max(WATER_WAVE_SPATIAL_FLOOR_A);
    let spatial_b = material.uv_scale_b.abs().max(WATER_WAVE_SPATIAL_FLOOR_B);
    // The renderer normalizes these same post-weather vectors in
    // `water.vert`. Deriving the CPU directions before adding weather would
    // make buoyancy/submersion sample a different crest orientation whenever
    // atmospheric wind is active.
    let mut dir_a = direction(scroll_a, WATER_WAVE_FALLBACK_DIR_A);
    let mut dir_b = direction(scroll_b, WATER_WAVE_FALLBACK_DIR_B);
    if refraction > 0.0 {
        dir_a = turn(dir_a, refraction * WATER_SHORE_RIPPLE_TURN_SHARE);
        dir_b = turn(dir_b, refraction * WATER_SHORE_RIPPLE_TURN_SHARE);
    }
    let rate = |scroll: [f32; 2], reference: f32| {
        ((scroll[0] * scroll[0] + scroll[1] * scroll[1]).sqrt() / reference)
            .clamp(WATER_WAVE_RATE_MIN, WATER_WAVE_RATE_MAX)
    };
    let rate_a = rate(scroll_a, WATER_WAVE_DEFAULT_SCROLL_A);
    let rate_b = rate(scroll_b, WATER_WAVE_DEFAULT_SCROLL_B);
    // #4728 — wave A travels along +dir_a (the −t sign), in lockstep with
    // `water.vert`'s phaseA under the same convention flip.
    let phase_a =
        (position.x * dir_a[0] + position.z * dir_a[1]) * spatial_a * std::f32::consts::TAU
            - time_secs * frequency * rate_a * std::f32::consts::TAU;
    let phase_b =
        (position.x * dir_b[0] + position.z * dir_b[1]) * spatial_b * std::f32::consts::TAU
            - time_secs * frequency * rate_b * (std::f32::consts::TAU * WATER_WAVE_PHASE_B_RATE);
    swell + amplitude * (phase_a.sin() * WATER_WAVE_MIX_A + phase_b.sin() * WATER_WAVE_MIX_B)
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
    let surface_q = world.query::<WaterSurfaceMesh>();
    let bed_q = world.query::<WaterBedField>();
    for (entity, plane) in wq.iter() {
        let Some(volume) = vq.get(entity) else {
            continue;
        };
        let flow = flow_q.as_ref().and_then(|fq| fq.get(entity).copied());
        out.push(WaterSurface {
            entity,
            kind: plane.kind,
            volume: *volume,
            surface: surface_q.as_ref().and_then(|q| q.get(entity).cloned()),
            material: plane.material,
            flow,
            damage_per_second: plane.damage_per_second,
            bed: bed_q.as_ref().and_then(|q| q.get(entity).cloned()),
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

/// Clear the user force of every body that was inside a current marker last
/// scan, for the no-marker early return, which never reaches the per-body
/// reset (#5933). Ragdoll links have no `RapierHandles` row; the dry-link
/// damping pass resets them.
fn release_current_forces(world: &World, in_current_prev: &[EntityId]) {
    if in_current_prev.is_empty() {
        return;
    }
    let bodies: Vec<_> = {
        let Some(handles_q) = world.query::<RapierHandles>() else {
            return;
        };
        in_current_prev
            .iter()
            .filter_map(|&entity| handles_q.get(entity).map(|handles| handles.body))
            .collect()
    };
    let mut pw = world.resource_mut::<PhysicsWorld>();
    for body in bodies {
        pw.reset_forces(body, false);
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
        rest_depth,
        scan_generation,
    } = scratch;
    *scan_generation = scan_generation.wrapping_add(1);
    let generation = *scan_generation;
    surfaces.clear();
    current_volumes.clear();
    targets.clear();
    writes.clear();
    in_current_now.clear();
    if world.try_resource::<PhysicsWorld>().is_none() {
        in_current_prev.clear();
        return;
    }
    // #5547 — last frame's per-tick lift registrations are stale by
    // definition (water may have streamed out, the scan may early-return).
    // #5698 — likewise the dry-link damping pass's skip set
    // (`water_force_targets`): it is cleared HERE, before the early returns
    // below, not only once a full scan reaches the body loop. After a plane
    // streams out (or the scene quiesces) the scan returns early, and a set
    // left over from the last full scan would keep ex-wet links "owned by
    // the water pass" — losing their dry damping until another full scan.
    {
        let mut pw = world.resource_mut::<PhysicsWorld>();
        pw.water_lift.clear();
        pw.water_force_targets.clear();
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
            release_current_forces(world, in_current_prev);
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
        if pw.active_island_counts().0 == 0 && !pw.pending_wake() && !had_newcomers && !waves_require_rescan
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
        // #5540/#5698 — `water_force_targets` was cleared at the top of the
        // scan (before the early returns); this run refills it below.
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
                body.translation()
            };

            // #3490 / #3973 (PHYS-D6-2026-08-27b-01, extended) — the
            // collider AABB is fetched ONCE here and its centre used on
            // ALL THREE axes for the union prefilter, the current-volume
            // containment, and the surface containment alike, so every
            // containment decision in this loop agrees on which point
            // represents "the body". #3490 fixed this for Y alone (the
            // current-volume and surface branches used to disagree on Y
            // — origin vs AABB centre); #3973 closes the X/Z half left
            // open by that fix, where the union prefilter and both
            // containment predicates still read `pos.x`/`pos.z` (the
            // rigid-body ORIGIN) while Y already read the AABB centre —
            // one body, two reference points on two different axes. They
            // coincide only for a shape centred on its body, which is
            // exactly what the bhk import path is not:
            // `collision_shape_to_parts` attaches every compound part at
            // its own local isometry, and ragdoll bones are offset by
            // construction — the norm, not an edge case. Missing
            // collider `continue`s the whole iteration.
            //
            // This computes the AABB before any XZ reject, trading the
            // old "cheap origin-only reject first" prefilter's saved
            // `compute_aabb()` calls for correctness on the boundary
            // case that trade-off was hiding: a body outside the union
            // footprint by its ORIGIN could have a collider that still
            // overlaps it. `targets` is every dynamic body in the world,
            // not pre-filtered by proximity to water, so this does cost
            // one `compute_aabb()` per dynamic body per tick regardless
            // of distance from water — bounded by body count, not by
            // volume count, and `compute_aabb()` is a local shape-extent
            // computation, not a broad-phase query.
            let Some(collider) = pw.colliders.get(t.handles.collider) else {
                continue;
            };
            let aabb = collider.compute_aabb();
            let reference_point = aabb.center();
            let (min_y, max_y) = (aabb.mins.y, aabb.maxs.y);
            let center_y = 0.5 * (min_y + max_y);

            let in_union_footprint = reference_point.x >= ux0
                && reference_point.x <= ux1
                && reference_point.z >= uz0
                && reference_point.z <= uz1;

            let current_flow = in_union_footprint
                .then(|| {
                    current_volumes
                        .iter()
                        .find(|current| {
                            let v = &current.volume;
                            reference_point.x >= v.min[0]
                                && reference_point.x <= v.max[0]
                                && center_y >= v.min[1]
                                && center_y <= v.max[1]
                                && reference_point.z >= v.min[2]
                                && reference_point.z <= v.max[2]
                        })
                        .map(|current| current.flow)
                })
                .flatten();

            // Find the containing surface: collider AABB centre inside the
            // volume's XZ extent (matching the union prefilter and the
            // current-volume test above — see the AABB-centre doc at the
            // top of this block), and the body within the column
            // (band-extended above the surface like the camera path so the
            // waterline is sticky). Bodies outside the union footprint
            // resolve to `None` without ever touching the collider set.
            // `(s, min_y, max_y)` borrows only the local `surfaces` Vec —
            // not `pw` — so the `get_mut` below is free.
            let surface = in_union_footprint
                .then(|| {
                    surfaces
                        .iter()
                        .filter_map(|s| {
                            if max_y < s.volume.min[1] {
                                return None;
                            }
                            let static_surface_y = s.volume.surface_y_at(
                                s.surface.as_ref(),
                                reference_point.x,
                                reference_point.z,
                                center_y,
                            )?;
                            let surface_y = time_secs
                                .map(|time| {
                                    // #5354 — the same per-kind weather the
                                    // renderer and camera sample; and the
                                    // horizontal sample point is the one
                                    // `surface_y_at` / containment use
                                    // (the collider AABB centre), not the
                                    // body origin.
                                    authored_wave_height_over_bed(
                                        &s.material,
                                        Vec3::new(reference_point.x, pos.y, reference_point.z),
                                        time,
                                        s.kind.weather_scroll_for(weather_scroll),
                                        wind_wave_scale,
                                        s.bed.as_ref(),
                                    )
                                })
                                .unwrap_or(0.0)
                                + static_surface_y;
                            (min_y <= surface_y + WATERLINE_HYSTERESIS).then_some((s, surface_y))
                        })
                        .min_by(|a, b| {
                            nearest_surface_distance(a.1, center_y)
                                .total_cmp(&nearest_surface_distance(b.1, center_y))
                        })
                        .map(|(s, surface_y)| (s, min_y, max_y, surface_y))
                })
                .flatten();

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

            // #5933 — a body that was inside a current marker last scan holds
            // that marker's last `add_force` until something resets it. A
            // marker that overlaps no plane never makes the body wet, so on
            // the frame it leaves the marker neither of the other terms
            // holds and the stale term would push it forever.
            if current_flow.is_some()
                || surface.is_some()
                || t.prior_wet
                || in_current_prev.binary_search(&t.entity).is_ok()
            {
                pw.reset_forces(t.handles.body, false);
            }

            match surface {
                Some((s, min_y, max_y, surface_y)) => {
                    // Same AABB centre the surface search sorted by (#2887).
                    let center_y = 0.5 * (min_y + max_y);
                    // #5540 — multibody links (every jointed ragdoll bone)
                    // never receive `RigidBody` damping: rapier's writeback
                    // path skips them, so `set_*_damping` there was stored
                    // and never applied, and a floating corpse's heave ran
                    // undamped until it left the volume (measured y range
                    // -1438..+136 over 40 s). For links the drag rides this
                    // pass's user FORCE instead — `-c·m·v` alongside the
                    // buoyancy force, plus `-c·(I·ω)` as a torque — with the
                    // authored coefficients folded in (the stored fields
                    // hold authored + ragdoll lever, which were equally
                    // inert on links).
                    let is_link = pw.is_multibody_link(t.handles.body);
                    // #5540/#5544 — this pass owns the body's user force
                    // while wet: the dry-link damping pass must not reset
                    // it, and the spawn re-sleep must not undo a
                    // legitimate dry→wet entry wake before it integrates.
                    pw.note_water_force_target(t.handles.body);
                    // Carries the derived force out of the `bodies.get_mut`
                    // borrow so it can be applied through `PhysicsWorld`'s
                    // own API rather than the raw Rapier body (#2889).
                    let mut pending_force: Option<Vec3> = None;
                    let mut pending_torque: Option<Vec3> = None;
                    let mut pending_lift: Option<WaterLift> = None;
                    let frac = submerged_fraction(min_y, max_y, surface_y);
                    // The current as it runs at this body: a river is faster
                    // over the deep channel and runs along the banks
                    // (`WaterFlow::over_bed`), exactly as `water.frag` shows
                    // it; every other kind, and water with no bed, keeps the
                    // plane's flow.
                    let local_flow = s.flow.map(|flow| {
                        match (s.kind, s.bed.as_ref()) {
                            (WaterKind::River | WaterKind::Rapids, Some(bed)) => bed
                                .sample_baked(reference_point.x, reference_point.z)
                                .map_or(flow, |sample| flow.over_bed(sample)),
                            _ => flow,
                        }
                    });
                    if frac > 0.0 {
                        if let Some(b) = pw.bodies.get_mut(t.handles.body) {
                            // Submerged viscous damping so the float settles
                            // (and can then sleep) rather than bobbing
                            // forever. Inert on multibody links — see above.
                            if !is_link {
                                b.set_linear_damping(consts.linear_damping_in);
                                b.set_angular_damping(consts.angular_damping_in);
                            }
                            // Wake ONCE on entry so a body that streamed in
                            // already submerged (spawned asleep) floats up.
                            // #5561 — a SLEEPING float re-wakes for waves when
                            // its depth drifts from the depth it settled at,
                            // not from last scan's depth: `WaterContact::depth`
                            // is rewritten every scan, so a per-scan delta is
                            // (wave rate × scan dt) and falls under the
                            // threshold at high frame rates — the float then
                            // sat rigid while the rendered surface moved.
                            let depth_now = surface_y - center_y;
                            let wake = if !t.prior_wet {
                                rest_depth.remove(&t.entity);
                                true
                            } else if b.is_sleeping() {
                                let latch = rest_depth
                                    .entry(t.entity)
                                    .or_insert((depth_now, generation));
                                latch.1 = generation;
                                let rest = latch.0;
                                let drifted = (depth_now - rest).abs() > WAVE_WAKE_DEPTH_DELTA;
                                if drifted {
                                    rest_depth.remove(&t.entity);
                                }
                                drifted
                            } else {
                                rest_depth.remove(&t.entity);
                                false
                            };
                            if wake {
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
                                // #5547 — the lift (and the heave damping
                                // that stabilises its hold) is re-derived at
                                // every fixed tick by `refresh_water_lift`;
                                // this scan's value is the first tick's.
                                let heave_damping = heave_hold_damping(
                                    max_y - min_y,
                                    consts.linear_damping_in,
                                    consts.buoyancy_density_ratio,
                                    gravity_y,
                                );
                                f.y -= heave_damping * mass * b.linvel().y;
                                pending_lift = Some(WaterLift {
                                    body: t.handles.body,
                                    collider: t.handles.collider,
                                    surface_y,
                                    mass,
                                    gravity_y,
                                    density_ratio: consts.buoyancy_density_ratio,
                                    heave_damping,
                                    applied: f,
                                });
                                if is_link {
                                    // Drag as a force: water coefficient +
                                    // the (stored-but-inert) authored one.
                                    let c =
                                        consts.linear_damping_in + b.linear_damping().max(0.0);
                                    let v = b.linvel();
                                    f.x -= c * mass * v.x;
                                    f.y -= c * mass * v.y;
                                    f.z -= c * mass * v.z;
                                    let c_ang = consts.angular_damping_in
                                        + b.angular_damping().max(0.0);
                                    // -c·I·ω (#5697), the same torque the
                                    // dry pass applies.
                                    pending_torque = Some(PhysicsWorld::angular_damping_torque(b, c_ang));
                                }
                                if let Some(flow) = local_flow {
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
                        if let Some(lift) = pending_lift.take() {
                            pw.water_lift.push(lift);
                        }
                        if let Some(torque) = pending_torque.take() {
                            pw.add_torque(t.handles.body, torque, false);
                        }
                        writes.push((
                            t.entity,
                            WaterContact {
                                surface_entity: Some(s.entity),
                                depth: surface_y - center_y,
                                submerged_fraction: frac,
                                head_submerged: max_y <= surface_y,
                                flow: local_flow,
                                damage_per_second: s.damage_per_second,
                                // #5562 — the waterline under the collider AABB
                                // centre, the reference point every decision in
                                // this loop used, at the surface (waves
                                // included) it sampled.
                                waterline: Some(Vec3::new(
                                    reference_point.x,
                                    surface_y,
                                    reference_point.z,
                                )),
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
                        rest_depth.remove(&t.entity);
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
                        rest_depth.remove(&t.entity);
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
                            // #5544 — a deliberate current wake is not the
                            // begin-touch strong-wake; keep the re-sleep
                            // off it.
                            pw.note_water_force_target(t.handles.body);
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
                // The surface arm applies authored damping to multibody
                // links because Rapier does not apply rigid-body damping to
                // articulations. A marker-only current owns the link's force
                // accumulator too, so preserve those authored coefficients
                // here when no surface arm did it this frame.
                if surface.is_none() && pw.is_multibody_link(t.handles.body) {
                    let damping = pw.bodies.get(t.handles.body).and_then(|b| {
                        if b.is_sleeping() {
                            return None;
                        }
                        let mass = b.mass();
                        let linear = b.linear_damping().max(0.0);
                        let angular = b.angular_damping().max(0.0);
                        if linear <= 0.0 && angular <= 0.0 {
                            return None;
                        }
                        let v = b.linvel();
                        Some((
                            Vec3::new(
                                -linear * mass * v.x,
                                -linear * mass * v.y,
                                -linear * mass * v.z,
                            ),
                            PhysicsWorld::angular_damping_torque(b, angular),
                        ))
                    });
                    if let Some((force, torque)) = damping {
                        pw.add_force(t.handles.body, force, false);
                        pw.add_torque(t.handles.body, torque, false);
                    }
                }
                if let Some(f) = current {
                    // Per-frame term, so no wake — see the surface branch.
                    pw.add_force(t.handles.body, f, false);
                    // #5698 — this pass owns the body's user force this
                    // frame, exactly like the surface arm: without the note
                    // the dry-link damping pass `reset_forces` the placed
                    // current away on an awake damped ragdoll link whose
                    // marker overlaps no water plane.
                    pw.note_water_force_target(t.handles.body);
                }
            }
        }

        // A body woken on a dry→wet transition is NOT yet in rapier's
        // active set (that island state reflects the prior step),
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
        if !rest_depth.is_empty() {
            // Despawned bodies leave no dry branch to clear them — and
            // neither refresh their stamp, which every live asleep-wet body
            // does each scan. Every other arm removes its own latch.
            rest_depth.retain(|_, (_, stamp)| *stamp == generation);
        }
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

    #[test]
    fn a_swell_running_offshore_keeps_its_heading_and_never_collapses() {
        let shore = [1.0, 0.0];
        // Offshore, perpendicular and slightly shoreward headings all stay
        // finite and unit-length at every share, including the 0.5 that would
        // zero a plain mix of opposite vectors.
        for heading in [[-1.0, 0.0], [0.0, 1.0], [0.3, 0.954]] {
            for share in [0.0, 0.25, 0.5, 0.7] {
                let out = refract_swell(heading, shore, share);
                let len = (out[0] * out[0] + out[1] * out[1]).sqrt();
                assert!((len - 1.0).abs() < 1.0e-4, "{heading:?} @ {share}: {out:?}");
            }
        }
        assert_eq!(refract_swell([-1.0, 0.0], shore, 0.5), [-1.0, 0.0]);
        // Perpendicular: no turn yet (continuous with the offshore side).
        let perpendicular = refract_swell([0.0, 1.0], shore, 0.7);
        assert!((perpendicular[0]).abs() < 1.0e-6 && (perpendicular[1] - 1.0).abs() < 1.0e-6);
        // Travelling at the shore: turned toward it, not away.
        let shoreward = refract_swell([0.6, 0.8], shore, 0.7);
        assert!(shoreward[0] > 0.6, "{shoreward:?}");
    }
    use super::*;
    use crate::convert::pose_from_trs;
    use crate::world::{PhysicsWorld, PHYSICS_DT};
    use byroredux_core::ecs::components::water::WaterCurrentVolume;
    use byroredux_core::math::{Quat, Vec3};
    use rapier3d::prelude::*;

    const G_Y: f32 = -686.7; // PhysicsWorld gravity, BU/s².

    /// #4728 — the CPU crest mirror must move wave A along +dir_a in
    /// lockstep with `water.vert`'s flipped phaseA (−t sign). Pre-fix the
    /// CPU used +t, so buoyancy sampled a crest orientation that ran
    /// opposite the rendered wave whenever weather scroll was live.
    #[test]
    fn cpu_wave_phase_a_travels_along_dir_a_in_lockstep_with_the_shader() {
        // Scan only the production half so this test's own literals cannot
        // satisfy the scan (the established meta-guard shape).
        let prod = byroredux_core::source_scan::production_text(include_str!("water.rs"));
        assert!(
            prod.contains("- time_secs * frequency * rate_a * std::f32::consts::TAU"),
            "phase_a must use the −t sign so its crest travels along +dir_a \
             (#4728, in lockstep with water.vert)"
        );
        assert!(
            !prod.contains("+ time_secs * frequency * rate_a"),
            "the +t phase_a form must not come back — it ran the CPU crest \
             against the rendered wave direction"
        );
    }

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
            kind: WaterKind::Calm,
            volume: WaterVolume {
                min: [-10.0, -10.0, -10.0],
                max: [10.0, 0.0, 10.0],
            },
            surface: None,
            material: WaterMaterial::default(),
            flow: None,
            damage_per_second: 0.0,
            bed: None,
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

    /// #5562 — a wet contact publishes the waterline under the collider AABB
    /// centre, not under the body origin. A compound part sits at its own
    /// local offset (the bhk import path, ragdoll bones), so origin and AABB
    /// centre differ by tens of BU; the buoyancy scan already used the AABB
    /// centre for every containment decision, and the splash/ripple adapter
    /// rebuilt its position from the origin instead.
    #[test]
    fn wet_contact_publishes_the_waterline_at_the_collider_aabb_centre() {
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

        let water = world.spawn();
        world.insert(
            water,
            WaterPlane {
                kind: byroredux_core::ecs::components::water::WaterKind::Calm,
                material: WaterMaterial::default(),
                damage_per_second: 0.0,
            },
        );
        world.insert(
            water,
            WaterVolume {
                min: [-100.0, -100.0, -100.0],
                max: [100.0, 3.0, 100.0],
            },
        );

        // One ball whose collider is offset (30, -2.5, 0) from the body origin
        // at (0, 2, 0): AABB centre (30, -0.5, 0), straddling the surface.
        let body = world.spawn();
        world.insert(
            body,
            CollisionShape::Compound {
                children: vec![(
                    Vec3::new(30.0, -2.5, 0.0),
                    Quat::IDENTITY,
                    Box::new(CollisionShape::Ball { radius: 1.0 }),
                )],
            },
        );
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
                surface_material: 0,
                part_surface_materials: Box::new([]),
                havok_layer: 0,
            },
        );
        let origin = Vec3::new(0.0, 2.0, 0.0);
        world.insert(body, GlobalTransform::new(origin, Quat::IDENTITY, 1.0));
        world.insert(body, Transform::from_translation(origin));

        physics_sync_system(&world, PHYSICS_DT);

        let contact = *world
            .get::<WaterContact>(body)
            .expect("the body straddling the surface has a contact");
        assert!(contact.submerged_fraction > 0.0, "fixture is wet: {contact:?}");
        let waterline = contact.waterline.expect("a wet contact publishes its waterline");
        assert!(
            (waterline.x - 30.0).abs() < 0.05 && waterline.z.abs() < 0.05,
            "XZ is the collider AABB centre (30, 0), not the origin (0, 0): {waterline:?}"
        );
        assert!(
            (waterline.y - 3.0).abs() < 0.05,
            "Y is the sampled surface height (the volume's surface, 3.0): {waterline:?}"
        );
        // The depth it rides with is relative to the same AABB centre.
        assert!(
            (contact.depth - (waterline.y - (-0.5))).abs() < 0.05,
            "depth = surface_y − AABB-centre y: {contact:?}"
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
                surface_material: 0,
                part_surface_materials: Box::new([]),
                havok_layer: 0,
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
            scroll_a: [WATER_WAVE_DEFAULT_SCROLL_A, 0.0],
            scroll_b: [0.0, WATER_WAVE_DEFAULT_SCROLL_B],
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
            scroll_a: [WATER_WAVE_DEFAULT_SCROLL_A, 0.0],
            scroll_b: [0.0, WATER_WAVE_DEFAULT_SCROLL_B],
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

    /// #5582 — the shader clamps the uploaded amplitude AFTER the wind
    /// scale (`clamp(tune.w, 0, MAX)` with `tune.w = amplitude × scale`)
    /// and reads `abs` of the frequency. The CPU crest used to clamp before
    /// scaling (an authored 30 BU wave under the 1.5× storm boost bobbed
    /// bodies on a 45 BU crest the visible water capped at 32) and to floor
    /// a negative frequency at 0 (a frozen crest under a moving one).
    #[test]
    fn authored_wave_height_mirrors_the_shader_amplitude_and_frequency_rules() {
        let material = WaterMaterial {
            wave_amplitude: 30.0,
            wave_frequency: 0.8,
            ..WaterMaterial::default()
        };
        let mut peak = 0.0f32;
        for step in 0..400 {
            let t = step as f32 * 0.05;
            let position = Vec3::new(step as f32 * 7.0, 0.0, step as f32 * -3.0);
            let height = authored_wave_height_with_weather(&material, position, t, [0.0; 2], 1.5);
            peak = peak.max(height.abs());
        }
        assert!(
            peak <= WATER_WAVE_AMPLITUDE_MAX + 1.0e-3,
            "wind-scaled crest {peak} exceeds the shader's clamp"
        );
        assert!(peak > 30.0, "fixture sanity: the storm boost really pushed past 30 ({peak})");

        let negative = WaterMaterial {
            wave_frequency: -0.8,
            ..material
        };
        let position = Vec3::new(13.0, 0.0, 29.0);
        for t in [0.0, 0.37, 1.9] {
            assert_eq!(
                authored_wave_height_with_weather(&negative, position, t, [0.0; 2], 1.0),
                authored_wave_height_with_weather(&material, position, t, [0.0; 2], 1.0),
                "a negative frequency must move like its magnitude, as abs() does on the GPU"
            );
        }
    }

    /// The swell reaches buoyancy through the same sampler the renderer's
    /// vertex shader mirrors: open water is exactly the core swell, a deep bed
    /// changes nothing, a shallow one shoals and breaks it, and at the
    /// waterline it carries no displacement.
    #[test]
    fn swell_reaches_the_crest_sampler_and_shoals_over_a_bed() {
        use byroredux_core::ecs::components::groundcover::MAX_WIND_SPEED;
        use byroredux_core::ecs::components::water::{
            WaterBedField, WATER_SWELL_AMPLITUDE_MAX, WATER_SWELL_BREAK_RATIO,
        };
        let material = WaterMaterial {
            wind_driven: true,
            wave_amplitude: 0.0, // no authored ripple: only the swell
            ..WaterMaterial::default()
        };
        let gale = [
            MAX_WIND_SPEED * byroredux_core::ecs::components::water::WEATHER_SCROLL_PER_BU_PER_S,
            0.0,
        ];
        let field = |depth: f32| {
            WaterBedField::new([-1000.0, -1000.0], 100.0, 21, vec![depth; 21 * 21]).unwrap()
        };
        let swell = material.swell(gale).unwrap();
        // Find a position where the swell is near a crest.
        let (position, time) = (0..200)
            .map(|i| (Vec3::new(i as f32 * 5.0, 0.0, 0.0), 1.3))
            .find(|(p, t)| swell.height(p.x, p.z, *t, 1.0e6) > 0.8 * WATER_SWELL_AMPLITUDE_MAX)
            .expect("the swell has a crest");

        let open = authored_wave_height_over_bed(&material, position, time, gale, 1.0, None);
        assert!((open - swell.height(position.x, position.z, time, 1.0e6)).abs() < 1e-4);
        assert!(open > 0.8 * WATER_SWELL_AMPLITUDE_MAX);

        let deep = field(5000.0);
        let over_deep = authored_wave_height_over_bed(&material, position, time, gale, 1.0, Some(&deep));
        assert!((over_deep - open).abs() < 1e-3, "a deep bed leaves open water alone");

        let shallow = field(30.0);
        let over_shallow =
            authored_wave_height_over_bed(&material, position, time, gale, 1.0, Some(&shallow));
        assert!(
            over_shallow.abs() <= WATER_SWELL_BREAK_RATIO * 30.0 + 1e-3,
            "the swell breaks over 30 BU of water: {over_shallow}"
        );
        let waterline = field(0.0);
        assert_eq!(
            authored_wave_height_over_bed(&material, position, time, gale, 1.0, Some(&waterline)),
            0.0,
            "no displacement where the surface meets the shore"
        );
        // Outside the field it is open water again.
        let far = Vec3::new(100_000.0, 0.0, 0.0);
        let far_open = authored_wave_height_over_bed(&material, far, time, gale, 1.0, None);
        let far_field = authored_wave_height_over_bed(&material, far, time, gale, 1.0, Some(&shallow));
        assert!((far_open - far_field).abs() < 1e-4);
        // A river has no swell however hard the wind blows.
        let river = WaterMaterial { wind_driven: false, wave_amplitude: 0.0, ..material };
        assert_eq!(authored_wave_height_over_bed(&river, position, time, gale, 1.0, None), 0.0);
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
            scroll_a: [WATER_WAVE_DEFAULT_SCROLL_A, 0.0],
            scroll_b: [0.0, WATER_WAVE_DEFAULT_SCROLL_B],
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
                .pose(pose_from_trs(Vec3::new(0.0, -50.0, 0.0), Quat::IDENTITY))
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
                surface_material: 0,
                part_surface_materials: Box::new([]),
                havok_layer: 0,
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

    /// Regression for #3490 (PHYS-D6-2026-08-27b-01) — the exact split
    /// #2887 (above) closed for the SURFACE branch, still present on the
    /// CURRENT-VOLUME branch 26 lines apart in the same loop. Reuses the
    /// identical offset-compound fixture: a 40 BU-below-origin leaf makes
    /// the AABB centre (-70) and the body origin (-30) disagree by a known
    /// amount, then authors a current-volume band that contains ONLY the
    /// AABB centre — a pre-fix read (`pos.y`, the origin) must miss the
    /// band entirely, while the fixed read (the AABB centre) must hit it.
    ///
    /// `WaterContact::flow` is NOT the right observable here — when the
    /// body is also inside a surface's water column (as this fixture's
    /// body is), `flow` is written from the SURFACE's own `WaterFlow`
    /// (`s.flow`, this loop's `writes.push` a few lines below), completely
    /// independent of `current_flow`. The current-volume branch's own
    /// effect is a bounded drag *force* applied directly to the Rapier
    /// body (`current_force`, further below), never surfaced on
    /// `WaterContact` at all — so this drives the current in a direction
    /// (+Z) the base water plane's own flow (+X) does not touch, and reads
    /// the resulting displacement instead.
    #[test]
    fn current_volume_flow_is_measured_from_the_collider_aabb_centre_not_the_body_origin() {
        use crate::physics_sync_system;
        use byroredux_core::ecs::components::collision::CollisionShape;
        use byroredux_core::ecs::components::water::WaterCurrentVolume;
        use byroredux_core::ecs::components::Transform;

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

        // AABB centre = origin + OFFSET = -70.0; body origin = -30.0. Band
        // contains the centre but not the origin — the two reads disagree
        // about whether this body is even inside the current at all.
        let aabb_centre_y = body_origin_y + OFFSET;
        assert!(
            (-80.0..=-60.0).contains(&aabb_centre_y),
            "fixture precondition: centre must fall inside the authored band"
        );
        assert!(
            !(-80.0..=-60.0).contains(&body_origin_y),
            "fixture precondition: origin must fall OUTSIDE the authored band, \
             or this test can't distinguish the two reads"
        );
        let marker = world.spawn();
        world.insert(
            marker,
            WaterCurrentVolume {
                volume: WaterVolume {
                    min: [-500.0, -80.0, -500.0],
                    max: [500.0, -60.0, 500.0],
                },
                // +Z — orthogonal to the base water plane's own +X flow
                // (`submerged_ball_world`), so any Z displacement can only
                // come from THIS current volume firing.
                flow: WaterFlow::new([0.0, 0.0, 1.0], 8.0),
            },
        );

        for _ in 0..600 {
            physics_sync_system(&world, PHYSICS_DT);
        }

        let position = world
            .get::<Transform>(body)
            .expect("transform present")
            .translation;
        assert!(
            position.z > 1.0,
            "the current volume must be detected via the collider AABB centre \
             (-70), which falls inside the authored band, and push the body \
             downstream along +Z; a body-origin read (-30) falls outside the \
             band and would miss the current entirely, leaving z≈0 — got \
             z={}",
            position.z
        );
    }

    /// Regression for #3973 (PHYS-D6-2026-09-06-01) — #3490 fixed the
    /// current-volume containment test's Y axis to read the collider AABB
    /// centre instead of the body origin, but left X and Z on the origin.
    /// Reuses the sibling Y-axis test's exact fixture shape, with the
    /// compound leaf's offset moved onto X instead of Y: the AABB centre
    /// (X=40) and the body origin (X=0) now disagree on X, and a current
    /// volume authored to contain only the offset centre's X band (not the
    /// origin's) must fire — a pre-fix (origin-only) read would miss it
    /// entirely.
    #[test]
    fn current_volume_flow_is_measured_from_the_collider_aabb_centre_on_x_not_the_body_origin() {
        use crate::physics_sync_system;
        use byroredux_core::ecs::components::collision::CollisionShape;
        use byroredux_core::ecs::components::water::WaterCurrentVolume;
        use byroredux_core::ecs::components::Transform;

        const OFFSET_X: f32 = 40.0;
        const RADIUS: f32 = 10.0;
        let body_origin_y = -30.0_f32;
        let (mut world, _water, body) = submerged_ball_world(body_origin_y);
        world.insert(
            body,
            CollisionShape::Compound {
                children: vec![(
                    Vec3::new(OFFSET_X, 0.0, 0.0),
                    Quat::IDENTITY,
                    Box::new(CollisionShape::Ball { radius: RADIUS }),
                )],
            },
        );

        // AABB centre X = origin X (0) + OFFSET_X = 40.0; body origin X = 0.
        // Band contains the centre but not the origin.
        let aabb_centre_x = 0.0 + OFFSET_X;
        assert!(
            (30.0..=50.0).contains(&aabb_centre_x),
            "fixture precondition: centre must fall inside the authored band"
        );
        assert!(
            !(30.0..=50.0).contains(&0.0_f32),
            "fixture precondition: origin must fall OUTSIDE the authored band, \
             or this test can't distinguish the two reads"
        );
        let marker = world.spawn();
        world.insert(
            marker,
            WaterCurrentVolume {
                volume: WaterVolume {
                    min: [30.0, -200.0, -500.0],
                    max: [50.0, 0.0, 500.0],
                },
                // +Z — orthogonal to the base water plane's own +X flow
                // (`submerged_ball_world`), so any Z displacement can only
                // come from THIS current volume firing.
                flow: WaterFlow::new([0.0, 0.0, 1.0], 8.0),
            },
        );

        for _ in 0..600 {
            physics_sync_system(&world, PHYSICS_DT);
        }

        let position = world
            .get::<Transform>(body)
            .expect("transform present")
            .translation;
        assert!(
            position.z > 1.0,
            "the current volume must be detected via the collider AABB centre's \
             X (40), which falls inside the authored band, and push the body \
             downstream along +Z; a body-origin read (X=0) falls outside the \
             band and would miss the current entirely, leaving z≈0 — got \
             z={}",
            position.z
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
    /// no awake dynamic in rapier's active set (the island state only updates inside
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

    /// #5354 — buoyancy must sample the crest the renderer and the camera
    /// do. `FLOWING_WATER_WEATHER_TRANSPORT` damps the weather scroll on
    /// flowing kinds, and `authored_wave_height_with_weather` derives crest
    /// direction and speed from the post-weather scroll, so a sampler that
    /// skipped the damping tracked a different crest. Drives the real
    /// `physics_sync_system` over a River plane with wind and compares the
    /// recorded contact depth with the kind-aware surface; the fixture's
    /// undamped surface is asserted to differ so the test cannot pass
    /// vacuously.
    #[test]
    fn buoyancy_samples_the_kind_damped_weather_crest_on_a_river() {
        use crate::physics_sync_system;
        use byroredux_core::ecs::components::groundcover::WindField;
        use byroredux_core::ecs::components::water::{WaterContact, WaterKind, WaterPlane};
        use byroredux_core::ecs::resources::TotalTime;

        let start_y = -6.0_f32;
        let (mut world, water, body) = submerged_ball_world(start_y);
        let material = byroredux_core::ecs::components::water::WaterMaterial {
            wave_amplitude: 6.0,
            wave_frequency: 1.0,
            scroll_a: [0.012, 0.004],
            scroll_b: [-0.006, 0.01],
            ..Default::default()
        };
        world.insert(
            water,
            WaterPlane {
                kind: WaterKind::River,
                material,
                damage_per_second: 0.0,
            },
        );
        world.insert_resource(WindField {
            direction: [1.0, 0.0],
            speed: 60.0,
            gust_amplitude: 0.0,
            gust_frequency: 0.0,
        });
        let t = 3.7_f32;
        world.insert_resource(TotalTime(t));

        physics_sync_system(&world, PHYSICS_DT);
        let contact = world.get::<WaterContact>(body).expect("body is wet");

        let (scroll, scale) = weather_wave_adjustment(&world, t);
        let at = Vec3::new(0.0, start_y, 0.0);
        let damped = authored_wave_height_with_weather(
            &material,
            at,
            t,
            WaterKind::River.weather_scroll_for(scroll),
            scale,
        );
        let undamped = authored_wave_height_with_weather(&material, at, t, scroll, scale);
        assert!(
            (damped - undamped).abs() > 0.25,
            "fixture: damped ({damped}) and raw ({undamped}) crests must differ"
        );
        let expected_depth = damped - start_y;
        assert!(
            (contact.depth - expected_depth).abs() < 1.0e-3,
            "buoyancy depth {} must follow the damped crest {} (raw crest would give {})",
            contact.depth,
            expected_depth,
            undamped - start_y,
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

    /// The current carries a floating body at the speed the channel runs there:
    /// the same river, with the bed deep under the body and then shallow
    /// (`WaterFlow::over_bed`), drifts it markedly further in the deep run, and
    /// a contact reports the current it was actually driven by.
    #[test]
    fn river_current_on_a_floating_body_follows_the_bed() {
        use crate::physics_sync_system;
        use byroredux_core::ecs::components::water::{
            WaterBedField, WaterContact, WaterKind, WaterMaterial, WaterPlane,
            WATER_RIVER_SPEED_MIN,
        };
        use byroredux_core::ecs::components::Transform;

        let run = |bed_depth: f32| {
            let (mut world, water, body) = submerged_ball_world(-5.0);
            world.insert(
                water,
                WaterPlane {
                    kind: WaterKind::River,
                    material: WaterMaterial::default(),
                    damage_per_second: 0.0,
                },
            );
            world.insert(
                water,
                WaterBedField::new([-500.0, -500.0], 100.0, 21, vec![bed_depth; 21 * 21]).unwrap(),
            );
            for _ in 0..600 {
                physics_sync_system(&world, PHYSICS_DT);
            }
            let x = world.get::<Transform>(body).unwrap().translation.x;
            let contact_speed = world.get::<WaterContact>(body).unwrap().flow.map(|f| f.speed);
            (x, contact_speed)
        };
        let (deep_x, deep_speed) = run(2000.0);
        let (shallow_x, shallow_speed) = run(0.0);
        assert!(deep_x > 5.0, "the deep river carries the body: x={deep_x}");
        assert!(
            shallow_x < deep_x * 0.75,
            "the shallow run must carry it clearly less: deep={deep_x} shallow={shallow_x}"
        );
        assert!((deep_speed.unwrap() - 8.0).abs() < 1e-3, "deep contact flow {deep_speed:?}");
        assert!(
            (shallow_speed.unwrap() - 8.0 * WATER_RIVER_SPEED_MIN).abs() < 1e-3,
            "shallow contact flow {shallow_speed:?}"
        );
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
                surface_material: 0,
                part_surface_materials: Box::new([]),
                havok_layer: 0,
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
            let (ad, _ak) = world.resource::<PhysicsWorld>().active_island_counts();
            if ad == 0 {
                break;
            }
        }
        let (ad, _ak) = world.resource::<PhysicsWorld>().active_island_counts();
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

    /// #5561 — a slept float must follow the waves at high frame rates too.
    /// The wake test compared against last scan's depth, which is rewritten
    /// every scan, so at 144 fps the per-frame change (wave rate × 1/144 s)
    /// stayed under 0.1 BU and the float sat rigid for good. Settle a float
    /// in calm time, then advance the clock at 144 fps: the accumulated drift
    /// from its settled depth must wake it.
    #[test]
    fn a_slept_float_wakes_for_waves_at_high_frame_rate() {
        use crate::{physics_sync_system, RapierHandles};
        use byroredux_core::ecs::components::collision::{
            CollisionShape, MotionType, RigidBodyData,
        };
        use byroredux_core::ecs::components::water::{
            WaterContact, WaterKind, WaterMaterial, WaterPlane, WaterVolume,
        };
        use byroredux_core::ecs::components::{GlobalTransform, Transform};
        use byroredux_core::ecs::resources::TotalTime;
        use byroredux_core::ecs::World;

        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        world.insert_resource(PhysicsWaterConstants {
            buoyancy_density_ratio: 1.02,
            ..PhysicsWaterConstants::default()
        });
        world.insert_resource(WaterContactScratch::default());
        world.insert_resource(TotalTime(0.0));
        world.register::<RapierHandles>();
        world.register::<WaterContact>();

        let material = WaterMaterial {
            wave_amplitude: 2.0,
            wave_frequency: 1.0,
            ..Default::default()
        };
        let water = world.spawn();
        world.insert(
            water,
            WaterPlane {
                kind: WaterKind::Calm,
                material,
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
        let body = world.spawn();
        world.insert(body, CollisionShape::Ball { radius: 15.0 });
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
                surface_material: 0,
                part_surface_materials: Box::new([]),
                havok_layer: 0,
            },
        );
        world.insert(
            body,
            GlobalTransform::new(Vec3::new(0.0, -5.0, 0.0), Quat::IDENTITY, 1.0),
        );
        world.insert(body, Transform::from_translation(Vec3::new(0.0, -5.0, 0.0)));

        // Settle with the clock stopped, until the island sleeps.
        for _ in 0..4000 {
            physics_sync_system(&world, PHYSICS_DT);
            if world.resource::<PhysicsWorld>().active_island_counts().0 == 0 {
                break;
            }
        }
        assert_eq!(
            world.resource::<PhysicsWorld>().active_island_counts().0,
            0,
            "fixture: the float must sleep before the waves start"
        );

        // 144 fps, clock advancing: per-frame crest change is far below 0.1.
        let frame_dt = 1.0 / 144.0;
        let mut woke = false;
        for frame in 1..=(144 * 4) {
            world.resource_mut::<TotalTime>().0 = frame as f32 * frame_dt;
            physics_sync_system(&world, frame_dt);
            if world.resource::<PhysicsWorld>().active_island_counts().0 > 0 {
                woke = true;
                break;
            }
        }
        assert!(
            woke,
            "a float that slept in calm water must wake once the waves have moved it \
             0.1 BU from its settled depth, at 144 fps as at 30"
        );
    }

    /// #5709 — the settled-depth latch of a float that despawns while asleep
    /// is swept by the scan stamp (no dry branch ever clears it), and a live
    /// sleeper's latch survives every scan.
    #[test]
    fn a_despawned_sleeper_leaves_no_rest_depth_latch() {
        use crate::{physics_sync_system, RapierHandles};
        use byroredux_core::ecs::components::collision::{
            CollisionShape, MotionType, RigidBodyData,
        };
        use byroredux_core::ecs::components::water::{
            WaterContact, WaterKind, WaterMaterial, WaterPlane, WaterVolume,
        };
        use byroredux_core::ecs::components::{GlobalTransform, Transform};
        use byroredux_core::ecs::resources::TotalTime;
        use byroredux_core::ecs::World;

        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        world.insert_resource(PhysicsWaterConstants {
            buoyancy_density_ratio: 1.02,
            ..PhysicsWaterConstants::default()
        });
        world.insert_resource(WaterContactScratch::default());
        world.insert_resource(TotalTime(0.0));
        world.register::<RapierHandles>();
        world.register::<WaterContact>();

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
        let body = world.spawn();
        world.insert(body, CollisionShape::Ball { radius: 15.0 });
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
                surface_material: 0,
                part_surface_materials: Box::new([]),
                havok_layer: 0,
            },
        );
        world.insert(
            body,
            GlobalTransform::new(Vec3::new(0.0, -5.0, 0.0), Quat::IDENTITY, 1.0),
        );
        world.insert(body, Transform::from_translation(Vec3::new(0.0, -5.0, 0.0)));

        for _ in 0..4000 {
            physics_sync_system(&world, PHYSICS_DT);
            if world.resource::<PhysicsWorld>().active_island_counts().0 == 0 {
                break;
            }
        }
        assert_eq!(
            world.resource::<PhysicsWorld>().active_island_counts().0,
            0,
            "fixture: the float must sleep"
        );
        // A few more asleep scans: the latch must persist (stamp refreshed).
        for _ in 0..5 {
            physics_sync_system(&world, PHYSICS_DT);
        }
        assert_eq!(
            world.resource::<WaterContactScratch>().rest_depth.len(),
            1,
            "a live sleeping float keeps its settled-depth latch across scans"
        );

        // The sweep runs on a full scan; a newcomer forces one (an all-asleep
        // scene takes the quiesced fast path and sweeps nothing).
        world.despawn(body);
        let newcomer = world.spawn();
        world.insert(newcomer, CollisionShape::Ball { radius: 5.0 });
        world.insert(
            newcomer,
            RigidBodyData {
                motion_type: MotionType::Dynamic,
                mass: 1.0,
                friction: 0.5,
                restitution: 0.0,
                linear_damping: 0.0,
                angular_damping: 0.0,
                collidable: true,
                surface_material: 0,
                part_surface_materials: Box::new([]),
                havok_layer: 0,
            },
        );
        world.insert(
            newcomer,
            GlobalTransform::new(Vec3::new(0.0, 400.0, 0.0), Quat::IDENTITY, 1.0),
        );
        world.insert(newcomer, Transform::from_translation(Vec3::new(0.0, 400.0, 0.0)));
        physics_sync_system(&world, PHYSICS_DT);
        assert!(
            world.resource::<WaterContactScratch>().rest_depth.is_empty(),
            "the despawned float's latch must be swept"
        );
    }

    /// Settle a thin float released slightly below equilibrium; returns
    /// (peak-to-peak y over the last 3 s, asleep at the end).
    fn thin_float_settle(height: f32, fps: f32) -> (f32, bool) {
        thin_float_settle_with(PhysicsWaterConstants::default(), height, fps)
    }

    /// [`thin_float_settle`] with explicit water tunables (#5618): the heave
    /// damping depends on the density ratio, so the matrix must be runnable
    /// away from the default.
    fn thin_float_settle_with(consts: PhysicsWaterConstants, height: f32, fps: f32) -> (f32, bool) {
        use crate::{physics_sync_system, RapierHandles};
        use byroredux_core::ecs::components::collision::{
            CollisionShape, MotionType, RigidBodyData,
        };
        use byroredux_core::ecs::components::water::{
            WaterContact, WaterKind, WaterMaterial, WaterPlane, WaterVolume,
        };
        use byroredux_core::ecs::components::{GlobalTransform, Transform};
        use byroredux_core::ecs::resources::TotalTime;
        use byroredux_core::ecs::World;

        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        world.insert_resource(consts);
        world.insert_resource(WaterContactScratch::default());
        world.insert_resource(TotalTime(0.0));
        world.register::<RapierHandles>();
        world.register::<WaterContact>();
        let water = world.spawn();
        world.insert(
            water,
            WaterPlane {
                kind: WaterKind::Calm,
                material: WaterMaterial {
                    wave_amplitude: 0.0,
                    ..WaterMaterial::default()
                },
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
        let start_y = -0.1 * height - 1.0;
        let body = world.spawn();
        world.insert(body, CollisionShape::Ball { radius: height * 0.5 });
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
                surface_material: 0,
                part_surface_materials: Box::new([]),
                havok_layer: 0,
            },
        );
        world.insert(
            body,
            GlobalTransform::new(Vec3::new(0.0, start_y, 0.0), Quat::IDENTITY, 1.0),
        );
        world.insert(body, Transform::from_translation(Vec3::new(0.0, start_y, 0.0)));
        let dt = 1.0 / fps;
        let frames = (30.0 * fps) as usize;
        let mut ys = Vec::new();
        for f in 0..frames {
            world.resource_mut::<TotalTime>().0 = f as f32 * dt;
            physics_sync_system(&world, dt);
            if f as f32 > (27.0 * fps) {
                ys.push(world.get::<Transform>(body).unwrap().translation.y);
            }
        }
        let (lo, hi) = ys
            .iter()
            .fold((f32::MAX, f32::MIN), |(l, h), y| (l.min(*y), h.max(*y)));
        let asleep = world.resource::<PhysicsWorld>().active_island_counts().0 == 0;
        (hi - lo, asleep)
    }

    /// #5547 — thin floats (plates, books, coins) must settle and sleep at
    /// any render frame rate. The lift was derived once per frame and held
    /// over every substep, so the float-vs-sink threshold scaled with the
    /// frame rate (≈4.8 BU at 60 fps, ≈11 BU at 30, ≈28 BU at 12) and thin
    /// clutter bobbed forever. With the lift re-derived each fixed tick the
    /// frame rate no longer matters for h ≥ 6 BU; below that the K-step hold
    /// inside rapier's solver needs `heave_hold_damping`
    /// (`HEAVE_HOLD_COEFFICIENT`, measured: 4 and 6 leave h = 2–4 BU
    /// limit-cycling at 60 fps, 8 and above settle every cell of this grid).
    /// Fails on the pre-fix scan (p2p 5.7 / 20 / 74 BU at h = 4).
    #[test]
    fn thin_floats_settle_and_sleep_at_any_frame_rate() {
        for height in [2.0_f32, 3.0, 4.0, 6.0, 10.0, 20.0] {
            for fps in [60.0_f32, 30.0, 12.0] {
                let (p2p, asleep) = thin_float_settle(height, fps);
                assert!(
                    asleep && p2p < 0.05,
                    "h={height} BU at {fps} fps: peak-to-peak {p2p} BU over the last 3 s, \
                     asleep={asleep} — a thin float must come to rest regardless of frame rate"
                );
            }
        }
    }

    /// #5618 — the coefficient was measured at the default density ratio and
    /// gravity; away from them the lift spring's stiffness (ρ·|g|/h) moves,
    /// and so must the damping that stabilises its hold.
    #[test]
    fn thin_floats_settle_at_a_denser_fluid_ratio() {
        let consts = PhysicsWaterConstants {
            buoyancy_density_ratio: 2.5,
            ..PhysicsWaterConstants::default()
        };
        for height in [2.0_f32, 4.0, 10.0] {
            let (p2p, asleep) = thin_float_settle_with(consts, height, 60.0);
            assert!(
                asleep && p2p < 0.05,
                "h={height} BU at density ratio 2.5: peak-to-peak {p2p} BU, asleep={asleep}"
            );
        }
    }

    /// #5547 — the heave damping is only what the hold needs: nothing for
    /// bodies the submerged damping already covers, growing as 1/h below.
    #[test]
    fn heave_hold_damping_is_zero_for_tall_bodies_and_grows_as_one_over_h() {
        const RATIO: f32 = 1.67; // PhysicsWaterConstants default
        assert_eq!(heave_hold_damping(20.0, 1.5, RATIO, G_Y), 0.0);
        assert!(heave_hold_damping(HEAVE_HOLD_COEFFICIENT / 1.5, 1.5, RATIO, G_Y).abs() < 1e-3);
        let thin = heave_hold_damping(2.0, 1.5, RATIO, G_Y);
        let thinner = heave_hold_damping(1.0, 1.5, RATIO, G_Y);
        assert!(
            (thin - (HEAVE_HOLD_COEFFICIENT / 2.0 - 1.5)).abs() < 1e-3,
            "at the measured stiffness the damping is the original 11/h - base"
        );
        assert!(thinner > thin);
        assert!(heave_hold_damping(0.0, 1.5, RATIO, G_Y).is_finite(), "degenerate AABB is finite");
    }

    /// #5618 — the damping follows the spring stiffness ρ·|g|, and never
    /// leaves the explicit-force stability region however thin the AABB.
    #[test]
    fn heave_hold_damping_scales_with_stiffness_and_stays_stable() {
        let base = 1.5;
        let at_default = heave_hold_damping(2.0, base, 1.67, G_Y);
        let denser = heave_hold_damping(2.0, base, 2.5, G_Y);
        // (11·ρ/1.67/h − base): linear in ρ before the base is subtracted.
        let want = HEAVE_HOLD_COEFFICIENT * (2.5 / 1.67) / 2.0 - base;
        assert!(denser > at_default && (denser - want).abs() < 1e-3, "{denser} vs {want}");
        let half_gravity = heave_hold_damping(2.0, base, 1.67, G_Y * 0.5);
        assert!(half_gravity < at_default, "weaker gravity, softer spring");
        for ratio in [1.02_f32, 1.67, 2.5, 8.0] {
            for height in [1.0e-3_f32, 0.01, 0.05, 0.1, 0.2, 1.0, 2.0, 50.0] {
                let c = heave_hold_damping(height, base, ratio, G_Y);
                assert!(
                    c * crate::world::PHYSICS_DT <= HEAVE_STABLE_FRACTION + 1e-6,
                    "h={height} ρ={ratio}: c·dt = {} is outside the stable region",
                    c * crate::world::PHYSICS_DT
                );
            }
        }
    }

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
                    surface_material: 0,
                    part_surface_materials: Box::new([]),
                    havok_layer: 0,
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
                    surface_material: 0,
                    part_surface_materials: Box::new([]),
                    havok_layer: 0,
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
                surface_material: 0,
                part_surface_materials: Box::new([]),
                havok_layer: 0,
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
                surface_material: 0,
                part_surface_materials: Box::new([]),
                havok_layer: 0,
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

    /// #5933 — a body leaving a marker-only current (no water plane, so it is
    /// never wet) must not keep the marker's last `add_force`: rapier user
    /// forces persist until `reset_forces`, and none of the old reset-gate terms
    /// held on the exit frame. Both exits: the marker moves away from the body
    /// (the full scan's reset gate), and the marker streams out entirely (the
    /// no-marker early return).
    #[test]
    fn leaving_a_marker_only_current_releases_its_force() {
        use crate::{physics_sync_system, RapierHandles};
        use byroredux_core::ecs::components::collision::{
            CollisionShape, MotionType, RigidBodyData,
        };
        use byroredux_core::ecs::components::{GlobalTransform, Transform};
        use byroredux_core::ecs::World;

        let marker_volume = |min: f32, max: f32| WaterCurrentVolume {
            volume: WaterVolume { min: [min; 3], max: [max; 3] },
            flow: WaterFlow::new([1.0, 0.0, 0.0], 8.0),
        };
        for despawn in [false, true] {
            let mut world = World::new();
            world.insert_resource(PhysicsWorld::new());
            world.insert_resource(PhysicsWaterConstants::default());
            // Carries the #3268 latch across scans, as the engine boot does.
            world.insert_resource(WaterContactScratch::default());
            world.register::<RapierHandles>();
            world.register::<WaterContact>();
            world.resource_mut::<PhysicsWorld>().gravity = Vector::ZERO;
            let marker = world.spawn();
            world.insert(marker, marker_volume(-500.0, 500.0));
            let body = world.spawn();
            world.insert(body, CollisionShape::Ball { radius: 10.0 });
            world.insert(
                body,
                RigidBodyData {
                    motion_type: MotionType::Dynamic,
                    mass: 20.0,
                    ..RigidBodyData::static_body()
                },
            );
            world.insert(body, GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0));
            world.insert(body, Transform::from_translation(Vec3::ZERO));
            physics_sync_system(&world, PHYSICS_DT);

            let user_force = |world: &World| -> f32 {
                let handles = *world.query::<RapierHandles>().unwrap().get(body).unwrap();
                let pw = world.resource::<PhysicsWorld>();
                let f = pw.bodies.get(handles.body).expect("body is registered").user_force();
                (f.x * f.x + f.y * f.y + f.z * f.z).sqrt()
            };
            let step_awake = |world: &World| {
                let handles = *world.query::<RapierHandles>().unwrap().get(body).unwrap();
                {
                    let mut pw = world.resource_mut::<PhysicsWorld>();
                    if let Some(b) = pw.bodies.get_mut(handles.body) {
                        b.wake_up(true);
                    }
                    pw.wake();
                }
                physics_sync_system(world, PHYSICS_DT);
            };
            for _ in 0..30 {
                step_awake(&world);
            }
            assert!(user_force(&world) > 1.0, "the marker never pushed the body");

            if despawn {
                world.despawn(marker);
            } else {
                world.insert(marker, marker_volume(5000.0, 6000.0));
            }
            for _ in 0..5 {
                step_awake(&world);
            }
            assert_eq!(
                user_force(&world),
                0.0,
                "the body kept the marker's last force after leaving it (despawn = {despawn})"
            );
        }
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
            build_ragdoll(&mut pw, &spec, &ContactConfig::DEFAULT).expect("sane ragdoll seed")
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
            build_ragdoll(&mut pw, &spec, &cfg).expect("sane ragdoll seed")
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

    /// #5540 — the jointed-ragdoll half of the buoyancy settle contract.
    /// Rapier applies `RigidBody` damping only on the rigid-body writeback
    /// path, which skips multibody links — so the submerged damping this
    /// pass sets was stored and never applied on every jointed rig, and a
    /// floating corpse's heave grew without bound (the audit's probe: two
    /// jointed balls reached y ∈ [-1438, +136] over 40 s while their
    /// `linear_damping` field read 1.5; the unjointed pair settled in 2.5 s
    /// and slept). Links now get their drag as a force alongside buoyancy.
    /// This fixture is the minimal jointed rig crossing the surface: the
    /// pre-fix divergence blows the ±80 BU bound within the run window.
    #[test]
    fn jointed_ragdoll_in_water_settles_instead_of_diverging() {
        use crate::ragdoll::{
            build_ragdoll, RagdollBodySpec, RagdollConstraintSpec, RagdollJointSpec, RagdollSpec,
        };
        use crate::world::PHYSICS_DT;
        use crate::{physics_sync_system, ContactConfig};
        use byroredux_core::ecs::components::collision::CollisionShape;
        use byroredux_core::ecs::components::water::{
            WaterContact, WaterKind, WaterMaterial, WaterPlane, WaterVolume,
        };
        use byroredux_core::ecs::World;

        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        // Ratio 2.0 puts the pair's equilibrium AT the surface (the lower
        // ball fully submerged, the upper dry — combined displacement 1.0),
        // so "settled" is visible as straddling the waterline rather than
        // hanging at depth like the 1.02 near-neutral default dictates.
        world.insert_resource(PhysicsWaterConstants {
            buoyancy_density_ratio: 2.0,
            ..PhysicsWaterConstants::default()
        });
        world.register::<WaterContact>();
        world.register::<crate::Ragdoll>();
        // The pass early-returns when either dynamic-body storage is
        // absent, before its Ragdoll arm runs — register both even though
        // this fixture has no standalone dynamic bodies.
        world.register::<crate::RapierHandles>();
        world.register::<byroredux_core::ecs::components::collision::RigidBodyData>();

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

        // Two radius-15 balls joined across the waterline: the lower is
        // submerged at seed, the upper rides it. The size is chosen above
        // the ZOH limit-cycle threshold for a jointed rig (the audit's
        // matrix: ragdolls hold the force over 16 internal small steps, so
        // the settle threshold is ~4x the single-body figure — an h=16 rig
        // limit-cycles at ±80-165 BU even with working drag, which is
        // PHYS-D5-2026-10-11-02's energy source, not this fix's).
        let bone_a = world.spawn();
        let bone_b = world.spawn();
        let spec = RagdollSpec {
            bodies: vec![
                RagdollBodySpec {
                    entity: bone_a,
                    translation: Vec3::new(0.0, -20.0, 0.0),
                    rotation: byroredux_core::math::Quat::IDENTITY,
                    scale: 1.0,
                    shape: CollisionShape::Ball { radius: 15.0 },
                    mass: 20.0,
                    linear_damping: 0.05,
                    angular_damping: 0.05,
                    friction: 0.5,
                    restitution: 0.0,
                },
                RagdollBodySpec {
                    entity: bone_b,
                    translation: Vec3::new(0.0, 10.0, 0.0),
                    rotation: byroredux_core::math::Quat::IDENTITY,
                    scale: 1.0,
                    shape: CollisionShape::Ball { radius: 15.0 },
                    mass: 20.0,
                    linear_damping: 0.05,
                    angular_damping: 0.05,
                    friction: 0.5,
                    restitution: 0.0,
                },
            ],
            constraints: vec![RagdollConstraintSpec {
                body_a: 0,
                body_b: 1,
                joint: RagdollJointSpec::Ragdoll {
                    twist_a: Vec3::X,
                    plane_a: Vec3::Y,
                    pivot_a: Vec3::new(0.0, 15.0, 0.0),
                    twist_b: Vec3::X,
                    plane_b: Vec3::Y,
                    pivot_b: Vec3::new(0.0, -15.0, 0.0),
                    cone_max: std::f32::consts::PI,
                    twist_min: -std::f32::consts::PI,
                    twist_max: std::f32::consts::PI,
                },
            }],
        };
        // In production `activate_ragdoll` inserts the component; here the
        // test owns that step (build_ragdoll is physics-side only).
        let rag = {
            let mut pw = world.resource_mut::<PhysicsWorld>();
            build_ragdoll(&mut pw, &spec, &ContactConfig::DEFAULT).expect("sane seed")
        };
        assert!(
            world
                .resource::<PhysicsWorld>()
                .is_multibody_link(rag.bodies[0].1),
            "the fixture must be a jointed articulation for #5540"
        );
        let handles: Vec<_> = rag.bodies.iter().map(|(_, h, _)| *h).collect();
        world.insert(bone_a, rag);

        // The contract has two halves: the transient stays an order of
        // magnitude below the pre-fix divergence (which reached y ∈
        // [-1438, +136] inside this window), and the rig SETTLES — its
        // last-100-ticks excursion is small, where the pre-fix limit
        // cycle kept growing to the end.
        let mut worst = 0.0_f32;
        let mut settled_worst = 0.0_f32;
        for tick in 0..3600 {
            physics_sync_system(&world, PHYSICS_DT);
            let pw = world.resource::<PhysicsWorld>();
            let mut frame_worst = 0.0_f32;
            for handle in &handles {
                if let Some(b) = pw.bodies.get(*handle) {
                    frame_worst = frame_worst.max(b.translation().y.abs());
                }
            }
            worst = worst.max(frame_worst);
            if tick >= 3500 {
                settled_worst = settled_worst.max(frame_worst);
            }
            if worst > 160.0 {
                break;
            }
        }
        assert!(
            worst < 160.0,
            "the transient must stay an order of magnitude below the pre-fix divergence \
             (worst |y| {worst:.1})"
        );
        assert!(
            settled_worst < 60.0,
            "the jointed rig must settle near the surface (last-100-ticks worst |y| \
             {settled_worst:.1}); a growing limit cycle is the #5540 divergence"
        );
    }

    /// Two jointed damped balls at rest in zero gravity, with the
    /// `WaterCurrentVolume` marker (no water plane anywhere) around them.
    fn damped_ragdoll_in_marker_only_current() -> (
        byroredux_core::ecs::World,
        Vec<rapier3d::prelude::RigidBodyHandle>,
    ) {
        use crate::ragdoll::{
            build_ragdoll, RagdollBodySpec, RagdollConstraintSpec, RagdollJointSpec, RagdollSpec,
        };
        use crate::ContactConfig;
        use byroredux_core::ecs::components::collision::{CollisionShape, RigidBodyData};
        use byroredux_core::ecs::World;

        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        world.insert_resource(PhysicsWaterConstants::default());
        world.register::<WaterContact>();
        world.register::<crate::Ragdoll>();
        world.register::<crate::RapierHandles>();
        world.register::<RigidBodyData>();
        world.resource_mut::<PhysicsWorld>().gravity.y = 0.0;

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
        assert!(world.query::<WaterPlane>().is_none(), "no water surface anywhere");

        let bone_a = world.spawn();
        let bone_b = world.spawn();
        let bone = |entity, y| RagdollBodySpec {
            entity,
            translation: Vec3::new(0.0, y, 0.0),
            rotation: byroredux_core::math::Quat::IDENTITY,
            scale: 1.0,
            shape: CollisionShape::Ball { radius: 8.0 },
            mass: 10.0,
            linear_damping: 1.0,
            angular_damping: 0.05,
            friction: 0.5,
            restitution: 0.0,
        };
        let spec = RagdollSpec {
            bodies: vec![bone(bone_a, 0.0), bone(bone_b, 16.0)],
            constraints: vec![RagdollConstraintSpec {
                body_a: 0,
                body_b: 1,
                joint: RagdollJointSpec::Ragdoll {
                    twist_a: Vec3::X,
                    plane_a: Vec3::Y,
                    pivot_a: Vec3::new(0.0, 15.0, 0.0),
                    twist_b: Vec3::X,
                    plane_b: Vec3::Y,
                    pivot_b: Vec3::new(0.0, -15.0, 0.0),
                    cone_max: std::f32::consts::PI,
                    twist_min: -std::f32::consts::PI,
                    twist_max: std::f32::consts::PI,
                },
            }],
        };
        let rag = {
            let mut pw = world.resource_mut::<PhysicsWorld>();
            build_ragdoll(&mut pw, &spec, &ContactConfig::DEFAULT).expect("sane seed")
        };
        let handles: Vec<_> = rag.bodies.iter().map(|(_, h, _)| *h).collect();
        assert!(world.resource::<PhysicsWorld>().is_multibody_link(handles[0]));
        {
            // Awake: a ragdoll built asleep would hide the bug (the
            // `current` arm only forces an awake body).
            let mut pw = world.resource_mut::<PhysicsWorld>();
            for h in &handles {
                pw.bodies[*h].wake_up(true);
            }
        }
        world.insert(bone_a, rag);
        (world, handles)
    }

    /// #5698 — a placed `WaterCurrentVolume` that overlaps no water plane
    /// must push an awake, damped ragdoll link. Pre-fix the current arm added
    /// its force without registering the body in `water_force_targets`, and
    /// the dry-link damping pass `reset_forces`d it away every stepping frame:
    /// the placed current did nothing to an awake ragdoll.
    #[test]
    fn marker_only_current_pushes_an_awake_damped_ragdoll_link() {
        use crate::physics_sync_system;
        use crate::world::PHYSICS_DT;
        let (world, handles) = damped_ragdoll_in_marker_only_current();
        // Keep the terminal speed above Rapier's sleep threshold now that
        // the link's authored damping correctly acts against the current.
        world.resource_mut::<PhysicsWaterConstants>().current_drag = 10.0;
        for _ in 0..60 {
            physics_sync_system(&world, PHYSICS_DT);
        }
        let pw = world.resource::<PhysicsWorld>();
        let vx = pw.bodies[handles[0]].linvel().x;
        assert!(
            vx > 1.0,
            "the +X current must accelerate the damped link (vx = {vx}); pre-fix the \
             damping pass wiped the current's force and vx stayed ~0"
        );
    }

    /// #5934 — when a current marker is the only water owner of a ragdoll
    /// link, its force must not suppress the link's authored damping.
    #[test]
    fn marker_only_current_keeps_authored_link_damping() {
        use crate::physics_sync_system;
        use crate::world::PHYSICS_DT;
        let (world, handles) = damped_ragdoll_in_marker_only_current();
        world.resource_mut::<PhysicsWorld>().gravity.y = -686.7;

        for _ in 0..60 {
            physics_sync_system(&world, PHYSICS_DT);
        }

        let vy = world.resource::<PhysicsWorld>().bodies[handles[0]].linvel().y;
        assert!(
            vy > -600.0,
            "authored link damping should slow marker-only free fall (vy = {vy}); \\
             without it the link approaches undamped gravity at about -684 BU/s"
        );
        assert!(vy < -100.0, "gravity still accelerates the link (vy = {vy})");
    }

    /// #5697 — the authored angular damping `c` must decay a link's spin at
    /// rate `c`: `ω(t) = ω₀·e^(−c·t)`. The torque used to be `−c·m·ω`, which
    /// decays at `c·m/I` (about `c/66` for a bone-sized capsule), so at
    /// `c = 0.9` a spin lost 22 % in 4 s where the authored value removes 97 %.
    /// Both bones spin up under a persistent torque with every damping off,
    /// then `c` is restored and 4 s are simulated through the production step.
    #[test]
    fn dry_multibody_link_angular_damping_decays_at_the_authored_rate() {
        use crate::world::PHYSICS_DT;
        use byroredux_core::ecs::components::collision::CollisionShape;

        for (c, bound) in [(0.5f32, 0.35f32), (0.9, 0.2)] {
            let (world, handle) = two_link_ragdoll(
                CollisionShape::Capsule { half_height: 10.0, radius: 5.0 },
                10.0,
                0.0,
                0.0,
            );
            {
                let mut pw = world.resource_mut::<PhysicsWorld>();
                pw.gravity.y = 0.0;
                pw.add_torque(handle, Vec3::new(0.0, 0.0, 13_000.0), true);
            }
            for _ in 0..30 {
                world.resource_mut::<PhysicsWorld>().step(PHYSICS_DT);
            }
            let w0 = {
                let mut pw = world.resource_mut::<PhysicsWorld>();
                pw.reset_forces(handle, true);
                for (_, body) in pw.bodies.iter_mut() {
                    body.set_angular_damping(c);
                }
                pw.bodies[handle].angvel().length()
            };
            assert!(w0 > 3.0, "the spin-up must build a spin to decay (|w| = {w0})");
            for _ in 0..240 {
                world.resource_mut::<PhysicsWorld>().step(PHYSICS_DT);
            }
            let w1 = world.resource::<PhysicsWorld>().bodies[handle].angvel().length();
            assert!(
                w1 < bound * w0,
                "c = {c}: |w| {w0:.2} -> {w1:.2} after 4 s must fall below {bound} of the \
                 start (authored e^-{c}·4 = {:.3}); the mass-scaled stand-in left ~0.8",
                (-c * 4.0).exp()
            );
        }
    }

    /// #5697 — the two wet-link angular drag sites (the surface arm and the
    /// marker-only arm) take their torque from the shared inertia-based
    /// helper; a hand-rolled `-c·m·ω` there would silently bring the stand-in
    /// back for submerged links, which no dry-pass test sees.
    #[test]
    fn wet_link_angular_drag_uses_the_shared_inertia_torque() {
        let src = include_str!("water.rs");
        let production = crate::source_scan::production_text(src);
        assert_eq!(
            production.matches("PhysicsWorld::angular_damping_torque(").count(),
            2,
            "both wet sites call the shared helper"
        );
        assert!(
            !production.contains("* mass * w."),
            "a mass-scaled angular torque is back in the wet scan"
        );
    }

    /// #5698 — `water_force_targets` is cleared at the TOP of the scan, so an
    /// early-returning scan (here: every marker and plane gone) cannot leave
    /// the last full scan's set behind to exclude ex-wet links from dry
    /// damping.
    #[test]
    fn an_early_returning_scan_clears_the_water_force_target_set() {
        use crate::physics_sync_system;
        use crate::world::PHYSICS_DT;
        let (mut world, handles) = damped_ragdoll_in_marker_only_current();
        for _ in 0..3 {
            physics_sync_system(&world, PHYSICS_DT);
        }
        assert!(
            world.resource::<PhysicsWorld>().water_force_targets.contains(&handles[0]),
            "premise: a full scan registered the link while the marker was present"
        );
        // The marker streams out: no surfaces, no current volumes.
        let markers: Vec<_> = world
            .query::<WaterCurrentVolume>()
            .map(|q| q.iter().map(|(e, _)| e).collect())
            .unwrap_or_default();
        for marker in markers {
            world.despawn(marker);
        }
        physics_sync_system(&world, PHYSICS_DT);
        assert!(
            world.resource::<PhysicsWorld>().water_force_targets.is_empty(),
            "the early-returning scan must not leave stale water ownership behind"
        );
    }

    /// Two bones (`shape`, `mass`, authored `linear_damping` and
    /// `angular_damping`) joined by a free ragdoll joint, high above the
    /// floor, in a world whose gravity is untouched. Returns the world and the
    /// first bone's body.
    fn two_link_ragdoll(
        shape: byroredux_core::ecs::components::collision::CollisionShape,
        mass: f32,
        linear_damping: f32,
        angular_damping: f32,
    ) -> (byroredux_core::ecs::World, rapier3d::prelude::RigidBodyHandle) {
        use crate::ragdoll::{
            build_ragdoll, RagdollBodySpec, RagdollConstraintSpec, RagdollJointSpec, RagdollSpec,
        };
        use crate::ContactConfig;
        use byroredux_core::ecs::World;

        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        world.register::<crate::Ragdoll>();

        let bone_a = world.spawn();
        let bone_b = world.spawn();
        let bone = |entity| RagdollBodySpec {
            entity,
            translation: Vec3::new(0.0, 1000.0, 0.0),
            rotation: byroredux_core::math::Quat::IDENTITY,
            scale: 1.0,
            shape: shape.clone(),
            mass,
            linear_damping,
            angular_damping,
            friction: 0.5,
            restitution: 0.0,
        };
        let spec = RagdollSpec {
            bodies: vec![bone(bone_a), RagdollBodySpec {
                translation: Vec3::new(0.0, 1016.0, 0.0),
                ..bone(bone_b)
            }],
            constraints: vec![RagdollConstraintSpec {
                body_a: 0,
                body_b: 1,
                joint: RagdollJointSpec::Ragdoll {
                    twist_a: Vec3::X,
                    plane_a: Vec3::Y,
                    pivot_a: Vec3::new(0.0, 15.0, 0.0),
                    twist_b: Vec3::X,
                    plane_b: Vec3::Y,
                    pivot_b: Vec3::new(0.0, -15.0, 0.0),
                    cone_max: std::f32::consts::PI,
                    twist_min: -std::f32::consts::PI,
                    twist_max: std::f32::consts::PI,
                },
            }],
        };
        let rag = {
            let mut pw = world.resource_mut::<PhysicsWorld>();
            build_ragdoll(&mut pw, &spec, &ContactConfig::DEFAULT).expect("sane seed")
        };
        let handle = rag.bodies[0].1;
        world.insert(bone_a, rag);
        (world, handle)
    }

    /// #5540, dry half — the authored damping on a multibody link must
    /// actually damp. Rapier never applies `RigidBody` damping to links, so
    /// pre-fix a jointed pair's drift velocity (built up under gravity, then
    /// released into zero gravity) stayed constant forever. The per-step
    /// link-damping pass must decay it.
    #[test]
    fn dry_multibody_link_damping_decays_drift_velocity() {
        use crate::world::PHYSICS_DT;
        use byroredux_core::ecs::components::collision::CollisionShape;

        let (world, handle) = two_link_ragdoll(CollisionShape::Ball { radius: 8.0 }, 10.0, 1.0, 0.05);

        // Build up drift under gravity, then release into zero gravity:
        // with the damping pass the drift decays; pre-fix it is constant.
        for _ in 0..30 {
            world.resource_mut::<PhysicsWorld>().step(PHYSICS_DT);
        }
        let drift_v0 = world.resource::<PhysicsWorld>().bodies[handle].linvel().y;
        assert!(drift_v0 < -50.0, "gravity must have built drift (got {drift_v0})");
        {
            let mut pw = world.resource_mut::<PhysicsWorld>();
            pw.gravity.y = 0.0;
            pw.reset_forces(handle, true);
        }
        for _ in 0..180 {
            world.resource_mut::<PhysicsWorld>().step(PHYSICS_DT);
        }
        let drift_v1 = world.resource::<PhysicsWorld>().bodies[handle].linvel().y;
        assert!(
            drift_v1.abs() < 0.45 * drift_v0.abs(),
            "the authored link damping must decay the drift: {drift_v0:.1} -> {drift_v1:.1}"
        );
    }
}

/// #5709 — the water pass's per-frame, per-body latches stay on the Fx hasher
/// (hot-path rule #2923; modelled on `skin_slot_pool_maps_are_not_siphash`).
#[cfg(test)]
mod water_latch_hasher_tests {
    use byroredux_core::source_scan::production_text;

    #[test]
    fn water_scan_latches_are_not_siphash() {
        let water = production_text(include_str!("water.rs"));
        let world = production_text(include_str!("world/mod.rs"));
        for (src, decl) in [
            (water, "rest_depth: FxHashMap<EntityId, (f32, u32)>,"),
            (world, "water_force_targets: FxHashSet<RigidBodyHandle>,"),
        ] {
            assert!(
                src.contains(decl),
                "`{decl}` is gone — the buoyancy scan probes it once per wet \
                 dynamic body per frame, which is what makes SipHash-1-3 the \
                 wrong default here (#2923, #5709)",
            );
        }
        // The prune must not regress to the O(latched × targets) search.
        assert!(
            !water.contains("targets.iter().any(|t| t.entity == *entity)"),
            "rest_depth is swept by scan stamp, not by searching `targets` (#5709)"
        );
        assert!(
            !water.contains("std::collections::HashMap")
                && !water.contains("use std::collections::HashMap"),
            "water.rs reaches for std SipHash maps again (#5709)"
        );
    }
}
