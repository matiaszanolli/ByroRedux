//! MaterialProvider / BGSM merge (#493).
//!
//! Extracted from the 2051-LOC `asset_provider/tests.rs` (#2411 / TD1-010)
//! at the topic-divider comments that file already carried. Contents
//! unchanged.

use super::super::*;
use super::imported_mesh_with_material_path;
use byroredux_nif::import::ImportedMaterial;
use std::sync::Arc;

// ── MaterialProvider / BGSM merge (#493) ──────────────────────────
//
// The merge logic in `merge_external_material` has three moving parts:
//   1. Dispatch on `material_path` extension (.bgsm / .bgem / other)
//   2. For BGSM: walk the template chain child-first, fill empties
//   3. For BGEM: single-file fill, no inheritance
//
// We test part 2 + 3 directly against synthetic `ResolvedMaterial` /
// `BgemFile` values (bypassing archive IO which the `bgsm` crate's
// own tests cover). Part 1 is covered through the resolve_bgsm
// failure-dedup test below.

use byroredux_bgsm::template::ResolvedMaterial;
use byroredux_bgsm::{BgemFile, BgsmFile};

/// Same fill closure as `merge_external_material` so the tests verify
/// the exact precedence rule the prod helper uses.
fn fill(slot: &mut Option<String>, value: &str) -> bool {
    if slot.is_none() && !value.is_empty() {
        *slot = Some(value.to_string());
        true
    } else {
        false
    }
}

/// Walks a ResolvedMaterial chain child-first, filling the 6 slots
/// the prod merge helper writes for BGSM files. The slot set mirrors
/// `merge_external_material` exactly; the closure is inlined in prod
/// for a single allocation-free pass.
fn apply_bgsm_chain(
    resolved: &ResolvedMaterial,
    texture_path: &mut Option<String>,
    normal_map: &mut Option<String>,
    glow_map: &mut Option<String>,
    gloss_map: &mut Option<String>,
    env_map: &mut Option<String>,
    parallax_map: &mut Option<String>,
) {
    for step in resolved.walk() {
        fill(texture_path, &step.file.diffuse_texture);
        fill(normal_map, &step.file.normal_texture);
        fill(glow_map, &step.file.glow_texture);
        fill(gloss_map, &step.file.smooth_spec_texture);
        fill(env_map, &step.file.envmap_texture);
        fill(parallax_map, &step.file.displacement_texture);
    }
}

#[test]
fn bgsm_merge_fills_only_empty_slots() {
    // NIF already authored a diffuse; BGSM should NOT overwrite it.
    let mut texture_path: Option<String> = Some("nif_diffuse.dds".into());
    let mut normal_map: Option<String> = None;
    let mut glow_map: Option<String> = None;
    let mut gloss_map: Option<String> = None;
    let mut env_map: Option<String> = None;
    let mut parallax_map: Option<String> = None;

    let bgsm = BgsmFile {
        diffuse_texture: "bgsm_diffuse.dds".into(),
        normal_texture: "bgsm_normal.dds".into(),
        glow_texture: "bgsm_glow.dds".into(),
        ..Default::default()
    };
    let resolved = ResolvedMaterial {
        file: bgsm,
        parent: None,
    };

    apply_bgsm_chain(
        &resolved,
        &mut texture_path,
        &mut normal_map,
        &mut glow_map,
        &mut gloss_map,
        &mut env_map,
        &mut parallax_map,
    );

    // NIF-authored field preserved.
    assert_eq!(texture_path.as_deref(), Some("nif_diffuse.dds"));
    // Empty slots filled.
    assert_eq!(normal_map.as_deref(), Some("bgsm_normal.dds"));
    assert_eq!(glow_map.as_deref(), Some("bgsm_glow.dds"));
}

#[test]
fn bgsm_merge_child_wins_over_parent() {
    let mut texture_path: Option<String> = None;
    let mut normal_map: Option<String> = None;
    let mut glow_map: Option<String> = None;
    let mut gloss_map: Option<String> = None;
    let mut env_map: Option<String> = None;
    let mut parallax_map: Option<String> = None;

    let child = BgsmFile {
        diffuse_texture: "child_diffuse.dds".into(),
        // child leaves normal empty → parent fills it
        ..Default::default()
    };
    let parent = BgsmFile {
        diffuse_texture: "parent_diffuse.dds".into(),
        normal_texture: "parent_normal.dds".into(),
        ..Default::default()
    };
    let resolved = ResolvedMaterial {
        file: child,
        parent: Some(Arc::new(ResolvedMaterial {
            file: parent,
            parent: None,
        })),
    };

    apply_bgsm_chain(
        &resolved,
        &mut texture_path,
        &mut normal_map,
        &mut glow_map,
        &mut gloss_map,
        &mut env_map,
        &mut parallax_map,
    );

    assert_eq!(texture_path.as_deref(), Some("child_diffuse.dds"));
    assert_eq!(normal_map.as_deref(), Some("parent_normal.dds"));
}

#[test]
fn bgem_merge_fills_effect_slots() {
    // BGEM has no template inheritance — single file.
    let mut texture_path: Option<String> = None;
    let mut normal_map: Option<String> = None;
    let mut env_mask: Option<String> = None;

    let bgem = BgemFile {
        base_texture: "effect_base.dds".into(),
        normal_texture: "effect_normal.dds".into(),
        envmap_mask_texture: "effect_mask.dds".into(),
        ..Default::default()
    };

    fill(&mut texture_path, &bgem.base_texture);
    fill(&mut normal_map, &bgem.normal_texture);
    fill(&mut env_mask, &bgem.envmap_mask_texture);

    assert_eq!(texture_path.as_deref(), Some("effect_base.dds"));
    assert_eq!(normal_map.as_deref(), Some("effect_normal.dds"));
    assert_eq!(env_mask.as_deref(), Some("effect_mask.dds"));
}

/// Regression for #1076 / FO4-D6-002 — BGSM v>2 standalone slots
/// (`specular_texture`, `lighting_texture`, `flow_texture`,
/// `wrinkles_texture`) must forward to `ImportedMesh`'s
/// `specular_map` / `lighting_map` / `flow_map` / `wrinkle_map`.
/// Pre-fix the parser decoded all four fields and the merge
/// dropped them on the floor — FO4 water surfaces lost their
/// flow direction, NPC skin lost wrinkle blending, PBR specular
/// fell back to the gloss_map's .r-only path.
#[test]
fn bgsm_merge_forwards_v2_plus_standalone_slots() {
    // Use the in-test `fill` helper (`Option<String>` variant)
    // that mirrors the prod merge's intern-and-set semantic.
    let mut specular_map: Option<String> = None;
    let mut lighting_map: Option<String> = None;
    let mut flow_map: Option<String> = None;
    let mut wrinkle_map: Option<String> = None;

    let bgsm = BgsmFile {
        specular_texture: "armor_specular.dds".into(),
        lighting_texture: "armor_lighting.dds".into(),
        flow_texture: "water_flow.dds".into(),
        wrinkles_texture: "ncr_wrinkles.dds".into(),
        ..Default::default()
    };

    // Mirror the prod loop body for the four new slots.
    fill(&mut specular_map, &bgsm.specular_texture);
    fill(&mut lighting_map, &bgsm.lighting_texture);
    fill(&mut flow_map, &bgsm.flow_texture);
    fill(&mut wrinkle_map, &bgsm.wrinkles_texture);

    assert_eq!(specular_map.as_deref(), Some("armor_specular.dds"));
    assert_eq!(lighting_map.as_deref(), Some("armor_lighting.dds"));
    assert_eq!(flow_map.as_deref(), Some("water_flow.dds"));
    assert_eq!(wrinkle_map.as_deref(), Some("ncr_wrinkles.dds"));
}

/// Regression for #2627 (SF-D9-2026-08-07-02) — the BGSM v<=2 legacy
/// texture list (envmap, glow, inner_layer, wrinkles, displacement) had
/// one slot the merge never forwarded, even though the sink
/// (`MaterialTextureSet::inner_layer`) is a live, populated role via
/// the NIF `BSLightingShaderProperty` multi-layer-parallax path. A BGSM
/// authoring its inner layer externally (Skyrim SE ice/glass, FO4
/// layered panes) rendered with the layer silently absent. Exercises
/// the real `merge_external_material` path, not a mirror of its loop.
#[test]
fn bgsm_merge_forwards_inner_layer_texture() {
    let mut pool = byroredux_core::string::StringPool::new();
    let path = "materials/tests/inner_layer.bgsm";
    let mut provider = MaterialProvider::new();
    provider.insert_bgsm_for_test(
        path,
        ResolvedMaterial {
            file: BgsmFile {
                inner_layer_texture: "effects/glass/icelayer.dds".into(),
                ..Default::default()
            },
            parent: None,
        },
    );
    let mut mesh = imported_mesh_with_material_path(&mut pool, path);

    assert!(merge_external_material(&mut mesh.material, &mut provider, &mut pool,).merged());
    let handle = mesh
        .material
        .textures
        .inner_layer
        .expect("inner_layer_texture must reach MaterialTextureSet::inner_layer");
    assert_eq!(pool.resolve(handle), Some("effects/glass/icelayer.dds"));
}

#[test]
fn external_merge_records_only_the_roles_it_filled() {
    let mut pool = byroredux_core::string::StringPool::new();
    let path = "materials/tests/provenance.bgsm";
    let mut provider = MaterialProvider::new();
    provider.insert_bgsm_for_test(
        path,
        ResolvedMaterial {
            file: BgsmFile {
                diffuse_texture: "bgsm_diffuse.dds".into(),
                normal_texture: "bgsm_normal.dds".into(),
                ..Default::default()
            },
            parent: None,
        },
    );
    let mut mesh = imported_mesh_with_material_path(&mut pool, path);
    mesh.material.textures.base_color = Some(pool.intern("inline_diffuse.dds"));

    assert!(merge_external_material(&mut mesh.material, &mut provider, &mut pool).merged());
    assert_eq!(
        mesh.material.texture_sources.base_color,
        byroredux_nif::import::ImportedTextureSource::NifTextureSet,
        "the inline NIF role wins and keeps its source"
    );
    assert_eq!(
        mesh.material.texture_sources.normal,
        byroredux_nif::import::ImportedTextureSource::Bgsm,
        "the role filled by the external chain records BGSM provenance"
    );
}

