//! NIF-to-ECS import — converts a parsed NifScene into meshes and nodes.
//!
//! Walks the NiNode scene graph tree, preserving hierarchy as `ImportedNode`
//! entries with parent indices. Produces `ImportedMesh` per geometry leaf and
//! `ImportedNode` per NiNode. Transforms are local (relative to parent).
//!
//! The output is GPU-agnostic: `ImportedMesh` contains plain `Vec<Vertex>`
//! and `Vec<u32>` data ready for upload via `MeshRegistry::upload()`.

pub mod collision;
mod coord;
mod material;
mod mesh;
mod transform;
mod walk;

// Re-export the public material capture types so `ImportedMesh`'s
// `effect_shader` field can name `BsEffectShaderData` without leaking
// the internal module path.
pub use material::{BsEffectShaderData, NoLightingFalloff, ShaderTypeFields};

use crate::scene::NifScene;
use crate::types::NiTransform;
use byroredux_core::ecs::components::collision::{CollisionShape, RigidBodyData};
use std::sync::Arc;

/// One light source extracted from a NIF scene, positioned in world space.
///
/// Populated from NiAmbientLight / NiDirectionalLight / NiPointLight /
/// NiSpotLight blocks during the flat walk. See issue #156.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedLight {
    /// World-space position (Y-up).
    pub translation: [f32; 3],
    /// Unit direction (Y-up) — zero for ambient/point, camera-facing
    /// meaningful only for directional and spot lights.
    pub direction: [f32; 3],
    /// Diffuse RGB in 0..1 (multiplied by dimmer, ignoring alpha).
    pub color: [f32; 3],
    /// Effective radius in Bethesda units, derived from the attenuation
    /// parameters. Zero for ambient/directional (infinite reach).
    pub radius: f32,
    /// Kind tag for the renderer. 0 = ambient, 1 = directional,
    /// 2 = point, 3 = spot.
    pub kind: LightKind,
    /// Outer cone half-angle in radians (0.0 for non-spot).
    pub outer_angle: f32,
    /// Names of the scene-graph nodes this light is restricted to,
    /// resolved from the `NiDynamicEffect.Affected Nodes` Ptr list. An
    /// empty `Vec` means "no restriction" (the light affects every
    /// nearby surface). Skyrim+ FO4 (BSVER >= 130) drops this list at
    /// the wire level, so it's always empty there. Renderer-side
    /// light-target filtering wiring is a separate change — pre-#335
    /// the importer dropped the field entirely. See #335.
    pub affected_node_names: Vec<Arc<str>>,
}

/// Kind of a parsed NIF light.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightKind {
    Ambient,
    Directional,
    Point,
    Spot,
}

/// Collision data extracted from a NiNode, positioned in world space.
///
/// Used by the flat import path to return collision alongside geometry,
/// since the flat path doesn't produce ImportedNode hierarchy.
#[derive(Debug)]
pub struct ImportedCollision {
    /// World-space translation (Y-up).
    pub translation: [f32; 3],
    /// World-space rotation as quaternion [x, y, z, w] (Y-up).
    pub rotation: [f32; 4],
    pub scale: f32,
    pub shape: CollisionShape,
    pub body: RigidBodyData,
}

/// A scene graph node (NiNode) extracted from a NIF file.
#[derive(Debug)]
pub struct ImportedNode {
    /// Node name from the NIF (e.g., "Bip01 Head", "Scene Root").
    /// Uses `Arc<str>` to share the string table entry without heap allocation.
    pub name: Option<Arc<str>>,
    /// Local-space translation (Y-up), relative to parent.
    pub translation: [f32; 3],
    /// Local-space rotation as quaternion [x, y, z, w] (Y-up).
    pub rotation: [f32; 4],
    pub scale: f32,
    /// Index into `ImportedScene.nodes` for this node's parent, or None for root.
    pub parent_node: Option<usize>,
    /// Collision shape and rigid body data (from bhkCollisionObject chain).
    pub collision: Option<(CollisionShape, RigidBodyData)>,
    /// Raw `BillboardMode` value if this node was a `NiBillboardNode`.
    /// `None` for regular NiNode and its non-billboard subclasses.
    /// The consumer maps this to the `Billboard` ECS component. See #225.
    pub billboard_mode: Option<u16>,
    /// SpeedTree bone metadata when this node is a `BSTreeNode` —
    /// `(branch_root_bone_names, trunk_bone_names)` resolved via
    /// each `BlockRef`'s `NiObjectNET.name`. Pre-#363 the parser
    /// kept the lists but the walker stripped them down to plain
    /// NiNode, blocking any future SpeedTree wind / bend simulation
    /// from finding the bones it would animate. The geometry renders
    /// correctly today via the regular `NiNode.children` path; this
    /// field exists purely so downstream consumers can branch on it.
    /// Mirrors the [`ImportedLight::affected_node_names`] resolution
    /// pattern from #335 — names are looked up by the scene builder
    /// against its `node_by_name` index. See audit S4-05.
    pub tree_bones: Option<TreeBones>,
    /// Wire-type discriminator when this node is a `BSRangeNode` /
    /// `BSDamageStage` / `BSBlastNode` / `BSDebrisNode`. Pre-#364 all
    /// four collapsed into a single `BsRangeNode` with no surviving
    /// discriminator; gameplay-side systems (destructible-object
    /// switching, blast-effect spawning, debris ejection) couldn't
    /// tell them apart. `None` when the source block was a plain
    /// `NiNode` or one of its non-range subclasses. See audit S4-06.
    pub range_kind: Option<BsRangeKind>,
    /// Raw `NiAVObject.flags` value. Carried through so the scene
    /// builder can spawn a `SceneFlags` component per entity. APP_CULLED
    /// (bit 0) is already consumed by the import-time visibility filter
    /// in `walk.rs`; this field preserves every other bit
    /// (SELECTIVE_UPDATE, DISABLE_SORTING, IS_NODE, DISPLAY_OBJECT,
    /// etc.) so gameplay-side systems can branch on them instead of
    /// re-reading the source NIF. See #222.
    pub flags: u32,
}

/// SpeedTree bone metadata surfaced from a [`BSTreeNode`] — bone
/// references resolved to scene-graph node names. The SpeedTree tool
/// labels the lists as "branch roots" and "trunk bones"; the future
/// SpeedTree wind / bend simulation animates the associated entities
/// under wind loads. The scene builder resolves names → entities via
/// its `node_by_name` index (same pattern as
/// [`ImportedLight::affected_node_names`] from #335).
/// See audit S4-05 / #363.
#[derive(Debug, Clone, Default)]
pub struct TreeBones {
    /// Branch-root bone names — the entities the wind sim swings the
    /// outer canopy from. Null refs and refs that don't resolve to a
    /// named NiObjectNET-bearing block are dropped silently.
    pub branch_roots: Vec<Arc<str>>,
    /// Trunk bone names — the entities the wind sim bends the trunk
    /// across. Same drop rules as `branch_roots`.
    pub trunk: Vec<Arc<str>>,
}

/// Re-export the `BsRangeKind` discriminator from the parser side so
/// callers downstream of `ImportedNode` don't have to reach into
/// `crate::blocks::node` directly.
pub use crate::blocks::node::BsRangeKind;

