//! Fallout 4 actors' running behavior graphs (M47.2): `PlayAnimation` on an actor.
//!
//! An actor's subgraph entry (CK *Subgraphs*) names a behavior graph (the humanoid
//! `MTBehavior.hkx`) and the ordered animation folders its clips are looked up in. The
//! graph is decoded, its sub-behaviors (`DialogueBehavior.hkx`, ...) are inlined as
//! machines of the states that refer to them (`BehaviorGraph::inline_references`), and
//! an [`ObjectAnimationGraph`] runs it on the actor: an event sent to the actor moves
//! the machines and the state's clip plays on the actor's skeleton, exactly as on a
//! placed prop. The graph names hundreds of clips and which file each is depends on the
//! actor's folders, so clips are resolved on demand ([`LazyClips`]): a state whose clip
//! is not loaded yet asks for it, and the next [`populate_actor_graphs`] pass decodes
//! it and restarts the graphs that were waiting.
//!
//! Evaluated exactly as for props: blend weights and variables, transition
//! blends and conditions, and the graph-level modifiers (the pose
//! modifiers stay parked — `animation_modifiers.rs`'s list).

use super::animation::convert_hkx_clip;
use super::TextureProvider;
use byroredux_core::animation::AnimationClipRegistry;
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_core::string::StringPool;
use byroredux_hkx::{BehaviorGraph, HkxSkeleton};
use byroredux_plugin::esm::records::SubgraphData;
use byroredux_scripting::{GraphClip, LazyClips, LoadedGraph, ObjectAnimationGraph};
use std::collections::HashMap;
use std::sync::Arc;

/// Actors waiting for the graph their subgraph entry names. `NOT_SAVED_BY_DESIGN`:
/// re-queued by `archetype_idle_system`.
#[derive(Default)]
pub(crate) struct PendingActorGraphs(pub(crate) Vec<(EntityId, SubgraphData)>);

impl Resource for PendingActorGraphs {}

/// One resolved graph: shared by every actor whose entry has the same key.
struct CachedActorGraph {
    loaded: Arc<LoadedGraph>,
    lazy: Arc<LazyClips>,
    /// The mesh-relative animation folders, lower-cased, in search order.
    folders: Vec<String>,
    skeleton: Option<Arc<HkxSkeleton>>,
}

/// Graphs resolved so far, by [`super::idle_key`]; `None` is an entry whose graph did
/// not decode (tried once), and the decoded behaviors by path. `NOT_SAVED_BY_DESIGN`:
/// rebuilt from the archives.
#[derive(Default)]
pub(crate) struct ActorGraphCache {
    graphs: HashMap<String, Option<CachedActorGraph>>,
    behaviors: HashMap<String, Option<BehaviorGraph>>,
    skeletons: HashMap<String, Option<Arc<HkxSkeleton>>>,
}

impl Resource for ActorGraphCache {}

impl ActorGraphCache {
    /// Whether a graph is waiting on a clip.
    pub(crate) fn has_wanted(&self) -> bool {
        self.graphs
            .values()
            .flatten()
            .any(|graph| graph.lazy.peek_wanted())
    }
}

fn mesh_path(relative: &str) -> String {
    let relative = relative.trim_start_matches(['\\', '/']).replace('/', "\\").to_ascii_lowercase();
    if relative.starts_with("meshes\\") {
        relative
    } else {
        format!("meshes\\{relative}")
    }
}

fn project_directory(behavior_graph: &str) -> Option<String> {
    let lower = behavior_graph.trim_start_matches(['\\', '/']).replace('/', "\\").to_ascii_lowercase();
    let lower = lower.strip_prefix("meshes\\").unwrap_or(&lower).to_owned();
    let at = lower.rfind("\\behaviors\\")?;
    Some(lower[..at].to_owned())
}

/// What a clip name means below a folder: `animations\posea_idle1.hkt` is
/// `posea_idle1`, `animations\furniture\barber\x.hkt` is `furniture\barber\x`.
fn clip_remainder(animation: &str) -> String {
    let name = animation.replace('/', "\\").to_ascii_lowercase();
    let name = name.strip_prefix("animations\\").unwrap_or(&name);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem).to_owned()
}

