//! Placed objects' Havok behavior graphs (M47.2): resolve the project a NIF
//! names, decode its character, behavior graph, skeleton and clips from the
//! archives, register the clips, and attach the running
//! [`ObjectAnimationGraph`] — see `byroredux_scripting::animation_graph`.
//!
//! A project is `meshes\<dir>\<Name>.hkx`; its character, the character's
//! skeleton, behavior and animations are named relative to `<dir>`.
//! The 64-bit packfiles of Skyrim Special Edition (Havok 2010) and Fallout 4
//! (Havok 2014) decode; anything else fails to decode and the object keeps no
//! graph (the animation natives then answer the way the CK documents a missing
//! graph). A clip that cannot be loaded costs its own states and nothing else:
//! the graph keeps running with those states left still, and the load says so.

use super::animation::convert_hkx_clip;
use super::TextureProvider;
use byroredux_core::animation::AnimationClipRegistry;
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_core::string::StringPool;
use byroredux_scripting::{BehaviorGraphRef, GraphClip, LoadedGraph, ObjectAnimationGraph};
use std::collections::HashMap;
use std::sync::Arc;

/// Graphs resolved so far, by lower-cased project path; `None` is a project
/// that did not decode (tried once). `NOT_SAVED_BY_DESIGN`: rebuilt from the
/// archives.
#[derive(Default)]
pub(crate) struct ObjectGraphCache {
    graphs: HashMap<String, Option<Arc<LoadedGraph>>>,
    /// Entities whose graph could not be attached, with the inputs that failed:
    /// the project and the sequences of their own NIF. The pass skips them
    /// until either changes (#6126) instead of re-deriving the cache key and
    /// finding the cached `None` every streaming frame.
    stuck: HashMap<EntityId, StuckGraph>,
}

/// What an entity's failed attach was made of (compared by identity).
struct StuckGraph {
    project: Arc<str>,
    sequences: Option<Arc<Vec<(String, GraphClip)>>>,
}

impl ObjectGraphCache {
    fn is_stuck(
        &self,
        entity: EntityId,
        project: &Arc<str>,
        sequences: Option<&Arc<Vec<(String, GraphClip)>>>,
    ) -> bool {
        self.stuck.get(&entity).is_some_and(|stuck| {
            Arc::ptr_eq(&stuck.project, project)
                && match (&stuck.sequences, sequences) {
                    (None, None) => true,
                    (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                    _ => false,
                }
        })
    }
}

/// Stuck entries past this many are checked for despawned entities.
const STUCK_PRUNE_THRESHOLD: usize = 256;

impl Resource for ObjectGraphCache {}

fn mesh_path(relative: &str) -> String {
    let relative = relative.trim_start_matches(['\\', '/']).replace('/', "\\");
    if relative.get(..7).is_some_and(|prefix| prefix.eq_ignore_ascii_case("meshes\\")) {
        relative
    } else {
        format!("meshes\\{relative}")
    }
}

/// `name` as a behavior project authors it — backslashes or slashes, `.` and `..` hops — joined onto
/// `directory` and collapsed to the exact key the archives store (#6556). FO4 characters name a
/// shared rig directories up (`..\..\GenericBehaviors\zSingleBoneSkeleton\SingleBoneSkeleton.hkt`),
/// and an archive lookup is an exact hit. A hop above the archive root stays at the root.
fn project_relative(directory: &str, name: &str) -> String {
    let mut parts: Vec<&str> = directory.split(['\\', '/']).filter(|part| !part.is_empty()).collect();
    for segment in name.split(['\\', '/']) {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            segment => parts.push(segment),
        }
    }
    parts.join("\\")
}

/// The archive paths a Havok file a project names is looked for at: as authored, then, for the
/// Havok toolchain's `.hkt` tagfile spelling (any case: `Skeleton.HKT`), the `.hkx` the FO4 archives
/// ship in its place (#6556) — the extension the actor route reads its skeletons by. FO4 characters
/// spell their rigs and FO4 graphs their clips that way.
fn havok_paths(path: &str) -> Vec<String> {
    let mut paths = vec![path.to_owned()];
    if path.to_ascii_lowercase().ends_with(".hkt") {
        // The suffix is ASCII, so the stem ends on a char boundary.
        paths.push(format!("{}.hkx", &path[..path.len() - 4]));
    }
    paths
}

