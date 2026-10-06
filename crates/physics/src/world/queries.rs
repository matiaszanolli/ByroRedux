//! `PhysicsWorld` queries and character motion (TD1-2026-10-05-01 / #5311).
//!
//! Split from `world.rs`: `cast_ray*`, `cast_ray_corridor`,
//! `line_of_sight_blocked`, the capsule probes, `colliders_near_xz`,
//! `static_colliders_aabb`, and `move_character`, plus the
//! `CharacterMove*` parameter/result types they own.

use rapier3d::prelude::*;

use super::{NearbyCollider, PhysicsRayHit, PhysicsWorld, solid_probe_filter};

pub struct CharacterMoveResult {
    /// Effective translation in engine world-space (Y-up). Apply this
    /// to the character body's Transform + queue as the kinematic
    /// next-translation.
    pub translation: byroredux_core::math::Vec3,
    /// Whether the character ended the step touching the ground.
    /// Read by the controller system to gate jump triggers + zero
    /// vertical velocity on landing.
    pub grounded: bool,
    /// Whether the character is currently sliding down a steep slope
    /// (slope > `max_slope_climb_deg`). Not consumed today; surfaced
    /// for future stamina / damage hooks.
    pub is_sliding_down_slope: bool,
}

/// Movement-step parameters for [`PhysicsWorld::move_character`]. Pure
/// data so the engine-side controller stays decoupled from
/// `rapier3d::control::KinematicCharacterController` field layout.
#[derive(Debug, Clone, Copy)]
pub struct CharacterMoveParams {
    /// Capsule half-height (Y-axis), excludes caps. BU.
    pub capsule_half_height: f32,
    /// Capsule radius. BU.
    pub capsule_radius: f32,
    /// Current body position in engine world-space (Y-up).
    pub position: byroredux_core::math::Vec3,
    /// Desired translation for this step (engine world-space).
    /// Caller is responsible for combining horizontal motion with
    /// gravity-integrated vertical motion into a single vector.
    pub desired_translation: byroredux_core::math::Vec3,
    /// Time-step (seconds) for ground-detection friction.
    pub dt: f32,
    /// Max climbable slope, degrees. KCC default 50°.
    pub max_slope_climb_deg: f32,
    /// Auto-step max height, BU. KCC default 32 BU (~46 cm — covers
    /// canonical Bethesda stairs).
    pub step_height: f32,
    /// Auto-step minimum platform width (tread depth). BU. Rapier only
    /// steps up when the surface above the obstacle is at least this
    /// wide. Smaller = more permissive. 8 BU handles FNV doorsteps
    /// whose treads are often 8-16 BU deep; using capsule_radius here
    /// blocks autostep on narrow thresholds.
    pub step_min_width: f32,
    /// Ground-snap distance, BU. Holds the character on terrain
    /// rolls without per-step bouncing.
    pub snap_to_ground: f32,
    /// Optional rapier collider handle to exclude from the
    /// shapecast — pass the character's own collider here so the
    /// KCC doesn't self-hit.
    pub exclude_collider: Option<rapier3d::prelude::ColliderHandle>,
    /// Optional interaction-group mask for the shapecast. `None` keeps the
    /// default (collide with everything not excluded otherwise). M42.10 —
    /// NPC locomotion passes [`actor_move_interaction_groups`] here: like
    /// the `cast_ray_down` self-hit problem (#2873), each bone is a
    /// separate body, so a single `exclude_collider` can never cover the
    /// walker's own bones — they all carry [`ACTOR_BONE_GROUP`] and the
    /// group is masked wholesale, which masks EVERY actor's bones (NPCs
    /// ghost through NPCs; #4690). Dynamics stay included in the sweep,
    /// but `move_character` applies no collision impulses to them: a
    /// walker is blocked by clutter it cannot push or step over.
    pub filter_groups: Option<rapier3d::prelude::InteractionGroups>,
    /// `KinematicCharacterController.offset` distance in BU. Sourced
    /// from `ContactConfig::kcc_offset_bu` by the controller system;
    /// surfaced as a param so `move_character` stays pure (no resource
    /// lookups on PhysicsWorld). Wider keeps the capsule from grazing
    /// TriMesh edges; narrower lets the player fit tighter clearances.
    pub kcc_offset_bu: f32,
}