#[test]
fn bgem_merge_records_effect_texture_provenance() {
    let mut pool = byroredux_core::string::StringPool::new();
    let path = "materials/tests/provenance.bgem";
    let mut provider = MaterialProvider::new();
    provider.insert_bgem_for_test(
        path,
        BgemFile {
            base_texture: "effect_base.dds".into(),
            glow_texture: "effect_glow.dds".into(),
            ..Default::default()
        },
    );
    let mut mesh = imported_mesh_with_material_path(&mut pool, path);

    assert!(merge_external_material(&mut mesh.material, &mut provider, &mut pool).merged());
    assert_eq!(
        mesh.material.texture_sources.base_color,
        byroredux_nif::import::ImportedTextureSource::Bgem
    );
    assert_eq!(
        mesh.material.texture_sources.emissive,
        byroredux_nif::import::ImportedTextureSource::Bgem
    );
}

/// Regression for #1077 / FO4-D6-003 Phase 1 — BGSM-only shader
/// flags (`translucency`, `model_space_normals`) must forward to
/// `ImportedMesh`'s matching sinks. Pre-fix the parser decoded both
/// and the merge dropped them on the floor.
///
/// `is_pbr` is deliberately NOT asserted here — it isn't driven by
/// `forward_bgsm_phase1_flags` at all (see `merge_external_material`'s
/// BGSM arm, and `vanilla_bgsm_resolve_promotes_imported_material_to_pbr`
/// for its dedicated coverage after #2700).
///
/// #2702 (FO4-D2-03) — calls the real production function
/// (`forward_bgsm_phase1_flags`) instead of a hand-copied mirror of
/// its gate, so this test can actually fail when `merge_external_material`
/// diverges from it.
#[test]
fn bgsm_merge_forwards_phase1_shader_flags() {
    let bgsm = BgsmFile {
        translucency: true,
        model_space_normals: true,
        ..Default::default()
    };

    let mut material = ImportedMaterial::default();
    let mut touched = false;
    forward_bgsm_phase1_flags(&mut material, &bgsm, &mut touched);

    assert!(material.has_translucency);
    assert!(material.model_space_normals);
    assert!(touched);
}

/// Regression for #2607 (FO4-D7-02) — BGSM's v<8 rim / backlight /
/// subsurface group reached `ImportedMaterial`'s long-existing sinks, which
/// `translate_material` already forwards onto the canonical `Material`. Before
/// this, every BGSM-authored surface fed the (now unconditionally live, #1352)
/// Disney subsurface/rimlight lobe hardcoded zeros no matter what the artist
/// wrote.
#[test]
fn bgsm_merge_forwards_rim_and_subsurface_when_enabled() {
    let bgsm = BgsmFile {
        rim_lighting: true,
        rim_power: 4.5,
        back_light_power: 1.25,
        subsurface_lighting: true,
        subsurface_lighting_rolloff: 0.75,
        ..Default::default()
    };

    let mut material = ImportedMaterial::default();
    let (mut set_rim, mut set_subsurface, mut touched) = (false, false, false);
    forward_bgsm_rim_subsurface(
        &mut material,
        &bgsm,
        &mut set_rim,
        &mut set_subsurface,
        &mut touched,
    );

    assert_eq!(material.rimlight_power, 4.5);
    assert_eq!(
        material.backlight_power, 1.25,
        "backlight rides the rim enable bit — the format gives it none of its own"
    );
    assert_eq!(material.subsurface_rolloff, 0.75);
    assert!(touched);
}

/// The gate half of #2607, and the load-bearing one. The parser reads this
/// whole group ONLY on the `version < 8` branch — v>=8 spends those bytes on
/// the translucency suite — so a modern BGSM arrives with `rim_power = 2.0`
/// and `subsurface_lighting_rolloff = 0.3` still at their struct defaults,
/// never having been read off disk. Forwarding those would be fabrication.
/// The enable bools are false in exactly that case.
#[test]
fn bgsm_merge_does_not_forward_never_parsed_rim_defaults() {
    // Exactly what `BgsmFile::parse` leaves behind for a v>=8 file: the
    // enables never read (so `false`), the payloads still holding the seed
    // values the parse initializer sets before the version branch — 2.0 and
    // 0.3, from `crates/bgsm/src/bgsm.rs`. Written out rather than taken from
    // `BgsmFile::default()`, which zeroes them and so would not model the
    // never-parsed case this gate exists for.
    let bgsm = BgsmFile {
        base: byroredux_bgsm::BaseMaterial {
            version: 8,
            ..Default::default()
        },
        rim_lighting: false,
        rim_power: 2.0,
        back_light_power: 0.0,
        subsurface_lighting: false,
        subsurface_lighting_rolloff: 0.3,
        ..Default::default()
    };

    let mut material = ImportedMaterial::default();
    let (mut set_rim, mut set_subsurface, mut touched) = (false, false, false);
    forward_bgsm_rim_subsurface(
        &mut material,
        &bgsm,
        &mut set_rim,
        &mut set_subsurface,
        &mut touched,
    );

    assert_eq!(material.rimlight_power, 0.0);
    assert_eq!(material.backlight_power, 0.0);
    assert_eq!(material.subsurface_rolloff, 0.0);
    assert!(
        !touched,
        "a disabled group must not count as merged authored data"
    );
}

/// Child-first precedence, matching every other payload field in the walk:
/// a closer BGSM that authored the group wins, and an ancestor cannot
/// overwrite it.
#[test]
fn bgsm_merge_rim_subsurface_honors_child_first_chain() {
    let child = BgsmFile {
        rim_lighting: true,
        rim_power: 4.0,
        subsurface_lighting: true,
        subsurface_lighting_rolloff: 0.9,
        ..Default::default()
    };
    let parent = BgsmFile {
        rim_lighting: true,
        rim_power: 99.0,
        subsurface_lighting: true,
        subsurface_lighting_rolloff: 99.0,
        ..Default::default()
    };

    let mut material = ImportedMaterial::default();
    let (mut set_rim, mut set_subsurface, mut touched) = (false, false, false);
    for bgsm in [&child, &parent] {
        forward_bgsm_rim_subsurface(
            &mut material,
            bgsm,
            &mut set_rim,
            &mut set_subsurface,
            &mut touched,
        );
    }

    assert_eq!(material.rimlight_power, 4.0, "child must win");
    assert_eq!(material.subsurface_rolloff, 0.9, "child must win");
}

