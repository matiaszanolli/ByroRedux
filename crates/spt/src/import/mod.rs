//! `SptScene → ImportedScene` adapter.
//!
//! Reuses the engine-wide [`byroredux_nif::import::ImportedScene`]
//! type so `byroredux/src/scene.rs`'s spawn path can consume `.spt`
//! output with zero new code paths. Same shape: `Vec<ImportedNode>`,
//! `Vec<ImportedMesh>`, no particle emitters / no embedded
//! animations.
//!
//! ## What lands today (Phase 1.4 first ship)
//!
//! The geometry-tail decoder isn't wired yet, so this importer ships
//! the **placeholder fallback** described in the SpeedTree
//! compatibility plan's section 1.7: a single yaw-billboard quad
//! textured with the leaf texture. Strictly better than today's
//! treeless cells:
//!
//! - **Leaf texture** comes from (in priority order): the TREE
//!   record's `ICON` field (passed via [`SptImportParams`]), the
//!   `.spt` file's first leaf-texture tag (`4003`), or stays empty
//!   when neither is available — the renderer's missing-texture
//!   placeholder takes over.
//! - **Size** comes from the TREE record's `OBND` bounds, falling
//!   back to a 256 × 512 game-unit default (Bethesda standard tree).
//! - **Billboard mode** is `BsRotateAboutUp` (yaw-to-camera) — the
//!   correct choice for tree imposters per the rendering plan.
//! - **Alpha** is alpha-test (cutout) at threshold `0.5`. Leaf
//!   billboards in vanilla content always use cutout, never blend.
//! - **Two-sided** is on, since the billboard rotates and we want
//!   both faces visible from any camera angle.
//!
//! ## Future sub-phases
//!
//! - Decode the geometry tail past `tail_offset` → real branch /
//!   frond meshes with the bark texture (`SptScene::bark_textures`).
//! - Decode leaf-card data → multiple per-leaf billboards positioned
//!   around the canopy (auto-instanced by the renderer's existing
//!   batching path, #272).
//! - Decode `BezierSpline` curves into typed wind-response data on
//!   a per-tree component.
//!
//! Each plugs in here without changing the public signature.

use std::sync::Arc;

use byroredux_core::string::{FixedString, StringPool};
use byroredux_nif::import::{ImportedMesh, ImportedNode, ImportedScene, ShaderTypeFields};

use crate::scene::SptScene;

/// Parameters threaded through from the cell loader's TREE record to
/// the SpeedTree importer. Every field is optional — empty defaults
/// produce a generic billboard.
#[derive(Debug, Default, Clone)]
pub struct SptImportParams<'a> {
    /// Leaf billboard texture path. Sourced from the TREE record's
    /// `ICON` sub-record. Wins over any leaf path discovered inside
    /// the `.spt` itself (mods retexture trees by re-pointing TREE
    /// ICON without rewriting the `.spt`).
    pub leaf_texture_override: Option<&'a str>,
    /// Object bounds from the TREE record's `OBND` sub-record.
    /// `(min, max)` in game units (Y-up). When absent the placeholder
    /// falls back to a 256 × 512 tree silhouette.
    pub bounds: Option<([f32; 3], [f32; 3])>,
    /// Wind sensitivity / strength from the TREE record's `BNAM`.
    /// Captured for Phase 2 wind animation; not consumed today.
    pub wind: Option<(f32, f32)>,
    /// FormID of the source TREE record. Useful when downstream code
    /// wants to seed per-tree variation (sway phase, leaf-tint
    /// random offset) deterministically.
    pub form_id: Option<u32>,
}

/// Default placeholder billboard size in game units (1 unit ≈ 1.4 cm).
/// 256 × 512 ≈ 3.6 m wide × 7.2 m tall — a believable middle ground
/// for FNV creosote / Joshua trees and Cyrodiil shrubs.
const DEFAULT_BILLBOARD_WIDTH: f32 = 256.0;
const DEFAULT_BILLBOARD_HEIGHT: f32 = 512.0;

/// `BillboardMode::BsRotateAboutUp` — yaw-to-camera with no pitch.
/// Mirrors `crates/core/src/ecs/components/billboard.rs` enum values
/// so the engine's billboard_system picks up the correct rotation
/// behaviour on the spawned entity.
const BILLBOARD_MODE_BS_ROTATE_ABOUT_UP: u16 = 5;

