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

use super::material::{BsEffectShaderData, NoLightingFalloff};
use byroredux_core::ecs::components::collision::{CollisionShape, RigidBodyData};
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::FixedString;
use std::sync::Arc;

/// #2205 — the canonical kind tag now lives on `LightSource`
/// (`crates/core/src/ecs/components/light.rs`) so the spawn boundary can
/// carry it straight through without a second type. Re-exported here so
/// every existing `byroredux_nif::import::LightKind` call site keeps
/// working unchanged.
pub use byroredux_core::ecs::components::LightKind;

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
    /// LOD distance ranges when this node is a `NiLODNode` — its
    /// `NiRangeLODData` (`lod_center` + per-level `(near, far)` extents),
    /// converted to engine Y-up. The children (the LOD levels) are parallel
    /// to `lod_levels` (level `i` ↔ child `i`). Pre-foundation the walker
    /// imported child 0 only and dropped the ranges; this surfaces them so a
    /// future distance-switch system has the data without re-walking the NIF.
    /// **Import behaviour is unchanged** — child 0 (highest detail) is still
    /// the only child imported; nothing consumes `lod_group` at runtime yet.
    /// `None` on plain `NiNode` / `NiSwitchNode` and other subclasses.
    pub lod_group: Option<LodGroupData>,
}

/// LOD distance ranges from a [`NiLODNode`]'s `NiRangeLODData` — surfaced on
/// the matching [`ImportedNode`]. Parsed + carried for a future in-cell
/// distance-switch consumer (NIFAL §2 in-cell-LOD foundation); the renderer
/// does not yet read it.
#[derive(Debug, Clone)]
pub struct LodGroupData {
    /// LOD center in engine Y-up, node-local space (combine with the node's
    /// world transform at the consumer). Camera distance is measured from here.
    pub center: [f32; 3],
    /// `(near, far)` extent per LOD level, parallel to the host node's
    /// children (level `i` ↔ child `i`). The active level is the one whose
    /// `[near, far)` contains the camera-to-center distance.
    pub levels: Vec<(f32, f32)>,
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
    /// space, lifted from the BSOrderedNode wire format. The center is
    /// Y-up (Z-up → Y-up converted at extraction, #2008) like every
    /// other position field on `ImportedNode` and its siblings; `radius`
    /// is a magnitude and is unaffected by the conversion.
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

/// Re-export the `BsSubIndexTriShapeData` segmentation payload from
/// the parser side so callers downstream of `ImportedMesh` don't have
/// to reach into `crate::blocks::tri_shape` directly. Mirrors the
/// [`BsRangeKind`] re-export pattern. See #1206 — pre-fix the importer
/// dropped the `BsTriShapeKind::SubIndex` discriminator, making the
/// dismemberment / body-part segmentation payload invisible to
/// consumers downstream of the NIF crate.
pub use crate::blocks::tri_shape::BsSubIndexTriShapeData;

/// Source-agnostic texture roles produced by NIF material translation.
///
/// Game-specific slot numbers and container formats stop at the NIF import
/// boundary. `NiTexturingProperty`, `BSShaderTextureSet`, BGSM/BGEM, and
/// `BSEffectShaderProperty` translators all populate these semantic roles;
/// asset resolution and rendering consume the same shape without branching on
/// the originating game.
///
/// `T` deliberately represents the texture at each pipeline stage:
/// `Option<FixedString>` for imported paths, `Option<String>` for resolved
/// archive paths, and bindless indices for renderer-facing material data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MaterialTextureSet<T> {
    pub base_color: T,
    pub normal: T,
    pub emissive: T,
    pub detail: T,
    /// Smoothness/specular-strength mask (legacy gloss map / BGSM smooth-spec).
    pub smooth_spec: T,
    /// Legacy multiplicative dark/light map.
    pub dark: T,
    pub height: T,
    pub environment: T,
    pub environment_mask: T,
    pub tint: T,
    pub inner_layer: T,
    /// Standalone specular-colour map, distinct from [`Self::smooth_spec`].
    pub specular: T,
    pub lighting: T,
    pub flow: T,
    pub wrinkle: T,
    pub greyscale_lut: T,
    pub reflectance: T,
    pub emittance_gradient: T,
    /// Ordered legacy material-overlay layers.
    pub decals: [T; 4],
}

impl<T> MaterialTextureSet<T> {
    /// Convert every role to the representation used by the next pipeline
    /// stage while preserving the semantic slot ordering.
    pub fn map_ref<U>(&self, mut map: impl FnMut(&T) -> U) -> MaterialTextureSet<U> {
        MaterialTextureSet {
            base_color: map(&self.base_color),
            normal: map(&self.normal),
            emissive: map(&self.emissive),
            detail: map(&self.detail),
            smooth_spec: map(&self.smooth_spec),
            dark: map(&self.dark),
            height: map(&self.height),
            environment: map(&self.environment),
            environment_mask: map(&self.environment_mask),
            tint: map(&self.tint),
            inner_layer: map(&self.inner_layer),
            specular: map(&self.specular),
            lighting: map(&self.lighting),
            flow: map(&self.flow),
            wrinkle: map(&self.wrinkle),
            greyscale_lut: map(&self.greyscale_lut),
            reflectance: map(&self.reflectance),
            emittance_gradient: map(&self.emittance_gradient),
            decals: std::array::from_fn(|index| map(&self.decals[index])),
        }
    }

