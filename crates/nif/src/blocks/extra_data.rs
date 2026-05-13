//! NiExtraData — generic extra data blocks.
//!
//! These carry metadata (BSXFlags, names, integers, binary blobs).
//! We parse the most common ones and skip unknown subtypes.

use super::NiObject;
use crate::stream::NifStream;
use crate::types::BlockRef;
use crate::version::NifVersion;
use std::any::Any;
use std::io;
use std::sync::Arc;

/// Generic extra data — covers NiStringExtraData, NiIntegerExtraData, etc.
#[derive(Debug)]
pub struct NiExtraData {
    pub type_name: String,
    pub name: Option<Arc<str>>,
    pub string_value: Option<Arc<str>>,
    pub integer_value: Option<u32>,
    /// Populated for `NiFloatExtraData` — a single f32 payload. FOV
    /// multipliers, scale overrides, wetness levels, etc. See #553.
    pub float_value: Option<f32>,
    pub binary_data: Option<Vec<u8>>,
    /// Populated for `NiStringsExtraData` — array of string table entries
    /// carrying e.g. material override lists.
    pub strings_array: Option<Vec<Option<Arc<str>>>>,
    /// Populated for `NiIntegersExtraData` — array of 32-bit integers.
    pub integers_array: Option<Vec<u32>>,
    /// Populated for `NiFloatsExtraData` — array of f32 values.
    pub floats_array: Option<Vec<f32>>,
    /// Populated for `BSBoneLODExtraData` (Skyrim+) — array of
    /// `(distance, bone_name)` pairs telling the engine when to swap
    /// the skeleton's bone-LOD level. The string is the bone's
    /// `NiFixedString` name; resolves to `None` for empty / null
    /// string-table indices. See nif.xml `BoneLOD` (line 2597) and #614.
    pub bone_lods: Option<Vec<(u32, Option<Arc<str>>)>>,
    /// Populated for `SkinAttach` (Starfield) — list of bone names
    /// the parent BSGeometry's skin instance should attach to. Each
    /// entry is a length-prefixed `NiString` (4-byte length). Per
    /// `nifly::SkinAttach::Sync` (ExtraData.cpp:436). See #708.
    pub skin_attach_bones: Option<Vec<String>>,
    /// Populated for `BoneTranslations` (Starfield) — `(bone_name,
    /// translation)` pairs supplying per-bone offset deltas for the
    /// skeleton at this LOD. Per `nifly::BoneTranslations::Sync`
    /// (ExtraData.cpp:441). See #708.
    pub bone_translations: Option<Vec<(String, [f32; 3])>>,
}

