//! NIF animation clip conversion and subtree name map building.

use byroredux_core::animation::{
    AnimBoolKey, AnimColorKey, AnimFloatKey, AnimationClip, BoolChannel, ColorChannel, ColorTarget,
    CycleType, FloatChannel, FloatTarget, KeyType, RotationKey, ScaleKey, TextureFlipChannel,
    TransformChannel, TranslationKey,
};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Children, Name, World};
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::{FixedString, StringPool};
use byroredux_renderer::VulkanContext;
use std::collections::HashMap;

use crate::asset_provider::TextureProvider;

/// Build a scoped name→entity map by walking the subtree rooted at `root`.
pub(crate) fn build_subtree_name_map(
    world: &World,
    root: EntityId,
) -> HashMap<FixedString, EntityId> {
    let mut map = HashMap::new();

    // Include the root itself.
    if let Some(nq) = world.query::<Name>() {
        if let Some(name) = nq.get(root) {
            map.insert(name.0, root);
        }
    }

    // BFS through children.
    let children_q = world.query::<Children>();
    let name_q = world.query::<Name>();
    let Some(ref cq) = children_q else { return map };

    let mut queue = vec![root];
    while let Some(entity) = queue.pop() {
        if let Some(children) = cq.get(entity) {
            for &child in &children.0 {
                if let Some(ref nq) = name_q {
                    if let Some(name) = nq.get(child) {
                        map.insert(name.0, child);
                    }
                }
                queue.push(child);
            }
        }
    }

    map
}