    /// Iterate every semantic role in canonical order.
    ///
    /// Centralizing this walk keeps lifecycle operations exhaustive: callers
    /// do not need to repeat the role list when releasing, validating, or
    /// otherwise visiting a material's textures.
    pub fn values(&self) -> impl Iterator<Item = &T> {
        [
            &self.base_color,
            &self.normal,
            &self.emissive,
            &self.detail,
            &self.smooth_spec,
            &self.dark,
            &self.height,
            &self.environment,
            &self.environment_mask,
            &self.tint,
            &self.inner_layer,
            &self.specular,
            &self.lighting,
            &self.flow,
            &self.wrinkle,
            &self.greyscale_lut,
            &self.reflectance,
            &self.emittance_gradient,
        ]
        .into_iter()
        .chain(self.decals.iter())
    }

    /// Iterate every role except base color.
    ///
    /// Runtime entities hold base color separately as `TextureHandle`; the
    /// remaining roles are owned by `MaterialTextureHandles`.
    pub fn secondary_values(&self) -> impl Iterator<Item = &T> {
        self.values().skip(1)
    }
}

#[cfg(test)]
mod material_texture_set_tests {
    use super::MaterialTextureSet;

    #[test]
    fn canonical_iteration_covers_every_role_once() {
        let textures = MaterialTextureSet {
            base_color: 0,
            normal: 1,
            emissive: 2,
            detail: 3,
            smooth_spec: 4,
            dark: 5,
            height: 6,
            environment: 7,
            environment_mask: 8,
            tint: 9,
            inner_layer: 10,
            specular: 11,
            lighting: 12,
            flow: 13,
            wrinkle: 14,
            greyscale_lut: 15,
            reflectance: 16,
            emittance_gradient: 17,
            decals: [18, 19, 20, 21],
        };

        assert_eq!(
            textures.values().copied().collect::<Vec<_>>(),
            (0..22).collect::<Vec<_>>()
        );
        assert_eq!(
            textures.secondary_values().copied().collect::<Vec<_>>(),
            (1..22).collect::<Vec<_>>()
        );
    }
}

