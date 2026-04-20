//! Base class data structs for NIF block inheritance.
//!
//! Gamebryo NIF blocks form a class hierarchy:
//!   NiObject → NiObjectNET → NiAVObject → NiNode / NiGeometry / BSTriShape
//!   NiObject → NiObjectNET → NiProperty → BSShaderProperty → ...
//!
//! These structs extract the shared base class fields so each concrete block
//! parser delegates to a single implementation instead of duplicating.

use crate::stream::NifStream;
use crate::types::{BlockRef, NiTransform};
use crate::version::NifVersion;
use std::io;
use std::sync::Arc;

/// NiObjectNET base class fields: name, extra data refs, controller ref.
///
/// Every named NIF block (nodes, geometry, properties, textures) inherits these.
/// Previously duplicated across 11 parsers.
#[derive(Debug, Clone)]
pub struct NiObjectNETData {
    pub name: Option<Arc<str>>,
    pub extra_data_refs: Vec<BlockRef>,
    pub controller_ref: BlockRef,
}

impl NiObjectNETData {
    /// Parse NiObjectNET fields from the stream.
    /// Works for all NIF versions (string table or inline, list or single ref).
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let name = stream.read_string()?;

        // Pre-Gamebryo (v < 10.0.1.0): NiObjectNET stores a single extra_data ref
        // (head of a linked list) instead of a counted array. Each NiExtraData block
        // has a next_extra_data_ref field that chains them together.
        let extra_data_refs = if stream.version() < NifVersion(0x0A000100) {
            let r = stream.read_block_ref()?;
            if r.is_null() {
                Vec::new()
            } else {
                vec![r]
            }
        } else {
            stream.read_block_ref_list()?
        };

        let controller_ref = stream.read_block_ref()?;
        Ok(Self {
            name,
            extra_data_refs,
            controller_ref,
        })
    }
}

/// NiAVObject base class fields: flags, transform, properties, collision ref.
///
/// Scene graph participants (nodes, geometry) inherit these on top of NiObjectNET.
/// Previously duplicated across 3 parsers (NiNode, NiTriShape, BsTriShape).
#[derive(Debug, Clone)]
pub struct NiAVObjectData {
    pub net: NiObjectNETData,
    pub flags: u32,
    pub transform: NiTransform,
    pub properties: Vec<BlockRef>,
    pub collision_ref: BlockRef,
}

impl NiAVObjectData {
    /// Parse standard NiAVObject (reads properties list when variant has it).
    /// Used by NiNode, NiTriShape, NiTriStrips.
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;

        // Flags: u32 for BSVER > 26 (FO3+), u16 for older (Oblivion and non-Bethesda).
        // Use actual BSVER from header, not the variant's hardcoded value, to handle
        // transitional versions (e.g., Oblivion files with uv=11, bsver=11).
        let flags = if stream.bsver() > 26 {
            stream.read_u32_le()?
        } else {
            stream.read_u16_le()? as u32
        };

        let transform = stream.read_ni_transform()?;

        // Pre-Gamebryo (v <= 4.2.2.0, Morrowind): NiAVObject has a velocity vector.
        if stream.version() <= NifVersion(0x04020200) {
            let _velocity = stream.read_ni_point3()?;
        }

        // Properties list: present on every pre-Skyrim NIF per nif.xml
        // `#NI_BS_LTE_FO3#` gate (BSVER <= 34). Removed in Skyrim+ where
        // shader/alpha are dedicated refs on NiGeometry. We use the raw
        // bsver() directly rather than `variant().has_properties_list()`
        // because the variant path returns `false` for `Unknown` —
        // misclassifying non-Bethesda Gamebryo files (Civ IV, Freedom
        // Force, etc.) and causing 4-byte stream misalignment on every
        // NiAVObject. Same pattern as the u32/u16 flags check above.
        // See issue #160.
        let properties = if stream.bsver() <= 34 {
            stream.read_block_ref_list()?
        } else {
            Vec::new()
        };

        // Three version branches per nif.xml:
        //  - v <= 4.2.2.0 (Morrowind / early NetImmerse): legacy
        //    `Has Bounding Volume: bool` + optional body
        //    (since="3.0" until="4.2.2.0").
        //  - v in (4.2.2.0, 10.0.1.0) — the NetImmerse→Gamebryo gap
        //    window: neither the bounding volume nor a collision ref
        //    is serialized. Reading `has_bv` here consumes a phantom
        //    byte and misaligns every downstream NiAVObject in the
        //    [4.2.2.1, 10.0.0.x] range. See #328 / audit N1-04.
        //  - v >= 10.0.1.0: dedicated `NiCollisionObject` ref
        //    (since="10.0.1.0").
        let collision_ref = if stream.version() >= NifVersion(0x0A000100) {
            stream.read_block_ref()?
        } else if stream.version() <= NifVersion(0x04020200) {
            skip_bounding_volume(stream)?;
            BlockRef::NULL
        } else {
            BlockRef::NULL
        };