/// Register every clip any state of `graph` plays, nested machines and blenders included.
/// `path_of` names the file a clip's animation is read from and `load` reads and decodes
/// it, but only for a clip not already registered. A clip it cannot load is reported and left
/// out, so the states that play it stay still and every other state of the graph still
/// animates (the runtime already treats an unregistered clip as "nothing to play"). `None`
/// only when the graph has no machine at all.
fn register_clips(
    world: &mut World,
    graph: &byroredux_hkx::BehaviorGraph,
    skeleton: &byroredux_hkx::HkxSkeleton,
    project_path: &str,
    path_of: &dyn Fn(&str) -> String,
    load: &mut dyn FnMut(&str) -> Result<byroredux_hkx::HkxAnimation, String>,
) -> Option<HashMap<(String, bool), GraphClip>> {
    fn collect<'a>(machine: &'a byroredux_hkx::StateMachine, out: &mut Vec<(&'a str, &'a byroredux_hkx::StateClip)>) {
        for state in &machine.states {
            for clip in state.clip.iter().chain(&state.content.blended_clips) {
                out.push((&state.name, clip));
            }
            for inner in &state.content.machines {
                collect(inner, out);
            }
        }
    }
    let mut playing = Vec::new();
    collect(graph.machine.as_ref()?, &mut playing);
    let mut clips = HashMap::new();
    let mut failed: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (state_name, clip) in playing {
        let looping = clip.mode == byroredux_hkx::ClipMode::Looping;
        let key = (clip.animation.to_ascii_lowercase(), looping);
        if clips.contains_key(&key) || failed.contains(&clip.animation.to_ascii_lowercase()) {
            continue;
        }
        let (handle, duration) = match load_clip(world, skeleton, state_name, looping, &path_of(&clip.animation), load) {
            Ok(registered) => registered,
            Err(why) => {
                log::warn!(
                    "behavior graph '{project_path}': state '{state_name}' plays clip '{}', which could not be loaded \
                     ({why}); the graph runs without it",
                    clip.animation
                );
                failed.insert(clip.animation.to_ascii_lowercase());
                continue;
            }
        };
        clips.insert(key, GraphClip { handle, duration });
    }
    Some(clips)
}

/// The registry handle and duration of `clip`, registering it from the file at `path` (read
/// through `load`) unless a clip of that path and loop mode is registered already.
fn load_clip(
    world: &mut World,
    skeleton: &byroredux_hkx::HkxSkeleton,
    state_name: &str,
    looping: bool,
    path: &str,
    load: &mut dyn FnMut(&str) -> Result<byroredux_hkx::HkxAnimation, String>,
) -> Result<(u32, f32), String> {
    let registry_key = format!("{}#{}", path.to_ascii_lowercase(), if looping { "loop" } else { "once" });
    let existing = world.resource::<AnimationClipRegistry>().get_by_path(&registry_key);
    if let Some(handle) = existing {
        let duration = world.resource::<AnimationClipRegistry>().get(handle).map(|registered| registered.duration);
        return duration.map(|duration| (handle, duration)).ok_or_else(|| "its registry entry is gone".to_owned());
    }
    let animation = load(path)?;
    let converted = convert_hkx_clip(path, state_name, skeleton, &animation, &mut world.resource_mut::<StringPool>(), !looping, None);
    let duration = converted.duration;
    let handle = world.resource_mut::<AnimationClipRegistry>().get_or_insert_by_path(registry_key, || converted);
    Ok((handle, duration))
}

