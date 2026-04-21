//! ECS systems for the application: camera, animation, transform propagation, etc.

use byroredux_core::animation::{
    advance_stack, advance_time, sample_blended_transform, sample_bool_channel,
    sample_color_channel, sample_float_channel, sample_rotation, sample_scale, sample_translation,
    split_root_motion, visit_stack_text_events, visit_text_key_events, AnimationClipRegistry,
    AnimationPlayer, AnimationStack, ColorTarget, FloatTarget, RootMotionDelta,
};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{
    ActiveCamera, AnimatedAlpha, AnimatedAmbientColor, AnimatedDiffuseColor, AnimatedEmissiveColor,
    AnimatedShaderColor, AnimatedSpecularColor, AnimatedVisibility, Billboard, BillboardMode,
    Children, DebugStats, DeltaTime, GlobalTransform, LocalBound, Name, Parent, ParticleEmitter,
    TotalTime, Transform, World, WorldBound,
};
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::FixedString;

use crate::anim_convert::build_subtree_name_map;
use crate::components::{
    CellLightingRes, GameTimeRes, InputState, NameIndex, SkyParamsRes, Spinning, SubtreeCache,
    WeatherDataRes,
};

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

/// Sample every color channel at `time` and route each sampled RGB
/// value to the matching `AnimatedDiffuseColor` / `AnimatedAmbient…` /
/// `AnimatedSpecular…` / `AnimatedEmissive…` / `AnimatedShader…`
/// component on the resolved target entity.
///
/// Replaces the pre-#517 single-bucket write to `AnimatedColor`, which
/// silently conflated diffuse, ambient, specular, emissive, and
/// BSLighting/BSEffect shader colors into one slot. Each target has
/// its own sparse component so an entity with both a diffuse
/// controller and an emissive controller keeps the two animations
/// independent (no last-write-wins).
///
/// The `resolve_entity` closure is whatever scope the caller wants —
/// flat `AnimationPlayer` uses `name_index` (+ optional
/// `SubtreeCache` for scoped subtree lookups), `AnimationStack`
/// layers use the stack's own `stack_resolve`.
fn apply_color_channels(
    world: &World,
    color_channels: &[(FixedString, byroredux_core::animation::ColorChannel)],
    time: f32,
    resolve_entity: &dyn Fn(&FixedString) -> Option<EntityId>,
) {
    // Lazy-acquire each target's write guard on first use. Most clips
    // carry only one or two color targets (an emissive pulse, maybe a
    // diffuse tint) so we avoid locking all five sparse storages
    // unconditionally.
    let mut diffuse_q = None;
    let mut ambient_q = None;
    let mut specular_q = None;
    let mut emissive_q = None;
    let mut shader_q = None;

    for (channel_name, channel) in color_channels {
        let Some(target_entity) = resolve_entity(channel_name) else {
            continue;
        };
        let value = sample_color_channel(channel, time);
        match channel.target {
            ColorTarget::Diffuse => {
                let q = diffuse_q.get_or_insert_with(|| world.query_mut::<AnimatedDiffuseColor>());
                if let Some(q) = q.as_mut() {
                    if let Some(c) = q.get_mut(target_entity) {
                        c.0 = value;
                    }
                }
            }
            ColorTarget::Ambient => {
                let q = ambient_q.get_or_insert_with(|| world.query_mut::<AnimatedAmbientColor>());
                if let Some(q) = q.as_mut() {
                    if let Some(c) = q.get_mut(target_entity) {
                        c.0 = value;
                    }
                }
            }
            ColorTarget::Specular => {
                let q =
                    specular_q.get_or_insert_with(|| world.query_mut::<AnimatedSpecularColor>());
                if let Some(q) = q.as_mut() {
                    if let Some(c) = q.get_mut(target_entity) {
                        c.0 = value;
                    }
                }
            }
            ColorTarget::Emissive => {
                let q =
                    emissive_q.get_or_insert_with(|| world.query_mut::<AnimatedEmissiveColor>());
                if let Some(q) = q.as_mut() {
                    if let Some(c) = q.get_mut(target_entity) {
                        c.0 = value;
                    }
                }
            }
            ColorTarget::ShaderColor => {
                let q = shader_q.get_or_insert_with(|| world.query_mut::<AnimatedShaderColor>());
                if let Some(q) = q.as_mut() {
                    if let Some(c) = q.get_mut(target_entity) {
                        c.0 = value;
                    }
                }
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
        prev_time: f32,
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
                prev_time: player.prev_time,
            });
        }
    } // AnimationPlayer lock released here

    // Emit text key events for AnimationPlayer entities (#211 / #339).
    {
        use byroredux_scripting::events::{AnimationTextKeyEvent, AnimationTextKeyEvents};
        let mut eq = world.query_mut::<AnimationTextKeyEvents>().unwrap();
        let mut events: Vec<AnimationTextKeyEvent> = Vec::new();
        for ps in &playback_states {
            let Some(clip) = registry.get(ps.clip_handle) else {
                continue;
            };
            events.clear();
            visit_text_key_events(clip, ps.prev_time, ps.current_time, |time, label| {
                events.push(AnimationTextKeyEvent {
                    label: label.to_owned(),
                    time,
                });
            });
            if !events.is_empty() {
                eq.insert(
                    ps.entity,
                    AnimationTextKeyEvents(std::mem::take(&mut events)),
                );
            }
        }
    }

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
        let scoped_map = ps
            .root_entity
            .and_then(|root| subtree_ref.as_ref().and_then(|c| c.map.get(&root)));
        let resolve_entity = |sym: &FixedString| -> Option<EntityId> {
            if let Some(scoped) = scoped_map {
                scoped.get(sym).copied()
            } else {
                name_index.map.get(sym).copied()
            }
        };

        // Apply transform channels.
        let is_accum_root =
            |name: &FixedString| -> bool { clip.accum_root_name.as_ref() == Some(name) };
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

        // Apply color channels — route to the right target component
        // by `channel.target`. Pre-#517 everything landed in a single
        // `AnimatedColor` slot, so an emissive pulse clobbered a
        // diffuse tint on the same entity and vice-versa. Each target
        // component is a separate `SparseSetStorage` so an entity with
        // both a diffuse and an emissive controller keeps both
        // animations independent.
        if !clip.color_channels.is_empty() {
            apply_color_channels(world, &clip.color_channels, current_time, &resolve_entity);
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
    let mut channel_names_scratch: Vec<FixedString> = Vec::new();
    let mut updates_scratch: Vec<(FixedString, EntityId, Vec3, Quat, f32)> = Vec::new();

    for entity in stack_entities {
        // Phase 1: advance all layers (write lock).
        {
            let mut sq = world.query_mut::<AnimationStack>().unwrap();
            let stack = sq.get_mut(entity).unwrap();
            advance_stack(stack, &registry, dt);
        }

        // Ensure subtree cache is populated for this stack's root before we
        // take the AnimationStack read lock below (cache rebuild acquires a
        // write lock on SubtreeCache, separate from AnimationStack).
        {
            let sq = world.query::<AnimationStack>().unwrap();
            let stack = sq.get(entity).unwrap();
            let root_entity = stack.root_entity;
            drop(sq);
            if let Some(root) = root_entity {
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
        }

        let subtree_ref2 = world.try_resource::<SubtreeCache>();

        // Phase 2: single read lock for everything that reads AnimationStack
        // (#287 — was 4 separate acquisitions, now 1). Collect all outputs
        // into owned / registry-borrowed data so the lock drops before any
        // writes. Dominant info is stored as (clip_handle, local_time) —
        // NO channel Vec clones (#265).
        // Text-key events scratch + dedup seen-set (#339). Owned by
        // this call; Vec capacity amortizes across layers.
        use byroredux_scripting::events::AnimationTextKeyEvent;
        let mut events: Vec<AnimationTextKeyEvent> = Vec::new();
        let mut seen_labels: Vec<&str> = Vec::new();
        let accum_root: Option<FixedString>;
        let dominant_info: Option<(u32, f32)>;
        let stack_root: Option<EntityId>;
        {
            let sq = world.query::<AnimationStack>().unwrap();
            let stack = sq.get(entity).unwrap();
            stack_root = stack.root_entity;

            // Text key events (#211 / #339) — visitor form allocates
            // `AnimationTextKeyEvent` only when events actually fire.
            visit_stack_text_events(stack, &registry, &mut seen_labels, |time, label| {
                events.push(AnimationTextKeyEvent {
                    label: label.to_owned(),
                    time,
                });
            });

            // Scoped name resolver — reads subtree cache (outer lock).
            let stack_scoped_map = stack
                .root_entity
                .and_then(|root| subtree_ref2.as_ref().and_then(|c| c.map.get(&root)));
            let stack_resolve = |sym: &FixedString| -> Option<EntityId> {
                if let Some(scoped) = stack_scoped_map {
                    scoped.get(sym).copied()
                } else {
                    name_index.map.get(sym).copied()
                }
            };

            // Collect channel names across active layers (#251 scratch reuse).
            channel_names_scratch.clear();
            for layer in &stack.layers {
                if let Some(clip) = registry.get(layer.clip_handle) {
                    for name in clip.channels.keys() {
                        channel_names_scratch.push(*name);
                    }
                }
            }
            channel_names_scratch.sort_unstable();
            channel_names_scratch.dedup();

            // Sample blended transforms (#252 scratch reuse).
            updates_scratch.clear();
            for &channel_name in &channel_names_scratch {
                let Some(target_entity) = stack_resolve(&channel_name) else {
                    continue;
                };
                if let Some((pos, rot, scale)) =
                    sample_blended_transform(stack, &registry, channel_name)
                {
                    updates_scratch.push((channel_name, target_entity, pos, rot, scale));
                }
            }

            // Accum root name from highest-weight active layer (#279 D6-04).
            let mut best: Option<(FixedString, f32)> = None;
            for layer in &stack.layers {
                let ew = layer.effective_weight();
                if ew < 0.001 {
                    continue;
                }
                if let Some(clip) = registry.get(layer.clip_handle) {
                    if let Some(name) = clip.accum_root_name {
                        if best.is_none_or(|(_, bw)| ew > bw) {
                            best = Some((name, ew));
                        }
                    }
                }
            }
            accum_root = best.map(|(n, _)| n);

            // Dominant layer: capture only clip_handle + local_time. The
            // float/color/bool channel Vecs are accessed via the registry
            // AFTER the stack lock drops — no clones required (#265).
            dominant_info = stack
                .layers
                .iter()
                .filter(|l| l.effective_weight() >= 0.001)
                .max_by(|a, b| {
                    a.effective_weight()
                        .partial_cmp(&b.effective_weight())
                        .unwrap()
                })
                .map(|l| (l.clip_handle, l.local_time));

            drop(sq);
        }

        // Phase 3a: emit text events (write lock on a different component).
        if !events.is_empty() {
            use byroredux_scripting::events::AnimationTextKeyEvents;
            let mut eq = world.query_mut::<AnimationTextKeyEvents>().unwrap();
            eq.insert(entity, AnimationTextKeyEvents(std::mem::take(&mut events)));
        }

        // Phase 3b: apply blended transforms with root motion splitting (AR-02).
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

        if root_motion != Vec3::ZERO {
            if let Some(mut rmq) = world.query_mut::<RootMotionDelta>() {
                if let Some(rm) = rmq.get_mut(entity) {
                    rm.0 = root_motion;
                }
            }
        }

        // Phase 3c: apply non-transform channels from the dominant layer
        // (AR-01). Access channel Vecs through the registry directly —
        // no clones. #265.
        let stack_scoped_map =
            stack_root.and_then(|root| subtree_ref2.as_ref().and_then(|c| c.map.get(&root)));
        let stack_resolve = |sym: &FixedString| -> Option<EntityId> {
            if let Some(scoped) = stack_scoped_map {
                scoped.get(sym).copied()
            } else {
                name_index.map.get(sym).copied()
            }
        };

        if let Some((clip_handle, time)) = dominant_info {
            if let Some(clip) = registry.get(clip_handle) {
                if !clip.float_channels.is_empty() {
                    if let Some(mut aq) = world.query_mut::<AnimatedAlpha>() {
                        for (channel_name, channel) in &clip.float_channels {
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
                if !clip.color_channels.is_empty() {
                    apply_color_channels(world, &clip.color_channels, time, &stack_resolve);
                }
                if !clip.bool_channels.is_empty() {
                    if let Some(mut vq) = world.query_mut::<AnimatedVisibility>() {
                        for (channel_name, channel) in &clip.bool_channels {
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

        let new_rot =
            compute_billboard_rotation(billboard.mode, global.translation, cam_pos, cam_forward);
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
        // Acquire Children and LocalBound once — used by both passes.
        // Previously re-acquired for pass 2 (#250).
        let local_q = world.query::<LocalBound>();
        let children_q = world.query::<Children>();

        // ── Pass 1: leaf bounds from LocalBound + GlobalTransform ──────
        {
            let Some(ref lb_q) = local_q else {
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

        // Fold children into parents. Must be post-order — children first.
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

/// CPU particle system: spawn at the configured rate, integrate
/// velocity + gravity, expire by age. Runs in `Update` after the
/// scene-graph propagation has settled the host transforms — particles
/// spawn in **world space** by sampling the host's `GlobalTransform`
/// translation at spawn time, which matches the legacy Gamebryo
/// behavior where particles detach from their host once emitted (so
/// the host can rotate / move without dragging old smoke/fire along).
///
/// See #401 — pre-fix every parsed particle block was discarded and
/// every torch / fire / magic FX rendered as an invisible node.
///
/// The PRNG is a tiny xorshift seeded by `(entity, frame_count)` so
/// per-emitter behavior is deterministic for tests and replays without
/// requiring a `rand` dependency. Particles burn ~10 FLOPs each per
/// integration step — well below the budget for the typical worst-case
/// (a brazier emitter at 64 live particles).
pub(crate) fn particle_system(world: &World, dt: f32) {
    if dt <= 0.0 {
        return;
    }
    let total_time_secs = world.resource::<TotalTime>().0;
    let frame_seed = (total_time_secs * 1000.0) as u64;

    // Read each emitter entity's world-space spawn origin from
    // GlobalTransform. We only mutate ParticleEmitter (live SoA +
    // accumulator), so the GlobalTransform query stays read-only and
    // doesn't fight any other PostUpdate writer.
    let Some((gt_q, mut em_q)) =
        world.query_2_mut::<GlobalTransform, ParticleEmitter>()
    else {
        return;
    };

    for (entity, em) in em_q.iter_mut() {
        let host_translation = match gt_q.get(entity) {
            Some(g) => g.translation,
            None => continue,
        };

        // Tiny xorshift32, seeded per-emitter per-frame. Avoids a `rand`
        // dependency and gives reproducible behavior under fixed-step
        // tests. The lower bits of `entity.index()` jitter sufficiently
        // across emitters spawned in the same frame.
        let mut state: u32 = (frame_seed as u32).wrapping_add(entity.wrapping_mul(2654435761));
        if state == 0 {
            state = 0x9E37_79B9; // golden-ratio fallback
        }
        let mut rng = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as f32) / (u32::MAX as f32)
        };

        // 1. Integrate live particles (velocity + gravity) and age them.
        let len = em.particles.len();
        for i in 0..len {
            em.particles.velocities[i][0] += em.gravity[0] * dt;
            em.particles.velocities[i][1] += em.gravity[1] * dt;
            em.particles.velocities[i][2] += em.gravity[2] * dt;
            em.particles.positions[i][0] += em.particles.velocities[i][0] * dt;
            em.particles.positions[i][1] += em.particles.velocities[i][1] * dt;
            em.particles.positions[i][2] += em.particles.velocities[i][2] * dt;
            em.particles.ages[i] += dt;
        }

        // 2. Expire particles whose age exceeds their lifespan. Iterate
        //    backwards so swap_remove doesn't skip survivors.
        let mut i = em.particles.len();
        while i > 0 {
            i -= 1;
            if em.particles.ages[i] >= em.particles.lifes[i] {
                em.particles.swap_remove(i);
            }
        }

        // 3. Spawn new particles at the configured rate. Fractional
        //    spawns accumulate across frames so a 30 Hz emitter under a
        //    60 fps frame still averages exactly 30 spawns/sec.
        em.spawn_accumulator += em.rate * dt;
        let spawn_count = em.spawn_accumulator.floor() as i32;
        em.spawn_accumulator -= spawn_count as f32;

        let cap = em.max_particles as usize;
        for _ in 0..spawn_count.max(0) {
            if em.particles.len() >= cap {
                break;
            }
            let local_offset = em.shape.sample(&mut rng);
            let world_pos = [
                host_translation.x + local_offset[0],
                host_translation.y + local_offset[1],
                host_translation.z + local_offset[2],
            ];

            // Build a velocity vector inside the declination cone around
            // local +Z, then jitter speed.
            let phi = rng() * std::f32::consts::TAU;
            let dec = em.declination + (rng() - 0.5) * em.declination_variation;
            let sin_dec = dec.sin();
            let cos_dec = dec.cos();
            let dir = [sin_dec * phi.cos(), sin_dec * phi.sin(), cos_dec];
            let speed = em.speed + (rng() - 0.5) * em.speed_variation;
            let vel = [dir[0] * speed, dir[1] * speed, dir[2] * speed];

            let life = em.life + (rng() - 0.5) * em.life_variation;
            // Guard against zero/negative life so the expire pass can
            // handle the particle correctly on the very next tick.
            let life = life.max(0.05);

            em.particles
                .push(world_pos, vel, life, em.start_color, em.start_size);
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
///
/// Writes at `log::info!` / target `engine::stats`, so `env_logger`'s
/// default filter (info) surfaces it in release builds without needing
/// `--debug`. Users who want a quieter console can set
/// `RUST_LOG=warn` or target-filter
/// `RUST_LOG=info,engine::stats=warn`. See #366.
pub(crate) fn log_stats_system(world: &World, _dt: f32) {
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

/// Build the time-of-day key table used by the `weather_system`
/// interpolator from a climate's `tod_hours`.
///
/// `tod_hours = [sunrise_begin, sunrise_end, sunset_begin, sunset_end]`
/// in floating-point game hours (CLMT TNAM bytes divided by 6). The
/// returned 7-entry table is `(hour, TOD slot index)` pairs the
/// interpolator walks in increasing-hour order:
///
///  - `midnight` (synthetic — TNAM doesn't encode it; anchored at 1h)
///  - `sunrise_begin` → `TOD_SUNRISE`
///  - `sunrise_end`   → `TOD_DAY`
///  - midpoint(sunrise_end, sunset_begin) → `TOD_HIGH_NOON`
///  - `sunset_begin - 2h` (clamped) → `TOD_DAY` re-anchor — preserves
///    the `day → sunset` ease-in the pre-#463 hardcoded path had
///  - `sunset_begin` → `TOD_SUNSET`
///  - `sunset_end + 2h` (clamped to 23h) → `TOD_NIGHT`
///
/// Kept `pub(crate)` so the unit test in this module can pin the
/// formula independently of a full World setup.
pub(crate) fn build_tod_keys(tod_hours: [f32; 4]) -> [(f32, usize); 7] {
    use byroredux_plugin::esm::records::weather::*;
    let [sunrise_begin, sunrise_end, sunset_begin, sunset_end] = tod_hours;
    let afternoon_peak = (sunrise_end + sunset_begin) * 0.5;
    let afternoon_cool = (sunset_begin - 2.0).max(sunrise_end + 0.1);
    let midnight = 1.0f32;
    let night = (sunset_end + 2.0).min(23.0);
    [
        (midnight, TOD_MIDNIGHT),
        (sunrise_begin, TOD_SUNRISE),
        (sunrise_end, TOD_DAY),
        (afternoon_peak, TOD_HIGH_NOON),
        (afternoon_cool, TOD_DAY),
        (sunset_begin, TOD_SUNSET),
        (night, TOD_NIGHT),
    ]
}

/// Weather & time-of-day system: advances game clock, interpolates WTHR
/// NAM0 sky colors, computes sun arc, and updates SkyParamsRes + CellLightingRes.
///
/// Only runs when WeatherDataRes + GameTimeRes exist (exterior cells with weather).
pub(crate) fn weather_system(world: &World, dt: f32) {
    // Advance game clock.
    let hour = {
        let Some(mut game_time) = world.try_resource_mut::<GameTimeRes>() else {
            return;
        };
        game_time.hour += dt * game_time.time_scale / 3600.0;
        if game_time.hour >= 24.0 {
            game_time.hour -= 24.0;
        }
        game_time.hour
    };

    let Some(wd) = world.try_resource::<WeatherDataRes>() else {
        return;
    };

    // Interpolate NAM0 colors based on game hour.
    // The 6 time slots map to these hours:
    //   0 = sunrise, 1 = day, 2 = sunset,
    //   3 = night, 4 = high_noon, 5 = midnight.
    //
    // Pre-#463 the breakpoints were hardcoded:
    //   midnight(1h) → sunrise(6h) → day(10h) → high_noon(13h) →
    //   day(16h) → sunset(18h) → night(22h) → midnight(25h/1h)
    // FO3 Capital Wasteland and FNV Mojave ship different CLMT TNAM
    // values (Wasteland sunrise is ~0.3 hr earlier). `tod_hours` on
    // WeatherDataRes now carries the climate-driven breakpoints; the
    // `high_noon` midpoint and the `midnight` anchor stay synthetic
    // (TNAM doesn't encode either). The afternoon `day` re-anchor is
    // picked at sunset_begin - 2h so we retain a `day → sunset` ease-
    // in rather than jumping straight from high_noon to sunset.
    use byroredux_plugin::esm::records::weather::*;
    let keys = build_tod_keys(wd.tod_hours);

    // Find which two keys we're between and compute blend factor.
    let (slot_a, slot_b, t) = {
        // Wrap hour to the key range [1, 25) — midnight at hour 0 maps to 24+1=25.
        let h = if hour < keys[0].0 { hour + 24.0 } else { hour };
        let last = keys.len() - 1;
        let mut found = (keys[last].1, keys[0].1, 0.0f32);
        for i in 0..last {
            let (h0, s0) = keys[i];
            let (h1, s1) = keys[i + 1];
            if h >= h0 && h < h1 {
                let frac = (h - h0) / (h1 - h0);
                found = (s0, s1, frac);
                break;
            }
        }
        // After last key (22h+): interpolate night → midnight.
        if h >= keys[last].0 {
            let h0 = keys[last].0;
            let h1 = keys[0].0 + 24.0; // midnight = 25
            let frac = ((h - h0) / (h1 - h0)).clamp(0.0, 1.0);
            found = (keys[last].1, keys[0].1, frac);
        }
        found
    };

    let lerp3 = |a: [f32; 3], b: [f32; 3], t: f32| -> [f32; 3] {
        [
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
        ]
    };

    let zenith = lerp3(
        wd.sky_colors[SKY_UPPER][slot_a],
        wd.sky_colors[SKY_UPPER][slot_b],
        t,
    );
    let horizon = lerp3(
        wd.sky_colors[SKY_HORIZON][slot_a],
        wd.sky_colors[SKY_HORIZON][slot_b],
        t,
    );
    let sun_col = lerp3(
        wd.sky_colors[SKY_SUN][slot_a],
        wd.sky_colors[SKY_SUN][slot_b],
        t,
    );
    let ambient = lerp3(
        wd.sky_colors[SKY_AMBIENT][slot_a],
        wd.sky_colors[SKY_AMBIENT][slot_b],
        t,
    );
    let sunlight = lerp3(
        wd.sky_colors[SKY_SUNLIGHT][slot_a],
        wd.sky_colors[SKY_SUNLIGHT][slot_b],
        t,
    );
    let fog_col = lerp3(
        wd.sky_colors[SKY_FOG][slot_a],
        wd.sky_colors[SKY_FOG][slot_b],
        t,
    );

    // Fog distance: interpolate between day and night based on
    // how "night-like" the current hour is (0 = full day, 1 = full night).
    let night_factor = if (6.0..=18.0).contains(&hour) {
        0.0 // daytime
    } else if hour >= 20.0 || hour <= 4.0 {
        1.0 // full night
    } else if hour > 18.0 {
        (hour - 18.0) / 2.0 // sunset transition
    } else {
        (6.0 - hour) / 2.0 // sunrise transition
    };
    let fog_near = wd.fog[0] + (wd.fog[2] - wd.fog[0]) * night_factor;
    let fog_far = wd.fog[1] + (wd.fog[3] - wd.fog[1]) * night_factor;

    // Sun direction: semicircular arc from east (6h) through zenith (12h) to west (18h).
    // Below horizon at night. Y-up coordinate system.
    let sun_dir = {
        // Solar angle: 0 at sunrise (6h), π at sunset (18h).
        let solar_hour = (hour - 6.0).clamp(0.0, 12.0);
        let angle = solar_hour / 12.0 * std::f32::consts::PI;
        // Sun arcs from east (+X) through up (+Y) to west (-X).
        // Add a slight south tilt (negative Z in Y-up).
        let x = angle.cos();
        let y = angle.sin();
        let z = -0.15_f32; // slight south tilt
        let len = (x * x + y * y + z * z).sqrt();
        if (6.0..=18.0).contains(&hour) {
            [x / len, y / len, z / len]
        } else {
            // Night: sun below horizon. Push it down so no sun disc renders.
            [0.0, -1.0, 0.0]
        }
    };

    // Sun intensity: fade in/out at sunrise/sunset.
    let sun_intensity = if (7.0..=17.0).contains(&hour) {
        4.0
    } else if (6.0..7.0).contains(&hour) {
        (hour - 6.0) * 4.0 // fade in
    } else if hour > 17.0 && hour <= 18.0 {
        (18.0 - hour) * 4.0 // fade out
    } else {
        0.0 // night
    };

    // Cloud layer 0 scroll rate. DNAM speeds are u8 (0-255). At 128 we want
    // a noticeable but not blurry drift — empirically ~0.02 UV/sec feels right
    // at the 0.15 tile scale set in scene.rs. Scroll uses dt (real seconds),
    // not game-time: clouds drift with wall-clock wind, not the day/night cycle.
    let cloud_speed_01 = wd.cloud_speeds[0] as f32 / 128.0;
    let cloud_scroll_rate = 0.02 * cloud_speed_01;

    drop(wd);

    // Update SkyParamsRes.
    if let Some(mut sky) = world.try_resource_mut::<SkyParamsRes>() {
        sky.zenith_color = zenith;
        sky.horizon_color = horizon;
        sky.sun_color = sun_col;
        sky.sun_direction = sun_dir;
        sky.sun_intensity = sun_intensity;
        // Wrap scroll at 1.0 so it never grows unboundedly; sampler REPEAT
        // makes the wrap invisible.
        sky.cloud_scroll[0] = (sky.cloud_scroll[0] + cloud_scroll_rate * dt).rem_euclid(1.0);
        sky.cloud_scroll[1] = (sky.cloud_scroll[1] + cloud_scroll_rate * 0.3 * dt).rem_euclid(1.0);
    }

    // Update CellLightingRes.
    if let Some(mut cell_lit) = world.try_resource_mut::<CellLightingRes>() {
        cell_lit.ambient = ambient;
        cell_lit.directional_color = sunlight;
        cell_lit.directional_dir = sun_dir;
        cell_lit.fog_color = fog_col;
        cell_lit.fog_near = fog_near;
        cell_lit.fog_far = fog_far;
    }
}

#[cfg(test)]
mod bound_propagation_tests {
    //! Regression tests for `make_world_bound_propagation_system` — issue #217.
    //! These cover leaf derivation, parent merging, and the scale path.

    use super::*;
    use byroredux_core::ecs::World;
    use byroredux_core::ecs::{Children, GlobalTransform, LocalBound, Parent, WorldBound};
    use byroredux_core::math::{Quat, Vec3};

    /// Spawn an entity with a LocalBound + GlobalTransform + empty WorldBound.
    fn spawn_leaf(
        world: &mut World,
        translation: Vec3,
        scale: f32,
        local_center: Vec3,
        local_radius: f32,
    ) -> byroredux_core::ecs::storage::EntityId {
        let e = world.spawn();
        world.insert(e, GlobalTransform::new(translation, Quat::IDENTITY, scale));
        world.insert(e, LocalBound::new(local_center, local_radius));
        world.insert(e, WorldBound::ZERO);
        e
    }

    #[test]
    fn leaf_bound_composes_local_with_global_transform() {
        let mut world = World::new();
        let e = spawn_leaf(&mut world, Vec3::new(10.0, 0.0, 0.0), 1.0, Vec3::ZERO, 2.0);

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
        let e = spawn_leaf(&mut world, Vec3::ZERO, 2.0, Vec3::new(1.0, 0.0, 0.0), 0.5);

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

        let left = spawn_leaf(&mut world, Vec3::new(-10.0, 0.0, 0.0), 1.0, Vec3::ZERO, 1.0);
        let right = spawn_leaf(&mut world, Vec3::new(10.0, 0.0, 0.0), 1.0, Vec3::ZERO, 1.0);

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

/// Regression tests for #463 — climate-driven TOD breakpoints on
/// `WeatherDataRes.tod_hours` flow through `build_tod_keys` so the
/// time-of-day interpolator runs on the right schedule per worldspace.
#[cfg(test)]
mod weather_tod_keys_tests {
    use super::*;
    use byroredux_plugin::esm::records::weather::*;

    /// Pre-#463 default — FNV Mojave-style hardcoded breakpoints.
    /// Verifies the fallback path still produces the same key table
    /// synthetic test cells used to get.
    #[test]
    fn default_tod_hours_reproduce_pre_fix_fnv_keys() {
        let keys = build_tod_keys([6.0, 10.0, 18.0, 22.0]);
        let expected = [
            (1.0, TOD_MIDNIGHT),
            (6.0, TOD_SUNRISE),
            (10.0, TOD_DAY),
            (14.0, TOD_HIGH_NOON), // midpoint(10, 18)
            (16.0, TOD_DAY),       // sunset_begin - 2
            (18.0, TOD_SUNSET),
            (23.0, TOD_NIGHT), // min(22+2, 23) = 23 (clamped)
        ];
        for (i, ((h, s), (eh, es))) in keys.iter().zip(expected.iter()).enumerate() {
            assert!(
                (h - eh).abs() < 1e-5,
                "key[{i}]: expected hour {eh:.2}, got {h:.2}"
            );
            assert_eq!(s, es, "key[{i}]: slot mismatch");
        }
    }

    /// FO3 Capital Wasteland ships slightly earlier sunrise per the
    /// audit. Feed representative Wasteland TNAM-derived hours and
    /// verify the interpolator hits those exact breakpoints instead
    /// of the hardcoded FNV values.
    #[test]
    fn fo3_wasteland_climate_shifts_sunrise_earlier() {
        // Hypothetical FO3 TNAM: sunrise_begin=32, sunrise_end=60,
        // sunset_begin=102, sunset_end=132 (in 10-minute units).
        //   → hours 5.33, 10.0, 17.0, 22.0.
        let wasteland = build_tod_keys([5.333, 10.0, 17.0, 22.0]);
        let fnv = build_tod_keys([6.0, 10.0, 18.0, 22.0]);
        // SUNRISE anchor moved earlier.
        assert!(
            wasteland[1].0 < fnv[1].0,
            "Wasteland SUNRISE key must fire before FNV SUNRISE"
        );
        // SUNSET anchor moved earlier too.
        assert!(
            wasteland[5].0 < fnv[5].0,
            "Wasteland SUNSET key must fire before FNV SUNSET"
        );
        // Slot identities stay put — only the hour anchors change.
        for i in 0..7 {
            assert_eq!(
                wasteland[i].1, fnv[i].1,
                "slot ordering must match across climates"
            );
        }
    }

    /// Keys must stay monotonically non-decreasing in hour so the
    /// piecewise-linear interpolator walks them in order.
    #[test]
    fn tod_keys_are_monotonic_on_realistic_climates() {
        for tod_hours in [
            [6.0, 10.0, 18.0, 22.0], // FNV
            [5.33, 10.0, 17.0, 22.0], // FO3 Wasteland
            [4.5, 9.0, 19.5, 22.0],   // Skyrim Tundra (hypothetical)
            [7.0, 11.0, 16.0, 19.0],  // compressed-day winter
        ] {
            let keys = build_tod_keys(tod_hours);
            for w in keys.windows(2) {
                assert!(
                    w[0].0 <= w[1].0 + 1e-5,
                    "TOD keys must be monotonic: {:?} → {:?} for tod_hours {:?}",
                    w[0],
                    w[1],
                    tod_hours,
                );
            }
        }
    }

    /// Afternoon_cool clamp — when `sunset_begin <= sunrise_end + 2`
    /// (very compressed day), the `sunset_begin - 2h` re-anchor would
    /// be at or before `sunrise_end`, breaking monotonicity. The
    /// `.max(sunrise_end + 0.1)` clamp guards against that.
    #[test]
    fn tod_keys_clamp_afternoon_cool_on_compressed_days() {
        // sunrise_end=10, sunset_begin=11 — only 1h of clear "day".
        let keys = build_tod_keys([5.0, 10.0, 11.0, 20.0]);
        let day_anchor = keys[2].0; // TOD_DAY at sunrise_end
        let afternoon_cool = keys[4].0; // TOD_DAY re-anchor
        assert!(
            afternoon_cool > day_anchor,
            "afternoon_cool ({afternoon_cool:.2}) must be strictly after \
             sunrise_end ({day_anchor:.2}) to keep keys monotonic"
        );
    }
}

#[cfg(test)]
mod color_routing_tests {
    //! Regression tests for `apply_color_channels` — issue #517.
    //! Pre-#517 every color channel wrote into a single `AnimatedColor`
    //! slot regardless of `channel.target`. Emissive pulses and diffuse
    //! tints on the same entity collided last-write-wins, and the
    //! shader-color path landed in the wrong component entirely. These
    //! tests pin the target-routing contract.

    use super::*;
    use byroredux_core::animation::{AnimColorKey, ColorChannel, ColorTarget};
    use byroredux_core::ecs::World;
    use byroredux_core::math::Vec3;
    use byroredux_core::string::StringPool;

    fn single_key_channel(target: ColorTarget, value: Vec3) -> ColorChannel {
        ColorChannel {
            target,
            keys: vec![AnimColorKey { time: 0.0, value }],
        }
    }

    #[test]
    fn emissive_channel_writes_only_to_emissive_component() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, AnimatedDiffuseColor(Vec3::ZERO));
        world.insert(e, AnimatedEmissiveColor(Vec3::ZERO));

        let mut pool = StringPool::new();
        let name = pool.intern("Glow");
        let channels = vec![(
            name,
            single_key_channel(ColorTarget::Emissive, Vec3::new(1.0, 0.5, 0.0)),
        )];
        let resolve = |s: &FixedString| if s == &name { Some(e) } else { None };
        apply_color_channels(&world, &channels, 0.0, &resolve);

        let dq = world.query::<AnimatedDiffuseColor>().unwrap();
        let eq = world.query::<AnimatedEmissiveColor>().unwrap();
        assert_eq!(dq.get(e).unwrap().0, Vec3::ZERO, "diffuse untouched");
        assert_eq!(
            eq.get(e).unwrap().0,
            Vec3::new(1.0, 0.5, 0.0),
            "emissive received the value"
        );
    }

    /// Both a diffuse AND an emissive controller target the same entity —
    /// pre-#517 they'd collide into the single `AnimatedColor` slot. Post-fix
    /// both land in their own component and both survive.
    #[test]
    fn diffuse_and_emissive_coexist_on_same_entity() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, AnimatedDiffuseColor(Vec3::ZERO));
        world.insert(e, AnimatedEmissiveColor(Vec3::ZERO));

        let mut pool = StringPool::new();
        let name = pool.intern("NeonSign");
        let channels = vec![
            (
                name,
                single_key_channel(ColorTarget::Diffuse, Vec3::new(0.1, 0.2, 0.3)),
            ),
            (
                name,
                single_key_channel(ColorTarget::Emissive, Vec3::new(0.9, 0.8, 0.7)),
            ),
        ];
        let resolve = |s: &FixedString| if s == &name { Some(e) } else { None };
        apply_color_channels(&world, &channels, 0.0, &resolve);

        let dq = world.query::<AnimatedDiffuseColor>().unwrap();
        let eq = world.query::<AnimatedEmissiveColor>().unwrap();
        assert_eq!(dq.get(e).unwrap().0, Vec3::new(0.1, 0.2, 0.3));
        assert_eq!(eq.get(e).unwrap().0, Vec3::new(0.9, 0.8, 0.7));
    }

    /// Shader-color target writes to `AnimatedShaderColor`, not to any of
    /// the NiMaterial slots. Covers the
    /// `BSEffectShaderPropertyColorController` path enabled by #431.
    #[test]
    fn shader_color_routes_to_shader_component() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, AnimatedDiffuseColor(Vec3::ZERO));
        world.insert(e, AnimatedShaderColor(Vec3::ZERO));

        let mut pool = StringPool::new();
        let name = pool.intern("PlasmaGlow");
        let channels = vec![(
            name,
            single_key_channel(ColorTarget::ShaderColor, Vec3::new(0.4, 0.4, 0.9)),
        )];
        let resolve = |s: &FixedString| if s == &name { Some(e) } else { None };
        apply_color_channels(&world, &channels, 0.0, &resolve);

        let dq = world.query::<AnimatedDiffuseColor>().unwrap();
        let sq = world.query::<AnimatedShaderColor>().unwrap();
        assert_eq!(dq.get(e).unwrap().0, Vec3::ZERO);
        assert_eq!(sq.get(e).unwrap().0, Vec3::new(0.4, 0.4, 0.9));
    }
}

#[cfg(test)]
mod particle_system_tests {
    //! Regression tests for `particle_system` — issue #401.
    use super::*;
    use byroredux_core::ecs::resources::TotalTime;
    use byroredux_core::ecs::{EmitterShape, ParticleEmitter, World};
    use byroredux_core::math::Vec3;

    fn world_with_emitter(em: ParticleEmitter, host_pos: Vec3) -> (World, u32) {
        let mut world = World::new();
        world.insert_resource(TotalTime(0.0));
        let e = world.spawn();
        world.insert(
            e,
            GlobalTransform::new(host_pos, byroredux_core::math::Quat::IDENTITY, 1.0),
        );
        world.insert(e, em);
        (world, e)
    }

    #[test]
    fn spawn_rate_accumulates_to_integer_count_per_frame() {
        // 30 spawns/sec, 0.5s frame → 15 particles per tick.
        let mut em = ParticleEmitter::default();
        em.rate = 30.0;
        em.life = 100.0; // never expire
        em.max_particles = 1024;
        let (world, e) = world_with_emitter(em, Vec3::ZERO);
        particle_system(&world, 0.5);
        let q = world.query::<ParticleEmitter>().unwrap();
        let em = q.get(e).unwrap();
        assert_eq!(em.particles.len(), 15);
    }

    #[test]
    fn fractional_rate_carries_across_frames() {
        // 25 spawns/sec at 1/60 s frames → 0.4167 spawn/frame.
        // After 6 frames we should see 2-3 spawns (fractional carry).
        let mut em = ParticleEmitter::default();
        em.rate = 25.0;
        em.life = 100.0;
        em.max_particles = 1024;
        let (world, e) = world_with_emitter(em, Vec3::ZERO);
        for _ in 0..6 {
            particle_system(&world, 1.0 / 60.0);
        }
        let q = world.query::<ParticleEmitter>().unwrap();
        let em = q.get(e).unwrap();
        // Floor(6 / 60 * 25) = floor(2.5) = 2.
        assert_eq!(em.particles.len(), 2);
    }

    #[test]
    fn cap_at_max_particles_drops_extra_spawns() {
        let mut em = ParticleEmitter::default();
        em.rate = 1000.0;
        em.life = 100.0;
        em.max_particles = 8;
        let (world, e) = world_with_emitter(em, Vec3::ZERO);
        particle_system(&world, 1.0);
        let q = world.query::<ParticleEmitter>().unwrap();
        let em = q.get(e).unwrap();
        assert_eq!(em.particles.len(), 8);
    }

    #[test]
    fn expire_pass_drops_particles_past_their_life() {
        let mut em = ParticleEmitter::default();
        em.rate = 0.0; // no new spawns
        em.life = 0.5;
        em.max_particles = 8;
        // Pre-seed two particles, one already past its life.
        em.particles
            .push([0.0, 0.0, 0.0], [0.0; 3], 0.5, [1.0; 4], 1.0);
        em.particles.ages[0] = 1.0; // already expired
        em.particles
            .push([0.0, 0.0, 0.0], [0.0; 3], 1.0, [1.0; 4], 1.0);
        let (world, e) = world_with_emitter(em, Vec3::ZERO);
        particle_system(&world, 0.1);
        let q = world.query::<ParticleEmitter>().unwrap();
        let em = q.get(e).unwrap();
        assert_eq!(em.particles.len(), 1);
        assert!((em.particles.lifes[0] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn integration_applies_velocity_and_gravity() {
        let mut em = ParticleEmitter::default();
        em.rate = 0.0;
        em.life = 100.0;
        em.gravity = [0.0, 0.0, -10.0];
        em.particles
            .push([0.0, 0.0, 0.0], [1.0, 0.0, 5.0], 100.0, [1.0; 4], 1.0);
        let (world, e) = world_with_emitter(em, Vec3::ZERO);
        particle_system(&world, 0.5);
        let q = world.query::<ParticleEmitter>().unwrap();
        let em = q.get(e).unwrap();
        // After 0.5s with v=(1,0,5) and a=(0,0,-10):
        // velocity_after = (1, 0, 5 + (-10)*0.5) = (1, 0, 0)
        // position_after = (0,0,0) + new_velocity*dt = (0.5, 0, 0)
        // Note: semi-implicit Euler — gravity updates v first, then x.
        assert!((em.particles.velocities[0][2] - 0.0).abs() < 1e-5);
        assert!((em.particles.positions[0][0] - 0.5).abs() < 1e-5);
        assert!((em.particles.positions[0][2] - 0.0).abs() < 1e-5);
    }

    #[test]
    fn spawn_uses_host_world_translation_as_origin() {
        let mut em = ParticleEmitter::default();
        em.rate = 100.0;
        em.life = 100.0;
        em.shape = EmitterShape::Point;
        em.speed = 0.0;
        em.declination = 0.0;
        let host = Vec3::new(50.0, 80.0, 12.5);
        let (world, e) = world_with_emitter(em, host);
        particle_system(&world, 0.05); // 5 spawns
        let q = world.query::<ParticleEmitter>().unwrap();
        let em = q.get(e).unwrap();
        for p in &em.particles.positions {
            assert!((p[0] - host.x).abs() < 1e-4);
            assert!((p[1] - host.y).abs() < 1e-4);
            assert!((p[2] - host.z).abs() < 1e-4);
        }
    }
}
