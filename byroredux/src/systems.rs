//! ECS systems for the application: camera, animation, transform propagation, etc.

use byroredux_core::animation::{
    advance_stack, advance_time, sample_blended_transform, sample_bool_channel,
    sample_color_channel, sample_float_channel, sample_rotation, sample_scale, sample_translation,
    split_root_motion, AnimationClipRegistry, AnimationPlayer, AnimationStack, FloatTarget,
    RootMotionDelta,
};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{
    ActiveCamera, AnimatedAlpha, AnimatedColor, AnimatedVisibility, Billboard, BillboardMode,
    Children, DebugStats, DeltaTime, EngineConfig, GlobalTransform, LocalBound, Name, Parent,
    TotalTime, Transform, World, WorldBound,
};
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::StringPool;

use crate::anim_convert::build_subtree_name_map;
use crate::components::{InputState, NameIndex, Spinning, SubtreeCache};

/// Fly camera system: WASD + mouse look. Updates the active camera's Transform.
pub(crate) fn fly_camera_system(world: &World, dt: f32) {
    let Some(active) = world.try_resource::<ActiveCamera>() else {
        return;
    };
    let cam_entity = active.0;
    drop(active);

    let Some(input) = world.try_resource::<InputState>() else {
        return;
    };
    if !input.mouse_captured {
        return;
    }

    let speed = input.move_speed * dt;
    let yaw = input.yaw;
    let pitch = input.pitch;

    // Build movement vector from held keys.
    let mut move_dir = Vec3::ZERO;
    if input.keys_held.contains(&winit::keyboard::KeyCode::KeyW) {
        move_dir.z += 1.0;
    }
    if input.keys_held.contains(&winit::keyboard::KeyCode::KeyS) {
        move_dir.z -= 1.0;
    }
    if input.keys_held.contains(&winit::keyboard::KeyCode::KeyA) {
        move_dir.x -= 1.0;
    }
    if input.keys_held.contains(&winit::keyboard::KeyCode::KeyD) {
        move_dir.x += 1.0;
    }
    if input.keys_held.contains(&winit::keyboard::KeyCode::Space) {
        move_dir.y += 1.0;
    }
    if input
        .keys_held
        .contains(&winit::keyboard::KeyCode::ShiftLeft)
    {
        move_dir.y -= 1.0;
    }

    // Speed boost with Ctrl.
    let boost = if input
        .keys_held
        .contains(&winit::keyboard::KeyCode::ControlLeft)
    {
        3.0
    } else {
        1.0
    };
    drop(input);

    // Build rotation from yaw/pitch.
    let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);

    // Compute desired world-space move vector (yaw-only, so Y stays level).
    let move_world = if move_dir != Vec3::ZERO {
        let dir = move_dir.normalize();
        let forward = Quat::from_rotation_y(yaw) * -Vec3::Z;
        let right = Quat::from_rotation_y(yaw) * Vec3::X;
        let up = Vec3::Y;
        (forward * dir.z + right * dir.x + up * dir.y) * boost
    } else {
        Vec3::ZERO
    };

    // Branch: physics-driven (camera has RapierHandles) vs free-fly fallback.
    let has_physics = world
        .query::<byroredux_physics::RapierHandles>()
        .map(|q| q.contains(cam_entity))
        .unwrap_or(false);

    if has_physics {
        // Always update rotation on the Transform — Rapier Phase 4 only
        // writes translation/rotation for dynamic bodies, but we want the
        // rotation to reflect input instantly.
        if let Some(mut tq) = world.query_mut::<Transform>() {
            if let Some(transform) = tq.get_mut(cam_entity) {
                transform.rotation = rotation;
            }
        }
        // Write linear velocity into the Rapier body. `speed` from
        // InputState is already per-frame — divide out dt to get per-second.
        let velocity_per_sec = if dt > 0.0 { speed / dt } else { 0.0 };
        let v = move_world * velocity_per_sec;
        byroredux_physics::set_linear_velocity(world, cam_entity, v);
    } else if let Some(mut tq) = world.query_mut::<Transform>() {
        if let Some(transform) = tq.get_mut(cam_entity) {
            transform.rotation = rotation;
            if move_world != Vec3::ZERO {
                transform.translation += move_world * speed;
            }
        }
    }
}