impl NiObject for NiExtraData {
    fn block_type_name(&self) -> &'static str {
        "NiExtraData"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiExtraData {
    pub fn parse(stream: &mut NifStream, type_name: &str) -> io::Result<Self> {
        // Three branches per nif.xml:
        //  - v <= 4.2.2.0: linked-list format with `Next Extra Data`
        //    (until=4.2.2.0) + `Num Bytes` (since=4.0.0.0, until=4.2.2.0)
        //    + subclass body. No `Name` field.
        //  - v in (4.2.2.0, 10.0.1.0): neither `Next Extra Data` nor
        //    `Num Bytes` is serialized; `Name` arrives at 10.0.1.0.
        //    Just read the subclass body. Fixes N1-06 / #330 — pre-fix
        //    `parse_legacy` claimed this entire window and consumed
        //    phantom ref + length bytes on every extra-data block.
        //  - v >= 10.0.1.0: inherits NiObjectNET's Name field
        //    (string-table at 20.1.0.1+, inline length-prefixed earlier).
        // `Next Extra Data` and `Num Bytes` are gated `until="4.2.2.0"`
        // (inclusive per the version.rs doctrine — present at v4.2.2.0).
        // The legacy path therefore claims v <= 4.2.2.0; the gap path
        // covers v in (4.2.2.0, 10.0.1.0).
        if stream.version() <= NifVersion(0x04020200) {
            return Self::parse_legacy(stream, type_name);
        }
        if stream.version() < NifVersion(0x0A000100) {
            return Self::parse_gap(stream, type_name);
        }

        let name = stream.read_string()?;

        let mut string_value = None;
        let mut integer_value = None;
        let mut float_value = None;
        let mut binary_data = None;
        let mut strings_array = None;
        let mut integers_array = None;
        let mut floats_array = None;
        let mut bone_lods = None;
        let mut skin_attach_bones = None;
        let mut bone_translations = None;

        match type_name {
            "NiStringExtraData" => {
                string_value = stream.read_string()?;
            }
            "NiIntegerExtraData" | "BSXFlags" => {
                integer_value = Some(stream.read_u32_le()?);
            }
            "NiBooleanExtraData" => {
                // nif.xml: Boolean Data is type "byte" (1 byte), NOT u32.
                integer_value = Some(stream.read_u8()? as u32);
            }
            // nif.xml line 4264 — single `Float Data: float` field.
            // #553: pre-fix this subclass was absent from the dispatch
            // (no match arm) so 1,492 SE + 156 FO3/FNV blocks fell into
            // NiUnknown, silently discarding every tool-authored FOV
            // multiplier / scale override / wetness level metadata tag.
            "NiFloatExtraData" => {
                float_value = Some(stream.read_f32_le()?);
            }
            "NiBinaryExtraData" => {
                let size = stream.read_u32_le()? as usize;
                binary_data = Some(stream.read_bytes(size)?);
            }
            // Array variants — count (u32) followed by N items. See #164.
            //
            // Per nif.xml line 5177 the entries are `SizedString`
            // (always u32-length-prefixed inline), NOT the version-aware
            // `string` type. Pre-#615 this used `read_string`, which on
            // Skyrim+ (v >= 20.1.0.1) reads a 4-byte string-table index
            // instead of an inline string. Result: every Skyrim
            // NiStringsExtraData with a non-empty array under-consumed
            // its payload — strings array body was misread (or skipped),
            // dropping SpeedTree LOD bone names, anim-event trigger
            // lists, and material-override slots. block_size recovery
            // hid the drift from the parse-rate gate. The Oblivion-era
            // path happened to work because pre-20.1.0.1 `read_string`
            // also reads length-prefixed inline (the formats coincide).
            "NiStringsExtraData" => {
                let count = stream.read_u32_le()?;
                let mut arr = stream.allocate_vec(count)?;
                for _ in 0..count {
                    let s = stream.read_sized_string()?;
                    if s.is_empty() {
                        arr.push(None);
                    } else {
                        arr.push(Some(Arc::from(s)));
                    }
                }
                strings_array = Some(arr);
            }
            "NiIntegersExtraData" => {
                // #981 — bulk-read via `read_u32_array`.
                let count = stream.read_u32_le()? as usize;
                integers_array = Some(stream.read_u32_array(count)?);
            }
            // nif.xml line 4269 — parallel to NiIntegersExtraData but
            // with f32 payload. Bundled with #553 because the authoring
            // tools emit both Float and Floats variants in the same DLC
            // content stream.
            "NiFloatsExtraData" => {
                // #981 — bulk-read via `read_f32_array`.
                let count = stream.read_u32_le()? as usize;
                floats_array = Some(stream.read_f32_array(count)?);
            }
            // BSBoneLODExtraData (Skyrim+) — bone-LOD distance thresholds
            // for skeleton mesh swapping. nif.xml lines 8183-8187:
            //   uint BoneLOD Count
            //   BoneLOD[BoneLOD Count] BoneLOD Info
            //   struct BoneLOD { uint Distance; NiFixedString Bone Name; }
            // Carried by every Skyrim SE skeleton.nif; pre-#614 the
            // dispatch was absent so 52 files in Meshes0.bsa fell into
            // NiUnknown and the parse-rate gate dropped from 100%.
            "BSBoneLODExtraData" => {
                let count = stream.read_u32_le()?;
                let mut arr = stream.allocate_vec(count)?;
                for _ in 0..count {
                    let distance = stream.read_u32_le()?;
                    let bone_name = stream.read_string()?;
                    arr.push((distance, bone_name));
                }
                bone_lods = Some(arr);
            }
            // SkinAttach (Starfield, #708 / NIF-D5-02). Pairs with
            // BSGeometry's skin-instance ref to tell the engine which
            // skeleton bones the mesh attaches to. Per
            // `nifly::SkinAttach::Sync`: a single NiStringVector field
            // = u32 count + count × NiString(u32 length + bytes).
            "SkinAttach" => {
                let count = stream.read_u32_le()?;
                let mut arr = stream.allocate_vec::<String>(count)?;
                for _ in 0..count {
                    arr.push(stream.read_sized_string()?);
                }
                skin_attach_bones = Some(arr);
            }
            // BoneTranslations (Starfield, #708 / NIF-D5-08). Per-LOD
            // bone-offset deltas. Per `nifly::BoneTranslations::Sync`:
            // u32 count + count × { NiString(u32 length) + Vector3 }.
            "BoneTranslations" => {
                let count = stream.read_u32_le()?;
                let mut arr = stream.allocate_vec::<(String, [f32; 3])>(count)?;
                for _ in 0..count {
                    let bone = stream.read_sized_string()?;
                    let tx = stream.read_f32_le()?;
                    let ty = stream.read_f32_le()?;
                    let tz = stream.read_f32_le()?;
                    arr.push((bone, [tx, ty, tz]));
                }
                bone_translations = Some(arr);
            }
            _ => {
                // Unknown extra data subtype — can't skip without size
            }
        }

        Ok(Self {
            type_name: type_name.to_string(),
            name,
            string_value,
            integer_value,
            float_value,
            binary_data,
            strings_array,
            integers_array,
            floats_array,
            bone_lods,
            skin_attach_bones,
            bone_translations,
        })
    }

    /// Parse the (4.2.2.0, 10.0.1.0) gap-window variant: no linked-list
    /// ref, no bytes-remaining, no Name. Only the subclass body is on
    /// disk. See N1-06 / #330. Mirrors [`Self::parse_legacy`] modulo the
    /// two header fields the legacy branch pre-reads.
    fn parse_gap(stream: &mut NifStream, type_name: &str) -> io::Result<Self> {
        let mut string_value = None;
        let mut integer_value = None;

        match type_name {
            "NiStringExtraData" => {
                // Pre-10.0.1.0 variant drops the `bytes_remaining`
                // prefix, but the subclass still serializes its payload
                // as a sized string (inline u32 length + bytes).
                let s = stream.read_sized_string()?;
                string_value = Some(Arc::from(s.as_str()));
            }
            "NiIntegerExtraData" => {
                integer_value = Some(stream.read_u32_le()?);
            }
            _ => {
                // Unknown subtype in the gap window — we have no way to
                // advance past an arbitrary body because `Num Bytes`
                // only exists until 4.2.2.0. Leave the stream untouched
                // and let the outer parse loop reconcile via block_size
                // (or fall through to NiUnknown on pre-block_size
                // content). Same policy the modern branch applies.
            }
        }

        Ok(Self {
            type_name: type_name.to_string(),
            name: None,
            string_value,
            integer_value,
            float_value: None,
            binary_data: None,
            strings_array: None,
            integers_array: None,
            floats_array: None,
            bone_lods: None,
            skin_attach_bones: None,
            bone_translations: None,
        })
    }

    /// Parse pre-Gamebryo NiExtraData (v <= 4.2.2.0, Morrowind / early
    /// NetImmerse). Old format: next_extra_data_ref + bytes_remaining +
    /// subclass data. No NiObjectNET inheritance (no name field).
    fn parse_legacy(stream: &mut NifStream, type_name: &str) -> io::Result<Self> {
        let _next_extra_data_ref = stream.read_block_ref()?;
        let bytes_remaining = stream.read_u32_le()?;

        let mut string_value = None;
        let mut integer_value = None;

        match type_name {
            "NiStringExtraData" => {
                // Old NiStringExtraData: bytes_remaining includes the u32 length prefix.
                let s = stream.read_sized_string()?;
                string_value = Some(Arc::from(s.as_str()));
            }
            "NiIntegerExtraData" => {
                integer_value = Some(stream.read_u32_le()?);
            }
            _ => {
                // Unknown old extra data — skip bytes_remaining to stay aligned.
                if bytes_remaining > 0 {
                    stream.skip(bytes_remaining as u64)?;
                }
            }
        }

        Ok(Self {
            type_name: type_name.to_string(),
            name: None,
            string_value,
            integer_value,
            float_value: None,
            binary_data: None,
            strings_array: None,
            integers_array: None,
            floats_array: None,
            bone_lods: None,
            skin_attach_bones: None,
            bone_translations: None,
        })
    }
}

// ── BSBound ────────────────────────────────────────────────────────

/// BSBound — bounding box extra data (center + half-extents).
///
/// Attached to root nodes for object-level bounding volume queries.
#[derive(Debug)]
pub struct BsBound {
    pub name: Option<Arc<str>>,
    pub center: [f32; 3],
    pub dimensions: [f32; 3],
}

impl NiObject for BsBound {
    fn block_type_name(&self) -> &'static str {
        "BSBound"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsBound {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiExtraData base: name — gated since 10.0.1.0 per nif.xml. See #329.
        let name = stream.read_extra_data_name()?;
        let center = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let dimensions = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        Ok(Self {
            name,
            center,
            dimensions,
        })
    }
}

// ── BSPositionData ──────────────────────────────────────────────────

/// `BSPositionData` (FO4 / FO76, nif.xml line 8342) — per-vertex blend
/// factor array carried as extra data on actor / cloth / dismemberment
/// meshes. The single `Vertex Data: Half Float[Num Vertices]` array
/// supplies the interpolation weight for the procedural vertex morph
/// (cape sway, dismemberment severance, FO76 cloth). Pre-#710 the
/// block was undispatched (2,961 instances across vanilla
/// `Fallout4 - Meshes.ba2` + `SeventySix - Meshes.ba2` fell into
/// `NiUnknown`), so all those meshes lost their per-vertex blend
/// data and reverted to default rigid behaviour.
///
/// Half-float storage matches the FO4 / FO76 vertex-stream
/// convention; decoded to `f32` via `tri_shape::half_to_f32` for
/// downstream consumers.
#[derive(Debug)]
pub struct BsPositionData {
    pub name: Option<Arc<str>>,
    /// Per-vertex blend factor in the range [0, 1] (typical) — driven
    /// by Havok cloth / dismemberment systems on FO4 / FO76.
    pub vertex_data: Vec<f32>,
}

impl NiObject for BsPositionData {
    fn block_type_name(&self) -> &'static str {
        "BSPositionData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsPositionData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiExtraData base: name — gated since 10.0.1.0 per nif.xml.
        // Every BSPositionData target version (FO4 = 20.2.0.7,
        // FO76 = 20.2.0.7) sits well past the boundary, so name is
        // always present in shipped content. See #329.
        let name = stream.read_extra_data_name()?;
        // Num Vertices: u32 — file-driven count, route through
        // `allocate_vec` so a corrupt 0xFFFFFFFF can't OOM-allocate
        // a 12 GB Vec before the inner half-float reads fail. See
        // #764 (the `allocate_vec` budget guard) and the issue's
        // explicit ALLOCATE_VEC completeness check.
        let num_vertices = stream.read_u32_le()?;
        let mut vertex_data = stream.allocate_vec::<f32>(num_vertices)?;
        for _ in 0..num_vertices {
            // Half Float (16-bit IEEE-754) — same encoding as the
            // FO4 / FO76 vertex-stream UV / position halfs decoded
            // by `tri_shape::half_to_f32`.
            let h = stream.read_u16_le()?;
            vertex_data.push(crate::blocks::tri_shape::half_to_f32(h));
        }
        Ok(Self { name, vertex_data })
    }
}