/// Source-normalized material payload attached to an [`ImportedMesh`].
///
/// Mesh extractors translate their property blocks into this shape once;
/// geometry-specific constructors no longer repeat the same material field
/// forwarding. BGSM/BGEM resolution may subsequently patch this payload, but
/// consumers never need to know which mesh block carried it.
#[derive(Debug, Clone)]
pub struct ImportedMaterial {
    /// Skyrim+ `BSWaterShaderProperty` authored water feature flags.
    pub water_shader_flags: u32,
    /// Whether the source shader is a dedicated mesh-water shader.
    pub is_water_shader: bool,
    pub textures: MaterialTextureSet<Option<FixedString>>,
    pub material_path: Option<FixedString>,
    pub has_alpha: bool,
    pub src_blend_mode: u8,
    pub dst_blend_mode: u8,
    pub alpha_test: bool,
    pub alpha_threshold: f32,
    pub alpha_test_func: u8,
    pub two_sided: bool,
    pub is_decal: bool,
    pub is_pbr: bool,
    pub has_translucency: bool,
    pub model_space_normals: bool,
    /// **Provenance only**: an external `.bgsm` *or* `.bgem` file resolved for
    /// this material. Says nothing about which fields it supplied — the BGEM
    /// arm sets this too, and BGEM authors no smoothness/specular at all.
    /// Load-bearing for the FO4-vs-Skyrim glass-keyword discriminator
    /// (`classify_glass_into_material`, #2710), which is why it stays set on
    /// both arms. For "authoritative PBR scalars were merged" use
    /// [`Self::bgsm_pbr_scalars_authored`] instead (#2609).
    pub from_bgsm: bool,
    pub bgem_glass: bool,
    pub thin_glass: bool,
    /// #2609 — the BGSM arm of `merge_external_material` resolved authoritative
    /// spec-glossiness scalars and wrote them into
    /// [`Self::metalness_override`] / [`Self::roughness_override`].
    ///
    /// Distinct from [`Self::from_bgsm`], which is set on the BGEM arm as well
    /// even though BGEM leaves both overrides `None`. It is also distinct from
    /// `roughness_override.is_some()`: legacy inline-NIF content arrives with
    /// `Some(...)` from the keyword classifier, which is a *guess*, not
    /// authored data. Only this flag means "a real material file said so", so
    /// it is the correct gate for any heuristic that would otherwise overwrite
    /// resolved roughness.
    pub bgsm_pbr_scalars_authored: bool,
    pub metalness_override: Option<f32>,
    pub roughness_override: Option<f32>,
    pub translucency_subsurface_color: [f32; 3],
    pub translucency_transmissive_scale: f32,
    pub translucency_turbulence: f32,
    pub translucency_thick_object: bool,
    pub translucency_mix_albedo: bool,
    pub parallax_max_passes: Option<f32>,
    pub parallax_height_scale: Option<f32>,
    pub vertex_color_mode: u8,
    pub texture_clamp_mode: u8,
    pub emissive_color: [f32; 3],
    pub emissive_mult: f32,
    pub emissive_source: byroredux_core::ecs::components::material::EmissiveSource,
    pub specular_color: [f32; 3],
    pub diffuse_color: [f32; 3],
    pub ambient_color: [f32; 3],
    pub specular_strength: f32,
    pub glossiness: f32,
    pub refraction_strength: f32,
    pub lighting_effect_1: f32,
    pub lighting_effect_2: f32,
    pub subsurface_rolloff: f32,
    pub rimlight_power: f32,
    pub backlight_power: f32,
    pub grayscale_to_palette_scale: f32,
    pub bgsm_greyscale_lut_is_alpha: bool,
    /// #2643 (SF-D9-2026-08-07-04) — BGEM's `grayscale_to_palette_color`
    /// enable bit, tracked independently from
    /// `bgsm_greyscale_lut_is_alpha`. The BGEM format permits authoring
    /// BOTH the shared `grayscale_to_palette_color` bit and BGEM's own
    /// `grayscale_to_palette_alpha` bit at once — `bgsm_greyscale_lut_is_alpha`
    /// alone can't represent that combination, so a BGEM authoring both
    /// used to silently lose the color variant (only `EFFECT_PALETTE_ALPHA`
    /// packed). `pack_imported_material_flags` now ORs `EFFECT_PALETTE_COLOR`
    /// and `EFFECT_PALETTE_ALPHA` independently from this field and
    /// `bgsm_greyscale_lut_is_alpha`, matching how the inline NIF
    /// effect-shader path (`pack_effect_shader_flags`) already derives the
    /// two bits independently. Always mirrors the BGSM/BGEM `base.
    /// grayscale_to_palette_color` bit; BGSM materials (no alpha variant)
    /// only ever set this one.
    pub bgsm_greyscale_lut_color: bool,
    /// #2108 (SF-D9-01) — the authored BGSM/BGEM palette-remap ENABLE bit
    /// (`grayscale_to_palette_color`, or BGEM's `grayscale_to_palette_alpha`
    /// as an alternate enable), captured at the same merge step that fills
    /// `textures.greyscale_lut`. The greyscale LUT slot is a legal,
    /// always-serialized field — its mere presence does NOT mean the
    /// material wants the palette remap; only this flag does. Gates
    /// `EFFECT_PALETTE_COLOR`/`ALPHA` in `pack_imported_material_flags`,
    /// mirroring how the inline NIF effect-shader path
    /// (`pack_effect_shader_flags`) gates on the real SLSF enable bit
    /// instead of texture presence.
    pub bgsm_greyscale_lut_enabled: bool,
    pub fresnel_power: f32,
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    pub mat_alpha: f32,
    pub env_map_scale: f32,
    pub z_test: bool,
    pub z_write: bool,
    pub z_function: u8,
    pub effect_shader: Option<BsEffectShaderData>,
    pub material_kind: u32,
    /// Normalised `BSLightingShaderType` this material's texture slots were
    /// resolved with (#2695). The REFR texture overlay reads it back so an
    /// `XTXR` slot swap lands in the same canonical role
    /// [`crate::import::material::slot_to_role`] gave the mesh's own slot.
    /// `0` (Default) for materials with no dedicated BSLightingShaderProperty
    /// — legacy FO3/FNV/Oblivion property chains, BGSM/BGEM-only, synthesized.
    pub shader_type: u32,
    /// Per-game slot vocabulary used by `BSShaderTextureSet` and REFR XTXR.
    pub texture_slot_layout: crate::import::TextureSlotLayout,
    /// Host material's explicit slot-2 glow enable. Required because Skyrim
    /// also puts soft/rim masks in slot 2 (#3068).
    pub slot2_glow_enabled: bool,
    pub shader_type_fields: byroredux_core::ecs::components::material::ShaderTypeFields,
    pub no_lighting_falloff: Option<NoLightingFalloff>,
    pub wireframe: bool,
    pub flat_shading: bool,
}

