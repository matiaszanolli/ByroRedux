//! `BSGeometry` — Starfield-era replacement for `BSTriShape`.
//!
//! Starfield (and forward) replaced the FO4-era `BSTriShape` /
//! `BSSubIndexTriShape` family with a new top-level mesh container
//! that splits the geometry data out of the `.nif` and into companion
//! `.mesh` files. The `.nif` itself only carries bounds, three
//! property/skin refs, and up to 4 `BSGeometryMesh` slots; each slot
//! holds either an external `.mesh` filename (the 99% Starfield case)
//! or — when bit `0x200` of the parent's NiAVObject flags is set — an
//! inline `BSGeometryMeshData` payload (UDEC3-packed normals/tangents,
//! half-float UVs, meshlets, cull data).
//!
//! ## Authority
//!
//! nif.xml does **not** define a top-level `<niobject name="BSGeometry">`
//! — only a comment inside `NiGeometry` (line 3855) describing the
//! rename. The wire layout this parser implements is taken from
//! [`nifly`](https://github.com/ousnius/nifly), the niftools fork that
//! has Starfield read/write support:
//!
//!   * `include/Geometry.hpp` — class definitions
//!   * `src/Geometry.cpp::BSGeometry::Sync` (line 1769)
//!   * `src/Geometry.cpp::BSGeometryMesh::Sync` (line 1754)
//!   * `src/Geometry.cpp::BSGeometryMeshData::Sync` (line 1596)
//!
//! Per-archive 2026-04-26 baseline:
//!   * `Starfield - Meshes01.ba2`  — 190 549 occurrences (24.74% of all blocks)
//!   * `Starfield - FaceMeshes.ba2` —  13 713 occurrences (14.27% of all blocks)
//!
//! Pre-#708 every one fell to `NiUnknown` and the entire mesh body
//! was discarded on read.

use super::base::NiAVObjectData;
use super::tri_shape::half_to_f32;
use super::{traits, NiObject};
use crate::stream::NifStream;
use crate::types::{BlockRef, NiTransform};
use std::any::Any;
use std::io;

/// Bit on the parent NiAVObject's `flags` field that switches
/// `BSGeometryMesh` between external (`meshName`) and inline
/// (`BSGeometryMeshData`) payload. Per nifly `BSGeometry::HasInternalGeomData`.
const FLAG_INTERNAL_GEOM_DATA: u32 = 0x200;

/// `BSGeometry` block (Starfield+). Top-level scene-graph mesh container.
#[derive(Debug)]
pub struct BSGeometry {
    /// `NiObjectNET` + `NiAVObject` base fields. The `flags` member
    /// here is read for the `0x200` internal-geom-data gate.
    pub av: NiAVObjectData,
    /// Per-mesh-LOD bounding sphere. `(center, radius)`.
    pub bounding_sphere: ([f32; 3], f32),
    /// `boundMinMax`: 6 contiguous floats nifly carries verbatim. Per
    /// the C++ source these are unbounded — the field is private and
    /// not consumed by any export or rendering path; we keep the raw
    /// values so a future LOD selector can use them.
    pub bound_min_max: [f32; 6],
    /// Skin instance ref (`NiBoneContainer`).
    pub skin_instance_ref: BlockRef,
    /// Shader-property ref (`BSShaderProperty` / `BSLightingShaderProperty`).
    pub shader_property_ref: BlockRef,
    /// Alpha-property ref (`NiAlphaProperty`).
    pub alpha_property_ref: BlockRef,
    /// Up to 4 mesh LOD slots. Each is either an external `.mesh`
    /// file reference or an inline mesh body. Vec is empty when all
    /// 4 slot bytes were zero.
    pub meshes: Vec<BSGeometryMesh>,
}

impl BSGeometry {
    /// `true` when bit `0x200` of `av.flags` is set — the parent
    /// NiAVObject's flag the per-mesh decode reads to choose between
    /// the `meshName` external-file branch and the inline
    /// `BSGeometryMeshData` branch.
    pub fn has_internal_geom_data(&self) -> bool {
        (self.av.flags & FLAG_INTERNAL_GEOM_DATA) != 0
    }

