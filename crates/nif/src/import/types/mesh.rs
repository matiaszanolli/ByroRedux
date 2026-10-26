//! Imported mesh leaves, morph targets, skinning data and dismemberment helpers.

use super::*;

/// Re-export the `BsSubIndexTriShapeData` segmentation payload from
/// the parser side so callers downstream of `ImportedMesh` don't have
/// to reach into `crate::blocks::tri_shape` directly. Mirrors the
/// [`BsRangeKind`] re-export pattern. See #1206 — pre-fix the importer
/// dropped the `BsTriShapeKind::SubIndex` discriminator, making the
/// dismemberment / body-part segmentation payload invisible to
/// consumers downstream of the NIF crate.
pub use crate::blocks::tri_shape::BsSubIndexTriShapeData;

/// A mesh extracted from a NIF file, ready for GPU upload.
#[derive(Debug, Clone)]
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
    ///     Unpacked into this field by `import/mesh/bs_geometry.rs`
    ///     (`unpack_udec3_xyzw`, with the W lane snapped to exact ±1 by
    ///     `clamp_sign` — #1086 / #2246), falling back to
    ///     `synthesize_tangents_yup` when the stream carries no tangents
    ///     (#1232). Pinned by `bs_geometry_tangent_tests.rs`.
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
    ///
    /// #4236 — write-only today: three producers populate this field
    /// (`bs_tri_shape.rs`, `walk/mod.rs` ×2) but nothing outside this
    /// crate's own tests reads it yet. Not a correctness bug — drawing
    /// every band at full detail (the current behavior) is the correct
    /// max-detail render, just an unrealized distance-LOD optimization
    /// (12,096 FO4 shapes pay full triangle cost at all distances). A
    /// future M35 selector reading this field is the intended consumer;
    /// this is not that selector.
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

    /// Regression: FO3 outfit NIFs name their neck/torso caps `headmeat` /
    /// `bodymeat` (measured on `wastelandclothing01/outfitf.nif`: body parts
    /// 101 and 201), which the old name list missed — a gore band rendered
    /// under every clothed NPC's chin. Caps are identified by body part.
    #[test]
    fn dismemberment_caps_are_identified_by_body_part_not_name() {
        let with_parts = |parts: Vec<u16>| {
            let mut mesh = ImportedMesh::from_geometry(
                vec![[0.0; 3]; 3],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                vec![0, 1, 2],
            );
            mesh.skin = Some(ImportedSkin {
                triangle_body_parts: parts,
                ..Default::default()
            });
            mesh
        };
        // outfitf.nif's headmeat / bodymeat, and the body NIF caps.
        assert!(with_parts(vec![101]).is_dismemberment_cap());
        assert!(with_parts(vec![201]).is_dismemberment_cap());
        assert!(with_parts(vec![103, 105, 107, 110]).is_dismemberment_cap());
        assert!(with_parts(vec![203, 205, 207, 210]).is_dismemberment_cap());
        // Visible body/outfit geometry mixes anatomical and torso-section parts.
        assert!(!with_parts(vec![0, 7, 10, 7000, 10000]).is_dismemberment_cap());
        // A mesh with any non-cap triangle is not a cap.
        assert!(!with_parts(vec![0, 101]).is_dismemberment_cap());
        // Skyrim biped slots in the same numeric neighbourhood are not caps.
        assert!(!with_parts(vec![130, 131, 143]).is_dismemberment_cap());
        assert!(!with_parts(vec![230]).is_dismemberment_cap());
        // No body-part data, or no skin at all: never a cap.
        assert!(!with_parts(Vec::new()).is_dismemberment_cap());
        let mut unskinned = with_parts(vec![101]);
        unskinned.skin = None;
        assert!(!unskinned.is_dismemberment_cap());
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
    /// NIF. Captured Y-up at extraction; identity when the source NIF
    /// carries none (FO4+ BSSkin paths).
    ///
    /// **Informational / diagnostic only — the runtime palette does NOT
    /// compose it** (#4555). Since #771 each bone's `bind_inverse` is
    /// nifly's compose-ready `transformSkinToBone`, which already encodes
    /// the skin-to-skeleton offset, so `SkinnedMesh::compute_palette_into`
    /// builds `bone_world × bind_inverse` alone; multiplying this transform
    /// in as well double-applies the offset on every legacy body NIF.
    /// `compute_palette_into`'s doc and its
    /// `palette_matches_nifly_skin_to_bone_semantics_with_non_identity_global`
    /// test are the authority. (OpenMW composes it as the outermost factor
    /// because its bind matrices omit that offset; the M41.0 Phase 1b.x
    /// history is in `byroredux/tests/skinning_e2e.rs`.)
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

/// FO3 / FNV dismemberment-cap body parts: nif.xml `BP_SECTIONCAP_*`
/// (101..=113) and `BP_TORSOCAP_*` (201..=213). Skyrim's 130..=161 /
/// 230..=261 biped slots are deliberately outside both ranges.
pub fn is_dismemberment_cap_body_part(body_part: u16) -> bool {
    matches!(body_part, 101..=113 | 201..=213)
}

impl ImportedMesh {
    /// Whether this mesh is a dismemberment cap: skinned geometry whose every
    /// triangle belongs to a cap body part. The legacy engine shows these
    /// gore sub-meshes (`limbcaps`, `bodycaps`, `headmeat`, `meatneck01`, …)
    /// only once the matching body part is severed.
    pub fn is_dismemberment_cap(&self) -> bool {
        self.skin.as_ref().is_some_and(|skin| {
            !skin.triangle_body_parts.is_empty()
                && skin
                    .triangle_body_parts
                    .iter()
                    .all(|&part| is_dismemberment_cap_body_part(part))
        })
    }

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
                if !start_index.is_multiple_of(3) {
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
        for (triangle, hidden) in self.indices.as_chunks::<3>().0.iter().zip(&hidden_triangles) {
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

#[cfg(test)]
mod fully_hidden_skin_tests {
    use super::*;

    /// A skin mesh with three triangles, each tagged with a Skyrim
    /// dismember body part.
    fn skin_mesh(body_parts: Vec<u16>) -> ImportedMesh {
        let triangle_count = body_parts.len();
        ImportedMesh {
            positions: vec![[0.0, 0.0, 0.0]; triangle_count * 3],
            colors: Vec::new(),
            normals: Vec::new(),
            tangents: Vec::new(),
            uvs: Vec::new(),
            indices: (0..(triangle_count * 3) as u32).collect(),
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: 1.0,
            material: ImportedMaterial::default(),
            name: Some("BODY".into()),
            parent_node: None,
            skin: Some(ImportedSkin {
                triangle_body_parts: body_parts,
                ..ImportedSkin::default()
            }),
            local_bound_center: [0.0; 3],
            local_bound_radius: 0.0,
            flags: 0,
            bs_lod_cutoffs: None,
            bs_sub_index: None,
            bs_geometry_lod_slot: None,
            billboard_mode: None,
            morph_targets: None,
        }
    }

    /// #3402 — a skin mesh whose every partition is covered by equipped
    /// gear legitimately ends up with **no triangles**.
    ///
    /// This is the real producer of the 23 zero-index meshes the runtime
    /// audit attributed to a broken SSE skinned decode. It is not a decode
    /// failure: re-parsing the exact shapes the audit named shows all of
    /// them leaving the importer *with* triangles (`MaleUnderwear_1`
    /// 417v/1548i, `FootMale_Big` 218v/948i, `HandFemale3rd` 872v/4344i,
    /// `FemaleUnderwear` 676v/2064i, `HandMaleBig3rd` 850v/4212i — one for
    /// one with its own vertex histogram). They are emptied here,
    /// afterwards, and correctly so. Consumers must therefore treat an
    /// empty `indices` as "nothing to draw", not as an error — which is
    /// what `nif_loader` now does instead of handing a zero-sized buffer
    /// to the allocator.
    #[test]
    fn a_fully_covered_skin_mesh_ends_up_with_no_triangles() {
        // Body (32) / Hands (33) / Feet (34) — biped bits 2, 3, 4.
        let mut mesh = skin_mesh(vec![32, 33, 34]);
        let full_cover = (1 << 2) | (1 << 3) | (1 << 4);

        let removed = mesh.hide_skin_partitions(full_cover);

        assert_eq!(removed, 3, "every triangle is covered");
        assert!(
            mesh.indices.is_empty(),
            "a fully-covered skin mesh has nothing left to draw",
        );
        assert_eq!(
            mesh.positions.len(),
            9,
            "vertices are deliberately left intact — only the draw list changes",
        );
    }

    /// The partial case must keep drawing, or the fix would be a
    /// regression: covering the torso still leaves hands and feet.
    #[test]
    fn a_partly_covered_skin_mesh_keeps_its_uncovered_triangles() {
        let mut mesh = skin_mesh(vec![32, 33, 34]);
        let torso_only = 1 << 2;

        let removed = mesh.hide_skin_partitions(torso_only);

        assert_eq!(removed, 1);
        assert_eq!(mesh.indices.len(), 6, "two triangles survive");
        assert_eq!(
            mesh.skin.as_ref().unwrap().triangle_body_parts,
            vec![33, 34],
            "the body-part map must stay aligned with the surviving triangles",
        );
    }
}