/// Convert a parsed `.spt` parameter section into a renderable
/// [`ImportedScene`]. See module docs for the placeholder-fallback
/// behaviour today.
pub fn import_spt_scene(
    scene: &SptScene,
    params: &SptImportParams,
    pool: &mut StringPool,
) -> ImportedScene {
    // Resolve leaf texture: TREE.ICON wins, .spt's tag 4003 next.
    let leaf_texture: Option<String> = params
        .leaf_texture_override
        .map(|s| s.to_string())
        .or_else(|| scene.leaf_textures().first().map(|s| s.to_string()));

    let texture_handle: Option<FixedString> = leaf_texture
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| pool.intern(s));

    // Compute billboard size from OBND when present, falling back to
    // the 256 × 512 default. The width tracks the X extent of OBND;
    // the height tracks the Z extent (Bethesda Z-up; the importer
    // converts to Y-up below).
    let (bb_width, bb_height) = compute_billboard_size(params);

    // Single root node — flagged as a billboard so the engine's
    // `billboard_system` rotates the spawned entity each frame.
    let root_node = placeholder_root_node(/* billboard */ true);

    // Single billboard quad, parented to the root.
    let mesh = placeholder_billboard_mesh(bb_width, bb_height, texture_handle);

    ImportedScene {
        nodes: vec![root_node],
        meshes: vec![mesh],
        particle_emitters: Vec::new(),
        bsx_flags: None,
        // Carry the bounds through so the cell loader can surface a
        // `LocalBound` ECS component on the spawned entity. Frustum
        // culling and tex.missing diagnostics both consume it.
        bs_bound: params.bounds.map(|(min, max)| {
            let center = [
                (min[0] + max[0]) * 0.5,
                (min[1] + max[1]) * 0.5,
                (min[2] + max[2]) * 0.5,
            ];
            let half_extents = [
                (max[0] - min[0]) * 0.5,
                (max[1] - min[1]) * 0.5,
                (max[2] - min[2]) * 0.5,
            ];
            (center, half_extents)
        }),
        // SpeedTree placeholders carry no FO4-weapon-mod attach graph.
        attach_points: None,
        child_attach_connections: None,
        embedded_clip: None,
    }
}

/// Pick a billboard size for the placeholder.
///
/// Returns `(width, height)` in game units — width is the X extent
/// of OBND, height the Z extent (Bethesda Z-up). Both clamped to
/// the [16, 8 192] band so a corrupt / mod-broken OBND can't produce
/// a 1-pixel mosquito or a floor-to-skybox planet-sized billboard.
fn compute_billboard_size(params: &SptImportParams) -> (f32, f32) {
    let Some((min, max)) = params.bounds else {
        return (DEFAULT_BILLBOARD_WIDTH, DEFAULT_BILLBOARD_HEIGHT);
    };
    let width = (max[0] - min[0]).abs().clamp(16.0, 8192.0);
    let height = (max[2] - min[2]).abs().clamp(16.0, 8192.0);
    (width, height)
}

