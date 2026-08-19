//! Skyrim IDLE record → HKX clip installation.

use super::TextureProvider;
use crate::components::HavokIdleCatalog;
use byroredux_core::animation::{
    AnimationClip, AnimationClipRegistry, CycleType, KeyType, RotationKey, ScaleKey,
    TransformChannel, TranslationKey,
};
use byroredux_core::ecs::World;
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::StringPool;
use byroredux_hkx::{HkxAnimation, HkxSkeleton};
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::EsmIndex;
use std::collections::{HashMap, HashSet};

const SKELETON_PATH: &str = r"meshes\actors\character\character assets\skeleton.hkx";
const ANIMATION_ROOT: &str = r"meshes\actors\character\animations";

/// Decode and register the Skyrim cart IDLE family used by MQ101 startup.
///
/// This intentionally stops at that live startup surface instead of eagerly
/// expanding all ~1,400 Skyrim IDLEs into frame keys. Repeated cell/runtime
/// installation is idempotent through `HavokIdleCatalog`.
pub(crate) fn populate_havok_idle_runtime(
    world: &mut World,
    index: &EsmIndex,
    provider: &TextureProvider,
) -> usize {
    if index.game != GameKind::Skyrim {
        return 0;
    }
    if world.try_resource::<HavokIdleCatalog>().is_none() {
        world.insert_resource(HavokIdleCatalog::default());
    }
    let already_loaded: HashSet<u32> = world
        .resource::<HavokIdleCatalog>()
        .handles
        .keys()
        .copied()
        .collect();
    let idles: Vec<_> = index
        .idle_animations
        .values()
        .filter(|idle| {
            !already_loaded.contains(&idle.form_id)
                && idle
                    .animation_event
                    .get(..8)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("idlecart"))
        })
        .collect();
    if idles.is_empty() {
        return 0;
    }

    let Some(skeleton_bytes) = provider.extract_mesh(SKELETON_PATH) else {
        log::warn!(
            "Skyrim cart animation: '{SKELETON_PATH}' is missing; add Skyrim - Animations.bsa"
        );
        return 0;
    };
    let skeleton = match byroredux_hkx::decode_skeleton(&skeleton_bytes) {
        Ok(skeleton) => skeleton,
        Err(error) => {
            log::warn!("Skyrim cart animation: failed to decode skeleton HKX: {error}");
            return 0;
        }
    };

    let mut installed = 0;
    for idle in idles {
        let mut resolved_handle = None;
        let mut resolved_clip = None;
        for path in idle_animation_candidates(&idle.animation_event) {
            if let Some(handle) = world.resource::<AnimationClipRegistry>().get_by_path(&path) {
                resolved_handle = Some(handle);
                break;
            }
            let Some(bytes) = provider.extract_mesh(&path) else {
                continue;
            };
            match byroredux_hkx::decode_spline_animation(&bytes) {
                Ok(animation) => {
                    resolved_clip = Some((path, animation));
                    break;
                }
                Err(error) => {
                    log::debug!(
                        "Skyrim IDLE {:08X} ({}): HKX '{}' is not usable: {error}",
                        idle.form_id,
                        idle.editor_id,
                        path,
                    );
                }
            }
        }
        if resolved_handle.is_none() && resolved_clip.is_none() {
            log::debug!(
                "Skyrim IDLE {:08X} ({}): no character animation HKX for event '{}'",
                idle.form_id,
                idle.editor_id,
                idle.animation_event,
            );
            continue;
        }

        let handle = if let Some(handle) = resolved_handle {
            handle
        } else {
            let (path, animation) = resolved_clip.expect("resolved clip checked above");
            let clip = {
                let mut pool = world.resource_mut::<StringPool>();
                convert_hkx_clip(
                    &path,
                    &idle.animation_event,
                    &skeleton,
                    &animation,
                    &mut pool,
                )
            };
            world
                .resource_mut::<AnimationClipRegistry>()
                .get_or_insert_by_path(path, || clip)
        };
        world
            .resource_mut::<HavokIdleCatalog>()
            .handles
            .insert(idle.form_id, handle);
        installed += 1;
    }
    if installed > 0 {
        log::info!("Installed {installed} Skyrim cart IDLE HKX clips");
    }
    installed
}