/// A mesh extracted from a NIF file, ready for GPU upload.
#[derive(Debug)]
pub struct ImportedMesh {
    /// Vertices in renderer format: position + color + normal + UV.
    pub positions: Vec<[f32; 3]>,
    /// Vertex colors (RGB). Falls back to material diffuse or white.
    pub colors: Vec<[f32; 3]>,
    /// Vertex normals. Falls back to +Y up if the mesh has no normals.
    pub normals: Vec<[f32; 3]>,
    /// UV coordinates. Empty if the mesh has no UVs.
    pub uvs: Vec<[f32; 2]>,
    /// Triangle indices (u32 for Vulkan compatibility).
    pub indices: Vec<u32>,
    /// Local-space translation (Y-up), relative to parent node.
    pub translation: [f32; 3],
    /// Local-space rotation as quaternion [x, y, z, w] (Y-up).
    pub rotation: [f32; 4],
    pub scale: f32,
    /// Texture file path (if a base texture was found in BSShaderTextureSet).
    pub texture_path: Option<String>,
    /// BGSM/BGEM material file path (FO4+). When present and texture_path is
    /// None, the real texture paths live inside this .bgsm file in the
    /// Materials BA2. Stored for debug diagnostics and future BGSM parsing.
    pub material_path: Option<String>,
    /// Node name from the NIF. Uses `Arc<str>` to avoid heap copies from the string table.
    pub name: Option<Arc<str>>,
    /// Whether this mesh uses alpha blending (from NiAlphaProperty bit 0).
    pub has_alpha: bool,
    /// Source blend factor from NiAlphaProperty flags bits 1–4.
    /// Gamebryo AlphaFunction: 0=ONE, 6=SRC_ALPHA (default), etc.
    pub src_blend_mode: u8,
    /// Destination blend factor from NiAlphaProperty flags bits 5–8.
    /// Gamebryo AlphaFunction: 0=ONE, 7=INV_SRC_ALPHA (default), etc.
    pub dst_blend_mode: u8,
    /// Whether this mesh uses alpha testing / cutout rendering
    /// (NiAlphaProperty bit 9 / mask 0x200). When `true`, the renderer
    /// should render opaque but `discard` fragments whose sampled
    /// texture alpha is below `alpha_threshold`. Mutually exclusive
    /// with `has_alpha` — the importer prefers alpha-test when both
    /// bits are set on the source material. See issue #152.
    pub alpha_test: bool,
    /// Alpha-test cutoff threshold in [0, 1] (NiAlphaProperty.threshold
    /// divided by 255). Only meaningful when `alpha_test` is `true`.
    pub alpha_threshold: f32,
    /// Alpha test comparison function from NiAlphaProperty flags bits 10–12.
    /// 0=ALWAYS, 1=LESS, 2=EQUAL, 3=LESSEQUAL, 4=GREATER, 5=NOTEQUAL,
    /// 6=GREATEREQUAL (default), 7=NEVER.
    pub alpha_test_func: u8,
    /// Whether this mesh should be rendered two-sided (no backface culling).
    pub two_sided: bool,
    /// Whether this mesh is a decal (should render on top of coplanar surfaces).
    pub is_decal: bool,
    /// Normal map texture path (if found in shader texture set).
    pub normal_map: Option<String>,
    /// Glow / self-illumination texture (NiTexturingProperty slot 4,
    /// Oblivion/FO3/FNV). Separate from the BSShaderTextureSet glow
    /// slot, which the Skyrim+ path pulls directly. See #214.
    pub glow_map: Option<String>,
    /// Detail overlay texture (NiTexturingProperty slot 2). See #214.
    pub detail_map: Option<String>,
    /// Specular-mask / gloss texture (NiTexturingProperty slot 3).
    /// Per-texel specular strength. See #214.
    pub gloss_map: Option<String>,
    /// Dark / multiplicative lightmap (NiTexturingProperty slot 1).
    /// Baked shadow modulation for Oblivion interior architecture. #264.
    pub dark_map: Option<String>,
    /// Parallax / height texture (BSShaderTextureSet slot 3). FO3/FNV
    /// `shader_type = 3` / `7` surfaces plus Skyrim ParallaxOcc /
    /// MultiLayerParallax materials route through this. See #452.
    pub parallax_map: Option<String>,
    /// Environment cubemap (BSShaderTextureSet slot 4). Paired with
    /// `env_map_scale` — glass, polished metal, power armor. See #452.
    pub env_map: Option<String>,
    /// Environment-reflection mask (BSShaderTextureSet slot 5). #452.
    pub env_mask: Option<String>,
    /// Parallax-occlusion max ray-march passes (from
    /// `BSShaderPPLightingProperty` or Skyrim `ShaderTypeData::ParallaxOcc`).
    /// `None` when the material doesn't author a value. See #452.
    pub parallax_max_passes: Option<f32>,
    /// Parallax-occlusion height scale. See #452.
    pub parallax_height_scale: Option<f32>,
    /// Vertex-color source mode from `NiVertexColorProperty`
    /// (`vertex_mode`). Values match Gamebryo's `SourceMode` enum:
    /// `0 = Ignore`, `1 = Emissive`, `2 = AmbientDiffuse` (default).
    /// The importer already handles `Ignore` by not populating the
    /// vertex-color vec; the value is forwarded so the material
    /// system can route `Emissive` to self-illumination later.
    /// See #214.
    pub vertex_color_mode: u8,
    /// Emissive color (RGB, linear).
    pub emissive_color: [f32; 3],
    /// Emissive intensity multiplier.
    pub emissive_mult: f32,
    /// Specular highlight color (RGB, linear).
    pub specular_color: [f32; 3],
    /// Specular intensity multiplier.
    pub specular_strength: f32,
    /// Glossiness / smoothness.
    pub glossiness: f32,
    /// UV texture coordinate offset [u, v].
    pub uv_offset: [f32; 2],
    /// UV texture coordinate scale [u, v].
    pub uv_scale: [f32; 2],
    /// Material alpha/transparency.
    pub mat_alpha: f32,
    /// Environment map reflection scale (from shader type 1).
    pub env_map_scale: f32,
    /// Index into `ImportedScene.nodes` for this mesh's parent node, or None.
    pub parent_node: Option<usize>,
    /// Skeletal skinning data. `None` for rigid meshes.
    pub skin: Option<ImportedSkin>,
    /// Depth test enabled (from NiZBufferProperty). Default: true.
    pub z_test: bool,
    /// Depth write enabled (from NiZBufferProperty). Default: true.
    pub z_write: bool,
    /// Depth comparison function (Gamebryo `TestFunction` enum from
    /// `NiZBufferProperty.z_function`). Default 3 (LESSEQUAL). See
    /// `MaterialInfo::z_function` for the enum values + #398.
    pub z_function: u8,
    /// Mesh-local bounding sphere center in Y-up renderer space. Extracted
    /// from `NiTriShapeData.center` / `BsTriShape.center` when present, or
    /// computed from the vertex positions when the NIF bound is zero.
    /// Consumers emit this as a `LocalBound` ECS component; the bound
    /// propagation system composes it with `GlobalTransform` to produce
    /// a world-space `WorldBound`. See #217.
    pub local_bound_center: [f32; 3],
    /// Mesh-local bounding sphere radius in Y-up renderer space.
    /// Expressed in the mesh's own local units — the propagation system
    /// multiplies by the world scale to produce the world-space radius.
    pub local_bound_radius: f32,
    /// Skyrim+ effect-shader (`BSEffectShaderProperty`) rich material
    /// data — soft-falloff cone, greyscale palette, FO4+/FO76 companion
    /// textures, lighting influence. `None` for non-effect materials
    /// (the common case for static / clutter / actor meshes).
    ///
    /// Captured by the importer so the renderer-side effect-shader
    /// dispatch (SK-D3-02) can route it to a dedicated VFX pipeline
    /// without re-reading the NIF. Until that lands, this rides along
    /// unused — the audit's "VISUAL: soft falloff edge visible" check
    /// can only be satisfied once the renderer hookup is in. See #345.
    pub effect_shader: Option<BsEffectShaderData>,
    /// Raw `BSLightingShaderProperty.shader_type` enum value (0–19),
    /// captured for the renderer-side variant dispatch in
    /// `triangle.frag`. 0 = Default lit (the safe fall-through, also
    /// emitted for non-Skyrim+ meshes that have no
    /// BSLightingShaderProperty backing). Surfacing this on
    /// `ImportedMesh` is the data side of #344 — pre-fix the importer
    /// captured `material_kind` on the internal `MaterialInfo` but
    /// dropped it on the way out, so the renderer had no way to
    /// branch on SkinTint / HairTint / EyeEnvmap / SparkleSnow /
    /// MultiLayerParallax. Variant rendering wiring inside the
    /// fragment shader is per-variant follow-up work.
    pub material_kind: u8,
    /// Raw `NiAVObject.flags` value (sibling of `ImportedNode.flags`).
    /// Consumers emit a `SceneFlags` component per shape entity. See #222.
    pub flags: u32,
    /// Shader-type-specific trailing fields decoded off
    /// `BSLightingShaderProperty.shader_type_data` — SkinTint color,
    /// HairTint color, EyeEnvmap centers, ParallaxOcc / MultiLayerParallax
    /// depth parameters, SparkleSnow packed rgba. Every variant is
    /// capture-ready here; renderer-side consumption happens as each
    /// `material_kind` branch is wired into `triangle.frag`. Before #430
    /// these fields were populated on `MaterialInfo` (NiTriShape path) but
    /// dropped in the construction of `ImportedMesh`, and the BsTriShape
    /// path ignored them entirely — both sides now populate uniformly.
    pub shader_type_fields: ShaderTypeFields,
    /// FO3/FNV `BSShaderNoLightingProperty` soft-falloff cone —
    /// four scalars that drive the angular alpha gradient on HUD
    /// overlays, VATS crosshair, scope reticles, Pip-Boy glow, and
    /// heat-shimmer planes. `None` for every non-NoLighting mesh.
    /// Renderer dispatch is follow-up work (same track as the
    /// BSEffectShaderProperty soft-falloff consumption). Pre-#451
    /// the parser captured these but the importer dropped them.
    pub no_lighting_falloff: Option<NoLightingFalloff>,
}