/// Animation system: advances AnimationPlayer time and applies interpolated
/// transforms to named entities that match the clip's channel names.
pub(crate) fn animation_system(world: &World, dt: f32) {
    // Read the clip registry (immutable).
    let Some(registry) = world.try_resource::<AnimationClipRegistry>() else {
        return;
    };
    if registry.is_empty() {
        return;
    }

    // Persisted subtree name maps — survives across frames, only cleared when
    // Name component count changes. Eliminates ~1500 HashMap insertions/frame
    // for typical animated scenes. #278.
    {
        let current_name_count = world.query::<Name>().map(|q| q.len()).unwrap_or(0);
        let needs_clear = world
            .try_resource::<SubtreeCache>()
            .map(|c| c.generation != current_name_count)
            .unwrap_or(false);
        if needs_clear {
            let mut cache = world.resource_mut::<SubtreeCache>();
            cache.map.clear();
            cache.generation = current_name_count;
        }
    }

    // Rebuild name→entity index only when the count of Name components
    // has changed. `QueryRead::len()` is O(1) (reads the storage's
    // element count) so the check itself is cheap. See #249 — before
    // this fix the generation tracked `world.next_entity_id()` and
    // every entity spawn (even unnamed ones) forced a full rebuild.
    {
        let current_name_count = world.query::<Name>().map(|q| q.len()).unwrap_or(0);
        let needs_rebuild = world
            .try_resource::<NameIndex>()
            .map(|idx| idx.generation != current_name_count)
            .unwrap_or(true);
        if needs_rebuild {
            let name_query = match world.query::<Name>() {
                Some(q) => q,
                None => return,
            };
            let mut new_map = std::collections::HashMap::new();
            for (entity, name_comp) in name_query.iter() {
                new_map.insert(name_comp.0, entity);
            }
            drop(name_query);
            let mut idx = world.resource_mut::<NameIndex>();
            idx.map = new_map;
            idx.generation = current_name_count;
        }
    }

    let Some(pool) = world.try_resource::<StringPool>() else {
        return;
    };
    let name_index = world.try_resource::<NameIndex>().unwrap();

    // Iterate all animation players and apply.
    let Some(player_query) = world.query_mut::<AnimationPlayer>() else {
        return;
    };
    let entities_with_players: Vec<_> = player_query.iter().map(|(e, _)| e).collect();
    drop(player_query);

    // Phase 1: Advance all players and collect playback state.
    // Single lock acquisition for AnimationPlayer, held for the entire batch.
    struct PlaybackState {
        entity: EntityId,
        clip_handle: u32,
        root_entity: Option<EntityId>,
        current_time: f32,
    }
    let mut playback_states = Vec::with_capacity(entities_with_players.len());
    {
        let mut player_query = world.query_mut::<AnimationPlayer>().unwrap();
        for &entity in &entities_with_players {
            let player = player_query.get_mut(entity).unwrap();
            let clip_handle = player.clip_handle;
            let root_entity_opt = player.root_entity;
            let Some(clip) = registry.get(clip_handle) else {
                continue;
            };
            advance_time(player, clip, dt);
            playback_states.push(PlaybackState {
                entity,
                clip_handle,
                root_entity: root_entity_opt,
                current_time: player.local_time,
            });
        }
    } // AnimationPlayer lock released here

    // Phase 2: Apply channels using pre-computed playback state.
    for ps in &playback_states {
        let entity = ps.entity;
        let Some(clip) = registry.get(ps.clip_handle) else {
            continue;
        };
        let current_time = ps.current_time;

        // Scoped name lookup — persisted across frames (#278).
        if let Some(root) = ps.root_entity {
            let needs_build = world
                .try_resource::<SubtreeCache>()
                .map(|c| !c.map.contains_key(&root))
                .unwrap_or(false);
            if needs_build {
                let map = build_subtree_name_map(world, root);
                let mut cache = world.resource_mut::<SubtreeCache>();
                cache.map.insert(root, map);
            }
        }
        let subtree_ref = world.try_resource::<SubtreeCache>();
        let scoped_map = ps.root_entity.and_then(|root| {
            subtree_ref.as_ref().and_then(|c| c.map.get(&root))
        });
        let resolve_entity = |channel_name: &str| -> Option<EntityId> {
            let sym = pool.get(channel_name)?;
            if let Some(scoped) = scoped_map {
                scoped.get(&sym).copied()
            } else {
                name_index.map.get(&sym).copied()
            }
        };

        // Apply transform channels.
        let is_accum_root = |name: &str| -> bool { clip.accum_root_name.as_deref() == Some(name) };
        {
            let mut transform_query = world.query_mut::<Transform>().unwrap();
            let mut root_motion = Vec3::ZERO;
            for (channel_name, channel) in &clip.channels {
                let Some(target_entity) = resolve_entity(channel_name) else {
                    continue;
                };
                let Some(transform) = transform_query.get_mut(target_entity) else {
                    continue;
                };
                if let Some(pos) = sample_translation(channel, current_time) {
                    if is_accum_root(channel_name) {
                        // Split: vertical → animation, horizontal → root motion delta.
                        let (anim_pos, delta) = split_root_motion(pos);
                        transform.translation = anim_pos;
                        root_motion += delta;
                    } else {
                        transform.translation = pos;
                    }
                }
                if let Some(rot) = sample_rotation(channel, current_time) {
                    transform.rotation = rot;
                }
                if let Some(scale) = sample_scale(channel, current_time) {
                    transform.scale = scale;
                }
            }
            drop(transform_query);

            // Write root motion delta to the player entity.
            if root_motion != Vec3::ZERO {
                if let Some(mut rmq) = world.query_mut::<RootMotionDelta>() {
                    if let Some(rm) = rmq.get_mut(entity) {
                        rm.0 = root_motion;
                    }
                }
            }
        }

        // Apply float channels (alpha, UV params, shader floats).
        // Hold lock for entire channel batch instead of per-channel.
        if !clip.float_channels.is_empty() {
            if let Some(mut aq) = world.query_mut::<AnimatedAlpha>() {
                for (channel_name, channel) in &clip.float_channels {
                    let Some(target_entity) = resolve_entity(channel_name) else {
                        continue;
                    };
                    let value = sample_float_channel(channel, current_time);
                    if channel.target == FloatTarget::Alpha {
                        if let Some(a) = aq.get_mut(target_entity) {
                            a.0 = value;
                        }
                    }
                }
            }
        }

        // Apply color channels — single lock for entire batch.
        if !clip.color_channels.is_empty() {
            if let Some(mut cq) = world.query_mut::<AnimatedColor>() {
                for (channel_name, channel) in &clip.color_channels {
                    let Some(target_entity) = resolve_entity(channel_name) else {
                        continue;
                    };
                    let value = sample_color_channel(channel, current_time);
                    if let Some(c) = cq.get_mut(target_entity) {
                        c.0 = value;
                    }
                }
            }
        }

        // Apply bool (visibility) channels — single lock for entire batch.
        if !clip.bool_channels.is_empty() {
            if let Some(mut vq) = world.query_mut::<AnimatedVisibility>() {
                for (channel_name, channel) in &clip.bool_channels {
                    let Some(target_entity) = resolve_entity(channel_name) else {
                        continue;
                    };
                    let value = sample_bool_channel(channel, current_time);
                    if let Some(v) = vq.get_mut(target_entity) {
                        v.0 = value;
                    }
                }
            }
        }
    }

    // ── AnimationStack processing (multi-layer blending) ──────────────
    let Some(stack_query) = world.query_mut::<AnimationStack>() else {
        return;
    };
    let stack_entities: Vec<_> = stack_query.iter().map(|(e, _)| e).collect();
    drop(stack_query);

    // Scratch buffers reused across entities to avoid per-tick heap
    // allocations (#251, #252). Cleared at the start of each iteration.
    let mut channel_names_scratch: Vec<&str> = Vec::new();
    let mut updates_scratch: Vec<(&str, EntityId, Vec3, Quat, f32)> = Vec::new();

    for entity in stack_entities {
        // Advance all layers.
        {
            let mut sq = world.query_mut::<AnimationStack>().unwrap();
            let stack = sq.get_mut(entity).unwrap();
            advance_stack(stack, &registry, dt);
        }

        // Sample blended transforms for each channel name.
        let sq = world.query::<AnimationStack>().unwrap();
        let stack = sq.get(entity).unwrap();

        // Scoped name lookup for stacks — persisted across frames (#278).
        if let Some(root) = stack.root_entity {
            let needs_build = world
                .try_resource::<SubtreeCache>()
                .map(|c| !c.map.contains_key(&root))
                .unwrap_or(false);
            if needs_build {
                let map = build_subtree_name_map(world, root);
                let mut cache = world.resource_mut::<SubtreeCache>();
                cache.map.insert(root, map);
            }
        }
        let subtree_ref2 = world.try_resource::<SubtreeCache>();
        let stack_scoped_map = stack.root_entity.and_then(|root| {
            subtree_ref2.as_ref().and_then(|c| c.map.get(&root))
        });
        let stack_resolve = |channel_name: &str| -> Option<EntityId> {
            let sym = pool.get(channel_name)?;
            if let Some(scoped) = stack_scoped_map {
                scoped.get(&sym).copied()
            } else {
                name_index.map.get(&sym).copied()
            }
        };

        // Collect all channel names across all active layers.
        // Reuse scratch buffer to avoid per-entity heap allocation (#251).
        channel_names_scratch.clear();
        for layer in &stack.layers {
            if let Some(clip) = registry.get(layer.clip_handle) {
                for name in clip.channels.keys() {
                    channel_names_scratch.push(name.as_str());
                }
            }
        }
        channel_names_scratch.sort_unstable();
        channel_names_scratch.dedup();

        // Batch pose updates to decouple sampling from transform writes (#252).
        updates_scratch.clear();
        for channel_name in &channel_names_scratch {
            let Some(target_entity) = stack_resolve(channel_name) else {
                continue;
            };
            if let Some((pos, rot, scale)) =
                sample_blended_transform(stack, &registry, channel_name)
            {
                updates_scratch.push((channel_name, target_entity, pos, rot, scale));
            }
        }
        drop(sq);

        // Determine the accum root name from the highest-weight active
        // layer (for root motion splitting). Borrows from the registry
        // (not the stack query) so the &str outlives the query. #279 D6-04.
        let accum_root: Option<&str> = {
            let sq = world.query::<AnimationStack>().unwrap();
            let stack = sq.get(entity).unwrap();
            let mut best: Option<(&str, f32)> = None;
            for layer in &stack.layers {
                let ew = layer.effective_weight();
                if ew < 0.001 { continue; }
                if let Some(clip) = registry.get(layer.clip_handle) {
                    if let Some(ref name) = clip.accum_root_name {
                        if best.map_or(true, |(_, bw)| ew > bw) {
                            best = Some((name.as_str(), ew));
                        }
                    }
                }
            }
            best.map(|(n, _)| n)
        };

        // Apply blended transforms, with root motion splitting (AR-02).
        let mut tq = world.query_mut::<Transform>().unwrap();
        let mut root_motion = Vec3::ZERO;
        for &(name, target, pos, rot, scale) in &updates_scratch {
            if let Some(transform) = tq.get_mut(target) {
                let is_accum = accum_root == Some(name);
                if is_accum {
                    let (anim_pos, delta) = split_root_motion(pos);
                    transform.translation = anim_pos;
                    root_motion += delta;
                } else {
                    transform.translation = pos;
                }
                transform.rotation = rot;
                transform.scale = scale;
            }
        }
        drop(tq);

        // Write root motion delta.
        if root_motion != Vec3::ZERO {
            if let Some(mut rmq) = world.query_mut::<RootMotionDelta>() {
                if let Some(rm) = rmq.get_mut(entity) {
                    rm.0 = root_motion;
                }
            }
        }

        // AR-01: Apply non-transform channels from the highest-weight
        // active layer. Float/color/bool channels don't blend naturally
        // (alpha is binary-ish, visibility is boolean), so we take the
        // dominant layer's sampled value rather than attempting a weighted
        // average that would produce meaningless intermediate states.
        //
        // Clone the channel vecs from the dominant clip so we can drop
        // the AnimationStack read lock before acquiring write locks on
        // AnimatedAlpha / AnimatedColor / AnimatedVisibility.
        let dominant_channels = {
            let sq = world.query::<AnimationStack>().unwrap();
            let stack = sq.get(entity).unwrap();
            let dominant = stack
                .layers
                .iter()
                .filter(|l| l.effective_weight() >= 0.001)
                .max_by(|a, b| {
                    a.effective_weight()
                        .partial_cmp(&b.effective_weight())
                        .unwrap()
                });
            dominant.and_then(|layer| {
                let clip = registry.get(layer.clip_handle)?;
                Some((
                    layer.local_time,
                    clip.float_channels.clone(),
                    clip.color_channels.clone(),
                    clip.bool_channels.clone(),
                ))
            })
        };

        if let Some((time, float_ch, color_ch, bool_ch)) = dominant_channels {
            if !float_ch.is_empty() {
                if let Some(mut aq) = world.query_mut::<AnimatedAlpha>() {
                    for (channel_name, channel) in &float_ch {
                        let Some(target_entity) = stack_resolve(channel_name) else {
                            continue;
                        };
                        let value = sample_float_channel(channel, time);
                        if channel.target == FloatTarget::Alpha {
                            if let Some(a) = aq.get_mut(target_entity) {
                                a.0 = value;
                            }
                        }
                    }
                }
            }
            if !color_ch.is_empty() {
                if let Some(mut cq) = world.query_mut::<AnimatedColor>() {
                    for (channel_name, channel) in &color_ch {
                        let Some(target_entity) = stack_resolve(channel_name) else {
                            continue;
                        };
                        let value = sample_color_channel(channel, time);
                        if let Some(c) = cq.get_mut(target_entity) {
                            c.0 = value;
                        }
                    }
                }
            }
            if !bool_ch.is_empty() {
                if let Some(mut vq) = world.query_mut::<AnimatedVisibility>() {
                    for (channel_name, channel) in &bool_ch {
                        let Some(target_entity) = stack_resolve(channel_name) else {
                            continue;
                        };
                        let value = sample_bool_channel(channel, time);
                        if let Some(v) = vq.get_mut(target_entity) {
                            v.0 = value;
                        }
                    }
                }
            }
        }
    }
}

