//! Fallout 4 actors' idle pose by animation archetype (M47.2).
//!
//! Actors' behavior graphs are not executed here, but a graph's *start* is data:
//! the humanoid `MTBehavior.hkx` enters `MTIdles`, refers to `DialogueBehavior.hkx`
//! and starts in `PoseA_Idle1` ([`byroredux_hkx::BehaviorGraph::start_clip`]). Which
//! file that name means is the actor's subgraph entry's job (CK *Subgraphs*: the
//! RACE's "Subgraph Data" entry that matches the most of the actor's keywords names
//! the behavior graph and an ordered list of animation folders): the clip is the
//! first folder holding it, so a nervous woman's `Animations\PoseA_Idle1.hkt`
//! resolves to `...\MT\Nervous\Female\PoseA_Idle1.hkx` and a neutral man's to
//! `...\MT\Neutral\PoseA_Idle1.hkx`. `Actor.ChangeAnimArchetype` changes which entry
//! an actor matches, so it changes which file its idle is.
//!
//! [`populate_archetype_idles`] resolves every third-person `MT` entry of the load
//! order once, registers the clips, and records them in [`ArchetypeIdleClips`] for
//! `systems::archetype_idle_system`.

use super::animation::convert_hkx_clip;
use super::TextureProvider;
use byroredux_core::animation::AnimationClipRegistry;
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::World;
use byroredux_core::string::StringPool;
use byroredux_hkx::{BehaviorGraph, HkxSkeleton};
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::{EsmIndex, SubgraphData, SubgraphRole};
use std::collections::HashMap;

/// A subgraph entry's identity for [`ArchetypeIdleClips`]: its behavior graph and
/// animation folders, lower-cased.
pub(crate) fn idle_key(entry: &SubgraphData) -> String {
    let mut key = entry.behavior_graph.to_ascii_lowercase();
    for folder in &entry.animation_paths {
        key.push('|');
        key.push_str(&folder.to_ascii_lowercase());
    }
    key
}

/// The idle clip each `MT` subgraph entry resolves to (`None`: the entry's graph
/// did not decode, or its clip is in none of its folders). `NOT_SAVED_BY_DESIGN`:
/// rebuilt from the archives.
#[derive(Default)]
pub(crate) struct ArchetypeIdleClips {
    handles: HashMap<String, Option<u32>>,
}

impl Resource for ArchetypeIdleClips {}

impl ArchetypeIdleClips {
    #[cfg(test)]
    pub(crate) fn for_test(clips: &[(&SubgraphData, u32)]) -> Self {
        Self { handles: clips.iter().map(|(entry, clip)| (idle_key(entry), Some(*clip))).collect() }
    }

    /// The registered idle clip for `entry`.
    pub(crate) fn clip_for(&self, entry: &SubgraphData) -> Option<u32> {
        self.handles.get(&idle_key(entry)).copied().flatten()
    }
}

/// `meshes\<relative>`, lower-cased.
fn mesh_path(relative: &str) -> String {
    let relative = relative.trim_start_matches(['\\', '/']).replace('/', "\\").to_ascii_lowercase();
    if relative.starts_with("meshes\\") {
        relative
    } else {
        format!("meshes\\{relative}")
    }
}

/// The directory a behavior path's project lives in: the part before its
/// `Behaviors\` folder (`actors\character\behaviors\mtbehavior.hkx` →
/// `actors\character`). References and `CharacterAssets\` are relative to it.
fn project_directory(behavior_graph: &str) -> Option<String> {
    let lower = behavior_graph.trim_start_matches(['\\', '/']).replace('/', "\\").to_ascii_lowercase();
    let lower = lower.strip_prefix("meshes\\").unwrap_or(&lower).to_owned();
    let at = lower.rfind("\\behaviors\\")?;
    Some(lower[..at].to_owned())
}

/// The file stem of an animation name a graph's clip carries
/// (`Animations\PoseA_Idle1.hkt` → `posea_idle1`).
fn clip_stem(animation: &str) -> Option<String> {
    let name = animation.rsplit(['\\', '/']).next()?;
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    (!stem.is_empty()).then(|| stem.to_ascii_lowercase())
}

