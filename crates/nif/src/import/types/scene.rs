//! Attach points, furniture markers and the top-level `ImportedScene`.

use super::*;

/// One named attach point harvested from `BSConnectPoint::Parents`
/// extra-data on a NIF's root node. FO4+ weapon-mod attachment graph
/// entry.
///
/// The `parent` field is the skeleton bone the attach point hangs off
/// (empty string for non-skinned anchoring on the host mesh root).
/// `name` is the `CON_xxx` tag the OMOD record / accessory NIF
/// references. Rotation / translation / scale form the local
/// transform relative to the parent bone (or root).
///
/// Coord conversion: the NIF wire format is Z-up, but the importer
/// applies the Z-up → Y-up swap at NIF parse time elsewhere — these
/// values arrive in the importer's downstream Y-up frame so they can
/// be copied 1:1 into the `AttachPoint` ECS component. See #985.
#[derive(Debug, Clone)]
pub struct ImportedAttachPoint {
    pub parent: String,
    pub name: String,
    pub rotation: [f32; 4],
    pub translation: [f32; 3],
    pub scale: f32,
}

/// One furniture entry marker lifted from a `BSFurnitureMarker` block —
/// where an actor sits, sleeps, or leans on the furniture. Maps onto the
/// engine's `Furniture` ECS component at spawn time (M41.5 Phase B). The
/// `offset` is converted to renderer Y-up at extraction (like
/// [`ImportedScene::bs_bound`] / attach points); `heading_z_radians` is
/// kept in Gamebryo source space — see the field docs on
/// `byroredux_core::ecs::components::FurnitureMarker`.
#[derive(Debug, Clone)]
pub struct ImportedFurnitureMarker {
    /// Entry position, renderer Y-up, local to the furniture mesh root.
    pub offset: [f32; 3],
    /// Skyrim+ `Heading` — rotation about Gamebryo +Z in radians. `None`
    /// on Oblivion/FO3/FNV (whose ushort `Orientation` has no verified
    /// radian mapping).
    pub heading_z_radians: Option<f32>,
    /// Skyrim+ `AnimationType` (1 = Sit, 2 = Sleep, 3 = Lean); `0` on the
    /// legacy path (no AnimationType field).
    pub animation_type: u16,
}

/// Child-side companion to [`ImportedAttachPoint`]: the list of
/// attach-point names this NIF connects back to on its parent host,
/// plus the `skinned` flag from `BSConnectPoint::Children`. Maps onto
/// the engine's `ChildAttachConnections` ECS component at spawn time.
/// See #985.
#[derive(Debug, Clone, Default)]
pub struct ImportedChildAttachConnections {
    pub point_names: Vec<String>,
    pub skinned: bool,
}

