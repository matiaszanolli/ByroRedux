//! Animation system — advances `AnimationPlayer` / `AnimationStack`
//! time and applies sampled channels (transform, color, float, bool,
//! morph) to named entities resolved through the `NameIndex` /
//! `SubtreeCache`.

use byroredux_core::animation::{
    advance_stack, advance_time, sample_blended_transform, sample_bool_channel,
    sample_color_channel, sample_float_channel, sample_rotation, sample_scale, sample_translation,
    split_root_motion, visit_stack_text_events, visit_text_key_events, AnimationClipRegistry,
    AnimationPlayer, AnimationStack, ColorTarget, CycleType, FloatTarget, RootMotionDelta,
    TransformChannel,
};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{
    AnimatedAlpha, AnimatedAmbientColor, AnimatedDiffuseColor, AnimatedEmissiveColor,
    AnimatedMorphWeights, AnimatedShaderColor, AnimatedShaderFloat, AnimatedSpecularColor,
    AnimatedUvTransform, AnimatedVisibility, Name, ResourceRead, Transform, World,
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

/// Acquire the `SubtreeCache` read guard, building the name→entity map
/// for the subtree rooted at `root` first if it isn't cached yet.
/// `None` when the resource isn't registered.
///
/// Both the flat-player path and the layered-stack path need the same
/// "lazy build per-root scoped resolver" behaviour; pulling it here
/// removes ~12 lines × 2 callsites from `animation_system`.
///
/// #2924 / PERF-D1-02 — this used to be an `ensure_subtree_cache(world,
/// root)` that acquired the cache, tested `contains_key`, dropped the
/// guard, and returned nothing; every caller then immediately acquired
/// the same resource again for the actual lookup. That is two
/// acquisitions per animated entity per frame on the steady-state path
/// (each one a `TypeId` probe into `resources` plus the
/// `lock_tracker::track_read`/`untrack_read` pair, both std-hasher
/// maps) for a cache that is already populated. Returning the guard the
/// hit path already holds makes it one. The miss path still costs three
/// — read, write, read — but that runs once per root, not per frame.
fn subtree_cache(world: &World, root: Option<EntityId>) -> Option<ResourceRead<'_, SubtreeCache>> {
    let cache = world.try_resource::<SubtreeCache>()?;
    let Some(root) = root else {
        return Some(cache);
    };
    if cache.map.contains_key(&root) {
        return Some(cache);
    }
    // Miss: release the read guard before `build_subtree_name_map`
    // (which queries `Name`/`Children`) and the write that follows.
    drop(cache);
    let map = build_subtree_name_map(world, root);
    world.resource_mut::<SubtreeCache>().map.insert(root, map);
    world.try_resource::<SubtreeCache>()
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

/// Convert an accumulation root's absolute sampled translation into the
/// displacement crossed during this tick. Havok cart exits author COM in
/// seat-relative coordinates, so applying the absolute sample every frame
/// would compound hundreds of units of motion.
fn sampled_root_motion_delta(
    clip: &byroredux_core::animation::AnimationClip,
    channel: &TransformChannel,
    previous_time: f32,
    current_time: f32,
) -> Vec3 {
    let horizontal = |time| {
        sample_translation(channel, time)
            .map(|position| split_root_motion(position).1)
            .unwrap_or(Vec3::ZERO)
    };
    let previous = horizontal(previous_time);
    let current = horizontal(current_time);
    if clip.cycle_type == CycleType::Loop && current_time < previous_time {
        (horizontal(clip.duration) - previous) + (current - horizontal(0.0))
    } else {
        current - previous
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
    // #2399 — CONC-D3-2026-08-07-01. The pre-fix version lazily acquired
    // each target's write guard on *first encounter* while scanning
    // `color_channels` in the order the NIF/KF clip authored them, and
    // held every guard acquired so far until the whole loop finished.
    // That makes the pairwise acquisition order across the five color
    // sinks (+ `LightSource`) content-determined: a clip ordered
    // `[Diffuse, Emissive]` acquires Diffuse-before-Emissive, a sibling
    // clip ordered `[Emissive, Diffuse]` acquires the reverse — exactly
    // the "authored order decides lock order" shape invariant #4 (see
    // `animation_system_inner`'s NameIndex/Name comment) exists to rule
    // out.
    //
    // Fixed instead: one pass per target, in a fixed declared order
    // (matching the `Stage::Update` access-declaration order in
    // `boot.rs`), each pass acquiring its guard, applying every channel
    // that targets it, and dropping the guard before the next target's
    // pass begins. No two guards are ever held at once, so the
    // acquisition order the lock tracker observes is fixed at compile
    // time regardless of clip content. The extra per-target scan over
    // `color_channels` is negligible — clips carry one or two color
    // targets in practice (an emissive pulse, maybe a diffuse tint).
    macro_rules! apply_target {
        ($Comp:ty, $target:pat, |$slot:ident, $value:ident| $write:expr) => {{
            if color_channels
                .iter()
                .any(|(_, c)| matches!(c.target, $target))
            {
                if let Some(mut q) = world.query_mut::<$Comp>() {
                    for (channel_name, channel) in color_channels {
                        if !matches!(channel.target, $target) {
                            continue;
                        }
                        let Some(target_entity) = resolve_entity(channel_name) else {
                            continue;
                        };
                        let $value = sample_color_channel(channel, time);
                        if let Some($slot) = q.get_mut(target_entity) {
                            $write
                        }
                    }
                }
            }
        }};
    }

    apply_target!(AnimatedDiffuseColor, ColorTarget::Diffuse, |c, value| c.0 =
        value);
    apply_target!(AnimatedAmbientColor, ColorTarget::Ambient, |c, value| c.0 =
        value);
    apply_target!(AnimatedSpecularColor, ColorTarget::Specular, |c, value| c
        .0 =
        value);
    apply_target!(AnimatedEmissiveColor, ColorTarget::Emissive, |c, value| c
        .0 =
        value);
    apply_target!(AnimatedShaderColor, ColorTarget::ShaderColor, |c, value| {
        c.0 = value
    });
    // #983 — NiLightColorController sink. The animated colour writes
    // straight into `LightSource.emitter.radiant_intensity`; the
    // light-buffer build then multiplies by `dimmer * intensity` to
    // produce the final per-light radiance.
    apply_target!(
        byroredux_core::ecs::LightSource,
        ColorTarget::LightDiffuse,
        |ls, value| {
            ls.emitter.radiant_intensity =
                byroredux_core::lighting::RadiantIntensityRgb::new([value.x, value.y, value.z]);
        }
    );
    // ColorTarget::LightAmbient — #983: captured but not consumed by the
    // current renderer (cell ambient drives the unlit fallback, no
    // per-light ambient slot in the light buffer). No storage to lock,
    // so no acquisition-order concern; left as a documented no-op.
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
    // #2399 — same fix as `apply_color_channels`: one pass per target
    // group, in a fixed declared order (Alpha → UV → ShaderFloat →
    // MorphWeights → LightSource, matching `boot.rs`'s access
    // declarations), each pass acquiring its guard, applying every
    // channel that targets it, and dropping the guard before the next
    // group's pass begins. See that function's comment for the full
    // rationale — the pre-fix lazy-acquire-in-channel-order shape was
    // the same here, five UV sub-targets and three light sub-targets
    // each sharing one storage (`AnimatedUvTransform` /
    // `LightSource`) and therefore one pass.

    // Alpha.
    if float_channels
        .iter()
        .any(|(_, c)| matches!(c.target, FloatTarget::Alpha))
    {
        if let Some(mut q) = world.query_mut::<AnimatedAlpha>() {
            for (channel_name, channel) in float_channels {
                if !matches!(channel.target, FloatTarget::Alpha) {
                    continue;
                }
                let Some(target_entity) = resolve_entity(channel_name) else {
                    continue;
                };
                let value = sample_float_channel(channel, time);
                if let Some(a) = q.get_mut(target_entity) {
                    a.0 = value;
                }
            }
        }
    }

    // UV offset/scale/rotation — five sub-targets, one shared component.
    let is_uv_target = |target: FloatTarget| {
        matches!(
            target,
            FloatTarget::UvOffsetU
                | FloatTarget::UvOffsetV
                | FloatTarget::UvScaleU
                | FloatTarget::UvScaleV
                | FloatTarget::UvRotation
        )
    };
    if float_channels.iter().any(|(_, c)| is_uv_target(c.target)) {
        if let Some(mut q) = world.query_mut::<AnimatedUvTransform>() {
            for (channel_name, channel) in float_channels {
                if !is_uv_target(channel.target) {
                    continue;
                }
                let Some(target_entity) = resolve_entity(channel_name) else {
                    continue;
                };
                let value = sample_float_channel(channel, time);
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
    }

    // ShaderFloat.
    if float_channels
        .iter()
        .any(|(_, c)| matches!(c.target, FloatTarget::ShaderFloat))
    {
        if let Some(mut q) = world.query_mut::<AnimatedShaderFloat>() {
            for (channel_name, channel) in float_channels {
                if !matches!(channel.target, FloatTarget::ShaderFloat) {
                    continue;
                }
                let Some(target_entity) = resolve_entity(channel_name) else {
                    continue;
                };
                let value = sample_float_channel(channel, time);
                if let Some(s) = q.get_mut(target_entity) {
                    s.0 = value;
                }
            }
        }
    }

    // MorphWeight(idx).
    if float_channels
        .iter()
        .any(|(_, c)| matches!(c.target, FloatTarget::MorphWeight(_)))
    {
        if let Some(mut q) = world.query_mut::<AnimatedMorphWeights>() {
            for (channel_name, channel) in float_channels {
                let FloatTarget::MorphWeight(idx) = channel.target else {
                    continue;
                };
                let Some(target_entity) = resolve_entity(channel_name) else {
                    continue;
                };
                let value = sample_float_channel(channel, time);
                if let Some(m) = q.get_mut(target_entity) {
                    m.set(idx as usize, value);
                }
            }
        }
    }

    // #983 — NiLight float controllers (Dimmer, Intensity, Radius) all
    // mutate `LightSource` on the target entity.
    let is_light_target = |target: FloatTarget| {
        matches!(
            target,
            FloatTarget::LightDimmer | FloatTarget::LightIntensity | FloatTarget::LightRadius
        )
    };
    if float_channels
        .iter()
        .any(|(_, c)| is_light_target(c.target))
    {
        if let Some(mut q) = world.query_mut::<byroredux_core::ecs::LightSource>() {
            for (channel_name, channel) in float_channels {
                if !is_light_target(channel.target) {
                    continue;
                }
                let Some(target_entity) = resolve_entity(channel_name) else {
                    continue;
                };
                let value = sample_float_channel(channel, time);
                if let Some(ls) = q.get_mut(target_entity) {
                    match channel.target {
                        FloatTarget::LightDimmer => ls.dimmer = value,
                        FloatTarget::LightIntensity => ls.intensity = value,
                        FloatTarget::LightRadius => {
                            ls.emitter.range =
                                byroredux_core::lighting::Meters::from_bethesda_units(value)
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
    }
}

/// #2221 — texture-flip (`NiFlipController`) cycle position. Only picks
/// among the bindless handles `anim_convert::attach_animation_sinks`
/// already resolved at clip-attach time (`AnimatedTextureFlip.handles`);
/// this system never touches the texture registry, matching every other
/// `apply_*_channels` function's "systems take `&World`, not GPU
/// resources" shape.
pub(crate) fn apply_texture_flip_channels(
    world: &World,
    texture_flip_channels: &[(FixedString, byroredux_core::animation::TextureFlipChannel)],
    time: f32,
    resolve_entity: &dyn Fn(&FixedString) -> Option<EntityId>,
) {
    use byroredux_core::animation::sample_texture_flip_index;
    use byroredux_core::ecs::AnimatedTextureFlip;

    let Some(mut q) = world.query_mut::<AnimatedTextureFlip>() else {
        return;
    };
    for (channel_name, channel) in texture_flip_channels {
        if channel.source_paths.is_empty() {
            continue;
        }
        let Some(target_entity) = resolve_entity(channel_name) else {
            continue;
        };
        let Some(flip) = q.get_mut(target_entity) else {
            continue;
        };
        let Some(entry) = flip
            .0
            .iter_mut()
            .find(|e| e.texture_slot == channel.texture_slot)
        else {
            continue;
        };
        // The handles Vec length is fixed at attach time (one per
        // `source_paths` entry) — re-derive the index against the LIVE
        // channel's key curve rather than trusting `entry.handles.len()`
        // could ever disagree with `channel.source_paths.len()` (same
        // data, just resolved once vs. read every frame).
        entry.current_index = sample_texture_flip_index(channel, time);
    }
}

/// Per-entity playback state collected during the player advance pass.
/// Defined at module level so `animation_system_inner` and
/// `make_animation_system` can share the type for the scratch buffer.
struct PlaybackState {
    entity: EntityId,
    clip_handle: u32,
    root_entity: Option<EntityId>,
    current_time: f32,
    prev_time: f32,
    /// Ping-pong direction (`CycleType::Reverse`) captured from the player,
    /// threaded to `visit_text_key_events` so a backward leg fires the keys
    /// it crossed rather than the loop-wrap complement. FNV-D6-01 / #2082.
    reverse_direction: bool,
}

/// Reusable per-frame scratch buffers for [`animation_system_inner`].
///
/// Captured by [`make_animation_system`] so their backing allocations
/// survive across frames (production), or built fresh per call (the
/// `#[cfg(test)]` wrapper). Pre-#1725 only `entities` / `playback`
/// persisted; every text-event buffer — the player path's `player_events`
/// and the stack path's `stack_events` / `seen_labels` / `channel_names` /
/// `updates` — was re-declared `Vec::new()` inside the function and
/// re-grown 0→N each frame, the same regrowth #828 / #1372 removed
/// elsewhere. (The stack set's "outer closure scope" comment was stale:
/// they were function-local, reused across *entities* within a frame but
/// dropped at frame end, never closure-captured.)
#[derive(Default)]
struct AnimScratch {
    /// Player-entity list, then reused for the stack-entity list (#1372).
    entities: Vec<EntityId>,
    /// Per-frame playback-state table (#1372).
    playback: Vec<PlaybackState>,
    /// AnimationPlayer text-key events for one entity (#211 / #339).
    player_events: Vec<byroredux_scripting::events::AnimationTextKeyEvent>,
    /// AnimationStack text-key events for one entity (#339 / #231 / #828).
    stack_events: Vec<byroredux_scripting::events::AnimationTextKeyEvent>,
    /// Dedup guard for labels already emitted this visit (#231).
    seen_labels: Vec<FixedString>,
    /// Sorted/deduped channel names across active layers (#251).
    channel_names: Vec<FixedString>,
    /// Pending (name, target, pos, rot, scale) transform writes (#252).
    updates: Vec<(FixedString, EntityId, Vec3, Quat, f32)>,
}

/// Inner implementation of the animation system, parameterised on a
/// reusable [`AnimScratch`] the caller either provides fresh (test path)
/// or persists across frames (production closure path, #1372 / #1725).
fn animation_system_inner(world: &World, dt: f32, scratch: &mut AnimScratch) {
    // Split the scratch into disjoint per-buffer references up front so the
    // player and stack passes each see independent `&mut Vec`s (the borrow
    // checker tracks them as distinct places — no whole-struct aliasing).
    let AnimScratch {
        entities: entities_scratch,
        playback: playback_scratch,
        player_events,
        stack_events,
        seen_labels,
        channel_names: channel_names_scratch,
        updates: updates_scratch,
    } = scratch;
    // Read the clip registry (immutable).
    let Some(registry) = world.try_resource::<AnimationClipRegistry>() else {
        return;
    };
    if registry.is_empty() {
        return;
    }

    // Name component count drives both the SubtreeCache generation check
    // and the NameIndex rebuild trigger. Read it in a scoped acquire so the
    // `Name` lock is released before we touch `NameIndex`: the subtree
    // builder (`ensure_subtree_cache` → `build_subtree_name_map`) acquires
    // `Name` *while `NameIndex` is held* further down, so acquiring
    // `NameIndex` here while holding `Name` would be the reverse order — a
    // cross-thread ABBA deadlock under the parallel scheduler. Acquisition
    // order is fixed at NameIndex-before-Name throughout this system. See
    // invariant #4, #313, #827, and #1410 (the BYRO_LOCK_ORDER_CHECK
    // detector flags exactly this pair). The pre-#827 merge that held one
    // shared `Name` query across the whole prelude traded that brittleness
    // for the lock-order hazard; the count read is O(1) and rebuilds are
    // rare, so re-acquiring `Name` is cheap.
    let current_name_count = world.query::<Name>().map(|q| q.len()).unwrap_or(0);

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
            // ABBA-safe order: take the `NameIndex` write FIRST, then the
            // `Name` read for the iter — NameIndex-before-Name, matching the
            // subtree builder below. A `None` Name query means the Name
            // storage has never existed; same early-return as the pre-#827
            // path. #824 — refill the existing HashMap in place (`clear()`
            // keeps the bucket array; `reserve(N)` forces one rehash on the
            // cold-start path so the refill doesn't growth-double).
            let mut idx = world.resource_mut::<NameIndex>();
            let Some(name_query) = world.query::<Name>() else {
                return;
            };
            idx.map.clear();
            idx.map.reserve(current_name_count);
            for (entity, name_comp) in name_query.iter() {
                idx.map.insert(name_comp.0, entity);
            }
            idx.generation = current_name_count;
        }
    }

    let name_index = world.try_resource::<NameIndex>().unwrap();

    // Iterate all animation players and apply.
    let Some(player_query) = world.query_mut::<AnimationPlayer>() else {
        return;
    };
    // Collect into the caller-supplied scratch buffer rather than a fresh
    // Vec. `clear()` + `extend()` reuses the backing allocation; on the
    // warm path (make_animation_system closure) this avoids the 0→N
    // re-growth that `collect()` / `Vec::new()` would cause (#1372).
    entities_scratch.clear();
    entities_scratch.extend(player_query.iter().map(|(e, _)| e));
    drop(player_query);

    // Phase 1: Advance all players and collect playback state.
    // Single lock acquisition for AnimationPlayer, held for the entire batch.
    playback_scratch.clear();
    {
        let mut player_query = world.query_mut::<AnimationPlayer>().unwrap();
        for &entity in entities_scratch.iter() {
            let player = player_query.get_mut(entity).unwrap();
            let clip_handle = player.clip_handle;
            let root_entity_opt = player.root_entity;
            let Some(clip) = registry.get(clip_handle) else {
                continue;
            };
            advance_time(player, clip, dt);
            playback_scratch.push(PlaybackState {
                entity,
                clip_handle,
                root_entity: root_entity_opt,
                current_time: player.local_time,
                prev_time: player.prev_time,
                reverse_direction: player.reverse_direction,
            });
        }
    } // AnimationPlayer lock released here

    // Emit text key events for AnimationPlayer entities (#211 / #339).
    {
        use byroredux_scripting::events::{AnimationTextKeyEvent, AnimationTextKeyEvents};
        let mut eq = world.query_mut::<AnimationTextKeyEvents>().unwrap();
        for ps in playback_scratch.iter() {
            let Some(clip) = registry.get(ps.clip_handle) else {
                continue;
            };
            player_events.clear();
            visit_text_key_events(
                clip,
                ps.prev_time,
                ps.current_time,
                ps.reverse_direction,
                |time, sym| {
                    player_events.push(AnimationTextKeyEvent { label: sym, time });
                },
            );
            if !player_events.is_empty() {
                // `clone()` instead of `mem::take` so the scratch keeps its
                // high-water-mark capacity across iterations *and* frames
                // (#828 / #1725). `AnimationTextKeyEvent` is Copy — the
                // clone is a memcpy of N × 8 bytes.
                eq.insert(ps.entity, AnimationTextKeyEvents(player_events.clone()));
            }
        }
    }

    // Phase 2: Apply channels using pre-computed playback state.
    for ps in playback_scratch.iter() {
        let entity = ps.entity;
        let Some(clip) = registry.get(ps.clip_handle) else {
            continue;
        };
        let current_time = ps.current_time;

        // Scoped name lookup — persisted across frames (#278). One
        // `SubtreeCache` acquisition per entity on the cache-hit path
        // (#2924).
        let subtree_ref = subtree_cache(world, ps.root_entity);
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
            let mut accum_root_animated = false;
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
                        let (anim_pos, _) = split_root_motion(pos);
                        transform.translation = anim_pos;
                        root_motion +=
                            sampled_root_motion_delta(clip, channel, ps.prev_time, current_time);
                        accum_root_animated = true;
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
            // Ground the skeleton (accum-root reset). The accumulation root
            // (e.g. FNV `Bip01`) is a root-motion *carrier*, not a pose node —
            // its skeleton *bind* translation (FNV rigs `Bip01` at Z≈67.77,
            // pelvis height) must not lift the body. When the clip animates the
            // accum root, the split above already reset it; but most idle clips
            // animate only `Bip01 NonAccum` (which holds the real root pose) and
            // leave the accum root untouched, so its bind lift stacks onto
            // NonAccum and floats the actor ~68 units off the floor. Zero the
            // accum-root translation in that case (rotation kept). Matches the
            // Gamebryo accum/non-accum model (cf. OpenMW `ResetAccumRootCallback`).
            //
            // #2924 / PERF-D1-02 — this runs inside the channel batch's
            // still-live `transform_query`, not after it. It used to
            // re-acquire `query_mut::<Transform>()` a few statements later,
            // taking the same write lock twice per animated entity per
            // frame — and on the COMMON branch, since the block exists
            // precisely because most idle clips leave the accum root
            // untouched. That eroded the one-guard-per-entity-per-component
            // invariant #53 landed, and widened the surface for the ABBA
            // hazards #313 / #827 / #1410 guard against. `write_root_motion`
            // now runs after the guard drops, so `RootMotionDelta` is still
            // taken with nothing else held — no new lock edge either way.
            if !accum_root_animated {
                if let Some(accum_entity) = clip.accum_root_name.as_ref().and_then(&resolve_entity)
                {
                    if let Some(t) = transform_query.get_mut(accum_entity) {
                        t.translation = Vec3::ZERO;
                    }
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

        // #2221 — texture flipbook cycle-position. Only picks among the
        // handles `anim_convert::attach_animation_sinks` already resolved
        // at clip-attach time; never touches the texture registry itself.
        if !clip.texture_flip_channels.is_empty() {
            apply_texture_flip_channels(
                world,
                &clip.texture_flip_channels,
                current_time,
                &resolve_entity,
            );
        }
    }

    // ── AnimationStack processing (multi-layer blending) ──────────────
    let Some(stack_query) = world.query_mut::<AnimationStack>() else {
        return;
    };
    // Reuse entities_scratch (now free — player list is no longer needed)
    // for the stack entity list. Same clear+extend pattern (#1372).
    entities_scratch.clear();
    entities_scratch.extend(stack_query.iter().map(|(e, _)| e));
    drop(stack_query);

    // Scratch buffers (`channel_names_scratch`, `updates_scratch`,
    // `stack_events`, `seen_labels`) are the destructured `AnimScratch`
    // fields — reused across entities within a frame (cleared per
    // iteration, #251 / #252 / #828) and across frames (#1725). Pre-#828
    // they were declared inside the loop and re-allocated per entity;
    // pre-#1725 they were function-local and re-grown 0→N per frame.
    use byroredux_scripting::events::AnimationTextKeyEvent;

    for entity in entities_scratch.iter().copied() {
        // Phase 1: advance all layers (write lock).
        {
            let mut sq = world.query_mut::<AnimationStack>().unwrap();
            let stack = sq.get_mut(entity).unwrap();
            advance_stack(stack, &registry, dt);
        }

        // Ensure subtree cache is populated for this stack's root before we
        // take the AnimationStack read lock below (cache rebuild acquires a
        // write lock on SubtreeCache, separate from AnimationStack). The
        // `AnimationStack` guard is dropped first, so the ordering this
        // block exists to enforce is unchanged; `subtree_cache` returns the
        // guard the lookup below needs instead of acquiring twice (#2924).
        let stack_root_entity = {
            let sq = world.query::<AnimationStack>().unwrap();
            sq.get(entity).unwrap().root_entity
        };
        let subtree_ref2 = subtree_cache(world, stack_root_entity);

        // Phase 2: single read lock for everything that reads AnimationStack
        // (#287 — was 4 separate acquisitions, now 1). Collect all outputs
        // into owned / registry-borrowed data so the lock drops before any
        // writes. Dominant info is stored as (clip_handle, local_time) —
        // NO channel Vec clones (#265).
        // Text-key event scratches (#339 / #231 / #828 / #1725) are the
        // persisted `AnimScratch` buffers; clear before each visit.
        // `seen_labels` is also cleared internally by
        // `visit_stack_text_events`, but we clear here too to keep the
        // contract obvious.
        stack_events.clear();
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
            visit_stack_text_events(stack, &registry, seen_labels, |time, sym| {
                stack_events.push(AnimationTextKeyEvent { label: sym, time });
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
            for &channel_name in channel_names_scratch.iter() {
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
        // `clone()` (not `mem::take`) so the scratch retains its capacity
        // across iterations and frames — `mem::take` swaps in a zero-cap
        // Vec and forces the next visitor to grow from scratch.
        // `AnimationTextKeyEvent` is Copy. See #828 / #1725.
        if !stack_events.is_empty() {
            use byroredux_scripting::events::AnimationTextKeyEvents;
            let mut eq = world.query_mut::<AnimationTextKeyEvents>().unwrap();
            eq.insert(entity, AnimationTextKeyEvents(stack_events.clone()));
        }

        // Phase 3b: apply blended transforms with root motion splitting (AR-02).
        let mut tq = world.query_mut::<Transform>().unwrap();
        let mut root_motion = Vec3::ZERO;
        for &(name, target, pos, rot, scale) in updates_scratch.iter() {
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
                if !clip.texture_flip_channels.is_empty() {
                    apply_texture_flip_channels(
                        world,
                        &clip.texture_flip_channels,
                        time,
                        &stack_resolve,
                    );
                }
            }
        }
    }
}

/// Animation system (plain function, allocates fresh scratch each call).
///
/// Kept for test ergonomics — tests call this directly and don't need
/// persistent scratch. Production code uses [`make_animation_system`],
/// which captures reusable buffers in the closure state.
#[cfg(test)]
pub(crate) fn animation_system(world: &World, dt: f32) {
    animation_system_inner(world, dt, &mut AnimScratch::default());
}

/// Animation system factory — returns a closure that captures two scratch
/// buffers and reuses their backing allocation across frames (#1372).
///
/// Equivalent behavior to [`animation_system`]; use this when wiring the
/// system into the scheduler so that every per-frame scratch `Vec` in
/// [`AnimScratch`] (entity lists, playback table, and both paths'
/// text-event buffers — #1725) is eliminated after the first warm-up frame.
pub(crate) fn make_animation_system() -> impl FnMut(&World, f32) + Send + Sync {
    let mut scratch = AnimScratch::default();
    move |world: &World, dt: f32| {
        animation_system_inner(world, dt, &mut scratch);
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

    /// #2399 — same entity, same two targets as
    /// `diffuse_and_emissive_coexist_on_same_entity`, but with the
    /// channel list in the *opposite* authored order (`[Emissive,
    /// Diffuse]`). Pre-fix, `apply_color_channels` lazily acquired each
    /// target's write guard on first encounter while scanning in list
    /// order, so this ordering acquired `AnimatedEmissiveColor` before
    /// `AnimatedDiffuseColor` — the reverse of the other test — making
    /// the pairwise lock order content-determined instead of fixed by
    /// code. The fixed version applies each target in one declared-order
    /// pass regardless of list order, so this test's outcome must be
    /// identical to the forward-order test: both values land, in either
    /// list order. (The acquisition-order fix itself is a structural
    /// property of the rewritten function — verified by inspection, not
    /// re-testable here, since the process-global lock-order graph the
    /// CONC-D3 detector uses lives in `byroredux-core`'s
    /// `lock_tracker::global_order` module and its test-enable hooks are
    /// `pub(super)`-scoped to that crate's own `ecs` module tests, not
    /// reachable from this crate.)
    #[test]
    fn diffuse_and_emissive_coexist_regardless_of_authored_order() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, AnimatedDiffuseColor(Vec3::ZERO));
        world.insert(e, AnimatedEmissiveColor(Vec3::ZERO));

        let mut pool = StringPool::new();
        let name = pool.intern("NeonSign");
        let channels = vec![
            (
                name,
                single_key_channel(ColorTarget::Emissive, Vec3::new(0.9, 0.8, 0.7)),
            ),
            (
                name,
                single_key_channel(ColorTarget::Diffuse, Vec3::new(0.1, 0.2, 0.3)),
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

    /// #2399 — mirrors the `apply_color_channels` reversed-order test.
    /// Pre-fix, `apply_float_channels` lazily acquired `AnimatedAlpha` /
    /// `AnimatedUvTransform` in whichever order the channel list listed
    /// them, making the pairwise acquisition order between the two
    /// storages content-determined. A clip with `[UvOffsetU, Alpha]`
    /// (this test) acquired the opposite order from one listing
    /// `[Alpha, UvOffsetU]`. The fixed version applies Alpha then UV in
    /// one declared-order pass each regardless of list order, so both
    /// orderings must produce the same result.
    #[test]
    fn alpha_and_uv_write_correctly_regardless_of_authored_order() {
        let (world, entity, name) = world_with_sinks();
        let channels = vec![
            (name, const_channel(FloatTarget::UvOffsetU, 0.25)),
            (name, const_channel(FloatTarget::Alpha, 0.5)),
        ];
        apply_float_channels(&world, &channels, 0.0, &resolve_to(entity));
        let aq = world.query::<AnimatedAlpha>().unwrap();
        let uq = world.query::<AnimatedUvTransform>().unwrap();
        assert_eq!(aq.get(entity).unwrap().0, 0.5);
        assert_eq!(uq.get(entity).unwrap().offset.x, 0.25);
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
            (
                name,
                const_channel(FloatTarget::UvRotation, std::f32::consts::FRAC_PI_2),
            ),
        ];
        apply_float_channels(&world, &channels, 0.0, &resolve_to(entity));
        let q = world.query::<AnimatedUvTransform>().unwrap();
        let t = q.get(entity).unwrap();
        assert_eq!(t.scale.x, 2.0);
        assert_eq!(t.scale.y, 0.5);
        assert!((t.rotation - std::f32::consts::FRAC_PI_2).abs() < 1e-4);
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

#[cfg(test)]
mod texture_flip_dispatch_tests {
    use super::*;
    use byroredux_core::animation::{AnimFloatKey, TextureFlipChannel};
    use byroredux_core::ecs::{AnimatedTextureFlip, TextureFlipEntry};
    use std::sync::Arc;

    fn resolve_to(entity: EntityId) -> impl Fn(&FixedString) -> Option<EntityId> {
        move |_sym: &FixedString| Some(entity)
    }

    fn flip_channel(source_count: usize, value: f32) -> TextureFlipChannel {
        TextureFlipChannel {
            texture_slot: 0,
            source_paths: (0..source_count)
                .map(|i| Arc::from(format!("frame{i}.dds")))
                .collect(),
            keys: vec![AnimFloatKey { time: 0.0, value }],
        }
    }

    /// #2221 — the per-frame system only picks among the handles already
    /// resolved at attach time (`anim_convert::attach_animation_sinks`);
    /// it never calls into the texture registry itself.
    #[test]
    fn picks_the_pre_resolved_handle_by_curve_index() {
        let mut world = World::new();
        let entity = world.spawn();
        world.insert(
            entity,
            AnimatedTextureFlip(vec![TextureFlipEntry {
                texture_slot: 0,
                handles: vec![10, 20, 30],
                current_index: 0,
            }]),
        );
        let mut pool = StringPool::new();
        let name = pool.intern("target");
        let channels = vec![(name, flip_channel(3, 2.0))];
        apply_texture_flip_channels(&world, &channels, 0.0, &resolve_to(entity));

        let q = world.query::<AnimatedTextureFlip>().unwrap();
        let flip = q.get(entity).unwrap();
        assert_eq!(flip.handle_for_slot(0), Some(30));
    }

    /// A channel targeting a `texture_slot` the entity's
    /// `AnimatedTextureFlip` has no entry for must not create one or
    /// panic — mirrors `apply_float_channels`'s missing-sink posture.
    #[test]
    fn unmatched_texture_slot_is_a_silent_noop() {
        let mut world = World::new();
        let entity = world.spawn();
        world.insert(
            entity,
            AnimatedTextureFlip(vec![TextureFlipEntry {
                texture_slot: 0,
                handles: vec![10, 20],
                current_index: 0,
            }]),
        );
        let mut pool = StringPool::new();
        let name = pool.intern("target");
        let mut channel = flip_channel(2, 1.0);
        channel.texture_slot = 4; // GLOW_MAP — no matching entry
        let channels = vec![(name, channel)];
        apply_texture_flip_channels(&world, &channels, 0.0, &resolve_to(entity));

        let q = world.query::<AnimatedTextureFlip>().unwrap();
        let flip = q.get(entity).unwrap();
        assert_eq!(
            flip.handle_for_slot(0),
            Some(10),
            "slot 0's entry must stay untouched by a slot-4 channel"
        );
    }

    /// Missing-sink case (no `AnimatedTextureFlip` on the entity at
    /// all) must not panic.
    #[test]
    fn missing_sink_component_is_a_silent_noop() {
        let mut world = World::new();
        let entity = world.spawn();
        let mut pool = StringPool::new();
        let name = pool.intern("target");
        let channels = vec![(name, flip_channel(2, 1.0))];
        apply_texture_flip_channels(&world, &channels, 0.0, &resolve_to(entity));
        assert!(world.query::<AnimatedTextureFlip>().is_none());
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
        TranslationKey,
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

    fn cart_com_channel() -> TransformChannel {
        TransformChannel {
            translation_keys: vec![
                TranslationKey {
                    time: 0.0,
                    value: Vec3::new(-30.0, 140.0, 200.0),
                    forward: Vec3::ZERO,
                    backward: Vec3::ZERO,
                    tbc: None,
                },
                TranslationKey {
                    time: 1.0,
                    value: Vec3::new(0.0, 70.0, 0.0),
                    forward: Vec3::ZERO,
                    backward: Vec3::ZERO,
                    tbc: None,
                },
            ],
            translation_type: KeyType::Linear,
            rotation_keys: Vec::new(),
            rotation_type: KeyType::Linear,
            scale_keys: Vec::new(),
            scale_type: KeyType::Linear,
            priority: 0,
        }
    }

    #[test]
    fn cart_com_absolute_pose_becomes_per_frame_horizontal_delta() {
        let channel = cart_com_channel();
        let clip = AnimationClip {
            name: "cart exit".into(),
            duration: 1.0,
            cycle_type: CycleType::Clamp,
            frequency: 1.0,
            weight: 1.0,
            accum_root_name: None,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: Vec::new(),
        };

        assert_eq!(
            sampled_root_motion_delta(&clip, &channel, 0.25, 0.75),
            Vec3::new(15.0, 0.0, -100.0)
        );
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

    #[test]
    fn animation_system_emits_cart_com_delta_and_keeps_vertical_pose_on_bone() {
        let bone_name = "NPC COM [COM ]";
        let (mut world, root, bone, _) = build_skeleton_and_clip(bone_name);
        world.register::<RootMotionDelta>();
        let bone_name = world.resource::<StringPool>().get(bone_name).unwrap();
        let mut channels = HashMap::new();
        channels.insert(bone_name, cart_com_channel());
        let handle = world
            .resource_mut::<AnimationClipRegistry>()
            .add(AnimationClip {
                name: "cart exit".into(),
                duration: 1.0,
                cycle_type: CycleType::Clamp,
                frequency: 1.0,
                weight: 1.0,
                accum_root_name: Some(bone_name),
                channels,
                float_channels: Vec::new(),
                color_channels: Vec::new(),
                bool_channels: Vec::new(),
                texture_flip_channels: Vec::new(),
                text_keys: Vec::new(),
            });
        world.insert(root, RootMotionDelta(Vec3::ZERO));
        world.insert(root, AnimationPlayer::new(handle).with_root(root));

        animation_system(&world, 0.5);

        assert_eq!(
            world.get::<RootMotionDelta>(root).unwrap().0,
            Vec3::new(15.0, 0.0, -100.0)
        );
        assert_eq!(
            world.get::<Transform>(bone).unwrap().translation,
            Vec3::new(0.0, 105.0, 0.0),
            "COM height remains skeletal pose while horizontal travel is extracted"
        );
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

#[cfg(test)]
mod sink_lifecycle_end_to_end_tests {
    //! #2221 — the guard that would have caught the original defect.
    //!
    //! Every pre-existing test in this file inserts the `Animated*`
    //! sink components by hand before calling `apply_*_channels`.
    //! Production never did: `attach_animation_sinks` did not exist, so
    //! the whole non-transform half of the animation system sampled
    //! correctly and wrote into nothing. Helper-level routing tests
    //! cannot see that — only a test that runs *attach then apply*,
    //! without hand-inserting a sink, can.
    //!
    //! If someone later removes the attach call from a spawn path, the
    //! routing tests still pass and this one fails. That asymmetry is
    //! the entire point.

    use super::*;
    use crate::anim_convert::attach_animation_sinks;
    use byroredux_core::animation::{
        AnimBoolKey, AnimColorKey, AnimFloatKey, BoolChannel, ColorChannel, ColorTarget,
        FloatChannel,
    };
    use byroredux_core::ecs::{Children, Name, World};
    use byroredux_core::math::Vec3;
    use byroredux_core::string::StringPool;

    #[test]
    fn attach_then_apply_lands_the_value_without_a_hand_inserted_sink() {
        let mut world = World::new();
        let mut pool = StringPool::new();

        let root = world.spawn();
        let child = world.spawn();
        let root_name = pool.intern("Root");
        let child_name = pool.intern("Fire01");
        world.insert(root, Name(root_name));
        world.insert(child, Name(child_name));
        world.insert(root, Children(vec![child]));

        // Two keys apiece so sampling at t=1.0 differs from the t=0
        // seed — otherwise the assertion could pass by reading the seed
        // back and the apply pass would go unverified.
        let floats = vec![(
            child_name,
            FloatChannel {
                target: FloatTarget::Alpha,
                keys: vec![
                    AnimFloatKey {
                        time: 0.0,
                        value: 0.0,
                    },
                    AnimFloatKey {
                        time: 1.0,
                        value: 1.0,
                    },
                ],
            },
        )];
        let colors = vec![(
            child_name,
            ColorChannel {
                target: ColorTarget::Emissive,
                keys: vec![
                    AnimColorKey {
                        time: 0.0,
                        value: Vec3::ZERO,
                    },
                    AnimColorKey {
                        time: 1.0,
                        value: Vec3::new(1.0, 0.5, 0.25),
                    },
                ],
            },
        )];
        let bools = vec![(
            child_name,
            BoolChannel {
                keys: vec![AnimBoolKey {
                    time: 0.0,
                    value: false,
                }],
            },
        )];

        attach_animation_sinks(&mut world, &bools, &floats, &colors, &[], None, None, root);

        let resolve = |n: &FixedString| -> Option<EntityId> {
            if *n == child_name {
                Some(child)
            } else {
                None
            }
        };
        apply_float_channels(&world, &floats, 1.0, &resolve);
        apply_color_channels(&world, &colors, 1.0, &resolve);

        let alpha = world.query::<AnimatedAlpha>().unwrap();
        assert_eq!(
            alpha.get(child).unwrap().0,
            1.0,
            "alpha sampled at t=1.0 must reach the sink the attach pass created"
        );
        drop(alpha);

        let emissive = world.query::<AnimatedEmissiveColor>().unwrap();
        assert_eq!(emissive.get(child).unwrap().0, Vec3::new(1.0, 0.5, 0.25));
        drop(emissive);

        // The bool sink needs no apply pass — it is seeded from the clip
        // at t=0, which is what keeps frame 0 from flashing the wrong
        // visibility before the system's first write.
        let vis = world.query::<AnimatedVisibility>().unwrap();
        assert!(
            !vis.get(child).unwrap().0,
            "visibility sink must be seeded from the clip, not defaulted to true"
        );
    }
}
