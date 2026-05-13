//! Public ECS-side data types produced by the NIF import pipeline.
//!
//! `ImportedScene` is the top-level result of [`super::import_nif_scene`].
//! It carries a tree of `ImportedNode`s (preserving NIF hierarchy via
//! parent indices), a flat list of `ImportedMesh` leaves, plus the
//! lights / texture effects / particle emitters / skinning data
//! extracted along the way.
//!
//! All transforms are local (relative to parent). Vertex / index data
//! is stored as plain `Vec<Vertex>` / `Vec<u32>` ready for GPU upload
//! via `MeshRegistry::upload()`.

use super::material::{BsEffectShaderData, NoLightingFalloff, ShaderTypeFields};
use byroredux_core::ecs::components::collision::{CollisionShape, RigidBodyData};
use byroredux_core::string::FixedString;
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
    /// The light's own NIF block name (from `NiObjectNETData.name`).
    /// `None` for anonymous lights (rare on shipped content, but
    /// possible in mods + debug content). The cell loader inserts a
    /// matching `Name` component on the spawned ECS entity so the
    /// animation system can resolve channels keyed by this name —
    /// `NiLightColorController` / `NiLightDimmerController` /
    /// `NiLightIntensityController` / `NiLightRadiusController` from
    /// the same NIF write into the light's `LightSource` per-frame.
    /// See #983.
    pub name: Option<Arc<str>>,
}

/// Kind of a parsed NIF light.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightKind {
    Ambient,
    Directional,
    Point,
    Spot,
}

/// One projected-texture effect extracted from a NIF scene, positioned
/// in world space. Populated from `NiTextureEffect` blocks during the
/// flat walk.
///
/// `NiTextureEffect` is a `NiDynamicEffect` subclass — same shape as
/// `ImportedLight` — that attaches a projected texture (sphere map /
/// env map / projected light cookie / projected shadow / fog) to its
/// `Affected Nodes` list. The legacy Gamebryo equivalent of a
/// projector light. The parser landed in #163 with all 12 wire fields,
/// but pre-#891 every block was parsed, validated, and silently
/// discarded — no consumer in `import/`, `byroredux/`, or `renderer/`
/// queried `scene.blocks` for `NiTextureEffect` downcasts.
///
/// Phase 1 (this struct) lights up the import-side capture so the
/// data is available to a future renderer-side projector pass without
/// needing a parser-side change. Phase 2 (renderer projector pipeline)
/// is deferred — currently no infrastructure exists.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedTextureEffect {
    /// World-space position (Y-up).
    pub translation: [f32; 3],
    /// World-space rotation as quaternion `[x, y, z, w]` (Y-up).
    pub rotation: [f32; 4],
    /// Y-up uniform scale.
    pub scale: f32,
    /// Texture path interned through the engine `StringPool`. `None`
    /// when `source_texture_ref` was null or didn't resolve to a
    /// `NiSourceTexture` with an external filename (embedded
    /// `NiPixelData` is not supported here — same convention as
    /// `tex_desc_source_path` in `import/material/mod.rs`).
    pub texture_path: Option<FixedString>,
    /// `texture_type` per nif.xml `TextureEffectType` enum:
    /// 0 = ProjectedLight, 1 = ProjectedShadow, 2 = Environment,
    /// 3 = FogMap. Renderer-side dispatch branches on this when the
    /// projector pass lands.
    pub texture_type: u32,
    /// `coordinate_generation_type` per nif.xml `CoordGenType` enum:
    /// 0 = WorldParallel, 1 = WorldPerspective, 2 = SphereMap,
    /// 3 = SpecularCubeMap, 4 = DiffuseCubeMap. Drives the projection
    /// math the renderer applies when sampling the projected texture.
    pub coordinate_generation_type: u32,
    /// Names of the scene-graph nodes this effect is restricted to,
    /// resolved from the `NiDynamicEffect.Affected Nodes` Ptr list.
    /// Same shape as [`ImportedLight::affected_node_names`] (#335) —
    /// empty `Vec` means "no restriction" (the projection affects
    /// every nearby surface).
    pub affected_node_names: Vec<Arc<str>>,
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
    /// `BSValueNode` numeric metadata `(value, value_flags)` when this
    /// node is a `BSValueNode`. FO3/FNV used the value field for
    /// LOD-distance overrides + billboard-mode hints on subtree roots;
    /// persisted into Skyrim chains. Pre-#625 the walker called
    /// `as_ni_node` which dropped the trailing fields — this surfaces
    /// them so a future LOD/billboard consumer can read them off the
    /// imported node instead of re-walking the NIF. `None` on plain
    /// `NiNode` and other subclasses. See #625 (SK-D4-02).
    pub bs_value_node: Option<BsValueNodeData>,
    /// `BSOrderedNode` draw-order metadata when this node is a
    /// `BSOrderedNode`. Children of an ordered node SHOULD render in
    /// sibling-index order (alpha-sorted UI / HUD overlays, Dragonborn
    /// banner stacks, FO3/FNV transparent stacks); the depth-only sort
    /// in `byroredux/src/render.rs::build_render_data` ignores this,
    /// producing alpha bleed on banner stacks. Pre-#625 the walker
    /// dropped `alpha_sort_bound` + `is_static_bound` along with the
    /// type identity. Renderer consumption (a `RenderOrderHint`
    /// component on each child + a sort-key tweak) is deferred — the
    /// data plumbing lands here so the eventual fix has the source
    /// material to work from. `None` on plain `NiNode` and other
    /// subclasses. See #625 (SK-D4-03).
    pub bs_ordered_node: Option<BsOrderedNodeData>,
}

