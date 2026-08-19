//! FO4 precombined shared-geometry decode (M49).
//!
//! A vanilla FO4 `meshes\precombined\…_oc.nif` carries only a
//! `BSPackedCombinedSharedGeomDataExtra` header (vertex count, descriptor,
//! per-LOD triangle counts) plus a `(filename_hash, data_offset)` pointer
//! into a `<Plugin> - Geometry.csg` blob. The container read lives in
//! [`byroredux_bsa::CsgArchive`]; this module turns one object's
//! already-extracted `[verts][tris]` PSG slice into renderer-space
//! geometry.
//!
//! The PSG vertex stream is a standard packed `BSVertexData` — identical
//! to inline `BSTriShape` geometry except positions are stored as half
//! even when the descriptor sets `VF_FULL_PRECISION`. So the decode
//! reuses [`crate::blocks::tri_shape::decode_bs_vertex_stream`] with
//! `full_precision = false`, rather than duplicating the packed-vertex
//! logic. Full byte-layout spec: `docs/engine/fo4-csg-format.md`.

use crate::blocks::extra_data::{BsPackedCombinedGeomDataExtra, BsPackedCombinedPayload};
use crate::blocks::traits::HasObjectNET;
use crate::blocks::tri_shape::{check_vertex_desc_offsets, decode_bs_vertex_stream, BsTriShape};
use crate::header::NifHeader;
use crate::import::{ImportedMaterial, ImportedMesh};
use crate::scene::NifScene;
use crate::stream::NifStream;
use crate::types::NiTransform;
use crate::version::{bsver, NifVersion};
use byroredux_core::string::StringPool;
use std::io;
use std::sync::Arc;

/// `VF_FULL_PRECISION` within the 12-bit attribute mask
/// (`vertex_desc >> 44`). Private mirror of the same const in
/// `blocks::tri_shape::bs_tri_shape`.
const VF_FULL_PRECISION: u16 = 0x400;

/// One precombined object's geometry, in renderer **Y-up** space and
/// ready for mesh upload. Per-vertex arrays are parallel; `normals` /
/// `uvs` / `tangents` / `colors` are empty when the object's descriptor
/// lacks that attribute.
#[derive(Debug, Default, Clone)]
pub struct PrecombineGeometry {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// xyz = Y-up tangent, w = bitangent sign. Empty unless the
    /// descriptor carries both `VF_NORMALS` and `VF_TANGENTS`.
    pub tangents: Vec<[f32; 4]>,
    pub colors: Vec<[f32; 4]>,
    /// Flattened triangle indices (3 per triangle) for the **single**
    /// LOD the caller selected (the finest — highest triangle count). The
    /// other LODs are alternative triangulations of the same surface and
    /// are intentionally not included (rendering them together z-fights).
    pub indices: Vec<u32>,
}

impl PrecombineGeometry {
    /// Place this object as a spawnable [`ImportedMesh`] using one
    /// `BSPackedGeomDataCombined` instance transform. The geometry is
    /// already Y-up; the instance transform (raw Z-up from the NIF) is
    /// converted independently — translation via `zup_point_to_yup`,
    /// rotation via `zup_matrix_to_yup_quat` (which conjugates by the
    /// Z→Y axis swap) — matching how the node walk converts every other
    /// local transform, so composition with the cell origin stays
    /// correct. The caller supplies the owning shape's fully translated
    /// material payload, keeping precombined and ordinary geometry on the
    /// same import contract.
    pub fn into_imported_mesh(
        self,
        instance: &NiTransform,
        material: ImportedMaterial,
    ) -> ImportedMesh {
        let mut mesh = ImportedMesh::from_geometry(
            self.positions,
            self.colors,
            self.normals,
            self.tangents,
            self.uvs,
            self.indices,
        );
        mesh.translation = super::coord::zup_point_to_yup(&instance.translation);
        mesh.rotation = super::coord::zup_matrix_to_yup_quat(&instance.rotation);
        mesh.scale = instance.scale;
        mesh.material = material;
        mesh.name = Some(Arc::from("PrecombineObject"));
        mesh
    }
}

