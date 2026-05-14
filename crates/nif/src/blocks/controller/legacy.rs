//! Items extracted from ../mod.rs (refactor stage C).
//!
//! Lead types: NiSequenceStreamHelper, NiUVController, NiLookAtController, NiPathController.

use super::*;

#[derive(Debug)]
pub struct NiSequenceStreamHelper {
    pub net: NiObjectNETData,
}

impl NiObject for NiSequenceStreamHelper {
    fn block_type_name(&self) -> &'static str {
        "NiSequenceStreamHelper"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiSequenceStreamHelper {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        Ok(Self {
            net: NiObjectNETData::parse(stream)?,
        })
    }
}

#[derive(Debug)]
pub struct NiUVController {
    pub base: NiTimeControllerBase,
    /// Texture slot index to animate. 0 = base, 1 = normal, etc. Rarely
    /// non-zero in Bethesda content.
    pub target_attribute: u16,
    /// Ref to the NiUVData block with the four KeyGroup channels.
    pub data_ref: BlockRef,
}

impl NiObject for NiUVController {
    fn block_type_name(&self) -> &'static str {
        "NiUVController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiUVController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        let target_attribute = stream.read_u16_le()?;
        let data_ref = stream.read_block_ref()?;
        Ok(Self {
            base,
            target_attribute,
            data_ref,
        })
    }
}

/// Legacy look-at constraint controller.
///
/// Rotates its owning block so that a chosen axis points at the
/// `look_at_ref` target every frame. The `flags` bit layout is the
/// `LookAtFlags` from nif.xml:
///   - bit 0: LOOK_FLIP (invert the follow axis)
///   - bit 1: LOOK_Y_AXIS (follow axis = Y instead of X)
///   - bit 2: LOOK_Z_AXIS (follow axis = Z instead of X)
///
/// The `flags` field is only present from version 10.1.0.0 onwards; on
/// earlier files only `look_at_ref` follows the base.
#[derive(Debug)]
pub struct NiLookAtController {
    pub base: NiTimeControllerBase,
    pub look_at_flags: u16,
    pub look_at_ref: BlockRef,
}

impl NiObject for NiLookAtController {
    fn block_type_name(&self) -> &'static str {
        "NiLookAtController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiLookAtController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        let look_at_flags = if stream.version() >= NifVersion::V10_1_0_0 {
            stream.read_u16_le()?
        } else {
            0
        };
        let look_at_ref = stream.read_block_ref()?;
        Ok(Self {
            base,
            look_at_flags,
            look_at_ref,
        })
    }
}

/// Legacy spline-path follower controller.
///
/// Walks the owning block along a 3D spline defined by `path_data_ref`
/// (NiPosData with XYZ keys) parameterized by `percent_data_ref`
/// (NiFloatData mapping time → [0, 1] along the path). `bank_dir` +
/// `max_bank_angle` drive roll around the motion axis, `smoothing`
/// dampens tangent changes, and `follow_axis` picks which local axis
/// tracks the tangent (0 = X, 1 = Y, 2 = Z).
///
/// The `path_flags` field is only present from version 10.1.0.0 onwards.
#[derive(Debug)]
pub struct NiPathController {
    pub base: NiTimeControllerBase,
    pub path_flags: u16,
    pub bank_dir: i32,
    pub max_bank_angle: f32,
    pub smoothing: f32,
    pub follow_axis: i16,
    pub path_data_ref: BlockRef,
    pub percent_data_ref: BlockRef,
}

impl NiObject for NiPathController {
    fn block_type_name(&self) -> &'static str {
        "NiPathController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiPathController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        let path_flags = if stream.version() >= NifVersion::V10_1_0_0 {
            stream.read_u16_le()?
        } else {
            0
        };
        let bank_dir = stream.read_i32_le()?;
        let max_bank_angle = stream.read_f32_le()?;
        let smoothing = stream.read_f32_le()?;
        // follow_axis is nominally `short` in nif.xml but the defined
        // range is 0/1/2 (X/Y/Z); read as u16 and reinterpret.
        let follow_axis = stream.read_u16_le()? as i16;
        let path_data_ref = stream.read_block_ref()?;
        let percent_data_ref = stream.read_block_ref()?;
        Ok(Self {
            base,
            path_flags,
            bank_dir,
            max_bank_angle,
            smoothing,
            follow_axis,
            path_data_ref,
            percent_data_ref,
        })
    }
}
