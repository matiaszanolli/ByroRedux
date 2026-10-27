//! Node entities for animated props (M47.2).
//!
//! The cell loader flattens a placed NIF: one entity per mesh, parented to the
//! placement root, with the NiNode hierarchy folded into the mesh transforms.
//! A prop that a Havok behavior graph animates (a lever, a portcullis, a
//! trap) moves *NiNodes* — the graph's skeleton bones are node names such as
//! `Lever01` — so those NIFs keep their node tree: [`build`] records it from a
//! hierarchical import (kept on the cache entry as [`GraphNodes`]) and
//! [`spawn_nodes`] creates the node entities under the placement root, with
//! each mesh re-parented beneath its node at the transform the hierarchical
//! import gives it relative to that node. The meshes' `GlobalTransform` are
//! unchanged (the world pose is the same), so a prop at rest renders exactly as
//! before; only the animation can now find, and move, the bone entities.

use std::collections::HashMap;
use std::sync::Arc;

use byroredux_core::ecs::{EntityId, GlobalTransform, Name, Parent, Transform, World};
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::StringPool;
use byroredux_nif::import::ImportedMesh;
use byroredux_nif::scene::NifScene;

/// One NiNode of the NIF, local to its parent.
#[derive(Debug, Clone)]
pub(super) struct GraphNode {
    pub(super) name: Option<Arc<str>>,
    pub(super) parent: Option<usize>,
    pub(super) translation: Vec3,
    pub(super) rotation: Quat,
    pub(super) scale: f32,
}

/// A flat mesh's place in the node tree: its node and its transform local to it.
#[derive(Debug, Clone, Copy)]
pub(super) struct MeshNodeLocal {
    pub(super) node: usize,
    pub(super) translation: Vec3,
    pub(super) rotation: Quat,
    pub(super) scale: f32,
}

/// The node tree of a behavior-graph NIF.
#[derive(Debug, Clone)]
pub(super) struct GraphNodes {
    pub(super) nodes: Vec<GraphNode>,
    /// Per flat collision (same order as `CachedNifImport::collisions`): the node
    /// whose pose it stands at, when it is that node's own body - the collider
    /// then rides the node as the animation moves it.
    pub(super) collision_nodes: Vec<Option<usize>>,
    /// Per flat mesh (same order as `CachedNifImport::meshes`): its node, when
    /// it hangs under one below the root.
    pub(super) meshes: Vec<Option<MeshNodeLocal>>,
}

fn is_identity(translation: [f32; 3], rotation: [f32; 4], scale: f32) -> bool {
    translation.iter().all(|v| v.abs() < 1.0e-4)
        && (rotation[0].abs() + rotation[1].abs() + rotation[2].abs()) < 1.0e-4
        && (rotation[3] - 1.0).abs() < 1.0e-4
        && (scale - 1.0).abs() < 1.0e-4
}

/// Record the node tree of `scene`, whose flat import is `flat`. `None` when the
/// hierarchical import does not line up with the flat one mesh for mesh (the
/// props this serves are simple, and a mismatch keeps the flat spawn), or the
/// root carries a transform the flat import folds in differently.
pub(super) fn build(
    scene: &NifScene,
    flat: &[ImportedMesh],
    collisions: &[byroredux_nif::import::ImportedCollision],
    pool: &mut StringPool,
) -> Option<Arc<GraphNodes>> {
    let imported = byroredux_nif::import::import_nif_scene(scene, pool);
    if imported.meshes.len() != flat.len()
        || imported.meshes.iter().zip(flat).any(|(hierarchical, flat)| hierarchical.name != flat.name)
    {
        log::debug!(
            "graph nodes: hierarchical import has {} meshes against {} flat — keeping the flat spawn",
            imported.meshes.len(),
            flat.len()
        );
        return None;
    }
    let root = imported.nodes.iter().position(|node| node.parent_node.is_none())?;
    let root_node = &imported.nodes[root];
    if !is_identity(root_node.translation, root_node.rotation, root_node.scale) {
        log::debug!("graph nodes: the root node carries a transform — keeping the flat spawn");
        return None;
    }
    let nodes: Vec<GraphNode> = imported
        .nodes
        .iter()
        .map(|node| GraphNode {
            name: node.name.clone(),
            parent: node.parent_node,
            translation: Vec3::from_array(node.translation),
            rotation: Quat::from_xyzw(node.rotation[0], node.rotation[1], node.rotation[2], node.rotation[3]),
            scale: node.scale,
        })
        .collect();
    let meshes = imported
        .meshes
        .iter()
        .map(|mesh| {
            let node = mesh.parent_node.filter(|&node| node != root)?;
            Some(MeshNodeLocal {
                node,
                translation: Vec3::from_array(mesh.translation),
                rotation: Quat::from_xyzw(mesh.rotation[0], mesh.rotation[1], mesh.rotation[2], mesh.rotation[3]),
                scale: mesh.scale,
            })
        })
        .collect();
    let collision_nodes = pair_collisions(&nodes, &imported.nodes, collisions);
    Some(Arc::new(GraphNodes { nodes, collision_nodes, meshes }))
}