/// On-disk PSG per-vertex stride for an object whose runtime descriptor
/// is `vertex_desc`. The CSG stores positions as half4 (8 bytes) even
/// when `VF_FULL_PRECISION` would make the runtime vertex carry float4
/// (16 bytes), so the on-disk stride is 8 bytes shorter in that case.
pub fn psg_vertex_stride(vertex_desc: u64) -> usize {
    let runtime = (vertex_desc & 0xF) as usize * 4;
    let attrs = (vertex_desc >> 44) as u16;
    if attrs & VF_FULL_PRECISION != 0 {
        runtime.saturating_sub(8)
    } else {
        runtime
    }
}

/// Decode one shared-geometry object from its PSG slice: `num_verts`
/// packed vertices (stride [`psg_vertex_stride`]), then **one LOD's**
/// triangles — `tri_count` `u16`-triples starting `tri_start` triangles
/// into the concatenated `[LOD0][LOD1][LOD2]` triangle block.
/// `vertex_desc` is the descriptor from the `BSPackedSharedGeomData`
/// header. Returns Y-up geometry.
///
/// The three LODs are alternative triangulations of the *same* surface
/// (nif.xml: "switch a geometry at a specified distance"), so rendering
/// more than one z-fights — the caller picks a single LOD (finest =
/// highest triangle count) and passes its slice. `psg` must hold at
/// least `num_verts * stride + (tri_start + tri_count) * 6` bytes.
pub fn decode_shared_geom_object(
    psg: &[u8],
    vertex_desc: u64,
    num_verts: usize,
    tri_start: usize,
    tri_count: usize,
) -> io::Result<PrecombineGeometry> {
    let attrs = ((vertex_desc >> 44) & 0xFFF) as u16;
    let stride = psg_vertex_stride(vertex_desc);
    // #2578 — diagnostic-only; see `check_vertex_desc_offsets` doc comment.
    // Matches the `full_precision = false` / `is_skinned = false` forced
    // below: CSG positions are always half4 on disk (see `psg_vertex_stride`)
    // and precombines are never skinned.
    check_vertex_desc_offsets(vertex_desc, attrs, /* full_precision = */ false, /* is_skinned = */ false);

    // Detached FO4 version context — only satisfies NifStream's
    // constructor. The decode never consults bsver here: full_precision
    // is forced false and precombines are never skinned.
    let header = NifHeader::detached(NifVersion::V20_2_0_7, 12, bsver::FALLOUT4);
    let mut stream = NifStream::new(psg, &header);

    let decoded = decode_bs_vertex_stream(
        &mut stream,
        num_verts,
        attrs,
        stride,
        /* full_precision = */ false,
        /* is_skinned = */ false,
    )?;

    // Triangles follow the packed vertex block. Skip to the chosen LOD's
    // first triangle (`tri_start` × 6 bytes) and read only its `tri_count`.
    if tri_start > 0 {
        stream.skip(tri_start as u64 * 6)?;
    }
    let tris = stream.read_u16_triple_array(tri_count)?;
    let mut indices = Vec::with_capacity(tri_count * 3);
    for [a, b, c] in tris {
        indices.push(a as u32);
        indices.push(b as u32);
        indices.push(c as u32);
    }

    // #1533 — reject decode-time index/vertex inconsistency. The PSG slice
    // is located by a `(filename_hash, data_offset)` pointer into a separate
    // `.csg` blob, so a hash collision or stale/mispointed offset decodes
    // arbitrary bytes as u16 indices (up to 65535) against this object's
    // `num_verts`. Caught here, it's a rejected object; let through, it
    // becomes an OOB raster/BLAS read in the shared global pool (the
    // producer half of #1532 — whose `accumulate_global_geometry` guard now
    // clamps, but the precombine path should fail loud rather than silently
    // render a mispointed object's garbage triangles).
    if let Some(&bad) = indices.iter().find(|&&i| i as usize >= num_verts) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "precombine PSG triangle index {bad} >= num_verts {num_verts} \
                 (tri_start {tri_start}, tri_count {tri_count}) — corrupt or \
                 mispointed CSG slice",
            ),
        ));
    }

    // Z-up (Gamebryo) → Y-up (renderer), reusing the shared converters.
    let positions = decoded
        .vertices
        .iter()
        .map(super::coord::zup_point_to_yup)
        .collect();
    let normals = decoded
        .normals
        .iter()
        .map(super::coord::zup_point_to_yup)
        .collect();
    let tangents = crate::import::mesh::bs_tangents_zup_to_yup(&decoded.tangents);

    Ok(PrecombineGeometry {
        positions,
        normals,
        uvs: decoded.uvs,
        tangents,
        colors: decoded.vertex_colors,
        indices,
    })
}