impl PhysicsWorld {
    /// Rebuild the `QueryPipeline` BVH from the current `ColliderSet`.
    ///
    /// `pipeline.step()` updates the query pipeline as a side-effect of
    /// each physics tick, but newly-inserted colliders are invisible to
    /// `cast_ray` / `intersection_with_shape` / etc. until the next
    /// step runs. M28.5 character spawn needs to ray-cast the floor
    /// BEFORE the first physics tick (the spawn position depends on
    /// the result), so we call this explicitly after newcomer
    /// registration to flush the BVH.
    pub fn update_query_pipeline(&mut self) {
        self.query_pipeline.update(&self.colliders);
        self.colliders_dirty = false;
    }

    /// Defer the query-pipeline rebuild until the next physics boundary.
    pub fn mark_colliders_dirty(&mut self) {
        self.colliders_dirty = true;
    }

    /// Cast a downward ray from `origin` and return the Y-coordinate
    /// of the first solid hit (the highest solid surface below the
    /// ray's start point), if any. Used by M28.5 character spawn to
    /// place the body on the actual floor rather than at
    /// `aabb.max.y + N` which lands on the building's exterior roof
    /// — that roof has structural gaps the KCC can slip through.
    ///
    /// Ranges over fixed (static) colliders only. `max_distance` is
    /// in BU; pass the AABB height + slack.
    ///
    /// **Caller must have called [`update_query_pipeline`]** since the
    /// last collider insertion, otherwise the BVH is stale and the ray
    /// will report no hits even when colliders exist.
    ///
    /// Returns the world-space Y of the hit; the caller adds capsule
    /// `half_height + offset` to place the capsule centre above the
    /// surface.
    ///
    /// `excluded_body` must be passed whenever the origin can lie inside a
    /// body — the player capsule, or an actor's own keyframed ragdoll bones.
    /// `exclude_dynamic()` does NOT cover those: both are
    /// `KinematicPositionBased`, and with `solid = true` rapier returns an
    /// impact at `toi = 0` for a ray starting inside a shape, which always
    /// wins the closest-hit search. The parameter is deliberately mandatory
    /// (rather than a defaulted sibling method) so every call site has to
    /// decide — a silent self-hit is invisible, since the returned
    /// `origin.y` is exactly what most callers use as their fallback (#2859).
    ///
    /// A live actor's keyframed ragdoll bones need no handle here: they carry
    /// [`ACTOR_BONE_GROUP`] and are masked out wholesale by
    /// [`ground_probe_groups`] (#2873). That matters because each bone is a
    /// separate body — `excluded_body` could never cover all ~18 of them.
    pub fn cast_ray_down(
        &self,
        origin: byroredux_core::math::Vec3,
        max_distance: f32,
        excluded_body: Option<rapier3d::prelude::RigidBodyHandle>,
    ) -> Option<f32> {
        use rapier3d::prelude::*;
        let ray = Ray::new(
            point![origin.x, origin.y, origin.z],
            vector![0.0, -1.0, 0.0],
        );
        // Restrict to fixed, non-sensor geometry — we don't want to spawn the
        // player standing on a dropped barrel, nor on a non-collidable marker
        // (#3116). See `solid_probe_filter`.
        let mut filter = solid_probe_filter();
        if let Some(body) = excluded_body {
            filter = filter.exclude_rigid_body(body);
        }
        self.query_pipeline
            .cast_ray(
                &self.bodies,
                &self.colliders,
                &ray,
                max_distance,
                /* solid = */ true,
                filter,
            )
            .map(|(_handle, toi)| origin.y - toi)
    }