// `make_transform_propagation_system` has moved to
// `byroredux_core::ecs::systems` so every downstream crate gets the same
// `NiNode::UpdateDownwardPass` equivalent without copy-pasting. Re-export
// it here under the existing name so call sites in this binary don't need
// to change. See issue #81.
pub(crate) use byroredux_core::ecs::make_transform_propagation_system;

/// Orients `Billboard` entities so their forward axis faces the active camera.
///
/// Runs after `transform_propagation_system`: it reads each billboard's
/// `GlobalTransform` translation, computes a fresh rotation toward the camera
/// position, and writes that rotation back into `GlobalTransform`. The local
/// `Transform` is left alone — child geometry of a billboard inherits the
/// updated world orientation via the composed parent chain next frame, and
/// the renderer reads `GlobalTransform` directly for its model matrix.
///
/// Mirrors Gamebryo's `NiBillboardNode::UpdateWorldBound`: the node's local
/// rotation is discarded and the world matrix is rebuilt with a camera-facing
/// basis. See issue #225.
pub(crate) fn billboard_system(world: &World, _dt: f32) {
    // Active camera lookup (position + forward).
    let Some(active) = world.try_resource::<ActiveCamera>() else {
        return;
    };
    let cam_entity = active.0;
    drop(active);

    let Some(cam_gq) = world.query::<GlobalTransform>() else {
        return;
    };
    let Some(cam_global) = cam_gq.get(cam_entity).copied() else {
        return;
    };
    drop(cam_gq);

    let cam_pos = cam_global.translation;
    // Camera forward = rotation * -Z (see Camera::view_matrix).
    let cam_forward = cam_global.rotation * -Vec3::Z;

    let Some(bq) = world.query::<Billboard>() else {
        return;
    };
    let Some(mut gq) = world.query_mut::<GlobalTransform>() else {
        return;
    };

    for (entity, billboard) in bq.iter() {
        let Some(global) = gq.get_mut(entity) else {
            continue;
        };

        let new_rot = compute_billboard_rotation(billboard.mode, global.translation, cam_pos, cam_forward);
        global.rotation = new_rot;
    }
}