impl Default for ImportedMaterial {
    fn default() -> Self {
        Self {
            water_shader_flags: 0,
            is_water_shader: false,
            textures: MaterialTextureSet::default(),
            material_path: None,
            has_alpha: false,
            src_blend_mode: 6,
            dst_blend_mode: 7,
            alpha_test: false,
            alpha_threshold: 0.5,
            alpha_test_func: 6,
            two_sided: false,
            is_decal: false,
            is_pbr: false,
            has_translucency: false,
            model_space_normals: false,
            from_bgsm: false,
            bgem_glass: false,
            thin_glass: false,
            bgsm_pbr_scalars_authored: false,
            metalness_override: None,
            roughness_override: None,
            translucency_subsurface_color: [0.0; 3],
            translucency_transmissive_scale: 0.0,
            translucency_turbulence: 0.0,
            translucency_thick_object: false,
            translucency_mix_albedo: false,
            parallax_max_passes: None,
            parallax_height_scale: None,
            vertex_color_mode: 2,
            texture_clamp_mode: 0,
            emissive_color: [0.0; 3],
            emissive_mult: 0.0,
            emissive_source: byroredux_core::ecs::components::material::EmissiveSource::None,
            specular_color: [1.0; 3],
            diffuse_color: [1.0; 3],
            ambient_color: [1.0; 3],
            specular_strength: 0.0,
            glossiness: 0.0,
            refraction_strength: 0.0,
            lighting_effect_1: 0.0,
            lighting_effect_2: 0.0,
            subsurface_rolloff: 0.0,
            rimlight_power: 0.0,
            backlight_power: 0.0,
            grayscale_to_palette_scale: 1.0,
            bgsm_greyscale_lut_is_alpha: false,
            bgsm_greyscale_lut_color: false,
            bgsm_greyscale_lut_enabled: false,
            fresnel_power: 5.0,
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            mat_alpha: 1.0,
            env_map_scale: 0.0,
            z_test: true,
            z_write: true,
            z_function: 3,
            effect_shader: None,
            material_kind: 0,
            shader_type: 0,
            texture_slot_layout: crate::import::TextureSlotLayout::default(),
            slot2_glow_enabled: false,
            shader_type_fields: Default::default(),
            no_lighting_falloff: None,
            wireframe: false,
            flat_shading: false,
        }
    }
}

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
    /// Canonical material translated independently of the source mesh block.
    pub material: ImportedMaterial,
    /// Node name from the NIF. Uses `Arc<str>` to avoid heap copies from the string table.
    pub name: Option<Arc<str>>,
    /// Index into `ImportedScene.nodes` for this mesh's parent node, or None.
    pub parent_node: Option<usize>,
    /// Skeletal skinning data. `None` for rigid meshes.
    pub skin: Option<ImportedSkin>,
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
    /// Raw `NiAVObject.flags` value (sibling of `ImportedNode.flags`).
    /// Consumers emit a `SceneFlags` component per shape entity. See #222.
    pub flags: u32,
    /// Distant-LOD triangle-count cutoffs `[lod0, lod1, lod2]` — the three
    /// thresholds an eventual M35 LOD selector will consult to choose
    /// which LOD level to draw at distance. Populated from two distinct
    /// wire types: `BSMeshLODTriShape` (FO4) via
    /// [`BsTriShapeKind::MeshLOD`], and `BSLODTriShape` (Skyrim/SSE,
    /// parsed as `NiLodTriShape`) via its own `lod{0,1,2}_size` fields
    /// threaded through by the import walker. Pre-#1207 the parser
    /// captured the FO4 triple but the importer dropped it; pre-#2283 the
    /// FO4 triple was itself discarded before import ever saw it (an
    /// intermediate kind immediately overwritten at dispatch) and the
    /// Skyrim/SSE triple was never threaded through at all. `None` on
    /// every non-LOD BSTriShape (the vast majority of meshes).
    pub bs_lod_cutoffs: Option<[u32; 3]>,
    /// `BSSubIndexTriShape` segmentation payload — segments table +
    /// optional FO4+ shared SSF metadata. Drives dismemberment /
    /// body-part segmentation / locational damage on actor meshes
    /// across Skyrim SE DLC, FO4, and FO76. Pre-#1206 the parser
    /// decoded the full [`BsSubIndexTriShapeData`] body via
    /// [`BsTriShapeKind::SubIndex`] but the importer dropped the
    /// discriminator entirely; the dismemberment system implementation
    /// (deferred) has nothing to consume. `None` on every non-SubIndex
    /// BSTriShape and every NiTriShape / BSGeometry.
    ///
    /// `Arc`, not an owned value (#2600): with zero readers today, every
    /// mesh import used to pay a full deep clone (segment table + nested
    /// sub-segment lists + shared-data string) into this field for
    /// nothing. `BsTriShapeKind::SubIndex` already holds the parsed
    /// payload behind an `Arc`, so this just shares that same allocation
    /// — an atomic refcount bump instead of a clone — while still handing
    /// the eventual consumer the full data, not a lossy presence flag.
    pub bs_sub_index: Option<Arc<BsSubIndexTriShapeData>>,
    /// #2631 / SF2D2-D2-03 — which of the source `BSGeometry`'s 4 LOD
    /// slots (0-3) this mesh was resolved from. `BSGeometry::meshes` only
    /// contains *present* slots (absent test-byte slots are skipped, not
    /// pushed as placeholders) and the importer additionally skips
    /// `scale<=0` sentinel slots when picking the first populated one
    /// (#1828/#1829), so `meshes[0]`/the resolved slot is the first
    /// *present and non-sentinel* slot, not necessarily authored LOD 0.
    /// Threaded through so a future LOD selector knows which level it
    /// actually loaded. `None` on every non-`BSGeometry` mesh (the
    /// concept doesn't exist for `NiTriShape`/`BSTriShape`).
    pub bs_geometry_lod_slot: Option<u32>,
    /// #2206 / NIFAL-D4-02 — nearest-ancestor `NiBillboardNode` mode, set
    /// by `walk_node_flat` when this mesh sits anywhere beneath one.
    /// `ImportedNode::billboard_mode` (above) carries the same data on the
    /// hierarchical (loose-NIF) walk, which spawns one entity per NIF node
    /// and can attach `Billboard` directly on the billboard node's own
    /// entity; the flat (cell-loader) walk spawns one entity per *mesh*
    /// with no node entities at all, so the mode has to ride along on the
    /// mesh instead. `None` for the vast majority of meshes (no billboard
    /// ancestor).
    pub billboard_mode: Option<u16>,
    /// #3231 — `NiGeomMorpherController` / `NiMorphData` vertex-delta
    /// targets, when the shape carries one. `None` for the vast majority
    /// of meshes (no morph controller, or every target failed the
    /// vertex-count guard — see `import::mesh::morph::extract_morph_targets`).
    /// The renderer's GPU morph-blend consumer is skinned-mesh-only in
    /// its current scope (mirrors `SkinSlotPool`'s per-entity slot
    /// model), but this field is captured regardless of `skin` so a
    /// future rigid-mesh consumer needs no second import-side change.
    pub morph_targets: Option<Vec<ImportedMorphTarget>>,
}