// ── BSEyeCenterExtraData ──────────────────────────────────────────

/// `BSEyeCenterExtraData` (FO4 / FO76, nif.xml line 8369) — eye-pivot
/// positions used by FaceGen and the dialogue camera framing system
/// to compute eye-tracking targets. Pre-#720 the block was undispatched
/// (625 instances across vanilla `Fallout4 - Meshes.ba2` (623) +
/// `SeventySix - Meshes.ba2` (2) fell into `NiUnknown`), so dialogue /
/// cinematic eye-tracking pointed at the NIF origin instead of the
/// actual eye centroid. Visible as cross-eyed NPCs in close-ups.
///
/// Layout per nif.xml `<niobject name="BSEyeCenterExtraData"
/// inherit="NiExtraData" module="BSMain" versions="#FO4# #F76#">`:
///
/// ```text
/// uint num_floats
/// f32[num_floats] floats   // typically 4: left+right eye XY in mesh space
/// ```
///
/// `num_floats` is captured raw on the struct so consumers can branch
/// on the (rare) non-4 case without re-reading the array length. Most
/// shipped content lands at exactly 4 (one (X, Y) pair per eye).
#[derive(Debug)]
pub struct BsEyeCenterExtraData {
    pub name: Option<Arc<str>>,
    /// Eye-pivot positions in mesh space. Typically 4 floats — left-eye
    /// XY then right-eye XY — but the layout is "Float[Num]" per
    /// nif.xml so consumers must check `floats.len()` before indexing.
    pub floats: Vec<f32>,
}

impl NiObject for BsEyeCenterExtraData {
    fn block_type_name(&self) -> &'static str {
        "BSEyeCenterExtraData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsEyeCenterExtraData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiExtraData base: name — gated since 10.0.1.0 per nif.xml.
        // FO4 / FO76 sit at 20.2.0.7, well past the boundary.
        let name = stream.read_extra_data_name()?;
        // Num Floats: u32 — bulk-read via `read_f32_array`, which
        // routes through `read_pod_vec` and keeps the byte-budget
        // guard the #408 sweep introduced. See #981.
        let num_floats = stream.read_u32_le()? as usize;
        let floats = stream.read_f32_array(num_floats)?;
        Ok(Self { name, floats })
    }
}

// ── BSDecalPlacementVectorExtraData ────────────────────────────────

/// A block of decal placement vectors (points + normals).
#[derive(Debug)]
pub struct DecalVectorBlock {
    pub points: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
}