    /// Parse a `BSGeometry` from `stream`.
    ///
    /// The pre-mesh prefix is the standard `NiAVObject` Skyrim+ shape
    /// (no properties list — same call as `BSTriShape`) followed by
    /// the bounding-sphere + bound-min-max + 3 refs that distinguish
    /// `BSGeometry` from its predecessors.
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let av = NiAVObjectData::parse_no_properties(stream)?;

        let bs_center = stream.read_ni_point3()?;
        let bs_radius = stream.read_f32_le()?;
        let bounding_sphere = ([bs_center.x, bs_center.y, bs_center.z], bs_radius);

        let mut bound_min_max = [0.0f32; 6];
        for slot in &mut bound_min_max {
            *slot = stream.read_f32_le()?;
        }

        let skin_instance_ref = stream.read_block_ref()?;
        let shader_property_ref = stream.read_block_ref()?;
        let alpha_property_ref = stream.read_block_ref()?;

        // Up to 4 mesh-LOD slots. nifly: `for i in 0..4 { stream.Sync(testByte);
        // if (testByte) meshes[i].Sync(stream); }`. The `testByte` is a u8
        // boolean — present (1) or absent (0).
        let internal = (av.flags & FLAG_INTERNAL_GEOM_DATA) != 0;
        let mut meshes = Vec::new();
        for _ in 0..4 {
            let test_byte = stream.read_u8()?;
            if test_byte != 0 {
                meshes.push(BSGeometryMesh::parse(stream, internal)?);
            }
        }

        Ok(Self {
            av,
            bounding_sphere,
            bound_min_max,
            skin_instance_ref,
            shader_property_ref,
            alpha_property_ref,
            meshes,
        })
    }
}

impl NiObject for BSGeometry {
    fn block_type_name(&self) -> &'static str {
        "BSGeometry"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_object_net(&self) -> Option<&dyn traits::HasObjectNET> {
        Some(self)
    }
    fn as_av_object(&self) -> Option<&dyn traits::HasAVObject> {
        Some(self)
    }
    fn as_shader_refs(&self) -> Option<&dyn traits::HasShaderRefs> {
        Some(self)
    }
}

impl traits::HasObjectNET for BSGeometry {
    fn name(&self) -> Option<&str> {
        self.av.net.name.as_deref()
    }
    fn name_arc(&self) -> Option<&std::sync::Arc<str>> {
        self.av.net.name.as_ref()
    }
    fn extra_data_refs(&self) -> &[BlockRef] {
        &self.av.net.extra_data_refs
    }
    fn controller_ref(&self) -> BlockRef {
        self.av.net.controller_ref
    }
}

impl traits::HasAVObject for BSGeometry {
    fn flags(&self) -> u32 {
        self.av.flags
    }
    fn transform(&self) -> &NiTransform {
        &self.av.transform
    }
    fn properties(&self) -> &[BlockRef] {
        &[]
    }
    fn collision_ref(&self) -> BlockRef {
        self.av.collision_ref
    }
}

impl traits::HasShaderRefs for BSGeometry {
    fn shader_property_ref(&self) -> BlockRef {
        self.shader_property_ref
    }
    fn alpha_property_ref(&self) -> BlockRef {
        self.alpha_property_ref
    }
}

/// One LOD slot of a `BSGeometry`. Carries either an external `.mesh`
/// file reference or — when the parent's flag `0x200` is set — the full
/// inline geometry body.
#[derive(Debug)]
pub struct BSGeometryMesh {
    /// Triangle-index byte size hint (always present, regardless of
    /// internal/external).
    pub tri_size: u32,
    /// Vertex-count hint (always present).
    pub num_verts: u32,
    /// Per-mesh flags. Distinct from `BSGeometry.av.flags` — nifly
    /// notes this is "often 64". Stored verbatim for downstream use.
    pub flags: u32,
    /// External `.mesh` file reference OR inline mesh body, gated by
    /// the parent `BSGeometry.av.flags & 0x200`.
    pub kind: BSGeometryMeshKind,
}