/// One morph target: name + per-vertex position deltas in the renderer's
/// Y-up convention (same axis-swap `zup_point_to_yup` applies to
/// [`ImportedMesh::positions`] — valid for deltas too since the swap is
/// a pure axis permutation/reflection with no translation term). See
/// [`ImportedMesh::morph_targets`].
#[derive(Debug, Clone)]
pub struct ImportedMorphTarget {
    /// Stable index in the source `NiMorphData.morphs` array. Animation
    /// channels resolve names to this same index, so import-time filtering
    /// must never compact it.
    pub original_index: u32,
    pub name: Option<Arc<str>>,
    /// `deltas.len()` is guaranteed to equal the owning mesh's vertex
    /// count — targets that failed that check at import time are
    /// dropped before reaching this struct, not represented here with a
    /// mismatched length.
    pub deltas: Vec<[f32; 3]>,
}

impl ImportedMesh {
    /// Build an opaque, untextured, lit mesh from raw renderer-space
    /// (Y-up) geometry, with identity transform and the same default
    /// Gamebryo material parameters the NIF importer applies when a mesh
    /// has no shader property (`diffuse`/`ambient` = white, `z_function`
    /// = LESSEQUAL, `fresnel_power` = 5, etc.). The caller sets
    /// `translation` / `rotation` / `scale` / `name` (and any material
    /// overrides) on the returned value.
    ///
    /// Shared builder for geometry that arrives without a NIF shader
    /// property — FO4 precombined CSG objects ([`crate::import::precombine`],
    /// M49) and synthetic/test geometry today. SpeedTree supplies authored
    /// bounds directly but shares [`ImportedMaterial::default`].
    /// `local_bound_*` is computed from `positions` via
    /// [`crate::import::mesh::extract_local_bound`].
    #[allow(clippy::too_many_arguments)]
    pub fn from_geometry(
        positions: Vec<[f32; 3]>,
        colors: Vec<[f32; 4]>,
        normals: Vec<[f32; 3]>,
        tangents: Vec<[f32; 4]>,
        uvs: Vec<[f32; 2]>,
        indices: Vec<u32>,
    ) -> Self {
        let (local_bound_center, local_bound_radius) = crate::import::mesh::extract_local_bound(
            crate::types::NiPoint3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            0.0,
            &positions,
        );
        Self {
            positions,
            colors,
            normals,
            tangents,
            uvs,
            indices,
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: 1.0,
            material: ImportedMaterial::default(),
            name: None,
            parent_node: None,
            skin: None,
            local_bound_center,
            local_bound_radius,
            flags: 0,
            bs_lod_cutoffs: None,
            bs_sub_index: None,
            bs_geometry_lod_slot: None,
            billboard_mode: None,
            morph_targets: None,
        }
    }
}

#[cfg(test)]
mod skin_partition_hiding_tests {
    use super::*;
    use crate::blocks::tri_shape::{
        BsGeometryPerSegmentSharedData, BsGeometrySegmentData, BsGeometrySegmentSharedData,
        BsGeometrySubSegment,
    };

    #[test]
    fn hides_only_triangles_whose_skyrim_body_slot_is_covered() {
        let mut mesh = ImportedMesh::from_geometry(
            vec![[0.0; 3]; 4],
            vec![[1.0; 4]; 4],
            vec![[0.0, 1.0, 0.0]; 4],
            Vec::new(),
            Vec::new(),
            vec![0, 1, 2, 0, 2, 3],
        );
        mesh.skin = Some(ImportedSkin {
            triangle_body_parts: vec![32, 33],
            ..Default::default()
        });

        let removed = mesh.hide_skin_partitions(1 << (32 - 30));

        assert_eq!(removed, 1);
        assert_eq!(mesh.indices, vec![0, 2, 3]);
        assert_eq!(mesh.skin.unwrap().triangle_body_parts, vec![33]);
    }

    #[test]
    fn fo3_anatomical_parts_are_not_misread_as_biped_slots() {
        assert_eq!(dismember_body_part_to_biped_bit(0), None);
        assert_eq!(dismember_body_part_to_biped_bit(13), None);
        assert_eq!(dismember_body_part_to_biped_bit(32), Some(2));
        assert_eq!(dismember_body_part_to_biped_bit(132), Some(2));
        assert_eq!(dismember_body_part_to_biped_bit(232), Some(2));
    }

