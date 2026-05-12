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
    AnimatedMorphWeights, AnimatedShaderColor, AnimatedShaderFloat, AnimatedSpecularColor,
    AnimatedUvTransform, AnimatedVisibility, Billboard, BillboardMode, Children, DebugStats,
    DeltaTime, GlobalTransform, LocalBound, Name, Parent, ParticleEmitter, TotalTime, Transform,
    World, WorldBound,
};
use byroredux_core::ecs::components::water::{SubmersionState, WaterPlane, WaterVolume};
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::FixedString;
#[cfg(test)]
use byroredux_core::string::StringPool;

use crate::anim_convert::build_subtree_name_map;
use crate::components::{
    CellLightingRes, CloudSimState, GameTimeRes, InputState, NameIndex, SkyParamsRes, Spinning,
    SubtreeCache, WeatherDataRes, WeatherTransitionRes,
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

/// Apply float channels to per-target sinks. Pre-#525 the only sink
/// was [`AnimatedAlpha`]; every other [`FloatTarget`] arm
/// (`UvOffsetU/V`, `UvScaleU/V`, `UvRotation`, `ShaderFloat`,
/// `MorphWeight(idx)`) sampled correctly but dropped the value on the
/// floor. The dispatch now covers every arm:
///
///   * `Alpha` → [`AnimatedAlpha`]
///   * `UvOffsetU/V` / `UvScaleU/V` / `UvRotation` →
///     [`AnimatedUvTransform`] (5 channels can write the same component
///     on one entity — each slot is updated independently)
///   * `ShaderFloat` → [`AnimatedShaderFloat`]
///   * `MorphWeight(idx)` → [`AnimatedMorphWeights`] (zero-pads on
///     first write past the current vec length)
///
/// Mirrors `apply_color_channels` — locks acquired lazily on first use
/// of each sink, so a clip carrying only UV-offset channels never
/// touches the morph or shader-float storages.
fn apply_float_channels(
    world: &World,
    float_channels: &[(FixedString, byroredux_core::animation::FloatChannel)],
    time: f32,
    resolve_entity: &dyn Fn(&FixedString) -> Option<EntityId>,
) {
    let mut alpha_q = None;
    let mut uv_q = None;
    let mut shader_q = None;
    let mut morph_q = None;

    for (channel_name, channel) in float_channels {
        let Some(target_entity) = resolve_entity(channel_name) else {
            continue;
        };
        let value = sample_float_channel(channel, time);
        match channel.target {
            FloatTarget::Alpha => {
                let q = alpha_q.get_or_insert_with(|| world.query_mut::<AnimatedAlpha>());
                if let Some(q) = q.as_mut() {
                    if let Some(a) = q.get_mut(target_entity) {
                        a.0 = value;
                    }
                }
            }
            FloatTarget::UvOffsetU
            | FloatTarget::UvOffsetV
            | FloatTarget::UvScaleU
            | FloatTarget::UvScaleV
            | FloatTarget::UvRotation => {
                let q = uv_q.get_or_insert_with(|| world.query_mut::<AnimatedUvTransform>());
                if let Some(q) = q.as_mut() {
                    if let Some(t) = q.get_mut(target_entity) {
                        match channel.target {
                            FloatTarget::UvOffsetU => t.offset.x = value,
                            FloatTarget::UvOffsetV => t.offset.y = value,
                            FloatTarget::UvScaleU => t.scale.x = value,
                            FloatTarget::UvScaleV => t.scale.y = value,
                            FloatTarget::UvRotation => t.rotation = value,
                            _ => unreachable!(),
                        }
                    }
                }
            }
            FloatTarget::ShaderFloat => {
                let q = shader_q.get_or_insert_with(|| world.query_mut::<AnimatedShaderFloat>());
                if let Some(q) = q.as_mut() {
                    if let Some(s) = q.get_mut(target_entity) {
                        s.0 = value;
                    }
                }
            }
            FloatTarget::MorphWeight(idx) => {
                let q = morph_q.get_or_insert_with(|| world.query_mut::<AnimatedMorphWeights>());
                if let Some(q) = q.as_mut() {
                    if let Some(m) = q.get_mut(target_entity) {
                        m.set(idx as usize, value);
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

    // Single shared Name query handle — drives both the SubtreeCache
    // generation check and the NameIndex rebuild path. Pre-#827 the
    // prelude took THREE `world.query::<Name>()` acquisitions (two for
    // .len() and one for the rebuild iter); merging them halves the
    // RwLock fast-path traffic on the hot path and removes a fragile
    // "Name spawned between block 1 and block 2" inconsistency window
    // (today unreachable, but the pattern was brittle).
    let name_query = world.query::<Name>();
    let current_name_count = name_query.as_ref().map(|q| q.len()).unwrap_or(0);

    // Persisted subtree name maps — survives across frames, only cleared when
    // Name component count changes. Eliminates ~1500 HashMap insertions/frame
    // for typical animated scenes. #278.
    {
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
        let needs_rebuild = world
            .try_resource::<NameIndex>()
            .map(|idx| idx.generation != current_name_count)
            .unwrap_or(true);
        if needs_rebuild {
            // Reuse the prelude's shared `name_query` for the iter —
            // a `None` here means the Name storage has never existed,
            // which is the same `return` semantics the pre-#827 path
            // used at line 332.
            let Some(ref name_query) = name_query else {
                return;
            };
            // #824 — refill the existing HashMap in place instead of
            // allocating a fresh one and dropping the old. `clear()`
            // keeps the bucket array; `reserve(N)` forces one rehash
            // to a sufficient size on the cold-start path so the
            // refill doesn't growth-double through 0→1→2→...→N.
            // Name (component) and NameIndex (resource) live on
            // different storages — holding `name_query` read while
            // taking `idx` write is fine, no TypeId conflict.
            let mut idx = world.resource_mut::<NameIndex>();
            idx.map.clear();
            idx.map.reserve(current_name_count);
            for (entity, name_comp) in name_query.iter() {
                idx.map.insert(name_comp.0, entity);
            }
            idx.generation = current_name_count;
        }
    }
    drop(name_query);

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
            visit_text_key_events(clip, ps.prev_time, ps.current_time, |time, sym| {
                events.push(AnimationTextKeyEvent { label: sym, time });
            });
            if !events.is_empty() {
                // `events.clone()` instead of `mem::take` so the scratch
                // keeps its high-water-mark capacity across iterations
                // (#828). `AnimationTextKeyEvent` is Copy — the clone is
                // a memcpy of N × 8 bytes.
                eq.insert(ps.entity, AnimationTextKeyEvents(events.clone()));
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

        // Apply float channels — alpha + UV params + shader floats +
        // morph weights. See `apply_float_channels` for the per-target
        // dispatch table; pre-#525 only `Alpha` had a sink and every
        // other `FloatTarget` arm dropped its value silently.
        if !clip.float_channels.is_empty() {
            apply_float_channels(world, &clip.float_channels, current_time, &resolve_entity);
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
    // allocations (#251, #252, #828). Cleared at the start of each
    // iteration. Text-event scratches were originally declared inside
    // the loop and re-allocated every entity per frame — see #828.
    let mut channel_names_scratch: Vec<FixedString> = Vec::new();
    let mut updates_scratch: Vec<(FixedString, EntityId, Vec3, Quat, f32)> = Vec::new();
    use byroredux_scripting::events::AnimationTextKeyEvent;
    let mut events: Vec<AnimationTextKeyEvent> = Vec::new();
    let mut seen_labels: Vec<FixedString> = Vec::new();

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
        // Text-key event scratches (#339 / #231 / #828) live on the outer
        // closure scope; clear before each visit. `seen_labels` is also
        // cleared internally by `visit_stack_text_events`, but we clear
        // here too to keep the contract obvious.
        events.clear();
        seen_labels.clear();
        let accum_root: Option<FixedString>;
        let dominant_info: Option<(u32, f32)>;
        let stack_root: Option<EntityId>;
        {
            let sq = world.query::<AnimationStack>().unwrap();
            let stack = sq.get(entity).unwrap();
            stack_root = stack.root_entity;

            // Text key events (#211 / #339 / #231) — visitor form allocates
            // `AnimationTextKeyEvent` only when events actually fire. Labels
            // are passed through as interned `FixedString` symbols.
            visit_stack_text_events(stack, &registry, &mut seen_labels, |time, sym| {
                events.push(AnimationTextKeyEvent { label: sym, time });
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
        // `events.clone()` (not `mem::take`) so the scratch retains its
        // capacity across iterations — `mem::take` swaps in a zero-cap
        // Vec and forces the next iteration's visitor to grow from
        // scratch. `AnimationTextKeyEvent` is Copy. See #828.
        if !events.is_empty() {
            use byroredux_scripting::events::AnimationTextKeyEvents;
            let mut eq = world.query_mut::<AnimationTextKeyEvents>().unwrap();
            eq.insert(entity, AnimationTextKeyEvents(events.clone()));
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
                    apply_float_channels(world, &clip.float_channels, time, &stack_resolve);
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

/// M44 Phase 6 — cell-acoustics → reverb send wiring (#846 / AUD-D5-NEW-05).
///
/// Watches [`CellLightingRes::is_interior`] and updates
/// [`byroredux_audio::AudioWorld::set_reverb_send_db`] so interior
/// cells get a subtle wet send (`-12 dB`) and exteriors stay dry
/// (`f32::NEG_INFINITY`). Pre-fix the setter existed but no caller
/// flipped it, so every cell sounded identically dry regardless of
/// interior/exterior. The audit's Phase 6 promise (interior reverb
/// detector) lands here.
///
/// Idempotent — only writes on transitions (the bit-equality check
/// handles `NEG_INFINITY` cleanly), so the system is cheap to leave
/// running every frame and only touches `AudioWorld` on actual cell
/// type changes.
///
/// **Kira semantics**: already-playing sounds keep their construction-
/// time send level — the change applies to sounds dispatched AFTER
/// the call. For cell-load handoffs that's by design (the new cell's
/// ambients & one-shots get the new reverb routing). A long-running
/// ambient that survives an interior→exterior transition keeps its
/// original send level until it ends naturally; that's a known
/// limitation tracked in AUD-D5-NEW-06 (per-cell acoustic data).
///
/// No-ops cleanly when:
///   - `CellLightingRes` resource isn't registered yet (engine boot
///     before any cell load — the default send is already
///     `NEG_INFINITY`, so dry is correct).
///   - `AudioWorld` resource isn't registered (engine started without
///     audio wiring).
///
/// Runs in `Stage::Late` alongside `audio_system` (registered first
/// in main.rs so the level is in place before any new spatial track
/// gets constructed this frame).
pub(crate) fn reverb_zone_system(world: &World, _dt: f32) {
    /// Subtle interior wet — matches `set_reverb_send_db` doc.
    /// `-6 dB` is more pronounced; `-12 dB` is the audit's call.
    const INTERIOR_REVERB_SEND_DB: f32 = -12.0;
    /// Exteriors stay dry — silent send (well below the `-60 dB`
    /// `with_send` cutoff in the audio crate).
    const EXTERIOR_REVERB_SEND_DB: f32 = f32::NEG_INFINITY;

    let is_interior = {
        let Some(cell_lit) = world.try_resource::<CellLightingRes>() else {
            return;
        };
        cell_lit.is_interior
    };
    let target_db = if is_interior {
        INTERIOR_REVERB_SEND_DB
    } else {
        EXTERIOR_REVERB_SEND_DB
    };

    let Some(mut audio_world) = world.try_resource_mut::<byroredux_audio::AudioWorld>() else {
        return;
    };
    // Bit-equality so `NEG_INFINITY → NEG_INFINITY` short-circuits
    // without touching the field. (`==` would also work — IEEE 754
    // says `inf == inf` — but `to_bits()` makes the no-op intent
    // explicit and dodges any future signaling-NaN edge case.)
    if audio_world.reverb_send_db().to_bits() == target_db.to_bits() {
        return;
    }
    audio_world.set_reverb_send_db(target_db);
    log::info!(
        "M44 Phase 6: reverb send → {:.1} dB (interior={})",
        target_db,
        is_interior,
    );
}

/// M44 Phase 3.5 — footstep gameplay loop.
///
/// Walks every entity with a `FootstepEmitter`, accumulates horizontal
/// (XZ-plane) movement from frame to frame against
/// `stride_threshold`, and queues a one-shot via
/// `AudioWorld::play_oneshot` each time the stride threshold is
/// crossed. Vertical movement (jumping, falling, elevators) does
/// NOT count toward stride.
///
/// No-ops cleanly when:
///   - `FootstepConfig` resource isn't registered (engine started
///     without audio wiring).
///   - `FootstepConfig.default_sound` is `None` (BSA-load failed
///     at startup; e.g. running without game data).
///   - `AudioWorld` is inactive (no audio device).
///   - The first tick on a fresh `FootstepEmitter` — the system seeds
///     `last_position` from the current pose without firing, so we
///     don't emit a "phantom footstep" against the default zero pose.
///
/// Spawn a `FootstepEmitter` on the player entity to opt in. The
/// fly-camera attach is wired in `main.rs::App::new`.
pub(crate) fn footstep_system(world: &World, _dt: f32) {
    use crate::components::{FootstepConfig, FootstepEmitter, FootstepScratch};

    let Some(config) = world.try_resource::<FootstepConfig>() else {
        return;
    };
    let Some(sound) = config.default_sound.clone() else {
        return;
    };
    let volume = config.volume;
    drop(config);

    // Phase 1: walk every emitter, accumulate stride, collect the
    // positions where a footstep should fire this tick. Holding
    // GlobalTransform read + FootstepEmitter write concurrently is
    // fine (separate storages), but we want to release both locks
    // before touching `AudioWorld` in Phase 2 — minimises contention
    // with `audio_system` running in the same stage.
    //
    // The triggers buffer is held in `FootstepScratch` (a Resource)
    // and `clear()`-reused across frames — pre-#932 a fresh
    // `Vec<Vec3>` was allocated every frame even when no NPCs were
    // walking. The buffer is sized 32 in `FootstepScratch::default`
    // to cover the typical 5–10 / peak ~50 walking-NPC range
    // without re-growing.
    let Some(mut scratch) = world.try_resource_mut::<FootstepScratch>() else {
        return;
    };
    scratch.triggers.clear();
    {
        let Some(gt_q) = world.query::<GlobalTransform>() else {
            return;
        };
        let Some(mut fs_q) = world.query_mut::<FootstepEmitter>() else {
            return;
        };
        for (entity, fs) in fs_q.iter_mut() {
            let Some(gt) = gt_q.get(entity) else {
                continue;
            };
            let pos = gt.translation;
            if !fs.initialised {
                fs.last_position = pos;
                fs.initialised = true;
                continue;
            }
            // XZ-plane delta only — vertical (Y) motion isn't a step.
            let dx = pos.x - fs.last_position.x;
            let dz = pos.z - fs.last_position.z;
            let horizontal = (dx * dx + dz * dz).sqrt();
            fs.accumulated_stride += horizontal;
            fs.last_position = pos;
            if fs.accumulated_stride >= fs.stride_threshold {
                fs.accumulated_stride = 0.0;
                scratch.triggers.push(pos);
            }
        }
    }

    // Phase 2: dispatch one-shots for every triggered stride.
    if scratch.triggers.is_empty() {
        return;
    }
    // Drop the scratch lock BEFORE acquiring AudioWorld — both are
    // resource-mut locks, holding both at once would force a strict
    // TypeId-sorted acquisition contract. Drain the scratch into a
    // local before releasing it (cheap — Vec move, no allocation).
    let triggers = std::mem::take(&mut scratch.triggers);
    drop(scratch);

    let Some(mut audio_world) = world.try_resource_mut::<byroredux_audio::AudioWorld>() else {
        // Audio gone — restore the scratch buffer for next frame
        // (preserves the heap allocation) and bail. Re-acquiring the
        // scratch lock here costs one resource_mut hop, but loses the
        // capacity otherwise.
        if let Some(mut scratch) = world.try_resource_mut::<FootstepScratch>() {
            scratch.triggers = triggers;
        }
        return;
    };
    for pos in &triggers {
        audio_world.play_oneshot(
            std::sync::Arc::clone(&sound),
            *pos,
            byroredux_audio::Attenuation {
                // Tighter attenuation than the default — footsteps
                // drop off fast in real environments. 0.5m → full
                // volume, 12m → inaudible.
                min_distance: 0.5,
                max_distance: 12.0,
            },
            volume,
        );
    }
    drop(audio_world);

    // Restore the scratch buffer (with its persisted capacity) so
    // next frame's `clear()` doesn't strand the allocation.
    if let Some(mut scratch) = world.try_resource_mut::<FootstepScratch>() {
        scratch.triggers = triggers;
    }
}

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

    // Single GlobalTransform write query — `get` reads the camera GT
    // through the same handle that drives the billboard writes below.
    // Pre-#829 the system cycled a read lock + write lock on the same
    // storage every frame; the read-then-write pair burned ~50–100 ns
    // and a Vec allocation in release (compounding with #823) plus
    // opened a window for a future deadlock if the prelude grew
    // another acquisition between the two.
    let Some(mut gq) = world.query_mut::<GlobalTransform>() else {
        return;
    };
    let Some(cam_global) = gq.get(cam_entity).copied() else {
        return;
    };
    let cam_pos = cam_global.translation;
    // Camera forward = rotation * -Z (see Camera::view_matrix).
    let cam_forward = cam_global.rotation * -Vec3::Z;

    let Some(bq) = world.query::<Billboard>() else {
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
    // (GlobalTransform::len(), Parent::len(), World::next_entity_id()) —
    // generation key for the cached `roots` set. Mirrors the
    // `transform_propagation_system` pattern from #825 — same root
    // discovery anti-pattern, different storage. See #826.
    let mut last_seen_roots: Option<(usize, usize, EntityId)> = None;

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
        post_order.clear();
        stack.clear();

        {
            let Some(tq) = world.query::<GlobalTransform>() else {
                return;
            };
            let parent_q = world.query::<Parent>();
            // Cache the root set across frames; re-scan only when the
            // (GlobalTransform::len, Parent::len, next_entity_id) key
            // moves. Steady-state interior cells touch ~6 k
            // GlobalTransforms with ~30 roots — pre-#826 this rescanned
            // every frame at ~250 µs, on top of the same waste in
            // transform_propagation_system (#825). Same generation
            // pattern as `NameIndex` / `transform_propagation`.
            let key = (
                tq.len(),
                parent_q.as_ref().map(|q| q.len()).unwrap_or(0),
                world.next_entity_id(),
            );
            if last_seen_roots != Some(key) {
                roots.clear();
                for (entity, _) in tq.iter() {
                    let is_root = parent_q
                        .as_ref()
                        .map(|pq| pq.get(entity).is_none())
                        .unwrap_or(true);
                    if is_root {
                        roots.push(entity);
                    }
                }
                last_seen_roots = Some(key);
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
    let Some((gt_q, mut em_q)) = world.query_2_mut::<GlobalTransform, ParticleEmitter>() else {
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

/// Walk a `build_tod_keys` table at `hour` and return the bracketing
/// `(slot_a, slot_b, t)` tuple for piecewise-linear palette + fog
/// interpolation. `t` is the fraction along the `[slot_a → slot_b]`
/// segment; pre/post-key hours land on the wrap segment
/// `keys[last] → keys[0] + 24`.
///
/// Hoisted out of `weather_system` so the current snapshot walk and
/// the WTHR cross-fade target walk share one implementation —
/// REN-D15-NEW-05 (audit `2026-05-09`).
pub(crate) fn pick_tod_pair(keys: &[(f32, usize); 7], hour: f32) -> (usize, usize, f32) {
    // Wrap pre-midnight hours (e.g. 0.5) into the [1, 25) range so the
    // last-key → first-key wrap segment is reachable from a single
    // monotonic compare below.
    let h = if hour < keys[0].0 { hour + 24.0 } else { hour };
    let last = keys.len() - 1;
    let mut found = (keys[last].1, keys[0].1, 0.0f32);
    for i in 0..last {
        let (h0, s0) = keys[i];
        let (h1, s1) = keys[i + 1];
        if h >= h0 && h < h1 {
            found = (s0, s1, (h - h0) / (h1 - h0));
            break;
        }
    }
    // After last key (typically 22h+): interpolate night → midnight.
    if h >= keys[last].0 {
        let h0 = keys[last].0;
        let h1 = keys[0].0 + 24.0;
        let frac = ((h - h0) / (h1 - h0)).clamp(0.0, 1.0);
        found = (keys[last].1, keys[0].1, frac);
    }
    found
}

/// Map a TOD slot to its `night_factor` contribution in `[0.0, 1.0]`
/// (`0.0 = full daytime fog distance, 1.0 = full night fog distance`).
/// Used by `weather_system` to lerp fog distance through the same TOD
/// slot pair the colour interpolator just walked, keeping palette and
/// fog in lockstep.
///
/// Pre-#897 the fog distance used hardcoded hour breakpoints (6, 18,
/// 20, 4) while colours used the climate-driven `build_tod_keys` table.
/// On non-default-hour CLMTs (FO3 Capital Wasteland's `[5.333, 10, 17,
/// 22]` is the canonical case) the palette transitioned at the
/// authored hours while fog snapped at 6/18 — palette and fog
/// disagreed on "day" vs "transitioning" for ~0.3-2h windows. See #897
/// / REN-D15-01.
///
/// Slot mapping:
/// - `TOD_DAY`, `TOD_HIGH_NOON` → `0.0` (full day fog)
/// - `TOD_NIGHT`, `TOD_MIDNIGHT` → `1.0` (full night fog)
/// - `TOD_SUNRISE`, `TOD_SUNSET` → `0.5` (half-transitioned — the
///   per-key lerp toward the adjacent DAY/NIGHT slot completes the
///   smooth transition)
pub(crate) fn tod_slot_night_factor(slot: usize) -> f32 {
    use byroredux_plugin::esm::records::weather::*;
    if slot == TOD_DAY || slot == TOD_HIGH_NOON {
        0.0
    } else if slot == TOD_NIGHT || slot == TOD_MIDNIGHT {
        1.0
    } else {
        // TOD_SUNRISE / TOD_SUNSET — half-transitioned. The lerp
        // through `(slot_a, slot_b, t)` covers [0.5, 0.0] (sunrise→day)
        // and [0.5, 1.0] (sunset→night) smoothly.
        0.5
    }
}

/// Weather & time-of-day system: advances game clock, interpolates WTHR
/// NAM0 sky colors, computes sun arc, and updates SkyParamsRes + CellLightingRes.
///
/// Only runs when WeatherDataRes + GameTimeRes exist (exterior cells with weather).
///
/// M33.1 — when `WeatherTransitionRes` is present, the system blends the
/// per-TOD-sampled colours between the current `WeatherDataRes` and the
/// transition's `target` snapshot by `t = elapsed_secs / duration_secs`.
/// Each weather is independently TOD-sampled (so the transition stays
/// correct across midnight wraps where each side might land on a
/// different slot); only the final per-channel lerp uses `t`. When the
/// transition completes (`t >= 1.0`) the resource is removed and the
/// live `WeatherDataRes` is replaced with `target` for subsequent frames.
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

    // M33.1 — advance the in-flight WTHR cross-fade timer (if any) and
    // capture the blend weight + finished flag for use below. When the
    // transition completes we swap WeatherDataRes to the target snapshot
    // and drop the transition resource.
    let (transition_t, transition_done) =
        if let Some(mut tr) = world.try_resource_mut::<WeatherTransitionRes>() {
            tr.elapsed_secs += dt;
            let dur = tr.duration_secs.max(1e-3);
            let t = (tr.elapsed_secs / dur).clamp(0.0, 1.0);
            (t, t >= 1.0)
        } else {
            (0.0, false)
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
    let (slot_a, slot_b, t) = pick_tod_pair(&keys, hour);

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
    // #541 — SKY_LOWER drives `composite.frag`'s below-horizon
    // branch in lieu of the pre-fix `horizon * 0.3` fake.
    let lower = lerp3(
        wd.sky_colors[SKY_LOWER][slot_a],
        wd.sky_colors[SKY_LOWER][slot_b],
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

    // Fog distance: lerp between day and night fog based on the same
    // TOD slot pair the colour interpolator just walked. Pre-#897 this
    // used hardcoded hour breakpoints (6, 18, 20, 4) which disagreed
    // with the climate-driven colour breakpoints on non-default CLMTs
    // (FO3 Capital Wasteland's earlier sunrise was the canonical case
    // — palette transitioned at hour 5.333 while fog snapped at 6.0).
    // Sharing `(slot_a, slot_b, t)` keeps fog distance in lockstep with
    // sky palette across every shipped CLMT. See #897 / REN-D15-01.
    let night_a = tod_slot_night_factor(slot_a);
    let night_b = tod_slot_night_factor(slot_b);
    let night_factor = night_a + (night_b - night_a) * t;
    let fog_near = wd.fog[0] + (wd.fog[2] - wd.fog[0]) * night_factor;
    let fog_far = wd.fog[1] + (wd.fog[3] - wd.fog[1]) * night_factor;

    // M33.1 — if a WTHR cross-fade is in flight, run the same TOD-slot
    // pick + per-group sampling on the target snapshot and blend each
    // colour channel by `transition_t`. The TOD slots are independent
    // per-side (target may use the same `keys` table since `tod_hours`
    // is on WeatherDataRes; we re-derive it from the target's own
    // breakpoints to stay correct if the target ships a different CLMT).
    let (zenith, horizon, lower, sun_col, ambient, sunlight, fog_col, fog_near, fog_far) =
        if transition_t > 0.0 {
            let tr = world
                .try_resource::<WeatherTransitionRes>()
                .expect("transition_t > 0 implies WeatherTransitionRes");
            let target = &tr.target;

            let keys_b = build_tod_keys(target.tod_hours);
            let (b_a, b_b, b_t) = pick_tod_pair(&keys_b, hour);

            let target_zenith = lerp3(
                target.sky_colors[SKY_UPPER][b_a],
                target.sky_colors[SKY_UPPER][b_b],
                b_t,
            );
            let target_horizon = lerp3(
                target.sky_colors[SKY_HORIZON][b_a],
                target.sky_colors[SKY_HORIZON][b_b],
                b_t,
            );
            // #541 — `SKY_LOWER` cross-fades with the rest of the sky
            // colour set during a WTHR transition.
            let target_lower = lerp3(
                target.sky_colors[SKY_LOWER][b_a],
                target.sky_colors[SKY_LOWER][b_b],
                b_t,
            );
            let target_sun_col = lerp3(
                target.sky_colors[SKY_SUN][b_a],
                target.sky_colors[SKY_SUN][b_b],
                b_t,
            );
            let target_ambient = lerp3(
                target.sky_colors[SKY_AMBIENT][b_a],
                target.sky_colors[SKY_AMBIENT][b_b],
                b_t,
            );
            let target_sunlight = lerp3(
                target.sky_colors[SKY_SUNLIGHT][b_a],
                target.sky_colors[SKY_SUNLIGHT][b_b],
                b_t,
            );
            let target_fog_col = lerp3(
                target.sky_colors[SKY_FOG][b_a],
                target.sky_colors[SKY_FOG][b_b],
                b_t,
            );
            let target_fog_near = target.fog[0] + (target.fog[2] - target.fog[0]) * night_factor;
            let target_fog_far = target.fog[1] + (target.fog[3] - target.fog[1]) * night_factor;

            let lerp1 = |a: f32, b: f32, k: f32| a + (b - a) * k;
            (
                lerp3(zenith, target_zenith, transition_t),
                lerp3(horizon, target_horizon, transition_t),
                lerp3(lower, target_lower, transition_t),
                lerp3(sun_col, target_sun_col, transition_t),
                lerp3(ambient, target_ambient, transition_t),
                lerp3(sunlight, target_sunlight, transition_t),
                lerp3(fog_col, target_fog_col, transition_t),
                lerp1(fog_near, target_fog_near, transition_t),
                lerp1(fog_far, target_fog_far, transition_t),
            )
        } else {
            (
                zenith, horizon, lower, sun_col, ambient, sunlight, fog_col, fog_near, fog_far,
            )
        };

    // Sun direction: semicircular arc from east (6h) through zenith (12h) to west (18h).
    // Below horizon at night. Y-up coordinate system.
    let sun_dir = {
        // Solar angle: 0 at sunrise (6h), π at sunset (18h).
        let solar_hour = (hour - 6.0).clamp(0.0, 12.0);
        let angle = solar_hour / 12.0 * std::f32::consts::PI;
        // Sun arcs from east (+X) through up (+Y) to west (-X) with a
        // slight south tilt — #802 / SUN-N2. Per the Z-up → Y-up swap
        // in `crates/nif/src/import/coord.rs:18` (`(x, y, z) → (x, z, -y)`),
        // Bethesda's authored +Y (north) maps to engine -Z, so SOUTH is
        // engine +Z. All four Bethesda settings (Mojave, Capital
        // Wasteland, Tamriel, Commonwealth) sit at NH latitudes where
        // the real sun arcs through the southern sky; pre-#802 this
        // constant was -0.15 (a NORTH tilt) despite the comment
        // claiming south.
        let x = angle.cos();
        let y = angle.sin();
        let z = 0.15_f32; // slight south tilt (engine +Z = Bethesda -Y = south)
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

    // Cloud layer 0 scroll rate. Pre-#535 the rate was "derived" from
    // `wd.cloud_speeds[0] / 128.0 * 0.02`, but that byte was actually
    // the first character of the DNAM cloud-path zstring (typically
    // `'s'` = 0x73 = 115 → factor 0.898 → ≈0.018 UV/sec). The visible
    // result looked fine because the authored constant was close, so
    // keep it here as a named baseline while the real per-weather
    // scroll source stays unknown. WTHR has ONAM (4 B, looks f32-ish)
    // and INAM (304 B, per-image transition data) that plausibly carry
    // the speed; sourcing that is deferred — cross-cuts #541's
    // "unused WTHR fields" scope and needs UESP-authoritative byte
    // sampling before committing to an offset.
    let cloud_scroll_rate: f32 = 0.018;

    drop(wd);

    // Update SkyParamsRes.
    if let Some(mut sky) = world.try_resource_mut::<SkyParamsRes>() {
        sky.zenith_color = zenith;
        sky.horizon_color = horizon;
        // #541 — SKY_LOWER drives the renderer's below-horizon
        // gradient. Pre-fix the value was discarded and the shader
        // faked it as `horizon * 0.3`.
        sky.lower_color = lower;
        sky.sun_color = sun_col;
        sky.sun_direction = sun_dir;
        sky.sun_intensity = sun_intensity;
    }

    // #803 — cloud scroll lives on `CloudSimState`, which survives
    // cell transitions (unlike `SkyParamsRes`, which `unload_cell`
    // removes on every cell unload). Writing here keeps the
    // accumulator alive across interior visits so the renderer's
    // next-frame sample lands at the same UV the player saw before
    // entering the interior, rather than snapping back to origin.
    //
    // Wrap scroll at 1.0 so it never grows unboundedly; sampler
    // REPEAT makes the wrap invisible.
    if let Some(mut clouds) = world.try_resource_mut::<CloudSimState>() {
        clouds.cloud_scroll[0] = (clouds.cloud_scroll[0] + cloud_scroll_rate * dt).rem_euclid(1.0);
        clouds.cloud_scroll[1] =
            (clouds.cloud_scroll[1] + cloud_scroll_rate * 0.3 * dt).rem_euclid(1.0);
        // Layer 1 drifts in the opposite U direction at 1.35× speed.
        // Creates visible parallax against layer 0 with no per-weather
        // source needed. See #541 (ONAM/INAM decode) for eventual
        // authoritative values.
        clouds.cloud_scroll_1[0] =
            (clouds.cloud_scroll_1[0] - cloud_scroll_rate * 1.35 * dt).rem_euclid(1.0);
        clouds.cloud_scroll_1[1] =
            (clouds.cloud_scroll_1[1] + cloud_scroll_rate * 0.5 * dt).rem_euclid(1.0);
        // Layer 2 (WTHR ANAM) and layer 3 (BNAM) used to mirror layer 0
        // and layer 1 verbatim — when ANAM/BNAM resolved to the same
        // texture as DNAM/CNAM (or were absent), the four-layer composite
        // collapsed to two visually identical pairs. Until WTHR ONAM
        // (4 B, looks f32-ish) and INAM (304 B per-image transition data)
        // are decoded as the authoritative per-weather scroll source,
        // pick distinct multipliers so the four layers always have four
        // visibly different drifts. Slower base U on the high layers
        // matches the conventional cirrus-vs-stratus authoring pattern
        // (cirrus drifts slowly relative to the lower deck). #899.
        clouds.cloud_scroll_2[0] =
            (clouds.cloud_scroll_2[0] + cloud_scroll_rate * 0.85 * dt).rem_euclid(1.0);
        clouds.cloud_scroll_2[1] =
            (clouds.cloud_scroll_2[1] + cloud_scroll_rate * 0.45 * dt).rem_euclid(1.0);
        clouds.cloud_scroll_3[0] =
            (clouds.cloud_scroll_3[0] - cloud_scroll_rate * 1.15 * dt).rem_euclid(1.0);
        clouds.cloud_scroll_3[1] =
            (clouds.cloud_scroll_3[1] + cloud_scroll_rate * 0.6 * dt).rem_euclid(1.0);
    }

    // Update CellLightingRes — exterior cells only. Interior cells own
    // their own ambient / directional / fog values from XCLL or LGTM
    // records (see `scene.rs::load_cell` interior path); the weather
    // system would otherwise clobber them with sky-tinted exterior fog
    // and time-of-day-driven ambient/directional from the most recent
    // exterior worldspace, producing visibly wrong lighting on every
    // interior cell loaded after any exterior session. See #782.
    if let Some(mut cell_lit) = world.try_resource_mut::<CellLightingRes>() {
        if !cell_lit.is_interior {
            cell_lit.ambient = ambient;
            cell_lit.directional_color = sunlight;
            cell_lit.directional_dir = sun_dir;
            cell_lit.fog_color = fog_col;
            cell_lit.fog_near = fog_near;
            cell_lit.fog_far = fog_far;
        }
    }

    // M33.1 — promote the in-flight transition target into the live
    // WeatherDataRes once the cross-fade completes. Uses in-place
    // mutation via try_resource_mut (interior mutability, &World safe).
    // elapsed_secs is saturated at duration_secs so subsequent frames
    // skip the blend path without removing the resource (remove_resource
    // needs &mut World which systems do not have).
    if transition_done {
        if let Some(tr) = world.try_resource::<WeatherTransitionRes>() {
            let new_sky = tr.target.sky_colors;
            let new_fog = tr.target.fog;
            let new_tod = tr.target.tod_hours;
            drop(tr);
            if let Some(mut wd) = world.try_resource_mut::<WeatherDataRes>() {
                wd.sky_colors = new_sky;
                wd.fog = new_fog;
                wd.tod_hours = new_tod;
            }
            // Set duration to infinity so t = elapsed/duration = 0.0 from
            // now on — the transition is permanently dormant without removal.
            if let Some(mut tr) = world.try_resource_mut::<WeatherTransitionRes>() {
                tr.elapsed_secs = 0.0;
                tr.duration_secs = f32::INFINITY;
            }
        }
    }
}

/// Submersion detection — write `SubmersionState` onto the active
/// camera entity each frame.
///
/// Tests every `WaterPlane` entity in the world against the camera's
/// world position; when the camera falls inside a [`WaterVolume`]'s
/// horizontal extent and below the plane's surface height, the
/// computed depth + selected material are written through.
///
/// MVP scope:
///
/// - Only the active camera receives `SubmersionState`. Actors are a
///   follow-up once the actor controller lands (gameplay layer
///   reads `head_submerged` to switch to swim state).
/// - Linear scan over `WaterPlane` entities. Cells ship 1–3 water
///   planes max; a broadphase would only matter once we hit
///   dozens.
/// - `head_submerged` is computed at zero offset for cameras (the
///   eye is the submerged surface). The component still carries the
///   bool for downstream uniformity with the actor path.
pub(crate) fn submersion_system(world: &World, _dt: f32) {
    let Some(active) = world.try_resource::<ActiveCamera>() else {
        return;
    };
    let cam_entity = active.0;
    drop(active);

    let Some(gq) = world.query::<GlobalTransform>() else {
        return;
    };
    let Some(cam_global) = gq.get(cam_entity).copied() else {
        return;
    };
    let cam_pos = cam_global.translation;
    drop(gq);

    // Snapshot every active water plane's volume + material. We
    // re-acquire GlobalTransform here only to confirm the plane's
    // world Y matches its volume `max.y` (defensive — `WaterVolume`
    // is authored at spawn time so the two should already agree).
    let mut best: Option<(f32, SubmersionState)> = None;
    let Some(wq) = world.query::<WaterPlane>() else {
        // No water entities at all → clear any prior state on the
        // camera so the next frame's render reads default-above-water.
        if let Some(mut sq) = world.query_mut::<SubmersionState>() {
            if let Some(state) = sq.get_mut(cam_entity) {
                *state = SubmersionState::default();
            }
        }
        return;
    };
    let Some(vq) = world.query::<WaterVolume>() else {
        return;
    };
    for (entity, plane) in wq.iter() {
        let Some(volume) = vq.get(entity) else {
            continue;
        };
        // Full 3-D AABB containment. The previous version checked
        // only the horizontal extent + a "below the surface"
        // condition, which mis-flagged cameras that sat far below
        // a water plane (e.g., outdoor cell with a tiny pond plane
        // authored high above some other piece of terrain the
        // camera happens to share an XZ column with). Requiring
        // `cam_pos.y >= volume.min.y` rejects those — to be
        // underwater you must be inside the actual water column.
        if cam_pos.x < volume.min[0]
            || cam_pos.x > volume.max[0]
            || cam_pos.y < volume.min[1]
            || cam_pos.y > volume.max[1]
            || cam_pos.z < volume.min[2]
            || cam_pos.z > volume.max[2]
        {
            continue;
        }
        // Surface is at volume.max.y; the AABB pre-test already
        // ensured cam_pos.y ≤ surface_y, so depth is always ≥ 0
        // here. No further sign check needed.
        let surface_y = volume.max[1];
        let depth = surface_y - cam_pos.y;
        // Pick the closest match (smallest depth wins — for nested
        // / overlapping water volumes, the one closest to the camera
        // controls the underwater FX).
        let candidate = (
            depth,
            SubmersionState {
                depth,
                head_submerged: depth > 0.0,
                material: Some(plane.material),
            },
        );
        match best {
            None => best = Some(candidate),
            Some((prev_depth, _)) if depth < prev_depth => best = Some(candidate),
            _ => {}
        }
    }
    drop(wq);
    drop(vq);

    let new_state = best.map(|(_, s)| s).unwrap_or_default();
    let Some(mut sq) = world.query_mut::<SubmersionState>() else {
        return;
    };
    // `SubmersionState` is inserted on the camera entity at setup
    // time (see scene.rs camera spawn). If the component is somehow
    // missing, skip silently — structural inserts mid-frame would
    // require `&mut World` and we keep this system on the pure-
    // mutation path with the rest of the per-frame systems.
    if let Some(state) = sq.get_mut(cam_entity) {
        // One-time-per-transition log. Catches the "everything
        // underwater" failure mode where a misplaced water plane
        // flags the camera as submerged on cells where the player
        // is clearly above ground. Logs at INFO so it's visible
        // without raising the global log level.
        let was = state.head_submerged;
        let now = new_state.head_submerged;
        if was != now {
            if now {
                log::info!(
                    "submersion: ENTER underwater — depth={:.1} cam=({:.1}, {:.1}, {:.1})",
                    new_state.depth,
                    cam_pos.x,
                    cam_pos.y,
                    cam_pos.z,
                );
            } else {
                log::info!(
                    "submersion: EXIT underwater — cam=({:.1}, {:.1}, {:.1})",
                    cam_pos.x,
                    cam_pos.y,
                    cam_pos.z,
                );
            }
        }
        *state = new_state;
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

    /// Regression test for #826: the cached root set must invalidate
    /// when the scene-graph topology changes between frames. Mirrors
    /// the sibling test for `transform_propagation_system` (#825). All
    /// three transitions (new root spawned, child-gains-Parent,
    /// child-loses-Parent) move the
    /// `(GlobalTransform::len, Parent::len, next_entity_id)` key, so
    /// each must trigger a rescan and a corrected post-order walk.
    #[test]
    fn root_cache_invalidates_on_topology_change() {
        let mut world = World::new();
        // Initial: one root with one child leaf at (-10, 0, 0) r=1.
        let parent = world.spawn();
        world.insert(parent, GlobalTransform::IDENTITY);
        world.insert(parent, WorldBound::ZERO);

        let leaf = spawn_leaf(&mut world, Vec3::new(-10.0, 0.0, 0.0), 1.0, Vec3::ZERO, 1.0);
        world.insert(leaf, Parent(parent));
        world.insert(parent, Children(vec![leaf]));

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);
        let parent_wb_initial = *world.query::<WorldBound>().unwrap().get(parent).unwrap();
        assert!((parent_wb_initial.center - Vec3::new(-10.0, 0.0, 0.0)).length() < 1e-5);

        // 1) Spawn a NEW top-level root (unrelated). Cache key
        //    (GlobalTransform::len) bumps; rescan must include it
        //    even though `parent` already had an entry.
        let new_root = spawn_leaf(&mut world, Vec3::new(50.0, 0.0, 0.0), 1.0, Vec3::ZERO, 2.0);
        sys(&world, 0.016);
        let new_root_wb = *world.query::<WorldBound>().unwrap().get(new_root).unwrap();
        assert!(
            (new_root_wb.center - Vec3::new(50.0, 0.0, 0.0)).length() < 1e-5,
            "new root must be discovered after cache invalidation, got {:?}",
            new_root_wb.center
        );

        // 2) Add a SECOND child to `parent` — Parent::len bumps. The
        //    parent's WorldBound must re-fold to enclose both leaves.
        let leaf2 = spawn_leaf(&mut world, Vec3::new(10.0, 0.0, 0.0), 1.0, Vec3::ZERO, 1.0);
        world.insert(leaf2, Parent(parent));
        world.insert(parent, Children(vec![leaf, leaf2]));
        sys(&world, 0.016);
        let parent_wb = *world.query::<WorldBound>().unwrap().get(parent).unwrap();
        assert!(
            (parent_wb.center - Vec3::ZERO).length() < 1e-5,
            "parent center should be midpoint after second child added, got {:?}",
            parent_wb.center
        );
        assert!(
            (parent_wb.radius - 11.0).abs() < 1e-5,
            "parent radius should enclose both leaves (r=11), got {}",
            parent_wb.radius
        );

        // 3) Promote `leaf2` to root by removing its Parent. Parent::len
        //    drops; rescan must include it. After the walk, `parent`'s
        //    WorldBound should fall back to enclosing only `leaf`.
        world.remove::<Parent>(leaf2);
        world.insert(parent, Children(vec![leaf]));
        sys(&world, 0.016);
        let parent_wb_after = *world.query::<WorldBound>().unwrap().get(parent).unwrap();
        assert!(
            (parent_wb_after.center - Vec3::new(-10.0, 0.0, 0.0)).length() < 1e-5,
            "parent should re-fold to single child after promote, got {:?}",
            parent_wb_after.center
        );
    }

    /// Steady-state cache hit: with no topology change, the cached
    /// root set must still drive a correct post-order walk so leaf
    /// transform changes propagate up to parent bounds (the
    /// counterpart to `root_cache_steady_state_still_runs_propagation`
    /// in #825 — confirms cache hits don't stall pass 2).
    #[test]
    fn root_cache_steady_state_still_refolds_parent_bounds() {
        let mut world = World::new();
        let parent = world.spawn();
        world.insert(parent, GlobalTransform::IDENTITY);
        world.insert(parent, WorldBound::ZERO);

        let leaf = spawn_leaf(&mut world, Vec3::new(-10.0, 0.0, 0.0), 1.0, Vec3::ZERO, 1.0);
        world.insert(leaf, Parent(parent));
        world.insert(parent, Children(vec![leaf]));

        let mut sys = make_world_bound_propagation_system();
        sys(&world, 0.016);
        let initial = *world.query::<WorldBound>().unwrap().get(parent).unwrap();
        assert!((initial.center - Vec3::new(-10.0, 0.0, 0.0)).length() < 1e-5);

        // Move the leaf without any topology change — the cache key
        // stays valid, but pass 1 (leaf bound) and pass 2 (parent
        // fold) must still re-execute against the cached root.
        {
            let mut gq = world.query_mut::<GlobalTransform>().unwrap();
            let g = gq.get_mut(leaf).unwrap();
            g.translation = Vec3::new(20.0, 0.0, 0.0);
        }
        sys(&world, 0.016);
        let after = *world.query::<WorldBound>().unwrap().get(parent).unwrap();
        assert!(
            (after.center - Vec3::new(20.0, 0.0, 0.0)).length() < 1e-5,
            "parent bound must re-fold against cached root after leaf moved, got {:?}",
            after.center
        );
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
            [6.0, 10.0, 18.0, 22.0],  // FNV
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

    /// `tod_slot_night_factor` — the per-slot fog-distance contribution
    /// that pairs with `build_tod_keys` to keep fog in lockstep with
    /// the sky palette. DAY-class slots map to 0, NIGHT-class to 1,
    /// transition slots to 0.5 so the per-key lerp covers the
    /// half-transitioned span smoothly. See #897 / REN-D15-01.
    #[test]
    fn night_factor_full_day_slots_are_zero() {
        assert_eq!(tod_slot_night_factor(TOD_DAY), 0.0);
        assert_eq!(tod_slot_night_factor(TOD_HIGH_NOON), 0.0);
    }

    #[test]
    fn night_factor_full_night_slots_are_one() {
        assert_eq!(tod_slot_night_factor(TOD_NIGHT), 1.0);
        assert_eq!(tod_slot_night_factor(TOD_MIDNIGHT), 1.0);
    }

    #[test]
    fn night_factor_transition_slots_are_half() {
        // The midpoint values let the per-key lerp through
        // `(slot_a, slot_b, t)` cover SUNRISE→DAY (0.5→0.0) and
        // SUNSET→NIGHT (0.5→1.0) smoothly.
        assert_eq!(tod_slot_night_factor(TOD_SUNRISE), 0.5);
        assert_eq!(tod_slot_night_factor(TOD_SUNSET), 0.5);
    }

    /// Regression for #897 / REN-D15-01.
    ///
    /// Pre-fix: at hour 5.7 with FO3 Capital Wasteland-style climate
    /// (`tod_hours = [5.333, 10.0, 17.0, 22.0]`), the colour
    /// interpolator landed in the `(SUNRISE, DAY)` slot pair (palette
    /// = sunrise) while the hardcoded fog `night_factor` returned
    /// `(6.0 - 5.7) / 2.0 = 0.15` (fog mostly day) — palette and fog
    /// disagreed on "day" vs "transitioning" by ~0.3 h window.
    ///
    /// Post-fix: fog uses the same `(slot_a, slot_b, t)` tuple and the
    /// `tod_slot_night_factor` helper. At hour 5.7 the lerp from
    /// SUNRISE (0.5) toward DAY (0.0) at `t = (5.7 - 5.333) / (10.0
    /// - 5.333) ≈ 0.0786` produces `night_factor ≈ 0.461` —
    /// half-transitioned, matching the SUNRISE-class palette.
    #[test]
    fn fo3_wasteland_sunrise_fog_lockstep_with_palette() {
        let keys = build_tod_keys([5.333, 10.0, 17.0, 22.0]);
        let h = 5.7_f32;
        // Walk the keys exactly the way `weather_system` does.
        let mut slot_a = keys[keys.len() - 1].1;
        let mut slot_b = keys[0].1;
        let mut t = 0.0_f32;
        for i in 0..keys.len() - 1 {
            let (h0, s0) = keys[i];
            let (h1, s1) = keys[i + 1];
            if h >= h0 && h < h1 {
                slot_a = s0;
                slot_b = s1;
                t = (h - h0) / (h1 - h0);
                break;
            }
        }
        assert_eq!(
            slot_a, TOD_SUNRISE,
            "slot_a at FO3 hour 5.7 must be SUNRISE"
        );
        assert_eq!(slot_b, TOD_DAY, "slot_b at FO3 hour 5.7 must be DAY");
        let na = tod_slot_night_factor(slot_a);
        let nb = tod_slot_night_factor(slot_b);
        let night_factor = na + (nb - na) * t;
        assert!(
            night_factor > 0.4 && night_factor < 0.5,
            "night_factor at FO3 hour 5.7 must be half-transitioned \
             (in [0.4, 0.5]) so fog tracks the SUNRISE-class palette. \
             Pre-#897 hardcoded hours produced 0.15 here. \
             Got {night_factor:.3}",
        );
    }

    /// `pick_tod_pair` mid-segment — hour lands inside a key bracket
    /// and returns the surrounding slot pair plus the linear fraction.
    /// This is the common path every gameplay frame walks.
    #[test]
    fn pick_tod_pair_mid_segment_lerp() {
        let keys = build_tod_keys([6.0, 10.0, 18.0, 22.0]);
        // Hour 7.0 sits between SUNRISE (6.0) and DAY (10.0) → t = 0.25.
        let (a, b, t) = pick_tod_pair(&keys, 7.0);
        assert_eq!(a, TOD_SUNRISE);
        assert_eq!(b, TOD_DAY);
        assert!((t - 0.25).abs() < 1e-5, "expected t≈0.25, got {t}");
    }

    /// `pick_tod_pair` wrap branch — pre-midnight hours (< first key)
    /// must reach into the [last, first+24) wrap segment so the night
    /// → midnight blend stays smooth across the day boundary.
    #[test]
    fn pick_tod_pair_pre_midnight_wraps_into_night_segment() {
        let keys = build_tod_keys([6.0, 10.0, 18.0, 22.0]);
        // Hour 0.5 wraps to 24.5; falls inside NIGHT (23) → MIDNIGHT (25).
        let (a, b, t) = pick_tod_pair(&keys, 0.5);
        assert_eq!(a, TOD_NIGHT, "pre-midnight hour 0.5 wraps into NIGHT");
        assert_eq!(b, TOD_MIDNIGHT);
        // t = (24.5 - 23) / (25 - 23) = 0.75.
        assert!((t - 0.75).abs() < 1e-5, "expected t≈0.75, got {t}");
    }

    /// `pick_tod_pair` post-last-key branch — hour after the last
    /// authored key (typically 22h+) interpolates NIGHT → MIDNIGHT
    /// through the same wrap segment as the pre-midnight case.
    #[test]
    fn pick_tod_pair_post_night_anchor_returns_night_to_midnight() {
        let keys = build_tod_keys([6.0, 10.0, 18.0, 22.0]);
        // Hour 24.0 (equivalently 0.0 next day, but the wrap normalizes
        // pre-keys[0]; this test hits the >= keys[last] branch directly).
        let (a, b, t) = pick_tod_pair(&keys, 23.5);
        assert_eq!(a, TOD_NIGHT);
        assert_eq!(b, TOD_MIDNIGHT);
        assert!(t > 0.0 && t <= 1.0);
    }

    /// Default FNV-style climate at noon must yield zero night_factor
    /// (the easy case — both sides DAY-class, lerp stays at 0).
    #[test]
    fn fnv_default_noon_fog_is_full_day() {
        let keys = build_tod_keys([6.0, 10.0, 18.0, 22.0]);
        let h = 12.0_f32;
        let mut slot_a = keys[0].1;
        let mut slot_b = keys[0].1;
        let mut t = 0.0_f32;
        for i in 0..keys.len() - 1 {
            let (h0, s0) = keys[i];
            let (h1, s1) = keys[i + 1];
            if h >= h0 && h < h1 {
                slot_a = s0;
                slot_b = s1;
                t = (h - h0) / (h1 - h0);
                break;
            }
        }
        let na = tod_slot_night_factor(slot_a);
        let nb = tod_slot_night_factor(slot_b);
        let night_factor = na + (nb - na) * t;
        assert_eq!(
            night_factor, 0.0,
            "noon must produce full-day fog (both endpoints DAY-class)"
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

/// Regression tests for #782 — `weather_system` was unconditionally
/// writing time-of-day-derived `ambient` / `directional` / `fog_color`
/// (etc.) into `CellLightingRes` regardless of whether the active cell
/// was interior or exterior. Interior cells loaded after any exterior
/// session inherited the most-recent WTHR fog tint (typically sky-blue
/// `[0.65, 0.7, 0.8]`) instead of their own XCLL-authored fog. The
/// composite pass blended that into distant pixels at up to 70%
/// opacity in HDR linear space pre-ACES, producing a visibly chromy /
/// posterized look on every distant interior surface.
///
/// The fix gates all six `cell_lit.*` writes on `!is_interior` —
/// interior cells preserve their XCLL/LGTM-authored values from the
/// cell loader; exterior cells continue to be driven by weather TOD.
#[cfg(test)]
mod weather_interior_gate_tests {
    use super::*;
    use byroredux_core::ecs::World;

    /// Insert the minimum resource set that lets `weather_system` reach
    /// the `CellLightingRes` update without early-returning, with a
    /// `WeatherDataRes` populated to a deliberately bright sky-blue
    /// fog so any leak into `cell_lit.fog_color` is unambiguous.
    fn build_world(is_interior: bool) -> World {
        let mut world = World::new();

        // Interior fog the cell loader supposedly placed — a dim
        // brownish tint that we expect to survive `weather_system`.
        const INTERIOR_FOG_COLOR: [f32; 3] = [0.05, 0.06, 0.08];
        const INTERIOR_FOG_NEAR: f32 = 64.0;
        const INTERIOR_FOG_FAR: f32 = 4000.0;

        world.insert_resource(CellLightingRes {
            ambient: [0.1, 0.1, 0.1],
            directional_color: [0.3, 0.3, 0.3],
            directional_dir: [0.0, 1.0, 0.0],
            is_interior,
            fog_color: INTERIOR_FOG_COLOR,
            fog_near: INTERIOR_FOG_NEAR,
            fog_far: INTERIOR_FOG_FAR,
            // Test fixture — extended XCLL fields not exercised here.
            directional_fade: None,
            fog_clip: None,
            fog_power: None,
            fog_far_color: None,
            fog_max: None,
            light_fade_begin: None,
            light_fade_end: None,
            directional_ambient: None,
            specular_color: None,
            specular_alpha: None,
            fresnel_power: None,
        });

        world.insert_resource(GameTimeRes {
            hour: 12.0,      // mid-day so the TOD slot is unambiguous
            time_scale: 0.0, // freeze the clock so dt advances are no-ops
        });

        // Build a WTHR snapshot with sky-blue fog at every TOD slot so
        // any unconditional write would clobber the interior fog with
        // (0.65, 0.7, 0.8) — the symptom from #782.
        let bright_sky_blue = [0.65_f32, 0.7, 0.8];
        let mut sky_colors = [[[0.0_f32; 3]; 6]; 10];
        for slot in 0..6 {
            sky_colors[byroredux_plugin::esm::records::weather::SKY_FOG][slot] = bright_sky_blue;
            sky_colors[byroredux_plugin::esm::records::weather::SKY_AMBIENT][slot] =
                [0.5, 0.5, 0.5];
            sky_colors[byroredux_plugin::esm::records::weather::SKY_SUNLIGHT][slot] =
                [1.0, 1.0, 1.0];
        }
        world.insert_resource(WeatherDataRes {
            sky_colors,
            fog: [100.0, 60000.0, 200.0, 30000.0],
            tod_hours: [6.0, 10.0, 18.0, 22.0],
        });

        world
    }

    /// Interior gate — `cell_lit.fog_color` (and the rest of the gated
    /// fields) must NOT change after `weather_system` runs against a
    /// world whose `CellLightingRes.is_interior == true`, even when
    /// `WeatherDataRes` carries a fog target wildly different from the
    /// XCLL-authored value.
    #[test]
    fn interior_cell_fog_is_not_overwritten_by_weather() {
        let world = build_world(true);
        weather_system(&world, 0.016);

        let cell_lit = world.try_resource::<CellLightingRes>().unwrap();
        assert_eq!(
            cell_lit.fog_color,
            [0.05, 0.06, 0.08],
            "interior fog_color was overwritten by weather_system — \
             #782 regression"
        );
        assert!(
            (cell_lit.fog_near - 64.0).abs() < 1e-5,
            "interior fog_near was overwritten — #782 regression"
        );
        assert!(
            (cell_lit.fog_far - 4000.0).abs() < 1e-5,
            "interior fog_far was overwritten — #782 regression"
        );
        // Sibling fields gated together with fog — same regression risk.
        assert_eq!(
            cell_lit.ambient,
            [0.1, 0.1, 0.1],
            "interior ambient was overwritten — #782 regression"
        );
        assert_eq!(
            cell_lit.directional_color,
            [0.3, 0.3, 0.3],
            "interior directional_color was overwritten — #782 regression"
        );
    }

    /// Exterior path still works — weather_system MUST update fog on
    /// exterior cells (otherwise sky-tinted fog never reaches the
    /// composite UBO at all). Negative test that pins the gate's
    /// `!is_interior` polarity.
    #[test]
    fn exterior_cell_fog_is_updated_by_weather() {
        let world = build_world(false);
        weather_system(&world, 0.016);

        let cell_lit = world.try_resource::<CellLightingRes>().unwrap();
        // Mid-day with the sky-blue fog at every slot — interpolator
        // returns the slot value unchanged.
        assert!(
            (cell_lit.fog_color[0] - 0.65).abs() < 1e-3,
            "exterior fog_color was not updated by weather_system: {:?}",
            cell_lit.fog_color
        );
        assert!(
            (cell_lit.fog_color[2] - 0.8).abs() < 1e-3,
            "exterior fog_color was not updated by weather_system: {:?}",
            cell_lit.fog_color
        );
    }
}

// ── #525 / FNV-ANIM-2 regression guards ───────────────────────────────
//
// Pre-#525 the float-channel dispatch in `animation_system` only had
// a sink for `FloatTarget::Alpha`. Every other variant (UvOffsetU/V,
// UvScaleU/V, UvRotation, ShaderFloat, MorphWeight) was sampled and
// then silently dropped — animated UV scrolling on water / lava /
// conveyor belts / HUD backdrops did nothing at runtime, and
// FaceGen lip-sync morphs likewise had no consumer.
//
// `apply_float_channels` now routes each arm to a dedicated sparse
// component. The tests exercise the helper directly with synthetic
// (target, value) channels so the dispatch table itself is pinned;
// full clip-registry/player wiring is covered by upstream animation
// integration tests.
#[cfg(test)]
mod float_channel_dispatch_tests {
    use super::*;
    use byroredux_core::animation::{AnimFloatKey, FloatChannel};
    use byroredux_core::ecs::World;

    /// Build a world with an entity carrying every float-channel sink
    /// pre-inserted at identity so `apply_float_channels` has a target
    /// for every dispatch arm. Returns the entity id, a fresh
    /// `StringPool` (only used to mint dummy `FixedString` keys for the
    /// channel name slot), and a `resolve_entity` closure that maps
    /// any name back to the entity.
    fn world_with_sinks() -> (World, EntityId, FixedString) {
        let mut world = World::new();
        let entity = world.spawn();
        world.insert(entity, AnimatedAlpha(1.0));
        world.insert(entity, AnimatedUvTransform::identity());
        world.insert(entity, AnimatedShaderFloat(0.0));
        world.insert(entity, AnimatedMorphWeights(Vec::new()));
        let mut pool = StringPool::new();
        let dummy = pool.intern("target");
        (world, entity, dummy)
    }

    /// Single-keyframe channel that always samples to `value` regardless
    /// of `time`. Mirrors how a constant-value controller authors a
    /// flat slider position.
    fn const_channel(target: FloatTarget, value: f32) -> FloatChannel {
        FloatChannel {
            target,
            keys: vec![AnimFloatKey { time: 0.0, value }],
        }
    }

    fn resolve_to(entity: EntityId) -> impl Fn(&FixedString) -> Option<EntityId> {
        move |_sym: &FixedString| Some(entity)
    }

    /// `FloatTarget::Alpha` keeps the pre-#525 behaviour — the only
    /// arm that already had a sink. Pinned here to guard against the
    /// helper accidentally dropping it during a future refactor.
    #[test]
    fn alpha_target_writes_animated_alpha() {
        let (world, entity, name) = world_with_sinks();
        let channels = vec![(name, const_channel(FloatTarget::Alpha, 0.5))];
        apply_float_channels(&world, &channels, 0.0, &resolve_to(entity));
        let q = world.query::<AnimatedAlpha>().unwrap();
        assert_eq!(q.get(entity).unwrap().0, 0.5);
    }

    /// `FloatTarget::UvOffsetU` writes `AnimatedUvTransform.offset.x`
    /// only — `offset.y` / `scale` / `rotation` stay at identity.
    /// Pre-#525 the value was sampled and dropped; the static
    /// `Material.uv_offset` ran the shader, so animated water never
    /// scrolled.
    #[test]
    fn uv_offset_u_writes_offset_x_only() {
        let (world, entity, name) = world_with_sinks();
        let channels = vec![(name, const_channel(FloatTarget::UvOffsetU, 0.25))];
        apply_float_channels(&world, &channels, 0.0, &resolve_to(entity));
        let q = world.query::<AnimatedUvTransform>().unwrap();
        let t = q.get(entity).unwrap();
        assert_eq!(t.offset.x, 0.25);
        assert_eq!(t.offset.y, 0.0, "UvOffsetU must not bleed into offset.y");
        assert_eq!(t.scale.x, 1.0, "UvOffsetU must not touch scale");
        assert_eq!(t.scale.y, 1.0);
        assert_eq!(t.rotation, 0.0);
    }

    /// `FloatTarget::UvOffsetV` writes only the V slot. Same isolation
    /// guarantee as the U test, on the orthogonal axis.
    #[test]
    fn uv_offset_v_writes_offset_y_only() {
        let (world, entity, name) = world_with_sinks();
        let channels = vec![(name, const_channel(FloatTarget::UvOffsetV, 0.75))];
        apply_float_channels(&world, &channels, 0.0, &resolve_to(entity));
        let q = world.query::<AnimatedUvTransform>().unwrap();
        let t = q.get(entity).unwrap();
        assert_eq!(t.offset.x, 0.0);
        assert_eq!(t.offset.y, 0.75);
    }

    /// `FloatTarget::UvScaleU` / `UvScaleV` / `UvRotation` each land
    /// in their dedicated slot. Bundled into one test because they
    /// share the same `AnimatedUvTransform` sink and the dispatch
    /// table is the same shape — verifying all three together pins
    /// that no channel cross-writes another's slot.
    #[test]
    fn uv_scale_and_rotation_route_to_distinct_slots() {
        let (world, entity, name) = world_with_sinks();
        let channels = vec![
            (name, const_channel(FloatTarget::UvScaleU, 2.0)),
            (name, const_channel(FloatTarget::UvScaleV, 0.5)),
            (name, const_channel(FloatTarget::UvRotation, 1.5708)),
        ];
        apply_float_channels(&world, &channels, 0.0, &resolve_to(entity));
        let q = world.query::<AnimatedUvTransform>().unwrap();
        let t = q.get(entity).unwrap();
        assert_eq!(t.scale.x, 2.0);
        assert_eq!(t.scale.y, 0.5);
        assert!((t.rotation - 1.5708).abs() < 1e-4);
        // Offset stays at identity even though scale/rotation wrote.
        assert_eq!(t.offset.x, 0.0);
        assert_eq!(t.offset.y, 0.0);
    }

    /// `FloatTarget::ShaderFloat` writes `AnimatedShaderFloat.0`
    /// (single-slot today; per-named-uniform dispatch is downstream
    /// growth). Driven by `BSLightingShaderPropertyFloatController`
    /// on Skyrim+/FO4 content.
    #[test]
    fn shader_float_target_writes_shader_float_component() {
        let (world, entity, name) = world_with_sinks();
        let channels = vec![(name, const_channel(FloatTarget::ShaderFloat, 7.5))];
        apply_float_channels(&world, &channels, 0.0, &resolve_to(entity));
        let q = world.query::<AnimatedShaderFloat>().unwrap();
        assert_eq!(q.get(entity).unwrap().0, 7.5);
    }

    /// `FloatTarget::MorphWeight(idx)` indexes into the morph weights
    /// vec. Multiple channels on the same entity at distinct indices
    /// stack — each writes its own slot. Pre-#525 every morph-weight
    /// sample was dropped on the floor; FaceGen lip-sync NPC heads
    /// stayed at the bind-pose blend.
    #[test]
    fn morph_weight_target_indexed_writes() {
        let (world, entity, name) = world_with_sinks();
        let channels = vec![
            (name, const_channel(FloatTarget::MorphWeight(0), 0.3)),
            (name, const_channel(FloatTarget::MorphWeight(2), 0.9)),
        ];
        apply_float_channels(&world, &channels, 0.0, &resolve_to(entity));
        let q = world.query::<AnimatedMorphWeights>().unwrap();
        let weights = q.get(entity).unwrap();
        // Vec grows to fit the highest written index; intermediate
        // (idx=1) zero-pads.
        assert_eq!(weights.0.len(), 3);
        assert_eq!(weights.get(0), 0.3);
        assert_eq!(weights.get(1), 0.0, "unwritten morph slot must stay zero");
        assert_eq!(weights.get(2), 0.9);
    }

    /// Missing-sink case — when an entity carries the float channels
    /// but doesn't have the matching sparse component (e.g. an
    /// importer didn't insert `AnimatedUvTransform` for a non-UV-
    /// scrolling mesh), the dispatch is a no-op rather than panicking.
    /// Mirrors the SCOL/PKIN parser's defensive posture.
    #[test]
    fn missing_sink_component_is_a_silent_noop() {
        let mut world = World::new();
        let entity = world.spawn();
        // Only AnimatedAlpha — no AnimatedUvTransform. UV channel
        // arrives with no sink; helper must not panic.
        world.insert(entity, AnimatedAlpha(1.0));
        let mut pool = StringPool::new();
        let name = pool.intern("target");
        let channels = vec![(name, const_channel(FloatTarget::UvOffsetU, 0.5))];
        apply_float_channels(&world, &channels, 0.0, &resolve_to(entity));
        // Survived the call — alpha untouched, no panic.
        let q = world.query::<AnimatedAlpha>().unwrap();
        assert_eq!(q.get(entity).unwrap().0, 1.0);
    }
}

// ── #794 / M41-IDLE end-to-end animation_system regression guards ─────
//
// `animation_system` is the single consumer of imported KF clips on
// the apply side. Pre-#794 NPCs spawned with `AnimationPlayer` (post
// #772 close) attached to the placement_root and `with_root(skel_root)`,
// but bones never visibly moved. Suspect 2 in the issue (B-spline
// rotation decoder near-identity) was ruled out via the
// `mtidle_motion_diagnostic` test in `crates/nif/tests/`. These tests
// pin the *system-level* path: synthetic clip → tick → bone Transform
// must change.
#[cfg(test)]
mod animation_system_e2e_tests {
    use super::*;
    use byroredux_core::animation::{
        AnimationClip, AnimationClipRegistry, CycleType, KeyType, RotationKey, TransformChannel,
    };
    use byroredux_core::ecs::World;
    use std::collections::HashMap;

    /// Build a clip with one rotation channel keyed by `bone_name` —
    /// two rotation keys, identity at t=0 and a 90° around Y at t=1.
    fn rotation_clip(pool: &mut StringPool, bone_name: &str) -> AnimationClip {
        let sym = pool.intern(bone_name);
        let mut channels = HashMap::new();
        let half = std::f32::consts::FRAC_1_SQRT_2;
        channels.insert(
            sym,
            TransformChannel {
                translation_keys: Vec::new(),
                translation_type: KeyType::Linear,
                rotation_keys: vec![
                    RotationKey {
                        time: 0.0,
                        value: Quat::IDENTITY,
                        tbc: None,
                    },
                    RotationKey {
                        time: 1.0,
                        value: Quat::from_xyzw(0.0, half, 0.0, half),
                        tbc: None,
                    },
                ],
                rotation_type: KeyType::Linear,
                scale_keys: Vec::new(),
                scale_type: KeyType::Linear,
                priority: 0,
            },
        );
        AnimationClip {
            name: "rot_test".to_string(),
            duration: 1.0,
            cycle_type: CycleType::Loop,
            frequency: 1.0,
            weight: 1.0,
            accum_root_name: None,
            channels,
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: Vec::new(),
        }
    }

    /// Insert the resources the system reads. Returns the bone entity
    /// (the one keyed by `bone_name` and parented under `root`).
    fn build_skeleton_and_clip(bone_name: &str) -> (World, EntityId, EntityId, u32) {
        let mut world = World::new();
        // animation_system queries `AnimationTextKeyEvents` unconditionally
        // and unwraps the storage handle; the production engine relies on
        // `byroredux_scripting::register(&mut world)` having seeded the
        // sparse-set so the query is `Some`. Mirror that here.
        byroredux_scripting::register(&mut world);
        world.insert_resource(StringPool::new());
        world.insert_resource(NameIndex::new());
        world.insert_resource(SubtreeCache::new());
        world.insert_resource(AnimationClipRegistry::new());

        // Spawn the skeleton root and one named child bone. Mirror
        // npc_spawn's shape: root has a Name + Transform, bone has a
        // Name + Transform, bone's Parent = root, root.Children = [bone].
        let root = world.spawn();
        let bone = world.spawn();
        world.insert(root, Transform::IDENTITY);
        world.insert(bone, Transform::IDENTITY);
        world.insert(bone, Parent(root));
        world.insert(root, Children(vec![bone]));

        let bone_sym = {
            let mut pool = world.resource_mut::<StringPool>();
            pool.intern(bone_name)
        };
        world.insert(bone, Name(bone_sym));
        let root_sym = {
            let mut pool = world.resource_mut::<StringPool>();
            pool.intern("__root__")
        };
        world.insert(root, Name(root_sym));

        // Register a synthetic clip and grab its handle.
        let handle = {
            let clip = {
                let mut pool = world.resource_mut::<StringPool>();
                rotation_clip(&mut pool, bone_name)
            };
            let mut reg = world.resource_mut::<AnimationClipRegistry>();
            reg.add(clip)
        };

        (world, root, bone, handle)
    }

    /// End-to-end pin for #794: a player attached to the root entity
    /// with `root_entity = root` must drive the named bone's local
    /// rotation when the system ticks. If this fails, the apply phase
    /// has a regression — that's the third suspect in #794.
    #[test]
    fn rotation_channel_writes_bone_transform_through_animation_system() {
        let bone_name = "Bip01 Spine";
        let (mut world, root, bone, handle) = build_skeleton_and_clip(bone_name);

        // Attach the player on the root, scoped to its own subtree —
        // mirrors npc_spawn::spawn_npc_entity's `with_root(skel_root)`
        // pattern. Pre-#794 the engine's runtime equivalent of this
        // call left bones at bind pose despite ticking.
        let player = AnimationPlayer::new(handle).with_root(root);
        world.insert(root, player);

        // Tick to t=0.5 — rotation should be ~halfway between identity
        // and 90° around Y. SLERP at t=0.5 of (Quat::IDENTITY,
        // Quat(y=√2/2, w=√2/2)) is a non-identity rotation — any
        // non-zero y component proves the apply phase wrote.
        animation_system(&world, 0.5);

        let q = world.query::<Transform>().unwrap();
        let bone_transform = q.get(bone).expect("bone Transform present");
        assert!(
            bone_transform.rotation.y.abs() > 1e-3,
            "bone rotation.y must be non-zero after tick — got {:?} \
             (apply phase isn't writing into the resolved bone entity, \
             matching #794 suspect 3)",
            bone_transform.rotation
        );
    }

    /// `local_time` advances on every tick when `playing=true`. If the
    /// player happens to flip to `playing=false` (or `dt=0`), the bone
    /// stays frozen — that's #794 suspect 1.
    #[test]
    fn animation_player_local_time_advances_per_tick() {
        let bone_name = "Bip01 Spine";
        let (mut world, root, _bone, handle) = build_skeleton_and_clip(bone_name);
        let player = AnimationPlayer::new(handle).with_root(root);
        world.insert(root, player);

        animation_system(&world, 0.1);
        animation_system(&world, 0.1);
        animation_system(&world, 0.1);

        let q = world.query::<AnimationPlayer>().unwrap();
        let p = q.get(root).expect("player present");
        assert!(
            p.local_time > 0.25,
            "local_time must accumulate across ticks — got {}",
            p.local_time
        );
    }

    /// Player on a separate entity with `root_entity = skel_root` —
    /// the cell_loader pattern. Functionally equivalent to player-on-
    /// root, pinned here so future divergence between the two patterns
    /// surfaces as a test failure.
    #[test]
    fn player_on_separate_entity_still_drives_bone_rotation() {
        let bone_name = "Bip01 Spine";
        let (mut world, root, bone, handle) = build_skeleton_and_clip(bone_name);

        let player_entity = world.spawn();
        let mut player = AnimationPlayer::new(handle);
        player.root_entity = Some(root);
        world.insert(player_entity, player);

        animation_system(&world, 0.5);

        let q = world.query::<Transform>().unwrap();
        let bone_transform = q.get(bone).expect("bone Transform present");
        assert!(
            bone_transform.rotation.y.abs() > 1e-3,
            "separate-player-entity pattern must also drive bone rotation \
             — got {:?}",
            bone_transform.rotation
        );
    }

    /// Real-content closure for #794 — loads FNV `mtidle.kf` from the
    /// vanilla BSA, runs it through the production import + convert
    /// path, attaches an AnimationPlayer to a synthetic skeleton with
    /// the same bone names as mtidle's channels, ticks
    /// `animation_system` four times across the clip, and asserts at
    /// least one bone's local rotation diverges from its initial state.
    ///
    /// This is the closure-strength version of the in-crate
    /// synthetic e2e test above: same shape, but using real
    /// B-spline-quantized rotation channels read straight off
    /// disk. If this fails after the synthetic counterpart passes,
    /// the divergence is in the parser-to-system glue (pool
    /// interning, channel-name resolution against scoped subtree
    /// maps), not in either layer alone.
    ///
    /// `#[ignore]` because it needs vanilla FNV game data; run with
    /// `BYROREDUX_FNV_DATA=<path> cargo test -p byroredux --bin byroredux
    /// rotation_through_animation_system_on_real_mtidle -- --ignored
    /// --nocapture`.
    #[test]
    #[ignore]
    fn rotation_through_animation_system_on_real_mtidle() {
        use byroredux_bsa::BsaArchive;
        use std::path::PathBuf;

        const MTIDLE_PATH: &str = r"meshes\characters\_male\locomotion\mtidle.kf";
        const FNV_BSA: &str = "Fallout - Meshes.bsa";

        let data_dir = std::env::var("BYROREDUX_FNV_DATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                PathBuf::from("/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data")
            });
        if !data_dir.is_dir() {
            eprintln!("skipping: FNV data dir not found at {:?}", data_dir);
            return;
        }
        let bsa_path = data_dir.join(FNV_BSA);
        let archive = match BsaArchive::open(&bsa_path) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("skipping: failed to open {:?}: {}", bsa_path, e);
                return;
            }
        };
        let bytes = archive
            .extract(MTIDLE_PATH)
            .expect("vanilla FNV BSA must contain mtidle.kf");
        let nif_scene = byroredux_nif::parse_nif(&bytes).expect("mtidle.kf parses");
        let mut nif_clips = byroredux_nif::anim::import_kf(&nif_scene);
        assert!(!nif_clips.is_empty(), "import_kf yields a clip");
        let nif_clip = nif_clips.remove(0);
        let channel_names: Vec<std::sync::Arc<str>> = nif_clip.channels.keys().cloned().collect();
        eprintln!(
            "real mtidle: '{}' duration={:.2}s freq={} channels={}",
            nif_clip.name,
            nif_clip.duration,
            nif_clip.frequency,
            channel_names.len()
        );

        // Build a World with one fake bone per channel, all under a
        // synthetic skel_root parented under a synthetic placement_root.
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.insert_resource(StringPool::new());
        world.insert_resource(NameIndex::new());
        world.insert_resource(SubtreeCache::new());
        world.insert_resource(AnimationClipRegistry::new());

        let placement_root = world.spawn();
        world.insert(placement_root, Transform::IDENTITY);
        let skel_root = world.spawn();
        world.insert(skel_root, Transform::IDENTITY);
        world.insert(skel_root, Parent(placement_root));
        world.insert(placement_root, Children(vec![skel_root]));

        let mut bones: Vec<(std::sync::Arc<str>, EntityId)> =
            Vec::with_capacity(channel_names.len());
        let mut child_ids: Vec<EntityId> = Vec::with_capacity(channel_names.len());
        for name_arc in &channel_names {
            let bone = world.spawn();
            world.insert(bone, Transform::IDENTITY);
            let sym = {
                let mut pool = world.resource_mut::<StringPool>();
                pool.intern(name_arc)
            };
            world.insert(bone, Name(sym));
            world.insert(bone, Parent(skel_root));
            bones.push((name_arc.clone(), bone));
            child_ids.push(bone);
        }
        world.insert(skel_root, Children(child_ids));

        // Convert + register the clip through the production pool.
        let handle = {
            let clip = {
                let mut pool = world.resource_mut::<StringPool>();
                crate::anim_convert::convert_nif_clip(&nif_clip, &mut pool)
            };
            let mut reg = world.resource_mut::<AnimationClipRegistry>();
            reg.add(clip)
        };
        let player = AnimationPlayer::new(handle).with_root(skel_root);
        world.insert(placement_root, player);

        // Capture initial bone Transforms.
        let initial: HashMap<EntityId, Quat> = {
            let q = world.query::<Transform>().unwrap();
            bones
                .iter()
                .map(|(_, e)| (*e, q.get(*e).unwrap().rotation))
                .collect()
        };

        // Tick through ~half the clip in 4 steps.
        let step = (nif_clip.duration / 4.0).max(0.05);
        for _ in 0..4 {
            animation_system(&world, step);
        }

        // Find the maximum component-wise rotation delta across all
        // bones. mtidle's max inter-sample rotation delta in the
        // mtidle_motion_diagnostic test was 0.065; gating at 1e-3 is
        // far below that and well above float noise.
        let mut max_delta = 0.0f32;
        let mut max_name: Option<std::sync::Arc<str>> = None;
        {
            let q = world.query::<Transform>().unwrap();
            for (name_arc, e) in &bones {
                let r0 = initial[e];
                let r1 = q.get(*e).unwrap().rotation;
                let d = (r1.x - r0.x).abs().max(
                    (r1.y - r0.y)
                        .abs()
                        .max((r1.z - r0.z).abs().max((r1.w - r0.w).abs())),
                );
                if d > max_delta {
                    max_delta = d;
                    max_name = Some(name_arc.clone());
                }
            }
        }

        eprintln!(
            "max rotation delta after 4 ticks @ {:.2}s = {:.6} on bone '{}'",
            step,
            max_delta,
            max_name.as_deref().unwrap_or("<none>"),
        );
        assert!(
            max_delta > 1e-3,
            "real mtidle.kf piped through animation_system must move *some* \
             bone (max component delta {:.6} ≤ 1e-3). Production runtime \
             reports 'NPCs stand rigid' under exactly this composition; if \
             this lab test passes too, the visible-motion gap is downstream \
             of the apply phase (skinning palette, body skin resolution, \
             or perceptual amplitude — mtidle's authored deltas are subtle).",
            max_delta,
        );
    }

    /// Faithful npc_spawn composition: placement_root → skel_root →
    /// bone, with player on placement_root and `with_root(skel_root)`.
    /// The body-NIF clone hierarchy adds a *second* "Bip01 Spine"
    /// entity directly under placement_root (mirroring the body NIF's
    /// own skeleton-shaped NiNode hierarchy). The scoped subtree map
    /// must dispatch to the **skeleton's** bone, not the body's clone
    /// — verified by checking the body clone stays at identity.
    #[test]
    fn npc_spawn_shape_drives_skeleton_bone_not_body_clone() {
        let bone_name = "Bip01 Spine";
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.insert_resource(StringPool::new());
        world.insert_resource(NameIndex::new());
        world.insert_resource(SubtreeCache::new());
        world.insert_resource(AnimationClipRegistry::new());

        // placement_root (carries world pose, NPC editor_id name)
        let placement_root = world.spawn();
        world.insert(placement_root, Transform::IDENTITY);
        let editor_id_sym = {
            let mut pool = world.resource_mut::<StringPool>();
            pool.intern("DocMitchell")
        };
        world.insert(placement_root, Name(editor_id_sym));

        // skel_root (skeleton.nif root) under placement_root
        let skel_root = world.spawn();
        world.insert(skel_root, Transform::IDENTITY);
        let skel_root_sym = {
            let mut pool = world.resource_mut::<StringPool>();
            pool.intern("NPC")
        };
        world.insert(skel_root, Name(skel_root_sym));
        world.insert(skel_root, Parent(placement_root));

        // Skeleton's bone — actual animation target
        let skel_bone = world.spawn();
        world.insert(skel_bone, Transform::IDENTITY);
        let bone_sym = {
            let mut pool = world.resource_mut::<StringPool>();
            pool.intern(bone_name)
        };
        world.insert(skel_bone, Name(bone_sym));
        world.insert(skel_bone, Parent(skel_root));
        world.insert(skel_root, Children(vec![skel_bone]));

        // Body-NIF clone of "Bip01 Spine" — directly under
        // placement_root (NOT under skel_root, per npc_spawn's
        // documented intent at the body parenting comment).
        let body_clone = world.spawn();
        world.insert(body_clone, Transform::IDENTITY);
        world.insert(body_clone, Name(bone_sym));
        world.insert(body_clone, Parent(placement_root));
        world.insert(placement_root, Children(vec![skel_root, body_clone]));

        let handle = {
            let clip = {
                let mut pool = world.resource_mut::<StringPool>();
                rotation_clip(&mut pool, bone_name)
            };
            let mut reg = world.resource_mut::<AnimationClipRegistry>();
            reg.add(clip)
        };

        let player = AnimationPlayer::new(handle).with_root(skel_root);
        world.insert(placement_root, player);

        animation_system(&world, 0.5);

        let q = world.query::<Transform>().unwrap();
        let skel_xf = q.get(skel_bone).expect("skel bone");
        let body_xf = q.get(body_clone).expect("body clone");

        assert!(
            skel_xf.rotation.y.abs() > 1e-3,
            "skeleton's bone must rotate — got {:?}",
            skel_xf.rotation
        );
        assert!(
            (body_xf.rotation.y).abs() < 1e-6 && (body_xf.rotation.w - 1.0).abs() < 1e-6,
            "body clone (outside skel_root subtree) must stay at identity \
             — got {:?}; subtree scoping is broken",
            body_xf.rotation
        );
    }
}

// ── M44 Phase 3.5 — footstep_system regression tests ──────────────
//
// Synthetic-only: walk an emitter through a known-distance path,
// verify the stride accumulator triggers exactly when expected and
// queues a one-shot for each trigger. Audio device not required —
// `AudioWorld` runs in the no-device branch and `play_oneshot`
// queues into the pending vec without touching kira.
#[cfg(test)]
mod footstep_system_tests {
    use super::*;
    use crate::components::{FootstepConfig, FootstepEmitter, FootstepScratch};
    use byroredux_audio::{Frame, Sound, SoundSettings};
    use byroredux_core::ecs::World;
    use std::sync::Arc;

    fn synth_world(volume: f32) -> (World, Arc<Sound>) {
        let mut world = World::new();
        let sound = Arc::new(Sound {
            sample_rate: 22_050,
            frames: Arc::from(
                vec![
                    Frame {
                        left: 0.0,
                        right: 0.0
                    };
                    50
                ]
                .into_boxed_slice(),
            ),
            settings: SoundSettings::default(),
            slice: None,
        });
        world.insert_resource(FootstepConfig {
            default_sound: Some(Arc::clone(&sound)),
            volume,
        });
        // AudioWorld via `Default::default()` — picks up the
        // headless fallback path when the test host has no audio
        // device, otherwise creates a real manager. Either way
        // `play_oneshot` enqueues without immediately dispatching
        // (drain only fires inside `audio_system`, which we don't
        // call from the footstep tests).
        world.insert_resource(byroredux_audio::AudioWorld::default());
        world.insert_resource(FootstepScratch::default());
        (world, sound)
    }

    /// First tick on a fresh `FootstepEmitter` must seed
    /// `last_position` and NOT fire — otherwise the emitter would
    /// always emit one phantom footstep against the default zero
    /// pose at spawn time.
    #[test]
    fn first_tick_seeds_last_position_without_firing() {
        let (mut world, _sound) = synth_world(0.5);
        let entity = world.spawn();
        world.insert(entity, Transform::IDENTITY);
        world.insert(
            entity,
            GlobalTransform::new(Vec3::new(10.0, 0.0, 5.0), Quat::IDENTITY, 1.0),
        );
        world.insert(entity, FootstepEmitter::new());

        footstep_system(&world, 0.016);

        let aw = world.resource::<byroredux_audio::AudioWorld>();
        assert_eq!(
            aw.pending_oneshot_count(),
            0,
            "first tick must NOT fire — only seed last_position"
        );
        let q = world.query::<FootstepEmitter>().unwrap();
        let fs = q.get(entity).unwrap();
        assert!(fs.initialised, "first tick must mark emitter initialised");
        assert_eq!(fs.last_position, Vec3::new(10.0, 0.0, 5.0));
        assert_eq!(fs.accumulated_stride, 0.0);
    }

    /// Walking exactly one threshold distance fires exactly one
    /// footstep. Vertical motion is excluded — only XZ delta counts.
    #[test]
    fn stride_threshold_fires_exactly_one_footstep() {
        let (mut world, _sound) = synth_world(0.7);
        let entity = world.spawn();
        world.insert(entity, Transform::IDENTITY);
        world.insert(
            entity,
            GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0),
        );
        world.insert(entity, FootstepEmitter::new());

        // Tick 1: seed last_position at origin.
        footstep_system(&world, 0.016);

        // Move 1.5 game-units along +X (exactly the default threshold).
        // Also bump Y by 100 — vertical-only motion that must NOT
        // contribute to stride.
        {
            let mut q = world.query_mut::<GlobalTransform>().unwrap();
            let gt = q.get_mut(entity).unwrap();
            gt.translation = Vec3::new(1.5, 100.0, 0.0);
        }

        // Tick 2: stride accumulates 1.5 units, hits threshold, fires.
        footstep_system(&world, 0.016);

        let aw = world.resource::<byroredux_audio::AudioWorld>();
        assert_eq!(
            aw.pending_oneshot_count(),
            1,
            "1.5-unit horizontal stride must fire exactly one footstep"
        );
    }

    /// Walking 4× the threshold distance in one tick must fire 1
    /// footstep (stride resets when the threshold is crossed; a
    /// catastrophic teleport doesn't multiply footsteps). This pins
    /// the "reset to zero on fire" semantic — a "subtract threshold,
    /// keep remainder" refactor would fire 4 footsteps and feel
    /// machine-gun-like at high speeds.
    #[test]
    fn single_large_jump_fires_one_footstep_only() {
        let (mut world, _sound) = synth_world(1.0);
        let entity = world.spawn();
        world.insert(entity, Transform::IDENTITY);
        world.insert(
            entity,
            GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0),
        );
        world.insert(entity, FootstepEmitter::new());

        footstep_system(&world, 0.016); // seed

        // 6.0 horizontal units in one frame — 4× threshold.
        {
            let mut q = world.query_mut::<GlobalTransform>().unwrap();
            let gt = q.get_mut(entity).unwrap();
            gt.translation = Vec3::new(6.0, 0.0, 0.0);
        }

        footstep_system(&world, 0.016);

        let aw = world.resource::<byroredux_audio::AudioWorld>();
        assert_eq!(
            aw.pending_oneshot_count(),
            1,
            "single-tick teleport must fire exactly one footstep, not multiple"
        );
    }

    /// A standing-still emitter (zero stride) never fires. Pinned
    /// because a regression that "fires on every tick when stride
    /// >= 0" would silently spam audio when the player isn't moving.
    #[test]
    fn standing_still_never_fires() {
        let (mut world, _sound) = synth_world(0.5);
        let entity = world.spawn();
        world.insert(entity, Transform::IDENTITY);
        world.insert(
            entity,
            GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0),
        );
        world.insert(entity, FootstepEmitter::new());

        for _ in 0..30 {
            footstep_system(&world, 0.016);
        }

        let aw = world.resource::<byroredux_audio::AudioWorld>();
        assert_eq!(aw.pending_oneshot_count(), 0);
    }

    /// Footsteps no-op cleanly when no `default_sound` is loaded
    /// (i.e. user didn't pass --sounds-bsa). The emitter should still
    /// update its last_position so a future runtime reload of the
    /// sound picks up cleanly without a phantom step.
    #[test]
    fn no_default_sound_is_silent_noop() {
        let (mut world, _sound) = synth_world(0.5);
        // Drop the sound reference, leaving the config but with
        // default_sound: None.
        {
            let mut config = world.resource_mut::<FootstepConfig>();
            config.default_sound = None;
        }
        let entity = world.spawn();
        world.insert(entity, Transform::IDENTITY);
        world.insert(
            entity,
            GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0),
        );
        world.insert(entity, FootstepEmitter::new());

        footstep_system(&world, 0.016);
        {
            let mut q = world.query_mut::<GlobalTransform>().unwrap();
            let gt = q.get_mut(entity).unwrap();
            gt.translation = Vec3::new(5.0, 0.0, 0.0);
        }
        footstep_system(&world, 0.016);

        let aw = world.resource::<byroredux_audio::AudioWorld>();
        assert_eq!(aw.pending_oneshot_count(), 0);
    }
}

// ── M44 Phase 6 — reverb_zone_system regression tests (#846) ──────
#[cfg(test)]
mod reverb_zone_system_tests {
    use super::*;
    use crate::components::CellLightingRes;
    use byroredux_core::ecs::World;

    /// Build a synthetic CellLightingRes with the specified
    /// interior/exterior flag. All extended-XCLL fields stay `None`
    /// — the system only reads `is_interior`, so the rest is
    /// irrelevant.
    fn cell_lit(is_interior: bool) -> CellLightingRes {
        CellLightingRes {
            ambient: [0.1, 0.1, 0.1],
            directional_color: [1.0, 1.0, 1.0],
            directional_dir: [0.0, 1.0, 0.0],
            is_interior,
            fog_color: [0.5, 0.5, 0.5],
            fog_near: 100.0,
            fog_far: 1000.0,
            directional_fade: None,
            fog_clip: None,
            fog_power: None,
            fog_far_color: None,
            fog_max: None,
            light_fade_begin: None,
            light_fade_end: None,
            directional_ambient: None,
            specular_color: None,
            specular_alpha: None,
            fresnel_power: None,
        }
    }

    /// Interior cell flips the reverb send to a subtle wet level.
    /// Pre-fix this was `NEG_INFINITY` regardless of cell type — the
    /// audit's "every cell sounds dry" complaint.
    #[test]
    fn interior_cell_sets_subtle_reverb_send() {
        let mut world = World::new();
        world.insert_resource(byroredux_audio::AudioWorld::default());
        world.insert_resource(cell_lit(true));

        // Pre-condition: default AudioWorld boots with NEG_INFINITY.
        assert!(
            world
                .resource::<byroredux_audio::AudioWorld>()
                .reverb_send_db()
                .is_infinite(),
            "default AudioWorld must boot with NEG_INFINITY reverb send"
        );

        reverb_zone_system(&world, 0.016);

        let aw = world.resource::<byroredux_audio::AudioWorld>();
        assert_eq!(
            aw.reverb_send_db(),
            -12.0,
            "interior cell must set the subtle-wet reverb send level"
        );
    }

    /// Exterior cell keeps the send dry (NEG_INFINITY). Default
    /// already is, but verify the system doesn't accidentally trip
    /// to a finite value on exterior.
    #[test]
    fn exterior_cell_keeps_dry_send() {
        let mut world = World::new();
        world.insert_resource(byroredux_audio::AudioWorld::default());
        world.insert_resource(cell_lit(false));

        reverb_zone_system(&world, 0.016);

        let db = world
            .resource::<byroredux_audio::AudioWorld>()
            .reverb_send_db();
        assert!(
            db.is_infinite() && db.is_sign_negative(),
            "exterior cell must leave reverb send at NEG_INFINITY (got {db})"
        );
    }

    /// Interior → exterior transition flips the send back to dry.
    /// Pin the round trip so a future regression that breaks the
    /// exterior branch (e.g. wrong sign, wrong constant) shows up.
    #[test]
    fn interior_to_exterior_transition_resets_send_to_dry() {
        let mut world = World::new();
        world.insert_resource(byroredux_audio::AudioWorld::default());

        // Tick 1 — interior: send = -12 dB.
        world.insert_resource(cell_lit(true));
        reverb_zone_system(&world, 0.016);
        assert_eq!(
            world
                .resource::<byroredux_audio::AudioWorld>()
                .reverb_send_db(),
            -12.0,
        );

        // Tick 2 — exterior cell load: send must drop back to dry.
        world.insert_resource(cell_lit(false));
        reverb_zone_system(&world, 0.016);
        let db = world
            .resource::<byroredux_audio::AudioWorld>()
            .reverb_send_db();
        assert!(
            db.is_infinite() && db.is_sign_negative(),
            "interior → exterior transition must reset send to NEG_INFINITY (got {db})"
        );
    }

    /// No `CellLightingRes` (engine boot before any cell load) → the
    /// system must no-op without panic. Default AudioWorld send stays
    /// at NEG_INFINITY (= dry, which is the correct safe default).
    #[test]
    fn no_cell_lighting_resource_is_safe_noop() {
        let mut world = World::new();
        world.insert_resource(byroredux_audio::AudioWorld::default());
        // Deliberately omit CellLightingRes.

        reverb_zone_system(&world, 0.016);

        let db = world
            .resource::<byroredux_audio::AudioWorld>()
            .reverb_send_db();
        assert!(
            db.is_infinite() && db.is_sign_negative(),
            "no-CellLightingRes path must leave default send untouched"
        );
    }

    /// No `AudioWorld` (engine started without audio wiring) → the
    /// system must no-op without panic when the resource is absent.
    #[test]
    fn no_audio_world_is_safe_noop() {
        let mut world = World::new();
        world.insert_resource(cell_lit(true));
        // Deliberately omit AudioWorld.

        reverb_zone_system(&world, 0.016);
        // Survival is the assertion — no panic, no aborted run.
    }
}