/// Branch of [`BSGeometryMesh`] — external file path or inline mesh body.
#[derive(Debug)]
pub enum BSGeometryMeshKind {
    /// External `.mesh` file reference. Holds either a 41-character
    /// SHA-1 hex name (the vanilla Starfield convention) or a
    /// human-readable path. Mirrors `BSGeometryMesh::meshName`.
    External { mesh_name: String },
    /// Inline mesh body. Present when the parent's NiAVObject flag
    /// `0x200` is set. Vanilla Starfield does **not** ship inline
    /// data — every mesh references an external `.mesh` file — but
    /// the format permits it and authoring tools / ports may emit it.
    Internal { mesh_data: BSGeometryMeshData },
}

impl BSGeometryMesh {
    /// Parse one mesh-LOD slot. `internal` is the parent's
    /// `flags & 0x200`-derived gate.
    pub fn parse(stream: &mut NifStream, internal: bool) -> io::Result<Self> {
        let tri_size = stream.read_u32_le()?;
        let num_verts = stream.read_u32_le()?;
        let flags = stream.read_u32_le()?;

        let kind = if internal {
            BSGeometryMeshKind::Internal {
                mesh_data: BSGeometryMeshData::parse(stream)?,
            }
        } else {
            // `meshName.Sync(stream, 4)` in nifly — a u32 length-prefixed
            // inline string. Same as `read_sized_string`.
            let mesh_name = stream.read_sized_string()?;
            BSGeometryMeshKind::External { mesh_name }
        };

        Ok(Self {
            tri_size,
            num_verts,
            flags,
            kind,
        })
    }
}

/// Inline mesh body. Mirrors `nifly::BSGeometryMeshData` — the
/// version-gated Starfield mesh-data payload.
///
/// Storage choices: vertex positions are decoded from i16 NORM
/// (per-axis scale = `havok_scale * scale * (norm / 32768)`) into
/// `[f32; 3]` as they're read. Normals and tangents are stored as
/// the raw u32 UDEC3 values; the renderer can unpack via
/// [`unpack_udec3_xyzw`] when consumed. Half-float UVs are unpacked
/// to f32 immediately because there's no value in deferring.
#[derive(Debug)]
pub struct BSGeometryMeshData {
    /// Format version. nifly bails out early if `version > 2` so the
    /// rest of the body is opt-out for unrecognised stream versions.
    pub version: u32,
    /// Triangle list. Each triangle is three u16 vertex indices.
    pub triangles: Vec<[u16; 3]>,
    /// Position scale factor written to disk (multiplied by the
    /// hard-coded havok-units constant when decoding vertex coords).
    /// `<= 0.0` is a sentinel for "no body follows" — mirrors nifly's
    /// `if (scale <= 0.0f) return;` early-out.
    pub scale: f32,
    /// Number of bone weights stored per vertex. `0` means no skin
    /// weights are serialised.
    pub weights_per_vert: u32,
    /// Decoded vertex positions in mesh-local Y-up units (havok-scaled).
    pub vertices: Vec<[f32; 3]>,
    /// Primary UV channel (decoded from half-floats).
    pub uvs0: Vec<[f32; 2]>,
    /// Secondary UV channel (decoded from half-floats).
    pub uvs1: Vec<[f32; 2]>,
    /// Vertex colours (`ByteColor4` — 4×u8, raw).
    pub colors: Vec<[u8; 4]>,
    /// Raw UDEC3 packed normals (10:10:10:2 unsigned-fixed-point).
    /// Use [`unpack_udec3_xyzw`] for the per-channel `[-1, 1]` floats.
    pub normals_raw: Vec<u32>,
    /// Raw UDEC3 packed tangents (10:10:10:2). The 2-bit `W` channel
    /// is the bitangent sign.
    pub tangents_raw: Vec<u32>,
    /// Per-vertex bone weights. Outer index = vertex; inner length =
    /// [`Self::weights_per_vert`].
    pub skin_weights: Vec<Vec<BoneWeight>>,
    /// Optional LOD triangle lists. Each entry is a full reduced
    /// triangle list at the corresponding LOD level.
    pub lods: Vec<Vec<[u16; 3]>>,
    /// Meshlet table (DirectX-style cluster culling primitives).
    pub meshlets: Vec<Meshlet>,
    /// Cull-data per meshlet — `(center, expand)` Vector3 pair.
    pub cull_data: Vec<CullData>,
}