fn decode_behavior(cache: &mut ActorGraphCache, provider: &TextureProvider, path: &str) -> Option<BehaviorGraph> {
    cache
        .behaviors
        .entry(path.to_owned())
        .or_insert_with(|| {
            let bytes = provider.extract_mesh(path)?;
            byroredux_hkx::decode_behavior_graph(&bytes)
                .map_err(|error| log::warn!("actor graph: behavior '{path}' did not decode: {error}"))
                .ok()
        })
        .clone()
}

fn build(cache: &mut ActorGraphCache, provider: &TextureProvider, entry: &SubgraphData) -> Option<CachedActorGraph> {
    let directory = project_directory(&entry.behavior_graph)?;
    let mut graph = decode_behavior(cache, provider, &mesh_path(&entry.behavior_graph))?;
    {
        let mut resolve = |reference: &str| {
            let path = mesh_path(&format!("{directory}\\{}", reference.trim_start_matches(['\\', '/'])));
            decode_behavior(cache, provider, &path)
        };
        graph.inline_references(&mut resolve);
    }
    let skeleton = cache
        .skeletons
        .entry(directory.clone())
        .or_insert_with(|| {
            let bytes = provider.extract_mesh(&mesh_path(&format!("{directory}\\characterassets\\skeleton.hkx")))?;
            byroredux_hkx::decode_skeleton(&bytes)
                .map_err(|error| log::warn!("actor graph: skeleton of '{directory}' did not decode: {error} - none of its clips can load"))
                .ok()
                .map(Arc::new)
        })
        .clone();
    let lazy = Arc::new(LazyClips::default());
    let loaded = Arc::new(LoadedGraph {
        graph,
        clips: HashMap::new(),
        sequences: HashMap::new(),
        lazy: Some(Arc::clone(&lazy)),
    });
    let folders = entry.animation_paths.iter().map(|folder| mesh_path(folder)).collect();
    Some(CachedActorGraph { loaded, lazy, folders, skeleton })
}

/// Whether [`populate_actor_graphs`] has something to do: an actor waiting for its graph
/// or a graph waiting for a clip. One resource read each, every frame.
pub(crate) fn actor_graph_work_pending(world: &World) -> bool {
    world.try_resource::<PendingActorGraphs>().is_some_and(|pending| !pending.0.is_empty())
        || world.try_resource::<ActorGraphCache>().is_some_and(|cache| cache.has_wanted())
}

/// The archives an interior session resolves actor clips from after its cell load has
/// let go of its own provider. `NOT_SAVED_BY_DESIGN`.
pub(crate) struct ActorClipProvider(pub(crate) TextureProvider);

impl Resource for ActorClipProvider {}

/// Attach the graphs actors are waiting for, and resolve the clips running graphs asked
/// for. Returns how many graphs were attached.
pub(crate) fn populate_actor_graphs(world: &mut World, provider: &TextureProvider) -> usize {
    world.init_resource::<ActorGraphCache>();
    let pending: Vec<(EntityId, SubgraphData)> = match world.try_resource_mut::<PendingActorGraphs>() {
        Some(mut pending) => std::mem::take(&mut pending.0),
        None => Vec::new(),
    };
    let mut attached = 0;
    for (actor, entry) in pending {
        // #5861 — a corpse gets no graph: it would re-animate it (FO4 ragdolls
        // do not activate yet, so nothing overrides the restored pose).
        if world.get::<byroredux_core::ecs::components::Dead>(actor).is_some() {
            continue;
        }
        let key = super::idle_key(&entry);
        let known = world.resource::<ActorGraphCache>().graphs.contains_key(&key);
        if !known {
            let built = {
                let mut cache = std::mem::take(&mut *world.resource_mut::<ActorGraphCache>());
                let built = build(&mut cache, provider, &entry);
                *world.resource_mut::<ActorGraphCache>() = cache;
                built
            };
            if built.is_none() {
                log::debug!("actor graph '{}' unavailable", entry.behavior_graph);
            }
            world.resource_mut::<ActorGraphCache>().graphs.insert(key.clone(), built);
        }
        let Some(loaded) = world.resource::<ActorGraphCache>().graphs.get(&key).and_then(|g| g.as_ref().map(|g| Arc::clone(&g.loaded))) else {
            continue;
        };
        provide_bones(world, &key);
        let Some(skeleton_root) = world.get::<crate::components::AnimationTarget>(actor).map(|target| target.skeleton_root) else {
            continue;
        };
        let Some(mut graph) = ObjectAnimationGraph::new(loaded, actor).map(|graph| graph.with_animated_root(skeleton_root)) else {
            continue;
        };
        let start = graph.start();
        if let Some(mut graphs) = world.query_mut::<ObjectAnimationGraph>() {
            graphs.insert(actor, graph);
        }
        if let Some(playback) = start {
            byroredux_scripting::animation_graph::apply_playback_for(world, actor, skeleton_root, playback);
        }
        attached += 1;
    }
    resolve_wanted(world, provider);
    attached
}