/// Insert the `Animated*` sink components a clip's non-transform
/// channels need, on the entities those channels actually target.
///
/// # Why this exists (#2221)
///
/// `animation_system` writes non-transform channels through
/// `query_mut::<T>().get_mut(target)` — a *write* into an existing
/// component, never an insert. Systems take `&World`, so they
/// structurally cannot create the sink themselves. The result before
/// this function existed: every non-transform channel parsed
/// correctly, resolved its target entity correctly, sampled correctly,
/// and then dropped the value because no production spawn path had
/// ever inserted the component. The only `world.insert(e, Animated…)`
/// calls in the tree were inside `#[cfg(test)]` blocks, which is why
/// the routing unit tests all passed while nothing animated in-game.
///
/// Attach-time is the correct place: it is the one moment we hold
/// `&mut World`, know the clip, and know the subtree root the channel
/// names resolve against.
///
/// # Contract
///
/// - **Only sinks the clip actually needs.** A clip carrying one alpha
///   channel gets `AnimatedAlpha` on one entity — not all seven
///   components on the whole subtree. Sparse storages stay sparse.
/// - **Never overwrite an existing sink.** Two clips can target the
///   same entity (an idle plus a flicker); the second attach must not
///   reset the first's live value.
/// - **Seeded from `t = 0`, not from a neutral default.** The sink is
///   read by `build_render_data` in the same frame it is created, so a
///   default-initialised component would show one frame of the wrong
///   visibility/alpha/tint before the system's first write.
/// - **`LightSource` is deliberately excluded.** The `Light*` channel
///   targets write into an existing light; synthesising one here would
///   spawn a phantom light at every `NiLightColorController` target
///   that has no `LIGH` record behind it.
/// - `TextureFlipChannel` (#2221's flipbook half) resolves its
///   `source_paths` to bindless texture handles HERE, once, via
///   `resolve_texture` — the only production call site with a
///   `TextureProvider` + `VulkanContext` in scope. Storing pre-resolved
///   handles means the per-frame animation system (`&World` only, no
///   GPU-resource access) can pick among them by index without ever
///   touching the texture registry.
///
/// # Why slices, not `&AnimationClip`
///
/// Channels are passed as slices rather than as a whole `&AnimationClip`
/// because the caller must release its `AnimationClipRegistry` read
/// guard before this function takes `&mut World`. Cloning the four
/// non-transform channel vectors out of the registry is cheap — the
/// bulk of a clip is its per-bone `TransformChannel` map, which this
/// path never touches.
///
/// `ctx` / `tex_provider` are `Option` rather than required so tests that
/// only exercise the bool/float/color sinks don't need a live Vulkan
/// device. Every production caller passes `Some` (all three attach
/// sites hold both already, for the mesh's other texture slots); `None`
/// skips flipbook resolution only — the other three channel kinds still
/// attach normally.
#[allow(clippy::too_many_arguments)]
pub(crate) fn attach_animation_sinks(
    world: &mut World,
    bool_channels: &[(FixedString, BoolChannel)],
    float_channels: &[(FixedString, FloatChannel)],
    color_channels: &[(FixedString, ColorChannel)],
    texture_flip_channels: &[(FixedString, TextureFlipChannel)],
    ctx: Option<&mut VulkanContext>,
    tex_provider: Option<&TextureProvider>,
    root: EntityId,
) {
    use byroredux_core::animation::{
        sample_bool_channel, sample_color_channel, sample_float_channel, sample_texture_flip_index,
    };
    use byroredux_core::ecs::{
        AnimatedAlpha, AnimatedAmbientColor, AnimatedDiffuseColor, AnimatedEmissiveColor,
        AnimatedMorphWeights, AnimatedShaderColor, AnimatedShaderFloat, AnimatedSpecularColor,
        AnimatedTextureFlip, AnimatedUvTransform, AnimatedVisibility, TextureFlipEntry,
    };

    // Nothing to do for a pure transform clip — the overwhelmingly
    // common case (every skeletal animation). Bail before paying for
    // the subtree walk.
    if bool_channels.is_empty()
        && float_channels.is_empty()
        && color_channels.is_empty()
        && texture_flip_channels.is_empty()
    {
        return;
    }

    let names = build_subtree_name_map(world, root);

    // Pass 1 — resolve channels to (entity, seeded value). Grouped per
    // sink so pass 2 can take one storage lock per component type.
    let mut visibility: Vec<(EntityId, AnimatedVisibility)> = Vec::new();
    let mut alpha: Vec<(EntityId, AnimatedAlpha)> = Vec::new();
    let mut shader_float: Vec<(EntityId, AnimatedShaderFloat)> = Vec::new();
    let mut diffuse: Vec<(EntityId, AnimatedDiffuseColor)> = Vec::new();
    let mut ambient: Vec<(EntityId, AnimatedAmbientColor)> = Vec::new();
    let mut specular: Vec<(EntityId, AnimatedSpecularColor)> = Vec::new();
    let mut emissive: Vec<(EntityId, AnimatedEmissiveColor)> = Vec::new();
    let mut shader_color: Vec<(EntityId, AnimatedShaderColor)> = Vec::new();
    // UV and morph accumulate across channels: five FloatTargets share
    // one `AnimatedUvTransform`, and N `MorphWeight(idx)` channels share
    // one weight vector. Keyed by entity so each gets a single fully
    // seeded component rather than one per channel.
    let mut uv: HashMap<EntityId, AnimatedUvTransform> = HashMap::new();
    let mut morph: HashMap<EntityId, AnimatedMorphWeights> = HashMap::new();

    for (name, channel) in bool_channels {
        if let Some(&e) = names.get(name) {
            visibility.push((e, AnimatedVisibility(sample_bool_channel(channel, 0.0))));
        }
    }

    for (name, channel) in color_channels {
        let Some(&e) = names.get(name) else { continue };
        let v = sample_color_channel(channel, 0.0);
        match channel.target {
            ColorTarget::Diffuse => diffuse.push((e, AnimatedDiffuseColor(v))),
            ColorTarget::Ambient => ambient.push((e, AnimatedAmbientColor(v))),
            ColorTarget::Specular => specular.push((e, AnimatedSpecularColor(v))),
            ColorTarget::Emissive => emissive.push((e, AnimatedEmissiveColor(v))),
            ColorTarget::ShaderColor => shader_color.push((e, AnimatedShaderColor(v))),
            // Writes into an existing LightSource — see the contract note.
            ColorTarget::LightDiffuse | ColorTarget::LightAmbient => {}
        }
    }

    for (name, channel) in float_channels {
        let Some(&e) = names.get(name) else { continue };
        let v = sample_float_channel(channel, 0.0);
        match channel.target {
            FloatTarget::Alpha => alpha.push((e, AnimatedAlpha(v))),
            FloatTarget::UvOffsetU
            | FloatTarget::UvOffsetV
            | FloatTarget::UvScaleU
            | FloatTarget::UvScaleV
            | FloatTarget::UvRotation => {
                let t = uv.entry(e).or_insert_with(AnimatedUvTransform::identity);
                match channel.target {
                    FloatTarget::UvOffsetU => t.offset.x = v,
                    FloatTarget::UvOffsetV => t.offset.y = v,
                    FloatTarget::UvScaleU => t.scale.x = v,
                    FloatTarget::UvScaleV => t.scale.y = v,
                    FloatTarget::UvRotation => t.rotation = v,
                    _ => unreachable!(),
                }
            }
            FloatTarget::ShaderFloat => shader_float.push((e, AnimatedShaderFloat(v))),
            FloatTarget::MorphWeight(idx) => {
                morph
                    .entry(e)
                    .or_insert_with(|| AnimatedMorphWeights(Vec::new()))
                    .set(idx as usize, v);
            }
            // Writes into an existing LightSource — see the contract note.
            FloatTarget::LightDimmer | FloatTarget::LightIntensity | FloatTarget::LightRadius => {}
        }
    }

    // Texture flipbooks — resolve `source_paths` to bindless handles
    // now (the only point with `ctx`/`tex_provider` in scope), sample
    // the cycle-position curve at t=0 for the seed index, and group by
    // entity (mirroring `uv`/`morph` above) since one shape can in
    // principle carry more than one flip controller.
    let mut texture_flip: HashMap<EntityId, Vec<TextureFlipEntry>> = HashMap::new();
    if let (Some(ctx), Some(tex_provider)) = (ctx, tex_provider) {
        for (name, channel) in texture_flip_channels {
            let Some(&e) = names.get(name) else { continue };
            if channel.source_paths.is_empty() {
                continue;
            }
            let handles: Vec<u32> = channel
                .source_paths
                .iter()
                .map(|path| crate::asset_provider::resolve_texture(ctx, tex_provider, Some(path)))
                .collect();
            let current_index = sample_texture_flip_index(channel, 0.0);
            texture_flip.entry(e).or_default().push(TextureFlipEntry {
                texture_slot: channel.texture_slot,
                handles,
                current_index,
            });
        }
    }

    // Pass 2 — insert the ones that aren't already there.
    insert_missing_sinks(world, visibility);
    insert_missing_sinks(world, alpha);
    insert_missing_sinks(world, shader_float);
    insert_missing_sinks(world, diffuse);
    insert_missing_sinks(world, ambient);
    insert_missing_sinks(world, specular);
    insert_missing_sinks(world, emissive);
    insert_missing_sinks(world, shader_color);
    insert_missing_sinks(world, uv.into_iter().collect::<Vec<_>>());
    insert_missing_sinks(world, morph.into_iter().collect::<Vec<_>>());
    insert_missing_sinks(
        world,
        texture_flip
            .into_iter()
            .map(|(e, entries)| (e, AnimatedTextureFlip(entries)))
            .collect::<Vec<_>>(),
    );
}

