//! `physics_sync_system` — per-tick bridge between ECS and Rapier.
//!
//! Runs after `transform_propagation_system` so that `GlobalTransform`
//! is fresh for Phase 1 spawning and Phase 2 kinematic pushes. Walks
//! four phases plus a 2.5 buoyancy hook:
//!
//! 1. **Register** new entities: `(CollisionShape, RigidBodyData,
//!    GlobalTransform)` without `RapierHandles` → build & insert
//!    Rapier body + collider, attach `RapierHandles`.
//! 2. **Push kinematic** transforms: keyframed bodies track the ECS
//!    `GlobalTransform` via `set_next_kinematic_position`.
//! 2.5. **Buoyancy** (`crate::water::apply_buoyancy`, WATAL Phase 2):
//!    Archimedes lift + submerged damping on dynamic bodies inside a
//!    `WaterVolume`. **Its position is the correctness property** — forces
//!    have to be applied before the step that integrates them, so moving it
//!    after phase 3 makes lift lag a frame and reads as a water bug, not an
//!    ordering bug. Numbered 2.5 rather than renumbering 3/4 because it is a
//!    hook into an existing sequence, and it carries its own labelled
//!    `BYRO_PROFILE` bracket (`buoyancy=`).
//! 3. **Step**: drain the fixed-timestep accumulator.
//! 4. **Pull dynamic** transforms back into ECS `Transform` (dynamic
//!    bodies only — static/keyframed are driven the other way).
//!
//! The system early-returns when no `PhysicsWorld` resource is present. That
//! covers test fixtures and embedders that omit it — **not** a loose-NIF
//! viewer opt-out: the shipping binary inserts `PhysicsWorld`
//! unconditionally (`byroredux/src/boot.rs`), so every path including
//! `cargo run -- mesh.nif` runs the full tick (#2880).

use byroredux_core::ecs::components::collision::{CollisionShape, MotionType, RigidBodyData};
use byroredux_core::ecs::components::{
    FormIdComponent, GlobalTransform, Parent, PhysicsSourceForm, RenderLayer, Transform,
};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_core::form_id::FormIdPool;
use rapier3d::prelude::{ColliderBuilder, RigidBodyBuilder, RigidBodyType};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use crate::components::{ActorBoneCollider, RapierHandles};
use crate::config::ContactConfig;
use crate::convert::{
    collision_shape_to_parts, iso_from_trs, quat_from_na, vec3_from_translation, vec3_to_na,
};
use crate::world::PhysicsWorld;

/// Helper for systems that don't depend on rapier3d: set the linear
/// velocity of an ECS-tracked body by `EntityId`. Returns `false` if the
/// entity has no physics handle yet.
pub fn set_linear_velocity(world: &World, entity: EntityId, velocity: glam::Vec3) -> bool {
    let handles = match world
        .query::<RapierHandles>()
        .and_then(|q| q.get(entity).copied())
    {
        Some(h) => h,
        None => return false,
    };
    let nonzero = velocity.length_squared() > 1e-6;
    let mut pw = world.resource_mut::<PhysicsWorld>();
    let Some(body) = pw.bodies.get_mut(handles.body) else {
        return false;
    };
    body.set_linvel(vec3_to_na(velocity), nonzero);
    if nonzero {
        // Re-engage the pipeline step this frame so the velocity integrates
        // even if the scene was otherwise asleep (the static-scene fast path).
        // A zero velocity (a stop) must NOT wake — otherwise a stationary
        // velocity-driven body pins the simulation awake every frame.
        pw.wake();
    }
    true
}

/// M28.5 — queue the next kinematic translation on a body by ECS
/// `EntityId`. Used by `byroredux::systems::character_controller_system`
/// to push the post-KCC-corrected position into Rapier so other
/// bodies' queries see the player at the right spot.
///
/// Mirrors [`set_linear_velocity`]'s shape — opaque to callers that
/// don't depend on rapier3d directly. Returns `false` if the entity
/// has no physics handle yet.
pub fn set_kinematic_translation(world: &World, entity: EntityId, translation: glam::Vec3) -> bool {
    let handles = match world
        .query::<RapierHandles>()
        .and_then(|q| q.get(entity).copied())
    {
        Some(h) => h,
        None => return false,
    };
    let mut pw = world.resource_mut::<PhysicsWorld>();
    let Some(body) = pw.bodies.get_mut(handles.body) else {
        return false;
    };
    let target = vec3_to_na(translation);
    // Only wake on actual movement. The character controller pushes the
    // player capsule every frame to track the camera — even in fly mode and
    // even when standing still — so an unconditional wake here pins the
    // simulation awake forever (the static-scene fast path never engages).
    // A no-op re-target leaves the body's kinematic velocity at zero anyway.
    let moved = (target - *body.translation()).norm_squared() > 1e-4;
    body.set_next_kinematic_translation(target);
    if moved {
        pw.wake();
    }
    true
}

/// Scheduler-compatible physics tick. Place after transform propagation.
pub fn physics_sync_system(world: &World, dt: f32) {
    if world.try_resource::<PhysicsWorld>().is_none() {
        return;
    }

    // `BYRO_PROFILE=1` logs a per-phase breakdown so the dominant phase
    // can be localized without guessing (Phase 1 collect/register, 2
    // push-kinematic, 2.5 buoyancy, 3 Rapier step, 4 pull-dynamic). Silent
    // otherwise.
    let profile = profile_phases();
    let t = |on: bool| on.then(std::time::Instant::now);
    let ms = |s: Option<std::time::Instant>| s.map(|i| i.elapsed().as_secs_f32() * 1000.0);

    // Build list of newcomers while holding only read locks on the
    // relevant storages. We collect to a Vec so Phase 1 can release the
    // read locks before acquiring write locks on PhysicsWorld + RapierHandles.
    let s1 = t(profile);
    let newcomers = collect_newcomers(world);
    let n_new = newcomers.len();
    if !newcomers.is_empty() {
        register_newcomers(world, newcomers);
    }
    let ms1 = ms(s1);

    // Phase 2: push kinematic poses.
    let s2 = t(profile);
    push_kinematic(world);
    let ms2 = ms(s2);

    // Phase 2.5: water buoyancy (WATAL Phase 2). Applies Archimedes lift +
    // submerged damping to dynamic bodies inside a `WaterVolume`, BEFORE the
    // step integrates them. Wake-disciplined (see `crate::water`) so it
    // never pins the static-scene fast path. No-op when no `WaterPlane`
    // entities exist (the loose-NIF demo / interior-without-water path).
    let s25 = t(profile);
    // `n_new > 0` lets the buoyancy phase still float a body that streamed in
    // already submerged (spawned asleep) on its very first frame — the one
    // case its quiesced-scene fast path must not skip.
    crate::water::apply_buoyancy(world, n_new > 0);
    let ms25 = ms(s25);

    // Phase 3: step.
    let s3 = t(profile);
    let steps = {
        let mut pw = world.resource_mut::<PhysicsWorld>();
        pw.step(dt)
    };
    if steps > 1 {
        log::trace!("physics: {steps} substeps consumed");
    }
    let ms3 = ms(s3);

    // Phase 4: pull dynamic transforms back into ECS.
    let s4 = t(profile);
    pull_dynamic(world);
    let ms4 = ms(s4);

    if profile {
        let (awake_dyn, awake_kin) = world.resource::<PhysicsWorld>().awake_counts();
        log::info!(
            "physics_sync phases: collect/register={:.2}ms (new={}) push_kin={:.2}ms buoyancy={:.2}ms step={:.2}ms({} substeps) pull_dyn={:.2}ms | awake dyn={} kin={}",
            ms1.unwrap_or(0.0),
            n_new,
            ms2.unwrap_or(0.0),
            ms25.unwrap_or(0.0),
            ms3.unwrap_or(0.0),
            steps,
            ms4.unwrap_or(0.0),
            awake_dyn,
            awake_kin,
        );
    }

    // #1698 — opt-in awake-faller diagnostic (separate from BYRO_PROFILE so a
    // root-cause run can name the clutter behind a cell-entry settle storm
    // without the per-phase spam).
    if profile_fallers() {
        dump_awake_fallers(world);
    }
}

/// `BYRO_PROFILE` — per-phase timing breakdown.
///
/// #2881 — read once per process, not once per frame. The previous
/// `std::env::var_os(...)` call sites carried the comment "zero cost when the
/// flag is unset", which describes the *branch*, not the lookup: `var_os`
/// takes the process-wide environ lock on every call and allocates an
/// `OsString` on a hit, regardless of the value. Both flags are read on the
/// per-tick path, so that ran twice a frame forever.
///
/// Caching also makes the flags honest: a diagnostic that could flip
/// mid-session would produce a log with an unannounced gap in it.
fn profile_phases() -> bool {
    static PROFILE: OnceLock<bool> = OnceLock::new();
    *PROFILE.get_or_init(|| std::env::var_os("BYRO_PROFILE").is_some())
}