        Ok(Self {
            net,
            flags,
            transform,
            properties,
            collision_ref,
        })
    }

    /// Parse NiAVObject WITHOUT properties list (for BSTriShape).
    /// BSTriShape inherits NiAVObject but never has a properties array —
    /// it uses direct shader_property_ref/alpha_property_ref fields instead.
    pub fn parse_no_properties(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;

        // BSTriShape is Skyrim+ only — flags are always u32.
        let flags = stream.read_u32_le()?;
        let transform = stream.read_ni_transform()?;
        let collision_ref = stream.read_block_ref()?;

        Ok(Self {
            net,
            flags,
            transform,
            properties: Vec::new(),
            collision_ref,
        })
    }
}

/// BSShaderProperty base class fields (FO3/FNV era).
///
/// Shared by BSShaderPPLightingProperty and BSShaderNoLightingProperty.
/// BSLightingShaderProperty (Skyrim+) has a different layout and parses its own fields.
///
/// Inheritance chain on disk:
///   NiProperty → NiShadeProperty → BSShaderProperty → BSShaderLightingProperty
///
/// The first u16 (`shade_flags`) is **NiShadeProperty.Flags**, *not*
/// BSShaderProperty's own shader flags (which are the u32 pair
/// `shader_flags_1` / `shader_flags_2`). Previously this field was
/// named `shader_flags` which conflated the two levels. See #167.
#[derive(Debug, Clone)]
pub struct BSShaderPropertyData {
    /// NiShadeProperty flags (u16) — smooth/specular bits at the
    /// NiShadeProperty inheritance level. NOT the BSShaderProperty
    /// u32 shader flags.
    pub shade_flags: u16,
    pub shader_type: u32,
    pub shader_flags_1: u32,
    pub shader_flags_2: u32,
    pub env_map_scale: f32,
}

impl BSShaderPropertyData {
    /// Parse FO3-era BSShaderProperty + BSShaderLightingProperty base fields.
    /// Returns (shader_data, texture_clamp_mode).
    pub fn parse_fo3(stream: &mut NifStream) -> io::Result<(Self, u32)> {
        let shader = Self::parse_base(stream)?;
        // BSShaderLightingProperty adds texture_clamp_mode.
        let texture_clamp_mode = stream.read_u32_le()?;
        Ok((shader, texture_clamp_mode))
    }

    /// Parse BSShaderProperty base only (no texture_clamp_mode).
    ///
    /// Used by blocks that inherit `BSShaderProperty` directly rather than
    /// via `BSShaderLightingProperty` — e.g. `WaterShaderProperty`,
    /// `TallGrassShaderProperty`, `DistantLODShaderProperty` (nif.xml
    /// lines 6322/6354/6346). Previously these were aliased to
    /// `BSShaderPPLightingProperty::parse` which over-read the Lighting
    /// branch + refraction + parallax fields, masked by `block_sizes`
    /// recovery. See issue #474.
    pub fn parse_base(stream: &mut NifStream) -> io::Result<Self> {
        let shade_flags = stream.read_u16_le()?;
        let shader_type = stream.read_u32_le()?;
        let shader_flags_1 = stream.read_u32_le()?;
        let shader_flags_2 = stream.read_u32_le()?;
        let env_map_scale = stream.read_f32_le()?;

        Ok(Self {
            shade_flags,
            shader_type,
            shader_flags_1,
            shader_flags_2,
            env_map_scale,
        })
    }
}

// ── Pre-Gamebryo bounding volume ──────────────────────────────────────

/// Read and discard a NiBoundingVolume (pre-Gamebryo NiAVObject).
/// The bounding volume replaces the collision_ref in NIF v < 10.0.1.0.
fn skip_bounding_volume(stream: &mut NifStream) -> io::Result<()> {
    let has_bv = stream.read_u8()? != 0;
    if has_bv {
        read_and_skip_bounding_volume(stream)?;
    }
    Ok(())
}

fn read_and_skip_bounding_volume(stream: &mut NifStream) -> io::Result<()> {
    let bv_type = stream.read_u32_le()?;
    match bv_type {
        0 => {
            // SPHERE: center(3×f32) + radius(f32) = 16 bytes
            stream.skip(16)?;
        }
        1 => {
            // BOX: center(3×f32) + 3 axes(9×f32) + extents(3×f32) = 60 bytes
            stream.skip(60)?;
        }
        2 => {
            // CAPSULE: center(3×f32) + origin(3×f32) + extent(f32) + radius(f32) = 32 bytes
            stream.skip(32)?;
        }
        4 => {
            // UNION: num_bv(u32) + BoundingVolume[num_bv]
            let count = stream.read_u32_le()?;
            for _ in 0..count {
                read_and_skip_bounding_volume(stream)?;
            }
        }
        5 => {
            // HALF_SPACE: plane(4×f32) + center(3×f32) = 28 bytes
            stream.skip(28)?;
        }
        _ => {
            log::warn!("Unknown bounding volume type {}, skipping", bv_type);
        }
    }
    Ok(())
}