/// BSDecalPlacementVectorExtraData — decal projection data for placed decals.
///
/// Inherits NiFloatExtraData (NiExtraData + f32). Contains arrays of
/// point/normal pairs defining where decals are projected onto geometry.
#[derive(Debug)]
pub struct BsDecalPlacementVectorExtraData {
    pub name: Option<Arc<str>>,
    pub float_value: f32,
    pub vector_blocks: Vec<DecalVectorBlock>,
}

impl NiObject for BsDecalPlacementVectorExtraData {
    fn block_type_name(&self) -> &'static str {
        "BSDecalPlacementVectorExtraData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsDecalPlacementVectorExtraData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiExtraData base: name — gated since 10.0.1.0 per nif.xml. See #329.
        let name = stream.read_extra_data_name()?;
        // NiFloatExtraData: float value
        let float_value = stream.read_f32_le()?;
        // BSDecalPlacementVectorExtraData: vector blocks.
        // #981 — inner `points` / `normals` arrays are POD `[f32; 3]`
        // sequences; bulk-read via `read_f32_triple_array` (one
        // allocation + one read_exact per array instead of one
        // allocation + N per-component reads). The outer block list
        // stays on a typed push loop because each iteration parses
        // a variable-width payload, not a fixed-stride POD record.
        let num_blocks = stream.read_u16_le()? as u32;
        let mut vector_blocks: Vec<DecalVectorBlock> = stream.allocate_vec(num_blocks)?;
        for _ in 0..num_blocks {
            let num_vectors = stream.read_u16_le()? as usize;
            let points = stream.read_f32_triple_array(num_vectors)?;
            let normals = stream.read_f32_triple_array(num_vectors)?;
            vector_blocks.push(DecalVectorBlock { points, normals });
        }
        Ok(Self {
            name,
            float_value,
            vector_blocks,
        })
    }
}

// ── BSBehaviorGraphExtraData ───────────────────────────────────────

/// Behavior graph reference for Havok animation behavior files.
/// Present on characters and animated objects (Skyrim+).
#[derive(Debug)]
pub struct BsBehaviorGraphExtraData {
    pub name: Option<Arc<str>>,
    pub behaviour_graph_file: Option<Arc<str>>,
    pub controls_base_skeleton: bool,
}

impl NiObject for BsBehaviorGraphExtraData {
    fn block_type_name(&self) -> &'static str {
        "BSBehaviorGraphExtraData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsBehaviorGraphExtraData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiExtraData base — gated since 10.0.1.0 per nif.xml. See #329.
        let name = stream.read_extra_data_name()?;
        let behaviour_graph_file = stream.read_string()?;
        // nif.xml line 8192: `Controls Base Skeleton: bool`. Pre-#106 we
        // read 4 bytes (u32-as-bool), desyncing every Skyrim skeleton
        // NIF with a behavior-graph reference by 3 bytes. The version-
        // aware `read_bool` helper does the right thing for both pre-
        // and post-4.1.0.1 (= 1 byte everywhere Skyrim+ cares about).
        let controls_base_skeleton = stream.read_bool()?;
        Ok(Self {
            name,
            behaviour_graph_file,
            controls_base_skeleton,
        })
    }
}

// ── BSInvMarker ────────────────────────────────────────────────────

/// Inventory display marker — rotation and zoom for in-menu 3D preview.
/// Rotation values are radians × 1000 stored as u16.
#[derive(Debug)]
pub struct BsInvMarker {
    pub name: Option<Arc<str>>,
    pub rotation_x: u16,
    pub rotation_y: u16,
    pub rotation_z: u16,
    pub zoom: f32,
}

impl NiObject for BsInvMarker {
    fn block_type_name(&self) -> &'static str {
        "BSInvMarker"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsInvMarker {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiExtraData base — gated since 10.0.1.0 per nif.xml. See #329.
        let name = stream.read_extra_data_name()?;
        let rotation_x = stream.read_u16_le()?;
        let rotation_y = stream.read_u16_le()?;
        let rotation_z = stream.read_u16_le()?;
        let zoom = stream.read_f32_le()?;
        Ok(Self {
            name,
            rotation_x,
            rotation_y,
            rotation_z,
            zoom,
        })
    }
}

// ── BSWArray ───────────────────────────────────────────────────────

/// Wide signed-integer array extra data (FO3+).
///
/// nif.xml: `BSWArray inherit="NiExtraData" versions="#FO3_AND_LATER#"`.
/// Fields: Num Items (u32) + Items ([i32; Num Items]).
#[derive(Debug)]
pub struct BsWArray {
    pub name: Option<Arc<str>>,
    pub items: Vec<i32>,
}

impl NiObject for BsWArray {
    fn block_type_name(&self) -> &'static str {
        "BSWArray"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsWArray {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let name = stream.read_string()?;
        // #981 — bulk-read i32 array.
        let count = stream.read_u32_le()? as usize;
        let items = stream.read_i32_array(count)?;
        Ok(Self { name, items })
    }
}

// ── BSClothExtraData ───────────────────────────────────────────────

/// Havok cloth simulation data (opaque binary blob). FO4+.
///
/// Inherits `BSExtraData`, NOT `NiExtraData` directly — this matters
/// because nif.xml line 3222 marks the `Name` field as
/// `excludeT="BSExtraData"`, so the BS-side hierarchy explicitly
/// drops it. `name` therefore stays `None` for every cloth block;
/// kept on the struct as a placeholder for shape symmetry with the
/// other `Bs*ExtraData` parsers.
#[derive(Debug)]
pub struct BsClothExtraData {
    pub name: Option<Arc<str>>,
    pub data: Vec<u8>,
}

impl NiObject for BsClothExtraData {
    fn block_type_name(&self) -> &'static str {
        "BSClothExtraData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsClothExtraData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // BSExtraData omits the NiExtraData `Name` field — nif.xml
        // line 3222 carries `excludeT="BSExtraData"` on it. Pre-#722
        // the parser called `read_extra_data_name` here, consuming 4
        // bytes (string-table index) of the cloth payload as a name
        // reference and then reading the next 4 bytes as the length.
        // 1,523 / 1,523 cloth-bearing FO4 / FO76 / Starfield NIFs
        // failed through `block_size` recovery as a result — capes,
        // flags, curtains, hair fell back to rigid geometry.
        let length = stream.read_u32_le()? as usize;
        let data = stream.read_bytes(length)?;
        Ok(Self { name: None, data })
    }
}