/// One shared-geometry object resolved from an `_oc.nif` scene: where its
/// vertex/triangle data lives in the CSG, the LOD table, the per-instance
/// placements, and the material from the owning shape. The cell loader
/// reads the geometry from the CSG (`byroredux_bsa`) and builds spawnable
/// meshes — see `byroredux::cell_loader::precombined`.
#[derive(Debug, Clone)]
pub struct PrecombineGeomRef {
    pub filename_hash: u32,
    pub data_offset: u32,
    pub num_verts: usize,
    pub vertex_desc: u64,
    /// Triangle count per LOD level (LOD0/1/2).
    pub lod_counts: [u32; 3],
    /// Triangle index-unit offset per LOD level within the block.
    pub lod_offsets: [u32; 3],
    /// Per-`BSPackedGeomDataCombined` placements (raw Z-up), including
    /// the authored bound used to validate transform interpretation.
    pub instances: Vec<PrecombineInstance>,
    pub material: ImportedMaterial,
}

/// One placement from `BSPackedGeomDataCombined`.
///
/// Keeping the authored sphere alongside the transform is important: it
/// gives importers and diagnostics a geometry-independent ground truth for
/// validating matrix layout and transform composition. Previously this was
/// discarded at the NIF-to-runtime boundary.
#[derive(Debug, Clone, Copy)]
pub struct PrecombineInstance {
    pub grayscale_to_palette_scale: f32,
    pub transform: NiTransform,
    /// Raw Gamebryo Z-up `[center.x, center.y, center.z, radius]`.
    pub bounding_sphere: [f32; 4],
}

/// Walk an `_oc.nif` scene and collect every shared-geometry object with
/// its CSG location, LOD table, placements, and resolved material (M49).
/// Each `BSPackedCombinedSharedGeomDataExtra` block is paired with the
/// shape whose `extra_data_refs` points at it, and that shape's
/// shader/alpha properties resolve the material via the standard
/// `extract_material_info_from_refs` path. Blocks with no owning shape
/// fall back to an empty (untextured) material so geometry is never lost.
pub fn collect_precombine_geom_refs(
    scene: &NifScene,
    pool: &mut StringPool,
) -> Vec<PrecombineGeomRef> {
    let mut out = Vec::new();
    for (idx, block) in scene.blocks.iter().enumerate() {
        let Some(packed) = block
            .as_any()
            .downcast_ref::<BsPackedCombinedGeomDataExtra>()
        else {
            continue;
        };
        let BsPackedCombinedPayload::Shared { objects, data } = &packed.payload else {
            continue;
        };
        let material = find_owning_shape(scene, idx)
            .map(|shape| precombine_material_from_shape(scene, shape, pool))
            .unwrap_or_default();
        for (obj, hdr) in objects.iter().zip(data.iter()) {
            out.push(PrecombineGeomRef {
                filename_hash: obj.filename_hash,
                data_offset: obj.data_offset,
                num_verts: hdr.num_verts as usize,
                vertex_desc: hdr.vertex_desc,
                lod_counts: [hdr.tri_count_lod0, hdr.tri_count_lod1, hdr.tri_count_lod2],
                lod_offsets: [
                    hdr.tri_offset_lod0,
                    hdr.tri_offset_lod1,
                    hdr.tri_offset_lod2,
                ],
                instances: hdr
                    .combined
                    .iter()
                    .map(|c| PrecombineInstance {
                        grayscale_to_palette_scale: c.grayscale_to_palette_scale,
                        transform: c.transform,
                        bounding_sphere: c.bounding_sphere,
                    })
                    .collect(),
                material: material.clone(),
            });
        }
    }
    out
}