/// One bone-weight entry. `bone_index` is the slot in the parent
/// skin-instance bones array; `weight` is u16 NORM (`/ 65535.0`).
///
/// `#[repr(C)]` + `Default` so the parser can bulk-read the flat
/// per-vertex weight array via `read_pod_vec::<BoneWeight>(n)` —
/// 4 bytes per entry (2× u16, no padding), matches the on-disk
/// little-endian layout. See #873.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct BoneWeight {
    pub bone_index: u16,
    pub weight: u16,
}

/// Meshlet entry — DirectX 12 cluster-culling layout.
///
/// `#[repr(C)]` + `Default` so `read_pod_vec::<Meshlet>(n)` lands
/// the bulk read in one `read_exact`. 16 bytes per entry (4× u32,
/// no padding). See #873.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct Meshlet {
    pub vert_count: u32,
    pub vert_offset: u32,
    pub prim_count: u32,
    pub prim_offset: u32,
}

/// Per-meshlet bounding cull data — `center` + `expand` (axis-aligned
/// half-extents).
///
/// `#[repr(C)]` + `Default` so `read_pod_vec::<CullData>(n)` lands
/// the bulk read in one `read_exact`. 24 bytes per entry (6× f32 as
/// two `[f32; 3]` arrays, no padding). See #873.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct CullData {
    pub center: [f32; 3],
    pub expand: [f32; 3],
}

impl BSGeometryMeshData {
    /// Hard-coded havok-scale constant from nifly. Starfield `.mesh`
    /// files are normalised to metric units; this scale brings the
    /// default vertex positions back to Skyrim-equivalent sizes.
    /// Per `BSGeometryMeshData::havokScale` (private member).
    const HAVOK_SCALE: f32 = 69.969;

    /// Parse a standalone external `.mesh` file (SF-D4-02 Stage B).
    ///
    /// The external `.mesh` format is identical to the inline `BSGeometryMeshData`
    /// body — nifly's `NifFile::LoadExternalShapeData` calls `meshData.Sync(s)`
    /// directly on the raw stream with no wrapper header or magic number.
    pub fn parse_from_bytes(bytes: &[u8]) -> io::Result<Self> {
        use crate::header::NifHeader;
        use crate::version::NifVersion;
        let header = NifHeader {
            version: NifVersion::V20_2_0_7,
            little_endian: true,
            user_version: 12,
            user_version_2: 172,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        };
        let mut stream = NifStream::new(bytes, &header);
        Self::parse(&mut stream)
    }

    fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let version = stream.read_u32_le()?;
        // nifly: `if (version > 2) return;` — leave every body field
        // empty so the parent record can still finalise.
        if version > 2 {
            return Ok(Self::empty(version));
        }

        let n_tri_indices = stream.read_u32_le()?;
        let tri_count = (n_tri_indices / 3) as usize;
        let mut triangles = stream.allocate_vec::<[u16; 3]>(tri_count as u32)?;
        for _ in 0..tri_count {
            let a = stream.read_u16_le()?;
            let b = stream.read_u16_le()?;
            let c = stream.read_u16_le()?;
            triangles.push([a, b, c]);
        }

        let scale = stream.read_f32_le()?;
        // Sentinel: scale ≤ 0 means "the rest of the body is absent".
        // Vanilla Starfield uses this for the segment-only / skin-weight-only
        // mesh slots that share a parent BSGeometry with a populated slot.
        if scale <= 0.0 {
            return Ok(Self {
                version,
                triangles,
                scale,
                weights_per_vert: 0,
                vertices: Vec::new(),
                uvs0: Vec::new(),
                uvs1: Vec::new(),
                colors: Vec::new(),
                normals_raw: Vec::new(),
                tangents_raw: Vec::new(),
                skin_weights: Vec::new(),
                lods: Vec::new(),
                meshlets: Vec::new(),
                cull_data: Vec::new(),
            });
        }