// ── BSCollisionQueryProxyExtraData ─────────────────────────────────

/// FO76 collision-query-proxy metadata (nif.xml line 8498). Inherits
/// `BSExtraData` (no `Name` field per the `excludeT` gate, same as
/// `BSClothExtraData`) and adds a single `ByteArray` payload. The
/// binary blob is opaque to us today; capturing it correctly prevents
/// the previous `NiUnknown` silent drop on the 2 occurrences observed
/// in `SeventySix - Meshes.ba2`. See #728 / NIF-D5-10.
#[derive(Debug)]
pub struct BsCollisionQueryProxyExtraData {
    pub data: Vec<u8>,
}

impl NiObject for BsCollisionQueryProxyExtraData {
    fn block_type_name(&self) -> &'static str {
        "BSCollisionQueryProxyExtraData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsCollisionQueryProxyExtraData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // Same wire layout as `BsClothExtraData` — both inherit
        // `BSExtraData` (no Name) and carry a single ByteArray
        // (length-prefixed). Pre-#728 these blocks landed on the
        // NiUnknown recovery path. Block-size recovery hid the drop;
        // the per-block telemetry surface added in #939 will catch a
        // future regression here.
        let length = stream.read_u32_le()? as usize;
        let data = stream.read_bytes(length)?;
        Ok(Self { data })
    }
}

// ── BSDistantObjectLargeRefExtraData ────────────────────────────────

/// Skyrim SE "is this a large reference?" flag, attached to worldspace
/// objects that participate in precombined-LOD scheduling. Inherits
/// `NiExtraData`; nif.xml: single `bool Large Ref` field.
///
/// Pre-#942 the block landed on the `NiUnknown` recovery path, so the
/// flag was lost and every large-ref worldspace object missed the
/// precombined-LOD pool — the renderer re-uploaded the geometry per cell
/// instead of binding the pre-merged batch. See issue #942 / NIF-D5-NEW-03.
#[derive(Debug)]
pub struct BsDistantObjectLargeRefExtraData {
    pub name: Option<Arc<str>>,
    /// `true` when the host object should be scheduled through the
    /// precombined-LOD pool; renderer / cell loader consumers gate on
    /// this. Vanilla SSE flips it for ~every large static (rocks,
    /// towers, large trees) in exterior cells.
    pub large_ref: bool,
}

impl NiObject for BsDistantObjectLargeRefExtraData {
    fn block_type_name(&self) -> &'static str {
        "BSDistantObjectLargeRefExtraData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsDistantObjectLargeRefExtraData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiExtraData base: name — gated since 10.0.1.0 per nif.xml.
        // SSE is well past that gate, so the name field is always present.
        let name = stream.read_extra_data_name()?;
        // SSE bsver >= 83, so version >= 20.2.0.7 — `read_bool` reads a
        // single byte. Kept on the version-aware path for symmetry with
        // other NiExtraData subclasses.
        let large_ref = stream.read_bool()?;
        Ok(Self { name, large_ref })
    }
}

// ── BSConnectPoint::Parents ────────────────────────────────────────

/// Workshop connection point definition. FO4+.
#[derive(Debug)]
pub struct ConnectPointData {
    pub parent: String,
    pub name: String,
    pub rotation: [f32; 4],
    pub translation: [f32; 3],
    pub scale: f32,
}

#[derive(Debug)]
pub struct BsConnectPointParents {
    pub name: Option<Arc<str>>,
    pub connect_points: Vec<ConnectPointData>,
}

impl NiObject for BsConnectPointParents {
    fn block_type_name(&self) -> &'static str {
        "BSConnectPoint::Parents"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsConnectPointParents {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiExtraData base — gated since 10.0.1.0 per nif.xml. See #329.
        let name = stream.read_extra_data_name()?;
        let count = stream.read_u32_le()?;
        let mut connect_points = stream.allocate_vec(count)?;
        for _ in 0..count {
            let parent = stream.read_sized_string()?;
            let cp_name = stream.read_sized_string()?;
            let rotation = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            let translation = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            let scale = stream.read_f32_le()?;
            connect_points.push(ConnectPointData {
                parent,
                name: cp_name,
                rotation,
                translation,
                scale,
            });
        }
        Ok(Self {
            name,
            connect_points,
        })
    }
}

// ── BSPackedCombined[Shared]GeomDataExtra ──────────────────────────

/// One placed instance inside a packed-combined batch — grayscale
/// tint, engine-space transform, and a bounding sphere. 72 bytes.
/// `size="72"` per nif.xml `BSPackedGeomDataCombined`.
#[derive(Debug, Clone)]
pub struct BsPackedGeomDataCombined {
    /// Per-instance tint / palette index (f32). Drives the
    /// grayscale_to_palette shader input for merged LOD dressing.
    pub grayscale_to_palette_scale: f32,
    pub transform: crate::types::NiTransform,
    /// Bounding sphere: `[cx, cy, cz, radius]`.
    pub bounding_sphere: [f32; 4],
}

impl BsPackedGeomDataCombined {
    fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let grayscale_to_palette_scale = stream.read_f32_le()?;
        // nif.xml line 8377: `<field name="Transform" type="NiTransform" />`.
        // The `NiTransform` STRUCT (nif.xml line 1808) ships
        // Rotation → Translation → Scale on disk, opposite to NiAVObject's
        // inline (Translation → Rotation → Scale) layout. Use the STRUCT
        // reader, NOT `read_ni_transform()`. Sibling of the M41.0 Phase
        // 1b.x `8ec6a69` NiSkinData fix — same Rust type, same byte
        // layout mismatch, different parser. See #767 + the
        // `read_ni_transform_struct` doc-comment.
        let transform = stream.read_ni_transform_struct()?;
        let cx = stream.read_f32_le()?;
        let cy = stream.read_f32_le()?;
        let cz = stream.read_f32_le()?;
        let radius = stream.read_f32_le()?;
        Ok(Self {
            grayscale_to_palette_scale,
            transform,
            bounding_sphere: [cx, cy, cz, radius],
        })
    }
}