/// `BSValueNode` numeric payload — surfaced on the matching
/// [`ImportedNode`] entry. Pre-#625 the walker dropped these fields
/// when it unwrapped the wrapper to plain `NiNode`. Future consumers:
/// LOD-distance override (FO3/FNV), billboard-mode hint on subtree
/// roots, gameplay-side cell-marker metadata.
#[derive(Debug, Clone, Copy)]
pub struct BsValueNodeData {
    /// Raw `BSValueNode.value` u32. Semantics depend on the parent
    /// subtree's gameplay role; the importer captures the wire value
    /// verbatim and lets consumers interpret.
    pub value: u32,
    /// `BSValueNode.value_flags` byte. Pre-#625 dropped along with
    /// `value`.
    pub flags: u8,
}

/// `BSOrderedNode` draw-order metadata — surfaced on the matching
/// [`ImportedNode`] entry. Pre-#625 the walker dropped both fields
/// when it unwrapped the wrapper to plain `NiNode`. Future renderer
/// consumption: tag children with `RenderOrderHint(sibling_index)` so
/// `build_render_data`'s sort prefers parent-supplied order over
/// `Transform.translation.z`. Carry `alpha_sort_bound` separately if
/// the renderer grows occlusion / culling on the bound.
#[derive(Debug, Clone, Copy)]
pub struct BsOrderedNodeData {
    /// Alpha-sort bounding sphere `[x, y, z, radius]` in node-local
    /// space, lifted from the BSOrderedNode wire format.
    pub alpha_sort_bound: [f32; 4],
    /// `true` when the bound is fixed (doesn't update with animation).
    /// Lets the renderer skip per-frame bound recomputation for static
    /// containers like a stack of inn-room banners.
    pub is_static_bound: bool,
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
    /// Vertex colors (RGBA). Falls back to material diffuse + 1.0 alpha
    /// or white. The alpha lane preserves authored per-vertex modulation
    /// for hair-tip cards, eyelash strips, and BSEffectShader meshes
    /// (#618). The renderer's current `Vertex` struct keeps a 3-channel
    /// color attribute, so consumers drop the alpha at upload — but the
    /// data is preserved here for the future 4-channel vertex format.
    pub colors: Vec<[f32; 4]>,
    /// Vertex normals. Falls back to +Y up if the mesh has no normals.
    pub normals: Vec<[f32; 3]>,
    /// Per-vertex tangents in the [`Vertex::tangent`] format: `[Tx, Ty, Tz,
    /// bitangent_sign]`. The bitangent is reconstructed shader-side as
    /// `bitangent_sign * cross(N, T)` (standard glTF/Vulkan convention).
    /// Empty when the source NIF does not author tangent data; the
    /// fragment shader's `perturbNormal` falls back to screen-space
    /// derivative TBN reconstruction in that case (the pre-#783 code
    /// path, retained for non-Bethesda content). See #783 / M-NORMALS.
    ///
    /// Per-game source:
    ///   - **Oblivion / FO3 / FNV** (NiTriShape): `NiBinaryExtraData` with
    ///     name `"Tangent space (binormal & tangent vectors)"`. Format per
    ///     nifly NifFile.cpp: `numVerts × 24 bytes` = N tangent
    ///     `Vector3` followed by N bitangent `Vector3`.
    ///   - **Skyrim LE/SE / FO4** (BSTriShape): packed inline in the
    ///     vertex stream as `bitangent_X` (in the position record's
    ///     trailing f32/hfloat slot), `bitangent_Y` (in the normal
    ///     record's trailing normbyte slot), and `tangent` + `bitangent_Z`
    ///     (in the tangent record's 4 normbytes).
    ///   - **Starfield** (BSGeometry): UDEC3 (10:10:10:2) packed in
    ///     `tangents_raw: Vec<u32>`. The 2-bit W is the bitangent sign.
    ///     Wired through to `tangents_raw` today; UDEC3 unpack into
    ///     `[f32; 4]` is a follow-up to this issue.
    pub tangents: Vec<[f32; 4]>,
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
    /// Holds an interned [`FixedString`] handle into the engine-wide
    /// [`StringPool`] (#609 / D6-NEW-01). Resolve via
    /// `pool.resolve(handle)` to get the original `&str`.
    pub texture_path: Option<FixedString>,
    /// BGSM/BGEM material file path (FO4+). When present and texture_path is
    /// None, the real texture paths live inside this .bgsm file in the
    /// Materials BA2. Stored for debug diagnostics and future BGSM parsing.
    pub material_path: Option<FixedString>,
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
    pub normal_map: Option<FixedString>,
    /// Glow / self-illumination texture (NiTexturingProperty slot 4,
    /// Oblivion/FO3/FNV). Separate from the BSShaderTextureSet glow
    /// slot, which the Skyrim+ path pulls directly. See #214.
    pub glow_map: Option<FixedString>,
    /// Detail overlay texture (NiTexturingProperty slot 2). See #214.
    pub detail_map: Option<FixedString>,
    /// Specular-mask / gloss texture (NiTexturingProperty slot 3).
    /// Per-texel specular strength. See #214.
    pub gloss_map: Option<FixedString>,
    /// Dark / multiplicative lightmap (NiTexturingProperty slot 1).
    /// Baked shadow modulation for Oblivion interior architecture. #264.
    pub dark_map: Option<FixedString>,
    /// Parallax / height texture (BSShaderTextureSet slot 3). FO3/FNV
    /// `shader_type = 3` / `7` surfaces plus Skyrim ParallaxOcc /
    /// MultiLayerParallax materials route through this. See #452.
    pub parallax_map: Option<FixedString>,
    /// Environment cubemap (BSShaderTextureSet slot 4). Paired with
    /// `env_map_scale` — glass, polished metal, power armor. See #452.
    pub env_map: Option<FixedString>,
    /// Environment-reflection mask (BSShaderTextureSet slot 5). #452.
    pub env_mask: Option<FixedString>,
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
    /// Gamebryo `TexClampMode` from the diffuse slot's `TexDesc.flags`
    /// (lower 4 bits): `0 = WRAP_S_WRAP_T` (default REPEAT), `1 =
    /// WRAP_S_CLAMP_T`, `2 = CLAMP_S_WRAP_T`, `3 = CLAMP_S_CLAMP_T`.
    /// Sourced from either `NiTexturingProperty.base_texture` (Oblivion
    /// / FO3 / FNV statics) or `BSEffectShaderProperty.texture_clamp_mode`
    /// (Skyrim+ effects). The renderer's bindless cache picks the
    /// matching `VkSamplerAddressMode` pair at descriptor-write time
    /// — pre-#610 the value was dropped and every texture rendered
    /// with REPEAT, leaving decal / scope-reticle / skybox-seam edges
    /// visibly bleeding.
    pub texture_clamp_mode: u8,
    /// Emissive color (RGB, linear).
    pub emissive_color: [f32; 3],
    /// Emissive intensity multiplier.
    pub emissive_mult: f32,
    /// Specular highlight color (RGB, linear).
    pub specular_color: [f32; 3],
    /// Diffuse tint (RGB, linear) from `NiMaterialProperty.diffuse`.
    /// Multiplied into sampled albedo by the fragment shader. Default
    /// `[1.0; 3]` (no tint) when the mesh ships only a BSShader path.
    /// Audit `AUDIT_LEGACY_COMPAT_2026-04-10.md` D4-09 / #221.
    pub diffuse_color: [f32; 3],
    /// Ambient color (RGB) from `NiMaterialProperty.ambient`. Modulates
    /// the cell ambient lighting term per material. Default `[1.0; 3]`.
    pub ambient_color: [f32; 3],
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
    /// fragment shader is per-variant follow-up work. Widened to
    /// `u32` per #570 (SK-D3-03); see `MaterialInfo::material_kind`
    /// for the rationale.
    pub material_kind: u32,
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
    /// Forces wireframe rendering (polygon_mode = LINE). Set by
    /// `NiWireframeProperty { flags: 1 }`. Oblivion vanilla never uses this;
    /// common in FO3/FNV mods. Renderer-side `VK_POLYGON_MODE_LINE` is
    /// deferred — tracked at #869 (O4-D4-NEW-01). The eventual fix ships a
    /// `WireframeOpaque { two_sided }` pipeline variant + matching `Blended`
    /// arm in `crates/renderer/src/vulkan/pipeline.rs`. Until then this
    /// bool is captured but not consulted on the render path.
    pub wireframe: bool,
    /// Forces flat shading (no per-vertex normal interpolation). Set by
    /// `NiShadeProperty { flags: 0 }` (bit 0 off = flat). Used on a handful
    /// of Oblivion architectural pieces for a faceted look. Renderer-side
    /// consumption is deferred — tracked at #869 (O4-D4-NEW-01). The two
    /// implementation paths are (a) parallel `triangle_flat.vert/frag` pair
    /// with pipeline-time switch, or (b) per-fragment dFdx/dFdy face-normal
    /// recompute gated on a per-batch flag. Until then this bool is captured
    /// but not consulted on the render path.
    pub flat_shading: bool,
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
/// Two extraction paths keep `vertex_bone_indices` /
/// `vertex_bone_weights` populated in parallel with
/// `ImportedMesh::positions`:
///
///   - **Legacy NiTriShape** — sparse per-bone weights from
///     `NiSkinData` are densified by keeping the 4 highest-weight
///     bones per vertex and re-normalising to sum to 1. See
///     `densify_sparse_weights` in `mesh.rs`.
///   - **Modern BSTriShape** — the packed vertex buffer's VF_SKINNED
///     bit-range decodes bone indices + weights at parse time (#177).
///     The importer clones them directly into this struct.
///
/// When either path cannot populate weights (e.g. a legacy shape with
/// no NiSkinData backing, or a BsTriShape whose `vertex_desc` lacks
/// VF_SKINNED), the two vecs are empty and the consumer must fall
/// back to rigid transform propagation.
#[derive(Debug, Clone)]
pub struct ImportedSkin {
    /// Bones this mesh binds to, in the order the interpolator refers
    /// to them by index.
    pub bones: Vec<ImportedBone>,
    /// Skeleton root bone name, if identifiable. The animation system
    /// uses this to know where to start applying joint transforms.
    pub skeleton_root: Option<Arc<str>>,
    /// Per-vertex bone indices (up to 4) — **already remapped to
    /// global indices into [`ImportedSkin::bones`]**. Parallel to
    /// `ImportedMesh::positions`. Pre-#613 BsTriShape stored these as
    /// `[u8; 4]` carrying *partition-local* values (indices into the
    /// per-`NiSkinPartition` `bones` palette, not the global skin
    /// list); shapes with > 1 partition silently aliased every vertex
    /// past partition 0 to the wrong bones. The importer now walks the
    /// partition table during extraction and emits global indices, so
    /// every value here resolves directly through `bones[idx]`.
    /// Widened to `u16` because vanilla Skyrim character + worn-armor
    /// skins routinely exceed 255 bones; mods can push higher.
    /// Empty if weights come from a separate source.
    pub vertex_bone_indices: Vec<[u16; 4]>,
    /// Per-vertex bone weights (up to 4). Must sum to 1.0 after
    /// normalization. Parallel to `vertex_bone_indices`.
    pub vertex_bone_weights: Vec<[f32; 4]>,
    /// **Global skin transform** (`NiSkinData::skinTransform`, the
    /// per-skin field, NOT the per-bone one). Bethesda's legacy
    /// NiSkinData ships this as a non-identity transform on every body
    /// NIF. The OpenMW skinning evaluator
    /// (`components/sceneutil/riggeometry.cpp:175-208`) composes it
    /// into the runtime palette as the OUTERMOST factor; NifSkope's
    /// partition path silently drops it (`tools/nifskope/src/gl/glmesh.cpp:875`)
    /// which is why our pre-Phase-1b.x palette computed `bone × invBind`
    /// without it and produced the body-skinning ribbon artifact. Y-up
    /// converted at extraction; identity if the source NIF didn't carry
    /// one (FO4+ BSSkin paths). See M41.0 Phase 1b.x research in
    /// `byroredux/tests/skinning_e2e.rs`.
    pub global_skin_transform: [[f32; 4]; 4],
}

impl Default for ImportedSkin {
    fn default() -> Self {
        Self {
            bones: Vec::new(),
            skeleton_root: None,
            vertex_bone_indices: Vec::new(),
            vertex_bone_weights: Vec::new(),
            // Identity matrix in column-major glam form. Required so a
            // default ImportedSkin doesn't multiply vertices by a zero
            // matrix when `global_skin_transform` is unused (e.g.
            // BSSkinInstance paths that don't carry this field).
            global_skin_transform: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }
}

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
    /// Ambient animation clip collecting every mesh-embedded controller
    /// (alpha fade, UV scroll, visibility flicker, material color
    /// pulse, shader float/color). Populated by
    /// [`crate::anim::import_embedded_animations`] during scene import.
    /// `None` when the NIF authored no such controllers — most
    /// non-FX/non-water meshes. See #261.
    pub embedded_clip: Option<crate::anim::AnimationClip>,
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
    /// Authored start / end RGBA from a `NiPSysColorModifier ->
    /// NiColorData` chain when the NIF carries one. `None` falls back
    /// to the name-heuristic preset; `Some` overrides the preset's
    /// `start_color` / `end_color` so authored Dragonsreach embers
    /// (warm orange) read distinctly from generic torch flames. Pre-#707
    /// the parser captured the ref but immediately discarded it, so
    /// every emitter rendered with the heuristic preset's colour.
    pub color_curve: Option<ParticleColorCurve>,
    /// Force fields collected from the source NIF's `modifier_refs`
    /// list — one `ImportedParticleForceField` per
    /// `NiPSys{Gravity,Vortex,Drag,Turbulence,Air,Radial}FieldModifier`
    /// in the chain. Empty when the NIF authored no field modifiers
    /// (most static-FX emitters like torches and ambient embers).
    /// See #984 / NIF-D5-ORPHAN-A2.
    pub force_fields: Vec<ImportedParticleForceField>,
}

/// One authored force field, mirrored 1:1 from a
/// `NiPSys{Gravity,Vortex,Drag,Turbulence,Air,Radial}FieldModifier`.
/// Renderer-facing fields stay in NIF Z-up local space — the scene
/// builder converts to engine Y-up world space when it spawns the
/// `ParticleEmitter.force_fields` entries (see byroredux/src/scene.rs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImportedParticleForceField {
    Gravity {
        direction: [f32; 3],
        strength: f32,
        decay: f32,
    },
    Vortex {
        axis: [f32; 3],
        strength: f32,
        decay: f32,
    },
    Drag {
        strength: f32,
        direction: [f32; 3],
        use_direction: bool,
    },
    Turbulence {
        frequency: f32,
        scale: f32,
    },
    Air {
        direction: [f32; 3],
        strength: f32,
        falloff: f32,
    },
    Radial {
        strength: f32,
        falloff: f32,
    },
}

