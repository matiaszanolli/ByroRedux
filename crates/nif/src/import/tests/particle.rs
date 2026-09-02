//! Particle-system emitter tests: hierarchical/flat emitter surfacing,
//! modern `NiParticleSystem` aliases, `NiPSysColorModifier` color-curve
//! capture, and modifier-only block filtering.

use super::super::*;
use crate::types::BlockRef;

use super::{identity_transform, make_ni_node, scene_from_blocks, translated};

/// Build a synthetic NIF scene where the root NiNode has a single
/// NiParticleSystem child. The hierarchical importer must surface
/// the emitter via `ImportedScene::particle_emitters` and the flat
/// importer must surface it via `import_nif_particle_emitters`.
/// Pre-#401 both paths discarded the block silently.
///
/// Modern emitter types (`NiParticleSystem` / `NiMeshParticleSystem` /
/// `NiParticles` / `BSStripParticleSystem`) dispatch to the typed
/// `NiParticleSystem` struct post-#984 — the synthetic fixture matches.
/// Any other name builds an opaque `NiPSysBlock` (the modifier fallback),
/// used to fixture modifier-only blocks for the "skipped, not surfaced as
/// an emitter" tests. NB: the legacy `NiParticleSystemController` /
/// `NiAutoNormalParticles` / `NiRotatingParticles` types actually dispatch
/// to `legacy_particle::*` (not `NiPSysBlock`) and are deliberately not
/// surfaced as emitters (#1327) — so they are not used as emitter fixtures
/// here.
///
/// #2568 re-examined that decision and confirmed it: zero legacy particle
/// blocks exist across 54 202 vanilla NIFs spanning all five target games
/// (per-archive counts in `blocks/mod.rs`). These assertions pin a
/// deliberate scope boundary backed by measurement, not an unfixed gap.
fn synthetic_particle_block(type_name: &str) -> Box<dyn crate::blocks::NiObject> {
    match type_name {
        "NiParticleSystem" | "NiMeshParticleSystem" | "NiParticles" | "BSStripParticleSystem" => {
            Box::new(crate::blocks::particle::NiParticleSystem {
                original_type: type_name.to_string(),
                transform: crate::types::NiTransform::default(),
                properties: Vec::new(),
                shader_property_ref: BlockRef::NULL,
                alpha_property_ref: BlockRef::NULL,
                modifier_refs: Vec::new(),
            })
        }
        _ => Box::new(crate::blocks::particle::NiPSysBlock::marker(type_name)),
    }
}