/// Regression for #2608 (FO4-D7-03) — the authored env-map mask scale reaches
/// `ImportedMaterial.env_map_scale`, which `translate_material` already
/// forwards to the canonical `Material`.
#[test]
fn bgsm_merge_forwards_authored_env_map_mask_scale() {
    let bgsm = BgsmFile {
        base: byroredux_bgsm::BaseMaterial {
            version: 2,
            environment_mapping: true,
            environment_mapping_mask_scale: 2.5,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut material = ImportedMaterial::default();
    let (mut set_scale, mut touched) = (false, false);
    forward_bgsm_env_map_scale(&mut material, &bgsm, &mut set_scale, &mut touched);

    assert_eq!(material.env_map_scale, 2.5);
    assert!(touched);
}

/// The gate half of #2608. From version 10 the `(environment_mapping,
/// environment_mapping_mask_scale)` bytes became `depth_bias`, and the parser
/// substitutes a synthetic `(false, 1.0)`. That 1.0 is not authored data:
/// forwarding it would put every modern BGSM above `resolve_pbr`'s
/// `env_map_scale > 0.3` arm, fabricating reflection intent that then drives
/// real roughness on the bulk of FO4 content.
#[test]
fn bgsm_merge_does_not_forward_synthetic_v10_env_map_scale() {
    let bgsm = BgsmFile {
        base: byroredux_bgsm::BaseMaterial {
            version: 10,
            environment_mapping: false,
            // Exactly what `parse_after_magic` substitutes on the v>=10 path.
            environment_mapping_mask_scale: 1.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut material = ImportedMaterial::default();
    let before = material.env_map_scale;
    let (mut set_scale, mut touched) = (false, false);
    forward_bgsm_env_map_scale(&mut material, &bgsm, &mut set_scale, &mut touched);

    assert_eq!(
        material.env_map_scale, before,
        "a never-parsed 1.0 must not reach the canonical material"
    );
    assert!(!touched);
}

/// Companion: with both flags `false` on the BGSM, the translucency /
/// model-space-normal mesh fields must stay at their defaults (a
/// `false` author doesn't override).
///
/// `is_pbr` is out of scope here — since #2700 it is unconditional on
/// any successful BGSM resolve (`merge_external_material`'s
/// `material.is_pbr = true`, not gated on any per-file bit), so a
/// `bgsm.pbr` value has no bearing on this test's fixture at all. See
/// `vanilla_bgsm_resolve_promotes_imported_material_to_pbr` for its
/// dedicated coverage.
#[test]
fn bgsm_merge_does_not_set_phase1_flags_from_false() {
    let bgsm = BgsmFile {
        translucency: false,
        model_space_normals: false,
        ..Default::default()
    };

    let mut material = ImportedMaterial::default();
    let mut touched = false;
    forward_bgsm_phase1_flags(&mut material, &bgsm, &mut touched);

    assert!(!material.has_translucency);
    assert!(!material.model_space_normals);
    assert!(
        !touched,
        "no flag flipped → forward fn must report untouched"
    );
}

/// Child-first precedence for `forward_bgsm_phase1_flags` — first
/// authored `true` in the BGSM template chain wins, mirroring the
/// texture-slot precedence (which the parser walks child-first). A
/// `false` (unauthored) child followed by a `true` parent template
/// must still flip the flag; the child authoring `false` must not
/// permanently lock the flag off for the rest of the chain walk.
///
/// (Previously this test exercised `pbr`'s chain-walk precedence
/// instead — moot since #2700 made `is_pbr` unconditional on any
/// BGSM resolve, not walked per-field. Repurposed onto `translucency`,
/// which is still genuinely gated per-field and had no dedicated
/// precedence coverage of its own.)
#[test]
fn bgsm_merge_phase1_flags_honor_child_first_chain() {
    let child = BgsmFile {
        translucency: false, // child doesn't author it
        ..Default::default()
    };
    let parent = BgsmFile {
        translucency: true, // parent template enables it
        ..Default::default()
    };
    let resolved = ResolvedMaterial {
        file: child,
        parent: Some(Arc::new(ResolvedMaterial {
            file: parent,
            parent: None,
        })),
    };

    let mut material = ImportedMaterial::default();
    let mut touched = false;
    for step in resolved.walk() {
        forward_bgsm_phase1_flags(&mut material, &step.file, &mut touched);
    }
    assert!(
        material.has_translucency,
        "parent's translucency=true must flow down when the child doesn't author it"
    );
    assert!(!material.model_space_normals);
}

/// Regression for FO4 BGSM glass / alpha-blended decals. FO4
/// moved per-material blend state out of `NiAlphaProperty` into
/// BGSM's `base.alpha_blend_mode`. Pre-fix the merge dropped
/// that tuple, leaving `ImportedMaterial.has_alpha = false` on
/// every BGSM-only glass pane → no `AlphaBlend` component
/// attached → `INSTANCE_FLAG_ALPHA_BLEND` never set → the
/// `MATERIAL_KIND_GLASS` classifier in `static_meshes.rs`
/// short-circuited and the panel rendered fully opaque
/// (visible symptom: Institute Bioscience glass too opaque,
/// no refraction, wrong tint). Pins:
///   1. `function > 0` → `has_alpha = true` + blend factors copied
///   2. `function == 0` (None) → no override (NIF-side value wins)
///
/// The three real `(function, src, dst)` tuples below are taken
/// directly from the reference implementation's
/// `ConvertAlphaBlendMode` (`Material-Editor:BaseMaterialFile.cs:363-387`),
/// not invented — see #1823, which replaced this test's prior
/// synthetic `(function=2, src=1, dst=1)` Additive fixture (a tuple
/// the reference parser never actually emits) that had masked the
/// #1651 blend-swap regression.
#[test]
fn bgsm_merge_forwards_alpha_blend_mode() {
    use ash::vk;
    use byroredux_bgsm::AlphaBlendMode;
    use byroredux_renderer::vulkan::pipeline::gamebryo_to_vk_blend_factor;

    // Mirror the prod merge's three writes for the alpha-blend block.
    // `bgsm_blend_to_gamebryo` is a narrowing cast, not a translation
    // (#1823) — `src_blend`/`dst_blend` are already Gamebryo-native.
    fn apply(bgsm: &BgsmFile, has_alpha: &mut bool, src: &mut u8, dst: &mut u8) {
        if bgsm.base.alpha_blend_mode.function > 0 {
            *has_alpha = true;
            *src = bgsm_blend_to_gamebryo(bgsm.base.alpha_blend_mode.src_blend);
            *dst = bgsm_blend_to_gamebryo(bgsm.base.alpha_blend_mode.dst_blend);
        }
    }

    // Case 1: Standard (function=1, src=6, dst=7) — Institute glass
    // case. Must produce (SRC_ALPHA, ONE_MINUS_SRC_ALPHA).
    let mut has_alpha = false;
    let mut src = 0u8;
    let mut dst = 0u8;
    let mut bgsm = BgsmFile::default();
    bgsm.base.alpha_blend_mode = AlphaBlendMode {
        function: 1,
        src_blend: 6,
        dst_blend: 7,
    };
    apply(&bgsm, &mut has_alpha, &mut src, &mut dst);
    assert!(has_alpha, "function=1 (Standard) must set has_alpha");
    assert_eq!(src, 6);
    assert_eq!(dst, 7);
    assert_eq!(gamebryo_to_vk_blend_factor(src), vk::BlendFactor::SRC_ALPHA);
    assert_eq!(
        gamebryo_to_vk_blend_factor(dst),
        vk::BlendFactor::ONE_MINUS_SRC_ALPHA
    );

    // Case 2: Additive (function=1, src=6, dst=0) — the real reference
    // tuple for FO4 effect/glow-card BGEMs. Must produce
    // (SRC_ALPHA, ONE) — additive accumulation. #1651's swap turned
    // dst=0 into 1 (ZERO), corrupting this to an alpha-weighted
    // opaque overwrite instead of additive.
    let mut has_alpha = false;
    let mut src = 0u8;
    let mut dst = 0u8;
    let mut bgsm = BgsmFile::default();
    bgsm.base.alpha_blend_mode = AlphaBlendMode {
        function: 1,
        src_blend: 6,
        dst_blend: 0,
    };
    apply(&bgsm, &mut has_alpha, &mut src, &mut dst);
    assert!(has_alpha);
    assert_eq!(src, 6);
    assert_eq!(dst, 0);
    assert_eq!(gamebryo_to_vk_blend_factor(src), vk::BlendFactor::SRC_ALPHA);
    assert_eq!(
        gamebryo_to_vk_blend_factor(dst),
        vk::BlendFactor::ONE,
        "Additive dst must resolve to ONE for accumulation"
    );

    // Case 3: Multiplicative (function=1, src=4, dst=1) — the real
    // reference tuple. Must produce (DST_COLOR, ZERO) — dst *= src.
    // #1651's swap turned dst=1 into 0 (ONE), leaking the destination
    // through instead of multiplying it out.
    let mut has_alpha = false;
    let mut src = 0u8;
    let mut dst = 0u8;
    let mut bgsm = BgsmFile::default();
    bgsm.base.alpha_blend_mode = AlphaBlendMode {
        function: 1,
        src_blend: 4,
        dst_blend: 1,
    };
    apply(&bgsm, &mut has_alpha, &mut src, &mut dst);
    assert!(has_alpha);
    assert_eq!(src, 4);
    assert_eq!(dst, 1);
    assert_eq!(gamebryo_to_vk_blend_factor(src), vk::BlendFactor::DST_COLOR);
    assert_eq!(
        gamebryo_to_vk_blend_factor(dst),
        vk::BlendFactor::ZERO,
        "Multiplicative dst must resolve to ZERO so it doesn't leak through"
    );

    // Case 4: function=0 (None) — the BGSM explicitly says "no
    // blend." Don't flip has_alpha. Caller's `set_blend` guard
    // then also prevents a subsequent parent from re-triggering.
    let mut has_alpha = false;
    let mut src = 6u8;
    let mut dst = 7u8;
    let bgsm = BgsmFile::default();
    assert_eq!(bgsm.base.alpha_blend_mode.function, 0);
    apply(&bgsm, &mut has_alpha, &mut src, &mut dst);
    assert!(!has_alpha, "function=0 must NOT set has_alpha");
    assert_eq!(src, 6, "src untouched when function=0");
    assert_eq!(dst, 7);
}

/// #1823 — `bgsm_blend_to_gamebryo` performs no translation, only a
/// `u32 -> u8` narrowing cast. Pins the identity contract across the
/// full valid range (0..=10), specifically including `0`/`1` — the
/// #1651 regression swapped exactly those two, which corrupted the
/// real Additive (`dst=0`) and Multiplicative (`dst=1`) blend modes
/// (see `bgsm_merge_forwards_alpha_blend_mode`). Shared by the BGSM
/// and BGEM merge branches, so this pins the contract for both.
#[test]
fn bgsm_blend_to_gamebryo_is_identity_narrowing() {
    for v in 0u32..=10 {
        assert_eq!(
            bgsm_blend_to_gamebryo(v),
            v as u8,
            "must pass {v} through unchanged, no 0/1 swap"
        );
    }
}

/// Companion regression for the SIBLING half of #1076 — BGEM also
/// authors `specular_texture` and `lighting_texture` (BGEM does
/// not author flow / wrinkles per `bgem.rs`). Pre-fix the BGEM
/// merge dropped both, leaving FO4 effect shaders that authored
/// a per-texel specular layer rendering on NIF-fallback specular.
#[test]
fn bgem_merge_forwards_specular_and_lighting_slots() {
    let mut specular_map: Option<String> = None;
    let mut lighting_map: Option<String> = None;

    let bgem = BgemFile {
        specular_texture: "fx_specular.dds".into(),
        lighting_texture: "fx_lighting.dds".into(),
        ..Default::default()
    };

    fill(&mut specular_map, &bgem.specular_texture);
    fill(&mut lighting_map, &bgem.lighting_texture);

    assert_eq!(specular_map.as_deref(), Some("fx_specular.dds"));
    assert_eq!(lighting_map.as_deref(), Some("fx_lighting.dds"));
}

#[test]
fn legacy_bgem_transmissive_feature_bundle_selects_shared_glass() {
    use byroredux_bgsm::{AlphaBlendMode, BaseMaterial};

    // Vanilla FO4 PortADiner02.bgem (v2) predates `glass_enabled` but
    // authors the dome as a hard transparent, environment-mapped shell.
    let bgem = BgemFile {
        base: BaseMaterial {
            version: 2,
            alpha: 0.9,
            alpha_blend_mode: AlphaBlendMode {
                function: 1,
                src_blend: 6,
                dst_blend: 7,
            },
            z_buffer_write: false,
            z_buffer_test: true,
            two_sided: true,
            non_occluder: true,
            environment_mapping: true,
            ..Default::default()
        },
        envmap_texture: "Shared/Cubemaps/mipblur_DefaultOutside1.dds".into(),
        envmap_mask_texture: "SetDressing/FoodVendingMachines/PortADiner02_s.dds".into(),
        normal_texture: "SetDressing/FoodVendingMachines/PortADiner02_n.dds".into(),
        effect_lighting_enabled: true,
        falloff_enabled: true,
        ..Default::default()
    };

    assert!(bgem_uses_glass_behavior(&bgem));
    assert!(
        bgem_uses_thin_glass_behavior(&bgem),
        "Port-A-Diner's non-occluding dome must select thin shared glass"
    );
}

/// Regression for #2358: BGEM v10-v20 moved environment mapping out of the
/// shared prefix and into the subclass section. The classifier must consume
/// that modern field instead of the shared copy, which parses as false.
#[test]
fn v20_bgem_transmissive_bundle_reads_subclass_environment_mapping() {
    use byroredux_bgsm::{AlphaBlendMode, BaseMaterial};

    let bgem = BgemFile {
        base: BaseMaterial {
            version: 20,
            alpha: 0.9,
            alpha_blend_mode: AlphaBlendMode {
                function: 1,
                src_blend: 6,
                dst_blend: 7,
            },
            z_buffer_write: false,
            z_buffer_test: true,
            two_sided: true,
            non_occluder: true,
            environment_mapping: false,
            ..Default::default()
        },
        environment_mapping: true,
        envmap_texture: "Shared/Cubemaps/mipblur_DefaultOutside1.dds".into(),
        envmap_mask_texture: "Effects/Glass/glassmask.dds".into(),
        normal_texture: "Effects/Glass/glass_n.dds".into(),
        effect_lighting_enabled: true,
        falloff_enabled: true,
        ..Default::default()
    };

    assert!(bgem_uses_glass_behavior(&bgem));
    assert!(bgem_uses_thin_glass_behavior(&bgem));
}

/// Regression for #2700 (FO4-D2-01) — any successful BGSM resolve must
/// promote `ImportedMaterial.is_pbr`, matching #1352's original,
/// still-documented contract. `a0f75fc5` narrowed this to an explicit
/// `pbr` bit walked across the resolved template chain — measured live
/// at 0 of 6,616 vanilla FO4 BGSMs, so every vanilla surface silently
/// reverted to the legacy Lambert diffuse lobe with a green test suite
/// (the two prior tests this replaced) pinning exactly that regressed
/// behaviour as correct. Exercises the real `merge_external_material`
/// path against a plain vanilla-shaped BGSM fixture — neither the leaf
/// nor its template parent sets `pbr` — not a mirror of the gate.
#[test]
fn vanilla_bgsm_resolve_promotes_imported_material_to_pbr() {
    let mut pool = byroredux_core::string::StringPool::new();
    let path = "materials/tests/vanilla.bgsm";
    let mut provider = MaterialProvider::new();
    provider.insert_bgsm_for_test(
        path,
        ResolvedMaterial {
            file: BgsmFile {
                pbr: false, // vanilla FO4 content never sets this
                ..Default::default()
            },
            parent: Some(Arc::new(ResolvedMaterial {
                file: BgsmFile {
                    pbr: false,
                    ..Default::default()
                },
                parent: None,
            })),
        },
    );
    let mut mesh = imported_mesh_with_material_path(&mut pool, path);
    assert!(!mesh.material.is_pbr);

    assert!(merge_external_material(&mut mesh.material, &mut provider, &mut pool).merged());
    assert!(
        mesh.material.is_pbr,
        "any successful BGSM resolve must promote is_pbr regardless of the bgsm.pbr bit \
         (#1352 / #2700) — vanilla FO4 content never sets that bit, so gating on it left \
         is_pbr false on 100% of vanilla surfaces"
    );
}

/// Regression for #2366: the parsed v20+ BGEM PBR opt-in must reach the
/// canonical imported-material flag through the real merge path.
#[test]
fn bgem_effect_pbr_specular_promotes_imported_material_to_pbr() {
    let mut pool = byroredux_core::string::StringPool::new();
    let path = "materials/tests/effect_pbr.bgem";
    let mut provider = MaterialProvider::new();
    provider.insert_bgem_for_test(
        path,
        BgemFile {
            base: byroredux_bgsm::BaseMaterial {
                version: 20,
                ..Default::default()
            },
            effect_pbr_specular: true,
            ..Default::default()
        },
    );
    let mut mesh = imported_mesh_with_material_path(&mut pool, path);
    assert!(!mesh.material.is_pbr);

    assert!(merge_external_material(&mut mesh.material, &mut provider, &mut pool,).merged());
    assert!(
        mesh.material.is_pbr,
        "BGEM effect_pbr_specular=true must promote ImportedMaterial.is_pbr"
    );
}

/// Regression for #2108 (SF-D9-01), real merge path: a BGEM that fills the
/// greyscale LUT slot WITHOUT authoring either enable bit
/// (`grayscale_to_palette_color` / `grayscale_to_palette_alpha`) must leave
/// `bgsm_greyscale_lut_enabled = false` — the slot is a legal,
/// always-serialized field, not itself an enable signal. `cell_loader.rs`'s
/// `pack_imported_material_flags` gates `EFFECT_PALETTE_COLOR`/`ALPHA` on
/// this bit, not on the texture's presence.
#[test]
fn bgem_merge_leaves_palette_disabled_when_neither_enable_bit_is_authored() {
    let mut pool = byroredux_core::string::StringPool::new();
    let path = "materials/tests/disabled_palette.bgem";
    let mut provider = MaterialProvider::new();
    provider.insert_bgem_for_test(
        path,
        BgemFile {
            grayscale_texture: "textures\\effects\\gradients\\fire.dds".into(),
            grayscale_to_palette_alpha: false,
            base: byroredux_bgsm::BaseMaterial {
                grayscale_to_palette_color: false,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let mut mesh = imported_mesh_with_material_path(&mut pool, path);

    assert!(merge_external_material(&mut mesh.material, &mut provider, &mut pool,).merged());
    assert!(
        mesh.material.textures.greyscale_lut.is_some(),
        "the LUT texture slot must still fill — this fix does not touch texture forwarding"
    );
    assert!(
        !mesh.material.bgsm_greyscale_lut_enabled,
        "neither grayscale_to_palette_color nor _alpha was authored — the \
         remap must stay disabled despite the filled LUT slot (#2108)"
    );
}

/// Regression for #2609 (FO4-D7-04) — `from_bgsm` and
/// `bgsm_pbr_scalars_authored` mean different things, and the BGEM arm is
/// where they diverge.
///
/// Pre-split, `from_bgsm` was set identically on both arms while only the
/// BGSM arm populated `metalness_override`/`roughness_override`, so any
/// consumer reading it as "PBR scalars authored" — which is precisely what
/// #2606's fix needed — would be wrong on every effect material. The BGEM arm
/// keeps `from_bgsm` (it is load-bearing for the FO4-vs-Skyrim glass-keyword
/// discriminator, #2710) and must leave the scalar flag clear.
#[test]
fn bgem_merge_sets_provenance_but_not_the_scalar_authored_flag() {
    let mut pool = byroredux_core::string::StringPool::new();
    let path = "materials/tests/scalar_flag.bgem";
    let mut provider = MaterialProvider::new();
    provider.insert_bgem_for_test(path, BgemFile::default());
    let mut mesh = imported_mesh_with_material_path(&mut pool, path);

    assert!(merge_external_material(&mut mesh.material, &mut provider, &mut pool).merged());

    assert!(
        mesh.material.from_bgsm,
        "provenance stays set on the BGEM arm — #2710's glass discriminator \
         depends on it"
    );
    assert!(
        !mesh.material.bgsm_pbr_scalars_authored,
        "BGEM authors no smoothness/specular, so it must not claim to have \
         authored PBR scalars (#2609)"
    );
    assert_eq!(
        (
            mesh.material.metalness_override,
            mesh.material.roughness_override
        ),
        (None, None),
        "the flag's meaning must match what the arm actually populated"
    );
}

/// The other half of #2609, and the one that keeps #2606's fix from becoming
/// a silent no-op: the BGSM arm must SET `bgsm_pbr_scalars_authored`, right
/// where it populates the two overrides the flag describes. If that write went
/// missing, every FO4 BGSM would fall back through the #2606 gate again with
/// nothing to catch it.
///
/// Drives the real `merge_external_material` BGSM arm via the seeded template
/// cache — not a mirror of its loop.
#[test]
fn bgsm_merge_sets_the_scalar_authored_flag_alongside_the_overrides() {
    let mut pool = byroredux_core::string::StringPool::new();
    let path = "materials/tests/scalar_flag.bgsm";
    let mut provider = MaterialProvider::new();
    provider.insert_bgsm_for_test(
        path,
        ResolvedMaterial {
            file: BgsmFile {
                smoothness: 0.8,
                specular_color: [1.0, 1.0, 1.0],
                specular_mult: 1.0,
                ..Default::default()
            },
            parent: None,
        },
    );
    let mut mesh = imported_mesh_with_material_path(&mut pool, path);

    assert!(merge_external_material(&mut mesh.material, &mut provider, &mut pool).merged());

    assert!(mesh.material.from_bgsm, "provenance");
    assert!(
        mesh.material.bgsm_pbr_scalars_authored,
        "the BGSM arm authored metalness/roughness, so the flag gating \
         #2606's overwrite must be set"
    );
    assert!(mesh.material.metalness_override.is_some());
    assert!(
        mesh.material.roughness_override.is_some(),
        "roughness = 1 - smoothness is exactly what #2606 protects"
    );
    // 1 - 0.8, clamped to the renderer's [0.04, 1.0] band.
    let roughness = mesh.material.roughness_override.unwrap();
    assert!(
        (roughness - 0.2).abs() < 1e-5,
        "authored smoothness must drive roughness; got {roughness}"
    );
}

/// #3639 — `smoothness == 1.0` lowers `roughness` to the renderer's 0.04
/// clamp floor (near-mirror dielectric). `triangle.frag` only modulates that
/// back up via the per-texel gloss map; with no `smooth_spec_texture`
/// anywhere in the template chain there's nothing to modulate with, so the
/// merge must fall back to a neutral roughness instead of leaving the
/// material pinned at the floor with no per-pixel escape.
#[test]
fn bgsm_merge_falls_back_to_neutral_roughness_when_smoothness_is_one_with_no_gloss_map() {
    let mut pool = byroredux_core::string::StringPool::new();
    let path = "materials/tests/mirror_no_gloss.bgsm";
    let mut provider = MaterialProvider::new();
    provider.insert_bgsm_for_test(
        path,
        ResolvedMaterial {
            file: BgsmFile {
                smoothness: 1.0,
                specular_color: [1.0, 1.0, 1.0],
                specular_mult: 1.0,
                smooth_spec_texture: String::new(),
                ..Default::default()
            },
            parent: None,
        },
    );
    let mut mesh = imported_mesh_with_material_path(&mut pool, path);

    assert!(merge_external_material(&mut mesh.material, &mut provider, &mut pool).merged());

    assert_eq!(
        mesh.material.roughness_override,
        Some(0.5),
        "no gloss map anywhere in the chain to recover the floor with — must \
         fall back to the neutral roughness classify_pbr's keyword arms use, \
         not stay pinned at 0.04"
    );
}

/// Sibling of the fallback test above: when a gloss map DOES resolve (even
/// from a template parent, not just the leaf), the shader has per-pixel
/// information to modulate with, so the merge must NOT override the
/// authored 0.04 floor.
#[test]
fn bgsm_merge_keeps_the_floor_when_smoothness_is_one_and_a_gloss_map_resolves() {
    let mut pool = byroredux_core::string::StringPool::new();
    let path = "materials/tests/mirror_with_gloss.bgsm";
    let mut provider = MaterialProvider::new();
    provider.insert_bgsm_for_test(
        path,
        ResolvedMaterial {
            file: BgsmFile {
                smoothness: 1.0,
                specular_color: [1.0, 1.0, 1.0],
                specular_mult: 1.0,
                smooth_spec_texture: "textures/tests/gloss.dds".to_string(),
                ..Default::default()
            },
            parent: None,
        },
    );
    let mut mesh = imported_mesh_with_material_path(&mut pool, path);

    assert!(merge_external_material(&mut mesh.material, &mut provider, &mut pool).merged());

    let roughness = mesh
        .material
        .roughness_override
        .expect("BGSM arm must still author roughness");
    assert!(
        (roughness - 0.04).abs() < 1e-5,
        "a resolvable gloss map means the shader CAN modulate the floor back \
         up per-pixel — the merge must leave it at 0.04, got {roughness}"
    );
}

/// End-to-end companion for #2607/#2608 through the real merge: an authored
/// v<10 BGSM carrying rim, subsurface and env-mask values lands all of them on
/// `ImportedMaterial`. The extracted-function tests above pin the gates; this
/// pins that the merge loop actually calls them.
#[test]
fn bgsm_merge_forwards_rim_subsurface_and_env_scale_end_to_end() {
    let mut pool = byroredux_core::string::StringPool::new();
    let path = "materials/tests/rim_env.bgsm";
    let mut provider = MaterialProvider::new();
    provider.insert_bgsm_for_test(
        path,
        ResolvedMaterial {
            file: BgsmFile {
                rim_lighting: true,
                rim_power: 3.5,
                back_light_power: 0.5,
                subsurface_lighting: true,
                subsurface_lighting_rolloff: 0.6,
                base: byroredux_bgsm::BaseMaterial {
                    version: 2,
                    environment_mapping: true,
                    environment_mapping_mask_scale: 1.75,
                    ..Default::default()
                },
                ..Default::default()
            },
            parent: None,
        },
    );
    let mut mesh = imported_mesh_with_material_path(&mut pool, path);

    assert!(merge_external_material(&mut mesh.material, &mut provider, &mut pool).merged());

    assert_eq!(mesh.material.rimlight_power, 3.5);
    assert_eq!(mesh.material.backlight_power, 0.5);
    assert_eq!(mesh.material.subsurface_rolloff, 0.6);
    assert_eq!(mesh.material.env_map_scale, 1.75);
}

/// The `grayscale_to_palette_color` enable bit alone (no alpha variant)
/// must forward to `bgsm_greyscale_lut_enabled = true` via the real merge
/// path, matching `pack_imported_material_flags`'s color-default branch.
#[test]
fn bgem_merge_forwards_color_enable_bit_via_real_merge() {
    let mut pool = byroredux_core::string::StringPool::new();
    let path = "materials/tests/color_palette.bgem";
    let mut provider = MaterialProvider::new();
    provider.insert_bgem_for_test(
        path,
        BgemFile {
            grayscale_texture: "textures\\effects\\gradients\\electricity.dds".into(),
            grayscale_to_palette_alpha: false,
            base: byroredux_bgsm::BaseMaterial {
                grayscale_to_palette_color: true,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let mut mesh = imported_mesh_with_material_path(&mut pool, path);

    assert!(merge_external_material(&mut mesh.material, &mut provider, &mut pool,).merged());
    assert!(
        mesh.material.bgsm_greyscale_lut_enabled,
        "grayscale_to_palette_color=true must forward to bgsm_greyscale_lut_enabled"
    );
    assert!(
        !mesh.material.bgsm_greyscale_lut_is_alpha,
        "color-only enable must not also select the alpha variant"
    );
}

/// Regression for #2643 (SF-D9-2026-08-07-04), real merge path: a BGEM
/// authoring BOTH the shared `grayscale_to_palette_color` bit and its own
/// `grayscale_to_palette_alpha` bit at once (the format permits this) must
/// forward BOTH `bgsm_greyscale_lut_color` and `bgsm_greyscale_lut_is_alpha`
/// as true, so `pack_imported_material_flags` packs both
/// `EFFECT_PALETTE_COLOR` and `EFFECT_PALETTE_ALPHA` instead of losing the
/// color variant to a mutually-exclusive choice.
#[test]
fn bgem_merge_forwards_both_palette_bits_when_both_authored() {
    let mut pool = byroredux_core::string::StringPool::new();
    let path = "materials/tests/both_palette_bits.bgem";
    let mut provider = MaterialProvider::new();
    provider.insert_bgem_for_test(
        path,
        BgemFile {
            grayscale_texture: "textures\\effects\\gradients\\fire.dds".into(),
            grayscale_to_palette_alpha: true,
            base: byroredux_bgsm::BaseMaterial {
                grayscale_to_palette_color: true,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let mut mesh = imported_mesh_with_material_path(&mut pool, path);

    assert!(merge_external_material(&mut mesh.material, &mut provider, &mut pool,).merged());
    assert!(
        mesh.material.bgsm_greyscale_lut_enabled,
        "either enable bit alone must already enable the remap"
    );
    assert!(
        mesh.material.bgsm_greyscale_lut_color,
        "grayscale_to_palette_color=true must forward even when the alpha bit is also set (#2643)"
    );
    assert!(
        mesh.material.bgsm_greyscale_lut_is_alpha,
        "grayscale_to_palette_alpha=true must forward even when the color bit is also set (#2643)"
    );
}

/// Regression for #2643 (SF-D9-2026-08-07-04), real merge path: a BGEM
/// authoring `envmap_texture`/`envmap_mask_texture` but leaving the
/// version-appropriate `env_mapping_enabled()` bit off must NOT fill
/// `textures.environment`/`environment_mask`. Pre-fix these filled
/// unconditionally, ignoring the same enable bit
/// `bgem_uses_glass_behavior` already consults.
#[test]
fn bgem_merge_skips_envmap_fill_when_env_mapping_disabled() {
    let mut pool = byroredux_core::string::StringPool::new();
    let path = "materials/tests/envmap_disabled.bgem";
    let mut provider = MaterialProvider::new();
    provider.insert_bgem_for_test(
        path,
        BgemFile {
            base: byroredux_bgsm::BaseMaterial {
                environment_mapping: false, // v2 default reads the shared bit
                ..Default::default()
            },
            envmap_texture: "Shared/Cubemaps/mipblur_DefaultOutside1.dds".into(),
            envmap_mask_texture: "Effects/Glass/glassmask.dds".into(),
            ..Default::default()
        },
    );
    let mut mesh = imported_mesh_with_material_path(&mut pool, path);

    assert!(merge_external_material(&mut mesh.material, &mut provider, &mut pool,).merged());
    assert!(
        mesh.material.textures.environment.is_none(),
        "env_mapping_enabled()==false must skip the environment texture fill (#2643)"
    );
    assert!(
        mesh.material.textures.environment_mask.is_none(),
        "env_mapping_enabled()==false must skip the environment mask fill (#2643)"
    );
}

/// Sibling of the above with the enable bit ON — the envmap textures must
/// still fill via the real merge path.
#[test]
fn bgem_merge_fills_envmap_when_env_mapping_enabled() {
    let mut pool = byroredux_core::string::StringPool::new();
    let path = "materials/tests/envmap_enabled.bgem";
    let mut provider = MaterialProvider::new();
    provider.insert_bgem_for_test(
        path,
        BgemFile {
            base: byroredux_bgsm::BaseMaterial {
                environment_mapping: true,
                ..Default::default()
            },
            envmap_texture: "Shared/Cubemaps/mipblur_DefaultOutside1.dds".into(),
            envmap_mask_texture: "Effects/Glass/glassmask.dds".into(),
            ..Default::default()
        },
    );
    let mut mesh = imported_mesh_with_material_path(&mut pool, path);

    assert!(merge_external_material(&mut mesh.material, &mut provider, &mut pool,).merged());
    assert!(
        mesh.material.textures.environment.is_some(),
        "env_mapping_enabled()==true must fill the environment texture (#2643)"
    );
    assert!(
        mesh.material.textures.environment_mask.is_some(),
        "env_mapping_enabled()==true must fill the environment mask (#2643)"
    );
}

#[test]
fn closed_bgem_glass_does_not_select_thin_surface_behavior() {
    let bgem = BgemFile {
        glass_enabled: true,
        base: byroredux_bgsm::BaseMaterial {
            non_occluder: false,
            ..Default::default()
        },
        ..Default::default()
    };

    assert!(bgem_uses_glass_behavior(&bgem));
    assert!(!bgem_uses_thin_glass_behavior(&bgem));
}

#[test]
fn bgem_glass_forwards_authored_optics_and_overlay_roles() {
    let mut pool = byroredux_core::string::StringPool::new();
    let path = "materials/tests/authored_glass.bgem";
    let mut provider = MaterialProvider::new();
    provider.insert_bgem_for_test(
        path,
        BgemFile {
            glass_enabled: true,
            glass_fresnel_color: [0.2, 0.4, 0.8],
            glass_refraction_scale_base: 0.09,
            glass_blur_scale_base: 0.25,
            glass_blur_scale_factor: 1.75,
            glass_roughness_scratch: "Effects/Glass/scratches.dds".into(),
            glass_dirt_overlay: "Effects/Glass/dirt.dds".into(),
            ..Default::default()
        },
    );
    let mut mesh = imported_mesh_with_material_path(&mut pool, path);

    assert!(merge_external_material(&mut mesh.material, &mut provider, &mut pool).merged());
    assert_eq!(mesh.material.glass_fresnel_color, [0.2, 0.4, 0.8]);
    assert_eq!(mesh.material.glass_refraction_scale, 0.09);
    assert_eq!(mesh.material.glass_blur_scale, 0.25);
    assert_eq!(mesh.material.glass_blur_scale_factor, 1.75);
    assert_eq!(
        mesh.material
            .textures
            .glass_roughness_scratch
            .and_then(|value| pool.resolve(value)),
        Some("effects/glass/scratches.dds")
    );
    assert_eq!(
        mesh.material
            .textures
            .glass_dirt_overlay
            .and_then(|value| pool.resolve(value)),
        Some("effects/glass/dirt.dds")
    );
}

#[test]
fn legacy_bgem_effect_cards_do_not_become_glass() {
    use byroredux_bgsm::{AlphaBlendMode, BaseMaterial};

    let fire = BgemFile {
        base: BaseMaterial {
            version: 2,
            alpha: 0.8,
            alpha_blend_mode: AlphaBlendMode {
                function: 1,
                src_blend: 6,
                dst_blend: 0,
            },
            z_buffer_write: false,
            two_sided: true,
            non_occluder: true,
            ..Default::default()
        },
        grayscale_texture: "Effects/Gradients/FireGradient.dds".into(),
        soft_enabled: true,
        ..Default::default()
    };

    assert!(!bgem_uses_glass_behavior(&fire));
}

/// Regression for #2626 (SF-D9-2026-08-07-01) — `base.refraction` is a
/// shared `BaseMaterial` screen-distortion bit (heat shimmer, cloaking
/// shells, force-field ripple, fire/plasma distortion), not a glass
/// signal, and it isn't version-gated the way `glass_enabled` is.
/// Pre-fix `refraction=true` alone short-circuited straight to glass
/// regardless of everything else — the issue's own fixture shape: no
/// `glass_enabled`, no lit-fresnel-falloff signal, no envmap stack.
#[test]
fn refraction_alone_does_not_select_glass_behavior() {
    use byroredux_bgsm::BaseMaterial;

    let distortion_card = BgemFile {
        base: BaseMaterial {
            refraction: true,
            ..Default::default()
        },
        effect_lighting_enabled: false,
        ..Default::default()
    };

    assert!(
        !bgem_uses_glass_behavior(&distortion_card),
        "a bare refraction bit (heat shimmer / cloaking / fire distortion) \
         must not classify as glass"
    );
}

/// Regression for #1358 — BGEM `base_color` / `base_color_scale` must
/// forward to `mesh.material.emissive_color` / `mesh.material.emissive_mult` with
/// `emissive_source = EmissiveSource::Effect`. Pre-fix the BGEM merge
/// set `emissive_color = bgem.emittance_color` (a separate v≥11
/// additive glow) and left `emissive_mult = 0.0` and
/// `emissive_source = None`, causing all FO4 effect surfaces (fire,
/// electricity, plasma, neon signs) to render white instead of their
/// authored tint.
#[test]
fn bgem_merge_forwards_base_color_as_emissive() {
    use byroredux_bgsm::BgemFile;
    use byroredux_core::ecs::components::material::EmissiveSource;

    let bgem = BgemFile {
        base_color: [0.8, 0.2, 0.1],
        base_color_scale: 2.5,
        emittance_color: [0.0, 1.0, 0.0], // distinct — must NOT be forwarded
        ..Default::default()
    };

    // Mirror the prod assignment from the BGEM branch.
    let emissive_color = bgem.base_color;
    let emissive_mult = bgem.base_color_scale;
    let emissive_source = EmissiveSource::Effect;

    assert_eq!(emissive_color, [0.8, 0.2, 0.1]);
    assert!((emissive_mult - 2.5).abs() < f32::EPSILON);
    assert!(
        matches!(emissive_source, EmissiveSource::Effect),
        "BGEM emissive_source must be Effect, not Material or Lighting"
    );
    // emittance_color must NOT be used as the primary emissive
    assert_ne!(emissive_color, bgem.emittance_color);
}

/// Regression for the FO4 HalluciGen gas-lab white-out — BGEM
/// `soft`/`soft_depth` must forward to `mesh.material.effect_shader` so
/// `material_translate` builds `soft_falloff_depth` + MAT_FLAG_EFFECT_SOFT
/// for the soft-particle depth fade in triangle.frag. Pre-fix only the NIF
/// `BSEffectShaderProperty` path populated these, so every FO4 BGEM
/// mist / steam / beam volume (`soft = true` in the authored file)
/// rendered with no depth feather and stacked to an opaque white-out.
#[test]
fn bgem_merge_forwards_soft_particle_depth() {
    use byroredux_bgsm::BgemFile;
    use byroredux_nif::import::BsEffectShaderData;
    use byroredux_renderer::vulkan::material::material_flag::EFFECT_SOFT;

    let bgem = BgemFile {
        soft_enabled: true,
        soft_depth: 200.0,
        effect_lighting_enabled: true,
        lighting_influence: 1.0,
        falloff_start_angle: 0.5,
        falloff_stop_angle: 0.2,
        falloff_start_opacity: 0.9,
        falloff_stop_opacity: 0.1,
        ..Default::default()
    };

    // Mirror the prod assignment from the BGEM branch.
    let es = BsEffectShaderData {
        falloff_start_angle: bgem.falloff_start_angle,
        falloff_stop_angle: bgem.falloff_stop_angle,
        falloff_start_opacity: bgem.falloff_start_opacity,
        falloff_stop_opacity: bgem.falloff_stop_opacity,
        soft_falloff_depth: bgem.soft_depth,
        effect_soft: bgem.soft_enabled,
        effect_lit: bgem.effect_lighting_enabled,
        lighting_influence: (bgem.lighting_influence.clamp(0.0, 1.0) * 255.0).round() as u8,
        ..Default::default()
    };

    assert!(
        (es.soft_falloff_depth - 200.0).abs() < f32::EPSILON,
        "soft_depth must forward to soft_falloff_depth"
    );
    assert!(es.effect_soft, "soft_enabled must map to effect_soft");
    assert_eq!(
        es.lighting_influence, 255,
        "1.0 influence → 255 on u8 payload"
    );
    let flags = crate::cell_loader::pack_effect_shader_flags(Some(&es));
    assert_ne!(
        flags & EFFECT_SOFT,
        0,
        "EFFECT_SOFT must be packed so the shader's soft-fade branch fires"
    );
}

/// Regression for #1585 / F6 — `geometry_csg` must open + resolve the
/// `<Plugin> - Geometry.csg` companion ONCE per plugin across N precombine
/// cell-loads, caching even the negative (no-CSG) result so a plugin
/// without a companion blob isn't re-stat'd on every cell. Pre-fix
/// `spawn_precombined_meshes` called `open_geometry_csg` unconditionally
/// per cell, re-parsing the chunk table and discarding the warm zlib cache.
#[test]
fn geometry_csg_caches_result_across_cell_loads() {
    let mut mp = MaterialProvider::new();
    // No companion `… - Geometry.csg` exists beside this path → None.
    let plugin = "/nonexistent/does-not-exist/Fallout4.esm";

    assert!(mp.geometry_csg(plugin).is_none());
    assert_eq!(
        mp.csg_cache.len(),
        1,
        "the negative result is cached under the plugin key"
    );
    // A second (and Nth) precombine cell-load is a pure cache hit — no
    // re-open, no re-stat, no chunk-table re-parse.
    assert!(mp.geometry_csg(plugin).is_none());
    assert_eq!(
        mp.csg_cache.len(),
        1,
        "second call hits cache; no new probe of the missing CSG"
    );
}

/// Regression for #1453 — BGEM `grayscale_texture` (palette/gradient LUT
/// for fire-gradient, electricity-gradient, magic VFX) must forward to
/// `mesh.bgsm_greyscale_lut_path`. Pre-fix the field was silently dropped,
/// so effect materials that relied on a colour-ramp palette rendered
/// without the LUT lookup.
#[test]
fn bgem_merge_forwards_grayscale_texture_as_lut_path() {
    use byroredux_bgsm::BgemFile;

    let bgem = BgemFile {
        grayscale_texture: "textures\\effects\\gradients\\fire_gradient.dds".into(),
        ..Default::default()
    };

    // Mirror the prod assignment from the BGEM branch.
    let mut lut_path: Option<String> = None;
    if lut_path.is_none() && !bgem.grayscale_texture.is_empty() {
        lut_path = Some(bgem.grayscale_texture.clone());
    }

    assert_eq!(
        lut_path.as_deref(),
        Some("textures\\effects\\gradients\\fire_gradient.dds"),
        "BGEM grayscale_texture must be forwarded to bgsm_greyscale_lut_path"
    );

    // An empty grayscale_texture must NOT overwrite an already-set path.
    let bgem_empty = BgemFile {
        grayscale_texture: String::new(),
        ..Default::default()
    };
    let original_path = lut_path.clone();
    if lut_path.is_none() && !bgem_empty.grayscale_texture.is_empty() {
        lut_path = Some(bgem_empty.grayscale_texture.clone());
    }
    assert_eq!(
        lut_path, original_path,
        "empty texture must not clobber existing path"
    );
}

/// Regression for #1580 — BGEM's `grayscale_to_palette_alpha` bool must
/// forward alongside the LUT path so `pack_imported_material_flags` (in
/// `cell_loader.rs`) can pick `EFFECT_PALETTE_ALPHA` over the color
/// default. Pre-fix the bool had zero consumers outside the parser.
#[test]
fn bgem_merge_forwards_grayscale_to_palette_alpha_bool() {
    use byroredux_bgsm::BgemFile;

    let bgem = BgemFile {
        grayscale_texture: "textures\\effects\\gradients\\electricity.dds".into(),
        grayscale_to_palette_alpha: true,
        ..Default::default()
    };

    // Mirror the prod assignment from the BGEM branch.
    let mut lut_path: Option<String> = None;
    let mut lut_is_alpha = false;
    if lut_path.is_none() && !bgem.grayscale_texture.is_empty() {
        lut_path = Some(bgem.grayscale_texture.clone());
        lut_is_alpha = bgem.grayscale_to_palette_alpha;
    }

    assert_eq!(
        lut_path.as_deref(),
        Some("textures\\effects\\gradients\\electricity.dds")
    );
    assert!(
        lut_is_alpha,
        "grayscale_to_palette_alpha=true must forward to bgsm_greyscale_lut_is_alpha"
    );

    // BGSM never authors an alpha variant — a BGSM-only path stays color.
    let bgem_color_only = BgemFile {
        grayscale_texture: "textures\\effects\\gradients\\fire.dds".into(),
        grayscale_to_palette_alpha: false,
        ..Default::default()
    };
    let mut lut_path2: Option<String> = None;
    let mut lut_is_alpha2 = false;
    if lut_path2.is_none() && !bgem_color_only.grayscale_texture.is_empty() {
        lut_path2 = Some(bgem_color_only.grayscale_texture.clone());
        lut_is_alpha2 = bgem_color_only.grayscale_to_palette_alpha;
    }
    assert!(lut_path2.is_some());
    assert!(
        !lut_is_alpha2,
        "default BGEM/BGSM path must stay the color variant"
    );
}

/// Every failing-to-resolve path logs at most once, so a broken
/// material in a 1000-REFR cell doesn't spam the log.
#[test]
fn failed_path_set_dedups_warnings() {
    let mut provider = MaterialProvider::new();
    // No archives registered → every resolve_bgsm fails at the
    // archive read step. The failed_paths set grows on the first
    // call only.
    let before = provider.failed_paths.len();
    let _ = provider.resolve_bgsm("materials/absent.bgsm");
    let _ = provider.resolve_bgsm("materials/absent.bgsm");
    let _ = provider.resolve_bgsm("materials/absent.bgsm");
    let after = provider.failed_paths.len();
    assert_eq!(after, before + 1);
}

/// Regression for #2601 — a BGSM resolve failure reached through the
/// real `merge_external_material` path (not `resolve_bgsm` called
/// directly, as the sibling test above does) must be diagnosably
/// distinct from "material is authored non-PBR": both silently return
/// `MergeOutcome::Unresolved` and leave the mesh at its NIF-native
/// keyword-classified fallback, but only a genuine resolve failure
/// should populate `MaterialProvider::failed_paths` — the queryable,
/// deduplicated record a developer can inspect (via a future debug
/// surface, or directly in-process) to confirm "this mesh's chrome
/// look is a broken BGSM reference, not intentional legacy authoring".
#[test]
fn merge_external_material_records_bgsm_resolve_failure() {
    let mut pool = byroredux_core::string::StringPool::new();
    // No archives registered on this provider — the path is dispatched
    // as BGSM (by extension) but can never resolve.
    let path = "materials/tests/broken.bgsm";
    let mut provider = MaterialProvider::new();
    let mut mesh = imported_mesh_with_material_path(&mut pool, path);

    assert!(provider.failed_paths.is_empty());
    let outcome = merge_external_material(&mut mesh.material, &mut provider, &mut pool);
    assert_eq!(outcome, MergeOutcome::Unresolved);
    assert!(
        !provider.failed_paths.is_empty(),
        "a genuine BGSM resolve failure through the real merge path must be \
         recorded in failed_paths — the diagnostic signal distinguishing it \
         from a mesh that legitimately has no material_path at all"
    );
}

/// BGEM sibling of the above — same silent-Unresolved shape, same fix.
#[test]
fn merge_external_material_records_bgem_resolve_failure() {
    let mut pool = byroredux_core::string::StringPool::new();
    let path = "materials/tests/broken.bgem";
    let mut provider = MaterialProvider::new();
    let mut mesh = imported_mesh_with_material_path(&mut pool, path);

    assert!(provider.failed_paths.is_empty());
    let outcome = merge_external_material(&mut mesh.material, &mut provider, &mut pool);
    assert_eq!(outcome, MergeOutcome::Unresolved);
    assert!(
        !provider.failed_paths.is_empty(),
        "a genuine BGEM resolve failure through the real merge path must be \
         recorded in failed_paths, same as the BGSM arm"
    );
}

/// A mesh with NO `material_path` at all is the legitimate case this
/// diagnostic must NOT fire for — it is not a failure, it's a mesh
/// that was never routed through BGSM/BGEM in the first place.
#[test]
fn merge_external_material_no_material_path_does_not_record_a_failure() {
    let mut pool = byroredux_core::string::StringPool::new();
    let mut provider = MaterialProvider::new();
    let mut material = ImportedMaterial::default();
    assert!(material.material_path.is_none());

    let outcome = merge_external_material(&mut material, &mut provider, &mut pool);
    assert_eq!(outcome, MergeOutcome::Unresolved);
    assert!(
        provider.failed_paths.is_empty(),
        "no material_path is not a resolve failure — must not be conflated \
         with a broken BGSM/BGEM reference"
    );
}

/// `build_material_provider` on CLI args without `--materials-ba2`
/// returns an empty provider — the merge helper short-circuits
/// when the archive lookup fails, so pre-FO4 content pays zero cost.
#[test]
fn build_material_provider_without_args_is_empty() {
    let provider = build_material_provider(&[]);
    assert!(provider.archives.is_empty());
}

/// `build_script_provider` without `--scripts-bsa` yields an empty
/// provider whose every `.pex` lookup misses — the attach path then
/// skips the VMAD branch (the `is_empty` fast-out) and falls through
/// exactly like an unregistered SCPT. No game data needed.
#[test]
fn build_script_provider_without_args_is_empty_and_misses() {
    let provider = build_script_provider(&[]);
    assert!(provider.is_empty());
    assert!(provider.extract_pex("DA10MainDoorScript").is_none());
}

#[test]
fn legacy_script_principal_is_portable_and_archive_scoped() {
    let first = legacy_script_principal("/games/Data/My Mod - Scripts.bsa");
    let relocated = legacy_script_principal("D:\\Portable\\Data\\MY MOD - SCRIPTS.BSA");
    let other = legacy_script_principal("/games/Data/Other Mod - Scripts.bsa");

    assert_eq!(first, relocated);
    assert_ne!(first, other);
    assert!(first.as_str().starts_with("legacy.scripts.my-mod-scripts."));
}

/// The `.pex` archive-key normalisation: a bare VMAD-authored script
/// name resolves to `scripts\<lower>.pex`, and names that already
/// carry the folder / extension / forward-slashes are idempotent.
#[test]
fn pex_archive_path_normalises_every_authored_form() {
    // Bare name (the common VMAD case).
    assert_eq!(
        pex_archive_path("DA10MainDoorScript"),
        "scripts\\da10maindoorscript.pex"
    );
    // Already lowercase + folder + extension → unchanged.
    assert_eq!(
        pex_archive_path("scripts\\da10maindoorscript.pex"),
        "scripts\\da10maindoorscript.pex"
    );
    // Extension present, folder missing.
    assert_eq!(pex_archive_path("MyScript.pex"), "scripts\\myscript.pex");
    // Forward slashes are converted to the archive's backslashes.
    assert_eq!(
        pex_archive_path("scripts/Sub/MyScript"),
        "scripts\\sub\\myscript.pex"
    );
    // Mixed case folded.
    assert_eq!(pex_archive_path("FXShader"), "scripts\\fxshader.pex");
}

/// Regression for #583 / #1454 / #1455 — synthetic BGSM template chain
/// exercises child-first scalar precedence inline with the prod helper
/// body. Child authors `emit_enabled=true` + distinct emissive, specular,
/// glossiness, alpha, UV, fresnel_power, grayscale_to_palette_scale, and
/// two_sided; parent authors different values. The child's scalar values
/// must win; parent must contribute only fields the child left at its
/// default.
///
/// This mirrors the prod `merge_external_material` scalar body (minus the
/// archive-read step); any future drift between the two surfaces here.
#[test]
fn bgsm_merge_forwards_scalars_child_first() {
    use byroredux_bgsm::template::ResolvedMaterial;
    use byroredux_bgsm::{BaseMaterial, BgsmFile};
    use std::sync::Arc;

    let child = BgsmFile {
        base: BaseMaterial {
            alpha: 0.25,
            u_offset: 0.1,
            v_offset: 0.2,
            u_scale: 2.0,
            v_scale: 3.0,
            two_sided: true,
            ..Default::default()
        },
        emit_enabled: true,
        emittance_color: [1.0, 0.5, 0.25],
        emittance_mult: 7.0,
        specular_color: [0.9, 0.8, 0.7],
        specular_mult: 3.5,
        smoothness: 0.85,
        fresnel_power: 3.5, // non-default; must win over parent's 9.0
        grayscale_to_palette_scale: 0.75, // non-default; must win over parent's 2.5
        ..Default::default()
    };
    let parent = BgsmFile {
        base: BaseMaterial {
            alpha: 0.5,
            u_offset: 99.0, // must NOT win
            ..Default::default()
        },
        emit_enabled: true,
        emittance_color: [0.0, 0.0, 0.0],
        emittance_mult: 0.0,
        specular_mult: 0.01,             // must NOT win
        smoothness: 0.01,                // must NOT win
        fresnel_power: 9.0,              // must NOT win
        grayscale_to_palette_scale: 2.5, // must NOT win
        ..Default::default()
    };
    let resolved = ResolvedMaterial {
        file: child,
        parent: Some(Arc::new(ResolvedMaterial {
            file: parent,
            parent: None,
        })),
    };

    // Replicate the scalar-forwarding half of merge_external_material
    // inline. Mesh starts with engine defaults so every write below
    // is the BGSM path overriding a fallback.
    let mut emissive_color = [0.0f32; 3];
    let mut emissive_mult = 0.0f32;
    let mut specular_color = [1.0f32; 3];
    let mut specular_strength = 1.0f32;
    let mut glossiness = 0.0f32;
    let mut mat_alpha = 1.0f32;
    let mut uv_offset = [0.0f32; 2];
    let mut uv_scale = [1.0f32; 2];
    let mut two_sided = false;
    let mut is_decal = false;
    let mut fresnel_power = 5.0f32;
    let mut grayscale_to_palette_scale = 1.0f32;

    let mut set_emissive = false;
    let mut set_specular = false;
    let mut set_glossiness = false;
    let mut set_alpha = false;
    let mut set_uv = false;
    let mut set_fresnel = false;
    let mut set_palette_scale = false;
    for step in resolved.walk() {
        let bgsm = &step.file;
        if !set_emissive && bgsm.emit_enabled {
            emissive_color = bgsm.emittance_color;
            emissive_mult = bgsm.emittance_mult;
            set_emissive = true;
        }
        if !set_specular {
            specular_color = bgsm.specular_color;
            specular_strength = bgsm.specular_mult;
            set_specular = true;
        }
        if !set_glossiness {
            // Mirror of the production conversion (`bgsm.smoothness * 100.0`)
            // — 0–1 smoothness on the BGSM side becomes 0–100 glossiness
            // on the Material side.
            glossiness = bgsm.smoothness * 100.0;
            set_glossiness = true;
        }
        if !set_fresnel {
            fresnel_power = bgsm.fresnel_power;
            set_fresnel = true;
        }
        if !set_palette_scale {
            grayscale_to_palette_scale = bgsm.grayscale_to_palette_scale;
            set_palette_scale = true;
        }
        if !set_alpha {
            mat_alpha = bgsm.base.alpha;
            set_alpha = true;
        }
        if !set_uv {
            uv_offset = [bgsm.base.u_offset, bgsm.base.v_offset];
            uv_scale = [bgsm.base.u_scale, bgsm.base.v_scale];
            set_uv = true;
        }
        if bgsm.base.two_sided {
            two_sided = true;
        }
        if bgsm.base.decal {
            is_decal = true;
        }
    }

    // Child values must win.
    assert_eq!(emissive_color, [1.0, 0.5, 0.25]);
    assert_eq!(emissive_mult, 7.0);
    assert_eq!(specular_color, [0.9, 0.8, 0.7]);
    assert_eq!(specular_strength, 3.5);
    // BGSM smoothness 0.85 → 85.0 on the Material 0–100 scale.
    assert_eq!(glossiness, 85.0);
    assert_eq!(mat_alpha, 0.25);
    assert_eq!(uv_offset, [0.1, 0.2]);
    assert_eq!(uv_scale, [2.0, 3.0]);
    // #1454 — child's non-default fresnel wins over parent's 9.0.
    assert!((fresnel_power - 3.5).abs() < f32::EPSILON);
    // #1455 — child's non-default palette scale wins over parent's 2.5.
    assert!((grayscale_to_palette_scale - 0.75).abs() < f32::EPSILON);
    // Boolean OR across the chain — child authored true.
    assert!(two_sided);
    assert!(!is_decal);
}

/// Regression for #2212 (NIFAL-D8-01): the synthesized NIF F4SF2 bit-25
/// alpha-test threshold (128/255, #1985) must NOT outrank an authored BGSM
/// `alpha_test_ref`. Pre-fix, `material.alpha_test` (already `true` from the
/// NIF flag, set before the BGSM merge loop runs) gated the threshold
/// write — so a BGSM authoring a non-128 `alpha_test_ref` never landed.
///
/// Mirrors the prod `merge_external_material` alpha-test body (minus the
/// archive-read step), same convention as
/// `bgsm_merge_forwards_scalars_child_first` above.
#[test]
fn bgsm_alpha_test_threshold_wins_over_nif_presynthesized_default() {
    use byroredux_bgsm::template::ResolvedMaterial;
    use byroredux_bgsm::{BaseMaterial, BgsmFile};

    let bgsm = BgsmFile {
        base: BaseMaterial {
            alpha_test: true,
            alpha_test_ref: 200, // non-128; must win over the NIF-synthesized default
            ..Default::default()
        },
        ..Default::default()
    };
    let resolved = ResolvedMaterial {
        file: bgsm,
        parent: None,
    };

    // `material.alpha_test` starts `true` with the #1985-synthesized 128/255
    // threshold, simulating the NIF F4SF2 bit-25 path having already run.
    let mut alpha_test = true;
    let mut alpha_threshold = 128.0 / 255.0;
    let mut set_alpha_test = false;
    for step in resolved.walk() {
        let bgsm = &step.file;
        if bgsm.base.alpha_test {
            alpha_test = true;
            if !set_alpha_test {
                alpha_threshold = f32::from(bgsm.base.alpha_test_ref) / 255.0;
                set_alpha_test = true;
            }
        }
    }

    assert!(alpha_test);
    assert!(
        (alpha_threshold - 200.0 / 255.0).abs() < f32::EPSILON,
        "authored BGSM alpha_test_ref (200) must win over the NIF-synthesized \
         default (128), got threshold {alpha_threshold}"
    );
}

/// `detect_kind` returns `Bgem` for a buffer with BGEM magic even
/// when the caller intended BGSM dispatch. This is the unit
/// foundation for the wrong-extension footgun guard (#758): a forged
/// `.bgsm`-named file carrying BGEM magic is correctly identified.
#[test]
fn detect_kind_returns_bgem_for_bgem_magic_in_bgsm_named_file() {
    use byroredux_bgsm::{detect_kind, MaterialKind};
    // Minimal BGEM header (just the 4-byte magic) — enough for detect_kind.
    let bgem_magic_bytes: &[u8] = b"BGEM";
    assert_eq!(
        detect_kind(bgem_magic_bytes),
        Some(MaterialKind::Bgem),
        "detect_kind must return Bgem even when the caller opened a .bgsm-named file"
    );

    let bgsm_magic_bytes: &[u8] = b"BGSM";
    assert_eq!(
        detect_kind(bgsm_magic_bytes),
        Some(MaterialKind::Bgsm),
        "detect_kind must return Bgsm for genuine BGSM magic"
    );

    // A mismatched extension is detected by comparing ext_kind vs magic_kind
    // as done in merge_external_material. Simulate the comparison logic.
    let ext_kind = Some(MaterialKind::Bgsm); // extension says .bgsm
    let magic_kind = detect_kind(bgem_magic_bytes); // magic says BGEM
    assert_ne!(
        ext_kind, magic_kind,
        "extension (.bgsm) and magic (BGEM) must disagree — this is the mismatch case"
    );
}

/// #3371 (SKY-2026-08-27-D7-03) — the BGEM merge is the fourth writer of
/// `EmissiveSource`, and #2591 gated only the three NIF-side ones. An
/// unauthored BGEM (default `base_color` / `base_color_scale`) was still
/// tagged `Effect`, degenerating the discriminator to "has an effect
/// shader" rather than "authored a contribution" — the exact regression
/// #2591 removed everywhere else.
///
/// Both directions matter: the gate must not also swallow a genuinely
/// authored tint, or the fix trades one wrong tag for another.
#[test]
fn bgem_merge_gates_emissive_source_on_authored_contribution() {
    use byroredux_core::ecs::components::material::EmissiveSource;

    // Unauthored: `BgemFile::default()` ships base_color [0,0,0] with
    // scale 0.0 — nothing to attribute, so the tag must stay `None`.
    let mut pool = byroredux_core::string::StringPool::new();
    let unauthored = "materials/tests/emissive_gate_off.bgem";
    let mut provider = MaterialProvider::new();
    provider.insert_bgem_for_test(unauthored, BgemFile::default());
    let mut mesh = imported_mesh_with_material_path(&mut pool, unauthored);
    assert!(merge_external_material(&mut mesh.material, &mut provider, &mut pool).merged());
    assert_eq!(
        mesh.material.emissive_source,
        EmissiveSource::None,
        "#3371: a BGEM authoring no tint must not claim EmissiveSource::Effect"
    );

    // Authored: non-zero colour AND non-zero scale — the tag must land.
    let authored = "materials/tests/emissive_gate_on.bgem";
    provider.insert_bgem_for_test(
        authored,
        BgemFile {
            base_color: [0.2, 0.6, 1.0],
            base_color_scale: 2.5,
            ..Default::default()
        },
    );
    let mut mesh = imported_mesh_with_material_path(&mut pool, authored);
    assert!(merge_external_material(&mut mesh.material, &mut provider, &mut pool).merged());
    assert_eq!(
        mesh.material.emissive_source,
        EmissiveSource::Effect,
        "#3371: the gate must still tag a genuinely authored BGEM tint"
    );
    assert_eq!(mesh.material.emissive_mult, 2.5);
}

/// Regression for #3507 (FO4-2026-08-27-D5-01), real merge path: BGSM's
/// `tile_u`/`tile_v` pair is its own texture-address-mode authoring
/// channel, separate from the NIF shader property's `texture_clamp_mode`
/// field — and was never mapped onto `ImportedMaterial::texture_clamp_mode`
/// at all. `tile_u = false, tile_v = true` must resolve to
/// CLAMP_S_WRAP_T (1): bit 1 (S) clear, bit 0 (T) set — the same bit
/// packing `crates/bgsm/src/base.rs` decodes the wire byte with.
#[test]
fn bgsm_merge_forwards_tile_flags_to_texture_clamp_mode() {
    let mut pool = byroredux_core::string::StringPool::new();
    let path = "materials/tests/clamp_s_wrap_t.bgsm";
    let mut provider = MaterialProvider::new();
    provider.insert_bgsm_for_test(
        path,
        ResolvedMaterial {
            file: BgsmFile {
                base: byroredux_bgsm::BaseMaterial {
                    tile_u: false,
                    tile_v: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            parent: None,
        },
    );
    let mut mesh = imported_mesh_with_material_path(&mut pool, path);
    assert_eq!(
        mesh.material.texture_clamp_mode, 0,
        "sanity: ImportedMaterial's own default is CLAMP_S_CLAMP_T"
    );

    assert!(merge_external_material(&mut mesh.material, &mut provider, &mut pool).merged());
    assert_eq!(
        mesh.material.texture_clamp_mode, 1,
        "tile_u=false (S clamped), tile_v=true (T wraps) must map to \
         CLAMP_S_WRAP_T (1)"
    );
}

/// BGEM sibling of the above — same field, same mapping, no inheritance
/// chain to walk (single unconditional write, matching the neighbouring
/// `uv_offset`/`uv_scale` forwarding).
#[test]
fn bgem_merge_forwards_tile_flags_to_texture_clamp_mode() {
    let mut pool = byroredux_core::string::StringPool::new();
    let path = "materials/tests/clamp_wrap_s_clamp_t.bgem";
    let mut provider = MaterialProvider::new();
    provider.insert_bgem_for_test(
        path,
        BgemFile {
            base: byroredux_bgsm::BaseMaterial {
                tile_u: true,
                tile_v: false,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let mut mesh = imported_mesh_with_material_path(&mut pool, path);

    assert!(merge_external_material(&mut mesh.material, &mut provider, &mut pool).merged());
    assert_eq!(
        mesh.material.texture_clamp_mode, 2,
        "tile_u=true (S wraps), tile_v=false (T clamped) must map to \
         WRAP_S_CLAMP_T (2)"
    );
}

/// #2642 (SF-D9-2026-08-07-03) — `distance_field_alpha_texture` (BGSM
/// v>=17, FO76/Starfield-era) is deliberately unforwarded: no
/// `MaterialTextureSet` role exists for it. Pins the deferral marker at
/// both the field declaration (`bgsm.rs`) and the merge site
/// (`material.rs`, this crate) so a future editor who adds a role
/// without wiring it — or removes the note while "cleaning up" — doesn't
/// leave the gap undocumented again.
#[test]
fn distance_field_alpha_texture_deferral_is_documented_at_both_sites() {
    let bgsm_rs = include_str!("../../../../crates/bgsm/src/bgsm.rs");
    let material_rs = include_str!("../material.rs");

    assert!(
        bgsm_rs.contains("#2642") && bgsm_rs.contains("distance_field_alpha_texture"),
        "bgsm.rs's field declaration must carry the #2642 deferral marker"
    );
    assert!(
        material_rs.contains("#2642")
            && material_rs.contains("distance_field_alpha_texture"),
        "material.rs's merge site must carry the #2642 deferral marker too, \
         not just the field declaration"
    );
}