/// Compute a world-space rotation for a billboard.
///
/// `ALWAYS_FACE_CENTER` / `RIGID_FACE_CENTER` point the billboard's forward
/// axis at the camera position (per-billboard look-at). `ALWAYS_FACE_CAMERA`
/// / `RIGID_FACE_CAMERA` use the camera's forward direction for every
/// billboard (parallel planes — cheaper, no per-billboard yaw changes when
/// walking sideways past a sprite). Up-locked modes keep world Y fixed and
/// only rotate around it.
fn compute_billboard_rotation(
    mode: BillboardMode,
    billboard_pos: Vec3,
    cam_pos: Vec3,
    cam_forward: Vec3,
) -> Quat {
    // Direction the billboard needs to LOOK toward (in world space).
    // "Face camera" rules want the billboard to look at the camera, so its
    // local -Z (forward) should point toward `cam_pos` (or along the
    // camera's forward plane).
    let look_dir = match mode {
        BillboardMode::AlwaysFaceCamera
        | BillboardMode::RigidFaceCamera
        | BillboardMode::AlwaysFaceCenter
        | BillboardMode::RigidFaceCenter => {
            let to_cam = cam_pos - billboard_pos;
            if to_cam.length_squared() < 1.0e-6 {
                // Billboard at camera origin — fall back to camera forward.
                -cam_forward
            } else {
                to_cam.normalize()
            }
        }
        BillboardMode::RotateAboutUp | BillboardMode::RotateAboutUp2 => {
            // Rotate only around world Y. Project the to-camera vector onto
            // the XZ plane, normalize, and use it as the horizontal look
            // direction.
            let mut to_cam = cam_pos - billboard_pos;
            to_cam.y = 0.0;
            if to_cam.length_squared() < 1.0e-6 {
                Vec3::Z
            } else {
                to_cam.normalize()
            }
        }
        BillboardMode::BsRotateAboutUp => {
            // Rotate only around the billboard's local Z axis (stays in
            // its local X-Y plane). We don't have the local frame here,
            // so fall back to the world-up lock — visually identical for
            // grass/foliage where BsRotateAboutUp is typically used.
            let mut to_cam = cam_pos - billboard_pos;
            to_cam.y = 0.0;
            if to_cam.length_squared() < 1.0e-6 {
                Vec3::Z
            } else {
                to_cam.normalize()
            }
        }
    };

    // Build a look-at rotation: forward = look_dir, up = world Y.
    // `Quat::from_rotation_arc(-Z, look_dir)` handles the short-path rotation
    // and keeps roll stable when up is parallel to look_dir.
    let from = -Vec3::Z;
    if (look_dir - from).length_squared() < 1.0e-6 {
        return Quat::IDENTITY;
    }
    Quat::from_rotation_arc(from, look_dir)
}