/// Two-keyframe sample of a `NiColorData` curve, captured at NIF import
/// time. `start` is the t=0 RGBA value and `end` is the t=clip_end
/// RGBA value — i.e. the per-particle colour at spawn vs at death.
/// Skipping the in-between keys is the minimal first-pass per #707; a
/// full curve sampler is a follow-up. RGBA components are linear-
/// space floats in `[0, 1]` (NiColorData stores floats directly per
/// nif.xml's `Color4Key`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParticleColorCurve {
    pub start: [f32; 4],
    pub end: [f32; 4],
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
    /// Authored colour curve from `NiPSysColorModifier -> NiColorData`,
    /// when the NIF carries one. Overrides the heuristic preset's
    /// start / end colour at spawn time. `None` falls back to the
    /// preset. See [`ImportedParticleEmitter::color_curve`] for the
    /// rationale; same field, same #707 / FX-2 origin.
    pub color_curve: Option<ParticleColorCurve>,
    /// Force fields collected from the source NIF's
    /// `NiPSys{Gravity,Vortex,Drag,Turbulence,Air,Radial}FieldModifier`
    /// chain — empty for most non-FX emitters. See
    /// [`ImportedParticleEmitter::force_fields`] / #984.
    pub force_fields: Vec<ImportedParticleForceField>,
}