/// Insert each `(entity, component)` pair whose entity does not already
/// carry that component.
///
/// The existence check is scoped so the read guard is dropped before
/// `World::insert` takes `&mut self` — holding both would deadlock on
/// the storage's `RwLock`. A missing storage (`query` → `None`) means
/// no entity has the component yet, so every pair is inserted.
fn insert_missing_sinks<T: byroredux_core::ecs::Component>(
    world: &mut World,
    items: Vec<(EntityId, T)>,
) {
    if items.is_empty() {
        return;
    }
    let mut pending = Vec::with_capacity(items.len());
    {
        let existing = world.query::<T>();
        for (entity, component) in items {
            let already_present = existing.as_ref().is_some_and(|q| q.get(entity).is_some());
            if !already_present {
                pending.push((entity, component));
            }
        }
    }
    for (entity, component) in pending {
        world.insert(entity, component);
    }
}

/// Convert a NIF animation clip (byroredux_nif types) to a core animation clip (glam types).
///
/// Channel names are interned into `pool` at conversion time so the animation
/// hot path can use `FixedString` (integer comparison, zero allocation). #340.
/// Clamp a raw `NiControllerSequence.frequency` to a playback rate the
/// animation clock can integrate (#3258).
///
/// Non-finite or non-positive values fall back to `1.0`, Gamebryo's default
/// rate. See the call site in [`convert_nif_clip`] for why this is resolved
/// here rather than defended against downstream.
fn sanitized_clip_frequency(frequency: f32) -> f32 {
    if frequency.is_finite() && frequency > 0.0 {
        frequency
    } else {
        1.0
    }
}

/// Clamp a raw `NiControllerSequence.stop_time - start_time` to a duration
/// the animation clock can integrate (#3432, sibling of `frequency` above).
///
/// `CycleType::Reverse`'s `fold_reverse_time` folds a NaN duration into a
/// permanently-NaN `local_time` (`NaN <= 0.0` is false, so its zero-or-less
/// guard doesn't catch it), and `CycleType::Loop`'s modulo has the same
/// latch. Every cycle arm already treats a non-positive duration as
/// "no wrap / no fold", so non-finite (and negative) collapses to `0.0`
/// here — resolved once at the translate boundary rather than re-derived
/// at render time (NIFAL doctrine).
fn sanitized_clip_duration(duration: f32) -> f32 {
    if duration.is_finite() && duration > 0.0 {
        duration
    } else {
        0.0
    }
}

/// Clamp a raw `NiControllerSequence.weight` the same way (#3432).
///
/// `sample_blended_transform`'s per-layer skip (`ew < 0.001`) is
/// NaN-transparent, so a NaN weight is never filtered out and poisons
/// `total_weight` — and therefore every blended bone transform on the
/// channel — instead of being skipped. `1.0` is nif.xml's own default.
fn sanitized_clip_weight(weight: f32) -> f32 {
    if weight.is_finite() {
        weight
    } else {
        1.0
    }
}