/// Compute each entity's world-space `WorldBound`.
///
/// Two passes:
///
/// 1. **Leaf bounds** — for every entity with a `LocalBound` (set at import
///    time from NIF `NiBound`), compose it with `GlobalTransform` to
///    produce a world-space sphere. The center is rotated and translated
///    by the entity's world transform; the radius is scaled uniformly
///    by the world scale.
///
/// 2. **Parent bounds** — for every entity that has `Children` but no
///    `LocalBound` (i.e. pure scene-graph nodes), fold the children's
///    `WorldBound`s into a single enclosing sphere via
///    [`WorldBound::merge`]. Runs bottom-up through post-order traversal
///    so each parent sees its descendants' final bounds. Walks the
///    hierarchy reusing the same `queue` vec so we don't allocate per
///    frame; the initial root set is also reused across frames.
///
/// Runs after `transform_propagation_system` and `billboard_system`
/// (scheduled as an exclusive PostUpdate step) so both leaf transforms
/// and billboard overrides are final. See issue #217.
pub(crate) fn make_world_bound_propagation_system() -> impl FnMut(&World, f32) + Send + Sync {
    let mut roots: Vec<EntityId> = Vec::new();
    let mut post_order: Vec<EntityId> = Vec::new();
    let mut stack: Vec<(EntityId, bool)> = Vec::new();

    move |world: &World, _dt: f32| {
        // ── Pass 1: leaf bounds from LocalBound + GlobalTransform ──────
        {
            let Some(lb_q) = world.query::<LocalBound>() else {
                return;
            };
            let Some(g_q) = world.query::<GlobalTransform>() else {
                return;
            };
            let Some(mut wb_q) = world.query_mut::<WorldBound>() else {
                return;
            };
            for (entity, local) in lb_q.iter() {
                let Some(global) = g_q.get(entity) else {
                    continue;
                };
                let world_center =
                    global.translation + global.rotation * (local.center * global.scale);
                let world_radius = local.radius * global.scale;
                if let Some(wb) = wb_q.get_mut(entity) {
                    *wb = WorldBound::new(world_center, world_radius);
                }
            }
        }

        // ── Pass 2: parent bounds as unions of children ────────────────
        //
        // Walk the hierarchy from each root entity (one without a Parent)
        // and record a post-order list. Then iterate that list in order —
        // by the time we process a parent, every child has already had its
        // bound assigned. Entities that already have a LocalBound are
        // leaves in this sense (their bound comes from pass 1) and are
        // skipped here.
        roots.clear();
        post_order.clear();
        stack.clear();

        {
            let Some(tq) = world.query::<GlobalTransform>() else {
                return;
            };
            let parent_q = world.query::<Parent>();
            for (entity, _) in tq.iter() {
                let is_root = parent_q
                    .as_ref()
                    .map(|pq| pq.get(entity).is_none())
                    .unwrap_or(true);
                if is_root {
                    roots.push(entity);
                }
            }
        }

        let children_q = world.query::<Children>();
        for &root in &roots {
            stack.push((root, false));
            while let Some((entity, visited)) = stack.pop() {
                if visited {
                    post_order.push(entity);
                    continue;
                }
                stack.push((entity, true));
                if let Some(ref cq) = children_q {
                    if let Some(children) = cq.get(entity) {
                        for &child in &children.0 {
                            stack.push((child, false));
                        }
                    }
                }
            }
        }
        drop(children_q);

        // Fold children into parents. Must be post-order — children first.
        let local_q = world.query::<LocalBound>();
        let children_q = world.query::<Children>();
        let Some(mut wb_q) = world.query_mut::<WorldBound>() else {
            return;
        };

        for &entity in &post_order {
            // Leaves (entities with a LocalBound) already have their bound
            // from pass 1. We still need parents above them to fold them in,
            // so skip only the write step here.
            if local_q
                .as_ref()
                .map(|q| q.get(entity).is_some())
                .unwrap_or(false)
            {
                continue;
            }

            // Collect child bounds.
            let Some(ref cq) = children_q else {
                continue;
            };
            let Some(children) = cq.get(entity) else {
                continue;
            };
            let mut merged = WorldBound::ZERO;
            for &child in &children.0 {
                // Read the child's bound via the mutable query — the
                // storage allows a copy-out even though we hold `wb_q`
                // mutably, because we're not aliasing across iterations.
                if let Some(child_bound) = wb_q.get_mut(child).map(|b| *b) {
                    merged = merged.merge(&child_bound);
                }
            }
            if let Some(wb) = wb_q.get_mut(entity) {
                *wb = merged;
            }
        }
    }
}