/// Resolve and register the idle clip of every third-person `MT` subgraph entry.
/// Fallout 4 only; idempotent through the clip registry. Returns how many entries
/// resolved a clip.
pub(crate) fn populate_archetype_idles(world: &mut World, index: &EsmIndex, provider: &TextureProvider) -> usize {
    if index.game != GameKind::Fallout4 {
        return 0;
    }
    let entries: Vec<SubgraphData> = match world.try_resource::<byroredux_scripting::anim_subgraph::SubgraphTable>() {
        Some(table) => table.entries_for(SubgraphRole::Mt).cloned().collect(),
        None => return 0,
    };
    if entries.is_empty() {
        return 0;
    }
    world.init_resource::<ArchetypeIdleClips>();
    // The idle clip *name* of each behavior graph, and the rig of each project.
    let mut idle_names: HashMap<String, Option<String>> = HashMap::new();
    let mut skeletons: HashMap<String, Option<HkxSkeleton>> = HashMap::new();
    let mut resolved: HashMap<String, Option<String>> = HashMap::new();
    let mut handles: HashMap<String, Option<u32>> = HashMap::new();
    for entry in &entries {
        let key = idle_key(entry);
        if handles.contains_key(&key) {
            continue;
        }
        let handle = (|| {
            let directory = project_directory(&entry.behavior_graph)?;
            let name = idle_names
                .entry(entry.behavior_graph.to_ascii_lowercase())
                .or_insert_with(|| {
                    let graph = load_graph(provider, &mesh_path(&entry.behavior_graph))?;
                    let mut loader = |reference: &str| {
                        let path = mesh_path(&format!("{directory}\\{}", reference.trim_start_matches(['\\', '/'])));
                        load_graph(provider, &path)
                    };
                    graph.start_clip(&mut loader).map(|clip| clip.animation)
                })
                .clone()?;
            let stem = clip_stem(&name)?;
            let path = resolved
                .entry(format!("{}|{stem}", key))
                .or_insert_with(|| {
                    entry
                        .animation_paths
                        .iter()
                        .map(|folder| mesh_path(&format!("{folder}\\{stem}.hkx")))
                        .find(|path| provider.has_mesh(path))
                })
                .clone()?;
            log::debug!("archetype idle: {:?} -> '{path}'", entry.animation_paths.first());
            if let Some(handle) = world.resource::<AnimationClipRegistry>().get_by_path(&path) {
                return Some(handle);
            }
            let skeleton = skeletons
                .entry(directory.clone())
                .or_insert_with(|| {
                    let bytes = provider.extract_mesh(&mesh_path(&format!("{directory}\\characterassets\\skeleton.hkx")))?;
                    byroredux_hkx::decode_skeleton(&bytes)
                        .map_err(|error| log::debug!("archetype idle: skeleton of '{directory}': {error}"))
                        .ok()
                })
                .as_ref()?;
            let animation = byroredux_hkx::decode_spline_animation(&provider.extract_mesh(&path)?)
                .map_err(|error| log::debug!("archetype idle: '{path}': {error}"))
                .ok()?;
            let clip = {
                let mut pool = world.resource_mut::<StringPool>();
                convert_hkx_clip(&path, "idle", skeleton, &animation, &mut pool, false, None)
            };
            Some(world.resource_mut::<AnimationClipRegistry>().get_or_insert_by_path(path, || clip))
        })();
        handles.insert(key, handle);
    }
    let count = handles.values().flatten().count();
    world.resource_mut::<ArchetypeIdleClips>().handles.extend(handles);
    if count > 0 {
        log::info!("Fallout 4 archetype idles: {count} of {} MT subgraph entries resolved an idle clip", entries.len());
    }
    count
}

fn load_graph(provider: &TextureProvider, path: &str) -> Option<BehaviorGraph> {
    let bytes = provider.extract_mesh(path)?;
    byroredux_hkx::decode_behavior_graph(&bytes)
        .map_err(|error| log::debug!("archetype idle: graph '{path}': {error}"))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_directory_is_the_part_before_the_behaviors_folder() {
        assert_eq!(
            project_directory(r"Actors\Character\Behaviors\MTBehavior.hkx").as_deref(),
            Some(r"actors\character")
        );
        assert_eq!(
            project_directory(r"meshes\actors\dogmeat\behaviors\dogmeatdefault.hkx").as_deref(),
            Some(r"actors\dogmeat")
        );
        assert_eq!(project_directory(r"Actors\Character\MTBehavior.hkx"), None);
    }

    #[test]
    fn clip_stem_drops_the_folder_and_extension() {
        assert_eq!(clip_stem(r"Animations\PoseA_Idle1.hkt").as_deref(), Some("posea_idle1"));
        assert_eq!(clip_stem("Idle").as_deref(), Some("idle"));
        assert_eq!(clip_stem(r"Animations\").as_deref(), None);
    }

    #[test]
    fn the_key_names_the_graph_and_every_folder_in_order() {
        let entry = SubgraphData {
            actor_keywords: vec![1],
            behavior_graph: r"Actors\Character\Behaviors\MTBehavior.hkx".into(),
            animation_paths: vec![r"A\Nervous".into(), r"A\Neutral".into()],
            target_keywords: Vec::new(),
            role: SubgraphRole::Mt,
            first_person: false,
        };
        let swapped = SubgraphData { animation_paths: vec![r"A\Neutral".into(), r"A\Nervous".into()], ..entry.clone() };
        assert_ne!(idle_key(&entry), idle_key(&swapped), "the folder order is the search order");
        assert_eq!(idle_key(&entry), idle_key(&entry.clone()));
    }
}