pub(crate) fn convert_nif_clip(
    nif: &byroredux_nif::anim::AnimationClip,
    pool: &mut StringPool,
) -> AnimationClip {
    use byroredux_nif::anim as na;

    let cycle_type = match nif.cycle_type {
        na::CycleType::Clamp => CycleType::Clamp,
        na::CycleType::Loop => CycleType::Loop,
        na::CycleType::Reverse => CycleType::Reverse,
    };

    let channels = nif
        .channels
        .iter()
        .map(|(name, ch)| {
            let sym = pool.intern(name);
            let convert_key_type = |kt: byroredux_nif::blocks::interpolator::KeyType| match kt {
                byroredux_nif::blocks::interpolator::KeyType::Linear => KeyType::Linear,
                byroredux_nif::blocks::interpolator::KeyType::Quadratic => KeyType::Quadratic,
                byroredux_nif::blocks::interpolator::KeyType::Tbc => KeyType::Tbc,
                byroredux_nif::blocks::interpolator::KeyType::XyzRotation => KeyType::Linear,
                // KEY_CONST = stepped hold (no interpolation). Previously
                // collapsed to Linear, which wrongly LERPed step keys
                // (LC-D5-02 / #1441). The NIF XYZ-Euler scalar sampler bakes
                // its own Constant handling; this maps the runtime TRS path.
                byroredux_nif::blocks::interpolator::KeyType::Constant => KeyType::Const,
            };

            let translation_keys = ch
                .translation_keys
                .iter()
                .map(|k| TranslationKey {
                    time: k.time,
                    value: Vec3::from_array(k.value),
                    forward: Vec3::from_array(k.forward),
                    backward: Vec3::from_array(k.backward),
                    tbc: k.tbc,
                })
                .collect();

            let rotation_keys = ch
                .rotation_keys
                .iter()
                .map(|k| RotationKey {
                    time: k.time,
                    value: Quat::from_xyzw(k.value[0], k.value[1], k.value[2], k.value[3]),
                    tbc: k.tbc,
                })
                .collect();

            let scale_keys = ch
                .scale_keys
                .iter()
                .map(|k| ScaleKey {
                    time: k.time,
                    value: k.value,
                    forward: k.forward,
                    backward: k.backward,
                    tbc: k.tbc,
                })
                .collect();

            (
                sym,
                TransformChannel {
                    translation_keys,
                    translation_type: convert_key_type(ch.translation_type),
                    rotation_keys,
                    rotation_type: convert_key_type(ch.rotation_type),
                    scale_keys,
                    scale_type: convert_key_type(ch.scale_type),
                    priority: ch.priority,
                },
            )
        })
        .collect();

    let convert_float_target = |t: na::FloatTarget| match t {
        na::FloatTarget::Alpha => FloatTarget::Alpha,
        na::FloatTarget::UvOffsetU => FloatTarget::UvOffsetU,
        na::FloatTarget::UvOffsetV => FloatTarget::UvOffsetV,
        na::FloatTarget::UvScaleU => FloatTarget::UvScaleU,
        na::FloatTarget::UvScaleV => FloatTarget::UvScaleV,
        na::FloatTarget::UvRotation => FloatTarget::UvRotation,
        na::FloatTarget::ShaderFloat => FloatTarget::ShaderFloat,
        na::FloatTarget::MorphWeight(idx) => FloatTarget::MorphWeight(idx),
        na::FloatTarget::LightDimmer => FloatTarget::LightDimmer,
        na::FloatTarget::LightIntensity => FloatTarget::LightIntensity,
        na::FloatTarget::LightRadius => FloatTarget::LightRadius,
    };

    let convert_color_target = |t: na::ColorTarget| match t {
        na::ColorTarget::Diffuse => ColorTarget::Diffuse,
        na::ColorTarget::Ambient => ColorTarget::Ambient,
        na::ColorTarget::Specular => ColorTarget::Specular,
        na::ColorTarget::Emissive => ColorTarget::Emissive,
        na::ColorTarget::ShaderColor => ColorTarget::ShaderColor,
        na::ColorTarget::LightDiffuse => ColorTarget::LightDiffuse,
        na::ColorTarget::LightAmbient => ColorTarget::LightAmbient,
    };

    let float_channels = nif
        .float_channels
        .iter()
        .map(|(name, ch)| {
            (
                pool.intern(name),
                FloatChannel {
                    target: convert_float_target(ch.target),
                    keys: ch
                        .keys
                        .iter()
                        .map(|k| AnimFloatKey {
                            time: k.time,
                            value: k.value,
                        })
                        .collect(),
                },
            )
        })
        .collect();

    let color_channels = nif
        .color_channels
        .iter()
        .map(|(name, ch)| {
            (
                pool.intern(name),
                ColorChannel {
                    target: convert_color_target(ch.target),
                    keys: ch
                        .keys
                        .iter()
                        .map(|k| AnimColorKey {
                            time: k.time,
                            value: Vec3::from_array(k.value),
                        })
                        .collect(),
                },
            )
        })
        .collect();

    let bool_channels = nif
        .bool_channels
        .iter()
        .map(|(name, ch)| {
            (
                pool.intern(name),
                BoolChannel {
                    keys: ch
                        .keys
                        .iter()
                        .map(|k| AnimBoolKey {
                            time: k.time,
                            value: k.value,
                        })
                        .collect(),
                },
            )
        })
        .collect();

    let texture_flip_channels = nif
        .texture_flip_channels
        .iter()
        .map(|(name, ch)| {
            (
                pool.intern(name),
                TextureFlipChannel {
                    texture_slot: ch.texture_slot,
                    source_paths: ch.source_paths.clone(),
                    keys: ch
                        .keys
                        .iter()
                        .map(|k| AnimFloatKey {
                            time: k.time,
                            value: k.value,
                        })
                        .collect(),
                },
            )
        })
        .collect();

    // Intern text key labels (#231 / SI-04). Most labels are short
    // ("hit", "FootLeft", "sound: wpn_swing") and repeat heavily across
    // clips of the same actor, so dedup into the StringPool eliminates
    // both the per-clip label allocation here and the per-fire
    // `to_owned()` in the event-emission path.
    let text_keys = nif
        .text_keys
        .iter()
        .map(|(t, label)| (*t, pool.intern(label)))
        .collect();

    AnimationClip {
        name: nif.name.clone(),
        // #3432 — `NiControllerSequence.duration` is raw file data; see
        // `sanitized_clip_duration` for why it's resolved here.
        duration: sanitized_clip_duration(nif.duration),
        cycle_type,
        // #3258 — `NiControllerSequence.frequency` is raw file data and the
        // only unvalidated factor in `advance_time`'s `dt * speed *
        // frequency`. A NaN or ±inf here latches `local_time` to NaN on the
        // first tick of any looping clip and the NaN then flows through
        // `find_key_pair` into the bone transform, `GlobalTransform`, and the
        // GPU instance matrices. Resolve it once here, at the translate
        // boundary, the same way every other per-game quirk is; `1.0` is the
        // Gamebryo default playback rate. A non-positive rate is also
        // rejected: `advance_time` has no notion of a clip that plays
        // backwards by authored frequency (that is `CycleType::Reverse`), so
        // a negative one would only walk `local_time` backwards forever.
        frequency: sanitized_clip_frequency(nif.frequency),
        // #3345 — `NiControllerSequence.phase` reached the NIF-side clip under
        // #3097 but stopped here: this literal copied every sibling field and
        // not this one, and `byroredux_core`'s `AnimationClip` had nowhere to
        // put it. Sanitized on the same grounds as `frequency` directly above
        // — it is raw file data that seeds `AnimationPlayer::local_time`, so a
        // NaN/inf would latch the player's time on the first tick. Negative is
        // allowed (a clip legitimately starts before t=0 and the sampler's
        // cycle handling normalises it); only non-finite is rejected.
        phase: if nif.phase.is_finite() {
            nif.phase
        } else {
            0.0
        },
        // #3432 — `NiControllerSequence.weight` is raw file data; see
        // `sanitized_clip_weight` for why it's resolved here.
        weight: sanitized_clip_weight(nif.weight),
        accum_root_name: nif.accum_root_name.as_deref().map(|s| pool.intern(s)),
        channels,
        float_channels,
        color_channels,
        bool_channels,
        texture_flip_channels,
        text_keys,
    }
}