/// Rotates only entities marked with the Spinning component.
pub(crate) fn spin_system(world: &World, dt: f32) {
    if let Some((sq, mut tq)) = world.query_2_mut::<Spinning, Transform>() {
        for (entity, _) in sq.iter() {
            if let Some(transform) = tq.get_mut(entity) {
                let rotation = Quat::from_rotation_y(dt * 1.0) * Quat::from_rotation_x(dt * 0.3);
                transform.rotation = rotation * transform.rotation;
            }
        }
    }
}

/// Logs engine stats once per second using DebugStats.
pub(crate) fn log_stats_system(world: &World, _dt: f32) {
    let config = world.resource::<EngineConfig>();
    if !config.debug_logging {
        return;
    }

    let total = world.resource::<TotalTime>().0;
    let dt = world.resource::<DeltaTime>().0;
    let prev = total - dt;

    if prev < 0.0 || total.floor() != prev.floor() {
        let stats = world.resource::<DebugStats>();
        log::info!(
            target: "engine::stats",
            "fps={:.0} avg={:.0} dt={:.2}ms entities={} meshes={} textures={} draws={}",
            stats.fps, stats.avg_fps(), stats.frame_time_ms,
            stats.entity_count, stats.mesh_count, stats.texture_count, stats.draw_call_count,
        );
    }
}