/// Decode `project` and register its clips.
fn load_graph(
    world: &mut World,
    provider: &TextureProvider,
    project: &str,
    sequences: &HashMap<String, GraphClip>,
) -> Option<Arc<LoadedGraph>> {
    let project_path = mesh_path(project);
    let directory = project_path.rsplit_once('\\')?.0.to_owned();
    let relative = |name: &str| project_relative(&directory, name);
    let fetch = |path: &str, what: &str| {
        let bytes = havok_paths(path).iter().find_map(|candidate| provider.extract_mesh(candidate));
        if bytes.is_none() {
            log::debug!("behavior graph '{project_path}': {what} '{path}' is not in the archives");
        }
        bytes
    };
    let characters = byroredux_hkx::decode_project(&fetch(&project_path, "project")?)
        .map_err(|error| log::warn!("behavior graph '{project_path}': project did not decode: {error}"))
        .ok()?;
    let character = byroredux_hkx::decode_character(&fetch(&relative(characters.first()?), "character")?)
        .map_err(|error| log::warn!("behavior graph '{project_path}': character did not decode: {error}"))
        .ok()?;
    let graph = byroredux_hkx::decode_behavior_graph(&fetch(&relative(&character.behavior), "behavior")?)
        .map_err(|error| log::warn!("behavior graph '{project_path}': behavior did not decode: {error}"))
        .ok()?;
    let skeleton = byroredux_hkx::decode_skeleton(&fetch(&relative(&character.rig), "skeleton")?)
        .map_err(|error| log::warn!("behavior graph '{project_path}': skeleton did not decode: {error}"))
        .ok()?;

    let clips = register_clips(world, &graph, &skeleton, &project_path, &relative, &mut |path| {
        let bytes = havok_paths(path)
            .iter()
            .find_map(|candidate| provider.extract_mesh(candidate))
            .ok_or_else(|| "it is not in the archives".to_owned())?;
        byroredux_hkx::decode_animation(&bytes).map_err(|error| error.to_string())
    })?;
    Some(Arc::new(LoadedGraph { graph, clips, sequences: sequences.clone(), lazy: None }))
}