/// `BYRO_PROFILE_FALLERS` — one-shot awake-faller dump. See
/// [`profile_phases`] for why this is cached (#2881).
fn profile_fallers() -> bool {
    static PROFILE_FALLERS: OnceLock<bool> = OnceLock::new();
    *PROFILE_FALLERS.get_or_init(|| std::env::var_os("BYRO_PROFILE_FALLERS").is_some())
}

// ── #1698 awake-faller diagnostic ───────────────────────────────────────

/// Only fire the dump once at least this many dynamic bodies are awake, so
/// it captures a genuine cell-entry settle storm rather than incidental
/// startup motion. Diagnostic trigger only — never affects simulation.
const AWAKE_FALLER_DUMP_FLOOR: usize = 16;

/// Process one-shot: the diagnostic dumps a single time so the 28 s storm
/// doesn't flood the log every frame.
static AWAKE_FALLERS_DUMPED: AtomicBool = AtomicBool::new(false);

/// One awake dynamic body, resolved to its identifying ECS data.
#[derive(Clone)]
struct FallerEntry {
    entity: String,
    y: f32,
    vy: f32,
    /// Stable local form id (xEdit-resolvable), when the entity carries a
    /// resolvable `FormIdComponent`.
    form: Option<u32>,
    /// `RenderLayer` label (`Clutter` / `Arch` / …), when present.
    layer: Option<&'static str>,
}

/// Sort by most-negative vertical velocity (worst free-fallers first) and
/// take the first `n`. A large negative `vy` ⇒ free-falling with no collider
/// beneath it (the #1698 coverage gap); `vy ≈ 0` ⇒ jittering in a
/// spawn-interpenetration pile. Pure, so it's unit-testable.
fn worst_fallers(mut entries: Vec<FallerEntry>, n: usize) -> Vec<FallerEntry> {
    entries.sort_by(|a, b| a.vy.partial_cmp(&b.vy).unwrap_or(std::cmp::Ordering::Equal));
    entries.truncate(n);
    entries
}

/// #1698 — pick the `FormId` to resolve for one awake body: prefer a
/// `FormIdComponent` the entity happens to carry directly (e.g. a future
/// physics-on-render-entity path) over the diagnostic-only
/// `PhysicsSourceForm` backlink that today's bhk-collision spawn path
/// attaches instead. Pure, so it's unit-testable without a live `World`.
fn resolve_source_form(
    direct: Option<byroredux_core::form_id::FormId>,
    backlink: Option<byroredux_core::form_id::FormId>,
) -> Option<byroredux_core::form_id::FormId> {
    direct.or(backlink)
}

fn render_layer_label(l: RenderLayer) -> &'static str {
    match l {
        RenderLayer::Architecture => "Arch",
        RenderLayer::Clutter => "Clutter",
        RenderLayer::Actor => "Actor",
        RenderLayer::Decal => "Decal",
    }
}

/// #1698 — name the awake dynamic bodies behind a Dragonsreach-style
/// settle storm. Implements the curated ISSUE.md's "log entity→base-form
/// for awake fallers" next step: the form ids let a runtime session resolve
/// the specific STAT/FURN/clutter whose bhk or synth-trimesh collision isn't
/// materializing (the root-cause coverage gap the anti-spiral substep budget
/// only mitigates). Gated by `BYRO_PROFILE_FALLERS`, one-shot, pure logging.
fn dump_awake_fallers(world: &World) {
    use rapier3d::prelude::RigidBodyHandle;

    // Snapshot everything `PhysicsWorld` alone can answer (awake handles +
    // their y/vy), then drop the guard before opening any ECS storage —
    // every other site in this crate (`push_kinematic`, `pull_dynamic`,
    // `apply_buoyancy`) acquires `RapierHandles` before `PhysicsWorld`;
    // holding `PhysicsWorld` underneath `RapierHandles` here was the
    // opposite order (#2136).
    let body_snapshots: Vec<(RigidBodyHandle, f32, f32)> = {
        let pw = world.resource::<PhysicsWorld>();
        let awake: Vec<RigidBodyHandle> = pw.islands.active_dynamic_bodies().to_vec();
        if awake.len() < AWAKE_FALLER_DUMP_FLOOR {
            return; // not a storm yet — don't consume the one-shot
        }
        if AWAKE_FALLERS_DUMPED.swap(true, Ordering::Relaxed) {
            return; // already dumped once this process
        }
        awake
            .iter()
            .filter_map(|h| {
                pw.bodies
                    .get(*h)
                    .map(|body| (*h, body.translation().y, body.linvel().y))
            })
            .collect()
    };

    // Invert RapierHandles (entity → body) into body → entity for lookup.
    let mut body_to_entity: HashMap<RigidBodyHandle, EntityId> = HashMap::new();
    if let Some(hq) = world.query::<RapierHandles>() {
        for (entity, h) in hq.iter() {
            body_to_entity.insert(h.body, entity);
        }
    }
    let layer_q = world.query::<RenderLayer>();
    let form_q = world.query::<FormIdComponent>();
    // #1698 — bhk-collision physics entities (`cell_loader::spawn`'s
    // standalone collision spawn loop) aren't the REFR's `FormIdComponent`-
    // carrying placement root; they carry `PhysicsSourceForm` instead (see
    // that component's doc for why `FormIdComponent` itself isn't reused
    // here). Prefer `FormIdComponent` when an entity happens to carry it
    // directly (e.g. a future physics-on-render-entity path), else fall
    // back to the source backlink.
    let physics_source_q = world.query::<PhysicsSourceForm>();
    let pool = world.try_resource::<FormIdPool>();

    let mut entries: Vec<FallerEntry> = Vec::with_capacity(body_snapshots.len());
    let (mut vy_min, mut vy_max) = (f32::INFINITY, f32::NEG_INFINITY);
    for (h, y, vy) in &body_snapshots {
        let vy = *vy;
        vy_min = vy_min.min(vy);
        vy_max = vy_max.max(vy);
        let entity = body_to_entity.get(h).copied();
        let layer = entity
            .and_then(|e| layer_q.as_ref().and_then(|q| q.get(e).copied()))
            .map(render_layer_label);
        let form = entity.and_then(|e| {
            let direct = form_q.as_ref().and_then(|q| q.get(e).copied()).map(|c| c.0);
            let backlink = physics_source_q
                .as_ref()
                .and_then(|q| q.get(e).copied())
                .map(|c| c.0);
            let fid = resolve_source_form(direct, backlink)?;
            pool.as_ref()?.resolve(fid).map(|pair| pair.local.0)
        });
        entries.push(FallerEntry {
            entity: entity.map(|e| e.to_string()).unwrap_or_else(|| "?".into()),
            y: *y,
            vy,
            form,
            layer,
        });
    }

    let total = entries.len();
    let worst = worst_fallers(entries, 24);
    log::warn!(
        "#1698 awake-faller dump: {total} awake dynamic bodies, vy range [{vy_min:.0}, {vy_max:.0}] \
         BU/s. Worst {} by downward velocity (large -vy = free-falling with no collider beneath; \
         vy≈0 = spawn-interpenetration jitter pile). Resolve the form ids in xEdit to find the \
         STAT/FURN/clutter whose collision isn't materializing:",
        worst.len(),
    );
    for f in &worst {
        log::warn!(
            "  entity {} layer={} form={} y={:.0} vy={:.0}",
            f.entity,
            f.layer.unwrap_or("?"),
            f.form
                .map(|id| format!("{id:#08X}"))
                .unwrap_or_else(|| "?".into()),
            f.y,
            f.vy,
        );
    }
}

/// Number of nearby colliders named individually by
/// [`dump_spawn_collider_census`]. A vestibule is a handful of architecture
/// pieces; anything past this is a wall of log that hides the answer.
///
/// Safe to truncate at because the entries are ordered by distance from the
/// probe height (#2875) — the cut falls on the far end of the column, not on
/// the floor. Before that fix the ordering was absolute-Y descending, so in
/// any two-storey interior these 24 slots went to roof beams and the upper
/// landing while the spawn-height geometry landed in the "omitted" tail.
const SPAWN_CENSUS_DETAIL_CAP: usize = 24;

/// Cell-wide collision-authoring totals, forwarded into the census so a
/// `0 total` column can name its cause (#2874).
///
/// Summed by the caller over the cell's `CachedNifImport` entries — the same
/// `CollisionAuthoringSummary` counts the spawn path already uses to pick the
/// packed-Havok proxy. Without them the census's `0 total` bucket conflates
/// "nothing was authored" with "N classic shapes were authored and every one
/// was dropped in translation", which is exactly the distinction that summary
/// was built to remove (`docs/engine/physics.md`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpawnCensusAuthoring {
    /// `BhkCollisionObject` chains — decodable classic Havok.
    pub classic: u32,
    /// `BhkNPCollisionObject` — FO4+ packed Havok, payload still opaque.
    pub new_physics: u32,
    /// `BhkPCollisionObject` — Skyrim+ phantom / trigger volumes.
    pub phantom: u32,
}