/// Construct a minimal `ImportedNode` for the placeholder root.
fn placeholder_root_node(billboard: bool) -> ImportedNode {
    ImportedNode {
        name: Some(Arc::from("SptPlaceholderRoot")),
        translation: [0.0, 0.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: 1.0,
        parent_node: None,
        collision: None,
        billboard_mode: billboard.then_some(BILLBOARD_MODE_BS_ROTATE_ABOUT_UP),
        tree_bones: None,
        range_kind: None,
        flags: 0,
        bs_value_node: None,
        bs_ordered_node: None,
    }
}

/// Construct a single-quad billboard mesh facing -Z, sized in
/// game units. Corner ordering is bottom-left → bottom-right →
/// top-right → top-left, with UVs `(0,1) (1,1) (1,0) (0,0)` so the
/// texture's top-left maps to the quad's top-left at sample time.
fn placeholder_billboard_mesh(
    width: f32,
    height: f32,
    texture_path: Option<FixedString>,
) -> ImportedMesh {
    let half_w = width * 0.5;
    let positions = vec![
        [-half_w, 0.0, 0.0],    // bottom-left
        [half_w, 0.0, 0.0],     // bottom-right
        [half_w, height, 0.0],  // top-right
        [-half_w, height, 0.0], // top-left
    ];
    let normals = vec![[0.0, 0.0, 1.0]; 4];
    let uvs = vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    let colors = vec![[1.0, 1.0, 1.0, 1.0]; 4];
    let indices = vec![0, 1, 2, 2, 3, 0];

    ImportedMesh {
        positions,
        colors,
        normals,
        tangents: Vec::new(),
        uvs,
        indices,
        translation: [0.0, 0.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: 1.0,
        texture_path,
        material_path: None,
        name: Some(Arc::from("SptPlaceholderBillboard")),
        // Alpha-test (cutout) is the right call for tree leaves —
        // vanilla TREE ICON textures encode the leaf silhouette in
        // the alpha channel.
        has_alpha: false,
        src_blend_mode: 6, // SRC_ALPHA — unused under alpha-test
        dst_blend_mode: 7, // INV_SRC_ALPHA — unused under alpha-test
        alpha_test: true,
        alpha_threshold: 0.5,
        alpha_test_func: 6, // GREATEREQUAL
        // Tree billboard rotates with the camera, so both faces will
        // be visible during the rotation interpolation. Two-sided
        // matches vanilla SpeedTree leaf-card behaviour.
        two_sided: true,
        is_decal: false,
        normal_map: None,
        glow_map: None,
        detail_map: None,
        gloss_map: None,
        dark_map: None,
        parallax_map: None,
        env_map: None,
        env_mask: None,
        parallax_max_passes: None,
        parallax_height_scale: None,
        vertex_color_mode: 2,  // AmbientDiffuse
        texture_clamp_mode: 0, // WRAP_S_WRAP_T
        emissive_color: [0.0; 3],
        emissive_mult: 0.0,
        specular_color: [1.0; 3],
        diffuse_color: [1.0; 3],
        ambient_color: [1.0; 3],
        specular_strength: 0.0,
        glossiness: 0.0,
        uv_offset: [0.0, 0.0],
        uv_scale: [1.0, 1.0],
        mat_alpha: 1.0,
        env_map_scale: 0.0,
        parent_node: Some(0),
        skin: None,
        z_test: true,
        z_write: true,
        z_function: 3, // LESSEQUAL
        local_bound_center: [0.0, height * 0.5, 0.0],
        local_bound_radius: (half_w * half_w + height * height * 0.25).sqrt(),
        effect_shader: None,
        material_kind: 0, // Default lit
        flags: 0,
        shader_type_fields: ShaderTypeFields::default(),
        no_lighting_falloff: None,
        wireframe: false,
        flat_shading: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{SptScene, SptValue, TagEntry};

    fn empty_scene() -> SptScene {
        SptScene::default()
    }

    fn scene_with_tag(tag: u32, value: SptValue) -> SptScene {
        SptScene {
            entries: vec![TagEntry {
                tag,
                value,
                offset: 0,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn placeholder_uses_default_size_without_bounds() {
        let mut pool = StringPool::new();
        let scene = empty_scene();
        let params = SptImportParams::default();
        let imported = import_spt_scene(&scene, &params, &mut pool);

        assert_eq!(imported.nodes.len(), 1, "single root placeholder");
        assert_eq!(imported.meshes.len(), 1, "single billboard quad");
        assert_eq!(
            imported.nodes[0].billboard_mode,
            Some(BILLBOARD_MODE_BS_ROTATE_ABOUT_UP),
            "root flagged as yaw-to-camera billboard",
        );
        assert!(imported.bs_bound.is_none(), "no bounds without TREE OBND");
        assert!(imported.embedded_clip.is_none());

        let mesh = &imported.meshes[0];
        // Default size: 256 wide × 512 tall.
        assert_eq!(mesh.positions[0], [-128.0, 0.0, 0.0]);
        assert_eq!(mesh.positions[2], [128.0, 512.0, 0.0]);
        assert_eq!(mesh.indices, vec![0, 1, 2, 2, 3, 0]);
        assert!(mesh.alpha_test, "leaf billboards use alpha-test cutout");
        assert!(mesh.two_sided, "billboard rotates, both faces visible");
        assert!(!mesh.has_alpha, "alpha-test and alpha-blend are exclusive");
        assert_eq!(mesh.parent_node, Some(0), "mesh is a child of node 0");
    }

    #[test]
    fn placeholder_uses_obnd_bounds_when_present() {
        let mut pool = StringPool::new();
        let scene = empty_scene();
        let params = SptImportParams {
            bounds: Some(([-50.0, -50.0, 0.0], [50.0, 50.0, 800.0])),
            ..Default::default()
        };
        let imported = import_spt_scene(&scene, &params, &mut pool);

        let mesh = &imported.meshes[0];
        // Width = 100 (X extent), height = 800 (Z extent).
        assert_eq!(mesh.positions[0], [-50.0, 0.0, 0.0]);
        assert_eq!(mesh.positions[2], [50.0, 800.0, 0.0]);
        assert!(imported.bs_bound.is_some());
        let (center, half) = imported.bs_bound.unwrap();
        assert_eq!(center, [0.0, 0.0, 400.0]);
        assert_eq!(half, [50.0, 50.0, 400.0]);
    }

    #[test]
    fn leaf_texture_override_wins_over_spt_tag() {
        let mut pool = StringPool::new();
        let scene = scene_with_tag(4003, SptValue::String("trees\\insidespt.dds".to_string()));
        let params = SptImportParams {
            leaf_texture_override: Some("textures/treeicon.dds"),
            ..Default::default()
        };
        let imported = import_spt_scene(&scene, &params, &mut pool);
        let texture = imported.meshes[0]
            .texture_path
            .and_then(|h| pool.resolve(h).map(|s| s.to_string()));
        assert_eq!(
            texture.as_deref(),
            Some("textures/treeicon.dds"),
            "TREE.ICON override wins over .spt tag 4003",
        );
    }

    #[test]
    fn falls_back_to_spt_leaf_tag_when_no_override() {
        let mut pool = StringPool::new();
        let scene = scene_with_tag(4003, SptValue::String("trees\\bushleaf.dds".to_string()));
        let params = SptImportParams::default();
        let imported = import_spt_scene(&scene, &params, &mut pool);
        let texture = imported.meshes[0]
            .texture_path
            .and_then(|h| pool.resolve(h).map(|s| s.to_string()));
        assert_eq!(texture.as_deref(), Some("trees\\bushleaf.dds"));
    }

    #[test]
    fn empty_texture_leaves_path_unset_for_renderer_placeholder() {
        let mut pool = StringPool::new();
        let scene = empty_scene();
        let params = SptImportParams::default();
        let imported = import_spt_scene(&scene, &params, &mut pool);
        assert!(
            imported.meshes[0].texture_path.is_none(),
            "no texture → leave path unset, renderer fills the magenta placeholder",
        );
    }

    #[test]
    fn corrupt_obnd_clamps_size_to_safe_band() {
        let mut pool = StringPool::new();
        // Inverted bounds (max < min) are mod-content edge cases.
        let scene = empty_scene();
        let params = SptImportParams {
            bounds: Some(([1000.0, 1000.0, 1000.0], [0.0, 0.0, 0.0])),
            ..Default::default()
        };
        let imported = import_spt_scene(&scene, &params, &mut pool);
        let mesh = &imported.meshes[0];
        // .abs() recovers the magnitude, then clamp keeps us in band.
        assert!(mesh.positions[2][0] > 0.0, "positive width");
        assert!(mesh.positions[2][1] > 0.0, "positive height");
        assert!(mesh.positions[2][1] <= 8192.0, "height clamped");
    }

    #[test]
    fn local_bound_radius_encloses_quad() {
        let mut pool = StringPool::new();
        let scene = empty_scene();
        let params = SptImportParams::default();
        let imported = import_spt_scene(&scene, &params, &mut pool);
        let mesh = &imported.meshes[0];
        // Bounding sphere centred at (0, height/2, 0) must reach the
        // quad's farthest corner (top-left or top-right).
        let max_corner_dist = ((128.0_f32).powi(2) + (256.0_f32).powi(2)).sqrt();
        assert!(
            mesh.local_bound_radius >= max_corner_dist - 0.01,
            "radius {} must enclose the corner distance {}",
            mesh.local_bound_radius,
            max_corner_dist,
        );
    }

    /// `BsValueNodeData` / `BsOrderedNodeData` / `TreeBones` are
    /// only relevant for NIF-rooted Skyrim+ trees — they should be
    /// absent on every `.spt` placeholder. Pin that so a future
    /// refactor that copies fields from the NIF importer can't
    /// accidentally start populating them on the SpeedTree path.
    #[test]
    fn placeholder_clears_nif_specific_node_metadata() {
        let mut pool = StringPool::new();
        let imported = import_spt_scene(&empty_scene(), &SptImportParams::default(), &mut pool);
        let n = &imported.nodes[0];
        assert!(n.bs_value_node.is_none());
        assert!(n.bs_ordered_node.is_none());
        assert!(n.tree_bones.is_none());
        assert!(n.range_kind.is_none());
        assert!(n.collision.is_none());
    }
}
