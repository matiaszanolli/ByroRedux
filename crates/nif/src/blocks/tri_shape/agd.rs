//! NiAdditionalGeometryData / BSPackedAdditionalGeometryData — per-vertex auxiliary
//! channels (tangents / bitangents / blend weights, etc.) attached to a
//! `NiGeometryData` via its `Additional Data` ref. Used predominantly in FO3 / FNV
//! architecture meshes; 4,039 blocks in vanilla corpora were previously demoted to
//! `NiUnknown` before #547.
//!
//! Split out of the prior monolithic `blocks/tri_shape.rs` (TD9-005 / #1118).

use super::super::NiObject;
use crate::stream::NifStream;
use std::any::Any;
use std::io;

// Re-export `NifVersion` into this module's scope so the `#[path]`-mounted
// `tri_shape_ni_additional_geometry_data_tests.rs` (which calls `use super::*;`)
// resolves `NifVersion::V20_2_0_7` and friends without an explicit `use` line.
// Pre-split it inherited the import from `tri_shape.rs`'s module head; the
// split moved AGD into its own file which doesn't otherwise need the import.
#[cfg(test)]
use crate::version::NifVersion;

// ── NiAdditionalGeometryData ──────────────────────────────────────────
//
// Per-vertex auxiliary channels (tangents / bitangents / blend weights /
// optional skin bone IDs) referenced by NiGeometryData.additional_data_ref.
// Replaced by BSTriShape's embedded vertex-attribute blob at 20.2.0.7+.
// Oblivion predates it; FO3 + FNV ship 4,039 vanilla blocks. See #547.
//
// Wire layout (nif.xml lines 6996-7011):
//
//   NiAdditionalGeometryData (arg=0)                  BSPackedAdditionalGeometryData (arg=1)
//     num_vertices: u16                                 (identical)
//     num_block_infos: u32                              ...
//     block_infos[num_block_infos]: NiAGDDataStream     ...
//     num_blocks: u32                                   ...
//     blocks[num_blocks]: NiAGDDataBlocks(arg)          blocks[num_blocks]: NiAGDDataBlocks(arg=1)
//
//   NiAGDDataStream (25 bytes):
//     type, unit_size, total_size, stride, block_index,
//     block_offset: u32 × 6
//     flags: u8
//
//   NiAGDDataBlocks:
//     has_data: bool
//     if has_data: NiAGDDataBlock(arg)
//
//   NiAGDDataBlock:
//     block_size: u32
//     num_blocks: u32
//     block_offsets[num_blocks]: u32
//     num_data: u32
//     data_sizes[num_data]: u32
//     data[num_data][block_size]: u8           (flat num_data * block_size byte blob)
//     if arg == 1: shader_index: u32            (BSPackedAdditionalGeometryData only)
//     if arg == 1: total_size: u32              ...

/// Per-channel descriptor: which vertex attribute this stream carries,
/// its byte layout within the packed vertex, and mutability flags.
/// nif.xml `NiAGDDataStream` (line 6969).
#[derive(Debug, Clone)]
pub struct NiAgdDataStream {
    pub ty: u32,
    pub unit_size: u32,
    pub total_size: u32,
    pub stride: u32,
    pub block_index: u32,
    pub block_offset: u32,
    pub flags: u8,
}

impl NiAgdDataStream {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        Ok(Self {
            ty: stream.read_u32_le()?,
            unit_size: stream.read_u32_le()?,
            total_size: stream.read_u32_le()?,
            stride: stream.read_u32_le()?,
            block_index: stream.read_u32_le()?,
            block_offset: stream.read_u32_le()?,
            flags: stream.read_u8()?,
        })
    }
}

/// One variable-length data block. The `data` field is a flat
/// `num_data × block_size` byte blob — the 2D `[Num Data][Block Size]`
/// layout from nif.xml is preserved row-major so consumers can slice
/// it by `block_size * row_index`.
#[derive(Debug)]
pub struct NiAgdDataBlock {
    pub block_size: u32,
    pub block_offsets: Vec<u32>,
    pub data_sizes: Vec<u32>,
    pub data: Vec<u8>,
    /// Only populated for `BSPackedAdditionalGeometryData` (nif.xml arg==1).
    pub shader_index: Option<u32>,
    /// Only populated for `BSPackedAdditionalGeometryData` (nif.xml arg==1).
    pub total_size: Option<u32>,
}

