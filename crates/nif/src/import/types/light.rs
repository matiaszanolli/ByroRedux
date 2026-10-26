//! Imported light sources and texture effects.

use super::*;

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
    /// Unit direction (Y-up): column 0 of the light's world rotation for
    /// every kind (#4395), negated for directional lights so it points
    /// toward the light. Meaningful only for directional and spot lights;
    /// consumers ignore it for ambient/point.
    pub direction: [f32; 3],
    /// Diffuse RGB in 0..1 (multiplied by dimmer, ignoring alpha).
    pub color: [f32; 3],
    /// Effective radius in Bethesda units, derived from the attenuation
    /// parameters. Zero for ambient/directional (infinite reach).
    pub radius: f32,
    /// Light kind — the canonical `EmitterKind` (`LightKind` is its alias).
    /// The GPU's numeric packing lives in `gpu_light_from_emitter`
    /// (`byroredux/src/render/lights.rs`), not here.
    pub kind: LightKind,
    /// Outer cone half-angle in radians (0.0 for non-spot).
    pub outer_angle: f32,
    /// Names of the scene-graph nodes this light is restricted to,
    /// resolved from the `NiDynamicEffect.Affected Nodes` Ptr list.
    /// #5189 — an empty `Vec` does NOT mean "no restriction": Gamebryo
    /// scopes a dynamic effect to its registered affected nodes
    /// (`AttachAffectedNode` — "causes the effect to affect the entire
    /// subtree rooted at the given object"). Oblivion-era content
    /// (v20.0.0.x) carries the list on the wire — nif.xml gates
    /// `Num Affected Nodes` / `Affected Nodes` `since="10.1.0.0"` (the
    /// `until="4.0.0.2"` band is Morrowind-era) — but writes it empty
    /// and registers scope on the node side (`NiNode.effects`), so the
    /// empty field means "scope carried elsewhere or none" — never
    /// unrestricted. Do NOT gate the read off for v20.0: that is a
    /// 4-byte under-read per light on a format with no `block_sizes`,
    /// which truncates the scene. FO4 (BSVER >= 130) drops the list at
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
    /// empty means "scope carried elsewhere or none" (#5189), never
    /// "no restriction".
    pub affected_node_names: Vec<Arc<str>>,
}