/// Everything [`dump_spawn_collider_census`] needs about the spawn probe that
/// failed, so the diagnostic can reproduce the sweep instead of guessing at
/// its geometry.
#[derive(Debug, Clone, Copy)]
pub struct SpawnCensusProbe {
    /// Probe XZ — the column to census.
    pub x: f32,
    /// Probe height. Both the census sort key and the origin of the
    /// unfiltered re-sweep.
    pub y: f32,
    pub z: f32,
    /// Column half-width, BU.
    pub radius: f32,
    /// Capsule the failing rungs swept, so the re-sweep matches them.
    pub capsule_half_height: f32,
    pub capsule_radius: f32,
    pub max_distance: f32,
    /// Walkability threshold the rungs applied to `normal1.y`.
    pub min_walkable_normal_y: f32,
    /// Cell-wide authoring totals; `None` when the caller has no NIF cache to
    /// sum (a synthetic World, or the loose-NIF path).
    pub authoring: Option<SpawnCensusAuthoring>,
}

/// Verdict of the unfiltered re-sweep run on the census path (#2874).
///
/// `cast_capsule_down_onto_walkable_surface` returns `None` for both "hit
/// nothing" and "hit something too steep to stand on", so the three-rung
/// ladder cannot report which happened. Re-running through
/// `cast_capsule_down_surface_and_normal` recovers the normal it discarded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpawnProbeVerdict {
    /// The swept capsule met nothing at all within `max_distance`.
    NoHit,
    /// It met a surface whose slope failed the walkable test — the spawn was
    /// blocked by a ramp/wall, NOT by a missing or mis-transformed collider.
    RejectedNonWalkable { surface_y: f32, normal_y: f32 },
    /// It met a walkable surface. Reaching this from the all-rungs-missed
    /// path means the census probe and the spawn rungs disagree (different
    /// origin, capsule, or a query pipeline refreshed in between).
    Walkable { surface_y: f32, normal_y: f32 },
}

impl SpawnProbeVerdict {
    fn classify(hit: Option<(f32, f32)>, min_walkable_normal_y: f32) -> Self {
        match hit {
            None => Self::NoHit,
            Some((surface_y, normal_y)) => {
                if normal_y.abs() >= min_walkable_normal_y.clamp(0.0, 1.0) {
                    Self::Walkable {
                        surface_y,
                        normal_y,
                    }
                } else {
                    Self::RejectedNonWalkable {
                        surface_y,
                        normal_y,
                    }
                }
            }
        }
    }
}

/// One line of a spawn-column census (#2202). Pure data so the grouping is
/// unit-testable without a live `World`.
#[derive(Debug, Clone, PartialEq)]
pub struct SpawnCensusEntry {
    pub body_type: &'static str,
    pub is_sensor: bool,
    /// AABB centre Y — the "is there anything at floor height here?" number.
    pub center_y: f32,
    pub min_y: f32,
    pub max_y: f32,
    pub entity: Option<String>,
    pub form: Option<u32>,
    pub layer: Option<&'static str>,
}

/// Tally of a census by parent body type, in the order the #2202
/// candidate table reads. Pure, so it's unit-testable.
///
/// Returns `(fixed, dynamic, kinematic, sensors, orphan)`.
fn census_tally(entries: &[SpawnCensusEntry]) -> (usize, usize, usize, usize, usize) {
    let mut t = (0usize, 0usize, 0usize, 0usize, 0usize);
    for e in entries {
        if e.is_sensor {
            t.3 += 1;
        }
        match e.body_type {
            "Fixed" => t.0 += 1,
            "Dynamic" => t.1 += 1,
            "KinematicPos" | "KinematicVel" => t.2 += 1,
            _ => t.4 += 1,
        }
    }
    t
}

/// Boot-time sink for [`spawn_collider_census_report`] — emits the same
/// report through `log::warn!`.
///
/// #2876 — the report body moved into `spawn_collider_census_report` so the
/// `phys.census` console command can render the identical text. This
/// diagnostic used to be reachable only from `setup_scene`'s frame-0
/// door-teleport branch, which is never the situation an operator is in: a
/// missing floor is noticed by falling through it, not by reading boot logs.
pub fn dump_spawn_collider_census(world: &World, probe: SpawnCensusProbe) {
    for line in spawn_collider_census_report(world, probe) {
        log::warn!("{line}");
    }
}

/// #2202 — census every collider in the vertical column around a spawn XZ,
/// grouped by parent body type and resolved back to the placement form that
/// produced it.
///
/// The discriminating diagnostic the issue asks for as its first step. When
/// the door-spawn floor probe misses on all three rungs, the existing logs
/// cannot tell these apart:
///
/// - colliders present, all Dynamic → the floor is authored non-Fixed, so
///   `QueryFilter::exclude_dynamic` hid it from the probe and
///   `static_colliders_aabb`'s Fixed-only count hid it from the sanity log;
/// - colliders present, Fixed, wrong Y → a transform/scale composition bug,
///   not a missing collider;
/// - no colliders in the column but the owning REFR resolves → the shape
///   dropped during import while a sibling shape in the same NIF kept
///   `cached.collisions` non-empty, which suppresses the synthesized-trimesh
///   fallback (that gate is per-NIF, `spawn.rs`, while the fallback itself is
///   per-sub-mesh);
/// - nothing at all → a REFR-level gap, upstream of collision entirely.
///
/// Pure reads, no state change — `PhysicsWorld`'s query surface plus the ECS
/// rows that name the colliders it finds. Same lock-order discipline as
/// `dump_awake_fallers` (#2136): snapshot everything `PhysicsWorld` can
/// answer, drop the guard, then open ECS storages.
///
/// Returned as lines rather than logged so both sinks render identical
/// text: [`dump_spawn_collider_census`] on the boot-time all-rungs-missed
/// path, and the `phys.census` console command on demand (#2876).
///
/// `probe` is the failed spawn probe itself: the XZ column to census, the Y
/// the floor was expected near, and the capsule the three rungs swept. It
/// drives three things the pre-fix census could not do (#2874 / #2875) —
/// ordering the column by distance from the probe rather than by absolute
/// world Y, re-running the sweep *unfiltered* so a walkability rejection is
/// distinguishable from a genuine miss, and (via `authoring`) splitting
/// "no collider authored" from "authored but dropped in translation".
pub fn spawn_collider_census_report(world: &World, probe: SpawnCensusProbe) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let SpawnCensusProbe {
        x,
        y: probe_y,
        z,
        radius,
        capsule_half_height,
        capsule_radius,
        max_distance,
        min_walkable_normal_y,
        authoring,
    } = probe;
    // Same lock-order discipline as `dump_awake_fallers` (#2136): snapshot
    // everything `PhysicsWorld` can answer, drop the guard, then open ECS
    // storages.
    let (nearby, verdict) = {
        let pw = world.resource::<PhysicsWorld>();
        let nearby = pw.colliders_near_xz(x, probe_y, z, radius);
        // Re-run the rungs' own sweep WITHOUT the walkability filter, so the
        // normal `cast_capsule_down_onto_walkable_surface` discards is
        // available to the log (#2874).
        let hit = pw.cast_capsule_down_surface_and_normal(
            byroredux_core::math::Vec3::new(x, probe_y, z),
            capsule_half_height,
            capsule_radius,
            max_distance,
            None,
        );
        (
            nearby,
            SpawnProbeVerdict::classify(hit, min_walkable_normal_y),
        )
    };

    let mut body_to_entity: HashMap<rapier3d::prelude::RigidBodyHandle, EntityId> = HashMap::new();
    if let Some(hq) = world.query::<RapierHandles>() {
        for (entity, h) in hq.iter() {
            body_to_entity.insert(h.body, entity);
        }
    }
    let layer_q = world.query::<RenderLayer>();
    let form_q = world.query::<FormIdComponent>();
    let physics_source_q = world.query::<PhysicsSourceForm>();
    let pool = world.try_resource::<FormIdPool>();

    let entries: Vec<SpawnCensusEntry> = nearby
        .iter()
        .map(|n| {
            let entity = n.body.and_then(|h| body_to_entity.get(&h).copied());
            SpawnCensusEntry {
                body_type: n.body_type,
                is_sensor: n.is_sensor,
                center_y: 0.5 * (n.aabb_min[1] + n.aabb_max[1]),
                min_y: n.aabb_min[1],
                max_y: n.aabb_max[1],
                entity: entity.map(|e| e.to_string()),
                form: entity.and_then(|e| {
                    let direct = form_q.as_ref().and_then(|q| q.get(e).copied()).map(|c| c.0);
                    let backlink = physics_source_q
                        .as_ref()
                        .and_then(|q| q.get(e).copied())
                        .map(|c| c.0);
                    let fid = resolve_source_form(direct, backlink)?;
                    pool.as_ref()?.resolve(fid).map(|pair| pair.local.0)
                }),
                layer: entity
                    .and_then(|e| layer_q.as_ref().and_then(|q| q.get(e).copied()))
                    .map(render_layer_label),
            }
        })
        .collect();

    let (fixed, dynamic, kinematic, sensors, orphan) = census_tally(&entries);
    // #2874 — the walkability arm the pre-fix summary was missing entirely.
    // It comes FIRST because when it fires, the column tallies below are a
    // red herring: there is a surface here, it is simply too steep to stand
    // on, and the old text would have steered the reader at "Fixed>0 at a
    // wrong Y ⇒ transform composition".
    match verdict {
        SpawnProbeVerdict::RejectedNonWalkable {
            surface_y,
            normal_y,
        } => out.push(format!(
            "#2874 unfiltered sweep from y={probe_y:.0} HIT y={surface_y:.1} with \
             normal_y={normal_y:.3} ⇒ REJECTED as non-walkable (min={min_walkable_normal_y:.3}). \
             The spawn is blocked by a slope, not by a missing or mis-transformed collider — \
             the collider census below is NOT the cause.",
        )),
        SpawnProbeVerdict::NoHit => out.push(format!(
            "#2874 unfiltered sweep from y={probe_y:.0} over {max_distance:.0} BU hit NOTHING \
             (walkability is not the cause; the column census below is).",
        )),
        SpawnProbeVerdict::Walkable {
            surface_y,
            normal_y,
        } => out.push(format!(
            "#2874 unfiltered sweep from y={probe_y:.0} hit a WALKABLE surface y={surface_y:.1} \
             normal_y={normal_y:.3} — the census probe and the spawn rungs disagree (different \
             origin/capsule, or the query pipeline was refreshed between them).",
        )),
    }
    // #2874 — split `0 total` into "nothing authored" vs "authored, dropped
    // in translation". `CollisionAuthoringSummary` already computes the
    // discriminator; the pre-fix census read the Rapier side only and so
    // re-introduced the exact conflation that summary exists to remove.
    match authoring {
        Some(a) if entries.is_empty() && a.classic + a.new_physics + a.phantom > 0 => {
            out.push(format!(
                "#2874 …but the cell's NIFs DID author collision: classic={} new_physics={} \
             phantom={} ⇒ the shapes were DROPPED IN TRANSLATION (decode/registration), not \
             absent from the source. `new_physics>0` additionally means FO4+ packed Havok whose \
             payload is still opaque — the compatibility proxy should have covered it.",
                a.classic, a.new_physics, a.phantom,
            ))
        }
        Some(_) if entries.is_empty() => out.push(format!(
            "#2874 …and the cell's NIFs authored NO collision at all (classic=0 new_physics=0 \
             phantom=0) ⇒ genuinely non-colliding content or a REFR-level gap, NOT a \
             translation drop.",
        )),
        Some(a) => out.push(format!(
            "#2874 cell collision authoring: classic={} new_physics={} phantom={}.",
            a.classic, a.new_physics, a.phantom,
        )),
        None => out.push(format!(
            "#2874 cell collision authoring unavailable (no NIF import cache) — `0 total` below \
             cannot be split into 'nothing authored' vs 'dropped in translation'.",
        )),
    }
    out.push(format!(
        "#2202 spawn-column census at XZ ({x:.1}, {z:.1}) ±{radius:.0} BU around y={probe_y:.0}: \
         {} colliders (Fixed={fixed} Dynamic={dynamic} Kinematic={kinematic} orphan={orphan}, of \
         which {sensors} sensors). Fixed=0 with Dynamic>0 ⇒ the floor is authored non-Fixed and \
         both the spawn probe (exclude_dynamic) and the static census are blind to it; Fixed>0 \
         at a wrong Y ⇒ transform composition. Nearest {} by distance from the probe height:",
        entries.len(),
        entries.len().min(SPAWN_CENSUS_DETAIL_CAP),
    ));
    for e in entries.iter().take(SPAWN_CENSUS_DETAIL_CAP) {
        out.push(format!(
            "  {}{} entity={} form={} layer={} y=[{:.0}, {:.0}] centre={:.0}",
            e.body_type,
            if e.is_sensor { " (sensor)" } else { "" },
            e.entity.as_deref().unwrap_or("?"),
            e.form
                .map(|id| format!("{id:#08X}"))
                .unwrap_or_else(|| "?".into()),
            e.layer.unwrap_or("?"),
            e.min_y,
            e.max_y,
            e.center_y,
        ));
    }
    if entries.len() > SPAWN_CENSUS_DETAIL_CAP {
        out.push(format!(
            "  … {} further colliders omitted",
            entries.len() - SPAWN_CENSUS_DETAIL_CAP,
        ));
    }
    out
}

