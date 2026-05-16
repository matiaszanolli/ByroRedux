//! Tests for `walk_node_hierarchical`, `walk_node_flat`, and the
//! various flat satellite walkers (lights, particle emitters,
//! texture effects). Lifted out of the pre-#1118 monolithic
//! `walk.rs` (TD9-004). Inner-module `use super::*;` statements
//! were updated to `use super::super::*;` so they still resolve
//! to the `walk` module after the directory promotion.

#[cfg(test)]
mod affected_nodes_tests {
    //! Regression tests for issue #335 — `NiDynamicEffect.Affected
    //! Nodes` Ptr list must surface on `ImportedLight` so the
    //! renderer's per-light filter can later restrict the light's
    //! effect to the named subtrees.
    use super::super::*;
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::node::NiNode;
    use crate::types::BlockRef;
    use std::sync::Arc;

    fn node_with_name(name: &str) -> NiNode {
        NiNode {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: Some(Arc::from(name)),
                    extra_data_refs: Vec::new(),
                    controller_ref: BlockRef::NULL,
                },
                flags: 0,
                transform: crate::types::NiTransform::default(),
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            children: Vec::new(),
            effects: Vec::new(),
        }
    }

    #[test]
    fn resolve_skips_null_pointer() {
        let scene = NifScene::default();
        let names = resolve_affected_node_names(&scene, &[u32::MAX]);
        assert!(names.is_empty());
    }

    #[test]
    fn resolve_skips_out_of_range_pointer() {
        // Empty scene — index 0 is out of range. Must be silently
        // dropped rather than panic.
        let scene = NifScene::default();
        let names = resolve_affected_node_names(&scene, &[0u32]);
        assert!(names.is_empty());
    }

    #[test]
    fn resolve_extracts_node_name() {
        // Regression: pre-#335 the `affected_nodes` Vec was parsed
        // (light.rs:48) but never read. Now the importer surfaces the
        // names on `ImportedLight` for the renderer's per-light filter.
        let mut scene = NifScene::default();
        scene
            .blocks
            .push(Box::new(node_with_name("HandLanternBone")));
        scene.blocks.push(Box::new(node_with_name("BipedHead")));
        let names = resolve_affected_node_names(&scene, &[0u32, 1u32]);
        assert_eq!(names.len(), 2);
        assert_eq!(&*names[0], "HandLanternBone");
        assert_eq!(&*names[1], "BipedHead");
    }

    #[test]
    fn resolve_drops_unnamed_target() {
        // Sibling check — a target block that exists but has no name
        // (`net.name == None`) must drop out of the result rather
        // than emitting an empty string. Empty names break consumer
        // hash-set lookups silently.
        let mut scene = NifScene::default();
        let mut anon = node_with_name("");
        anon.av.net.name = None;
        scene.blocks.push(Box::new(anon));
        scene.blocks.push(Box::new(node_with_name("Named")));
        let names = resolve_affected_node_names(&scene, &[0u32, 1u32]);
        assert_eq!(names.len(), 1);
        assert_eq!(&*names[0], "Named");
    }

    #[test]
    fn resolve_partial_failure_keeps_recoverable_entries() {
        // A mix of [valid, null, out-of-range] must yield exactly the
        // one valid entry — the null-as-no-restriction convention
        // means we'd lose meaning if a single bad pointer collapsed
        // the whole list to empty.
        let mut scene = NifScene::default();
        scene.blocks.push(Box::new(node_with_name("OnlyValid")));
        let names = resolve_affected_node_names(&scene, &[0u32, u32::MAX, 99u32]);
        assert_eq!(names.len(), 1);
        assert_eq!(&*names[0], "OnlyValid");
    }

    /// Regression for #872 / NIF-PERF-08. Both resolvers must take the
    /// `name_arc()` fast path (`Arc::clone` ⇒ refcount bump) instead of
    /// `Arc::from(&str)` (fresh heap alloc + byte copy). On cell-load
    /// critical paths — many lights' affected_nodes, every BSTreeNode's
    /// trunk + branch bone lists — that's the difference between
    /// `O(refs)` allocations and zero. We pin the contract via
    /// `Arc::ptr_eq`: the returned Arc must alias the source Arc on
    /// the underlying NiObjectNET, not a freshly minted copy.
    #[test]
    fn resolvers_refcount_bump_instead_of_realloc() {
        let mut scene = NifScene::default();
        scene.blocks.push(Box::new(node_with_name("TrunkBone")));
        scene.blocks.push(Box::new(node_with_name("BranchBoneA")));

        let original_trunk_arc = scene
            .get(0)
            .and_then(|b| b.as_object_net())
            .and_then(|n| n.name_arc())
            .expect("seed Arc must exist on the NiObjectNET")
            .clone();
        let original_branch_arc = scene
            .get(1)
            .and_then(|b| b.as_object_net())
            .and_then(|n| n.name_arc())
            .expect("seed Arc must exist")
            .clone();

        // Path 1: BSTreeNode bone-list resolution (the SpeedTree case
        // called out in #872). BlockRef-typed.
        let bone_refs = [BlockRef(0), BlockRef(1)];
        let bone_names = resolve_block_ref_names(&scene, &bone_refs);
        assert_eq!(bone_names.len(), 2);
        assert!(
            std::sync::Arc::ptr_eq(&bone_names[0], &original_trunk_arc),
            "BSTreeNode bone-list resolver must Arc::clone, not Arc::from(&str)"
        );
        assert!(
            std::sync::Arc::ptr_eq(&bone_names[1], &original_branch_arc),
            "all entries take the refcount-bump fast path"
        );

        // Path 2: NiDynamicEffect.affected_nodes resolution (the lights
        // case bundled in the same fix). Ptr-typed (u32).
        let affected = [0u32, 1u32];
        let lit_names = resolve_affected_node_names(&scene, &affected);
        assert_eq!(lit_names.len(), 2);
        assert!(
            std::sync::Arc::ptr_eq(&lit_names[0], &original_trunk_arc),
            "affected_nodes resolver shares the same fast path"
        );
        assert!(std::sync::Arc::ptr_eq(&lit_names[1], &original_branch_arc));

        // Strong-count sanity: the seed clone above + 2 entries each
        // from the two resolvers ⇒ ≥ 4 references to the trunk Arc.
        // Pre-fix the resolvers minted fresh allocations, leaving
        // strong_count == 2 (block storage + our seed clone) and the
        // returned Arcs would each be strong_count == 1.
        assert!(
            std::sync::Arc::strong_count(&original_trunk_arc) >= 4,
            "post-fix every resolved entry shares the seed Arc — \
             strong_count must reflect the refcount bump (was {})",
            std::sync::Arc::strong_count(&original_trunk_arc)
        );
    }
}