fn idle_animation_candidates(event: &str) -> Vec<String> {
    let event = event.trim_matches('\0').trim();
    let stem = event
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("idle"))
        .map_or(event, |_| &event[4..])
        .to_ascii_lowercase();
    if stem.is_empty() {
        return Vec::new();
    }

    let mut stems = vec![stem.clone()];
    // `IdleCartDriverSway` is authored as `CartDriverIdleSway.hkx` while
    // prisoner sway events map directly. Keep both deterministic candidates.
    if let Some(prefix) = stem.strip_suffix("sway") {
        stems.push(format!("{prefix}idlesway"));
    }
    if !stem.ends_with("idle") {
        stems.push(format!("{stem}idle"));
    }
    stems.dedup();
    stems
        .into_iter()
        .map(|stem| format!(r"{ANIMATION_ROOT}\{stem}.hkx"))
        .collect()
}

fn convert_hkx_clip(
    path: &str,
    idle_event: &str,
    skeleton: &HkxSkeleton,
    animation: &HkxAnimation,
    pool: &mut StringPool,
) -> AnimationClip {
    let mut channels = HashMap::with_capacity(animation.tracks.len());
    for (track_index, samples) in animation.tracks.iter().enumerate() {
        let Some(&bone_index) = animation.track_to_bone.get(track_index) else {
            // `decode_spline_animation` always produces `track_to_bone.len()
            // == tracks.len()`; this is defensive, not a reachable vanilla
            // path. Logged at debug rather than warn for that reason.
            log::debug!(
                "HKX '{path}' (event '{idle_event}'): track {track_index} has no \
                 track_to_bone entry — dropped",
            );
            continue;
        };
        // #3013 — an out-of-range `track_to_bone` entry (a bone index the
        // decoded skeleton doesn't have) used to `continue` silently here,
        // dropping the whole track with no diagnostic. This is where
        // animation and skeleton are actually bound together — the `hkx`
        // crate decodes each packfile independently and never sees the
        // other, so it has no skeleton to validate against; here both are
        // in hand, so a mismatch is loud instead of a bone that quietly
        // never animates.
        let Some(bone) = skeleton.bones.get(bone_index as usize) else {
            log::warn!(
                "HKX '{path}' (event '{idle_event}'): track {track_index} binds bone \
                 index {bone_index}, but skeleton '{}' only has {} bones — track dropped",
                skeleton.name,
                skeleton.bones.len(),
            );
            continue;
        };
        let mut translation_keys = Vec::with_capacity(samples.len());
        let mut rotation_keys = Vec::with_capacity(samples.len());
        let mut scale_keys = Vec::with_capacity(samples.len());
        for (frame, sample) in samples.iter().enumerate() {
            let time = (frame as f32 * animation.frame_duration).min(animation.duration);
            let translation = byroredux_core::math::coord::zup_to_yup_pos(sample.translation);
            let rotation = byroredux_core::math::coord::zup_to_yup_quat_wxyz([
                sample.rotation[3],
                sample.rotation[0],
                sample.rotation[1],
                sample.rotation[2],
            ]);
            let scale = (sample.scale[0] + sample.scale[1] + sample.scale[2]) / 3.0;
            translation_keys.push(TranslationKey {
                time,
                value: Vec3::from_array(translation),
                forward: Vec3::ZERO,
                backward: Vec3::ZERO,
                tbc: None,
            });
            rotation_keys.push(RotationKey {
                time,
                value: Quat::from_array(rotation),
                tbc: None,
            });
            scale_keys.push(ScaleKey {
                time,
                value: scale,
                forward: 0.0,
                backward: 0.0,
                tbc: None,
            });
        }
        channels.insert(
            pool.intern(&bone.name),
            TransformChannel {
                translation_keys,
                translation_type: KeyType::Linear,
                rotation_keys,
                rotation_type: KeyType::Linear,
                scale_keys,
                scale_type: KeyType::Linear,
                priority: 0,
            },
        );
    }

    let completion_events = behavior_completion_events(idle_event);
    let accum_root_name = if completion_events.is_empty() {
        None
    } else {
        skeleton
            .bones
            .iter()
            .find(|bone| bone.name.eq_ignore_ascii_case("NPC COM [COM ]"))
            .map(|bone| pool.intern(&bone.name))
    };
    let mut authored_labels: Vec<_> = animation
        .annotations
        .iter()
        .map(|annotation| annotation.text.as_str())
        .collect();
    let mut text_keys: Vec<_> = animation
        .annotations
        .iter()
        .map(|annotation| (annotation.time, pool.intern(&annotation.text)))
        .collect();
    for event in completion_events {
        if !authored_labels
            .iter()
            .any(|label| label.eq_ignore_ascii_case(event))
        {
            text_keys.push((animation.duration, pool.intern(event)));
            authored_labels.push(event);
        }
    }
    text_keys.sort_by(|left, right| left.0.total_cmp(&right.0));

    AnimationClip {
        name: path.to_owned(),
        duration: animation.duration,
        cycle_type: if behavior_completion_events(idle_event).is_empty() {
            CycleType::Loop
        } else {
            CycleType::Clamp
        },
        frequency: 1.0,
        weight: 1.0,
        accum_root_name,
        channels,
        float_channels: Vec::new(),
        color_channels: Vec::new(),
        bool_channels: Vec::new(),
        texture_flip_channels: Vec::new(),
        text_keys,
    }
}