// ─────────────────────────────────────────────────────────────────────
// #2221 — sink attachment
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod sink_attachment_tests {
    //! Regression guards for [`attach_animation_sinks`].
    //!
    //! The bug these pin was invisible to the pre-existing routing
    //! tests because those tests inserted the sink components
    //! themselves before calling the apply helpers. Production never
    //! did. So every helper-level assertion passed while the shipped
    //! engine animated nothing but transforms.
    //!
    //! The end-to-end test below is therefore the important one: it
    //! runs attach → apply and asserts the value actually lands,
    //! without the test ever inserting a sink by hand.

    use super::*;
    use byroredux_core::animation::{AnimColorKey, AnimFloatKey};
    use byroredux_core::ecs::{
        AnimatedAlpha, AnimatedMorphWeights, AnimatedShaderFloat, AnimatedTextureFlip,
        AnimatedUvTransform, AnimatedVisibility, Children, LightSource, Name, TextureFlipEntry,
    };

    /// Build a two-entity subtree (`root` → `child`) with both named,
    /// mirroring the shape `build_subtree_name_map` walks in production.
    fn subtree(world: &mut World, pool: &mut StringPool) -> (EntityId, EntityId, FixedString) {
        let root = world.spawn();
        let child = world.spawn();
        let root_name = pool.intern("Root");
        let child_name = pool.intern("Fire01");
        world.insert(root, Name(root_name));
        world.insert(child, Name(child_name));
        world.insert(root, Children(vec![child]));
        (root, child, child_name)
    }

    fn float_channel(target: FloatTarget, value: f32) -> FloatChannel {
        FloatChannel {
            target,
            keys: vec![AnimFloatKey { time: 0.0, value }],
        }
    }

    #[test]
    fn attaches_only_the_sinks_the_clip_actually_targets() {
        let mut world = World::new();
        let mut pool = StringPool::new();
        let (root, child, child_name) = subtree(&mut world, &mut pool);

        // A UV-scroll-only clip — the classic animated-water case.
        let floats = vec![(child_name, float_channel(FloatTarget::UvOffsetU, 0.25))];
        attach_animation_sinks(&mut world, &[], &floats, &[], &[], None, None, root);

        let uv = world.query::<AnimatedUvTransform>().unwrap();
        assert!(
            uv.get(child).is_some(),
            "the UV channel's target must receive AnimatedUvTransform"
        );
        assert_eq!(uv.get(child).unwrap().offset.x, 0.25, "seeded from t=0");
        assert!(
            uv.get(root).is_none(),
            "sinks attach to the channel's target only, not the whole subtree"
        );
        drop(uv);

        // Sparse storages must stay sparse: no sink for a channel type
        // this clip never carried.
        assert!(
            world
                .query::<AnimatedAlpha>()
                .is_none_or(|q| q.get(child).is_none()),
            "a UV-only clip must not attach AnimatedAlpha"
        );
        assert!(
            world
                .query::<AnimatedMorphWeights>()
                .is_none_or(|q| q.get(child).is_none()),
            "a UV-only clip must not attach AnimatedMorphWeights"
        );
    }

    #[test]
    fn does_not_overwrite_an_existing_sink() {
        let mut world = World::new();
        let mut pool = StringPool::new();
        let (root, child, child_name) = subtree(&mut world, &mut pool);

        // A first clip already attached and the system has since ticked
        // it to a live value.
        world.insert(child, AnimatedAlpha(0.4));

        let floats = vec![(child_name, float_channel(FloatTarget::Alpha, 1.0))];
        attach_animation_sinks(&mut world, &[], &floats, &[], &[], None, None, root);

        let alpha = world.query::<AnimatedAlpha>().unwrap();
        assert_eq!(
            alpha.get(child).unwrap().0,
            0.4,
            "a second clip attaching to the same entity must not reset the live value"
        );
    }

    #[test]
    fn light_channels_never_synthesise_a_lightsource() {
        let mut world = World::new();
        let mut pool = StringPool::new();
        let (root, child, child_name) = subtree(&mut world, &mut pool);

        let floats = vec![
            (child_name, float_channel(FloatTarget::LightDimmer, 0.5)),
            (child_name, float_channel(FloatTarget::LightRadius, 512.0)),
        ];
        let colors = vec![(
            child_name,
            ColorChannel {
                target: ColorTarget::LightDiffuse,
                keys: vec![AnimColorKey {
                    time: 0.0,
                    value: Vec3::ONE,
                }],
            },
        )];
        attach_animation_sinks(&mut world, &[], &floats, &colors, &[], None, None, root);

        assert!(
            world
                .query::<LightSource>()
                .is_none_or(|q| q.get(child).is_none()),
            "light channels write into an existing LightSource; synthesising one \
             would spawn a phantom light wherever no LIGH record exists"
        );
    }

    #[test]
    fn unresolvable_channel_names_are_skipped() {
        let mut world = World::new();
        let mut pool = StringPool::new();
        let (root, _child, _) = subtree(&mut world, &mut pool);
        let orphan = pool.intern("NotInThisSubtree");

        let floats = vec![(orphan, float_channel(FloatTarget::Alpha, 1.0))];
        attach_animation_sinks(&mut world, &[], &floats, &[], &[], None, None, root);

        assert!(
            world.query::<AnimatedAlpha>().is_none_or(|q| q.is_empty()),
            "a channel whose name resolves to no entity in the subtree \
             must not attach a sink anywhere"
        );
    }

    #[test]
    fn morph_and_uv_channels_coalesce_into_one_component_each() {
        let mut world = World::new();
        let mut pool = StringPool::new();
        let (root, child, child_name) = subtree(&mut world, &mut pool);

        let floats = vec![
            (child_name, float_channel(FloatTarget::UvOffsetU, 0.1)),
            (child_name, float_channel(FloatTarget::UvScaleV, 2.0)),
            (child_name, float_channel(FloatTarget::MorphWeight(0), 0.3)),
            (child_name, float_channel(FloatTarget::MorphWeight(2), 0.9)),
        ];
        attach_animation_sinks(&mut world, &[], &floats, &[], &[], None, None, root);

        let uv = world.query::<AnimatedUvTransform>().unwrap();
        let t = uv.get(child).unwrap();
        assert_eq!(t.offset.x, 0.1, "five UV targets share one component");
        assert_eq!(t.scale.y, 2.0);
        assert_eq!(t.scale.x, 1.0, "untouched UV slots keep identity");
        drop(uv);

        let morph = world.query::<AnimatedMorphWeights>().unwrap();
        let m = morph.get(child).unwrap();
        assert_eq!(m.get(0), 0.3);
        assert_eq!(m.get(1), 0.0, "gap between morph indices zero-pads");
        assert_eq!(m.get(2), 0.9);
    }

    #[test]
    fn second_clip_targeting_a_new_texture_flip_slot_is_silently_dropped() {
        // Documents a known, pre-existing limitation of `insert_missing_sinks`
        // (#3243): `AnimatedTextureFlip` is a `Vec<TextureFlipEntry>` designed
        // to hold more than one flipbook slot on an entity, but the merge only
        // happens *within* a single `attach_animation_sinks` call. A second,
        // later-attached clip that introduces a *new* texture_slot for an
        // entity that already carries an `AnimatedTextureFlip` (from a first
        // clip) has its slot dropped entirely, because `insert_missing_sinks`
        // only checks "does this entity have *a* component of this type
        // already" — it doesn't know the type is a mergeable collection.
        // Same trade-off already accepted for `AnimatedMorphWeights`; this
        // test exists so the boundary is verified rather than assumed.
        let mut world = World::new();
        let entity = world.spawn();
        let first_clip_entry = AnimatedTextureFlip(vec![TextureFlipEntry {
            texture_slot: 0,
            handles: vec![1, 2],
            current_index: 0,
        }]);
        insert_missing_sinks(&mut world, vec![(entity, first_clip_entry)]);

        // A second clip targeting a *different* slot on the same entity.
        let second_clip_entry = AnimatedTextureFlip(vec![TextureFlipEntry {
            texture_slot: 1,
            handles: vec![3, 4],
            current_index: 0,
        }]);
        insert_missing_sinks(&mut world, vec![(entity, second_clip_entry)]);

        let flip = world.query::<AnimatedTextureFlip>().unwrap();
        let entries = &flip.get(entity).unwrap().0;
        assert_eq!(
            entries.len(),
            1,
            "current behavior: the second clip's slot-1 entry is dropped, \
             not merged, because `insert_missing_sinks` sees the entity \
             already has an AnimatedTextureFlip from the first clip"
        );
        assert_eq!(
            entries[0].texture_slot, 0,
            "only the first clip's slot survives"
        );
    }

    #[test]
    fn pure_transform_clip_attaches_nothing() {
        let mut world = World::new();
        let mut pool = StringPool::new();
        let (root, child, _) = subtree(&mut world, &mut pool);

        attach_animation_sinks(&mut world, &[], &[], &[], &[], None, None, root);

        assert!(world
            .query::<AnimatedShaderFloat>()
            .is_none_or(|q| q.get(child).is_none()));
        assert!(world
            .query::<AnimatedVisibility>()
            .is_none_or(|q| q.get(child).is_none()));
    }
}