#[cfg(test)]
mod texture_effect_import_tests {
    //! Regression tests for #891 / LC-D2-NEW-01 — `NiTextureEffect`
    //! blocks must surface as `ImportedTextureEffect` after the import
    //! walk, with world-space pose, interned texture path, and
    //! resolved affected-node names. Pre-fix the parser captured all
    //! 12 wire fields but no consumer read them, so vanilla Oblivion
    //! sun gobos / FO3 / FNV light cookies parsed and were silently
    //! discarded.
    use super::super::*;
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::node::NiNode;
    use crate::blocks::texture::{NiSourceTexture, NiTextureEffect};
    use crate::import::ImportedTextureEffect;
    use crate::types::{BlockRef, NiMatrix3, NiTransform};
    use byroredux_core::string::StringPool;
    use std::sync::Arc;

    fn node_named(name: &str, children: Vec<BlockRef>) -> NiNode {
        NiNode {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: Some(Arc::from(name)),
                    extra_data_refs: Vec::new(),
                    controller_ref: BlockRef::NULL,
                },
                flags: 0,
                transform: NiTransform::default(),
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            children,
            effects: Vec::new(),
        }
    }

    fn make_texture_effect(
        affected: Vec<u32>,
        source_ref: BlockRef,
        texture_type: u32,
        coord_gen: u32,
    ) -> NiTextureEffect {
        NiTextureEffect {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: Some(Arc::from("SunGobo")),
                    extra_data_refs: Vec::new(),
                    controller_ref: BlockRef::NULL,
                },
                flags: 0,
                transform: NiTransform::default(),
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            switch_state: true,
            affected_nodes: affected,
            model_projection_matrix: NiMatrix3::default(),
            model_projection_translation: [0.0; 3],
            texture_filtering: 0,
            max_anisotropy: 1,
            texture_clamping: 0,
            texture_type,
            coordinate_generation_type: coord_gen,
            source_texture_ref: source_ref,
            enable_plane: false,
            plane: [0.0; 4],
            ps2_l: 0,
            ps2_k: 0,
        }
    }

    fn make_source_texture(filename: &str) -> NiSourceTexture {
        NiSourceTexture {
            net: NiObjectNETData {
                name: None,
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            use_external: true,
            filename: Some(Arc::from(filename)),
            pixel_data_ref: BlockRef::NULL,
            pixel_layout: 0,
            use_mipmaps: 1,
            alpha_format: 0,
            is_static: true,
        }
    }

    /// Happy path: a NIF with one NiNode root + one NiTextureEffect
    /// child that references a NiSourceTexture and lists two affected
    /// nodes. The walker must produce one `ImportedTextureEffect`
    /// with: interned texture path, both texture-type and
    /// coord-gen-type fields preserved, and both affected-node names
    /// resolved through the same `resolve_affected_node_names` path
    /// `ImportedLight` uses.
    #[test]
    fn import_texture_effect_round_trips_path_and_affected_nodes() {
        // Build the scene blocks:
        //   0 = root NiNode (has child #1)
        //   1 = NiTextureEffect (refs source #2, affects nodes #3, #4)
        //   2 = NiSourceTexture (filename = "textures\\sun_gobo.dds")
        //   3 = NiNode "SunDiscBone"
        //   4 = NiNode "CloudsBone"
        let mut scene = NifScene::default();
        scene
            .blocks
            .push(Box::new(node_named("Scene Root", vec![BlockRef(1)])));
        scene.blocks.push(Box::new(make_texture_effect(
            vec![3, 4],
            BlockRef(2),
            0, // ProjectedLight
            1, // WorldPerspective
        )));
        scene
            .blocks
            .push(Box::new(make_source_texture("textures\\sun_gobo.dds")));
        scene
            .blocks
            .push(Box::new(node_named("SunDiscBone", Vec::new())));
        scene
            .blocks
            .push(Box::new(node_named("CloudsBone", Vec::new())));
        scene.root_index = Some(0);

        let mut pool = StringPool::new();
        let effects = crate::import::import_nif_texture_effects(&scene, &mut pool);
        assert_eq!(
            effects.len(),
            1,
            "one NiTextureEffect → one ImportedTextureEffect"
        );
        let eff = &effects[0];

        // Texture path interned through the pool — resolve back for
        // the comparison; the pool lower-cases on intern.
        let path = eff
            .texture_path
            .and_then(|fs| pool.resolve(fs).map(str::to_owned));
        assert_eq!(path.as_deref(), Some("textures\\sun_gobo.dds"));

        assert_eq!(eff.texture_type, 0, "ProjectedLight roundtrip");
        assert_eq!(
            eff.coordinate_generation_type, 1,
            "WorldPerspective roundtrip"
        );

        assert_eq!(eff.affected_node_names.len(), 2);
        assert_eq!(&*eff.affected_node_names[0], "SunDiscBone");
        assert_eq!(&*eff.affected_node_names[1], "CloudsBone");
    }

    /// A NiTextureEffect whose `source_texture_ref` is null leaves
    /// `texture_path` as `None` — empty paths must drop rather than
    /// intern an empty string into the pool. Same convention the
    /// material walker uses for empty texture slots (#609).
    #[test]
    fn texture_effect_with_null_source_ref_leaves_path_none() {
        let mut scene = NifScene::default();
        scene
            .blocks
            .push(Box::new(node_named("Scene Root", vec![BlockRef(1)])));
        scene.blocks.push(Box::new(make_texture_effect(
            Vec::new(),
            BlockRef::NULL,
            2, // Environment
            2, // SphereMap
        )));
        scene.root_index = Some(0);

        let mut pool = StringPool::new();
        let effects: Vec<ImportedTextureEffect> =
            crate::import::import_nif_texture_effects(&scene, &mut pool);
        assert_eq!(effects.len(), 1);
        assert!(
            effects[0].texture_path.is_none(),
            "null source_texture_ref must produce no path"
        );
        assert_eq!(effects[0].texture_type, 2);
        assert_eq!(effects[0].coordinate_generation_type, 2);
    }

    /// A NIF without any `NiTextureEffect` blocks must produce an
    /// empty result. NO_REGRESSION check from the issue's
    /// completeness checklist — non-texture-effect scenes must not
    /// be perturbed by the new walker.
    #[test]
    fn scene_without_texture_effects_returns_empty() {
        let mut scene = NifScene::default();
        scene
            .blocks
            .push(Box::new(node_named("Scene Root", Vec::new())));
        scene.root_index = Some(0);

        let mut pool = StringPool::new();
        let effects = crate::import::import_nif_texture_effects(&scene, &mut pool);
        assert!(effects.is_empty());
    }
}