// ── Phase 1 ─────────────────────────────────────────────────────────────

/// Snapshot of one entity to register in Rapier.
///
/// One unified path: every newcomer carries the engine `CollisionShape`,
/// `RigidBodyData` (with motion type), and `GlobalTransform`. The
/// rotation-lock decision is derived from `motion_type` — character
/// kinematic bodies (and only those) lock rotations, everything else
/// follows the body data verbatim.
struct Newcomer {
    entity: EntityId,
    shape: CollisionShape,
    body_data: RigidBodyData,
    global: GlobalTransform,
    /// Entity carries [`ActorBoneCollider`] — its colliders join
    /// [`crate::ACTOR_BONE_GROUP`] so ground probes skip them (#2873).
    is_actor_bone: bool,
}

impl Newcomer {
    fn body_type(&self) -> RigidBodyType {
        motion_type_to_rapier(self.body_data.motion_type)
    }

    fn lock_rotations(&self) -> bool {
        matches!(self.body_data.motion_type, MotionType::CharacterKinematic)
    }
}

fn collect_newcomers(world: &World) -> Vec<Newcomer> {
    let mut out = Vec::new();

    // #2867 — this check used to live at the BOTTOM of `register_newcomers`,
    // *after* every newcomer's body and colliders had already been committed
    // to Rapier. Missing storage there meant returning with live solver
    // objects and no ECS row pointing at them: `release_victim_rapier_bodies`
    // walks `RapierHandles`/`Ragdoll` rows, so nothing could ever free them.
    // Worse, the skip below is gated on the same query, so the identical
    // newcomer set — full `TriMesh` vertex/index clones included — was
    // re-collected and re-inserted every single frame, unbounded (the #1520
    // leak shape on a different trigger). Refusing to collect at all keeps
    // the miss path from reaching the solver, and stops the re-collect with
    // the same edit.
    let Some(handles_q) = world.query::<RapierHandles>() else {
        log::error!(
            "RapierHandles storage missing — call World::register::<RapierHandles>() \
             during setup before running physics_sync_system. No colliders will be \
             registered this frame (registering them would leak into Rapier with no \
             ECS handle to free them by)."
        );
        return out;
    };

    let (Some(shape_q), Some(body_q)) = (
        world.query::<CollisionShape>(),
        world.query::<RigidBodyData>(),
    ) else {
        return out;
    };

    // Acquired last — matches `push_kinematic`'s Handles → Body → Global
    // order so the two functions never present the lock-order detector
    // with opposite edges for the same pair (#313).
    let Some(gq) = world.query::<GlobalTransform>() else {
        return out;
    };

    // Optional — a World with no live actors never registers the storage.
    let bone_q = world.query::<ActorBoneCollider>();

    for (entity, shape) in shape_q.iter() {
        if handles_q.contains(entity) {
            continue;
        }
        let Some(body_data) = body_q.get(entity) else {
            continue;
        };
        let Some(global) = gq.get(entity) else {
            continue;
        };
        out.push(Newcomer {
            entity,
            shape: shape.clone(),
            body_data: body_data.clone(),
            global: *global,
            is_actor_bone: bone_q.as_ref().is_some_and(|q| q.contains(entity)),
        });
    }

    out
}

fn motion_type_to_rapier(m: MotionType) -> RigidBodyType {
    match m {
        MotionType::Static => RigidBodyType::Fixed,
        MotionType::Keyframed | MotionType::CharacterKinematic => {
            RigidBodyType::KinematicPositionBased
        }
        MotionType::Dynamic => RigidBodyType::Dynamic,
    }
}