/// A fully imported NIF scene with hierarchy preserved.
#[derive(Debug, Clone)]
pub struct ImportedScene {
    /// Scene graph nodes (NiNode hierarchy).
    pub nodes: Vec<ImportedNode>,
    /// Leaf geometry meshes.
    pub meshes: Vec<ImportedMesh>,
    /// Parsed particle systems (NiParticleSystem / NiParticles /
    /// NiMeshParticleSystem / NiParticleSystemController). The current
    /// parser keeps `NiPSysBlock` opaque (every numeric field is
    /// discarded — see `crates/nif/src/blocks/particle.rs`), so the
    /// importer only flags presence + the host node index. The scene
    /// builder picks a heuristic [`crate::ParticleEmitter`] preset
    /// (torch_flame / smoke / magic_sparkles) by inspecting the host
    /// NiNode's name. See #401.
    pub particle_emitters: Vec<ImportedParticleEmitter>,
    /// BSXFlags value from the root node's extra data (physics/animation hints).
    /// Bits: 0=animated, 1=havok, 2=ragdoll, 3=complex, 4=addon, 5=editor marker,
    /// 6=dynamic, 7=articulated, 8=needs_transform_updates, 9=external_emit.
    pub bsx_flags: Option<u32>,
    /// BSBound from the root node's extra data (object-level bounding box).
    pub bs_bound: Option<([f32; 3], [f32; 3])>, // (center, half_extents)
    /// `(min, max)` of the scene's `bhkSimpleShapePhantom` volume in
    /// NIF-root Y-up space — the authored water volume of Skyrim's placed
    /// water meshes. See [`crate::import::collision::extract_phantom_bounds`].
    pub phantom_bounds: Option<([f32; 3], [f32; 3])>,
    /// FO4+ `BSConnectPoint::Parents` extra-data — named attach points
    /// this NIF *exposes* for modular accessories to connect to (e.g.
    /// `CON_Magazine`, `CON_Scope` on a 10mm pistol). Maps 1:1 onto the
    /// engine's `AttachPoints` ECS component at spawn time. `None`
    /// when the NIF authored no `BSConnectPoint::Parents` block —
    /// almost everything except modular FO4 weapons / armor.
    /// See #985 / NIF-D5-ORPHAN-A3.
    pub attach_points: Option<Vec<ImportedAttachPoint>>,
    /// FO4+ `BSConnectPoint::Children` extra-data — named attach points
    /// this NIF *connects back to* on its parent host (e.g. a reflex
    /// sight referencing `CON_Scope` on the pistol). Maps 1:1 onto the
    /// engine's `ChildAttachConnections` ECS component at spawn time.
    /// `None` for non-accessory NIFs.
    pub child_attach_connections: Option<ImportedChildAttachConnections>,
    /// Sit / sleep / lean entry markers from a `BSFurnitureMarker` block
    /// (chairs, benches, beds, wall-lean spots, workbenches). Empty for
    /// the overwhelming majority of meshes (non-furniture). Offsets are
    /// renderer Y-up; maps onto the `Furniture` ECS component at spawn.
    /// See M41.5 Phase B / [`ImportedFurnitureMarker`].
    pub furniture_markers: Vec<ImportedFurnitureMarker>,
    /// Ambient animation clip collecting every mesh-embedded controller
    /// (alpha fade, UV scroll, visibility flicker, material color
    /// pulse, shader float/color). Populated by
    /// [`crate::anim::import_embedded_animations`] during scene import.
    /// `None` when the NIF authored no such controllers — most
    /// non-FX/non-water meshes. See #261.
    pub embedded_clip: Option<crate::anim::AnimationClip>,
    /// Havok ragdoll articulation (rigid bodies + the constraints linking
    /// them) extracted from the classic `BhkRigidBody` chain, when the
    /// scene carries a real one (≥2 bodies + ≥1 decoded joint). `None`
    /// for non-skeletal NIFs and for FO4+ NP-blob ragdolls (not yet
    /// decodable). Consumed by the engine to build a Rapier multibody and
    /// drive Bethesda ragdolls on our own solver (M41.x).
    pub ragdoll: Option<ImportedRagdoll>,
    /// Authored `NiPointLight` / `NiSpotLight` / `NiAmbientLight` /
    /// `NiDirectionalLight` blocks, extracted via [`crate::import::import_nif_lights`].
    /// Empty unless the caller explicitly populates it — `import_nif_scene*`
    /// does NOT call `import_nif_lights` itself (each of this crate's three
    /// existing consumers has its own reason to call it separately, at a
    /// point where it still holds the raw `NifScene` borrow this field's
    /// producer needs). This field exists so a caller that already computed
    /// the lights at parse time (see `byroredux::scene::nif_loader::
    /// parse_import_and_merge`) has somewhere to stash them for a caching
    /// layer that only retains `ImportedScene`, not the raw scene. See
    /// #2530 / NIFAL-D3-NEW-01.
    pub lights: Vec<ImportedLight>,
}

impl ImportedScene {
    /// Axis-aligned envelope of every finite mesh vertex, composed through
    /// each mesh's parent-node chain into the space the scene's root node is
    /// spawned into (renderer Y-up).
    ///
    /// The per-mesh bounding spheres only *enclose* geometry, so a floor
    /// placed at a sphere's lowest point leaves the asset hovering; this is
    /// the tight box a host needs to stand an asset on a surface. Skinned
    /// meshes contribute their bind-pose positions. `None` when no mesh has
    /// a finite vertex.
    pub fn geometry_bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let nodes: Vec<SceneTransform> = self
            .nodes
            .iter()
            .map(|node| SceneTransform {
                translation: node.translation,
                rotation: node.rotation,
                scale: node.scale,
                parent: node.parent_node,
            })
            .collect();
        compose_geometry_bounds(
            &nodes,
            self.meshes.iter().map(|mesh| {
                (
                    SceneTransform {
                        translation: mesh.translation,
                        rotation: mesh.rotation,
                        scale: mesh.scale,
                        parent: mesh.parent_node,
                    },
                    mesh.positions.as_slice(),
                )
            }),
        )
    }
}

/// Local TRS of one node or mesh plus its parent-node index.
#[derive(Debug, Clone, Copy)]
struct SceneTransform {
    translation: [f32; 3],
    rotation: [f32; 4],
    scale: f32,
    parent: Option<usize>,
}

/// Uniform-scale similarity transform, the shape every NIF node carries.
#[derive(Debug, Clone, Copy)]
struct Similarity {
    translation: byroredux_core::math::Vec3,
    rotation: byroredux_core::math::Quat,
    scale: f32,
}

impl Similarity {
    const IDENTITY: Self = Self {
        translation: byroredux_core::math::Vec3::ZERO,
        rotation: byroredux_core::math::Quat::IDENTITY,
        scale: 1.0,
    };

    fn local(transform: &SceneTransform) -> Self {
        let rotation = byroredux_core::math::Quat::from_array(transform.rotation);
        Self {
            translation: byroredux_core::math::Vec3::from_array(transform.translation),
            rotation: if rotation.length_squared() > 1e-12 {
                rotation.normalize()
            } else {
                byroredux_core::math::Quat::IDENTITY
            },
            scale: transform.scale,
        }
    }