        let weights_per_vert = stream.read_u32_le()?;

        let n_vertices = stream.read_u32_le()?;
        let mut vertices = stream.allocate_vec::<[f32; 3]>(n_vertices)?;
        for _ in 0..n_vertices {
            let x = unpack_norm_i16(stream.read_u16_le()? as i16, scale, Self::HAVOK_SCALE);
            let y = unpack_norm_i16(stream.read_u16_le()? as i16, scale, Self::HAVOK_SCALE);
            let z = unpack_norm_i16(stream.read_u16_le()? as i16, scale, Self::HAVOK_SCALE);
            vertices.push([x, y, z]);
        }

        let n_uv1 = stream.read_u32_le()?;
        let mut uvs0 = stream.allocate_vec::<[f32; 2]>(n_uv1)?;
        for _ in 0..n_uv1 {
            let u = half_to_f32(stream.read_u16_le()?);
            let v = half_to_f32(stream.read_u16_le()?);
            uvs0.push([u, v]);
        }

        let n_uv2 = stream.read_u32_le()?;
        let mut uvs1 = stream.allocate_vec::<[f32; 2]>(n_uv2)?;
        for _ in 0..n_uv2 {
            let u = half_to_f32(stream.read_u16_le()?);
            let v = half_to_f32(stream.read_u16_le()?);
            uvs1.push([u, v]);
        }

        // Bulk reads — `[u8; 4]` and `u32` are POD; one `read_exact`
        // each replaces the per-element push loops. The `allocate_vec`
        // calls would have done their own check_alloc anyway; the bulk
        // readers do the equivalent check inside `read_pod_vec`. See
        // #873 (NIF-PERF-09).
        let n_colors = stream.read_u32_le()?;
        let colors = stream.read_u8_quad_array(n_colors as usize)?;

        let n_normals = stream.read_u32_le()?;
        let normals_raw = stream.read_u32_array(n_normals as usize)?;

        let n_tangents = stream.read_u32_le()?;
        let tangents_raw = stream.read_u32_array(n_tangents as usize)?;

        let n_total_weights = stream.read_u32_le()?;
        // Per nifly: weight count is interpreted as a *flat* count of
        // BoneWeight entries; the per-vertex grouping is `weights_per_vert`.
        // When `weights_per_vert == 0` we skip the resize (no skin weights).
        let mut skin_weights: Vec<Vec<BoneWeight>> = if weights_per_vert > 0 {
            let outer_len = n_total_weights / weights_per_vert;
            stream.allocate_vec::<Vec<BoneWeight>>(outer_len)?
        } else {
            Vec::new()
        };
        if weights_per_vert > 0 {
            let outer_len = (n_total_weights / weights_per_vert) as usize;
            for _ in 0..outer_len {
                // #768: route the inner allocation through `allocate_vec`
                // so a hostile `weights_per_vert = 0xFFFFFFFF` cannot
                // OOM-panic the process before the inner u16 reads
                // fail. The outer `outer_len` already goes through
                // `allocate_vec` at line 443 (since #764), but
                // `Vec::with_capacity(weights_per_vert)` here was an
                // unbounded sibling — companion fix to #764.
                let mut row = stream.allocate_vec::<BoneWeight>(weights_per_vert)?;
                for _ in 0..weights_per_vert {
                    let bone_index = stream.read_u16_le()?;
                    let weight = stream.read_u16_le()?;
                    row.push(BoneWeight { bone_index, weight });
                }
                skin_weights.push(row);
            }
        }

        let n_lods = stream.read_u32_le()?;
        let mut lods = stream.allocate_vec::<Vec<[u16; 3]>>(n_lods)?;
        for _ in 0..n_lods {
            let n_lod_tri_indices = stream.read_u32_le()?;
            let lod_tri_count = n_lod_tri_indices as usize / 3;
            // Bulk read 3-u16 triangles — `read_u16_triple_array` does
            // its own check_alloc against the byte budget. See #874 +
            // #873.
            let tris = stream.read_u16_triple_array(lod_tri_count)?;
            lods.push(tris);
        }