#[cfg(test)]
mod clip_frequency_tests {
    //! #3258 — the NIF-side half of the animation-clock NaN guard.
    use super::sanitized_clip_frequency;

    #[test]
    fn authored_rates_pass_through() {
        assert_eq!(sanitized_clip_frequency(1.0), 1.0);
        assert_eq!(sanitized_clip_frequency(0.5), 0.5);
        assert_eq!(sanitized_clip_frequency(2.5), 2.5);
    }

    #[test]
    fn non_integrable_rates_fall_back_to_the_gamebryo_default() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, -0.0, -1.0] {
            assert_eq!(
                sanitized_clip_frequency(bad),
                1.0,
                "frequency {bad} must not reach the animation clock"
            );
        }
    }
}

#[cfg(test)]
mod clip_phase_tests {
    //! #3345 — the NIF→core boundary must carry `phase` across.
    use super::convert_nif_clip;
    use byroredux_core::string::StringPool;
    use byroredux_nif::anim as na;
    use std::collections::HashMap;

    fn nif_clip(phase: f32) -> na::AnimationClip {
        na::AnimationClip {
            name: "phased".to_string(),
            duration: 2.0,
            cycle_type: na::CycleType::Loop,
            frequency: 1.0,
            phase,
            weight: 1.0,
            accum_root_name: None,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: Vec::new(),
        }
    }