/// Per-bone binding for a skinned mesh. Bone space is Y-up (converted
/// from Gamebryo Z-up on import).
#[derive(Debug, Clone)]
pub struct ImportedBone {
    /// Name of the bone's scene-graph node (e.g. "Bip01 Spine"). The
    /// consumer looks up the matching entity in the skeleton.
    pub name: Arc<str>,
    /// Mesh-space → bone-space transform at bind time, stored as a
    /// 4x4 matrix. Multiply by the bone's current world-space transform
    /// during skinning (matrix palette skinning).
    ///
    /// Packed column-major per glam convention.
    pub bind_inverse: [[f32; 4]; 4],
    /// Bounding sphere in bone space (center xyz + radius).
    pub bounding_sphere: [f32; 4],
}

/// Skinning data attached to an `ImportedMesh`. Up to 4 bone influences
/// per vertex (the hardware-standard palette).
///
/// For legacy NiTriShape meshes, per-vertex weights are computed from
/// `NiSkinData`'s sparse per-bone weight lists by keeping the 4 highest
/// weights per vertex and re-normalizing to sum to 1. For modern
/// BSTriShape meshes the weights live inside the packed vertex buffer
/// (VF_SKINNED) — currently not extracted, so those fields will be
/// empty and the consumer should fall back to the BSTriShape vertex
/// buffer directly. See follow-up issue.
#[derive(Debug, Clone, Default)]
pub struct ImportedSkin {
    /// Bones this mesh binds to, in the order the interpolator refers
    /// to them by index.
    pub bones: Vec<ImportedBone>,
    /// Skeleton root bone name, if identifiable. The animation system
    /// uses this to know where to start applying joint transforms.
    pub skeleton_root: Option<Arc<str>>,
    /// Per-vertex bone indices (up to 4). Parallel to `ImportedMesh::positions`.
    /// Empty if weights come from a separate source (BSTriShape).
    pub vertex_bone_indices: Vec<[u8; 4]>,
    /// Per-vertex bone weights (up to 4). Must sum to 1.0 after
    /// normalization. Parallel to `vertex_bone_indices`.
    pub vertex_bone_weights: Vec<[f32; 4]>,
}

/// A fully imported NIF scene with hierarchy preserved.
#[derive(Debug)]
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
}

/// One particle emitter discovered while walking the NIF scene graph.
/// See [`ImportedScene::particle_emitters`] for the full picture.
#[derive(Debug, Clone)]
pub struct ImportedParticleEmitter {
    /// Index into [`ImportedScene::nodes`] for the host scene-graph
    /// node — the scene builder anchors the spawned ECS emitter entity
    /// at this node so the particles inherit its world position.
    pub parent_node: Option<usize>,
    /// Original NIF block type name — `"NiParticleSystem"`,
    /// `"NiParticles"`, `"NiMeshParticleSystem"`,
    /// `"NiParticleSystemController"`, etc. The scene builder reads
    /// this only for telemetry; the heuristic preset is driven off the
    /// host node's name (`torch` / `fire` → flame, `smoke` → smoke,
    /// `magic` / `enchant` → sparkles, fallback → flame).
    pub original_type: String,
}

/// Flat-import variant of [`ImportedParticleEmitter`] used by the cell
/// loader, which doesn't reconstruct the NIF hierarchy. Carries the
/// emitter's NIF-local position (composed up to the host node), the
/// nearest named ancestor's name (for heuristic preset selection), and
/// the original NIF block type name. The cell loader composes
/// `local_position` with the REFR placement transform to land the
/// spawned emitter entity at the correct world position. See #401.
#[derive(Debug, Clone)]
pub struct ImportedParticleEmitterFlat {
    /// Y-up local position of the emitter inside its source NIF.
    pub local_position: [f32; 3],
    /// Nearest named ancestor's name in the NIF hierarchy. Used by the
    /// heuristic preset selector (`torch`/`fire`/`flame`/`brazier`/
    /// `candle` → flame, `smoke`/`steam` → smoke, `magic`/`enchant`/
    /// `sparkle`/`glow` → sparkles, fallback → flame).
    pub host_name: Option<std::sync::Arc<str>>,
    /// Original NIF block type name — used only for telemetry.
    pub original_type: String,
}

/// Walk a parsed NIF scene flat and return every renderable particle
/// emitter (`NiParticleSystem` and friends), with NIF-local positions
/// and the nearest named ancestor's name. See #401.
pub fn import_nif_particle_emitters(scene: &NifScene) -> Vec<ImportedParticleEmitterFlat> {
    let mut out = Vec::new();
    let Some(root_idx) = scene.root_index else {
        return out;
    };
    walk::walk_node_particle_emitters_flat(scene, root_idx, &NiTransform::default(), None, &mut out);
    out
}

/// Import all renderable meshes from a parsed NIF scene, preserving hierarchy.
///
/// Returns an `ImportedScene` with nodes (NiNode hierarchy) and meshes (geometry leaves).
/// Transforms are local-space (relative to parent). Use the parent indices
/// to rebuild the hierarchy in the ECS.
pub fn import_nif_scene(scene: &NifScene) -> ImportedScene {
    let mut imported = ImportedScene {
        nodes: Vec::new(),
        meshes: Vec::new(),
        particle_emitters: Vec::new(),
        bsx_flags: None,
        bs_bound: None,
    };

    // A truncated scene means at least one block was lost to a mid-parse
    // abort. The root NiNode heuristic may pick a sibling subtree
    // instead of the real root, and block refs inside descendant nodes
    // may dangle. Surface this to any caller (cell_loader, etc.) so the
    // partial import isn't silently accepted as complete. See #393.
    if scene.truncated {
        log::warn!(
            "importing truncated NIF scene — {} blocks dropped; root/refs may be incomplete",
            scene.dropped_block_count,
        );
    }

    let Some(root_idx) = scene.root_index else {
        return imported;
    };

    let mut props_stack: Vec<crate::types::BlockRef> = Vec::new();
    walk::walk_node_hierarchical(scene, root_idx, None, &mut props_stack, &mut imported);

    // Resolve extra data from the root node (BSXFlags, BSBound).
    if let Some(root_block) = scene.blocks.get(root_idx) {
        if let Some(node) = root_block
            .as_any()
            .downcast_ref::<crate::blocks::node::NiNode>()
        {
            for &ref_idx in &node.av.net.extra_data_refs {
                // BlockRef::NULL (`u32::MAX`) maps to `None`; non-null
                // refs to `Some(usize)`. Pre-cleanup the code did
                // `if idx < 0` on the raw `u32` (always false), tripping
                // an `unused_comparisons` warning.
                let Some(idx) = ref_idx.index() else { continue };
                if let Some(block) = scene.blocks.get(idx) {
                    if let Some(ed) = block
                        .as_any()
                        .downcast_ref::<crate::blocks::extra_data::NiExtraData>()
                    {
                        if ed.type_name == "BSXFlags" {
                            imported.bsx_flags = ed.integer_value;
                        }
                    }
                    if let Some(bb) = block
                        .as_any()
                        .downcast_ref::<crate::blocks::extra_data::BsBound>()
                    {
                        imported.bs_bound = Some((bb.center, bb.dimensions));
                    }
                }
            }
        }
    }

    imported
}