/// The pose of node `index` in the NIF's frame (its transforms composed down from
/// the root).
fn nif_pose(nodes: &[GraphNode], index: usize) -> (Vec3, Quat, f32) {
    let node = &nodes[index];
    match node.parent {
        None => (node.translation, node.rotation, node.scale),
        Some(parent) => {
            let (pos, rot, scale) = nif_pose(nodes, parent);
            GlobalTransform::compose_trs(pos, rot, scale, node.translation, node.rotation, node.scale)
        }
    }
}

/// For each flat collision, the node below the root that carries a body and stands
/// at its pose. The root is the placement root itself and never gets a node
/// entity, so a body on it stays on the flat detached spawn, like a mesh straight
/// under the root (#5932).
fn pair_collisions(
    nodes: &[GraphNode],
    imported: &[byroredux_nif::import::ImportedNode],
    collisions: &[byroredux_nif::import::ImportedCollision],
) -> Vec<Option<usize>> {
    collisions
        .iter()
        .map(|collision| {
            let position = Vec3::from_array(collision.translation);
            let rotation = Quat::from_xyzw(
                collision.rotation[0],
                collision.rotation[1],
                collision.rotation[2],
                collision.rotation[3],
            );
            (0..nodes.len()).filter(|&index| nodes[index].parent.is_some() && imported[index].collision.is_some()).find(|&index| {
                let (pos, rot, scale) = nif_pose(nodes, index);
                pos.distance(position) < 1.0e-2 && rot.dot(rotation).abs() > 0.9999 && (scale - collision.scale).abs() < 1.0e-3
            })
        })
        .collect()
}

#[cfg(test)]
impl GraphNodes {
    /// A graph of `(name, parent, translation, rotation)` nodes whose meshes are
    /// none and whose collision-carrying nodes are `collision_nodes` (tests).
    pub(super) fn for_test(
        nodes: Vec<(&str, Option<usize>, Vec3, Quat)>,
        collision_nodes: Vec<Option<usize>>,
    ) -> Self {
        Self {
            nodes: nodes
                .into_iter()
                .map(|(name, parent, translation, rotation)| GraphNode {
                    name: Some(name.into()),
                    parent,
                    translation,
                    rotation,
                    scale: 1.0,
                })
                .collect(),
            collision_nodes,
            meshes: Vec::new(),
        }
    }
}

/// The node entities of one placement, by node index.
pub(super) struct SpawnedNodes {
    entities: HashMap<usize, EntityId>,
}

impl SpawnedNodes {
    /// The entity of node `index`, if it was spawned.
    pub(super) fn node_entity(&self, index: usize) -> Option<EntityId> {
        self.entities.get(&index).copied()
    }

    /// The entity and local transform mesh `index` hangs from, if it has a node.
    pub(super) fn mesh_parent(&self, graph: &GraphNodes, index: usize) -> Option<(EntityId, MeshNodeLocal)> {
        let local = (*graph.meshes.get(index)?)?;
        Some((*self.entities.get(&local.node)?, local))
    }
}