/// Give a graph's clips the skeleton's bone index (bone channel name -> index) its layered
/// blends' bone weights are addressed by, once.
fn provide_bones(world: &World, key: &str) {
    let (lazy, skeleton) = {
        let cache = world.resource::<ActorGraphCache>();
        match cache.graphs.get(key).and_then(Option::as_ref) {
            Some(graph) if !graph.lazy.has_bones() => (Arc::clone(&graph.lazy), graph.skeleton.clone()),
            _ => return,
        }
    };
    let Some(skeleton) = skeleton else { return };
    let mut pool = world.resource_mut::<StringPool>();
    let bones: rustc_hash::FxHashMap<_, _> =
        skeleton.bones.iter().enumerate().map(|(index, bone)| (pool.intern(&bone.name), index as u16)).collect();
    lazy.provide_bones(Arc::new(bones));
}

/// The times of a clip's `SyncLeft` and `SyncRight` annotations, when it has exactly one of
/// each: the points a synced blend aligns its clips by. Locomotion clips carry them
/// alternately, left first or right first (Fallout 4: 1,357 of the humanoid clips).
fn sync_points(clip: &byroredux_core::animation::AnimationClip, pool: &StringPool) -> Option<[f32; 2]> {
    let (mut left, mut right) = (Vec::new(), Vec::new());
    for (time, label) in &clip.text_keys {
        match pool.resolve(*label) {
            Some(name) if name.eq_ignore_ascii_case("SyncLeft") => left.push(*time),
            Some(name) if name.eq_ignore_ascii_case("SyncRight") => right.push(*time),
            _ => {}
        }
    }
    match (left.as_slice(), right.as_slice()) {
        ([left], [right]) if left.is_finite() && right.is_finite() && left != right => Some([*left, *right]),
        _ => None,
    }
}

