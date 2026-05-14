//! Tests for `super::parse_block` (block dispatch table).
//!
//! Split out of the 3 667-LOC monolith into per-topic sibling files.
//! Shared fixtures (`oblivion_header`, `oblivion_bsshader_bytes`) live
//! here; everything else lives in a category-specific file:
//!
//! - [`shader`] — BSShaderPPLighting, Tile/Sky/Water/TallGrass variants
//! - [`havok`] — all `bhk*` blocks
//! - [`interpolators`] — NiPath, NiLookAt, B-spline + tread-transf
//! - [`controllers`] — NiFlip, NiBSBoneLOD, UV, KF, particle controllers
//! - [`extra_data`] — strings, bone-LOD ED, distant-object, eye-center, etc.
//! - [`nodes`] — node subtypes, BSTreeNode, BSMultiBoundNode, groupID prefix
//! - [`effects`] — lights, camera, texture, particle-modifier chain
//! - [`starfield`] — BSGeometry external/internal + skin attach

mod controllers;
mod effects;
mod extra_data;
mod havok;
mod interpolators;
mod nodes;
mod shader;
mod starfield;

use crate::header::NifHeader;
use crate::version::NifVersion;
use std::sync::Arc;

/// Build an Oblivion (bsver=0) header with a single string slot.
pub(super) fn oblivion_header() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_0_0_5,
        little_endian: true,
        user_version: 11,
        user_version_2: 0,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("SkyProp")],
        max_string_length: 8,
        num_groups: 0,
    }
}

/// Minimal Oblivion BSShaderPPLightingProperty-shaped payload: 22 bytes.
/// Matches the no-extra-fields path (no refraction/parallax).
pub(super) fn oblivion_bsshader_bytes() -> Vec<u8> {
    let mut d = Vec::new();
    // NiObjectNET: name string index
    d.extend_from_slice(&0i32.to_le_bytes());
    // extra_data_refs: count=0
    d.extend_from_slice(&0u32.to_le_bytes());
    // controller_ref: -1
    d.extend_from_slice(&(-1i32).to_le_bytes());
    // BSShaderProperty fields
    d.extend_from_slice(&0u16.to_le_bytes()); // shader_flags
    d.extend_from_slice(&1u32.to_le_bytes()); // shader_type
    d.extend_from_slice(&0u32.to_le_bytes()); // shader_flags_1
    d.extend_from_slice(&0u32.to_le_bytes()); // shader_flags_2
    d.extend_from_slice(&1.0f32.to_le_bytes()); // env_map_scale
    d.extend_from_slice(&3u32.to_le_bytes()); // texture_clamp_mode
    d.extend_from_slice(&5i32.to_le_bytes()); // texture_set_ref
    d
}

/// FO4 header (bsver=130) used by the NP-physics dispatch tests.
pub(super) fn fo4_header() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 130,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    }
}

/// FNV header (bsver=34, v20.2.0.7) used by the B-spline dispatch
/// tests. Compact B-spline interpolators are reachable on FO3/FNV per
/// the "B-splines aren't Skyrim+ only" feedback memory, so the float +
/// point3 fixtures use the FNV-style header.
pub(super) fn fnv_header_bspline() -> NifHeader {
    NifHeader {
        version: NifVersion(0x14020007),
        little_endian: true,
        user_version: 11,
        user_version_2: 34,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    }
}