#[test]
fn hierarchical_import_surfaces_particle_emitter_under_named_host() {
    // Root NiNode named "TorchNode" with a NiParticleSystem child at index 1.
    let root = make_ni_node(identity_transform(), vec![BlockRef(1)]);
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> =
        vec![Box::new(root), synthetic_particle_block("NiParticleSystem")];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);
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
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> =
        vec![Box::new(root), synthetic_particle_block("NiParticleSystem")];
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
fn flat_import_recognizes_modern_particle_system_aliases() {
    // The modern NiParticleSystem aliases all dispatch to the typed
    // `NiParticleSystem` struct (#984); the importer must surface each as
    // an emitter, not just the bare "NiParticleSystem" name. Legacy
    // controller / particle types (NiParticleSystemController,
    // NiAutoNormalParticles, NiRotatingParticles) dispatch to
    // `legacy_particle::*` and are deliberately not surfaced (#1327), so
    // they are not exercised here.
    for variant in [
        "NiMeshParticleSystem",
        "NiParticles",
        "BSStripParticleSystem",
    ] {
        let root = make_ni_node(identity_transform(), vec![BlockRef(1)]);
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> =
            vec![Box::new(root), synthetic_particle_block(variant)];
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

/// Regression for #707 / FX-2. When the scene has a real
/// `NiPSysColorModifier` chained to a `NiColorData` keyframe
/// stream, both the hierarchical and flat importers must
/// surface the captured `(start, end)` colour curve on every
/// emitter so the cell loader / scene builder can override the
/// name-heuristic preset's start_color / end_color.
///
/// Pre-fix the parser captured the modifier's `color_data_ref`
/// then immediately discarded it, every emitter rendered with
/// the heuristic preset's colour, and Dragonsreach embers
/// rendered as generic dark torch-flame columns.
#[test]
fn import_captures_color_curve_from_psys_color_modifier_chain() {
    use crate::blocks::interpolator::{Color4Key, KeyGroup, KeyType, NiColorData};
    use crate::blocks::particle::{NiPSysColorModifier, NiPSysModifierBase};

    // Scene layout:
    //   [0] NiNode root with the emitter as child
    //   [1] NiParticleSystem (the renderable emitter)
    //   [2] NiPSysColorModifier referencing block [3]
    //   [3] NiColorData with start = orange, end = red
    let root = make_ni_node(identity_transform(), vec![BlockRef(1), BlockRef(2)]);
    let modifier = NiPSysColorModifier {
        base: NiPSysModifierBase {
            name: Some(Arc::from("ColorMod")),
            order: 0,
            target_ref: BlockRef::NULL,
            active: true,
        },
        color_data_ref: BlockRef(3),
    };
    let color_data = NiColorData {
        keys: KeyGroup {
            key_type: KeyType::Linear,
            keys: vec![
                Color4Key {
                    time: 0.0,
                    value: [1.0, 0.55, 0.10, 1.0], // warm orange — start
                    tangent_forward: [0.0; 4],
                    tangent_backward: [0.0; 4],
                    tbc: None,
                },
                Color4Key {
                    time: 0.5,
                    value: [0.85, 0.20, 0.05, 0.7], // mid (intentionally distinct
                    // from start/end so an
                    // off-by-one would surface)
                    tangent_forward: [0.0; 4],
                    tangent_backward: [0.0; 4],
                    tbc: None,
                },
                Color4Key {
                    time: 1.0,
                    value: [0.30, 0.05, 0.02, 0.0], // dim red fade — end
                    tangent_forward: [0.0; 4],
                    tangent_backward: [0.0; 4],
                    tbc: None,
                },
            ],
        },
    };
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(root),
        synthetic_particle_block("NiParticleSystem"),
        Box::new(modifier),
        Box::new(color_data),
    ];
    let scene = scene_from_blocks(blocks);

    // Hierarchical import.
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);
    assert_eq!(imported.particle_emitters.len(), 1);
    let curve = imported.particle_emitters[0]
        .color_curve
        .expect("hierarchical import must capture the curve");
    assert_eq!(curve.start, [1.0, 0.55, 0.10, 1.0]);
    assert_eq!(curve.end, [0.30, 0.05, 0.02, 0.0]);

    // Flat import — same scene, same expectation.
    let flat = import_nif_particle_emitters(&scene);
    assert_eq!(flat.len(), 1);
    let flat_curve = flat[0]
        .color_curve
        .expect("flat import must capture the curve");
    assert_eq!(flat_curve.start, [1.0, 0.55, 0.10, 1.0]);
    assert_eq!(flat_curve.end, [0.30, 0.05, 0.02, 0.0]);
}

/// Companion: when no `NiPSysColorModifier` is present, the
/// importer leaves `color_curve = None` and the renderer falls
/// back to the name-heuristic preset.
#[test]
fn import_leaves_color_curve_none_when_no_color_modifier() {
    let root = make_ni_node(identity_transform(), vec![BlockRef(1)]);
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> =
        vec![Box::new(root), synthetic_particle_block("NiParticleSystem")];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);
    assert_eq!(imported.particle_emitters.len(), 1);
    assert!(
        imported.particle_emitters[0].color_curve.is_none(),
        "no NiPSysColorModifier in scene → color_curve must stay None"
    );
    let flat = import_nif_particle_emitters(&scene);
    assert!(flat[0].color_curve.is_none());
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
        synthetic_particle_block("NiPSysGravity"),
        synthetic_particle_block("NiPSysColorModifier"),
    ];
    let scene = scene_from_blocks(blocks);
    let emitters = import_nif_particle_emitters(&scene);
    assert!(
        emitters.is_empty(),
        "modifier-only NiPSysBlocks must not surface as emitters, got {} entries",
        emitters.len(),
    );
}

/// Build a minimal `BSEffectShaderProperty` with every string field empty
/// / every scalar at a neutral default except `greyscale_texture` — the
/// one field this test exercises. Mirrors
/// `effect_shader_capture_tests::fully_populated_fo4_shader`'s shape at a
/// smaller scale (that helper lives in a sibling test module and isn't
/// `pub`, so this is a separate minimal fixture, not a duplicate).
fn effect_shader_with_greyscale_texture(path: &str) -> crate::blocks::shader::BSEffectShaderProperty {
    use crate::blocks::base::NiObjectNETData;
    crate::blocks::shader::BSEffectShaderProperty {
        net: NiObjectNETData {
            name: None,
            extra_data_refs: Vec::new(),
            controller_ref: BlockRef::NULL,
        },
        material_reference: false,
        shader_flags_1: 0,
        shader_flags_2: 0,
        sf1_crcs: Vec::new(),
        sf2_crcs: Vec::new(),
        uv_offset: [0.0, 0.0],
        uv_scale: [1.0, 1.0],
        source_texture: "fx/glow.dds".to_string(),
        texture_clamp_mode: 3,
        lighting_influence: 0,
        env_map_min_lod: 0,
        falloff_start_angle: 0.0,
        falloff_stop_angle: 0.0,
        falloff_start_opacity: 0.0,
        falloff_stop_opacity: 0.0,
        refraction_power: 0.0,
        base_color: [0.0; 4],
        base_color_scale: 1.0,
        soft_falloff_depth: 0.0,
        greyscale_texture: path.to_string(),
        env_map_texture: String::new(),
        normal_texture: String::new(),
        env_mask_texture: String::new(),
        env_map_scale: 1.0,
        reflectance_texture: String::new(),
        lighting_texture: String::new(),
        emittance_color: [0.0; 3],
        emit_gradient_texture: String::new(),
        luminance: None,
        starfield_tail: Vec::new(),
    }
}