#[cfg(test)]
mod editor_marker_tests {
    //! Regression tests for `is_editor_marker` (#165 / audit N26-4-06).
    //! Exhaustive list of the prefixes the walker must filter across
    //! Gamebryo-lineage games. Missed patterns render as untextured
    //! debug geometry in the live scene (map pins, quest targets,
    //! patrol route markers, editor bounding pyramids).
    use super::super::is_editor_marker;

    #[test]
    fn matches_known_editor_marker_prefixes() {
        // Gamebryo editor / quest / patrol markers — every game.
        assert!(is_editor_marker(Some("EditorMarker")));
        assert!(is_editor_marker(Some("EDITORMARKER")));
        assert!(is_editor_marker(Some("EditorMarker_QuestNode")));
        assert!(is_editor_marker(Some("Marker_01")));
        assert!(is_editor_marker(Some("marker:patrol")));
        assert!(is_editor_marker(Some("MarkerX")));
        assert!(is_editor_marker(Some("markerx")));
    }

    /// Regression: #165 — Skyrim+ exterior-cell world map pins
    /// ("MapMarker") were rendering as untextured pyramids in the
    /// overworld. The match now catches the prefix (case-insensitive).
    #[test]
    fn matches_skyrim_map_marker() {
        assert!(is_editor_marker(Some("MapMarker")));
        assert!(is_editor_marker(Some("mapmarker")));
        assert!(is_editor_marker(Some("MapMarker_Whiterun")));
        assert!(is_editor_marker(Some("MAPMARKER")));
    }

