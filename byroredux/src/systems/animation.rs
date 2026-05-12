//! Animation system — advances `AnimationPlayer` / `AnimationStack`
//! time and applies sampled channels (transform, color, float, bool,
//! morph) to named entities resolved through the `NameIndex` /
//! `SubtreeCache`.

use byroredux_core::animation::{
    advance_stack, advance_time, sample_blended_transform, sample_bool_channel,
    sample_color_channel, sample_float_channel, sample_rotation, sample_scale, sample_translation,
    split_root_motion, visit_stack_text_events, visit_text_key_events, AnimationClipRegistry,
    AnimationPlayer, AnimationStack, ColorTarget, FloatTarget, RootMotionDelta,
};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{
    AnimatedAlpha, AnimatedAmbientColor, AnimatedDiffuseColor, AnimatedEmissiveColor,
    AnimatedMorphWeights, AnimatedShaderColor, AnimatedShaderFloat, AnimatedSpecularColor,
    AnimatedUvTransform, AnimatedVisibility, Name, Transform, World,
};
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::FixedString;
#[cfg(test)]
use byroredux_core::string::StringPool;

use crate::anim_convert::build_subtree_name_map;
use crate::components::{NameIndex, SubtreeCache};

// `make_transform_propagation_system` has moved to
// `byroredux_core::ecs::systems` so every downstream crate gets the same
// `NiNode::UpdateDownwardPass` equivalent without copy-pasting. Re-export
// it here under the existing name so call sites in this binary don't need
// to change. See issue #81.
pub(crate) use byroredux_core::ecs::make_transform_propagation_system;

// ── DRY helpers (shared between AnimationPlayer + AnimationStack paths) ──

/// If a `SubtreeCache` is registered and doesn't yet have an entry for
/// `root`, build the name→entity map for the subtree rooted at `root`
/// and insert it. No-op if the cache is missing or already populated.
///
/// Both the flat-player path and the layered-stack path need the same
/// "lazy build per-root scoped resolver" behaviour; pulling it here
/// removes ~12 lines × 2 callsites from `animation_system`.
fn ensure_subtree_cache(world: &World, root: EntityId) {
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

/// Write a non-zero root-motion delta into `RootMotionDelta` on
/// `entity`. No-op when the motion is `Vec3::ZERO` or when the
/// component isn't on the entity / storage isn't registered.
#[inline]
fn write_root_motion(world: &World, entity: EntityId, motion: Vec3) {
    if motion == Vec3::ZERO {
        return;
    }
    if let Some(mut rmq) = world.query_mut::<RootMotionDelta>() {
        if let Some(rm) = rmq.get_mut(entity) {
            rm.0 = motion;
        }
    }
}

/// Apply bool (visibility) channels — single lock for entire batch.
/// Shared between the AnimationPlayer and AnimationStack apply paths
/// (#211 / #517 / #525 sibling helper).
fn apply_bool_channels(
    world: &World,
    bool_channels: &[(FixedString, byroredux_core::animation::BoolChannel)],
    time: f32,
    resolve_entity: &dyn Fn(&FixedString) -> Option<EntityId>,
) {
    let Some(mut vq) = world.query_mut::<AnimatedVisibility>() else {
        return;
    };
    for (channel_name, channel) in bool_channels {
        let Some(target_entity) = resolve_entity(channel_name) else {
            continue;
        };
        let value = sample_bool_channel(channel, time);
        if let Some(v) = vq.get_mut(target_entity) {
            v.0 = value;
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
/// supplies its own.
pub(crate) fn apply_color_channels(
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

    // Macro collapses the 5 near-identical match arms (lazy-init,
    // get_mut, assign `.0`) into one line per target. `world`,
    // `target`, and `value` are threaded through explicitly so the
    // expansion is hygienic.
    macro_rules! write_lazy {
        ($cache:ident, $Comp:ty, $world:expr, $entity:expr, $value:expr) => {{
            let q = $cache.get_or_insert_with(|| $world.query_mut::<$Comp>());
            if let Some(q) = q.as_mut() {
                if let Some(c) = q.get_mut($entity) {
                    c.0 = $value;
                }
            }
        }};
    }

    for (channel_name, channel) in color_channels {
        let Some(target_entity) = resolve_entity(channel_name) else {
            continue;
        };
        let value = sample_color_channel(channel, time);
        match channel.target {
            ColorTarget::Diffuse => {
                write_lazy!(diffuse_q, AnimatedDiffuseColor, world, target_entity, value)
            }
            ColorTarget::Ambient => {
                write_lazy!(ambient_q, AnimatedAmbientColor, world, target_entity, value)
            }
            ColorTarget::Specular => write_lazy!(
                specular_q,
                AnimatedSpecularColor,
                world,
                target_entity,
                value
            ),
            ColorTarget::Emissive => write_lazy!(
                emissive_q,
                AnimatedEmissiveColor,
                world,
                target_entity,
                value
            ),
            ColorTarget::ShaderColor => {
                write_lazy!(shader_q, AnimatedShaderColor, world, target_entity, value)
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
pub(crate) fn apply_float_channels(
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
            ensure_subtree_cache(world, root);
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
            write_root_motion(world, entity, root_motion);
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

        // Apply bool (visibility) channels.
        if !clip.bool_channels.is_empty() {
            apply_bool_channels(world, &clip.bool_channels, current_time, &resolve_entity);
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
                ensure_subtree_cache(world, root);
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

        write_root_motion(world, entity, root_motion);

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
                    apply_bool_channels(world, &clip.bool_channels, time, &stack_resolve);
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

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
    use byroredux_core::ecs::{Children, Parent, World};
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