    /// #3097 carried `phase` onto the NIF-side clip; this literal then copied
    /// every sibling field except that one, so it died at the boundary.
    #[test]
    fn phase_survives_the_nif_to_core_conversion() {
        let mut pool = StringPool::new();
        let clip = convert_nif_clip(&nif_clip(0.4), &mut pool);
        assert_eq!(
            clip.phase, 0.4,
            "NiControllerSequence.phase must reach byroredux_core's AnimationClip"
        );
    }

    /// `phase` seeds `AnimationPlayer::local_time`, so a non-finite value
    /// would latch the animation clock to NaN on the first tick — the same
    /// failure mode `sanitized_clip_frequency` guards (#3258). Negative is
    /// allowed: the sampler's cycle handling normalises it.
    #[test]
    fn non_finite_phase_is_rejected_negative_is_kept() {
        let mut pool = StringPool::new();
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let clip = convert_nif_clip(&nif_clip(bad), &mut pool);
            assert_eq!(clip.phase, 0.0, "phase {bad} must not reach the clock");
        }
        let clip = convert_nif_clip(&nif_clip(-0.25), &mut pool);
        assert_eq!(clip.phase, -0.25);
    }
}

#[cfg(test)]
mod clip_duration_weight_tests {
    //! #3432 — sibling of `clip_frequency_tests` / `clip_phase_tests`:
    //! `NiControllerSequence.duration` and `.weight` are the two fields of
    //! the same struct #3258 left unsanitized. A NaN `duration` latches
    //! `CycleType::Reverse`'s `fold_reverse_time` (`NaN <= 0.0` is false);
    //! a NaN `weight` is never filtered by `sample_blended_transform`'s
    //! `ew < 0.001` skip (`NaN < 0.001` is also false).
    use super::{convert_nif_clip, sanitized_clip_duration, sanitized_clip_weight};
    use byroredux_core::string::StringPool;
    use byroredux_nif::anim as na;
    use std::collections::HashMap;

