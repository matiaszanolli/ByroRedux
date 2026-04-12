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

        // Pre-Gamebryo (v < 10.0.1.0): no collision_ref. Instead, there's
        // a bounding volume (bool + variable-size struct). We read and discard it.
        let collision_ref = if stream.version() >= NifVersion(0x0A000100) {
            stream.read_block_ref()?
        } else {
            skip_bounding_volume(stream)?;
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
        let shade_flags = stream.read_u16_le()?;
        let shader_type = stream.read_u32_le()?;
        let shader_flags_1 = stream.read_u32_le()?;
        let shader_flags_2 = stream.read_u32_le()?;
        let env_map_scale = stream.read_f32_le()?;

        // BSShaderLightingProperty adds texture_clamp_mode.
        let texture_clamp_mode = stream.read_u32_le()?;

        Ok((
            Self {
                shade_flags,
                shader_type,
                shader_flags_1,
                shader_flags_2,
                env_map_scale,
            },
            texture_clamp_mode,
        ))
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