    #[test]
    fn fo4_subindex_ranges_hide_body_but_preserve_nested_hand_segment() {
        let mut mesh = ImportedMesh::from_geometry(
            vec![[0.0; 3]; 5],
            vec![[1.0; 4]; 5],
            vec![[0.0, 1.0, 0.0]; 5],
            Vec::new(),
            Vec::new(),
            vec![0, 1, 2, 0, 2, 3, 0, 3, 4],
        );
        mesh.bs_sub_index = Some(Arc::new(BsSubIndexTriShapeData {
            num_primitives: 3,
            num_segments: 1,
            total_segments: 2,
            segments: vec![BsGeometrySegmentData {
                flags: None,
                start_index: 0,
                num_primitives: 3,
                parent_array_index: Some(0),
                sub_segments: vec![BsGeometrySubSegment {
                    start_index: 3,
                    num_primitives: 1,
                    parent_array_index: 1,
                    unused: 0,
                }],
            }],
            shared: Some(BsGeometrySegmentSharedData {
                num_segments: 1,
                total_segments: 2,
                segment_starts: vec![0],
                per_segment_data: vec![
                    BsGeometryPerSegmentSharedData {
                        user_index: 3, // FO4 BODY
                        bone_id: 1,
                        cut_offsets: Vec::new(),
                    },
                    BsGeometryPerSegmentSharedData {
                        user_index: 4, // FO4 L Hand
                        bone_id: 2,
                        cut_offsets: Vec::new(),
                    },
                ],
                ssf_filename: String::new(),
            }),
        }));

        let removed = mesh.hide_skin_partitions(1 << 3);

        assert_eq!(removed, 2);
        assert_eq!(mesh.indices, vec![0, 2, 3]);
    }
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
    /// Per-partition dismemberment flags (`BSDismemberSkinInstance`'s
    /// `BodyPartInfo`), in the same order as the linked `NiSkinPartition`'s
    /// `partitions`. The NPC equip pre-spawn hook consumes the derived
    /// per-triangle mapping below to suppress only the regions displaced by
    /// equipped armor. Empty when the skin instance is a
    /// plain `NiSkinInstance` (no dismemberment data) or the geometry
    /// path doesn't resolve one (BSGeometry / FO4+ BSSkin::Instance).
    /// See #1659.
    pub body_part_flags: Vec<crate::blocks::skin::BodyPartInfo>,
    /// One dismember body-part value per triangle in the owning mesh's
    /// `indices` vector. `u16::MAX` marks triangles that could not be matched
    /// to a `NiSkinPartition`. This closes the parser→spawn half of body-slot
    /// hiding: an NPC skin mesh can discard only the partitions displaced by
    /// equipped armor instead of keeping or dropping the whole mesh.
    pub triangle_body_parts: Vec<u16>,
}