        // Bulk struct reads — `Meshlet` (4 × u32 = 16 B) and `CullData`
        // (6 × f32 = 24 B) are `#[repr(C)] + Default + Copy` and decode
        // in one `read_pod_vec` per array. See #873.
        let n_meshlets = stream.read_u32_le()?;
        let meshlets = stream.read_pod_vec::<Meshlet>(n_meshlets as usize)?;

        let n_cull_data = stream.read_u32_le()?;
        let cull_data = stream.read_pod_vec::<CullData>(n_cull_data as usize)?;

        Ok(Self {
            version,
            triangles,
            scale,
            weights_per_vert,
            vertices,
            uvs0,
            uvs1,
            colors,
            normals_raw,
            tangents_raw,
            skin_weights,
            lods,
            meshlets,
            cull_data,
        })
    }

    /// All-empty body for unrecognised version numbers.
    fn empty(version: u32) -> Self {
        Self {
            version,
            triangles: Vec::new(),
            scale: 0.0,
            weights_per_vert: 0,
            vertices: Vec::new(),
            uvs0: Vec::new(),
            uvs1: Vec::new(),
            colors: Vec::new(),
            normals_raw: Vec::new(),
            tangents_raw: Vec::new(),
            skin_weights: Vec::new(),
            lods: Vec::new(),
            meshlets: Vec::new(),
            cull_data: Vec::new(),
        }
    }
}

/// Decode a single i16 NORM-encoded position component back to a
/// world-space float. Mirrors nifly's `unpack` lambda inside
/// `BSGeometryMeshData::Sync` (line 1644). Negative i16 divides by
/// `32768`, non-negative by `32767`.
fn unpack_norm_i16(raw: i16, scale: f32, axis_scale: f32) -> f32 {
    if raw < 0 {
        (raw as f32 / 32768.0) * scale * axis_scale
    } else {
        (raw as f32 / 32767.0) * scale * axis_scale
    }
}