/// Skyrim's cart completion notifications are behavior-graph events rather
/// than annotations embedded in the individual spline clip. Preserve that
/// authored boundary while the runtime deliberately avoids executing the
/// full Havok behavior VM.
fn behavior_completion_events(idle_event: &str) -> &'static [&'static str] {
    let event = idle_event.trim_matches('\0').trim();
    if event
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("idlecart"))
        && event
            .get(event.len().saturating_sub(4)..)
            .is_some_and(|suffix| suffix.eq_ignore_ascii_case("exit"))
    {
        &["ExitCartEnd", "IdleFurnitureExit"]
    } else {
        &[]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset_provider::build_texture_provider;

    #[test]
    fn idle_event_candidates_cover_direct_and_driver_aliases() {
        assert_eq!(
            idle_animation_candidates("IdleCartPrisonerASway")[0],
            r"meshes\actors\character\animations\cartprisonerasway.hkx"
        );
        assert!(idle_animation_candidates("IdleCartDriverSway")
            .contains(&r"meshes\actors\character\animations\cartdriveridlesway.hkx".to_owned()));
        assert_eq!(
            idle_animation_candidates("IdleCartPrisonerCIdle")[0],
            r"meshes\actors\character\animations\cartprisonercidle.hkx"
        );
        assert_eq!(
            idle_animation_candidates("IdleCartPrisonerAExit")[0],
            r"meshes\actors\character\animations\cartprisoneraexit.hkx"
        );
        assert_eq!(
            behavior_completion_events("IdleCartPrisonerAExit"),
            &["ExitCartEnd", "IdleFurnitureExit"]
        );
        assert!(behavior_completion_events("IdleCartPrisonerASway").is_empty());
    }

    /// #3013 — a `track_to_bone` entry naming a bone index the decoded
    /// skeleton doesn't have must drop only that track, not panic and not
    /// silently disappear (the drop itself is still asserted here; the
    /// accompanying `log::warn!` is a diagnostic, not a return value, so
    /// it isn't captured by this test). A sibling in-range track on the
    /// same clip must still convert normally, proving the out-of-range
    /// entry doesn't poison the whole clip.
    #[test]
    fn convert_hkx_clip_drops_only_the_out_of_range_bound_track() {
        use byroredux_hkx::{HkxAnnotation, HkxBone, HkxTransform};

        let skeleton = HkxSkeleton {
            name: "TestSkeleton".to_string(),
            bones: vec![HkxBone {
                name: "NPC Root [Root]".to_string(),
                parent_index: -1,
                reference_pose: HkxTransform::IDENTITY,
            }],
        };
        let sample = vec![HkxTransform::IDENTITY];
        let animation = HkxAnimation {
            duration: 1.0,
            num_frames: 1,
            frame_duration: 1.0,
            // Track 0 binds bone 0 (valid); track 1 binds bone 5, but the
            // skeleton above has only one bone (index 0) — out of range.
            tracks: vec![sample.clone(), sample],
            track_to_bone: vec![0, 5],
            annotations: Vec::<HkxAnnotation>::new(),
        };

        let mut pool = StringPool::new();
        let clip = convert_hkx_clip("test.hkx", "IdleTest", &skeleton, &animation, &mut pool);

        assert_eq!(
            clip.channels.len(),
            1,
            "only the in-range track (bone 0) should convert; the \
             out-of-range track (bone 5) must be dropped, not panic and \
             not silently produce a channel for a bone that doesn't exist",
        );
    }

    #[test]
    fn skyrim_cart_idle_catalog_installs_real_assets_when_available() {
        let data_dir = std::env::var_os("BYROREDUX_SKYRIM_DATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(
                    "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data",
                )
            });
        let esm_path = data_dir.join("Skyrim.esm");
        let animations_path = data_dir.join("Skyrim - Animations.bsa");
        if !esm_path.is_file() || !animations_path.is_file() {
            return;
        }

        let index = byroredux_plugin::esm::parse_esm(&std::fs::read(esm_path).unwrap()).unwrap();
        let provider = build_texture_provider(&[
            "--bsa".to_owned(),
            animations_path.to_string_lossy().into_owned(),
        ]);
        let mut world = World::new();
        world.insert_resource(StringPool::new());
        world.insert_resource(AnimationClipRegistry::new());
        world.insert_resource(HavokIdleCatalog::default());

        let installed = populate_havok_idle_runtime(&mut world, &index, &provider);
        assert!(
            installed >= 8,
            "only {installed} Skyrim cart IDLEs installed"
        );
        let catalog = world.resource::<HavokIdleCatalog>();
        for form_id in [
            0x000B_8C4F,
            0x000B_8C50,
            0x000B_8C51,
            0x000B_8C52,
            0x0010_6AE3,
            0x0010_6AE4,
            0x0010_6AE5,
            0x0010_6AE6,
        ] {
            assert!(
                catalog.handles.contains_key(&form_id),
                "IDLE {form_id:08X} did not resolve to a decoded HKX"
            );
        }

        let registry = world.resource::<AnimationClipRegistry>();
        let pool = world.resource::<StringPool>();
        for event_name in [
            "IdleCartDriverExit",
            "IdleCartPrisonerAExit",
            "IdleCartPrisonerBExit",
            "IdleCartPrisonerCExit",
            "IdleCartPrisonerDExit",
        ] {
            let idle = index
                .idle_animations
                .values()
                .find(|idle| idle.animation_event.eq_ignore_ascii_case(event_name))
                .unwrap_or_else(|| panic!("Skyrim.esm is missing IDLE event {event_name}"));
            let handle = catalog
                .handles
                .get(&idle.form_id)
                .unwrap_or_else(|| panic!("IDLE {} did not install", idle.editor_id));
            let clip = registry.get(*handle).unwrap();
            assert_eq!(clip.cycle_type, CycleType::Clamp, "{event_name}");
            assert_eq!(
                clip.accum_root_name.and_then(|name| pool.resolve(name)),
                Some("npc com [com ]"),
                "{event_name} did not bind Skyrim's COM trajectory"
            );
            for completion in ["ExitCartEnd", "IdleFurnitureExit"] {
                let &(time, _) = clip
                    .text_keys
                    .iter()
                    .find(|(_, label)| {
                        pool.resolve(*label)
                            .is_some_and(|label| label.eq_ignore_ascii_case(completion))
                    })
                    .unwrap_or_else(|| panic!("{event_name} is missing {completion}"));
                assert!((time - clip.duration).abs() < 1e-6, "{event_name}");
            }
        }
    }
}