#[cfg(test)]
mod niavobject_version_gate_tests {
    use super::*;
    use crate::header::NifHeader;
    use crate::stream::NifStream;

    fn header_at(version: NifVersion) -> NifHeader {
        NifHeader {
            version,
            little_endian: true,
            user_version: 0,
            user_version_2: 0,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        }
    }

    /// Common prologue: NiObjectNET (pre-Gamebryo variant — single
    /// extra_data ref) + flags (u16 at bsver=0) + identity transform.
    fn pre_gamebryo_prologue() -> Vec<u8> {
        let mut d = Vec::new();
        // NiObjectNET at version < 20.1.0.1: name is an inline
        // length-prefixed string (u32 length, 0 = empty ⇒ None). No body bytes.
        d.extend_from_slice(&0u32.to_le_bytes()); // name length = 0
                                                  // extra_data_ref (single i32 ref, -1 = null) — pre-Gamebryo branch.
        d.extend_from_slice(&(-1i32).to_le_bytes());
        // controller_ref (-1 = null).
        d.extend_from_slice(&(-1i32).to_le_bytes());
        // flags (bsver=0 ⇒ u16 branch)
        d.extend_from_slice(&0u16.to_le_bytes());
        // translation (3 f32)
        for _ in 0..3 {
            d.extend_from_slice(&0.0f32.to_le_bytes());
        }
        // identity 3×3 rotation (9 f32)
        for row in 0..3 {
            for col in 0..3 {
                let v: f32 = if row == col { 1.0 } else { 0.0 };
                d.extend_from_slice(&v.to_le_bytes());
            }
        }
        // scale
        d.extend_from_slice(&1.0f32.to_le_bytes());
        d
    }

    /// Regression: #328 / audit N1-04 — NiAVObject in the
    /// (4.2.2.0, 10.0.1.0) gap window has neither `Has Bounding Volume`
    /// (nif.xml until=4.2.2.0) nor `Collision Object` (since=10.0.1.0).
    /// The parser must consume neither.
    #[test]
    fn gap_window_reads_neither_bv_nor_collision_ref() {
        // 10.0.0.0 — firmly in the gap.
        let header = header_at(NifVersion(0x0A000000));
        let mut bytes = pre_gamebryo_prologue();
        // Properties list (bsver=0 ≤ 34 ⇒ list present): zero entries.
        bytes.extend_from_slice(&0u32.to_le_bytes());
        // No velocity (version > 4.2.2.0).
        // No bounding volume, no collision_ref — stream ends here.

        let mut stream = NifStream::new(&bytes, &header);
        let data = NiAVObjectData::parse(&mut stream)
            .expect("NiAVObject should parse in the NetImmerse→Gamebryo gap");
        assert_eq!(
            stream.position() as usize,
            bytes.len(),
            "gap-window NiAVObject must consume stream exactly — no phantom \
             has_bv bool and no phantom collision_ref"
        );
        assert!(data.collision_ref.is_null());
        assert!(data.properties.is_empty());
    }

    /// Pre-Gamebryo (v <= 4.2.2.0) still reads the legacy
    /// `Has Bounding Volume` bool (plus the per-version velocity vector).
    /// has_bv=false keeps the trailing body out of the fixture.
    #[test]
    fn pre_gamebryo_consumes_has_bounding_volume_bool() {
        let header = header_at(NifVersion(0x04020200));
        let mut bytes = pre_gamebryo_prologue();
        // Pre-Gamebryo velocity vector (3 f32) — see existing parse() branch.
        for _ in 0..3 {
            bytes.extend_from_slice(&0.0f32.to_le_bytes());
        }
        // Properties list: bsver=0 ≤ 34 ⇒ list present, zero entries.
        bytes.extend_from_slice(&0u32.to_le_bytes());
        // has_bv = false (no body follows).
        bytes.push(0u8);

        let mut stream = NifStream::new(&bytes, &header);
        let data =
            NiAVObjectData::parse(&mut stream).expect("pre-Gamebryo NiAVObject should parse");
        assert_eq!(
            stream.position() as usize,
            bytes.len(),
            "pre-Gamebryo NiAVObject must consume the velocity + has_bv bool"
        );
        assert!(data.collision_ref.is_null());
    }
}