/// Create the node entities every mesh of `graph` hangs from (and their
/// ancestors) under `placement_root`, which stands at `ref_*` in the world.
pub(super) fn spawn_nodes(
    world: &mut World,
    graph: &GraphNodes,
    placement_root: EntityId,
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
) -> SpawnedNodes {
    let mut needed = vec![false; graph.nodes.len()];
    let hosts = graph.meshes.iter().flatten().map(|local| local.node).chain(graph.collision_nodes.iter().flatten().copied());
    for host in hosts {
        let mut node = Some(host);
        while let Some(index) = node {
            if needed[index] {
                break;
            }
            needed[index] = true;
            node = graph.nodes[index].parent;
        }
    }
    // Parents before children: a NIF's node indices run depth-first, but resolve
    // by recursion so an out-of-order file still works.
    let mut entities: HashMap<usize, EntityId> = HashMap::new();
    let mut worlds: HashMap<usize, (Vec3, Quat, f32)> = HashMap::new();
    #[allow(clippy::too_many_arguments)] // Recursive parent-before-child resolver threading the shared maps and the spawn context.
    fn make(
        index: usize,
        world: &mut World,
        graph: &GraphNodes,
        placement_root: EntityId,
        root_world: (Vec3, Quat, f32),
        entities: &mut HashMap<usize, EntityId>,
        worlds: &mut HashMap<usize, (Vec3, Quat, f32)>,
    ) -> Option<EntityId> {
        if let Some(&entity) = entities.get(&index) {
            return Some(entity);
        }
        let node = &graph.nodes[index];
        // The root node is the placement root itself; nodes under it start there.
        let (parent_entity, parent_world) = match node.parent {
            Some(parent) if graph.nodes[parent].parent.is_some() => {
                let entity = make(parent, world, graph, placement_root, root_world, entities, worlds)?;
                (entity, worlds[&parent])
            }
            _ => (placement_root, root_world),
        };
        let (pos, rot, scale) = GlobalTransform::compose_trs(
            parent_world.0,
            parent_world.1,
            parent_world.2,
            node.translation,
            node.rotation,
            node.scale,
        );
        let entity = world.spawn();
        world.insert(entity, Transform::new(node.translation, node.rotation, node.scale));
        world.insert(entity, GlobalTransform::new(pos, rot, scale));
        world.insert(entity, Parent(parent_entity));
        crate::helpers::add_child(world, parent_entity, entity);
        if let Some(name) = &node.name {
            let symbol = world.resource_mut::<StringPool>().intern(name);
            world.insert(entity, Name(symbol));
        }
        entities.insert(index, entity);
        worlds.insert(index, (pos, rot, scale));
        Some(entity)
    }
    for (index, is_needed) in needed.iter().enumerate() {
        if *is_needed && graph.nodes[index].parent.is_some() {
            make(
                index,
                world,
                graph,
                placement_root,
                (ref_pos, ref_rot, ref_scale),
                &mut entities,
                &mut worlds,
            );
        }
    }
    SpawnedNodes { entities }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::Children;

    fn node(name: &str, parent: Option<usize>, translation: Vec3) -> GraphNode {
        GraphNode { name: Some(name.into()), parent, translation, rotation: Quat::IDENTITY, scale: 1.0 }
    }

    /// root -> RuinsLever -> Lever01, plus a mesh directly under the root, one
    /// under RuinsLever and one under Lever01.
    fn lever() -> GraphNodes {
        let local = |node, translation: Vec3| Some(MeshNodeLocal { node, translation, rotation: Quat::IDENTITY, scale: 1.0 });
        GraphNodes {
            nodes: vec![
                node("NorLever01", None, Vec3::ZERO),
                node("RuinsLever", Some(0), Vec3::new(1.0, 0.0, 0.0)),
                node("Lever01", Some(1), Vec3::new(0.0, 2.0, 0.0)),
            ],
            collision_nodes: Vec::new(),
            meshes: vec![None, local(1, Vec3::ZERO), local(2, Vec3::new(0.0, 0.0, 3.0))],
        }
    }

    fn world() -> World {
        let mut world = World::new();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<Parent>();
        world.register::<Children>();
        world.register::<Name>();
        world.insert_resource(StringPool::new());
        world
    }

    #[test]
    fn nodes_under_the_root_become_a_named_entity_chain_under_the_placement_root() {
        let mut world = world();
        let root = world.spawn();
        let graph = lever();
        let spawned = spawn_nodes(&mut world, &graph, root, Vec3::new(100.0, 0.0, 0.0), Quat::IDENTITY, 1.0);

        // A mesh straight under the root keeps the flat spawn.
        assert!(spawned.mesh_parent(&graph, 0).is_none());
        let (ruins, local) = spawned.mesh_parent(&graph, 1).expect("hangs from RuinsLever");
        assert_eq!(local.node, 1);
        let (lever, local) = spawned.mesh_parent(&graph, 2).expect("hangs from Lever01");
        assert_eq!(local.translation, Vec3::new(0.0, 0.0, 3.0));

        // RuinsLever hangs from the placement root, Lever01 from RuinsLever.
        assert_eq!(world.get::<Parent>(ruins).unwrap().0, root);
        assert_eq!(world.get::<Parent>(lever).unwrap().0, ruins);
        assert_eq!(world.get::<Transform>(lever).unwrap().translation, Vec3::new(0.0, 2.0, 0.0));
        // The seeded world pose composes the placement and both node offsets.
        assert_eq!(world.get::<GlobalTransform>(lever).unwrap().translation, Vec3::new(101.0, 2.0, 0.0));
        // The animation finds the bone by its NIF name.
        let name = world.get::<Name>(lever).unwrap().0;
        assert_eq!(name, world.resource_mut::<StringPool>().intern("Lever01"));
    }

    /// #5932 — a body on the root node pairs with no node, so the flat spawn keeps
    /// it (the root is never spawned as a node entity); a child body at the root's
    /// identity pose pairs with the child, not the root.
    #[test]
    fn a_root_hosted_body_stays_on_the_flat_spawn() {
        use byroredux_core::ecs::components::collision::{CollisionShape, RigidBodyData};
        use byroredux_nif::import::{ImportedCollision, ImportedNode};
        let imported_node = |parent: Option<usize>, body: bool| ImportedNode {
            name: None,
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: 1.0,
            parent_node: parent,
            collision: body.then(|| (CollisionShape::Ball { radius: 1.0 }, RigidBodyData::static_body())),
            billboard_mode: None,
            tree_bones: None,
            range_kind: None,
            flags: 0,
            bs_value_node: None,
            bs_ordered_node: None,
            lod_group: None,
        };
        let nodes = vec![node("BearTrap01", None, Vec3::ZERO), node("Jaws", Some(0), Vec3::ZERO)];
        let collision = ImportedCollision {
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: 1.0,
            shape: CollisionShape::Ball { radius: 1.0 },
            body: RigidBodyData::static_body(),
        };
        let collisions = std::slice::from_ref(&collision);
        let on_root = [imported_node(None, true), imported_node(Some(0), false)];
        assert_eq!(pair_collisions(&nodes, &on_root, collisions), vec![None], "the root body stays detached");
        let on_child = [imported_node(None, true), imported_node(Some(0), true)];
        assert_eq!(pair_collisions(&nodes, &on_child, collisions), vec![Some(1)], "the child body rides its own node");
    }

    #[test]
    fn only_the_nodes_a_mesh_hangs_from_are_spawned() {
        let mut world = world();
        let root = world.spawn();
        let mut graph = lever();
        graph.meshes = vec![None, None, None];
        let before = world.next_entity_id();
        spawn_nodes(&mut world, &graph, root, Vec3::ZERO, Quat::IDENTITY, 1.0);
        assert_eq!(world.next_entity_id(), before, "no mesh below the root, no node entities");
    }
}
