//! `physics_sync_system` — per-tick bridge between ECS and Rapier.
//!
//! Runs after `transform_propagation_system` so that `GlobalTransform`
//! is fresh for Phase 1 spawning and Phase 2 kinematic pushes. Walks
//! four phases:
//!
//! 1. **Register** new entities: `(CollisionShape, RigidBodyData,
//!    GlobalTransform)` without `RapierHandles` → build & insert
//!    Rapier body + collider, attach `RapierHandles`.
//! 2. **Push kinematic** transforms: keyframed bodies track the ECS
//!    `GlobalTransform` via `set_next_kinematic_position`.
//! 3. **Step**: drain the fixed-timestep accumulator.
//! 4. **Pull dynamic** transforms back into ECS `Transform` (dynamic
//!    bodies only — static/keyframed are driven the other way).

use byroredux_core::ecs::components::collision::{CollisionShape, MotionType, RigidBodyData};
use byroredux_core::ecs::components::{
    FormIdComponent, GlobalTransform, PhysicsSourceForm, RenderLayer, Transform,
};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_core::form_id::FormIdPool;
use rapier3d::prelude::{ColliderBuilder, RigidBodyBuilder, RigidBodyType};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::components::RapierHandles;
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
    // push-kinematic, 3 Rapier step, 4 pull-dynamic). Silent otherwise.
    let profile = std::env::var_os("BYRO_PROFILE").is_some();
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
    // without the per-phase spam). Zero cost when the flag is unset.
    if std::env::var_os("BYRO_PROFILE_FALLERS").is_some() {
        dump_awake_fallers(world);
    }
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

    let pw = world.resource::<PhysicsWorld>();
    let awake: Vec<RigidBodyHandle> = pw.islands.active_dynamic_bodies().to_vec();
    if awake.len() < AWAKE_FALLER_DUMP_FLOOR {
        return; // not a storm yet — don't consume the one-shot
    }
    if AWAKE_FALLERS_DUMPED.swap(true, Ordering::Relaxed) {
        return; // already dumped once this process
    }

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

    let mut entries: Vec<FallerEntry> = Vec::with_capacity(awake.len());
    let (mut vy_min, mut vy_max) = (f32::INFINITY, f32::NEG_INFINITY);
    for h in &awake {
        let Some(body) = pw.bodies.get(*h) else { continue };
        let vy = body.linvel().y;
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
            y: body.translation().y,
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
            f.form.map(|id| format!("{id:#08X}")).unwrap_or_else(|| "?".into()),
            f.y,
            f.vy,
        );
    }
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

    let handles_q = world.query::<RapierHandles>();

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

    for (entity, shape) in shape_q.iter() {
        if let Some(ref hq) = handles_q {
            if hq.contains(entity) {
                continue;
            }
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
        let parts = collision_shape_to_parts(&n.shape, &cfg);
        if parts.is_empty() {
            continue;
        }

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
        for (iso, shape) in parts {
            let collider = ColliderBuilder::new(shape)
                .position(iso)
                .friction(n.body_data.friction)
                .restitution(n.body_data.restitution)
                .mass(part_mass)
                .contact_skin(contact_skin)
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

    // Refresh the query-pipeline BVH so the new colliders are visible to
    // raycasts / contacts THIS frame — WITHOUT force-waking the simulation.
    // Pre-fix this called `pw.wake()`, which force-stepped the whole sim on
    // every registration; combined with dynamic newcomers spawning awake,
    // that stepped thousands of free-falling bodies and stalled the frame for
    // seconds (the exterior-freeze fix above). Dynamic newcomers now spawn
    // asleep, so they need no settling step — only the BVH must refresh.
    // Kinematic newcomers' motion still wakes the sim via `push_kinematic`.
    if !registered.is_empty() {
        pw.update_query_pipeline();
    }

    drop(pw);

    // Attach RapierHandles components. The ECS Query API doesn't expose
    // per-entity insert through a QueryWrite on a previously-unknown
    // entity, so we need mutable access to the World. But systems only
    // get `&World`. We work around this by pre-registering storage and
    // going through the QueryWrite::insert path, which IS available.
    if !registered.is_empty() {
        // Ensure storage exists for QueryWrite to succeed.
        let mut handles_q = match world.query_mut::<RapierHandles>() {
            Some(q) => q,
            None => {
                log::error!(
                    "RapierHandles storage missing — call World::register::<RapierHandles>() \
                     during setup before running physics_sync_system"
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

    let mut pw = world.resource_mut::<PhysicsWorld>();
    let mut pushed = false;
    for (entity, handles) in handles_q.iter() {
        let Some(body_data) = body_q.get(entity) else {
            continue;
        };
        // Only `Keyframed` bodies (doors, platforms, scripted props)
        // track their ECS `GlobalTransform` automatically. Character
        // kinematic bodies are driven explicitly by the character
        // controller system via `set_kinematic_translation`; pushing
        // the ECS Transform here would race with the controller's
        // KCC-corrected pose write.
        if body_data.motion_type != MotionType::Keyframed {
            continue;
        }
        let Some(g) = global_q.get(entity) else {
            continue;
        };
        if let Some(body) = pw.bodies.get_mut(handles.body) {
            let target = iso_from_trs(g.translation, g.rotation);
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

    // Build a list of (entity, new local translation, new local rotation)
    // before taking the Transform write lock.
    let mut updates: Vec<(EntityId, glam::Vec3, glam::Quat)> = Vec::new();
    {
        let pw = world.resource::<PhysicsWorld>();
        for (entity, handles) in handles_q.iter() {
            let Some(body_data) = body_q.get(entity) else {
                continue;
            };
            if body_data.motion_type != MotionType::Dynamic {
                continue;
            }
            let Some(body) = pw.bodies.get(handles.body) else {
                continue;
            };
            let iso = *body.position();
            let translation = vec3_from_translation(iso.translation);
            let rotation = quat_from_na(iso.rotation);
            updates.push((entity, translation, rotation));
        }
    }

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
        assert_eq!(worst[0].vy, -700.0, "most-negative vy (fastest faller) first");
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