    /// Cast a normalized gameplay ray against solid colliders.
    ///
    /// Sensors are excluded because trigger volumes do not obstruct sight.
    /// `excluded_body` is normally the player capsule: a camera ray can begin
    /// inside that body, which would otherwise return an immediate self-hit.
    /// The query pipeline must have been refreshed after collider insertion,
    /// matching [`cast_ray_down`](Self::cast_ray_down)'s contract.
    pub fn cast_ray(
        &self,
        origin: byroredux_core::math::Vec3,
        direction: byroredux_core::math::Vec3,
        max_distance: f32,
        excluded_body: Option<rapier3d::prelude::RigidBodyHandle>,
    ) -> Option<PhysicsRayHit> {
        if max_distance <= 0.0 || !max_distance.is_finite() {
            return None;
        }
        let direction = direction.normalize_or_zero();
        if direction.length_squared() == 0.0 {
            return None;
        }

        let ray = Ray::new(
            point![origin.x, origin.y, origin.z],
            vector![direction.x, direction.y, direction.z],
        );
        let mut filter = QueryFilter::default().exclude_sensors();
        if let Some(body) = excluded_body {
            filter = filter.exclude_rigid_body(body);
        }
        self.query_pipeline
            .cast_ray(
                &self.bodies,
                &self.colliders,
                &ray,
                max_distance,
                /* solid = */ true,
                filter,
            )
            .map(|(collider, distance)| PhysicsRayHit {
                body: self.colliders.get(collider).and_then(|hit| hit.parent()),
                distance,
            })
    }

    /// #5160 — a swept melee corridor: the same query surface as
    /// [`cast_ray`] (query pipeline, sensors excluded, optional own-body
    /// exclusion), but the ray is widened to a ball of `corridor_radius`
    /// because a swing sweeps a volume, not a line.
    ///
    /// Authored actor bone colliders are small boxes — measured 16-18 BU on
    /// FNV humanoids — so a zero-width ray can thread the gaps between them
    /// even with the aim centred on the actor: observed as a guaranteed
    /// `melee swing missed` from a textbook approach pose (p2-melee-core on
    /// `GSSettlercm`, 2026-10-01, where a 1.6° pitch difference decided
    /// hit vs thread-the-gap). Damage is actor-level, so the corridor is
    /// the honest target volume; per-bone fidelity is unaffected — the
    /// corridor still resolves through the actor's own bone colliders.
    pub fn cast_ray_corridor(
        &self,
        origin: byroredux_core::math::Vec3,
        direction: byroredux_core::math::Vec3,
        max_distance: f32,
        corridor_radius: f32,
        excluded_body: Option<rapier3d::prelude::RigidBodyHandle>,
    ) -> Option<PhysicsRayHit> {
        if max_distance <= 0.0 || !max_distance.is_finite() {
            return None;
        }
        let direction = direction.normalize_or_zero();
        if direction.length_squared() == 0.0 {
            return None;
        }
        if corridor_radius <= 0.0 {
            return self.cast_ray(origin, direction, max_distance, excluded_body);
        }

        use rapier3d::parry::query::ShapeCastOptions;
        use rapier3d::prelude::*;
        let shape = Ball::new(corridor_radius);
        let pos = Isometry::translation(origin.x, origin.y, origin.z);
        let mut filter = QueryFilter::default().exclude_sensors();
        if let Some(body) = excluded_body {
            filter = filter.exclude_rigid_body(body);
        }
        self.query_pipeline
            .cast_shape(
                &self.bodies,
                &self.colliders,
                &pos,
                &Vector::new(direction.x, direction.y, direction.z),
                &shape,
                ShapeCastOptions {
                    target_distance: 0.0,
                    stop_at_penetration: false,
                    max_time_of_impact: max_distance,
                    compute_impact_geometry_on_penetration: false,
                },
                filter,
            )
            .map(|(collider, hit)| PhysicsRayHit {
                body: self.colliders.get(collider).and_then(|hit| hit.parent()),
                distance: hit.time_of_impact,
            })
    }

    /// #4414 — does solid world geometry block the straight line `from → to`?
    ///
    /// The sight test ambient faction hostility gates an attack on. Uses the
    /// same [`solid_probe_filter`] as every solid-world probe: fixed,
    /// non-sensor geometry with every actor's bones masked, so neither the
    /// two actors nor a bystander between them occlude — walls, floors and
    /// fixed props do. `excluded_body` is the player capsule, a kinematic
    /// body `exclude_dynamic` does not cover, which a ray aimed at the
    /// player would otherwise always hit. Same **caller must have called
    /// [`update_query_pipeline`](Self::update_query_pipeline)** contract as
    /// the other probes.
    pub fn line_of_sight_blocked(
        &self,
        from: byroredux_core::math::Vec3,
        to: byroredux_core::math::Vec3,
        excluded_body: Option<rapier3d::prelude::RigidBodyHandle>,
    ) -> bool {
        let delta = to - from;
        let distance = delta.length();
        if distance <= 0.0 || !distance.is_finite() {
            return false;
        }
        let direction = delta / distance;
        let ray = Ray::new(
            point![from.x, from.y, from.z],
            vector![direction.x, direction.y, direction.z],
        );
        let mut filter = solid_probe_filter();
        if let Some(body) = excluded_body {
            filter = filter.exclude_rigid_body(body);
        }
        self.query_pipeline
            .cast_ray(
                &self.bodies,
                &self.colliders,
                &ray,
                distance,
                /* solid = */ true,
                filter,
            )
            .is_some()
    }