/// Backward-compatible flat import (used by cell loader where hierarchy is unnecessary).
///
/// Returns one `ImportedMesh` per NiTriShape with world-space transforms
/// (parent chain composed). Meshes have `parent_node: None`.
pub fn import_nif(scene: &NifScene) -> Vec<ImportedMesh> {
    let mut meshes = Vec::new();

    let Some(root_idx) = scene.root_index else {
        return meshes;
    };

    let mut props_stack: Vec<crate::types::BlockRef> = Vec::new();
    walk::walk_node_flat(
        scene,
        root_idx,
        &NiTransform::default(),
        &mut props_stack,
        &mut meshes,
        None,
    );
    meshes
}

/// Walk a parsed NIF scene and extract every NiLight subclass as an
/// `ImportedLight`, positioned in world space (Y-up).
///
/// This is an independent pass from `import_nif` — callers that care
/// about lights (currently: the cell loader) run it alongside the
/// mesh import. See issue #156.
/// Extract BSXFlags from the root node's extra data. Returns 0 if absent.
/// Bit 5 (0x20) = editor marker — the NIF should not be rendered.
pub fn extract_bsx_flags(scene: &NifScene) -> u32 {
    let Some(root_idx) = scene.root_index else {
        return 0;
    };
    let Some(root_block) = scene.blocks.get(root_idx) else {
        return 0;
    };
    let Some(node) = root_block
        .as_any()
        .downcast_ref::<crate::blocks::node::NiNode>()
    else {
        return 0;
    };
    for &ref_idx in &node.av.net.extra_data_refs {
        // BlockRef::NULL (`u32::MAX`) → `None`; non-null → `Some(usize)`.
        let Some(idx) = ref_idx.index() else { continue };
        if let Some(block) = scene.blocks.get(idx) {
            if let Some(ed) = block
                .as_any()
                .downcast_ref::<crate::blocks::extra_data::NiExtraData>()
            {
                if ed.type_name == "BSXFlags" {
                    return ed.integer_value.unwrap_or(0);
                }
            }
        }
    }
    0
}

pub fn import_nif_lights(scene: &NifScene) -> Vec<ImportedLight> {
    let mut lights = Vec::new();
    let Some(root_idx) = scene.root_index else {
        return lights;
    };
    walk::walk_node_lights(scene, root_idx, &NiTransform::default(), &mut lights);
    lights
}