#[cfg(test)]
mod bound_propagation_tests {
    //! Regression tests for `make_world_bound_propagation_system` — issue #217.
    //! These cover leaf derivation, parent merging, and the scale path.

    use super::*;
    use byroredux_core::ecs::{Children, GlobalTransform, LocalBound, Parent, WorldBound};
    use byroredux_core::math::{Quat, Vec3};
    use byroredux_core::ecs::World;

    /// Spawn an entity with a LocalBound + GlobalTransform + empty WorldBound.
    fn spawn_leaf(
        world: &mut World,
        translation: Vec3,
        scale: f32,
        local_center: Vec3,
        local_radius: f32,
    ) -> byroredux_core::ecs::storage::EntityId {
        let e = world.spawn();
        world.insert(
            e,
            GlobalTransform::new(translation, Quat::IDENTITY, scale),
        );
        world.insert(e, LocalBound::new(local_center, local_radius));
        world.insert(e, WorldBound::ZERO);
        e
    }

    #[test]
    fn leaf_bound_composes_local_with_global_transform() {
        let mut world = World::new();
        let e = spawn_leaf(
            &mut world,
            Vec3::new(10.0, 0.0, 0.0),
            1.0,
            Vec3::ZERO,
            2.0,
        );

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);

        let wb_q = world.query::<WorldBound>().unwrap();
        let wb = wb_q.get(e).unwrap();
        assert!((wb.center - Vec3::new(10.0, 0.0, 0.0)).length() < 1e-5);
        assert!((wb.radius - 2.0).abs() < 1e-5);
    }

    #[test]
    fn leaf_bound_scale_multiplies_radius() {
        let mut world = World::new();
        let e = spawn_leaf(&mut world, Vec3::ZERO, 3.0, Vec3::ZERO, 1.0);

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);

        let wb_q = world.query::<WorldBound>().unwrap();
        let wb = wb_q.get(e).unwrap();
        assert!((wb.radius - 3.0).abs() < 1e-5);
    }

    #[test]
    fn leaf_bound_nonzero_local_center_is_offset() {
        let mut world = World::new();
        // Mesh sits at world origin, scale 2, but its local sphere is
        // centered at (1, 0, 0) local. World center should be (2, 0, 0).
        let e = spawn_leaf(
            &mut world,
            Vec3::ZERO,
            2.0,
            Vec3::new(1.0, 0.0, 0.0),
            0.5,
        );

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);

        let wb_q = world.query::<WorldBound>().unwrap();
        let wb = wb_q.get(e).unwrap();
        assert!((wb.center - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-5);
        assert!((wb.radius - 1.0).abs() < 1e-5);
    }

    #[test]
    fn parent_bound_unions_child_bounds() {
        // Parent at origin (no LocalBound) with two leaf children at ±10
        // along x, each with local radius 1. The parent WorldBound should
        // be the smallest sphere enclosing both — center at origin, r=11.
        let mut world = World::new();
        let parent = world.spawn();
        world.insert(parent, GlobalTransform::IDENTITY);
        world.insert(parent, WorldBound::ZERO);

        let left = spawn_leaf(
            &mut world,
            Vec3::new(-10.0, 0.0, 0.0),
            1.0,
            Vec3::ZERO,
            1.0,
        );
        let right = spawn_leaf(
            &mut world,
            Vec3::new(10.0, 0.0, 0.0),
            1.0,
            Vec3::ZERO,
            1.0,
        );

        // Wire the hierarchy: both leaves are children of `parent`.
        world.insert(left, Parent(parent));
        world.insert(right, Parent(parent));
        world.insert(parent, Children(vec![left, right]));

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);

        let wb_q = world.query::<WorldBound>().unwrap();

        let left_wb = *wb_q.get(left).unwrap();
        assert!((left_wb.center - Vec3::new(-10.0, 0.0, 0.0)).length() < 1e-5);
        let right_wb = *wb_q.get(right).unwrap();
        assert!((right_wb.center - Vec3::new(10.0, 0.0, 0.0)).length() < 1e-5);

        let parent_wb = *wb_q.get(parent).unwrap();
        assert!(
            (parent_wb.center - Vec3::ZERO).length() < 1e-5,
            "parent center should be midpoint, got {:?}",
            parent_wb.center,
        );
        assert!(
            (parent_wb.radius - 11.0).abs() < 1e-5,
            "parent radius should enclose both leaves, got {}",
            parent_wb.radius,
        );
        // Contains-check both leaves' centers.
        assert!(parent_wb.contains_point(left_wb.center));
        assert!(parent_wb.contains_point(right_wb.center));
    }

    #[test]
    fn pure_parent_with_no_children_keeps_zero_bound() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, GlobalTransform::IDENTITY);
        world.insert(e, WorldBound::ZERO);

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);

        let wb_q = world.query::<WorldBound>().unwrap();
        let wb = wb_q.get(e).unwrap();
        assert_eq!(wb.radius, 0.0);
    }
}