fn register_newcomers(world: &World, newcomers: Vec<Newcomer>) {
    // Snapshot `ContactConfig` once per batch. Defaults if missing — the
    // resource is optional for backwards compatibility with embedders
    // that don't insert it explicitly. (See `byroredux::scene` for the
    // engine-side install.)
    let cfg = world
        .try_resource::<ContactConfig>()
        .map(|r| *r)
        .unwrap_or_default();

    let mut pw = world.resource_mut::<PhysicsWorld>();

    let mut registered: Vec<(EntityId, RapierHandles)> = Vec::with_capacity(newcomers.len());

    for n in newcomers {
        // Convert the engine shape into a flat list of Rapier parts.
        // Compounds are fully flattened and TriMeshes surface as their
        // own parts — attaching one collider per part is Rapier's
        // idiomatic path for mixed-composite compositions (#373, which
        // eliminated the 9,555/30s parry3d panic storm the previous
        // single-compound path produced on exterior cells).
        // #2860 — `GlobalTransform::scale` must be baked into the shape here.
        // Rapier's body isometry below carries translation + rotation only, so
        // this is the last point at which the placement's uniform scale can
        // reach the collider at all; dropping it gave every scaled REFR a
        // collider of the authored size (a 2× rock's collider half the visible
        // stone) while `compose_trs` still spread its parts apart.
        let parts = collision_shape_to_parts(&n.shape, n.global.scale, &cfg);
        // #3067 (PHYS-D3-2026-08-16-04) — `collision_shape_to_parts` always
        // yields at least one part: every `CollisionShape` variant pushes
        // unconditionally, and its own `out.is_empty()` fallback (a tiny
        // ball, #3066) covers the degenerate-shape case. An empty `parts`
        // here would mean that producer contract broke, not a legitimate
        // "nothing to register" case — state the invariant instead of
        // silently skipping the newcomer.
        debug_assert!(
            !parts.is_empty(),
            "collision_shape_to_parts must never return zero parts (see its own \
             out.is_empty() fallback) — a newcomer would silently register with \
             no collider"
        );

        let body_type = n.body_type();
        let lock_rotations = n.lock_rotations();

        let mut body_builder = RigidBodyBuilder::new(body_type)
            .position(iso_from_trs(n.global.translation, n.global.rotation))
            .linear_damping(n.body_data.linear_damping)
            .angular_damping(n.body_data.angular_damping);
        if lock_rotations {
            body_builder = body_builder.lock_rotations();
        }
        // EXTERIOR-FREEZE FIX: spawn dynamic bodies ASLEEP. A streamed cell's
        // dynamic content — above all each NPC's 18-bone RagdollTemplate —
        // would otherwise free-fall (no terrain collider beneath it), never
        // rest, never sleep, and pin `physics_sync_system`'s step over thousands
        // of awake bodies. Measured: a single Skyrim-exterior streaming frame
        // hit `atw_scheduler=3005ms` with ~3000 awake dynamics (the "renderer
        // freezes in exteriors" report; the near-water device-loss was a
        // downstream symptom of these multi-second stalls — `docs/engine/watal.md`
        // §0). A live actor's ragdoll is inert until death anyway, and placed
        // clutter is authored resting, so asleep is the correct spawn state.
        // Rapier auto-wakes a sleeping body on contact or applied force, so
        // player interaction and WATAL buoyancy still engage it.
        if matches!(body_type, RigidBodyType::Dynamic) {
            body_builder = body_builder.sleeping(true);
        }
        let body = body_builder.build();
        let body_handle = pw.bodies.insert(body);

        // Split-borrow: destructure to avoid "&mut pw twice" through field access.
        let PhysicsWorld {
            ref mut bodies,
            ref mut colliders,
            ..
        } = *pw;

        // Distribute mass across parts so the total matches the body's
        // configured mass — Rapier sums collider masses on insertion.
        // Friction/restitution copy onto every collider identically.
        let part_mass = n.body_data.mass.max(0.0) / parts.len() as f32;
        let contact_skin = cfg.default_contact_skin_bu.max(0.0);
        let mut first_collider_handle: Option<rapier3d::prelude::ColliderHandle> = None;
        // #2873 — label a live actor's ragdoll-bone colliders so downward
        // floor probes can mask them out. Membership only: the filter half
        // stays `Group::ALL`, so the bones still collide with everything
        // they did before; this changes what *queries* see, not the solver.
        let groups = if n.is_actor_bone {
            rapier3d::prelude::InteractionGroups::new(
                crate::ACTOR_BONE_GROUP,
                rapier3d::prelude::Group::ALL,
            )
        } else {
            rapier3d::prelude::InteractionGroups::all()
        };
        for (iso, shape) in parts {
            let collider = ColliderBuilder::new(shape)
                .position(iso)
                .friction(n.body_data.friction)
                .restitution(n.body_data.restitution)
                .mass(part_mass)
                .contact_skin(contact_skin)
                .collision_groups(groups)
                // #2549 — a body authored on Havok's non-collidable layer
                // (RigidBodyData::collidable = false) still gets a real
                // collider (queryable, e.g. for a future trigger-volume
                // consumer) but must not physically block the player/NPCs
                // or intercept gameplay rays — a sensor gives exactly that:
                // present in the solver, no contact response, and (per
                // `gameplay_ray_ignores_trigger_sensors` in world.rs)
                // already excluded from ray queries elsewhere in this crate.
                .sensor(!n.body_data.collidable)
                .build();
            let handle = colliders.insert_with_parent(collider, body_handle, bodies);
            if first_collider_handle.is_none() {
                first_collider_handle = Some(handle);
            }
        }

        registered.push((
            n.entity,
            RapierHandles {
                body: body_handle,
                // The first collider is the representative handle the
                // ECS keeps a reference to — Rapier owns the rest of
                // the parts through the parent body relationship.
                collider: first_collider_handle.expect("at least one part was appended above"),
            },
        ));
    }

    // Defer the query-pipeline BVH rebuild until the physics boundary. The
    // step path performs exactly one full rebuild after any substep (or on a
    // no-step dirty fast path), avoiding duplicate O(all colliders) work on
    // streaming frames (#2864).
    if !registered.is_empty() {
        pw.mark_colliders_dirty();
    }

    drop(pw);

    // Attach RapierHandles components. The ECS Query API doesn't expose
    // per-entity insert through a QueryWrite on a previously-unknown
    // entity, so we need mutable access to the World. But systems only
    // get `&World`. We work around this by pre-registering storage and
    // going through the QueryWrite::insert path, which IS available.
    if !registered.is_empty() {
        // #2867 — a defensive net, not the gate. The gate is in
        // `collect_newcomers`: reaching here with a non-empty batch means the
        // storage existed when the batch was collected, and nothing between
        // then and now can remove it. Keep the arm anyway, but note that by
        // this point the bodies and colliders are already in the solver — if
        // this ever fires, the batch leaks. Any future edit that wants to
        // reject a newcomer for a storage reason must do it before the
        // `pw.bodies.insert` above, not here.
        let mut handles_q = match world.query_mut::<RapierHandles>() {
            Some(q) => q,
            None => {
                log::error!(
                    "RapierHandles storage disappeared between collection and registration — \
                     {} bodies are now orphaned in Rapier with no ECS handle to free them by",
                    registered.len(),
                );
                return;
            }
        };
        for (entity, handles) in registered {
            handles_q.insert(entity, handles);
        }
    }
}

// ── Phase 2 ─────────────────────────────────────────────────────────────

fn push_kinematic(world: &World) {
    let Some(handles_q) = world.query::<RapierHandles>() else {
        return;
    };
    let Some(body_q) = world.query::<RigidBodyData>() else {
        return;
    };
    let Some(global_q) = world.query::<GlobalTransform>() else {
        return;
    };

    // Snapshot ECS poses before taking the PhysicsWorld resource guard. The
    // storage guards must not overlap the resource guard (#2404).
    let mut targets = Vec::new();
    for (entity, handles) in handles_q.iter() {
        let Some(body_data) = body_q.get(entity) else {
            continue;
        };
        if body_data.motion_type != MotionType::Keyframed {
            continue;
        }
        let Some(g) = global_q.get(entity) else {
            continue;
        };
        targets.push((handles.body, iso_from_trs(g.translation, g.rotation)));
    }
    drop(handles_q);
    drop(body_q);
    drop(global_q);

    let mut pw = world.resource_mut::<PhysicsWorld>();
    let mut pushed = false;
    for (body_handle, target) in targets {
        if let Some(body) = pw.bodies.get_mut(body_handle) {
            let cur = *body.position();
            // Only re-target a keyframed body whose pose actually changed.
            // Re-pushing an idle body (a closed door) every frame gives it a
            // (near-zero but nonzero) kinematic velocity, keeping the solver
            // busy and the `pending_wake` gate engaged — so it would pin the
            // whole simulation awake. Skipping the no-op push leaves its
            // velocity at exactly zero (the solver skips it) and lets the
            // static-scene fast path engage. See `PhysicsWorld::step`.
            let dt = (cur.translation.vector - target.translation.vector).norm();
            let dr = cur.rotation.angle_to(&target.rotation);
            if dt * dt > 1e-6 || dr > 1e-5 {
                body.set_next_kinematic_position(target);
                pushed = true;
            }
        }
    }
    // A keyframed body got a fresh target — re-engage the pipeline step so
    // it actually moves even if the rest of the scene is asleep.
    if pushed {
        pw.wake();
    }
}

// ── Phase 4 ─────────────────────────────────────────────────────────────

