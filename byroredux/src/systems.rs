//! ECS systems for the application: camera, animation, transform propagation, etc.

use byroredux_core::animation::{
    advance_stack, advance_time, sample_blended_transform, sample_bool_channel,
    sample_color_channel, sample_float_channel, sample_rotation, sample_scale, sample_translation,
    split_root_motion, AnimationClipRegistry, AnimationPlayer, AnimationStack, FloatTarget,
    RootMotionDelta,
};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{
    ActiveCamera, AnimatedAlpha, AnimatedColor, AnimatedVisibility, Children,
    DebugStats, DeltaTime, EngineConfig, GlobalTransform, Name, Parent, TotalTime, Transform, World,
};
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::{FixedString, StringPool};

use crate::anim_convert::build_subtree_name_map;
use crate::components::{InputState, NameIndex, Spinning};

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
    if input.keys_held.contains(&winit::keyboard::KeyCode::ShiftLeft) {
        move_dir.y -= 1.0;
    }

    // Speed boost with Ctrl.
    let boost = if input.keys_held.contains(&winit::keyboard::KeyCode::ControlLeft) {
        3.0
    } else {
        1.0
    };
    drop(input);

    // Build rotation from yaw/pitch.
    let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);

    if let Some(mut tq) = world.query_mut::<Transform>() {
        if let Some(transform) = tq.get_mut(cam_entity) {
            transform.rotation = rotation;

            if move_dir != Vec3::ZERO {
                let move_dir = move_dir.normalize();
                // Move relative to camera orientation (but yaw-only for horizontal).
                let forward = Quat::from_rotation_y(yaw) * -Vec3::Z;
                let right = Quat::from_rotation_y(yaw) * Vec3::X;
                let up = Vec3::Y;

                transform.translation += forward * move_dir.z * speed * boost;
                transform.translation += right * move_dir.x * speed * boost;
                transform.translation += up * move_dir.y * speed * boost;
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

    // Per-frame cache of subtree name maps — built once per unique root entity,
    // reused across all animated entities sharing that root. Avoids rebuilding
    // the HashMap + BFS walk for every AnimationPlayer/Stack each frame.
    let mut subtree_cache: std::collections::HashMap<
        EntityId,
        std::collections::HashMap<FixedString, EntityId>,
    > = std::collections::HashMap::new();

    // Rebuild name→entity index only when entities have been added.
    let current_gen = world.next_entity_id();
    {
        let needs_rebuild = world
            .try_resource::<NameIndex>()
            .map(|idx| idx.generation != current_gen)
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
            idx.generation = current_gen;
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

        // Scoped name lookup — cached per root entity.
        let scoped_map = ps.root_entity.map(|root| {
            subtree_cache
                .entry(root)
                .or_insert_with(|| build_subtree_name_map(world, root))
                as &std::collections::HashMap<FixedString, EntityId>
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

        // Scoped name lookup for stacks — cached per root entity.
        let stack_scoped_map = stack.root_entity.map(|root| {
            subtree_cache
                .entry(root)
                .or_insert_with(|| build_subtree_name_map(world, root))
                as &std::collections::HashMap<FixedString, EntityId>
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
        let mut channel_names: Vec<&str> = Vec::new();
        for layer in &stack.layers {
            if let Some(clip) = registry.get(layer.clip_handle) {
                for name in clip.channels.keys() {
                    channel_names.push(name.as_str());
                }
            }
        }
        channel_names.sort_unstable();
        channel_names.dedup();

        let mut updates: Vec<(EntityId, Vec3, Quat, f32)> = Vec::new();
        for channel_name in &channel_names {
            let Some(target_entity) = stack_resolve(channel_name) else {
                continue;
            };
            if let Some((pos, rot, scale)) =
                sample_blended_transform(stack, &registry, channel_name)
            {
                updates.push((target_entity, pos, rot, scale));
            }
        }
        drop(sq);

        // Apply blended transforms.
        let mut tq = world.query_mut::<Transform>().unwrap();
        for (target, pos, rot, scale) in updates {
            if let Some(transform) = tq.get_mut(target) {
                transform.translation = pos;
                transform.rotation = rot;
                transform.scale = scale;
            }
        }
    }
}

/// Transform propagation system: computes GlobalTransform from local Transform + parent chain.
///
/// For root entities (no Parent), GlobalTransform = Transform.
/// For child entities, GlobalTransform = parent.GlobalTransform ∘ child.Transform.
/// Must run after animation_system and before rendering.
/// Create the transform propagation system with reusable scratch buffers.
///
/// Returns a closure (FnMut) that captures `roots` and `queue` Vecs,
/// clearing and reusing them each frame instead of allocating new ones.
pub(crate) fn make_transform_propagation_system() -> impl FnMut(&World, f32) + Send + Sync {
    let mut roots: Vec<EntityId> = Vec::new();
    let mut queue: Vec<EntityId> = Vec::new();

    move |world: &World, _dt: f32| {
        roots.clear();
        queue.clear();

        // Phase 1: find root entities (have Transform but no Parent).
        {
            let Some(tq) = world.query::<Transform>() else {
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

        // Update root GlobalTransforms.
        {
            let tq = world.query::<Transform>().unwrap();
            let mut gq = match world.query_mut::<GlobalTransform>() {
                Some(q) => q,
                None => return,
            };
            for &entity in &roots {
                if let Some(t) = tq.get(entity) {
                    if let Some(g) = gq.get_mut(entity) {
                        g.translation = t.translation;
                        g.rotation = t.rotation;
                        g.scale = t.scale;
                    }
                }
            }
        }

        // Phase 2: propagate to children using BFS.
        let children_q = world.query::<Children>();
        let Some(ref cq) = children_q else { return };

        for &root in &roots {
            if let Some(children) = cq.get(root) {
                queue.extend_from_slice(&children.0);
            }
        }

        while let Some(entity) = queue.pop() {
            let parent_q = world.query::<Parent>().unwrap();
            let Some(parent) = parent_q.get(entity) else {
                continue;
            };
            let parent_id = parent.0;
            drop(parent_q);

            let gq_read = world.query::<GlobalTransform>().unwrap();
            let Some(parent_global) = gq_read.get(parent_id) else {
                continue;
            };
            let parent_global = *parent_global;
            drop(gq_read);

            let tq = world.query::<Transform>().unwrap();
            let local = tq.get(entity).copied().unwrap_or(Transform::IDENTITY);
            drop(tq);

            let composed = GlobalTransform::compose(
                &parent_global,
                local.translation,
                local.rotation,
                local.scale,
            );

            let mut gq_write = world.query_mut::<GlobalTransform>().unwrap();
            if let Some(g) = gq_write.get_mut(entity) {
                *g = composed;
            }
            drop(gq_write);

            if let Some(children) = cq.get(entity) {
                queue.extend_from_slice(&children.0);
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
