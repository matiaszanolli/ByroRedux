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

/// NiObjectNET base class fields: name, extra data refs, controller ref.
///
/// Every named NIF block (nodes, geometry, properties, textures) inherits these.
/// Previously duplicated across 11 parsers.
#[derive(Debug, Clone)]
pub struct NiObjectNETData {
    pub name: Option<String>,
    pub extra_data_refs: Vec<BlockRef>,
    pub controller_ref: BlockRef,
}

impl NiObjectNETData {
    /// Parse NiObjectNET fields from the stream.
    /// Works for all NIF versions (string table or inline, list or single ref).
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let name = stream.read_string()?;
        let extra_data_refs = stream.read_block_ref_list()?;
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

        // Flags: u32 for version >= 20.2.0.7, u16 for older (Oblivion).
        let flags = if stream.version() >= NifVersion::V20_2_0_7 {
            stream.read_u32_le()?
        } else {
            stream.read_u16_le()? as u32
        };

        let transform = stream.read_ni_transform()?;

        // Properties list: present in pre-Skyrim (Morrowind, Oblivion, FO3/FNV).
        // Removed in Skyrim+ where shader/alpha are dedicated refs on NiGeometry.
        let properties = if stream.variant().has_properties_list() {
            stream.read_block_ref_list()?
        } else {
            Vec::new()
        };

        let collision_ref = stream.read_block_ref()?;

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
#[derive(Debug, Clone)]
pub struct BSShaderPropertyData {
    pub shader_flags: u16,
    pub shader_type: u32,
    pub shader_flags_1: u32,
    pub shader_flags_2: u32,
    pub env_map_scale: f32,
}

impl BSShaderPropertyData {
    /// Parse FO3-era BSShaderProperty + BSShaderLightingProperty base fields.
    /// Returns (shader_data, texture_clamp_mode).
    pub fn parse_fo3(stream: &mut NifStream) -> io::Result<(Self, u32)> {
        let shader_flags = stream.read_u16_le()?;
        let shader_type = stream.read_u32_le()?;
        let shader_flags_1 = stream.read_u32_le()?;
        let shader_flags_2 = stream.read_u32_le()?;
        let env_map_scale = stream.read_f32_le()?;

        // BSShaderLightingProperty adds texture_clamp_mode.
        let texture_clamp_mode = stream.read_u32_le()?;

        Ok((
            Self {
                shader_flags,
                shader_type,
                shader_flags_1,
                shader_flags_2,
                env_map_scale,
            },
            texture_clamp_mode,
        ))
    }
}