/// The distinct `BSPackedGeomObject::filename_hash` values an `_oc.nif`
/// points at, in first-seen order.
///
/// This is the cheap half of [`collect_precombine_geom_refs`] — no owning-shape
/// search, no material translation, no string pool — so a caller can decide
/// *which* `<Plugin> - Geometry.csg` to open before paying for the full walk.
/// It matters because the CSG a precombine reads from is not implied by the
/// cell's owning plugin: a plugin that re-bakes a master-owned cell keeps the
/// root `meshes\precombined\` filename but stores the geometry in its own blob.
pub fn precombine_csg_filename_hashes(scene: &NifScene) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::new();
    for block in &scene.blocks {
        let Some(packed) = block
            .as_any()
            .downcast_ref::<BsPackedCombinedGeomDataExtra>()
        else {
            continue;
        };
        let BsPackedCombinedPayload::Shared { objects, .. } = &packed.payload else {
            continue;
        };
        for obj in objects {
            if !out.contains(&obj.filename_hash) {
                out.push(obj.filename_hash);
            }
        }
    }
    out
}

/// Find the shape (`BSMeshLODTriShape` / `BSTriShape`) whose
/// `extra_data_refs` claims the packed-combined block at `packed_idx`.
fn find_owning_shape(scene: &NifScene, packed_idx: usize) -> Option<&BsTriShape> {
    scene.blocks.iter().find_map(|b| {
        let shape = b.as_any().downcast_ref::<BsTriShape>()?;
        shape
            .extra_data_refs()
            .iter()
            .any(|r| r.index() == Some(packed_idx))
            .then_some(shape)
    })
}