fn pull_dynamic(world: &World) {
    let Some(handles_q) = world.query::<RapierHandles>() else {
        return;
    };
    let Some(body_q) = world.query::<RigidBodyData>() else {
        return;
    };

    // Snapshot the storage-side identity first, then release those guards
    // before touching PhysicsWorld (#2404).
    let mut dynamic_handles = Vec::new();
    for (entity, handles) in handles_q.iter() {
        let Some(body_data) = body_q.get(entity) else {
            continue;
        };
        if body_data.motion_type == MotionType::Dynamic {
            dynamic_handles.push((entity, handles.body));
        }
    }
    // #2135 — the `RapierHandles`/`RigidBodyData` read guards are dropped
    // here, before the `Transform` write lock is ever taken below (`updates`
    // carries everything the write pass needs). `character_controller_system`
    // acquires the reverse pair — a `Transform` read held across
    // `RapierHandles` — so a `Transform` write ever overlapping a live
    // `RapierHandles`/`RigidBodyData` read here would be the ABBA edge
    // against it.
    drop(handles_q);
    drop(body_q);

    // Read Rapier poses with no ECS storage guard alive.
    let mut body_states = Vec::with_capacity(dynamic_handles.len());
    {
        let pw = world.resource::<PhysicsWorld>();
        for (entity, handle) in dynamic_handles {
            let Some(body) = pw.bodies.get(handle) else {
                continue;
            };
            let iso = *body.position();
            body_states.push((
                entity,
                vec3_from_translation(iso.translation),
                quat_from_na(iso.rotation),
                body.is_sleeping(),
            ));
        }
    }

    // Build a list of (entity, new local translation, new local rotation)
    // before taking the Transform write lock.
    let mut updates: Vec<(EntityId, glam::Vec3, glam::Quat)> = Vec::new();
    {
        // #2866 — Rapier's pose is WORLD-space by construction (Phase 1 seeds
        // the body from `GlobalTransform`), but the write target below is the
        // entity's LOCAL `Transform`. For a root those coincide; for a
        // parented body they do not, and the next propagation pass composes
        // `parent_global ∘ local`, applying the parent chain a second time —
        // the body renders offset from where it simulates, by exactly the
        // parent transform. `load_nif_scene_hierarchical` produces precisely
        // this shape (a non-root `NiNode` carrying a Dynamic `bhkCollisionObject`
        // under a non-identity parent chain), so `cargo run -- mesh.nif` reaches
        // it. Divide the parent back out instead of restricting the loader.
        //
        // Both guards are read queries acquired here and dropped with this
        // block, before the `Transform` write below — `transform_propagation_system`
        // holds `Transform` while acquiring `Parent`/`GlobalTransform`, so
        // holding these across the write lock would be the reverse edge (#2135).
        let parent_q = world.query::<Parent>();
        let global_q = world.query::<GlobalTransform>();
        let transform_q = world.query::<Transform>();
        for (entity, translation, rotation, sleeping) in body_states {
            // A parent that has no `GlobalTransform` cannot be divided out;
            // propagation `continue`s on that same case, so the child's global
            // is never recomposed either and the world pose stays correct.
            let parent_global = parent_q
                .as_ref()
                .and_then(|pq| pq.get(entity))
                .and_then(|parent| global_q.as_ref().and_then(|gq| gq.get(parent.0)));
            let (translation, rotation) = match parent_global {
                Some(parent_global) => {
                    GlobalTransform::local_from_world(parent_global, translation, rotation)
                }
                None => (translation, rotation),
            };
            // Dynamic newcomers and settled clutter are intentionally spawned
            // asleep. Once ECS already matches Rapier, avoid handing out a
            // mutable Transform (and arming its dirty bit) every frame. Keep
            // the initial pull when registration seeded a different pose.
            if sleeping
                && transform_q.as_ref().is_some_and(|tq| {
                    tq.get(entity).is_some_and(|t| {
                        (t.translation - translation).length_squared() <= 1e-8
                            && t.rotation.dot(rotation).abs() >= 1.0 - 1e-6
                    })
                })
            {
                continue;
            }
            updates.push((entity, translation, rotation));
        }
    }

    // The `RapierHandles`/`RigidBodyData` read guards are already dropped
    // (see the #2135 note above) — nothing here holds them across this
    // `Transform` write.
    if updates.is_empty() {
        return;
    }

    let Some(mut tq) = world.query_mut::<Transform>() else {
        return;
    };
    for (entity, pos, rot) in updates {
        if let Some(t) = tq.get_mut(entity) {
            t.translation = pos;
            t.rotation = rot;
        }
    }
}

#[cfg(test)]
mod phase_sync_tests {
    use super::physics_sync_system;
    use crate::components::RapierHandles;
    use crate::world::PhysicsWorld;
    use byroredux_core::ecs::components::collision::{CollisionShape, MotionType, RigidBodyData};
    use byroredux_core::ecs::components::{GlobalTransform, Parent, Transform};
    use byroredux_core::ecs::world::World;
    use glam::{Quat, Vec3};

    fn dynamic_body() -> RigidBodyData {
        RigidBodyData {
            motion_type: MotionType::Dynamic,
            ..Default::default()
        }
    }

    /// #2549 — a body authored on Havok's non-collidable layer
    /// (`RigidBodyData::collidable = false`) must register as a Rapier
    /// sensor: present in the solver (queryable), but no contact
    /// response — a real collider that no longer physically blocks
    /// movement, closing the gap `havok_filter` being parsed and never
    /// consumed left open (the body used to spawn as an ordinary solid
    /// collider regardless of its authored layer).
    #[test]
    fn noncollidable_body_registers_as_a_sensor() {
        let mut world = physics_world();
        let entity = world.spawn();
        world.insert(entity, Transform::IDENTITY);
        world.insert(
            entity,
            GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0),
        );
        world.insert(entity, unit_box());
        world.insert(
            entity,
            RigidBodyData {
                collidable: false,
                ..RigidBodyData::STATIC
            },
        );

        physics_sync_system(&world, 0.0);

        let handles = world.query::<RapierHandles>().unwrap();
        let h = handles.get(entity).copied().expect("body must register");
        let pw = world.resource::<PhysicsWorld>();
        let collider = pw
            .colliders
            .get(h.collider)
            .expect("collider must exist in the solver");
        assert!(
            collider.is_sensor(),
            "collidable=false must produce a sensor collider, not a solid one"
        );
    }

    /// Companion negative guard — the ordinary (collidable) case must
    /// NOT be a sensor, the ubiquitous default every other test in this
    /// module implicitly relies on.
    #[test]
    fn collidable_body_does_not_register_as_a_sensor() {
        let mut world = physics_world();
        let entity = world.spawn();
        world.insert(entity, Transform::IDENTITY);
        world.insert(
            entity,
            GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0),
        );
        world.insert(entity, unit_box());
        world.insert(entity, RigidBodyData::STATIC);

        physics_sync_system(&world, 0.0);

        let handles = world.query::<RapierHandles>().unwrap();
        let h = handles.get(entity).copied().expect("body must register");
        let pw = world.resource::<PhysicsWorld>();
        let collider = pw.colliders.get(h.collider).expect("collider must exist");
        assert!(!collider.is_sensor());
    }

    fn unit_box() -> CollisionShape {
        CollisionShape::Cuboid {
            half_extents: Vec3::splat(1.0),
        }
    }

    /// Everything `physics_sync_system` touches, minus `RapierHandles` — the
    /// storage whose absence #2867 is about.
    fn world_without_handles_storage() -> World {
        let mut world = World::new();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<Parent>();
        world.register::<CollisionShape>();
        world.register::<RigidBodyData>();
        world.insert_resource(PhysicsWorld::new());
        world
    }

    fn physics_world() -> World {
        let mut world = world_without_handles_storage();
        world.register::<RapierHandles>();
        world
    }

    /// #2866 — Rapier's pose is world-space, the write target is the LOCAL
    /// `Transform`. A parented dynamic body must have its parent chain divided
    /// back out, or `transform_propagation_system` composes it a second time
    /// and the body renders a whole parent transform away from where it
    /// simulates. Pre-fix this stored the raw world pose, so the child's local
    /// came back as the world `(30, 0, 0)` rather than the local `(5, 0, 0)`.
    ///
    /// `dt = 0` so the solver cannot advance the body: the pose the pull reads
    /// back is exactly the pose registration seeded, which makes the assertion
    /// about the local/world contract alone rather than about integration.
    #[test]
    fn parented_dynamic_body_pulls_back_a_local_not_world_transform() {
        let mut world = physics_world();

        // Parent: translated, rotated a quarter turn about Y, scaled 2x — so a
        // pass that drops any one of the three still fails.
        let parent_global = GlobalTransform::new(
            Vec3::new(20.0, 5.0, -3.0),
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            2.0,
        );
        let parent = world.spawn();
        world.insert(parent, Transform::IDENTITY);
        world.insert(parent, parent_global);

        let local_translation = Vec3::new(5.0, 0.0, 0.0);
        let child_global =
            GlobalTransform::compose(&parent_global, local_translation, Quat::IDENTITY, 1.0);
        let child = world.spawn();
        world.insert(child, Transform::IDENTITY);
        world.insert(child, child_global);
        world.insert(child, Parent(parent));
        world.insert(child, unit_box());
        world.insert(child, dynamic_body());

        physics_sync_system(&world, 0.0);

        let transforms = world.query::<Transform>().unwrap();
        let local = transforms.get(child).copied().unwrap();
        assert!(
            (local.translation - local_translation).length() < 1e-2,
            "child local Transform is {:?}, want the LOCAL {:?} — a world-space \
             pose here gets the parent chain applied twice by propagation",
            local.translation,
            local_translation,
        );

        // The round trip is what actually matters: recomposing the stored
        // local under the parent must land back on the simulated world pose.
        let recomposed =
            GlobalTransform::compose(&parent_global, local.translation, local.rotation, 1.0);
        assert!((recomposed.translation - child_global.translation).length() < 1e-2);
    }

    /// A parentless dynamic body still receives the raw world pose — the
    /// fallback must not perturb the far more common root case.
    #[test]
    fn root_dynamic_body_still_pulls_back_the_world_pose() {
        let mut world = physics_world();
        let seed = Vec3::new(11.0, 22.0, 33.0);
        let entity = world.spawn();
        world.insert(entity, Transform::IDENTITY);
        world.insert(entity, GlobalTransform::new(seed, Quat::IDENTITY, 1.0));
        world.insert(entity, unit_box());
        world.insert(entity, dynamic_body());

        physics_sync_system(&world, 0.0);

        let transforms = world.query::<Transform>().unwrap();
        assert!((transforms.get(entity).unwrap().translation - seed).length() < 1e-2);
    }

    /// #2867 — with `RapierHandles` storage missing there is no way to record
    /// a handle, so nothing may reach the solver: an inserted body would be
    /// unreachable from `release_victim_rapier_bodies` (which walks
    /// `RapierHandles`/`Ragdoll` rows) and therefore unfreeable. Pre-fix the
    /// bodies and colliders went in first and the check ran forty lines later.
    ///
    /// Ticking repeatedly is the load-bearing half: the newcomer skip is gated
    /// on the same missing query, so the pre-fix path re-collected and
    /// re-inserted the identical batch every frame, unbounded.
    #[test]
    fn missing_handles_storage_registers_nothing_and_does_not_accumulate() {
        let mut world = world_without_handles_storage();
        for index in 0..4 {
            let entity = world.spawn();
            world.insert(entity, Transform::IDENTITY);
            world.insert(
                entity,
                GlobalTransform::new(
                    Vec3::new(index as f32 * 10.0, 0.0, 0.0),
                    Quat::IDENTITY,
                    1.0,
                ),
            );
            world.insert(entity, unit_box());
            world.insert(entity, dynamic_body());
        }

        for tick in 1..=8 {
            physics_sync_system(&world, 1.0 / 60.0);
            let pw = world.resource::<PhysicsWorld>();
            assert_eq!(
                (pw.bodies.len(), pw.colliders.len()),
                (0, 0),
                "tick {tick}: bodies/colliders reached the solver with no ECS row to free them by",
            );
        }
    }

    /// The same scene with the storage registered must still register
    /// normally — the #2867 guard rejects only the misconfigured case, and
    /// registers each newcomer exactly once across repeated ticks.
    #[test]
    fn registered_handles_storage_admits_each_newcomer_exactly_once() {
        let mut world = physics_world();
        for index in 0..4 {
            let entity = world.spawn();
            world.insert(entity, Transform::IDENTITY);
            world.insert(
                entity,
                GlobalTransform::new(
                    Vec3::new(index as f32 * 10.0, 0.0, 0.0),
                    Quat::IDENTITY,
                    1.0,
                ),
            );
            world.insert(entity, unit_box());
            world.insert(entity, dynamic_body());
        }

        for _ in 0..8 {
            physics_sync_system(&world, 1.0 / 60.0);
        }

        let pw = world.resource::<PhysicsWorld>();
        assert_eq!(pw.bodies.len(), 4, "one body per newcomer, registered once");
        assert_eq!(pw.colliders.len(), 4);
    }
}