    #[test]
    fn does_not_match_legitimate_names() {
        // False-positive regression guards — these are real NIF node
        // names that must NOT be filtered.
        assert!(!is_editor_marker(None));
        assert!(!is_editor_marker(Some("")));
        assert!(!is_editor_marker(Some("Bip01 Head")));
        assert!(!is_editor_marker(Some("NPC Torso [Tors]")));
        // "MapMarkerMesh" does get filtered — that's correct, any
        // prefix match is intentional (vanilla doesn't author non-
        // marker nodes starting with these prefixes).
        assert!(is_editor_marker(Some("MapMarkerMesh")));
    }
}

#[cfg(test)]
mod switch_node_walker_tests {
    //! Regression tests for #718 / NIF-D4-02: `walk_node_lights` and
    //! `walk_node_particle_emitters_flat` must walk through
    //! `NiSwitchNode` subtrees (previously they only called
    //! `as_ni_node`, which returns `None` for NiSwitchNode/NiLODNode,
    //! silently dropping any lights/emitters inside them).
    use super::super::*;
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::light::{NiLightBase, NiPointLight};
    use crate::blocks::node::{NiLODNode, NiNode, NiSwitchNode};
    use crate::types::{BlockRef, NiColor, NiTransform};
    use std::sync::Arc;

    fn blank_av(name: Option<&str>) -> NiAVObjectData {
        NiAVObjectData {
            net: NiObjectNETData {
                name: name.map(Arc::from),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        }
    }

    fn blank_node(name: Option<&str>, children: Vec<BlockRef>) -> NiNode {
        NiNode {
            av: blank_av(name),
            children,
            effects: Vec::new(),
        }
    }

    fn point_light_block() -> Box<dyn NiObject> {
        Box::new(NiPointLight {
            base: NiLightBase {
                av: blank_av(Some("TestLight")),
                switch_state: true,
                affected_nodes: Vec::new(),
                dimmer: 1.0,
                ambient_color: NiColor {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                },
                diffuse_color: NiColor {
                    r: 1.0,
                    g: 0.0,
                    b: 0.0,
                },
                specular_color: NiColor {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                },
            },
            constant_attenuation: 0.0,
            linear_attenuation: 0.0,
            quadratic_attenuation: 1.0,
        })
    }

    /// Regression for #718: a NiSwitchNode wrapping a NiPointLight child
    /// must yield the light from `walk_node_lights`.  Pre-fix the walker
    /// went straight to `as_ni_node`, which returns `None` for
    /// NiSwitchNode, silently dropping the light.
    #[test]
    fn walk_node_lights_traverses_ni_switch_node() {
        // Scene layout:
        //   [0] NiSwitchNode  { active_index=0, children=[1] }
        //   [1] NiPointLight  { diffuse=(1,0,0) }
        let mut scene = NifScene::default();
        scene.blocks.push(Box::new(NiSwitchNode {
            base: blank_node(None, vec![BlockRef(1)]),
            switch_flags: 0,
            index: 0,
        }));
        scene.blocks.push(point_light_block());
        scene.root_index = Some(0);

        let mut lights = Vec::new();
        walk_node_lights(&scene, 0, &NiTransform::default(), &mut lights);

        assert_eq!(
            lights.len(),
            1,
            "pre-#718: NiSwitchNode was invisible to walk_node_lights — light lost"
        );
        assert_eq!(lights[0].color, [1.0, 0.0, 0.0]);
    }

    /// Regression for #718: a NiLODNode wrapping a NiPointLight child
    /// must also yield the light (LOD 0 = highest detail is always walked).
    #[test]
    fn walk_node_lights_traverses_ni_lod_node() {
        let mut scene = NifScene::default();
        scene.blocks.push(Box::new(NiLODNode {
            base: NiSwitchNode {
                base: blank_node(None, vec![BlockRef(1), BlockRef::NULL]),
                switch_flags: 0,
                index: 0,
            },
            lod_level_data: BlockRef::NULL,
        }));
        scene.blocks.push(point_light_block());
        scene.root_index = Some(0);

        let mut lights = Vec::new();
        walk_node_lights(&scene, 0, &NiTransform::default(), &mut lights);

        assert_eq!(lights.len(), 1, "NiLODNode must expose its LOD-0 light");
    }
}