    fn then(self, child: Self) -> Self {
        Self {
            translation: self.translation + self.rotation * (child.translation * self.scale),
            rotation: self.rotation * child.rotation,
            scale: self.scale * child.scale,
        }
    }

    fn apply(self, point: byroredux_core::math::Vec3) -> byroredux_core::math::Vec3 {
        self.translation + self.rotation * (point * self.scale)
    }
}

fn compose_geometry_bounds<'a>(
    nodes: &[SceneTransform],
    meshes: impl Iterator<Item = (SceneTransform, &'a [[f32; 3]])>,
) -> Option<([f32; 3], [f32; 3])> {
    use byroredux_core::math::Vec3;
    let mut resolved: Vec<Option<Similarity>> = vec![None; nodes.len()];
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    let mut found = false;
    for (mesh, positions) in meshes {
        let parent = match mesh.parent {
            Some(index) => match node_to_root(nodes, index, &mut resolved) {
                Some(parent) => parent,
                None => continue,
            },
            None => Similarity::IDENTITY,
        };
        let transform = parent.then(Similarity::local(&mesh));
        for position in positions {
            let point = transform.apply(Vec3::from_array(*position));
            if point.is_finite() {
                min = min.min(point);
                max = max.max(point);
                found = true;
            }
        }
    }
    found.then(|| (min.to_array(), max.to_array()))
}

/// Root-space transform of node `index`, memoised in `resolved`. `None` for
/// an out-of-range index or a parent cycle.
fn node_to_root(
    nodes: &[SceneTransform],
    index: usize,
    resolved: &mut [Option<Similarity>],
) -> Option<Similarity> {
    let mut chain = Vec::new();
    let mut cursor = Some(index);
    let mut base = Similarity::IDENTITY;
    while let Some(current) = cursor {
        if let Some(done) = *resolved.get(current)? {
            base = done;
            break;
        }
        if chain.len() > nodes.len() {
            return None;
        }
        chain.push(current);
        cursor = nodes[current].parent;
    }
    for &node in chain.iter().rev() {
        base = base.then(Similarity::local(&nodes[node]));
        resolved[node] = Some(base);
    }
    Some(base)
}

#[cfg(test)]
mod geometry_bounds_tests {
    use super::*;

    fn transform(
        translation: [f32; 3],
        rotation: [f32; 4],
        scale: f32,
        parent: Option<usize>,
    ) -> SceneTransform {
        SceneTransform {
            translation,
            rotation,
            scale,
            parent,
        }
    }

    #[test]
    fn mesh_vertices_compose_through_the_parent_chain() {
        let quarter_turn_y = [
            0.0,
            std::f32::consts::FRAC_1_SQRT_2,
            0.0,
            std::f32::consts::FRAC_1_SQRT_2,
        ];
        let nodes = [
            transform([0.0, 10.0, 0.0], [0.0, 0.0, 0.0, 1.0], 2.0, None),
            transform([1.0, 0.0, 0.0], quarter_turn_y, 1.0, Some(0)),
        ];
        let positions = [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0]];
        let mesh = transform([0.0; 3], [0.0, 0.0, 0.0, 1.0], 1.0, Some(1));
        let (min, max) =
            compose_geometry_bounds(&nodes, std::iter::once((mesh, positions.as_slice()))).unwrap();
        // Node 1 sits at x = 2 (parent scale 2). The quarter turn about +Y
        // maps local +X to -Z, scaled by 2; local -Y stays -Y, scaled by 2.
        let expect_min = [2.0, 8.0, -2.0];
        let expect_max = [2.0, 10.0, 0.0];
        for axis in 0..3 {
            assert!((min[axis] - expect_min[axis]).abs() < 1e-5, "min {min:?}");
            assert!((max[axis] - expect_max[axis]).abs() < 1e-5, "max {max:?}");
        }
    }

    #[test]
    fn broken_parent_links_and_non_finite_vertices_are_skipped() {
        let nodes = [transform([0.0; 3], [0.0, 0.0, 0.0, 1.0], 1.0, Some(0))];
        let cyclic = transform([0.0; 3], [0.0, 0.0, 0.0, 1.0], 1.0, Some(0));
        let dangling = transform([0.0; 3], [0.0, 0.0, 0.0, 1.0], 1.0, Some(7));
        let rooted = transform([0.0; 3], [0.0, 0.0, 0.0, 1.0], 1.0, None);
        let finite = [[1.0, 2.0, 3.0]];
        let nan = [[f32::NAN, 0.0, 0.0]];
        let bounds = compose_geometry_bounds(
            &nodes,
            [
                (cyclic, finite.as_slice()),
                (dangling, finite.as_slice()),
                (rooted, nan.as_slice()),
            ]
            .into_iter(),
        );
        assert!(bounds.is_none());
        let bounds = compose_geometry_bounds(&nodes, std::iter::once((rooted, finite.as_slice())));
        assert_eq!(bounds, Some(([1.0, 2.0, 3.0], [1.0, 2.0, 3.0])));
    }
}