impl NiAgdDataBlock {
    fn parse(stream: &mut NifStream, packed: bool) -> io::Result<Self> {
        let block_size = stream.read_u32_le()?;
        // #981 — bulk-read both u32 arrays via `read_u32_array`.
        let num_blocks = stream.read_u32_le()? as usize;
        let block_offsets = stream.read_u32_array(num_blocks)?;
        let num_data = stream.read_u32_le()? as usize;
        let data_sizes = stream.read_u32_array(num_data)?;
        // Flat data blob: nif.xml `length="Num Data" width="Block Size"` is
        // a row-major 2D array. `read_bytes` already guards against a
        // corrupt multiplier via `check_alloc`.
        let total = (num_data as u64)
            .checked_mul(block_size as u64)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "NiAGDDataBlock: num_data * block_size overflowed u64",
                )
            })?;
        let data = stream.read_bytes(total as usize)?;
        let (shader_index, total_size) = if packed {
            (Some(stream.read_u32_le()?), Some(stream.read_u32_le()?))
        } else {
            (None, None)
        };
        Ok(Self {
            block_size,
            block_offsets,
            data_sizes,
            data,
            shader_index,
            total_size,
        })
    }
}

/// Discriminator for the two wire types that share the
/// [`NiAdditionalGeometryData`] Rust struct. `BSPackedAdditionalGeometryData`
/// appears in older FNV DLC (`nvdlc01vaultposter01.nif`) and carries the
/// two extra `shader_index` + `total_size` fields per data block. Mirrors
/// the [`BsTriShapeKind`] pattern — #560.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NiAgdKind {
    /// Plain `NiAdditionalGeometryData` — FO3 + FNV architecture tangents.
    Plain,
    /// `BSPackedAdditionalGeometryData` — nif.xml arg=1 packed variant.
    Packed,
}

/// `NiAdditionalGeometryData` / `BSPackedAdditionalGeometryData` — per-vertex
/// auxiliary channels (tangents / bitangents / blend weights, etc.) attached
/// to a NiGeometryData via its `Additional Data` ref. 4,039 FO3+FNV blocks
/// in vanilla corpora were previously demoted to `NiUnknown`. See #547.
#[derive(Debug)]
pub struct NiAdditionalGeometryData {
    pub num_vertices: u16,
    pub block_infos: Vec<NiAgdDataStream>,
    /// One entry per `num_blocks`; `None` when the `has_data` bool is false.
    pub blocks: Vec<Option<NiAgdDataBlock>>,
    pub kind: NiAgdKind,
}

impl NiObject for NiAdditionalGeometryData {
    fn block_type_name(&self) -> &'static str {
        match self.kind {
            NiAgdKind::Plain => "NiAdditionalGeometryData",
            NiAgdKind::Packed => "BSPackedAdditionalGeometryData",
        }
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiAdditionalGeometryData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        Self::parse_with_kind(stream, NiAgdKind::Plain)
    }

    pub fn parse_packed(stream: &mut NifStream) -> io::Result<Self> {
        Self::parse_with_kind(stream, NiAgdKind::Packed)
    }

    fn parse_with_kind(stream: &mut NifStream, kind: NiAgdKind) -> io::Result<Self> {
        let num_vertices = stream.read_u16_le()?;
        let num_block_infos = stream.read_u32_le()?;
        let mut block_infos = stream.allocate_vec::<NiAgdDataStream>(num_block_infos)?;
        for _ in 0..num_block_infos {
            block_infos.push(NiAgdDataStream::parse(stream)?);
        }
        let num_blocks = stream.read_u32_le()?;
        let mut blocks = stream.allocate_vec::<Option<NiAgdDataBlock>>(num_blocks)?;
        let packed = matches!(kind, NiAgdKind::Packed);
        for _ in 0..num_blocks {
            let has_data = stream.read_byte_bool()?;
            if has_data {
                blocks.push(Some(NiAgdDataBlock::parse(stream, packed)?));
            } else {
                blocks.push(None);
            }
        }
        Ok(Self {
            num_vertices,
            block_infos,
            blocks,
            kind,
        })
    }
}

#[cfg(test)]
#[path = "../tri_shape_ni_additional_geometry_data_tests.rs"]
mod ni_additional_geometry_data_tests;