#[cfg(test)]
mod spawn_census_tests {
    use super::{census_tally, SpawnCensusEntry};

    fn entry(body_type: &'static str, is_sensor: bool) -> SpawnCensusEntry {
        SpawnCensusEntry {
            body_type,
            is_sensor,
            center_y: 0.0,
            min_y: -1.0,
            max_y: 1.0,
            entity: None,
            form: None,
            layer: None,
        }
    }

    /// #2202 — the tally is what separates the four candidate mechanisms,
    /// so each family must land in its own bucket. Both kinematic variants
    /// collapse into one count: the spawn probe *can* see keyframed bodies
    /// (`havok_motion_type` maps raw 6 → Keyframed), so they are not part of
    /// the Dynamic blind spot the census exists to expose.
    #[test]
    fn tally_separates_the_four_candidate_families() {
        let entries = vec![
            entry("Fixed", false),
            entry("Fixed", false),
            entry("Dynamic", false),
            entry("KinematicPos", false),
            entry("KinematicVel", false),
            entry("orphan", false),
        ];
        assert_eq!(census_tally(&entries), (2, 1, 2, 0, 1));
    }

    /// Sensors are counted independently of body type — a trigger volume
    /// where the floor should be generates no contacts and is not a floor,
    /// so "Fixed=1" alone would be a misleading all-clear.
    #[test]
    fn sensors_are_counted_alongside_their_body_type() {
        let entries = vec![entry("Fixed", true), entry("Fixed", false)];
        let (fixed, dynamic, kinematic, sensors, orphan) = census_tally(&entries);
        assert_eq!((fixed, dynamic, kinematic, orphan), (2, 0, 0, 0));
        assert_eq!(sensors, 1);
    }

    /// The empty column — candidate (A)/(D) — tallies to all zeros rather
    /// than panicking or defaulting anything into a bucket.
    #[test]
    fn empty_census_tallies_to_zero() {
        assert_eq!(census_tally(&[]), (0, 0, 0, 0, 0));
    }
}

#[cfg(test)]
mod faller_diag_tests {
    use super::{resolve_source_form, worst_fallers, FallerEntry};
    use byroredux_core::form_id::{FormId, FormIdPair, FormIdPool, LocalFormId, PluginId};

    fn entry(vy: f32) -> FallerEntry {
        FallerEntry {
            entity: "e".into(),
            y: 0.0,
            vy,
            form: None,
            layer: None,
        }
    }

    /// #1698 — the diagnostic surfaces the worst free-fallers first: entries
    /// sort by most-negative vertical velocity, and `n` caps the dump size.
    #[test]
    fn worst_fallers_sorts_by_downward_velocity_and_caps() {
        let entries = vec![entry(-10.0), entry(-700.0), entry(0.5), entry(-100.0)];
        let worst = worst_fallers(entries, 2);
        assert_eq!(worst.len(), 2, "capped to n");
        assert_eq!(
            worst[0].vy, -700.0,
            "most-negative vy (fastest faller) first"
        );
        assert_eq!(worst[1].vy, -100.0);
    }

    /// Fewer entries than the cap returns them all, still sorted.
    #[test]
    fn worst_fallers_returns_all_when_under_cap() {
        let worst = worst_fallers(vec![entry(0.0), entry(-50.0)], 10);
        assert_eq!(worst.len(), 2);
        assert_eq!(worst[0].vy, -50.0);
    }

    fn fid(local: u32) -> FormId {
        let mut pool = FormIdPool::new();
        pool.intern(FormIdPair {
            plugin: PluginId::from_filename("Skyrim.esm"),
            local: LocalFormId(local),
        })
    }

    /// #1698 — the standalone bhk-collision entities this diagnostic exists
    /// for carry ONLY the `PhysicsSourceForm` backlink (no `FormIdComponent`
    /// — see that component's doc for why). The fallback must still resolve.
    #[test]
    fn falls_back_to_physics_source_form_when_no_direct_form_id() {
        let backlink = fid(0x0E283F);
        assert_eq!(resolve_source_form(None, Some(backlink)), Some(backlink));
    }

    /// A future path might attach `FormIdComponent` directly to a physics
    /// entity (e.g. physics registered on the render entity itself) — that
    /// must win over any stale/coincidental backlink.
    #[test]
    fn prefers_direct_form_id_over_backlink() {
        let direct = fid(0x000014);
        let backlink = fid(0x0E283F);
        assert_eq!(
            resolve_source_form(Some(direct), Some(backlink)),
            Some(direct)
        );
    }

    /// Neither present (an unresolvable physics entity, e.g. a ragdoll bone)
    /// must resolve to `None`, not panic or fabricate an id.
    #[test]
    fn resolves_to_none_when_neither_present() {
        assert_eq!(resolve_source_form(None, None), None);
    }
}

// ── #2873 / #2874 ───────────────────────────────────────────────────────

#[cfg(test)]
mod actor_bone_group_tests {
    use super::*;
    use crate::components::ActorBoneCollider;
    use crate::world::PhysicsWorld;
    use byroredux_core::ecs::components::collision::{CollisionShape, MotionType, RigidBodyData};
    use byroredux_core::ecs::components::{GlobalTransform, Parent, Transform};
    use byroredux_core::ecs::world::World;
    use glam::{Quat, Vec3};

    fn world() -> World {
        let mut world = World::new();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<Parent>();
        world.register::<CollisionShape>();
        world.register::<RigidBodyData>();
        world.register::<RapierHandles>();
        world.register::<ActorBoneCollider>();
        world.insert_resource(PhysicsWorld::new());
        world
    }