    /// Like [`cast_ray_down`](Self::cast_ray_down), but sweeps a capsule of
    /// the given dimensions instead of a zero-width ray. A bare ray can pass
    /// clean through a gap beside a sloped or narrow piece of architecture
    /// that a real capsule of nonzero radius would still clip — #2013 traced
    /// exactly this gap: the M28.5 door-spawn nudge picks an XZ a fixed
    /// distance into the room, and a ray straight down from that single
    /// point can miss the actual walkable floor a capsule spawned there
    /// would rest on (or, conversely, clip a sloped decoration a bare ray
    /// slips past — either way the ray and the KCC's own shape disagree).
    ///
    /// Returns the world-space Y of the surface a capsule of this size would
    /// rest on (equivalent contract to `cast_ray_down`: the caller still adds
    /// `half_height + radius + offset` to place the capsule centre above it).
    ///
    /// Same **caller must have called [`update_query_pipeline`]** and
    /// fixed-bodies-only caveats as `cast_ray_down`.
    ///
    /// **No walkable-normal screen.** As of #3971 this form has no production
    /// caller left: the spawn ladder, the door-arrival ladder and `phys.census`
    /// use [`cast_capsule_down_onto_walkable_surface`] (#2193), and the
    /// character controller's per-frame ground probe moved to
    /// [`cast_capsule_down_surface_and_normal`] so it can screen the half of
    /// its answer that feeds `is_grounded` while keeping the raw hit for its
    /// anti-drift correction. A new floor probe almost certainly wants one of
    /// those two rather than this.
    ///
    /// [`cast_capsule_down_onto_walkable_surface`]: Self::cast_capsule_down_onto_walkable_surface
    /// [`cast_capsule_down_surface_and_normal`]: Self::cast_capsule_down_surface_and_normal
    pub fn cast_capsule_down(
        &self,
        origin: byroredux_core::math::Vec3,
        capsule_half_height: f32,
        capsule_radius: f32,
        max_distance: f32,
        excluded_body: Option<rapier3d::prelude::RigidBodyHandle>,
    ) -> Option<f32> {
        self.cast_capsule_down_surface_and_normal(
            origin,
            capsule_half_height,
            capsule_radius,
            max_distance,
            excluded_body,
        )
        .map(|(surface_y, _)| surface_y)
    }

    /// Capsule floor probe that rejects walls and other non-walkable hits.
    ///
    /// A vertical capsule sweep can hit nearby door frames or shell walls
    /// before its bottom reaches the floor. Those hits have a near-horizontal
    /// normal and must not be used as a character spawn surface (#2193).
    /// Normal orientation is deliberately ignored: legacy Havok architecture
    /// can be consistently inward-wound, but its geometric slope is still a
    /// valid basis for deciding whether a surface is walkable.
    pub fn cast_capsule_down_onto_walkable_surface(
        &self,
        origin: byroredux_core::math::Vec3,
        capsule_half_height: f32,
        capsule_radius: f32,
        max_distance: f32,
        min_walkable_normal_y: f32,
        excluded_body: Option<rapier3d::prelude::RigidBodyHandle>,
    ) -> Option<f32> {
        self.cast_capsule_down_surface_and_normal(
            origin,
            capsule_half_height,
            capsule_radius,
            max_distance,
            excluded_body,
        )
        .and_then(|(surface_y, normal_y)| {
            (normal_y.abs() >= min_walkable_normal_y.clamp(0.0, 1.0)).then_some(surface_y)
        })
    }