/// Attach a running behavior graph to every placed object whose NIF names one
/// and that has none yet. Idempotent and cheap when nothing is pending.
/// Returns how many objects got a graph.
pub(crate) fn populate_object_graphs(world: &mut World, provider: &TextureProvider) -> usize {
    // (entity, project, the sequences of the entity's own NIF). One guard at a time.
    let wanting: Vec<(EntityId, Arc<str>)> = {
        let Some(refs) = world.query::<BehaviorGraphRef>() else {
            return 0;
        };
        let graphs = world.query::<ObjectAnimationGraph>();
        refs.iter()
            .filter(|(entity, _)| graphs.as_ref().is_none_or(|graphs| graphs.get(*entity).is_none()))
            .map(|(entity, reference)| (entity, Arc::clone(&reference.0)))
            .collect()
    };
    // (entity, project, the decoded sequences of the entity's own NIF)
    type PendingClip = (EntityId, Arc<str>, Option<Arc<Vec<(String, GraphClip)>>>);
    let pending: Vec<PendingClip> = {
        let sequences = world.query::<byroredux_scripting::GraphSequences>();
        let cache = world.try_resource::<ObjectGraphCache>();
        wanting
            .into_iter()
            .map(|(entity, project)| {
                let own = sequences.as_ref().and_then(|s| s.get(entity)).map(|s| Arc::clone(&s.0));
                (entity, project, own)
            })
            .filter(|(entity, project, own)| {
                !cache.as_ref().is_some_and(|cache| cache.is_stuck(*entity, project, own.as_ref()))
            })
            .collect()
    };
    if pending.is_empty() {
        return 0;
    }
    world.init_resource::<ObjectGraphCache>();
    let mut attached = 0;
    for (entity, project, own) in pending {
        let own_sequences = own.clone().unwrap_or_default();
        // A project can be shared by NIFs with different sequences (generic
        // behaviors), so the cache is keyed by both.
        let mut names: Vec<&str> = own_sequences.iter().map(|(name, _)| name.as_str()).collect();
        names.sort_unstable();
        let key = format!("{}|{}", project.to_ascii_lowercase(), names.join(","));
        let cached = world.resource::<ObjectGraphCache>().graphs.get(&key).cloned();
        let loaded = match cached {
            Some(loaded) => loaded,
            None => {
                let sequences: HashMap<String, GraphClip> = own_sequences.iter().cloned().collect();
                let loaded = load_graph(world, provider, &project, &sequences);
                match &loaded {
                    Some(graph) => log::info!(
                        "Behavior graph '{project}': {} events, {} clips, {} sequences",
                        graph.graph.events.len(),
                        graph.clips.len(),
                        graph.sequences.len()
                    ),
                    None => log::debug!("Behavior graph '{project}' unavailable"),
                }
                world.resource_mut::<ObjectGraphCache>().graphs.insert(key, loaded.clone());
                loaded
            }
        };
        let Some(mut graph) = loaded.and_then(|loaded| ObjectAnimationGraph::new(loaded, entity)) else {
            // Unavailable (cached `None`) or machine-less: skip this entity
            // until its project or sequences change (#6126).
            let despawned: Vec<EntityId> = {
                let cache = world.resource::<ObjectGraphCache>();
                if cache.stuck.len() >= STUCK_PRUNE_THRESHOLD {
                    let refs = world.query::<BehaviorGraphRef>();
                    cache
                        .stuck
                        .keys()
                        .copied()
                        .filter(|stuck| refs.as_ref().is_none_or(|refs| refs.get(*stuck).is_none()))
                        .collect()
                } else {
                    Vec::new()
                }
            };
            let mut cache = world.resource_mut::<ObjectGraphCache>();
            for gone in despawned {
                cache.stuck.remove(&gone);
            }
            cache.stuck.insert(entity, StuckGraph { project, sequences: own });
            continue;
        };
        let start = graph.start();
        // The graph owns the prop's animation: the NIF's own embedded controller
        // player (spawned beside the placement) would write the same node's
        // transform every frame and override it.
        let embedded: Vec<EntityId> = world
            .query::<byroredux_core::animation::AnimationPlayer>()
            .map(|players| {
                players
                    .iter()
                    .filter(|(player, state)| *player != entity && state.root_entity == Some(entity))
                    .map(|(player, _)| player)
                    .collect()
            })
            .unwrap_or_default();
        for player in embedded {
            world.despawn(player);
        }
        world.insert(entity, graph);
        log::debug!("Behavior graph '{project}' attached to entity {entity} (start playback {start:?})");
        if let Some(playback) = start {
            byroredux_scripting::animation_graph::apply_playback(world, entity, playback);
        }
        attached += 1;
    }
    attached
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_hkx::{
        BehaviorGraph, ClipMode, HkxAnimation, HkxBone, HkxSkeleton, HkxTransform, State, StateClip, StateContent, StateMachine,
    };

    fn state(id: i32, name: &str, animation: &str, mode: ClipMode) -> State {
        State {
            id,
            name: name.into(),
            transitions: Vec::new(),
            clip: Some(StateClip {
                animation: animation.into(),
                mode,
                playback_speed: 1.0,
                start_time: 0.0,
                user_fraction: 0.0,
                triggers: Vec::new(),
                playback_speed_variable: None,
            }),
            content: StateContent::default(),
        }
    }

    fn graph_of(states: Vec<State>) -> BehaviorGraph {
        BehaviorGraph {
            variables: Vec::new(),
            events: Vec::new(),
            machine: Some(StateMachine { start_state: 0, start_state_variable: None, states, wildcard: Vec::new() }),
        }
    }

    fn rig() -> HkxSkeleton {
        HkxSkeleton {
            name: "Rig".into(),
            bones: vec![HkxBone { name: "NPC Root [Root]".into(), parent_index: -1, reference_pose: HkxTransform::IDENTITY }],
        }
    }

    fn animation() -> HkxAnimation {
        HkxAnimation {
            duration: 1.0,
            num_frames: 1,
            frame_duration: 1.0,
            tracks: vec![vec![HkxTransform::IDENTITY]],
            track_to_bone: vec![0],
            annotations: Vec::new(),
            dropped_bones: Vec::new(),
            reference_frame: None,
            reference_frame_dropped: None,
            additive: false,
        }
    }

    fn world() -> World {
        let mut world = World::new();
        world.insert_resource(AnimationClipRegistry::new());
        world.insert_resource(StringPool::new());
        world
    }

    /// #6556 — a project-relative name is collapsed to the exact archive key: FO4 characters name a
    /// shared rig directories up, and the archives hold no `..` keys.
    #[test]
    fn project_relative_names_collapse_dot_dot_hops() {
        let dir = r"meshes\DLC05\SetDressing\Workshop\ConveyorBelts\DLC05WorkshopConveyorBelts";
        assert_eq!(
            project_relative(dir, r"..\..\..\..\..\GenericBehaviors\zSingleBoneSkeleton\SingleBoneSkeleton.hkt"),
            r"meshes\GenericBehaviors\zSingleBoneSkeleton\SingleBoneSkeleton.hkt"
        );
        assert_eq!(project_relative(dir, r"Behaviors/Behavior.hkx"), format!(r"{dir}\Behaviors\Behavior.hkx"));
        assert_eq!(project_relative(dir, r"\.\Animations\Idle.hkt"), format!(r"{dir}\Animations\Idle.hkt"));
        assert_eq!(project_relative(r"meshes\a", r"..\..\..\b.hkx"), "b.hkx", "a hop above the root stays at it");
    }

    /// #6556 — a `.hkt` spelling (any case) is also looked for as the `.hkx` the archives ship; any other
    /// name is looked for as authored only.
    #[test]
    fn havok_paths_try_the_hkx_for_a_hkt_spelling() {
        assert_eq!(havok_paths(r"a\CharacterAssets\Skeleton.HKT"), [r"a\CharacterAssets\Skeleton.HKT", r"a\CharacterAssets\Skeleton.hkx"]);
        assert_eq!(havok_paths(r"a\Animations\Idle.hkt"), [r"a\Animations\Idle.hkt", r"a\Animations\Idle.hkx"]);
        assert_eq!(havok_paths(r"a\Behaviors\Behavior.hkx"), [r"a\Behaviors\Behavior.hkx"]);
        assert_eq!(havok_paths("hkt"), ["hkt"]);
    }

    /// #6556 — against the shipped data: FO4 prop graphs load. Their characters spell the rig
    /// `..\..\GenericBehaviors\zSingleBoneSkeleton\SingleBoneSkeleton.hkt` or `CharacterAssets\Skeleton.HKT`
    /// and their graphs name `.hkt` clips; the archives ship `.hkx`. Before the fix none of the 187
    /// vanilla projects loaded; 184 do now (the other three are authoring errors in the data).
    ///
    ///   cargo test -p byroredux --bin byroredux fo4_prop_behavior_graphs -- --ignored
    #[test]
    #[ignore = "needs Fallout 4 game data on disk"]
    fn fo4_prop_behavior_graphs_load_with_their_rigs_and_clips() {
        let data = byroredux_plugin::esm::test_paths::fo4_data_dir();
        let archives = ["Fallout4 - Animations.ba2", "Fallout4 - Meshes.ba2", "DLCworkshop02 - Main.ba2", "DLCworkshop03 - Main.ba2"];
        let mut args = Vec::new();
        for file_name in archives {
            let path = data.join(file_name);
            if !path.is_file() {
                byroredux_plugin::esm::test_paths::require_or_skip(&path.display().to_string());
                return;
            }
            args.extend(["--bsa".to_owned(), path.to_string_lossy().into_owned()]);
        }
        let provider = crate::asset_provider::build_texture_provider(&args);
        // (project, its rig spelling, whether its graph names clips of its own)
        let projects = [
            (r"GenericBehaviors\Autoplay\Autoplay.hkx", "..-relative .hkt", false),
            (r"DLC05\SetDressing\Workshop\ConveyorBelts\DLC05WorkshopConveyorBelts\DLC05WorkshopConveyorBelts.hkx", "..-relative .hkt", false),
            (r"DLC05\SetDressing\Workshop\PitchingMachine\DLC05WorkshopPitchingMachine01.hkx", "CharacterAssets .hkt", true),
        ];
        for (project, rig, has_clips) in projects {
            let mut world = world();
            let loaded = load_graph(&mut world, &provider, project, &HashMap::new())
                .unwrap_or_else(|| panic!("{project} ({rig} rig) did not load"));
            if has_clips {
                assert!(!loaded.clips.is_empty(), "{project}: its .hkt clips must load from the .hkx");
            }
        }
    }

    /// #6126 — a prop whose behaviour graph is unavailable is skipped by later passes
    /// instead of being re-keyed and re-looked-up every streaming frame, until its
    /// project or its own NIF's sequences change.
    #[test]
    fn a_prop_whose_graph_is_unavailable_is_not_retried_until_its_inputs_change() {
        use byroredux_scripting::{BehaviorGraphRef, GraphSequences};
        let mut world = world();
        byroredux_scripting::register(&mut world);
        let provider = TextureProvider::new();
        let prop = world.spawn();
        world.insert(prop, BehaviorGraphRef(Arc::from("Meshes\\Missing\\Missing.hkx")));

        assert_eq!(populate_object_graphs(&mut world, &provider), 0);
        {
            let cache = world.resource::<ObjectGraphCache>();
            assert_eq!(cache.stuck.len(), 1, "the failed prop is remembered");
            assert_eq!(cache.graphs.len(), 1, "and its failure cached once");
        }
        // Later passes skip it: nothing new is keyed or recorded.
        for _ in 0..3 {
            assert_eq!(populate_object_graphs(&mut world, &provider), 0);
        }
        assert_eq!(world.resource::<ObjectGraphCache>().graphs.len(), 1);

        // New sequences from its NIF are new inputs: tried again, under a new key.
        world.insert(prop, GraphSequences(Arc::new(vec![("Open".to_owned(), GraphClip { handle: 1, duration: 1.0 })])));
        assert_eq!(populate_object_graphs(&mut world, &provider), 0);
        assert_eq!(world.resource::<ObjectGraphCache>().graphs.len(), 2, "re-attempted with the new sequences");
        assert_eq!(world.resource::<ObjectGraphCache>().stuck.len(), 1);
    }

    /// One clip the archives do not have (or that does not decode) costs the states that play
    /// it, not the object's whole graph: the others register, and the runtime leaves the state
    /// with no clip still. It used to return `None` for the whole project, cached for good.
    #[test]
    fn a_clip_that_cannot_load_costs_its_own_states_not_the_graph() {
        let graph = graph_of(vec![
            state(0, "Idle", "Animations\\Idle.hkx", ClipMode::Looping),
            state(1, "Gone", "Animations\\Gone.hkx", ClipMode::SinglePlay),
            state(2, "Broken", "Animations\\Broken.hkx", ClipMode::SinglePlay),
            state(3, "Open", "Animations\\Open.hkx", ClipMode::SinglePlay),
        ]);
        let mut world = world();
        let mut asked = Vec::new();
        let clips = register_clips(&mut world, &graph, &rig(), "meshes\\x\\x.hkx", &|name| format!("meshes\\x\\{name}"), &mut |path| {
            asked.push(path.to_owned());
            if path.ends_with("Gone.hkx") {
                Err("it is not in the archives".to_owned())
            } else if path.ends_with("Broken.hkx") {
                Err("HKX object class 'hkaSplineCompressedAnimation' is missing".to_owned())
            } else {
                Ok(animation())
            }
        })
        .expect("a graph with a machine");
        let mut registered: Vec<&str> = clips.keys().map(|(name, _)| name.as_str()).collect();
        registered.sort_unstable();
        assert_eq!(registered, ["animations\\idle.hkx", "animations\\open.hkx"]);
        assert!(clips[&("animations\\idle.hkx".to_owned(), true)].duration > 0.0);
        assert_eq!(asked.len(), 4, "each distinct clip is loaded once");
    }

    #[test]
    fn a_clip_two_states_share_is_loaded_once_and_a_registered_one_not_at_all() {
        let graph = graph_of(vec![
            state(0, "A", "Animations\\Walk.hkx", ClipMode::Looping),
            state(1, "B", "animations\\walk.hkx", ClipMode::Looping),
        ]);
        let mut world = world();
        let loads = std::cell::Cell::new(0);
        let mut load = |_: &str| {
            loads.set(loads.get() + 1);
            Ok(animation())
        };
        let first = register_clips(&mut world, &graph, &rig(), "p", &|name| name.to_owned(), &mut load).unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(loads.get(), 1);
        // A second graph naming the same file finds it in the registry and reads nothing.
        let second = register_clips(&mut world, &graph, &rig(), "p", &|name| name.to_owned(), &mut load).unwrap();
        assert_eq!(second, first);
        assert_eq!(loads.get(), 1, "the registered clip is not read again");
    }

    #[test]
    fn a_graph_without_a_machine_registers_nothing() {
        let graph = BehaviorGraph { variables: Vec::new(), events: Vec::new(), machine: None };
        let mut world = world();
        assert!(register_clips(&mut world, &graph, &rig(), "p", &|name| name.to_owned(), &mut |_| Ok(animation())).is_none());
    }
}

