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
//! ## SpeedTree Phase 2 (planned, no ROADMAP row — gated by
//! `crates/spt/docs/format-notes.md`'s "Geometry tail" section)
//!
//! - Decode the geometry tail past `tail_offset` → real branch /
//!   frond meshes with the bark texture (`SptScene::bark_textures`).
//! - Decode leaf-card data → multiple per-leaf billboards positioned
//!   around the canopy (auto-instanced by the renderer's existing
//!   batching path, #272).
//! - Decode `BezierSpline` curves into typed wind-response data on
//!   a per-tree component (consumed by [`SptImportParams::wind`]).
//!
//! Each plugs in here without changing the public signature. Until
//! Phase 2 lands, the parser-captured `wind` / `bound_radius` /
//! `billboard_size` fields below ride through onto `SptImportParams`
//! so the silent-drop is at the consumer, not the parser surface.

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
    /// Wind sensitivity / strength from the TREE record's `CNAM`
    /// (Oblivion ships 5 × f32; FO3/FNV ship 8 × f32 — exact field
    /// semantics not pinned). Captured for SpeedTree Phase 2 wind
    /// animation; not consumed today. **Not** sourced from `BNAM` —
    /// per UESP + the TREE parser, BNAM is FO3/FNV billboard
    /// width/height, which flows into `bounds` instead (see #1002).
    pub wind: Option<(f32, f32)>,
    /// FormID of the source TREE record. Useful when downstream code
    /// wants to seed per-tree variation (sway phase, leaf-tint
    /// random offset) deterministically.
    pub form_id: Option<u32>,
    /// MODB bound radius from the TREE record, in game units. Used as
    /// a per-tree size fallback when `bounds` (OBND) is absent —
    /// vanilla Oblivion ships MODB on 100 % of TREE records and OBND
    /// on none, while FO3/FNV ship OBND on 100 % and MODB on none
    /// (corpus stats 2026-05-13). Resolving to a billboard size:
    /// width ≈ R, height ≈ 2R (matches the existing default's 1:2
    /// ratio; verified against the Oblivion MODB range 157–3621).
    pub bound_radius: Option<f32>,
    /// BNAM billboard width/height (FO3/FNV only). Used as a fallback
    /// below OBND but above MODB. **Not** preferred over OBND despite
    /// being authored "for the billboard specifically": corpus check
    /// on FNV/FO3 (#1002, 2026-05-13) showed BNAM encodes a
    /// distance-imposter quad size that *clamps* tall trees — e.g.
    /// `WhiteOak01` OBND `802×1567` vs BNAM `768×768`, `Pine01` OBND
    /// `766×1277` vs BNAM `768×768`. The placeholder stands in for
    /// the *whole* tree (not just a distance imposter), so OBND's
    /// full silhouette is the more visually accurate source. BNAM
    /// is captured for the rare mod-content case where OBND is
    /// absent but BNAM is present.
    pub billboard_size: Option<(f32, f32)>,
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
    // "First wins" on duplicate tag-4003 entries: vanilla `.spt` files
    // emit at most one leaf texture tag, but mod content occasionally
    // ships several. The first one is the SpeedTree exporter's primary
    // — later entries (when present) are LOD-tier alternates that the
    // placeholder doesn't render at distance anyway. See #997.
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
        //
        // #995 — TREE.OBND is Bethesda Z-up; the engine expects Y-up.
        // Apply the same axis swap that the NIF importer applies
        // (`crates/nif/src/import/mod.rs:208-211`): (x, y, z)_zup →
        // (x, z, -y)_yup for the center, and the half-extent
        // magnitudes swap as `(x, z, y)` since they're unsigned. The
        // center swap routes through the canonical
        // `byroredux_core::math::coord::zup_to_yup_pos` helper so the
        // SpeedTree path can't drift from the NIF / REFR import
        // boundary (post-#1044 / TD3-004 consolidation).
        bs_bound: params.bounds.map(|(min, max)| {
            let cx = (min[0] + max[0]) * 0.5;
            let cy = (min[1] + max[1]) * 0.5;
            let cz = (min[2] + max[2]) * 0.5;
            let hx = (max[0] - min[0]).abs() * 0.5;
            let hy = (max[1] - min[1]).abs() * 0.5;
            let hz = (max[2] - min[2]).abs() * 0.5;
            let center_yup = byroredux_core::math::coord::zup_to_yup_pos([cx, cy, cz]);
            let half_yup = [hx, hz, hy];
            (center_yup, half_yup)
        }),
        // SpeedTree placeholders carry no FO4-weapon-mod attach graph.
        attach_points: None,
        child_attach_connections: None,
        embedded_clip: None,
    }
}