/// One baked-geometry object inside a `BSPackedCombinedGeomDataExtra`.
/// Carries metadata (LOD counts/offsets, per-instance transforms) plus
/// the raw vertex and triangle bytes — they are retained as
/// `Vec<u8>` / `Vec<[u16; 3]>` so a downstream LOD importer can decode
/// them once the terrain-streaming milestone needs them. See #158 and
/// #365.
#[derive(Debug, Clone)]
pub struct BsPackedGeomData {
    pub num_verts: u32,
    pub lod_levels: u32,
    pub tri_count_lod0: u32,
    pub tri_offset_lod0: u32,
    pub tri_count_lod1: u32,
    pub tri_offset_lod1: u32,
    pub tri_count_lod2: u32,
    pub tri_offset_lod2: u32,
    pub combined: Vec<BsPackedGeomDataCombined>,
    pub vertex_desc: u64,
    /// Raw vertex bytes — `num_verts * vertex_stride(vertex_desc)`.
    /// Stored verbatim; the downstream importer decodes per-vertex via
    /// the same `vertex_desc` machinery `BsTriShape` already uses.
    pub vertex_data: Vec<u8>,
    /// Triangle indices for all LODs concatenated, in order
    /// LOD0 → LOD1 → LOD2. Each triangle is 3 u16s.
    pub triangles: Vec<[u16; 3]>,
}

/// One shared-geometry reference inside a
/// `BSPackedCombinedSharedGeomDataExtra`. The actual vertex/triangle
/// data lives in an external PSG/CSG file addressed by filename hash
/// + byte offset. 8 bytes.
#[derive(Debug, Clone, Copy)]
pub struct BsPackedGeomObject {
    /// BSCRC32 of the `.psg`/`.csg` filename (without extension).
    pub filename_hash: u32,
    /// Byte offset into the PSG/CSG blob where this object's geometry
    /// starts.
    pub data_offset: u32,
}

/// Shared-geometry metadata — identical header layout to
/// `BsPackedGeomData` but with the vertex and triangle arrays elided
/// (they live in the external PSG/CSG file).
#[derive(Debug, Clone)]
pub struct BsPackedSharedGeomData {
    pub num_verts: u32,
    pub lod_levels: u32,
    pub tri_count_lod0: u32,
    pub tri_offset_lod0: u32,
    pub tri_count_lod1: u32,
    pub tri_offset_lod1: u32,
    pub tri_count_lod2: u32,
    pub tri_offset_lod2: u32,
    pub combined: Vec<BsPackedGeomDataCombined>,
    pub vertex_desc: u64,
}

/// Two-variant payload: baked geometry is self-contained; shared
/// geometry defers to an external PSG/CSG file.
#[derive(Debug, Clone)]
pub enum BsPackedCombinedPayload {
    /// `BSPackedCombinedGeomDataExtra` — vertex/triangle data is
    /// baked into this NIF.
    Baked(Vec<BsPackedGeomData>),
    /// `BSPackedCombinedSharedGeomDataExtra` — vertex/triangle data
    /// is in a companion `.psg`/`.csg` file, addressed by hash +
    /// offset in each `BsPackedGeomObject`.
    Shared {
        objects: Vec<BsPackedGeomObject>,
        data: Vec<BsPackedSharedGeomData>,
    },
}

/// `BSPackedCombinedGeomDataExtra` and
/// `BSPackedCombinedSharedGeomDataExtra` — FO4+ distant-LOD merged
/// geometry batches attached to `BSMultiBoundNode` roots in cell LOD
/// NIFs. The two variants differ in whether vertex/triangle data is
/// baked into the NIF (`Baked`) or deferred to a PSG/CSG companion
/// file (`Shared`).
///
/// The full wire format is now parsed (issue #365 / regression of
/// #158). Downstream LOD rendering — reconstructing merged
/// BSTriShape-equivalent batches from the baked-geometry arrays — is
/// still future work (tied to the terrain-streaming milestone), but
/// the structural data is no longer silently skipped.
#[derive(Debug)]
pub struct BsPackedCombinedGeomDataExtra {
    /// Discriminator: `"BSPackedCombinedGeomDataExtra"` or
    /// `"BSPackedCombinedSharedGeomDataExtra"`.
    pub type_name: &'static str,
    pub name: Option<Arc<str>>,
    pub vertex_desc: u64,
    pub num_vertices: u32,
    pub num_triangles: u32,
    pub unknown_flags_1: u32,
    pub unknown_flags_2: u32,
    pub num_data: u32,
    pub payload: BsPackedCombinedPayload,
}