    fn spawn_bone(world: &mut World, tag: bool) -> EntityId {
        let e = world.spawn();
        world.insert(e, Transform::new(Vec3::ZERO, Quat::IDENTITY, 1.0));
        world.insert(e, GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0));
        world.insert(
            e,
            CollisionShape::Cuboid {
                half_extents: Vec3::splat(4.0),
            },
        );
        world.insert(
            e,
            RigidBodyData {
                motion_type: MotionType::Keyframed,
                ..Default::default()
            },
        );
        if tag {
            world.insert(e, ActorBoneCollider);
        }
        e
    }

    /// #2873 — registration must file a tagged bone's collider under
    /// `ACTOR_BONE_GROUP` (membership only, filter untouched) and leave an
    /// untagged body on the default groups, so ground probes discriminate
    /// between an actor's shoulder and the packed-Havok floor proxy.
    #[test]
    fn tagged_bones_register_into_the_actor_bone_group() {
        let mut world = world();
        let bone = spawn_bone(&mut world, true);
        let prop = spawn_bone(&mut world, false);

        physics_sync_system(&world, 0.0);

        let handles = world.query::<RapierHandles>().expect("storage");
        let pw = world.resource::<PhysicsWorld>();
        let groups = |e: EntityId| {
            let h = handles.get(e).copied().expect("registered");
            pw.colliders
                .get(h.collider)
                .expect("collider")
                .collision_groups()
        };

        assert_eq!(groups(bone).memberships, crate::ACTOR_BONE_GROUP);
        assert_eq!(
            groups(bone).filter,
            rapier3d::prelude::Group::ALL,
            "the mask is query-side only — contact generation must be unchanged"
        );
        assert_eq!(
            groups(prop),
            rapier3d::prelude::InteractionGroups::all(),
            "an untagged keyframed body keeps the default groups"
        );
    }

    /// #2860 end-to-end: `GlobalTransform::scale` must survive the whole
    /// registration path, not just `collision_shape_to_parts` in isolation.
    /// No test exercised a non-unit scale through the collider boundary
    /// before, which is exactly why the drop was invisible to `cargo test`.
    #[test]
    fn placement_scale_reaches_the_registered_collider() {
        let mut world = world();
        let scaled = world.spawn();
        world.insert(scaled, Transform::new(Vec3::ZERO, Quat::IDENTITY, 2.0));
        world.insert(
            scaled,
            GlobalTransform::new(Vec3::new(100.0, 0.0, 0.0), Quat::IDENTITY, 2.0),
        );
        world.insert(
            scaled,
            CollisionShape::Cuboid {
                half_extents: Vec3::splat(10.0),
            },
        );
        world.insert(
            scaled,
            RigidBodyData {
                motion_type: MotionType::Static,
                ..Default::default()
            },
        );

        physics_sync_system(&world, 0.0);

        let handles = world.query::<RapierHandles>().expect("storage");
        let pw = world.resource::<PhysicsWorld>();
        let h = handles.get(scaled).copied().expect("registered");
        let collider = pw.colliders.get(h.collider).expect("collider");
        let half = collider
            .shape()
            .as_cuboid()
            .expect("cuboid survived registration")
            .half_extents;
        assert_eq!(
            (half.x, half.y, half.z),
            (20.0, 20.0, 20.0),
            "a 2× placement must register a 2× collider"
        );
        // The body pose still carries translation + rotation only — the scale
        // lives in the shape, which is the whole point of baking it here.
        assert_eq!(pw.bodies.get(h.body).expect("body").translation().x, 100.0);
    }

    /// #2874 — `cast_capsule_down_onto_walkable_surface` collapses "hit
    /// nothing" and "hit something too steep" into a single `None`, which is
    /// what made the census mis-attribute a ramp to a transform bug. The
    /// verdict must keep them apart.
    #[test]
    fn probe_verdict_separates_a_miss_from_a_walkability_rejection() {
        assert_eq!(
            SpawnProbeVerdict::classify(None, 0.6),
            SpawnProbeVerdict::NoHit
        );
        assert_eq!(
            SpawnProbeVerdict::classify(Some((12.0, 0.45)), 0.6),
            SpawnProbeVerdict::RejectedNonWalkable {
                surface_y: 12.0,
                normal_y: 0.45,
            },
            "a 63° ramp is a rejection, not a missing collider"
        );
        assert_eq!(
            SpawnProbeVerdict::classify(Some((12.0, 0.98)), 0.6),
            SpawnProbeVerdict::Walkable {
                surface_y: 12.0,
                normal_y: 0.98,
            }
        );
        // Legacy Havok architecture can be inward-wound, so orientation is
        // deliberately ignored — matching the walkable wrapper's own contract.
        assert_eq!(
            SpawnProbeVerdict::classify(Some((12.0, -0.98)), 0.6),
            SpawnProbeVerdict::Walkable {
                surface_y: 12.0,
                normal_y: -0.98,
            }
        );
    }
}

#[cfg(test)]
mod tick_documentation_tests {
    /// Regression for #2880. The module doc is the authoritative description
    /// of the tick, and it said "four phases" while the live sequence has
    /// five — `apply_buoyancy` runs as phase 2.5, between the kinematic push
    /// and the step. That omission matters because the phase's *position* is
    /// the correctness property: forces must be applied before the step that
    /// integrates them, so a future reorder would be checked against a doc
    /// that never mentioned the constraint.
    ///
    /// Source-inspection check (same pattern as `character::regen`'s
    /// doc-drift regressions) tying the doc to the call it describes.
    #[test]
    fn module_doc_covers_the_buoyancy_phase_and_its_ordering() {
        let src = include_str!("sync.rs");
        let doc: String = src
            .lines()
            .take_while(|line| line.starts_with("//!"))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            src.contains("crate::water::apply_buoyancy(world,"),
            "fixture precondition: the buoyancy phase is still called here"
        );
        // An *enumerated* entry, not merely the word somewhere in the prose:
        // the pre-fix doc listed 1-4 and mentioned water nowhere, and a fix
        // that only added a passing mention would leave the phase absent
        // from the list a reorder is checked against.
        let enumerated = doc
            .lines()
            .any(|line| line.starts_with("//! 2.5.") && line.to_lowercase().contains("buoyancy"));
        assert!(
            enumerated,
            "the tick doc must enumerate the 2.5 buoyancy phase in its phase \
             list — it is a real phase with its own BYRO_PROFILE bracket, \
             not an implementation detail of phase 3 (#2880)"
        );
        assert!(
            doc.contains("before the step"),
            "the doc must state WHY 2.5 precedes phase 3 — the ordering is \
             the correctness property, and a doc that only lists the phase \
             would not catch a reorder (#2880)"
        );
    }

    /// Companion for #2880. The doc claimed the `PhysicsWorld` early-return
    /// was "the loose-NIF viewer opt-out". It is not: the shipping binary
    /// inserts the resource unconditionally, so the premise hid a live code
    /// path from review. Pin the corrected wording.
    #[test]
    fn module_doc_does_not_claim_a_loose_nif_physics_opt_out() {
        let src = include_str!("sync.rs");
        let doc: String = src
            .lines()
            .take_while(|line| line.starts_with("//!"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            doc.contains("not** a loose-NIF"),
            "the early-return covers test fixtures and embedders; the doc \
             must not re-describe it as a viewer opt-out the shipping \
             binary does not have (#2880)"
        );
    }

    /// Regression for #2881. Both `BYRO_PROFILE*` probes ran
    /// `std::env::var_os` on the per-tick path while the adjacent comment
    /// claimed "zero cost when the flag is unset" — which describes the
    /// branch, not the lookup (`var_os` takes the environ lock and allocates
    /// on a hit regardless). Pin them to a once-per-process read.
    #[test]
    fn profile_flags_are_read_once_per_process_not_per_tick() {
        let src = include_str!("sync.rs");
        let system_start = src
            .find("pub fn physics_sync_system")
            .expect("the tick system is still here");
        let system_end = src[system_start..]
            .find("\n}\n")
            .expect("system body is delimited")
            + system_start;
        let system_body = &src[system_start..system_end];
        assert!(
            !system_body.contains("env::var"),
            "physics_sync_system must not probe the environment per tick — \
             hoist the flag into a OnceLock accessor like `profile_phases` \
             (#2881)"
        );
        // Both accessors must actually cache, not merely wrap the lookup.
        for accessor in ["fn profile_phases", "fn profile_fallers"] {
            let start = src.find(accessor).expect("accessor present");
            let body = &src[start..start + 400];
            assert!(
                body.contains("OnceLock"),
                "`{accessor}` must cache its lookup (#2881)"
            );
        }
    }

    /// The two accessors must agree with the environment they were built
    /// from. Cheap, but it stops a copy-paste that reads the same variable
    /// name twice from going unnoticed.
    #[test]
    fn profile_accessors_read_their_own_variables() {
        let src = include_str!("sync.rs");
        assert!(src.contains(r#"var_os("BYRO_PROFILE")"#));
        assert!(src.contains(r#"var_os("BYRO_PROFILE_FALLERS")"#));
        // Both are absent from this test process, so both must be false —
        // and, being cached, must stay stable across calls.
        assert_eq!(super::profile_phases(), super::profile_phases());
        assert_eq!(super::profile_fallers(), super::profile_fallers());
    }
}