/// Resolve a precombine shape's material through the same full translation
/// boundary used by inline BSTriShape geometry.
fn precombine_material_from_shape(
    scene: &NifScene,
    shape: &BsTriShape,
    pool: &mut StringPool,
) -> ImportedMaterial {
    let mat = super::material::extract_material_info_from_refs(
        scene,
        shape.shader_property_ref,
        shape.alpha_property_ref,
        &[],
        &[],
        pool,
    );
    mat.into_imported_material(pool, shape.av.net.name.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1188 follow-up — material attachment must preserve all translated
    /// flags, including decal/two-sided routing.
    #[test]
    fn mesh_construction_attaches_full_material() {
        let material = ImportedMaterial {
            is_decal: true,
            two_sided: true,
            emissive_mult: 7.0,
            material_kind: 16,
            ..Default::default()
        };
        let mesh =
            PrecombineGeometry::default().into_imported_mesh(&NiTransform::default(), material);
        assert!(mesh.material.is_decal);
        assert!(mesh.material.two_sided);
        assert_eq!(mesh.material.emissive_mult, 7.0);
        assert_eq!(mesh.material.material_kind, 16);
    }

    fn half(f: f32) -> [u8; 2] {
        // Encode f32 → IEEE-754 binary16 (round-to-nearest, no Inf/NaN
        // handling needed for the small test values used here).
        let bits = f.to_bits();
        let sign = ((bits >> 16) & 0x8000) as u16;
        let exp = ((bits >> 23) & 0xFF) as i32 - 127 + 15;
        let mant = (bits >> 13) & 0x3FF;
        let h = if exp <= 0 {
            sign
        } else if exp >= 0x1F {
            sign | 0x7C00
        } else {
            sign | ((exp as u16) << 10) | mant as u16
        };
        h.to_le_bytes()
    }

    fn nbyte(n: f32) -> u8 {
        (((n + 1.0) * 127.5).round()).clamp(0.0, 255.0) as u8
    }

    /// Hand-build a 2-vertex, 0-color (stride 20) PSG slice with known
    /// position / UV / normal and one degenerate triangle, then decode
    /// and assert the Y-up output. Exercises the whole reuse path:
    /// `decode_bs_vertex_stream` → coord conversion.
    #[test]
    fn decode_two_vertex_no_color_object() {
        // VERTEX|UV|NORMAL|TANGENT|FULL_PRECISION, runtime stride 28 → psg 20.
        let vertex_desc: u64 = 0x0041_b000_0065_0407;
        assert_eq!(psg_vertex_stride(vertex_desc), 20);

        let mut psg = Vec::new();
        // vertex 0: pos (1,2,3) Z-up, bitangentX, uv (0.5,0.25), normal +Z, tangent +X
        for (px, py, pz) in [(1.0f32, 2.0, 3.0), (-4.0, 5.0, -6.0)] {
            psg.extend_from_slice(&half(px));
            psg.extend_from_slice(&half(py));
            psg.extend_from_slice(&half(pz));
            psg.extend_from_slice(&half(1.0)); // bitangent.x
            psg.extend_from_slice(&half(0.5)); // u
            psg.extend_from_slice(&half(0.25)); // v
            psg.extend_from_slice(&[nbyte(0.0), nbyte(0.0), nbyte(1.0), nbyte(0.0)]); // normal +Z, bitY
            psg.extend_from_slice(&[nbyte(1.0), nbyte(0.0), nbyte(0.0), nbyte(0.0)]);
            // tangent +X, bitZ
        }
        assert_eq!(psg.len(), 2 * 20);
        // one triangle (0,1,0)
        psg.extend_from_slice(&0u16.to_le_bytes());
        psg.extend_from_slice(&1u16.to_le_bytes());
        psg.extend_from_slice(&0u16.to_le_bytes());

        let g = decode_shared_geom_object(&psg, vertex_desc, 2, 0, 1).unwrap();
        assert_eq!(g.positions.len(), 2);
        assert_eq!(g.indices, vec![0, 1, 0]);
        assert_eq!(g.uvs[0], [0.5, 0.25]);
        // Z-up (1,2,3) → Y-up. zup_point_to_yup maps (x,y,z) → (x,z,-y)
        // (verify against the shared converter's contract, not a guess).
        let yup0 = g.positions[0];
        assert!((yup0[0] - 1.0).abs() < 0.01, "x preserved, got {yup0:?}");
        // normal was +Z in Z-up; after conversion it should be unit length.
        let n = g.normals[0];
        let nlen = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        assert!((nlen - 1.0).abs() < 0.02, "normal unit length, got {nlen}");
        assert_eq!(g.colors.len(), 0, "no VF_COLORS → empty colors");
        assert_eq!(g.tangents.len(), 2, "VF_TANGENTS+VF_NORMALS → tangents");
    }

    /// #1533 — an out-of-range triangle index (a corrupt / mispointed CSG
    /// slice decoding garbage as indices) must be rejected at decode time,
    /// not flow on to become an OOB raster/BLAS read. Same 2-vertex slice as
    /// above, but the triangle references vertex 9 against num_verts 2.
    #[test]
    fn out_of_range_triangle_index_is_rejected() {
        let vertex_desc: u64 = 0x0041_b000_0065_0407;
        let mut psg = Vec::new();
        for (px, py, pz) in [(1.0f32, 2.0, 3.0), (-4.0, 5.0, -6.0)] {
            psg.extend_from_slice(&half(px));
            psg.extend_from_slice(&half(py));
            psg.extend_from_slice(&half(pz));
            psg.extend_from_slice(&half(1.0));
            psg.extend_from_slice(&half(0.5));
            psg.extend_from_slice(&half(0.25));
            psg.extend_from_slice(&[nbyte(0.0), nbyte(0.0), nbyte(1.0), nbyte(0.0)]);
            psg.extend_from_slice(&[nbyte(1.0), nbyte(0.0), nbyte(0.0), nbyte(0.0)]);
        }
        // one triangle (0, 9, 0) — index 9 overshoots the 2-vertex block.
        psg.extend_from_slice(&0u16.to_le_bytes());
        psg.extend_from_slice(&9u16.to_le_bytes());
        psg.extend_from_slice(&0u16.to_le_bytes());

        let err = decode_shared_geom_object(&psg, vertex_desc, 2, 0, 1)
            .expect_err("OOB index must be rejected");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(
            err.to_string().contains("index 9"),
            "message names the offending index: {err}",
        );
    }

    #[test]
    fn stride_formula_handles_colors_and_no_fullprec() {
        // colored + fullprec: runtime 32 → psg 24.
        assert_eq!(psg_vertex_stride(0x0043_b000_0765_0408), 24);
        // no fullprec (hypothetical): stride nibble 5 → 20, unchanged.
        assert_eq!(psg_vertex_stride(0x0001_b000_0065_0005), 20);
    }
}
