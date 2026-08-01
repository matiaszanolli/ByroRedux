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
                convert_hkx_clip(&path, &skeleton, &animation, &mut pool)
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
    skeleton: &HkxSkeleton,
    animation: &HkxAnimation,
    pool: &mut StringPool,
) -> AnimationClip {
    let mut channels = HashMap::with_capacity(animation.tracks.len());
    for (track_index, samples) in animation.tracks.iter().enumerate() {
        let Some(&bone_index) = animation.track_to_bone.get(track_index) else {
            continue;
        };
        let Some(bone) = skeleton.bones.get(bone_index as usize) else {
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

    AnimationClip {
        name: path.to_owned(),
        duration: animation.duration,
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
    }
}