impl NiObject for BsPackedCombinedGeomDataExtra {
    fn block_type_name(&self) -> &'static str {
        self.type_name
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Extract the per-vertex stride in bytes from a BSVertexDesc bitfield.
///
/// The low nibble stores "size-in-quads" — multiply by 4 to get bytes.
/// Matches the formula used by `BsTriShape` in tri_shape.rs.
#[inline]
fn vertex_stride_from_desc(vertex_desc: u64) -> usize {
    ((vertex_desc & 0xF) as usize) * 4
}

impl BsPackedCombinedGeomDataExtra {
    /// Parse the full wire format. See the struct doc comment for
    /// variant differences.
    pub fn parse(stream: &mut NifStream, type_name: &'static str) -> io::Result<Self> {
        // NiExtraData base — gated since 10.0.1.0 per nif.xml. See #329.
        let name = stream.read_extra_data_name()?;
        let vertex_desc = stream.read_u64_le()?;
        let num_vertices = stream.read_u32_le()?;
        let num_triangles = stream.read_u32_le()?;
        let unknown_flags_1 = stream.read_u32_le()?;
        let unknown_flags_2 = stream.read_u32_le()?;
        let num_data = stream.read_u32_le()?;

        let payload = if type_name == "BSPackedCombinedSharedGeomDataExtra" {
            // Shared variant: N × GeomObject (8 bytes each) then N ×
            // SharedGeomData (header-only, no vertex / triangle arrays).
            // #388: allocate_vec bounds the count against the stream
            // budget so a corrupt num_data can't OOM.
            let mut objects: Vec<BsPackedGeomObject> = stream.allocate_vec(num_data)?;
            for _ in 0..num_data {
                let filename_hash = stream.read_u32_le()?;
                let data_offset = stream.read_u32_le()?;
                objects.push(BsPackedGeomObject {
                    filename_hash,
                    data_offset,
                });
            }
            let mut data: Vec<BsPackedSharedGeomData> = stream.allocate_vec(num_data)?;
            for _ in 0..num_data {
                data.push(parse_shared_geom_data(stream)?);
            }
            BsPackedCombinedPayload::Shared { objects, data }
        } else {
            // Baked variant: N × BSPackedGeomData.
            let mut baked: Vec<BsPackedGeomData> = stream.allocate_vec(num_data)?;
            for _ in 0..num_data {
                baked.push(parse_baked_geom_data(stream)?);
            }
            BsPackedCombinedPayload::Baked(baked)
        };

        Ok(Self {
            type_name,
            name,
            vertex_desc,
            num_vertices,
            num_triangles,
            unknown_flags_1,
            unknown_flags_2,
            num_data,
            payload,
        })
    }
}

fn parse_common_geom_header(
    stream: &mut NifStream,
) -> io::Result<(
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
    Vec<BsPackedGeomDataCombined>,
    u64,
)> {
    let num_verts = stream.read_u32_le()?;
    let lod_levels = stream.read_u32_le()?;
    let tri_count_lod0 = stream.read_u32_le()?;
    let tri_offset_lod0 = stream.read_u32_le()?;
    let tri_count_lod1 = stream.read_u32_le()?;
    let tri_offset_lod1 = stream.read_u32_le()?;
    let tri_count_lod2 = stream.read_u32_le()?;
    let tri_offset_lod2 = stream.read_u32_le()?;
    let num_combined = stream.read_u32_le()?;
    let mut combined: Vec<BsPackedGeomDataCombined> = stream.allocate_vec(num_combined)?;
    for _ in 0..num_combined {
        combined.push(BsPackedGeomDataCombined::parse(stream)?);
    }
    let vertex_desc = stream.read_u64_le()?;
    Ok((
        num_verts,
        lod_levels,
        tri_count_lod0,
        tri_offset_lod0,
        tri_count_lod1,
        tri_offset_lod1,
        tri_count_lod2,
        tri_offset_lod2,
        combined,
        vertex_desc,
    ))
}

fn parse_baked_geom_data(stream: &mut NifStream) -> io::Result<BsPackedGeomData> {
    let (
        num_verts,
        lod_levels,
        tri_count_lod0,
        tri_offset_lod0,
        tri_count_lod1,
        tri_offset_lod1,
        tri_count_lod2,
        tri_offset_lod2,
        combined,
        vertex_desc,
    ) = parse_common_geom_header(stream)?;

    let stride = vertex_stride_from_desc(vertex_desc);
    let vertex_bytes = (num_verts as usize).saturating_mul(stride);
    let vertex_data = stream.read_bytes(vertex_bytes)?;

    let total_triangles = tri_count_lod0
        .saturating_add(tri_count_lod1)
        .saturating_add(tri_count_lod2);
    let mut triangles: Vec<[u16; 3]> = stream.allocate_vec(total_triangles)?;
    for _ in 0..total_triangles {
        let a = stream.read_u16_le()?;
        let b = stream.read_u16_le()?;
        let c = stream.read_u16_le()?;
        triangles.push([a, b, c]);
    }

    Ok(BsPackedGeomData {
        num_verts,
        lod_levels,
        tri_count_lod0,
        tri_offset_lod0,
        tri_count_lod1,
        tri_offset_lod1,
        tri_count_lod2,
        tri_offset_lod2,
        combined,
        vertex_desc,
        vertex_data,
        triangles,
    })
}

fn parse_shared_geom_data(stream: &mut NifStream) -> io::Result<BsPackedSharedGeomData> {
    let (
        num_verts,
        lod_levels,
        tri_count_lod0,
        tri_offset_lod0,
        tri_count_lod1,
        tri_offset_lod1,
        tri_count_lod2,
        tri_offset_lod2,
        combined,
        vertex_desc,
    ) = parse_common_geom_header(stream)?;

    Ok(BsPackedSharedGeomData {
        num_verts,
        lod_levels,
        tri_count_lod0,
        tri_offset_lod0,
        tri_count_lod1,
        tri_offset_lod1,
        tri_count_lod2,
        tri_offset_lod2,
        combined,
        vertex_desc,
    })
}

// ── BSFurnitureMarker ──────────────────────────────────────────────

/// A single furniture marker position — where an actor sits, sleeps, or leans.
///
/// Wire layout is version-split: BSVER ≤ 34 (up to and including FO3/FNV)
/// uses orientation + 2 position refs; BSVER > 34 (Skyrim+) replaces them
/// with heading + animation type + entry properties. Per nif.xml FurniturePosition.
#[derive(Debug, Clone)]
pub struct FurniturePosition {
    pub offset: [f32; 3],
    /// Oblivion/FO3/FNV: orientation + ref1 + ref2. Skyrim+: heading + anim + entry.
    pub data: FurniturePositionData,
}

#[derive(Debug, Clone)]
pub enum FurniturePositionData {
    /// BSVER ≤ 34 (Oblivion, FO3, FNV).
    Legacy {
        orientation: u16,
        position_ref_1: u8,
        position_ref_2: u8,
    },
    /// BSVER > 34 (Skyrim, Skyrim SE, FO4).
    Modern {
        heading: f32,
        animation_type: u16,
        entry_properties: u16,
    },
}

/// BSFurnitureMarker — sitting/sleeping/leaning position list on furniture meshes.
/// Introduced in Oblivion (v20.0.0.5, BSVER=11).
#[derive(Debug)]
pub struct BsFurnitureMarker {
    pub type_name: &'static str,
    pub name: Option<Arc<str>>,
    pub positions: Vec<FurniturePosition>,
}

impl NiObject for BsFurnitureMarker {
    fn block_type_name(&self) -> &'static str {
        self.type_name
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsFurnitureMarker {
    pub fn parse(stream: &mut NifStream, type_name: &'static str) -> io::Result<Self> {
        // NiExtraData base — gated since 10.0.1.0 per nif.xml. See #329.
        let name = stream.read_extra_data_name()?;
        let count = stream.read_u32_le()?;
        let legacy = stream.bsver() <= 34;
        let mut positions = stream.allocate_vec(count)?;
        for _ in 0..count {
            let offset = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            let data = if legacy {
                FurniturePositionData::Legacy {
                    orientation: stream.read_u16_le()?,
                    position_ref_1: stream.read_u8()?,
                    position_ref_2: stream.read_u8()?,
                }
            } else {
                FurniturePositionData::Modern {
                    heading: stream.read_f32_le()?,
                    animation_type: stream.read_u16_le()?,
                    entry_properties: stream.read_u16_le()?,
                }
            };
            positions.push(FurniturePosition { offset, data });
        }
        Ok(Self {
            type_name,
            name,
            positions,
        })
    }
}

// ── BSConnectPoint::Children ───────────────────────────────────────

/// Workshop connection point child references. FO4+.
#[derive(Debug)]
pub struct BsConnectPointChildren {
    pub name: Option<Arc<str>>,
    pub skinned: bool,
    pub point_names: Vec<String>,
}

impl NiObject for BsConnectPointChildren {
    fn block_type_name(&self) -> &'static str {
        "BSConnectPoint::Children"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsConnectPointChildren {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiExtraData base — gated since 10.0.1.0 per nif.xml. See #329.
        let name = stream.read_extra_data_name()?;
        // nif.xml: `Skinned` is type `byte`, not `uint` — reading it as
        // a u32 over-consumes 3 bytes of the following `Num Connect
        // Points` count. See issue #108.
        let skinned = stream.read_u8()? != 0;
        let count = stream.read_u32_le()?;
        let mut point_names = stream.allocate_vec(count)?;
        for _ in 0..count {
            point_names.push(stream.read_sized_string()?);
        }
        Ok(Self {
            name,
            skinned,
            point_names,
        })
    }
}

// ── BSAnimNote / BSAnimNotes ──────────────────────────────────────────
//
// Bethesda IK hint blocks attached to `NiControllerSequence` via
// `anim_note_refs` / the singular `anim_notes` ref. Before #432 these
// blocks hit the `NiUnknown` fallback on every FO3/FNV/Skyrim/FO4 .kf
// file, which — combined with the per-block recovery seek — silently
// dropped the IK hints. Layout per `docs/legacy/nif.xml:6871-6891`:
//
// ```
// enum AnimNoteType : uint { 0 = INVALID, 1 = GRABIK, 2 = LOOKIK }
// BSAnimNote : NiObject {
//     Type  : AnimNoteType,
//     Time  : f32,
//     Arm   : u32   cond Type == 1  (GRABIK arm index)
//     Gain  : f32   cond Type == 2  (LOOKIK blend gain)
//     State : u32   cond Type == 2  (LOOKIK target state)
// }
// BSAnimNotes : NiObject {
//     Num Anim Notes : u16,
//     Anim Notes     : Vec<Ref<BSAnimNote>>,
// }
// ```
//
// Note: these are IK hints (grab-IK arm picking, look-IK target tracking),
// NOT the generic gameplay text events that `NiTextKeyExtraData` carries.
// Footsteps / weapon-impact / SFX triggers flow through `text_keys` as
// before.

/// Type of a [`BsAnimNote`] — matches the `AnimNoteType` enum in nif.xml.
/// Unknown numeric values preserve the raw u32 so the importer can
/// diagnose corrupted content without losing information.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnimNoteType {
    Invalid,
    GrabIk,
    LookIk,
    Unknown(u32),
}

impl AnimNoteType {
    pub fn from_u32(v: u32) -> Self {
        match v {
            0 => AnimNoteType::Invalid,
            1 => AnimNoteType::GrabIk,
            2 => AnimNoteType::LookIk,
            other => AnimNoteType::Unknown(other),
        }
    }
}

/// Single IK hint attached to an animation sequence. See the module
/// comment above for the nif.xml layout.
#[derive(Debug, Clone)]
pub struct BsAnimNote {
    pub kind: AnimNoteType,
    pub time: f32,
    /// GRABIK — arm index (0 = left, 1 = right per Bethesda convention).
    /// Present only when `kind == GrabIk`.
    pub arm: Option<u32>,
    /// LOOKIK — blend-in gain. Present only when `kind == LookIk`.
    pub gain: Option<f32>,
    /// LOOKIK — target state. Present only when `kind == LookIk`.
    pub state: Option<u32>,
}

impl NiObject for BsAnimNote {
    fn block_type_name(&self) -> &'static str {
        "BSAnimNote"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsAnimNote {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let raw_type = stream.read_u32_le()?;
        let time = stream.read_f32_le()?;
        let kind = AnimNoteType::from_u32(raw_type);
        let (arm, gain, state) = match kind {
            AnimNoteType::GrabIk => (Some(stream.read_u32_le()?), None, None),
            AnimNoteType::LookIk => {
                let gain = stream.read_f32_le()?;
                let state = stream.read_u32_le()?;
                (None, Some(gain), Some(state))
            }
            // Invalid / Unknown — no conditional tail.
            AnimNoteType::Invalid | AnimNoteType::Unknown(_) => (None, None, None),
        };
        Ok(Self {
            kind,
            time,
            arm,
            gain,
            state,
        })
    }
}

/// Collection of [`BsAnimNote`] refs — one per IK event in the sequence.
#[derive(Debug, Clone)]
pub struct BsAnimNotes {
    pub notes: Vec<BlockRef>,
}

impl NiObject for BsAnimNotes {
    fn block_type_name(&self) -> &'static str {
        "BSAnimNotes"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsAnimNotes {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let count = stream.read_u16_le()? as u32;
        let mut notes = stream.allocate_vec(count)?;
        for _ in 0..count {
            notes.push(stream.read_block_ref()?);
        }
        Ok(Self { notes })
    }
}


#[cfg(test)]
#[path = "extra_data_tests.rs"]
mod tests;