/// Pick a billboard size for the placeholder.
///
/// Returns `(width, height)` in game units. Precedence:
/// 1. **OBND** (`params.bounds`) — width is the X extent, height the
///    Z extent (Bethesda Z-up). FO3/FNV TREE records ship this on
///    100 % of vanilla content.
/// 2. **BNAM** (`params.billboard_size`) — FO3/FNV billboard
///    width × height pair. Only reached when OBND is absent (#1002,
///    corpus verification showed BNAM clamps tall trees so OBND
///    wins for our whole-tree placeholder).
/// 3. **MODB** (`params.bound_radius`) — sphere radius converted as
///    `(width, height) = (R, 2R)`. Oblivion ships MODB on 100 % of
///    vanilla TREE records and OBND on none (corpus stats
///    2026-05-13), so this is the Oblivion path. Matches the
///    existing 256×512 default's 1:2 ratio.
/// 4. **Default** — 256 × 512. Only reaches mod content with no
///    fields authored.
///
/// All paths clamp to the `[16, 8192]` band so corrupt input can't
/// produce a 1-pixel mosquito or a floor-to-skybox planet-sized
/// billboard.
fn compute_billboard_size(params: &SptImportParams) -> (f32, f32) {
    if let Some((min, max)) = params.bounds {
        let width = (max[0] - min[0]).abs().clamp(16.0, 8192.0);
        let height = (max[2] - min[2]).abs().clamp(16.0, 8192.0);
        return (width, height);
    }
    if let Some((w, h)) = params.billboard_size {
        let width = w.abs().clamp(16.0, 8192.0);
        let height = h.abs().clamp(16.0, 8192.0);
        return (width, height);
    }
    if let Some(r) = params.bound_radius.filter(|r| *r > 0.0) {
        let width = r.clamp(16.0, 8192.0);
        let height = (2.0 * r).clamp(16.0, 8192.0);
        return (width, height);
    }
    (DEFAULT_BILLBOARD_WIDTH, DEFAULT_BILLBOARD_HEIGHT)
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
///
/// **Normal convention** (#1000): the billboard system rotates the
/// entity via `Quat::from_rotation_arc(-Z, look_dir)` — i.e. the
/// object-space `-Z` axis ends up pointing at the camera. The front
/// face of the placeholder must therefore have its normal along
/// `-Z` so the textured side renders toward the camera at the
/// rotation arc's terminal direction. Pre-#1000 the normals
/// pointed `+Z` and `two_sided: true` masked the inverted convention;
/// the indexed winding has been flipped to `[0, 3, 2, 2, 1, 0]` to
/// keep the front face CCW when viewed from -Z (matches the NIF
/// importer's billboard mesh convention).
fn placeholder_billboard_mesh(
    width: f32,
    height: f32,
    texture_path: Option<FixedString>,
) -> ImportedMesh {
    let half_w = width * 0.5;
    let positions = vec![
        [-half_w, 0.0, 0.0],    // 0: bottom-left
        [half_w, 0.0, 0.0],     // 1: bottom-right
        [half_w, height, 0.0],  // 2: top-right
        [-half_w, height, 0.0], // 3: top-left
    ];
    // Front-face normals point -Z (toward the camera after the
    // billboard rotation arc). See doc above for the rationale.
    let normals = vec![[0.0, 0.0, -1.0]; 4];
    let uvs = vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    let colors = vec![[1.0, 1.0, 1.0, 1.0]; 4];
    // Winding: (0 → 3 → 2) and (2 → 1 → 0). Traces CCW when viewed
    // from -Z — i.e., the camera at look-arc termination sees the
    // textured front face.
    let indices = vec![0, 3, 2, 2, 1, 0];

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
        // #1000 — front-face winding flipped to match the -Z normal
        // convention. Triangulation diagonal stays BL-TR (same as
        // pre-fix), but each triangle's vertex order is reversed.
        assert_eq!(mesh.indices, vec![0, 3, 2, 2, 1, 0]);
        assert!(mesh.alpha_test, "leaf billboards use alpha-test cutout");
        assert!(mesh.two_sided, "billboard rotates, both faces visible");
        assert!(!mesh.has_alpha, "alpha-test and alpha-blend are exclusive");
        assert_eq!(mesh.parent_node, Some(0), "mesh is a child of node 0");
    }

    /// #1000 — front-face normal must point -Z so the billboard
    /// rotation arc (`Quat::from_rotation_arc(-Z, look_dir)`) maps
    /// the textured side to the camera. Pre-fix the normals pointed
    /// +Z and `two_sided: true` masked the inverted convention.
    #[test]
    fn placeholder_normals_point_negative_z_for_billboard_arc() {
        let mut pool = StringPool::new();
        let imported = import_spt_scene(&empty_scene(), &SptImportParams::default(), &mut pool);
        let mesh = &imported.meshes[0];
        assert_eq!(mesh.normals.len(), 4, "one normal per quad corner");
        for (i, n) in mesh.normals.iter().enumerate() {
            assert_eq!(
                n, &[0.0, 0.0, -1.0],
                "normal[{i}] must point -Z (engine convention: object -Z faces camera at rotation-arc termination)"
            );
        }
    }

    /// #1000 — index winding must be CCW when viewed from -Z so the
    /// front face is the visible side after the billboard rotation
    /// arc. Computes the geometric normal of triangle 0 (vertices 0,
    /// 3, 2 = BL, TL, TR) via the cross product and asserts the Z
    /// component is negative (i.e., facing -Z).
    #[test]
    fn placeholder_index_winding_produces_negative_z_geometric_normal() {
        let mut pool = StringPool::new();
        let imported = import_spt_scene(&empty_scene(), &SptImportParams::default(), &mut pool);
        let mesh = &imported.meshes[0];

        let i0 = mesh.indices[0] as usize;
        let i1 = mesh.indices[1] as usize;
        let i2 = mesh.indices[2] as usize;
        let p0 = mesh.positions[i0];
        let p1 = mesh.positions[i1];
        let p2 = mesh.positions[i2];
        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        // Cross product (e1 × e2). Front face under CCW winding has
        // the cross product pointing along the front-face normal.
        let nz = e1[0] * e2[1] - e1[1] * e2[0];
        assert!(
            nz < 0.0,
            "geometric normal of triangle 0 must have negative Z (winding CCW from -Z); got nz = {nz}"
        );
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
        // #995 — bounds emerge in Y-up: (x, y, z)_zup → (x, z, -y)_yup.
        // Input Bethesda Z-up center `(0, 0, 400)` → Y-up `(0, 400, 0)`
        // (tall tree centred on Y-up vertical). Half-extent magnitudes
        // re-shuffle from `(50, 50, 400)` to `(50, 400, 50)` so the tall
        // axis lives on Y, matching the placeholder mesh's Y-vertical.
        assert_eq!(center, [0.0, 400.0, 0.0]);
        assert_eq!(half, [50.0, 400.0, 50.0]);
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

    /// #1002 — BNAM (FO3/FNV billboard width × height) is consumed
    /// when OBND is absent but BNAM is present. Mod-content edge case;
    /// vanilla FO3/FNV ship both fields and OBND wins.
    #[test]
    fn bnam_drives_placeholder_size_when_obnd_absent() {
        let mut pool = StringPool::new();
        let scene = empty_scene();
        let params = SptImportParams {
            billboard_size: Some((300.0, 600.0)),
            ..Default::default()
        };
        let imported = import_spt_scene(&scene, &params, &mut pool);
        let mesh = &imported.meshes[0];
        // Width = 300, height = 600 — straight from BNAM.
        assert_eq!(mesh.positions[0], [-150.0, 0.0, 0.0]);
        assert_eq!(mesh.positions[2], [150.0, 600.0, 0.0]);
    }

    /// #1002 — OBND wins over BNAM when both are authored. Verified
    /// against vanilla `WhiteOak01` (FNV/FO3): OBND `802×1567`, BNAM
    /// `768×768`. Pre-fix using BNAM would have rendered WhiteOak as
    /// a 768×768 stump instead of the 1567-tall tree.
    #[test]
    fn obnd_precedence_over_bnam() {
        let mut pool = StringPool::new();
        let scene = empty_scene();
        let params = SptImportParams {
            // Vanilla WhiteOak01 OBND extent (rounded).
            bounds: Some(([-401.0, -401.0, 0.0], [401.0, 401.0, 1567.0])),
            // Vanilla WhiteOak01 BNAM.
            billboard_size: Some((768.0, 768.0)),
            ..Default::default()
        };
        let imported = import_spt_scene(&scene, &params, &mut pool);
        let mesh = &imported.meshes[0];
        // Must size by OBND (1567 tall), not BNAM (would clamp to 768).
        assert_eq!(mesh.positions[2][1], 1567.0);
    }

    /// #1002 — corrupt BNAM (negative or oversized) clamps to the
    /// safe [16, 8192] band, mirroring the OBND/MODB clamp.
    #[test]
    fn bnam_clamps_to_safe_band() {
        let mut pool = StringPool::new();
        let scene = empty_scene();
        // Negative-w mod content (sign flip).
        let params = SptImportParams {
            billboard_size: Some((-500.0, 50_000.0)),
            ..Default::default()
        };
        let imported = import_spt_scene(&scene, &params, &mut pool);
        let mesh = &imported.meshes[0];
        // |-500| = 500 stays in band; 50000 clamps to 8192.
        assert_eq!(mesh.positions[2], [250.0, 8192.0, 0.0]);
    }

    /// #1002 — precedence chain order: when OBND is absent but both
    /// BNAM and MODB are authored, BNAM wins.
    #[test]
    fn bnam_precedence_over_modb() {
        let mut pool = StringPool::new();
        let scene = empty_scene();
        let params = SptImportParams {
            billboard_size: Some((400.0, 700.0)),
            bound_radius: Some(1000.0),
            ..Default::default()
        };
        let imported = import_spt_scene(&scene, &params, &mut pool);
        let mesh = &imported.meshes[0];
        // BNAM wins over MODB-derived (1000, 2000).
        assert_eq!(mesh.positions[2], [200.0, 700.0, 0.0]);
    }

    /// #1001 — Oblivion TREE records ship MODB but no OBND. Pre-fix
    /// the placeholder fell back to the 256×512 default, sizing
    /// Cyrodiil pine trees like Mojave creosote bushes. Post-fix the
    /// MODB radius drives the size as `(R, 2R)`.
    #[test]
    fn modb_drives_placeholder_size_when_obnd_absent() {
        let mut pool = StringPool::new();
        let scene = empty_scene();
        // Vanilla Oblivion `TreeSugarMapleForestFA`: MODB ≈ 1931.
        let params = SptImportParams {
            bound_radius: Some(1931.0),
            ..Default::default()
        };
        let imported = import_spt_scene(&scene, &params, &mut pool);
        let mesh = &imported.meshes[0];
        // width = R, height = 2R.
        assert_eq!(mesh.positions[0], [-1931.0 * 0.5, 0.0, 0.0]);
        assert_eq!(mesh.positions[2], [1931.0 * 0.5, 1931.0 * 2.0, 0.0]);
    }

    /// MODB outside the safe band must clamp like OBND does. A
    /// mod-authored MODB of 50 000 would otherwise span the entire
    /// skybox.
    #[test]
    fn modb_clamps_to_safe_band() {
        let mut pool = StringPool::new();
        let scene = empty_scene();
        let params = SptImportParams {
            bound_radius: Some(50_000.0),
            ..Default::default()
        };
        let imported = import_spt_scene(&scene, &params, &mut pool);
        let mesh = &imported.meshes[0];
        // Width clamps at 8192, height also at 8192 (2R = 100 000 → 8192).
        assert_eq!(mesh.positions[2][0], 8192.0 * 0.5);
        assert_eq!(mesh.positions[2][1], 8192.0);
    }

    /// OBND wins over MODB when both are present. FO3/FNV TREE records
    /// don't ship MODB so this is more of a defensive guarantee for
    /// mod content / Skyrim+ TREE records that authored both.
    #[test]
    fn obnd_precedence_over_modb() {
        let mut pool = StringPool::new();
        let scene = empty_scene();
        let params = SptImportParams {
            bounds: Some(([-100.0, -100.0, 0.0], [100.0, 100.0, 1000.0])),
            bound_radius: Some(9999.0),
            ..Default::default()
        };
        let imported = import_spt_scene(&scene, &params, &mut pool);
        let mesh = &imported.meshes[0];
        // Width = 200 (OBND X extent), height = 1000 (OBND Z extent).
        // MODB ignored.
        assert_eq!(mesh.positions[2], [100.0, 1000.0, 0.0]);
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