    fn nif_clip(duration: f32, weight: f32) -> na::AnimationClip {
        na::AnimationClip {
            name: "unsanitized".to_string(),
            duration,
            cycle_type: na::CycleType::Reverse,
            frequency: 1.0,
            phase: 0.0,
            weight,
            accum_root_name: None,
            channels: HashMap::new(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: Vec::new(),
        }
    }

    #[test]
    fn authored_duration_and_weight_pass_through() {
        assert_eq!(sanitized_clip_duration(2.5), 2.5);
        assert_eq!(sanitized_clip_weight(0.5), 0.5);
        // Weight has no #1544-style positivity requirement — a clip
        // legitimately authors a negative or zero blend weight.
        assert_eq!(sanitized_clip_weight(-0.5), -0.5);
        assert_eq!(sanitized_clip_weight(0.0), 0.0);
    }

    #[test]
    fn non_finite_or_non_positive_duration_collapses_to_zero() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, -1.0] {
            assert_eq!(
                sanitized_clip_duration(bad),
                0.0,
                "duration {bad} must not reach fold_reverse_time / the Loop modulo"
            );
        }
    }

    #[test]
    fn non_finite_weight_falls_back_to_the_nifxml_default() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                sanitized_clip_weight(bad),
                1.0,
                "weight {bad} must not reach sample_blended_transform's total_weight"
            );
        }
    }

    /// The NIF→core boundary must apply both sanitizers, not just carry the
    /// raw fields across — the same shape #3258 fixed for `frequency`.
    #[test]
    fn duration_and_weight_are_sanitized_at_the_nif_to_core_boundary() {
        let mut pool = StringPool::new();
        let clip = convert_nif_clip(&nif_clip(f32::NAN, f32::NAN), &mut pool);
        assert_eq!(
            clip.duration, 0.0,
            "NaN duration must not reach byroredux_core::AnimationClip"
        );
        assert_eq!(
            clip.weight, 1.0,
            "NaN weight must not reach byroredux_core::AnimationClip"
        );

        let clip = convert_nif_clip(&nif_clip(3.0, 0.75), &mut pool);
        assert_eq!(clip.duration, 3.0, "finite duration must pass through");
        assert_eq!(clip.weight, 0.75, "finite weight must pass through");
    }
}