/// Decode the clips the running graphs asked for and restart the graphs that were
/// waiting on them.
fn resolve_wanted(world: &mut World, provider: &TextureProvider) {
    struct Work {
        lazy: Arc<LazyClips>,
        loaded: Arc<LoadedGraph>,
        folders: Vec<String>,
        skeleton: Option<Arc<HkxSkeleton>>,
        wanted: Vec<(String, bool)>,
    }
    let work: Vec<Work> = world
        .resource::<ActorGraphCache>()
        .graphs
        .values()
        .flatten()
        .filter_map(|graph| {
            let wanted = graph.lazy.take_wanted();
            (!wanted.is_empty()).then(|| Work {
                lazy: Arc::clone(&graph.lazy),
                loaded: Arc::clone(&graph.loaded),
                folders: graph.folders.clone(),
                skeleton: graph.skeleton.clone(),
                wanted,
            })
        })
        .collect();
    for job in work {
        let mut provided = 0;
        for key in job.wanted {
            let remainder = clip_remainder(&key.0);
            let path = job
                .folders
                .iter()
                .map(|folder| format!("{folder}\\{remainder}.hkx"))
                .find(|path| provider.has_mesh(path));
            let resolved = path.as_ref().and_then(|path| {
                let registry_key = format!("{path}#{}", if key.1 { "loop" } else { "once" });
                if let Some(handle) = world.resource::<AnimationClipRegistry>().get_by_path(&registry_key) {
                    let duration = world.resource::<AnimationClipRegistry>().get(handle)?.duration;
                    return Some(GraphClip { handle, duration });
                }
                let Some(skeleton) = job.skeleton.as_ref() else {
                    log::debug!("actor graph clip '{path}' not loaded: the graph's skeleton did not decode (reported once, above)");
                    return None;
                };
                let bytes = provider.extract_mesh(path)?;
                let animation = byroredux_hkx::decode_animation(&bytes)
                    .map_err(|error| log::warn!("actor graph: clip '{path}' did not decode: {error}"))
                    .ok()?;
                let converted = {
                    let mut pool = world.resource_mut::<StringPool>();
                    convert_hkx_clip(path, "graph", skeleton, &animation, &mut pool, !key.1, None)
                };
                let duration = converted.duration;
                let handle = world
                    .resource_mut::<AnimationClipRegistry>()
                    .get_or_insert_by_path(registry_key, || converted);
                Some(GraphClip { handle, duration })
            });
            match resolved {
                Some(clip) => {
                    log::debug!("actor graph clip '{}' (looping {}) -> handle {} ({:.2} s)", key.0, key.1, clip.handle, clip.duration);
                    let points = {
                        let pool = world.resource::<StringPool>();
                        world.resource::<AnimationClipRegistry>().get(clip.handle).and_then(|registered| sync_points(registered, &pool))
                    };
                    if let Some(points) = points {
                        job.lazy.provide_sync(clip.handle, points);
                    }
                    job.lazy.provide(key, clip);
                    provided += 1;
                }
                None => {
                    // A found file that did not load has said why above; this is the file that
                    // is simply not in any of the folders.
                    if path.is_none() {
                        log::debug!("actor graph clip '{}' not found in {:?}", key.0, job.folders.first());
                    }
                    job.lazy.fail(key);
                }
            }
        }
        if provided == 0 {
            continue;
        }
        // The graphs waiting on these clips start playing them.
        let restarts: Vec<(EntityId, EntityId, byroredux_scripting::Playback)> = {
            let Some(mut graphs) = world.query_mut::<ObjectAnimationGraph>() else { continue };
            graphs
                .iter_mut()
                .filter(|(_, graph)| std::ptr::eq(graph.graph(), &*job.loaded))
                .filter_map(|(_, graph)| graph.start().map(|playback| (graph.root, graph.animated(), playback)))
                .collect()
        };
        for (owner, animated, playback) in restarts {
            byroredux_scripting::animation_graph::apply_playback_for(world, owner, animated, playback);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::animation::{AnimationClip, CycleType};

    fn clip_with(labels: &[(f32, &str)], pool: &mut StringPool) -> AnimationClip {
        AnimationClip {
            name: "test".into(),
            duration: 1.0,
            cycle_type: CycleType::Loop,
            frequency: 1.0,
            phase: 0.0,
            weight: 1.0,
            accum_root_name: None,
            channels: Default::default(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: labels.iter().map(|(time, label)| (*time, pool.intern(label))).collect(),
        }
    }

    /// A locomotion clip's sync points are its one `SyncLeft` and one `SyncRight`, in either
    /// order; a clip with more or fewer of them is not synced by point.
    #[test]
    fn sync_points_are_the_one_left_and_one_right_annotation() {
        let mut pool = StringPool::new();
        let walk = clip_with(&[(0.33, "FootLeft"), (0.33, "SyncLeft"), (0.93, "SyncRight")], &mut pool);
        assert_eq!(sync_points(&walk, &pool), Some([0.33, 0.93]));
        let run = clip_with(&[(0.27, "syncright"), (0.63, "SYNCLEFT")], &mut pool);
        assert_eq!(sync_points(&run, &pool), Some([0.63, 0.27]), "left first, whatever the order in the file");
        let only_left = clip_with(&[(0.3, "SyncLeft")], &mut pool);
        assert_eq!(sync_points(&only_left, &pool), None);
        let four = clip_with(&[(0.1, "SyncLeft"), (0.2, "SyncRight"), (0.3, "SyncLeft"), (0.4, "SyncRight")], &mut pool);
        assert_eq!(sync_points(&four, &pool), None, "a multi-step cycle falls back to even phase");
    }
}