    /// The unfiltered form of [`cast_capsule_down_onto_walkable_surface`]:
    /// returns `(surface_y, normal1.y)` for the first hit, walkable or not.
    ///
    /// Public because the walkable wrapper collapses two very different
    /// outcomes into `None` — "the swept capsule hit nothing" and "it hit
    /// something whose slope failed the walkable test" — and a spawn that
    /// misses every rung needs to tell those apart. Re-running the probe
    /// through this entry point on the failure path is what lets
    /// `dump_spawn_collider_census` report *"unfiltered sweep hit y=… with
    /// normal_y=… → REJECTED as non-walkable"* instead of mis-attributing a
    /// 60° ramp to a transform-composition bug (#2874).
    ///
    /// [`cast_capsule_down_onto_walkable_surface`]: Self::cast_capsule_down_onto_walkable_surface
    pub fn cast_capsule_down_surface_and_normal(
        &self,
        origin: byroredux_core::math::Vec3,
        capsule_half_height: f32,
        capsule_radius: f32,
        max_distance: f32,
        excluded_body: Option<rapier3d::prelude::RigidBodyHandle>,
    ) -> Option<(f32, f32)> {
        use rapier3d::parry::query::ShapeCastOptions;
        use rapier3d::prelude::*;
        let shape = character_capsule(capsule_half_height, capsule_radius);
        let pos = Isometry::translation(origin.x, origin.y, origin.z);
        let mut filter = solid_probe_filter();
        if let Some(body) = excluded_body {
            filter = filter.exclude_rigid_body(body);
        }
        self.query_pipeline
            .cast_shape(
                &self.bodies,
                &self.colliders,
                &pos,
                &-Vector::y_axis(),
                &shape,
                ShapeCastOptions {
                    target_distance: 0.0,
                    stop_at_penetration: false,
                    max_time_of_impact: max_distance,
                    compute_impact_geometry_on_penetration: true,
                },
                filter,
            )
            .map(|(_handle, hit)| {
                (
                    origin.y - hit.time_of_impact - capsule_half_height - capsule_radius,
                    hit.normal1.y,
                )
            })
    }

    /// Test the final capsule placement, not just the supporting floor.
    /// Downward casts with `stop_at_penetration=false` can find floor while
    /// already inside a door. Use the same solid-world filter as floor probes
    /// (including kinematic architecture, excluding sensors and actor bones).
    /// The query pipeline must be current, as for the floor probes.
    pub fn capsule_overlaps_solid(
        &self,
        center: byroredux_core::math::Vec3,
        half_height: f32,
        radius: f32,
        excluded_body: Option<RigidBodyHandle>,
    ) -> bool {
        let shape = character_capsule(half_height, radius);
        let pos = Isometry::translation(center.x, center.y, center.z);
        let mut filter = solid_probe_filter();
        if let Some(body) = excluded_body {
            filter = filter.exclude_rigid_body(body);
        }
        self.query_pipeline
            .intersection_with_shape(&self.bodies, &self.colliders, &pos, &shape, filter)
            .is_some()
    }

    /// Diagnostic — compute the AABB of all static colliders in the
    /// world, plus the count. Returns `None` when there are no static
    /// colliders. Used by the M28.5 controller's one-shot "collider
    /// world overlaps character XZ?" sanity log.
    pub fn static_colliders_aabb(&self) -> Option<([f32; 3], [f32; 3], u32)> {
        use rapier3d::prelude::*;
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        let mut count = 0u32;
        for (_h, c) in self.colliders.iter() {
            // #3116 — a sensor sitting where the floor should be is not a
            // floor, so it must not count toward "the collision world is
            // populated". Mirrors the discrimination `NearbyCollider::is_sensor`
            // already carries (#2874).
            if c.is_sensor() {
                continue;
            }
            if let Some(parent) = c.parent() {
                if let Some(rb) = self.bodies.get(parent) {
                    if rb.body_type() == RigidBodyType::Fixed {
                        let aabb = c.compute_aabb();
                        min[0] = min[0].min(aabb.mins.x);
                        min[1] = min[1].min(aabb.mins.y);
                        min[2] = min[2].min(aabb.mins.z);
                        max[0] = max[0].max(aabb.maxs.x);
                        max[1] = max[1].max(aabb.maxs.y);
                        max[2] = max[2].max(aabb.maxs.z);
                        count += 1;
                    }
                }
            }
        }
        if count == 0 {
            None
        } else {
            Some((min, max, count))
        }
    }