/// Unpack a UDEC3 (10:10:10:2 unsigned-fixed) word into per-channel
/// `[-1, 1]` floats. The 2-bit `W` channel becomes the 4th component
/// (used as the bitangent-sign on tangents). Per the FIXME at
/// `BSGeometryMeshData::Sync` line 1709 — Bethesda stores unsigned
/// fixed-point and expects the consumer to map back to signed range.
pub fn unpack_udec3_xyzw(raw: u32) -> [f32; 4] {
    let x = (raw & 0x3FF) as f32;
    let y = ((raw >> 10) & 0x3FF) as f32;
    let z = ((raw >> 20) & 0x3FF) as f32;
    let w = ((raw >> 30) & 0x3) as f32;
    [
        (x / 1023.0) * 2.0 - 1.0,
        (y / 1023.0) * 2.0 - 1.0,
        (z / 1023.0) * 2.0 - 1.0,
        (w / 3.0) * 2.0 - 1.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpack_udec3_zero_maps_to_minus_one() {
        // raw=0 → all channels at the minimum of the unsigned
        // 10-bit (or 2-bit for W) range → -1.0 after the
        // [0, 1023] → [-1, 1] remap.
        let unp = unpack_udec3_xyzw(0);
        assert_eq!(unp, [-1.0, -1.0, -1.0, -1.0]);
    }

    #[test]
    fn unpack_udec3_max_maps_to_plus_one() {
        // raw=0xFFFFFFFF → every channel at its max
        // (1023 for the three 10-bit slots, 3 for the 2-bit W).
        // All map back to +1.0 exactly.
        let unp = unpack_udec3_xyzw(0xFFFF_FFFF);
        assert!((unp[0] - 1.0).abs() < 1e-6);
        assert!((unp[1] - 1.0).abs() < 1e-6);
        assert!((unp[2] - 1.0).abs() < 1e-6);
        assert!((unp[3] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn unpack_udec3_axis_isolation() {
        // X = 511 (mid of 1023), Y = Z = 0, W = 0 → roughly 0.0 on X,
        // -1.0 elsewhere. Catches a misordered shift / mask pair.
        let raw: u32 = 511;
        let unp = unpack_udec3_xyzw(raw);
        assert!((unp[0] - ((511.0 / 1023.0) * 2.0 - 1.0)).abs() < 1e-6);
        assert_eq!(unp[1], -1.0);
        assert_eq!(unp[2], -1.0);
        assert_eq!(unp[3], -1.0);
    }

    #[test]
    fn unpack_norm_i16_scales_by_havok_constant() {
        // raw=32767 (max positive i16) with scale=1 should give
        // exactly 1 × HAVOK_SCALE. Negative case divides by 32768.
        let pos = unpack_norm_i16(32767, 1.0, BSGeometryMeshData::HAVOK_SCALE);
        assert!((pos - BSGeometryMeshData::HAVOK_SCALE).abs() < 1e-3);

        let neg = unpack_norm_i16(-32768, 1.0, BSGeometryMeshData::HAVOK_SCALE);
        assert!((neg + BSGeometryMeshData::HAVOK_SCALE).abs() < 1e-3);
    }

    /// Regression for #768 / NIF-D3-13. A hostile `weights_per_vert =
    /// 0xFFFFFFFF` paired with a matching `n_total_weights` makes
    /// `outer_len = 1` (passes the outer `allocate_vec` budget guard
    /// from #764). Pre-fix the inner row used
    /// `Vec::with_capacity(weights_per_vert)`, which on overcommit
    /// systems would request ~16 GB of virtual memory and the
    /// subsequent `read_u16_le` would fail with a generic EOF
    /// message (or, in resource-constrained environments, OOM-panic
    /// the process). Post-fix the inner allocation routes through
    /// `allocate_vec` and short-circuits with a descriptive
    /// "only N bytes remain" budget rejection BEFORE any heap
    /// allocation is attempted.
    ///
    /// The test stream is sized so the outer allocate_vec(1) passes
    /// (one trailing padding byte ≥ 1 element) but the inner
    /// allocate_vec(0xFFFFFFFF) sees remaining = 1 and fires the
    /// budget gate. The assertion on the error message text
    /// distinguishes the two failure modes.
    #[test]
    fn weights_per_vert_hostile_returns_err_not_panic() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&2u32.to_le_bytes());          // version (≤ 2 → keep parsing)
        bytes.extend_from_slice(&0u32.to_le_bytes());          // n_tri_indices
        bytes.extend_from_slice(&1.0f32.to_le_bytes());        // scale (positive)
        bytes.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // weights_per_vert HOSTILE
        bytes.extend_from_slice(&0u32.to_le_bytes());          // n_vertices
        bytes.extend_from_slice(&0u32.to_le_bytes());          // n_uv1
        bytes.extend_from_slice(&0u32.to_le_bytes());          // n_uv2
        bytes.extend_from_slice(&0u32.to_le_bytes());          // n_colors
        bytes.extend_from_slice(&0u32.to_le_bytes());          // n_normals
        bytes.extend_from_slice(&0u32.to_le_bytes());          // n_tangents
        bytes.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // n_total_weights HOSTILE
                                                                // (outer_len = 0xFFFFFFFF / 0xFFFFFFFF = 1)
        bytes.push(0x00);                                       // 1 byte padding so outer
                                                                // allocate_vec(1) passes its
                                                                // remaining ≥ count check.

        let result = BSGeometryMeshData::parse_from_bytes(&bytes);
        assert!(
            result.is_err(),
            "hostile weights_per_vert must error gracefully, not panic"
        );
        // Pre-fix would either panic (OOM) or fail with a generic
        // EOF read error; post-fix prints the budget-rejection
        // string from `allocate_vec`. Asserting on the text catches
        // any future regression that re-introduces the unbounded
        // allocation pattern.
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("only") && msg.contains("bytes remain"),
            "expected allocate_vec budget rejection, got: {msg}"
        );
    }
}