/// Flat import with collision data.
///
/// Like `import_nif()`, returns world-space meshes (flat, no hierarchy).
/// Additionally extracts collision shapes from NiNodes, returning them
/// in world space alongside the geometry.
pub fn import_nif_with_collision(scene: &NifScene) -> (Vec<ImportedMesh>, Vec<ImportedCollision>) {
    let mut meshes = Vec::new();
    let mut collisions = Vec::new();

    let Some(root_idx) = scene.root_index else {
        return (meshes, collisions);
    };

    let mut props_stack: Vec<crate::types::BlockRef> = Vec::new();
    walk::walk_node_flat(
        scene,
        root_idx,
        &NiTransform::default(),
        &mut props_stack,
        &mut meshes,
        Some(&mut collisions),
    );
    (meshes, collisions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::tri_shape::NiTriShapeData;
    use crate::types::{BlockRef, NiColor, NiMatrix3, NiPoint3, NiTransform};

    /// Helper: build a minimal NifScene with the given blocks.
    fn scene_from_blocks(blocks: Vec<Box<dyn crate::blocks::NiObject>>) -> NifScene {
        let root_index = if blocks.is_empty() { None } else { Some(0) };
        NifScene {
            blocks,
            root_index,
            ..NifScene::default()
        }
    }

    fn identity_transform() -> NiTransform {
        NiTransform::default()
    }

    fn translated(x: f32, y: f32, z: f32) -> NiTransform {
        NiTransform {
            translation: NiPoint3 { x, y, z },
            ..NiTransform::default()
        }
    }

    fn make_tri_shape_data() -> NiTriShapeData {
        NiTriShapeData {
            vertices: vec![
                NiPoint3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                NiPoint3 {
                    x: 1.0,
                    y: 0.0,
                    z: 0.0,
                },
                NiPoint3 {
                    x: 0.0,
                    y: 1.0,
                    z: 0.0,
                },
            ],
            normals: vec![
                NiPoint3 {
                    x: 0.0,
                    y: 0.0,
                    z: 1.0,
                },
                NiPoint3 {
                    x: 0.0,
                    y: 0.0,
                    z: 1.0,
                },
                NiPoint3 {
                    x: 0.0,
                    y: 0.0,
                    z: 1.0,
                },
            ],
            center: NiPoint3 {
                x: 0.33,
                y: 0.33,
                z: 0.0,
            },
            radius: 1.0,
            vertex_colors: Vec::new(),
            uv_sets: vec![vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]],
            triangles: vec![[0, 1, 2]],
        }
    }

    fn make_ni_node(
        transform: NiTransform,
        children: Vec<BlockRef>,
    ) -> crate::blocks::node::NiNode {
        use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
        crate::blocks::node::NiNode {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: Some(std::sync::Arc::from("TestNode")),
                    extra_data_refs: Vec::new(),
                    controller_ref: BlockRef::NULL,
                },
                flags: 0,
                transform,
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            children,
            effects: Vec::new(),
        }
    }

    fn make_ni_tri_shape(
        name: &str,
        transform: NiTransform,
        data_ref: u32,
        properties: Vec<BlockRef>,
    ) -> crate::blocks::tri_shape::NiTriShape {
        use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
        crate::blocks::tri_shape::NiTriShape {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: Some(std::sync::Arc::from(name)),
                    extra_data_refs: Vec::new(),
                    controller_ref: BlockRef::NULL,
                },
                flags: 0,
                transform,
                properties,
                collision_ref: BlockRef::NULL,
            },
            data_ref: BlockRef(data_ref),
            skin_instance_ref: BlockRef::NULL,
            shader_property_ref: BlockRef::NULL,
            alpha_property_ref: BlockRef::NULL,
            num_materials: 0,
            active_material_index: 0,
        }
    }

    #[test]
    fn import_empty_scene() {
        let scene = NifScene::default();
        let meshes = import_nif(&scene);
        assert!(meshes.is_empty());
    }

    #[test]
    fn import_single_shape_under_root() {
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
            Box::new(make_ni_tri_shape(
                "Triangle",
                identity_transform(),
                2,
                Vec::new(),
            )),
            Box::new(make_tri_shape_data()),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        assert_eq!(meshes.len(), 1);
        let m = &meshes[0];
        assert_eq!(m.name, Some(Arc::from("Triangle")));
        assert_eq!(m.positions.len(), 3);
        assert_eq!(m.indices, vec![0, 1, 2]);
        assert_eq!(m.uvs.len(), 3);
        assert_eq!(m.translation, [0.0, 0.0, 0.0]);
        assert_eq!(m.scale, 1.0);
    }

    #[test]
    fn import_inherits_parent_translation() {
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(translated(10.0, 0.0, 0.0), vec![BlockRef(1)])),
            Box::new(make_ni_tri_shape(
                "Mesh",
                identity_transform(),
                2,
                Vec::new(),
            )),
            Box::new(make_tri_shape_data()),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        assert_eq!(meshes.len(), 1);
        let m = &meshes[0];
        assert!((m.translation[0] - 10.0).abs() < 1e-6);
        assert!((m.translation[1]).abs() < 1e-6);
        assert!((m.translation[2]).abs() < 1e-6);
    }

    #[test]
    fn import_composes_nested_transforms() {
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(translated(5.0, 0.0, 0.0), vec![BlockRef(1)])),
            Box::new(make_ni_node(translated(0.0, 3.0, 0.0), vec![BlockRef(2)])),
            Box::new(make_ni_tri_shape(
                "Deep",
                identity_transform(),
                3,
                Vec::new(),
            )),
            Box::new(make_tri_shape_data()),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        assert_eq!(meshes.len(), 1);
        let m = &meshes[0];
        assert!((m.translation[0] - 5.0).abs() < 1e-6);
        assert!((m.translation[1] - 0.0).abs() < 1e-6);
        assert!((m.translation[2] - -3.0).abs() < 1e-6);
    }

    #[test]
    fn import_composes_scale() {
        let root_transform = NiTransform {
            scale: 2.0,
            ..NiTransform::default()
        };
        let shape_transform = NiTransform {
            translation: NiPoint3 {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
            scale: 3.0,
            ..NiTransform::default()
        };
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(root_transform, vec![BlockRef(1)])),
            Box::new(make_ni_tri_shape("Scaled", shape_transform, 2, Vec::new())),
            Box::new(make_tri_shape_data()),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        assert_eq!(meshes.len(), 1);
        let m = &meshes[0];
        assert!((m.scale - 6.0).abs() < 1e-6);
        assert!((m.translation[0] - 2.0).abs() < 1e-6);
    }

    #[test]
    fn import_multiple_shapes() {
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(
                identity_transform(),
                vec![BlockRef(1), BlockRef(3)],
            )),
            Box::new(make_ni_tri_shape(
                "A",
                translated(1.0, 0.0, 0.0),
                2,
                Vec::new(),
            )),
            Box::new(make_tri_shape_data()),
            Box::new(make_ni_tri_shape(
                "B",
                translated(-1.0, 0.0, 0.0),
                4,
                Vec::new(),
            )),
            Box::new(make_tri_shape_data()),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        assert_eq!(meshes.len(), 2);
        assert_eq!(meshes[0].name, Some(Arc::from("A")));
        assert_eq!(meshes[1].name, Some(Arc::from("B")));
    }

    #[test]
    fn import_uses_vertex_colors_when_available() {
        let mut data = make_tri_shape_data();
        data.vertex_colors = vec![
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
        ];

        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
            Box::new(make_ni_tri_shape(
                "Colored",
                identity_transform(),
                2,
                Vec::new(),
            )),
            Box::new(data),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        assert_eq!(meshes[0].colors[0], [1.0, 0.0, 0.0]);
        assert_eq!(meshes[0].colors[1], [0.0, 1.0, 0.0]);
        assert_eq!(meshes[0].colors[2], [0.0, 0.0, 1.0]);
    }

    /// Regression test for issue #131: Oblivion meshes store their
    /// tangent-space normal maps in `NiTexturingProperty.bump_texture`
    /// (the dedicated `normal_texture` slot landed in FO3). The
    /// importer must follow the `bump_texture.source_ref` through
    /// the scene to the referenced `NiSourceTexture.filename` and
    /// populate `ImportedMesh.normal_map`.
    #[test]
    fn import_extracts_oblivion_bump_texture_as_normal_map() {
        use crate::blocks::properties::{NiTexturingProperty, TexDesc};
        use crate::blocks::texture::NiSourceTexture;
        use std::sync::Arc;

        // Block layout:
        //  0: root NiNode
        //  1: NiTriShape referencing data at 2 and property at 3
        //  2: NiTriShapeData
        //  3: NiTexturingProperty with bump_texture → block 4
        //  4: NiSourceTexture for the bump map
        //  5: NiSourceTexture for the base texture (referenced too)
        let tex_prop = NiTexturingProperty {
            net: crate::blocks::base::NiObjectNETData {
                name: None,
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            texture_count: 6,
            base_texture: Some(TexDesc {
                source_ref: BlockRef(5),
                flags: 0,
                transform: None,
            }),
            dark_texture: None,
            detail_texture: None,
            gloss_texture: None,
            glow_texture: None,
            bump_texture: Some(TexDesc {
                source_ref: BlockRef(4),
                flags: 0,
                transform: None,
            }),
            normal_texture: None,
            decal_textures: Vec::new(),
        };
        let bump_src = NiSourceTexture {
            net: crate::blocks::base::NiObjectNETData {
                name: None,
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            use_external: true,
            filename: Some(Arc::from("textures\\architecture\\wall01_n.dds")),
            pixel_data_ref: BlockRef::NULL,
            pixel_layout: 0,
            use_mipmaps: 0,
            alpha_format: 0,
            is_static: true,
        };
        let base_src = NiSourceTexture {
            net: crate::blocks::base::NiObjectNETData {
                name: None,
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            use_external: true,
            filename: Some(Arc::from("textures\\architecture\\wall01.dds")),
            pixel_data_ref: BlockRef::NULL,
            pixel_layout: 0,
            use_mipmaps: 0,
            alpha_format: 0,
            is_static: true,
        };

        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
            Box::new(make_ni_tri_shape(
                "Wall",
                identity_transform(),
                2,
                vec![BlockRef(3)], // property: texturing
            )),
            Box::new(make_tri_shape_data()),
            Box::new(tex_prop),
            Box::new(bump_src),
            Box::new(base_src),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        assert_eq!(meshes.len(), 1);
        let m = &meshes[0];
        assert_eq!(
            m.texture_path.as_deref(),
            Some("textures\\architecture\\wall01.dds"),
            "base_texture should still be extracted"
        );
        assert_eq!(
            m.normal_map.as_deref(),
            Some("textures\\architecture\\wall01_n.dds"),
            "bump_texture slot should populate normal_map for Oblivion meshes"
        );
    }

    /// When both `bump_texture` and `normal_texture` slots are populated
    /// (an FO3/FNV mesh exported by a tool that kept the legacy slot
    /// filled), the importer should prefer `normal_texture` — it's the
    /// dedicated field and more likely to contain the current asset.
    #[test]
    fn import_prefers_normal_texture_over_bump_texture() {
        use crate::blocks::properties::{NiTexturingProperty, TexDesc};
        use crate::blocks::texture::NiSourceTexture;
        use std::sync::Arc;

        let make_src = |name: &str| NiSourceTexture {
            net: crate::blocks::base::NiObjectNETData {
                name: None,
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            use_external: true,
            filename: Some(Arc::from(name)),
            pixel_data_ref: BlockRef::NULL,
            pixel_layout: 0,
            use_mipmaps: 0,
            alpha_format: 0,
            is_static: true,
        };

        let tex_prop = NiTexturingProperty {
            net: crate::blocks::base::NiObjectNETData {
                name: None,
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            texture_count: 7,
            base_texture: None,
            dark_texture: None,
            detail_texture: None,
            gloss_texture: None,
            glow_texture: None,
            bump_texture: Some(TexDesc {
                source_ref: BlockRef(4),
                flags: 0,
                transform: None,
            }),
            normal_texture: Some(TexDesc {
                source_ref: BlockRef(5),
                flags: 0,
                transform: None,
            }),
            decal_textures: Vec::new(),
        };

        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
            Box::new(make_ni_tri_shape(
                "Wall",
                identity_transform(),
                2,
                vec![BlockRef(3)],
            )),
            Box::new(make_tri_shape_data()),
            Box::new(tex_prop),
            Box::new(make_src("legacy_bump.dds")),
            Box::new(make_src("modern_normal.dds")),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        assert_eq!(
            meshes[0].normal_map.as_deref(),
            Some("modern_normal.dds"),
            "normal_texture should win when both slots are populated"
        );
    }

    #[test]
    fn import_falls_back_to_material_diffuse() {
        use crate::blocks::properties::NiMaterialProperty;

        let mat = NiMaterialProperty {
            net: crate::blocks::base::NiObjectNETData {
                name: None,
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            ambient: NiColor {
                r: 0.2,
                g: 0.2,
                b: 0.2,
            },
            diffuse: NiColor {
                r: 0.8,
                g: 0.4,
                b: 0.2,
            },
            specular: NiColor::default(),
            emissive: NiColor {
                r: 0.0,
                g: 0.0,
                b: 0.0,
            },
            shininess: 10.0,
            alpha: 1.0,
            emissive_mult: 1.0,
        };

        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
            Box::new(make_ni_tri_shape(
                "Mat",
                identity_transform(),
                2,
                vec![BlockRef(3)],
            )),
            Box::new(make_tri_shape_data()),
            Box::new(mat),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        for color in &meshes[0].colors {
            assert!((color[0] - 0.8).abs() < 1e-6);
            assert!((color[1] - 0.4).abs() < 1e-6);
            assert!((color[2] - 0.2).abs() < 1e-6);
        }
    }

    #[test]
    fn import_defaults_to_white_without_material() {
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
            Box::new(make_ni_tri_shape(
                "NoMat",
                identity_transform(),
                2,
                Vec::new(),
            )),
            Box::new(make_tri_shape_data()),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        for color in &meshes[0].colors {
            assert_eq!(*color, [1.0, 1.0, 1.0]);
        }
    }

    #[test]
    fn import_shape_with_no_data_ref_is_skipped() {
        let mut shape = make_ni_tri_shape("NoData", identity_transform(), 0, Vec::new());
        shape.data_ref = BlockRef::NULL;

        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
            Box::new(shape),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);
        assert!(meshes.is_empty());
    }

    #[test]
    fn compose_transforms_identity() {
        let a = NiTransform::default();
        let b = NiTransform::default();
        let c = transform::compose_transforms(&a, &b);
        assert_eq!(c.scale, 1.0);
        assert!((c.translation.x).abs() < 1e-6);
    }

    #[test]
    fn compose_transforms_translation_only() {
        let a = translated(1.0, 2.0, 3.0);
        let b = translated(4.0, 5.0, 6.0);
        let c = transform::compose_transforms(&a, &b);
        assert!((c.translation.x - 5.0).abs() < 1e-6);
        assert!((c.translation.y - 7.0).abs() < 1e-6);
        assert!((c.translation.z - 9.0).abs() < 1e-6);
    }

    #[test]
    fn zup_to_yup_vertex_positions() {
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
            Box::new(make_ni_tri_shape(
                "Test",
                identity_transform(),
                2,
                Vec::new(),
            )),
            Box::new(make_tri_shape_data()),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);
        let m = &meshes[0];

        assert_eq!(m.positions[0], [0.0, 0.0, 0.0]);
        assert_eq!(m.positions[1], [1.0, 0.0, 0.0]);
        assert_eq!(m.positions[2], [0.0, 0.0, -1.0]);
    }

    #[test]
    fn zup_to_yup_vertex_normals() {
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
            Box::new(make_ni_tri_shape(
                "Test",
                identity_transform(),
                2,
                Vec::new(),
            )),
            Box::new(make_tri_shape_data()),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        for n in &meshes[0].normals {
            assert_eq!(*n, [0.0, 1.0, 0.0]);
        }
    }

    #[test]
    fn zup_to_yup_translation() {
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(translated(0.0, 0.0, 5.0), vec![BlockRef(1)])),
            Box::new(make_ni_tri_shape("Up", identity_transform(), 2, Vec::new())),
            Box::new(make_tri_shape_data()),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        assert!((meshes[0].translation[0]).abs() < 1e-6);
        assert!((meshes[0].translation[1] - 5.0).abs() < 1e-6);
        assert!((meshes[0].translation[2]).abs() < 1e-6);
    }

    #[test]
    fn zup_to_yup_translation_forward() {
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(translated(0.0, 7.0, 0.0), vec![BlockRef(1)])),
            Box::new(make_ni_tri_shape(
                "Fwd",
                identity_transform(),
                2,
                Vec::new(),
            )),
            Box::new(make_tri_shape_data()),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        assert!((meshes[0].translation[0]).abs() < 1e-6);
        assert!((meshes[0].translation[1]).abs() < 1e-6);
        assert!((meshes[0].translation[2] - -7.0).abs() < 1e-6);
    }

    #[test]
    fn zup_to_yup_identity_rotation_stays_identity() {
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
            Box::new(make_ni_tri_shape("Id", identity_transform(), 2, Vec::new())),
            Box::new(make_tri_shape_data()),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        let q = &meshes[0].rotation;
        assert!(q[0].abs() < 1e-4, "qx={}", q[0]);
        assert!(q[1].abs() < 1e-4, "qy={}", q[1]);
        assert!(q[2].abs() < 1e-4, "qz={}", q[2]);
        assert!((q[3].abs() - 1.0).abs() < 1e-4, "qw={}", q[3]);
    }

    #[test]
    fn zup_to_yup_winding_order_preserved() {
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
            Box::new(make_ni_tri_shape(
                "Wind",
                identity_transform(),
                2,
                Vec::new(),
            )),
            Box::new(make_tri_shape_data()),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);

        assert_eq!(meshes[0].indices, vec![0, 1, 2]);
    }

    #[test]
    fn compose_degenerate_zero_matrix_uses_identity() {
        // Since #277, degenerate rotations are repaired at parse time
        // (read_ni_transform → sanitize_rotation). This test mirrors that
        // pipeline by sanitizing manually before composition.
        let zero_rot = NiMatrix3 {
            rows: [[0.0; 3]; 3],
        };
        let parent = NiTransform {
            rotation: crate::rotation::sanitize_rotation(zero_rot),
            translation: NiPoint3 {
                x: 10.0,
                y: 0.0,
                z: 0.0,
            },
            scale: 1.0,
        };
        let child = translated(5.0, 0.0, 0.0);
        let result = transform::compose_transforms(&parent, &child);

        assert!((result.translation.x - 15.0).abs() < 1e-4);
        assert!((result.translation.y).abs() < 1e-4);
        assert!((result.translation.z).abs() < 1e-4);
    }

    #[test]
    fn compose_degenerate_scaled_rotation_uses_svd() {
        let scaled_identity = NiMatrix3 {
            rows: [[2.0, 0.0, 0.0], [0.0, 2.0, 0.0], [0.0, 0.0, 2.0]],
        };
        let parent = NiTransform {
            rotation: crate::rotation::sanitize_rotation(scaled_identity),
            translation: NiPoint3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            scale: 1.0,
        };
        let child = translated(3.0, 4.0, 5.0);
        let result = transform::compose_transforms(&parent, &child);

        assert!((result.translation.x - 3.0).abs() < 1e-4);
        assert!((result.translation.y - 4.0).abs() < 1e-4);
        assert!((result.translation.z - 5.0).abs() < 1e-4);
    }

    #[test]
    fn compose_degenerate_scaled_rotation_rotates_child() {
        let scaled_rot_z90 = NiMatrix3 {
            rows: [[0.0, -2.0, 0.0], [2.0, 0.0, 0.0], [0.0, 0.0, 2.0]],
        };
        let parent = NiTransform {
            rotation: crate::rotation::sanitize_rotation(scaled_rot_z90),
            translation: NiPoint3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            scale: 1.0,
        };
        let child = translated(1.0, 0.0, 0.0);
        let result = transform::compose_transforms(&parent, &child);

        assert!(
            (result.translation.x).abs() < 1e-4,
            "x={}",
            result.translation.x
        );
        assert!(
            (result.translation.y - 1.0).abs() < 1e-4,
            "y={}",
            result.translation.y
        );
        assert!(
            (result.translation.z).abs() < 1e-4,
            "z={}",
            result.translation.z
        );
    }

    #[test]
    fn zup_to_yup_90deg_ccw_rotation_around_z() {
        let rot_z90 = NiMatrix3 {
            rows: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        };
        let q = coord::zup_matrix_to_yup_quat(&rot_z90);
        let sin45 = std::f32::consts::FRAC_PI_4.sin();
        let cos45 = std::f32::consts::FRAC_PI_4.cos();
        assert!(q[0].abs() < 1e-4, "qx={}", q[0]);
        assert!((q[1].abs() - sin45).abs() < 1e-4, "qy={}", q[1]);
        assert!(q[2].abs() < 1e-4, "qz={}", q[2]);
        assert!((q[3].abs() - cos45).abs() < 1e-4, "qw={}", q[3]);
    }

    /// Regression: #333 / D4-05. Export-tool drift can produce matrices
    /// whose determinant is in the (1.0, 1.07] window that the fast-path
    /// gate admits; without normalisation the Shepperd extraction
    /// produced a quaternion up to ~3.5% off unity, which downstream
    /// consumers (`scene.rs`, `cell_loader.rs`) feed directly into
    /// `Quat::from_xyzw` without normalising. The post-fix output is
    /// always unit-length regardless of the input matrix's scale drift.
    #[test]
    fn zup_to_yup_drifted_rotation_returns_unit_quaternion() {
        // Identity-around-Z rotation scaled by 1.03 — 6% determinant
        // drift, still inside the fast path. Pre-fix |q| ≈ 1.03; post-fix
        // |q| == 1.0 to f32 precision.
        let drift = 1.03f32;
        let scaled_identity = NiMatrix3 {
            rows: [
                [drift, 0.0, 0.0],
                [0.0, drift, 0.0],
                [0.0, 0.0, drift],
            ],
        };
        let q = coord::zup_matrix_to_yup_quat(&scaled_identity);
        let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
        assert!(
            (len - 1.0).abs() < 1e-5,
            "fast-path quaternion must be unit-length; got {len} (q={q:?})"
        );
    }

    #[test]
    fn zup_to_yup_90deg_ccw_rotation_around_x() {
        let rot_x90 = NiMatrix3 {
            rows: [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
        };
        let q = coord::zup_matrix_to_yup_quat(&rot_x90);
        let sin45 = std::f32::consts::FRAC_PI_4.sin();
        let cos45 = std::f32::consts::FRAC_PI_4.cos();
        assert!((q[0].abs() - sin45).abs() < 1e-4, "qx={}", q[0]);
        assert!(q[1].abs() < 1e-4, "qy={}", q[1]);
        assert!(q[2].abs() < 1e-4, "qz={}", q[2]);
        assert!((q[3].abs() - cos45).abs() < 1e-4, "qw={}", q[3]);
    }

    /// Regression test for issue #150 — `BsOrderedNode` (and every other
    /// NiNode subclass with a `base: NiNode` field) must unwrap cleanly
    /// during scene-graph walks. Previously the walker only downcast to
    /// plain `NiNode`, so children of BSOrderedNode (FO3/FNV weapons,
    /// effects, architecture) were silently dropped.
    #[test]
    fn bs_ordered_node_children_are_walked() {
        use crate::blocks::node::BsOrderedNode;

        // Root BsOrderedNode with a single NiTriShape child.
        let inner_node = make_ni_node(identity_transform(), vec![BlockRef(1)]);
        let ordered = BsOrderedNode {
            base: inner_node,
            alpha_sort_bound: [0.0, 0.0, 0.0, 10.0],
            is_static_bound: false,
        };
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(ordered),
            Box::new(make_ni_tri_shape(
                "OrderedChild",
                identity_transform(),
                2,
                Vec::new(),
            )),
            Box::new(make_tri_shape_data()),
        ];
        let scene = scene_from_blocks(blocks);

        // Flat path — would return zero meshes before the fix.
        let meshes = import_nif(&scene);
        assert_eq!(
            meshes.len(),
            1,
            "BsOrderedNode subtree must yield 1 mesh in flat import"
        );
        assert_eq!(meshes[0].name, Some(Arc::from("OrderedChild")));

        // Hierarchical path — must register the parent node AND the mesh.
        let imported = import_nif_scene(&scene);
        assert_eq!(imported.nodes.len(), 1);
        assert_eq!(imported.meshes.len(), 1);
        assert_eq!(imported.meshes[0].parent_node, Some(0));
    }

    /// Regression test for issue #150 — `BsValueNode` is a NiNode
    /// subclass carrying numeric metadata; its children must also be
    /// walked.
    #[test]
    fn bs_value_node_children_are_walked() {
        use crate::blocks::node::BsValueNode;

        let inner_node = make_ni_node(identity_transform(), vec![BlockRef(1)]);
        let value_node = BsValueNode {
            base: inner_node,
            value: 42,
            value_flags: 0,
        };
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(value_node),
            Box::new(make_ni_tri_shape(
                "ValueChild",
                identity_transform(),
                2,
                Vec::new(),
            )),
            Box::new(make_tri_shape_data()),
        ];
        let scene = scene_from_blocks(blocks);
        let meshes = import_nif(&scene);
        assert_eq!(meshes.len(), 1);
        assert_eq!(meshes[0].name, Some(Arc::from("ValueChild")));
    }

    /// Build a synthetic NIF scene where the root NiNode has a single
    /// NiParticleSystem child. The hierarchical importer must surface
    /// the emitter via `ImportedScene::particle_emitters` and the flat
    /// importer must surface it via `import_nif_particle_emitters`.
    /// Pre-#401 both paths discarded the block silently.
    fn ni_psys_block(type_name: &str) -> crate::blocks::particle::NiPSysBlock {
        crate::blocks::particle::NiPSysBlock {
            original_type: type_name.to_string(),
        }
    }

    #[test]
    fn hierarchical_import_surfaces_particle_emitter_under_named_host() {
        // Root NiNode named "TorchNode" with a NiParticleSystem child at index 1.
        let root = make_ni_node(identity_transform(), vec![BlockRef(1)]);
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(root),
            Box::new(ni_psys_block("NiParticleSystem")),
        ];
        let scene = scene_from_blocks(blocks);
        let imported = import_nif_scene(&scene);
        assert_eq!(imported.particle_emitters.len(), 1);
        let em = &imported.particle_emitters[0];
        assert_eq!(em.original_type, "NiParticleSystem");
        // Host is the root NiNode (index 0 in imported.nodes).
        assert_eq!(em.parent_node, Some(0));
    }

    #[test]
    fn flat_import_surfaces_particle_emitter_with_nearest_named_host() {
        // Root NiNode at translation (5, 10, 20), with NiParticleSystem child.
        let root = make_ni_node(translated(5.0, 10.0, 20.0), vec![BlockRef(1)]);
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(root),
            Box::new(ni_psys_block("NiParticleSystem")),
        ];
        let scene = scene_from_blocks(blocks);
        let emitters = import_nif_particle_emitters(&scene);
        assert_eq!(emitters.len(), 1);
        let em = &emitters[0];
        // Y-up conversion: (5, 10, 20) → (5, 20, -10).
        assert!((em.local_position[0] - 5.0).abs() < 1e-5);
        assert!((em.local_position[1] - 20.0).abs() < 1e-5);
        assert!((em.local_position[2] + 10.0).abs() < 1e-5);
        // Host name is the root node's name ("TestNode" per make_ni_node).
        assert_eq!(em.host_name.as_deref(), Some("TestNode"));
        assert_eq!(em.original_type, "NiParticleSystem");
    }

    #[test]
    fn flat_import_recognizes_legacy_particle_block_types() {
        // Each variant's original_type comes from the NIF dispatcher;
        // the importer must recognize all of them, not just "NiParticleSystem".
        for variant in [
            "NiMeshParticleSystem",
            "NiParticles",
            "NiParticleSystemController",
            "NiBSPArrayController",
            "NiAutoNormalParticles",
            "NiRotatingParticles",
        ] {
            let root = make_ni_node(identity_transform(), vec![BlockRef(1)]);
            let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
                Box::new(root),
                Box::new(ni_psys_block(variant)),
            ];
            let scene = scene_from_blocks(blocks);
            let emitters = import_nif_particle_emitters(&scene);
            assert_eq!(
                emitters.len(),
                1,
                "{} should surface as a particle emitter",
                variant
            );
            assert_eq!(emitters[0].original_type, variant);
        }
    }

    #[test]
    fn flat_import_skips_modifier_only_blocks() {
        // NiPSysGravity / NiPSysColorModifier / etc. are NiPSysBlock too,
        // but they're not renderable emitters — only modifier inputs to a
        // host NiParticleSystem. Surfacing them as emitters would spawn
        // duplicates; the importer must filter them out by original_type.
        let root = make_ni_node(identity_transform(), vec![BlockRef(1), BlockRef(2)]);
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(root),
            Box::new(ni_psys_block("NiPSysGravity")),
            Box::new(ni_psys_block("NiPSysColorModifier")),
        ];
        let scene = scene_from_blocks(blocks);
        let emitters = import_nif_particle_emitters(&scene);
        assert!(
            emitters.is_empty(),
            "modifier-only NiPSysBlocks must not surface as emitters, got {} entries",
            emitters.len(),
        );
    }

    /// Helper for the #364 test: build a `BsRangeNode` block with the
    /// given discriminator + the canonical (min, max, current) triple.
    fn ni_range_node(
        kind: crate::blocks::node::BsRangeKind,
        min: u8,
        max: u8,
        current: u8,
    ) -> crate::blocks::node::BsRangeNode {
        use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
        let inner_node = crate::blocks::node::NiNode {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: Some(Arc::from("RangeHost")),
                    extra_data_refs: Vec::new(),
                    controller_ref: BlockRef::NULL,
                },
                flags: 0,
                transform: identity_transform(),
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            children: Vec::new(),
            effects: Vec::new(),
        };
        crate::blocks::node::BsRangeNode {
            base: inner_node,
            min,
            max,
            current,
            kind,
        }
    }

    /// Regression: #364 — BSRangeNode subclasses (BSBlastNode /
    /// BSDamageStage / BSDebrisNode) must surface their wire-type
    /// discriminator on the resulting `ImportedNode.range_kind`.
    /// Pre-fix all four collapsed into a `BsRangeNode` with no
    /// surviving discriminator and the walker stripped them down to
    /// plain NiNode — gameplay-side systems couldn't tell apart
    /// "switch the visible damage stage" from "spawn debris on
    /// detach" from "fire the blast effect".
    #[test]
    fn import_surfaces_bs_range_kind_for_each_subclass() {
        for kind in [
            crate::blocks::node::BsRangeKind::Range,
            crate::blocks::node::BsRangeKind::DamageStage,
            crate::blocks::node::BsRangeKind::Blast,
            crate::blocks::node::BsRangeKind::Debris,
        ] {
            let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![Box::new(ni_range_node(
                kind, 0, 5, 2,
            ))];
            let scene = scene_from_blocks(blocks);
            let imported = import_nif_scene(&scene);
            assert_eq!(imported.nodes.len(), 1, "{:?}", kind);
            assert_eq!(
                imported.nodes[0].range_kind,
                Some(kind),
                "range_kind should round-trip the dispatcher discriminator for {:?}",
                kind,
            );
        }
    }

    /// Regression: #364 — plain NiNode produces `range_kind: None`.
    /// Catches a regression that defaults the discriminator to
    /// `Some(BsRangeKind::Range)` for every node.
    #[test]
    fn import_plain_ninode_has_no_range_kind() {
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> =
            vec![Box::new(make_ni_node(identity_transform(), Vec::new()))];
        let scene = scene_from_blocks(blocks);
        let imported = import_nif_scene(&scene);
        assert_eq!(imported.nodes.len(), 1);
        assert!(imported.nodes[0].range_kind.is_none());
    }

    /// Regression: #363 — `BSTreeNode` bone-list metadata must surface
    /// on `ImportedNode.tree_bones` resolved to the targets'
    /// `NiObjectNET.name` (mirrors the `#335` affected-node-names
    /// pattern). Pre-fix the walker stripped the BSTreeNode down to
    /// plain NiNode and dropped both bone lists, blocking any future
    /// SpeedTree wind / bend simulation from finding what to animate.
    #[test]
    fn import_surfaces_bs_tree_node_bones_by_name() {
        use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
        // Build three bone targets (NiNodes with names) at indices 1, 2, 3.
        // Then a BSTreeNode at index 0 whose:
        //   bones_1 = [1, 3]  (branch roots)
        //   bones_2 = [2]     (trunk)
        let bone = |name: &str| -> Box<dyn crate::blocks::NiObject> {
            Box::new(crate::blocks::node::NiNode {
                av: NiAVObjectData {
                    net: NiObjectNETData {
                        name: Some(Arc::from(name)),
                        extra_data_refs: Vec::new(),
                        controller_ref: BlockRef::NULL,
                    },
                    flags: 0,
                    transform: identity_transform(),
                    properties: Vec::new(),
                    collision_ref: BlockRef::NULL,
                },
                children: Vec::new(),
                effects: Vec::new(),
            })
        };
        let host = crate::blocks::node::NiNode {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: Some(Arc::from("TreeRoot")),
                    extra_data_refs: Vec::new(),
                    controller_ref: BlockRef::NULL,
                },
                flags: 0,
                transform: identity_transform(),
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            children: Vec::new(),
            effects: Vec::new(),
        };
        let tree = crate::blocks::node::BsTreeNode {
            base: host,
            bones_1: vec![BlockRef(1), BlockRef(3)],
            bones_2: vec![BlockRef(2)],
        };
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
            Box::new(tree),
            bone("Branch_A"),
            bone("Trunk_0"),
            bone("Branch_B"),
        ];
        let scene = scene_from_blocks(blocks);
        let imported = import_nif_scene(&scene);
        let host_node = &imported.nodes[0];
        let bones = host_node
            .tree_bones
            .as_ref()
            .expect("BSTreeNode should surface tree_bones");
        let branch: Vec<&str> = bones.branch_roots.iter().map(|s| s.as_ref()).collect();
        let trunk: Vec<&str> = bones.trunk.iter().map(|s| s.as_ref()).collect();
        assert_eq!(branch, vec!["Branch_A", "Branch_B"]);
        assert_eq!(trunk, vec!["Trunk_0"]);
    }

    /// Regression: #363 — when every bone ref in a BSTreeNode is null
    /// or unresolvable, surface `tree_bones: None` rather than a
    /// `Some(TreeBones { empty, empty })` so the consumer doesn't have
    /// to filter empty payloads downstream.
    #[test]
    fn import_drops_bs_tree_node_with_only_unresolvable_bones() {
        use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
        let host = crate::blocks::node::NiNode {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: Some(Arc::from("EmptyTree")),
                    extra_data_refs: Vec::new(),
                    controller_ref: BlockRef::NULL,
                },
                flags: 0,
                transform: identity_transform(),
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            children: Vec::new(),
            effects: Vec::new(),
        };
        let tree = crate::blocks::node::BsTreeNode {
            base: host,
            bones_1: vec![BlockRef::NULL, BlockRef(99)], // null + out-of-range
            bones_2: Vec::new(),
        };
        let scene = scene_from_blocks(vec![Box::new(tree)]);
        let imported = import_nif_scene(&scene);
        assert!(imported.nodes[0].tree_bones.is_none());
    }
}