impl Default for ImportedSkin {
    fn default() -> Self {
        Self {
            bones: Vec::new(),
            skeleton_root: None,
            vertex_bone_indices: Vec::new(),
            vertex_bone_weights: Vec::new(),
            body_part_flags: Vec::new(),
            triangle_body_parts: Vec::new(),
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

/// Convert Skyrim-family `BSDismemberBodyPartType` slot values to the biped
/// mask bit used by ARMO/ARMA records. Ordinary values are 30..=61; cap
/// variants encode the same slot in their final two digits (130, 230, ...).
/// FO3/FNV's anatomical 0..=13 values are deliberately left unmapped because
/// they are not biped-slot numbers.
pub fn dismember_body_part_to_biped_bit(body_part: u16) -> Option<u8> {
    let slot = match body_part {
        30..=61 => body_part,
        130..=161 | 230..=261 => body_part % 100,
        _ => return None,
    };
    u8::try_from(slot - 30).ok()
}

impl ImportedMesh {
    /// Remove indexed triangles belonging to body partitions covered by
    /// `hidden_biped_mask`. Supports both classic
    /// `BSDismemberSkinInstance` triangle associations and
    /// `BSSubIndexTriShape` ranges used by SSE/FO4/FO76. Vertex/skin arrays
    /// stay intact; only the draw index list changes, so shared skeleton and
    /// palette contracts are unaffected.
    pub fn hide_skin_partitions(&mut self, hidden_biped_mask: u32) -> usize {
        if hidden_biped_mask == 0 {
            return 0;
        }

        let old_triangle_count = self.indices.len() / 3;
        let mut hidden_triangles = vec![false; old_triangle_count];

        if let Some(skin) = &self.skin {
            if skin.triangle_body_parts.len() == old_triangle_count {
                for (hidden, &body_part) in
                    hidden_triangles.iter_mut().zip(&skin.triangle_body_parts)
                {
                    *hidden = dismember_body_part_to_biped_bit(body_part)
                        .is_some_and(|bit| hidden_biped_mask & (1u32 << bit) != 0);
                }
            }
        }

        if let Some(segmentation) = &self.bs_sub_index {
            let mut segment_bits = vec![None; old_triangle_count];
            let mut assign_range = |start_index: u32, num_primitives: u32, bit: Option<u8>| {
                if start_index % 3 != 0 {
                    return;
                }
                let start = (start_index / 3) as usize;
                let end = start
                    .saturating_add(num_primitives as usize)
                    .min(segment_bits.len());
                if start < end {
                    segment_bits[start..end].fill(bit);
                }
            };

            let shared_bit = |array_index: Option<u32>| {
                let data = segmentation
                    .shared
                    .as_ref()?
                    .per_segment_data
                    .get(array_index? as usize)?;
                // nif.xml: UINT_MAX means `user_index` refers to another
                // segment, not a Biped Object. Leave those triangles visible
                // rather than guessing through a malformed/reference chain.
                (data.bone_id != u32::MAX && data.user_index < 32).then_some(data.user_index as u8)
            };

            for segment in &segmentation.segments {
                let bit = segment
                    .flags
                    .and_then(|body_part| dismember_body_part_to_biped_bit(body_part as u16))
                    .or_else(|| shared_bit(segment.parent_array_index));
                assign_range(segment.start_index, segment.num_primitives, bit);
                for sub_segment in &segment.sub_segments {
                    assign_range(
                        sub_segment.start_index,
                        sub_segment.num_primitives,
                        shared_bit(Some(sub_segment.parent_array_index)),
                    );
                }
            }
            for (hidden, bit) in hidden_triangles.iter_mut().zip(segment_bits) {
                *hidden |= bit.is_some_and(|bit| hidden_biped_mask & (1u32 << bit) != 0);
            }
        }

        if !hidden_triangles.iter().any(|hidden| *hidden) {
            return 0;
        }

        let mut kept_indices = Vec::with_capacity(self.indices.len());
        for (triangle, hidden) in self.indices.chunks_exact(3).zip(&hidden_triangles) {
            if !hidden {
                kept_indices.extend_from_slice(triangle);
            }
        }
        self.indices = kept_indices;
        if let Some(skin) = self.skin.as_mut() {
            if skin.triangle_body_parts.len() == old_triangle_count {
                skin.triangle_body_parts = skin
                    .triangle_body_parts
                    .iter()
                    .zip(&hidden_triangles)
                    .filter_map(|(&part, hidden)| (!hidden).then_some(part))
                    .collect();
            }
        }
        old_triangle_count - self.indices.len() / 3
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

/// A Havok ragdoll articulation, engine-native (Y-up, havok-scaled).
///
/// `bodies` and `constraints` form a kinematic tree: each constraint
/// links two `bodies` by array index. Built by
/// [`crate::import::collision::extract_ragdoll`] from the NIF's
/// `BhkRigidBody` + `BhkConstraint` blocks. See M41.x.
#[derive(Debug, Clone)]
pub struct ImportedRagdoll {
    pub bodies: Vec<ImportedRagdollBody>,
    pub constraints: Vec<ImportedRagdollConstraint>,
}

/// One rigid body of a ragdoll, hosted on a skeleton bone.
#[derive(Debug, Clone)]
pub struct ImportedRagdollBody {
    /// Host bone name (the NiNode that carries this body's collision
    /// object). The engine resolves it to the bone `EntityId` to seed and
    /// write back the simulated pose.
    pub bone_name: Arc<str>,
    pub mass: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub friction: f32,
    pub restitution: f32,
    /// Collider shape in body-local space (Y-up, havok-scaled).
    pub shape: CollisionShape,
    /// Rigid-body origin in skeleton-root/rest space (Y-up, scaled).
    /// The engine resolves this against the host bone's rest transform once
    /// when building its runtime ragdoll template (#2336). Only meaningful
    /// when [`Self::is_t`] is `true` — see that field.
    pub translation: Vec3,
    /// Rigid-body orientation in skeleton-root/rest space (Y-up). Only
    /// meaningful when [`Self::is_t`] is `true` — see that field.
    pub rotation: Quat,
    /// `true` only if the source block was `bhkRigidBodyT`. Mirrors
    /// `BhkRigidBody::is_t` (#2316): plain `bhkRigidBody` carries the same
    /// wire-format `translation`/`rotation` fields, but Gamebryo treats them
    /// as identity — the engine must not resolve [`Self::translation`] /
    /// [`Self::rotation`] against the bone rest pose when this is `false`,
    /// instead treating the body as coincident with its host bone's own
    /// rest transform (zero local offset). #2447 / PHYS-01.
    pub is_t: bool,
}

/// One joint linking two ragdoll bodies (indices into
/// [`ImportedRagdoll::bodies`]).
#[derive(Debug, Clone)]
pub struct ImportedRagdollConstraint {
    pub body_a: usize,
    pub body_b: usize,
    pub kind: ImportedJointKind,
}

/// Joint geometry, converted to engine space. Pivots are positions
/// (Y-up, scaled); axes are unit directions (Y-up, unscaled). Angles are
/// radians, passed through from Havok.
#[derive(Debug, Clone)]
pub enum ImportedJointKind {
    /// 3-DOF cone/twist ball joint (`bhkRagdollConstraint`).
    Ragdoll {
        twist_a: Vec3,
        plane_a: Vec3,
        pivot_a: Vec3,
        twist_b: Vec3,
        plane_b: Vec3,
        pivot_b: Vec3,
        cone_max: f32,
        plane_min: f32,
        plane_max: f32,
        twist_min: f32,
        twist_max: f32,
    },
    /// 1-DOF angle-limited hinge (`bhkLimitedHingeConstraint`).
    LimitedHinge {
        axis_a: Vec3,
        /// Authored zero-angle reference direction for side A
        /// (`Perp Axis In A1`), orthogonalised against `axis_a` at the
        /// solver boundary. Defines the plane `min_angle`/`max_angle` are
        /// measured from — without it the solver must synthesize an
        /// arbitrary reference frame, rotating the enforced angle window
        /// by an uncontrolled amount from what the content author
        /// intended (#2448 / PHYS-02).
        perp_a: Vec3,
        pivot_a: Vec3,
        axis_b: Vec3,
        /// Authored zero-angle reference direction for side B
        /// (`Perp Axis In B1`). Zeroed (not authored) on Oblivion/
        /// Morrowind content — the solver boundary's `frame_rot` falls
        /// back to a synthesized perpendicular for a degenerate/zero
        /// input, so this stays safe either way.
        perp_b: Vec3,
        pivot_b: Vec3,
        min_angle: f32,
        max_angle: f32,
    },
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
    /// Authored billboard sprite texture resolved from the particle
    /// system's shader/property chain. `None` means the source omitted a
    /// texture; consumers may use a named preset fallback, but must not
    /// render the bindless diagnostic/white slot as a particle sprite.
    pub texture_path: Option<String>,
    /// Authored Gamebryo source/destination blend factors.
    pub src_blend: Option<u8>,
    pub dst_blend: Option<u8>,
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
    /// Authored `NiPSysEmitter` base spawn parameters from the first
    /// emitter in the modifier chain, when present. Overrides the
    /// name-heuristic preset's spawn fields at translate time. See
    /// [`ImportedEmitterParams`].
    pub emitter_params: Option<ImportedEmitterParams>,
    /// Authored birth rate (particles/sec) from the emitter's
    /// `NiPSysEmitterCtlr` chain, when present. Overrides the preset's
    /// `rate` at translate time. `None` → keep the preset rate (legacy
    /// `NiParticleSystemController` content, or no controller). See
    /// `docs/engine/nifal.md` — particles spawn-rate follow-up.
    pub emitter_rate: Option<f32>,
    /// The particle block's own local translation (Y-up), relative to
    /// the host node. Pre-#1333 the `NiParticleSystem`'s `NiAVObjectData`
    /// base was parsed then dropped, so an emitter authored with a
    /// non-zero offset (campfire smoke above the fire, FO4 steam stacks)
    /// spawned at the host node origin. The scene builder anchors the
    /// emitter at host-world × this local TRS. `[0, 0, 0]` for the
    /// common vanilla case (offset already baked into the host node).
    pub local_translation: [f32; 3],
    /// The particle block's own local rotation (Y-up quaternion
    /// `[x, y, z, w]`), relative to the host node. See
    /// [`ImportedParticleEmitter::local_translation`]. Identity
    /// (`[0, 0, 0, 1]`) when the block authored no rotation.
    pub local_rotation: [f32; 4],
    /// The particle block's own local uniform scale. `1.0` when the
    /// block authored none. See
    /// [`ImportedParticleEmitter::local_translation`].
    pub local_scale: f32,
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

/// Authored `NiPSysEmitter` base spawn parameters, harvested from the
/// first emitter block in the particle system's modifier chain. When
/// present these **override** the name-heuristic preset's spawn fields
/// (`torch_flame()` / `smoke()` / `magic_sparkles()` / `embers()`) so a
/// NIF that authored a fast, narrow, long-lived spray reads as one
/// rather than collapsing to a generic flame. `None` when the NIF has no
/// `NiPSys*Emitter` (every value comes from the preset). Mirrors the
/// `color_curve` / `force_fields` override precedent (#707 / #984). See
/// `docs/engine/nifal.md` — particles slice.
///
/// Angles are radians; `initial_color` is linear RGBA; `initial_radius`
/// / `life_span` are world units / seconds — all directly consumable by
/// the canonical `ParticleEmitter` (no Z-up→Y-up conversion: these are
/// scalars / angles / colour, not directions).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImportedEmitterParams {
    pub speed: f32,
    pub speed_variation: f32,
    pub declination: f32,
    pub declination_variation: f32,
    pub initial_color: [f32; 4],
    pub initial_radius: f32,
    pub life_span: f32,
    pub life_span_variation: f32,
    /// FO3+ `NiPSysGrowFadeModifier.base_scale` — multiplier on
    /// `initial_radius` giving the particle's full size. `None` when no
    /// grow/fade modifier (or Oblivion, which has no base_scale field)
    /// → treated as `1.0`. The grow/fade bell-curve *shape* is not
    /// modelled (canonical size is a linear start→end lerp); only the
    /// authored magnitude `initial_radius × base_scale` is translated.
    pub base_scale: Option<f32>,
    /// `NiPSysEmitter.radius_variation` — per-particle spawn-size spread in
    /// world units (version-gated `>= 10.4.0.1`, satisfied by every retail
    /// Bethesda title). Scaled by `base_scale` and consumed as
    /// `ParticleEmitter.start_size_variation` at the overlay boundary; `0.0`
    /// when the emitter authored no spread (#1775).
    pub radius_variation: f32,
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
    /// Authored billboard sprite texture (owned because this flat import
    /// does not retain the temporary string pool used during extraction).
    pub texture_path: Option<String>,
    /// Authored Gamebryo source/destination blend factors.
    pub src_blend: Option<u8>,
    pub dst_blend: Option<u8>,
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
    /// Authored `NiPSysEmitter` base spawn parameters, when present.
    /// Mirror of [`ImportedParticleEmitter::emitter_params`] for the
    /// flat (cell-loader) path. See [`ImportedEmitterParams`].
    pub emitter_params: Option<ImportedEmitterParams>,
    /// Mirror of [`ImportedParticleEmitter::emitter_rate`] for the flat
    /// (cell-loader) path. Authored birth rate (particles/sec).
    pub emitter_rate: Option<f32>,
}