    /// Diagnostic — every collider whose AABB overlaps the vertical column
    /// of half-width `radius` around `(x, z)`, whatever its body type
    /// (#2202).
    ///
    /// [`static_colliders_aabb`](Self::static_colliders_aabb) answers
    /// "is the collision world populated and does it overlap this cell?" —
    /// cell-wide bounds and a Fixed-only count. That reads healthy for a
    /// cell with 2560 fixed colliders and a hole exactly under the player's
    /// spawn, which is why it cannot discriminate between a collider that
    /// is absent, a collider that exists but is Dynamic (and so invisible
    /// to both that census and the `exclude_dynamic` spawn probe), and a
    /// collider that exists as Fixed but composed to the wrong Y.
    ///
    /// This one is deliberately unfiltered: the *point* is to see colliders
    /// the spawn probe cannot. Returned entries carry the parent body handle
    /// so the caller can resolve it back to an entity and its
    /// `PhysicsSourceForm`.
    ///
    /// Sorted by **distance from `probe_y`**, nearest first (#2875). The
    /// pre-fix ordering sorted by absolute AABB centre Y descending and took
    /// only the first N, which inverted the diagnostic: the question is "is
    /// there a floor at or below the spawn?", whose answer lives at the low
    /// end of the column, while a two-storey inn's roof beams and upper
    /// landing monopolise the high end. In the dense-interior case this
    /// census exists for, the evidence was exactly what got truncated away.
    pub fn colliders_near_xz(
        &self,
        x: f32,
        probe_y: f32,
        z: f32,
        radius: f32,
    ) -> Vec<NearbyCollider> {
        use rapier3d::prelude::*;
        let mut out = Vec::new();
        for (_h, c) in self.colliders.iter() {
            let aabb = c.compute_aabb();
            // Column overlap test — a wall whose AABB straddles the column
            // counts even if its centre is far away.
            if aabb.maxs.x < x - radius
                || aabb.mins.x > x + radius
                || aabb.maxs.z < z - radius
                || aabb.mins.z > z + radius
            {
                continue;
            }
            let parent = c.parent();
            let body_type = parent
                .and_then(|p| self.bodies.get(p))
                .map(|rb| match rb.body_type() {
                    RigidBodyType::Fixed => "Fixed",
                    RigidBodyType::Dynamic => "Dynamic",
                    RigidBodyType::KinematicPositionBased => "KinematicPos",
                    RigidBodyType::KinematicVelocityBased => "KinematicVel",
                })
                .unwrap_or("orphan");
            out.push(NearbyCollider {
                body: parent,
                body_type,
                is_sensor: c.is_sensor(),
                aabb_min: [aabb.mins.x, aabb.mins.y, aabb.mins.z],
                aabb_max: [aabb.maxs.x, aabb.maxs.y, aabb.maxs.z],
            });
        }
        // Distance from the probe height, nearest first. Ties (a floor slab
        // and a ceiling slab equidistant from the probe) break downward, so
        // the one that could actually be a floor reads first.
        let distance = |c: &NearbyCollider| {
            let centre = 0.5 * (c.aabb_min[1] + c.aabb_max[1]);
            ((centre - probe_y).abs(), centre > probe_y)
        };
        out.sort_by(|a, b| {
            let (ad, a_above) = distance(a);
            let (bd, b_above) = distance(b);
            ad.partial_cmp(&bd)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a_above.cmp(&b_above))
        });
        out
    }

    /// Drive a kinematic character body forward one step using
    /// Rapier's `KinematicCharacterController` (M28.5). Returns the
    /// effective collide-and-slide-corrected motion + grounded status.
    ///
    /// Caller is responsible for:
    ///   1. Combining horizontal WASD-driven motion with vertical
    ///      gravity-integrated motion into `params.desired_translation`.
    ///   2. Applying `result.translation` to the character body's
    ///      `Transform` (engine-side) AND
    ///      `set_next_kinematic_translation` (Rapier-side) so the
    ///      simulation + ECS stay in lockstep.
    ///   3. Resetting `vertical_velocity` to 0 on `result.grounded`
    ///      transitions and to `jump_velocity` on jump triggers.
    pub fn move_character(&self, params: CharacterMoveParams) -> CharacterMoveResult {
        use rapier3d::control::{CharacterAutostep, CharacterLength, KinematicCharacterController};
        use rapier3d::prelude::*;

        // M28.5 KCC offset — at Skyrim's 70 BU/m scale, 0.5 BU
        // was only 7 mm of skin between the capsule and any surface,
        // letting the KCC's swept cast graze TriMesh edges and tunnel
        // through tiny gaps (Whiterun Bannered Mare floor planks have
        // ~1-2 BU vertex-gaps where adjacent collision triangles meet;
        // the 0.5 BU offset wasn't enough margin). The value lives on
        // `ContactConfig::kcc_offset_bu` (default 4 BU ≈ 5.7 cm) and
        // is plumbed through `CharacterMoveParams` so a single resource
        // edit can re-tune every character.
        //
        // Min slide angle: half-way between climb limit and 90° — once
        // the slope is steeper than this, the controller starts
        // sliding the character down instead of trying to hold pose.
        let controller = KinematicCharacterController {
            up: Vector::y_axis(),
            offset: CharacterLength::Absolute(params.kcc_offset_bu.max(0.0)),
            slide: true,
            autostep: Some(CharacterAutostep {
                max_height: CharacterLength::Absolute(params.step_height.max(0.0)),
                min_width: CharacterLength::Absolute(params.step_min_width.max(0.1)),
                include_dynamic_bodies: false,
            }),
            max_slope_climb_angle: params.max_slope_climb_deg.to_radians(),
            min_slope_slide_angle: ((params.max_slope_climb_deg + 90.0) * 0.5).to_radians(),
            snap_to_ground: if params.snap_to_ground > 0.0 {
                Some(CharacterLength::Absolute(params.snap_to_ground))
            } else {
                None
            },
            ..Default::default()
        };

        let shape = character_capsule(params.capsule_half_height, params.capsule_radius);
        let pos = Isometry::translation(params.position.x, params.position.y, params.position.z);
        let desired = Vector::new(
            params.desired_translation.x,
            params.desired_translation.y,
            params.desired_translation.z,
        );

        // #3116 — sensors must be excluded here too. Rapier 0.22's
        // `KinematicCharacterController` does not add the flag for you: the
        // only mutation it makes to the caller's filter is
        // `filter.flags |= QueryFilterFlags::EXCLUDE_DYNAMIC`
        // (`control/character_controller.rs:670`), and the sweep passes that
        // same filter straight into `queries.cast_shape`. Without this, every
        // Havok layer-15 body registered as a sensor since #2549 still walls
        // off the player — for the character controller that change was a
        // no-op, which is the exact bug #2549 was filed to fix.
        let base = QueryFilter::default().exclude_sensors();
        let base = match params.filter_groups {
            Some(groups) => base.groups(groups),
            None => base,
        };
        let filter = if let Some(exclude) = params.exclude_collider {
            base.exclude_collider(exclude)
        } else {
            base
        };

        let result = controller.move_shape(
            params.dt.max(1e-6),
            &self.bodies,
            &self.colliders,
            &self.query_pipeline,
            &shape,
            &pos,
            desired,
            filter,
            |_| {},
        );

        CharacterMoveResult {
            translation: byroredux_core::math::Vec3::new(
                result.translation.x,
                result.translation.y,
                result.translation.z,
            ),
            grounded: result.grounded,
            is_sliding_down_slope: result.is_sliding_down_slope,
        }
    }
}

/// The character / ground-probe capsule, by value. #4614 — the sweep and
/// overlap queries only need `&dyn Shape`, so the stack `Capsule` replaces
/// `SharedShape::capsule_y`, whose `Arc` was one heap allocation + free per
/// call: per walking NPC per tick since M42.10, plus the player and every
/// ground probe. Also the one place the degenerate-extent floor lives, for
/// #4134's clamp to extend.
pub(super) fn character_capsule(half_height: f32, radius: f32) -> rapier3d::parry::shape::Capsule {
    rapier3d::parry::shape::Capsule::new_y(half_height.max(1e-3), radius.max(1e-3))
}