/// Regression for #3590 (REN-2026-08-30-D6-03): the particle boundary
/// must carry the BGEM greyscale→palette LUT texture
/// (`BSEffectShaderProperty.greyscale_texture`, the fallback source when
/// no dedicated `BSShaderTextureSet` slot 3 is authored) through to
/// `ImportedParticleEmitter::greyscale_lut_map`. Pre-fix
/// `extract_particle_material` captured `effect_shader` in full — so
/// `pack_effect_shader_flags` still derived the correct
/// `EFFECT_PALETTE_COLOR`/`EFFECT_PALETTE_ALPHA` bits from it — but never
/// resolved the LUT *texture* itself, so those forwarded bits had no
/// palette to remap through and were structurally inert on every particle
/// draw.
#[test]
fn hierarchical_import_carries_effect_shader_greyscale_lut_to_the_particle_emitter() {
    // [0] NiNode root, host of [1]. [1] NiParticleSystem binds [2] via
    // shader_property_ref (BSEffectShaderProperty is a Skyrim+ shader,
    // bound this way rather than through the legacy NiProperty chain).
    let root = make_ni_node(identity_transform(), vec![BlockRef(1)]);
    let ps = crate::blocks::particle::NiParticleSystem {
        original_type: "NiParticleSystem".to_string(),
        transform: crate::types::NiTransform::default(),
        properties: Vec::new(),
        shader_property_ref: BlockRef(2),
        alpha_property_ref: BlockRef::NULL,
        modifier_refs: Vec::new(),
    };
    let shader = effect_shader_with_greyscale_texture("fx/palette_grad.dds");
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> =
        vec![Box::new(root), Box::new(ps), Box::new(shader)];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);

    assert_eq!(imported.particle_emitters.len(), 1);
    let em = &imported.particle_emitters[0];
    assert_eq!(
        em.greyscale_lut_map.as_deref(),
        Some("fx/palette_grad.dds"),
        "the BGEM greyscale_texture must reach \
         ImportedParticleEmitter::greyscale_lut_map"
    );
    // The palette bits `pack_effect_shader_flags` derives ride on this
    // SAME effect_shader payload — assert both survive together, since a
    // fix that carries one but not the other reintroduces the
    // structurally-inert-bit shape #3590 is about.
    assert!(
        em.effect_shader
            .as_ref()
            .is_some_and(|data| data.greyscale_texture.as_deref()
                == Some("fx/palette_grad.dds")),
        "effect_shader.greyscale_texture must also survive — it's what \
         pack_effect_shader_flags reads to set the palette bits"
    );
}

/// Flat (cell-loader) counterpart of
/// `hierarchical_import_carries_effect_shader_greyscale_lut_to_the_particle_emitter`
/// — the two import paths share `extract_particle_material` but build
/// different output structs (`ImportedParticleEmitterFlat` here), so both
/// need the field threaded through independently.
#[test]
fn flat_import_carries_effect_shader_greyscale_lut_to_the_particle_emitter() {
    let root = make_ni_node(identity_transform(), vec![BlockRef(1)]);
    let ps = crate::blocks::particle::NiParticleSystem {
        original_type: "NiParticleSystem".to_string(),
        transform: crate::types::NiTransform::default(),
        properties: Vec::new(),
        shader_property_ref: BlockRef(2),
        alpha_property_ref: BlockRef::NULL,
        modifier_refs: Vec::new(),
    };
    let shader = effect_shader_with_greyscale_texture("fx/palette_grad.dds");
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> =
        vec![Box::new(root), Box::new(ps), Box::new(shader)];
    let scene = scene_from_blocks(blocks);
    let emitters = import_nif_particle_emitters(&scene);

    assert_eq!(emitters.len(), 1);
    assert_eq!(
        emitters[0].greyscale_lut_map.as_deref(),
        Some("fx/palette_grad.dds")
    );
}
