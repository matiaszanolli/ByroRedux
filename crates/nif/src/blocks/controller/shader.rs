//! Items extracted from ../mod.rs (refactor stage C).
//!
//! Lead types: ShaderControllerKind, NiLightColorController, NiLightFloatController, BsShaderController, NiMaterialColorController, NiTextureTransformController.

use super::*;

/// Which shader-property controller kind and its enum payload.
///
/// The enum value decodes differently per block type (per nif.xml
/// `EffectShaderControlledVariable` / `EffectShaderControlledColor` /
/// `LightingShaderControlledFloat` / `LightingShaderControlledColor`),
/// so keep each variant as its own newtype until the importer grows a
/// real dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderControllerKind {
    /// `BSEffectShaderPropertyFloatController.Controlled Variable` — drives
    /// `EmissiveMultiple`, `Falloff Start Angle`, `Alpha`, `U Offset`, etc.
    EffectFloat(u32),
    /// `BSEffectShaderPropertyColorController.Controlled Color` — drives
    /// the base-color tint slot (alpha component ignored).
    EffectColor(u32),
    /// `BSLightingShaderPropertyFloatController.Controlled Variable` —
    /// drives `RefractionStrength`, `GlossinessMultiple`, shader-specific
    /// slots (skin tint, parallax, multi-layer, etc.).
    LightingFloat(u32),
    /// `BSLightingShaderPropertyColorController.Controlled Color` — drives
    /// emissive / skin tint / hair tint / sparkle colors per shader type.
    LightingColor(u32),
    /// `BSLightingShaderPropertyUShortController.Controlled Variable` —
    /// short-valued slot (wetness index, snow-material index).
    LightingUShort(u32),
}

/// `NiLightColorController` — animates the ambient / diffuse color of an
/// NiLight. Per nif.xml line 3776 it inherits `NiPoint3InterpController`
/// (which is a NiSingleInterpController pass-through) and adds
/// `Target Color: LightColor (u16, since 10.1.0.0)`. On FO3+ the
/// legacy `Data: Ref<NiPosData>` field is gated off (`until 10.1.0.103`).
///
/// The `target_color` selects which slot (Diffuse = 0, Ambient = 1) the
/// animated NiPoint3 drives — the future light-animation importer needs
/// the value, so we preserve it (block-size elision would have silently
/// dropped it).
#[derive(Debug)]
pub struct NiLightColorController {
    pub base: NiTimeControllerBase,
    pub interpolator_ref: BlockRef,
    /// `LightColor` enum per nif.xml line 1241 — u16. 0 = Diffuse,
    /// 1 = Ambient. Selects which NiLight color slot the controller
    /// drives.
    pub target_color: u16,
}

impl NiObject for NiLightColorController {
    fn block_type_name(&self) -> &'static str {
        "NiLightColorController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiLightColorController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        // NiSingleInterpController: interpolator_ref (since 10.1.0.104).
        let interpolator_ref = if stream.version() >= NifVersion(0x0A010068) {
            stream.read_block_ref()?
        } else {
            BlockRef::NULL
        };
        // NiPoint3InterpController contributes no fields; NiLightColorController
        // adds `Target Color: LightColor` (u16, `since="10.1.0.0"` inclusive
        // per the version.rs doctrine). FO3+ all satisfy the gate; pre-Gamebryo
        // NetImmerse content (v < 10.1.0.0) uses the `flags` bits on the
        // NiTimeController base for slot selection instead.
        let target_color = if stream.version() >= NifVersion(0x0A010000) {
            stream.read_u16_le()?
        } else {
            0
        };
        Ok(Self {
            base,
            interpolator_ref,
            target_color,
        })
    }
}

/// Plain `NiLight*Controller` — NiLightDimmerController /
/// NiLightIntensityController / NiLightRadiusController. All three
/// inherit `NiFloatInterpController` with no additional fields beyond
/// the `NiSingleInterpController` base (nif.xml lines 3750 / 5025 /
/// 8444). The `type_name` field preserves RTTI so the future
/// light-animation importer can match on the slot it drives.
#[derive(Debug)]
pub struct NiLightFloatController {
    pub type_name: &'static str,
    pub base: NiSingleInterpController,
}

impl NiObject for NiLightFloatController {
    fn block_type_name(&self) -> &'static str {
        self.type_name
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiLightFloatController {
    pub fn parse(stream: &mut NifStream, type_name: &'static str) -> io::Result<Self> {
        Ok(Self {
            type_name,
            base: NiSingleInterpController::parse(stream)?,
        })
    }
}

/// Skyrim+ shader-property controller — `NiSingleInterpController` plus
/// a 4-byte controlled-variable enum.
#[derive(Debug)]
pub struct BsShaderController {
    /// Original block type name (e.g. `"BSEffectShaderPropertyFloatController"`)
    /// so telemetry and downstream dispatch can match the RTTI. One of 5
    /// values: `BSEffectShaderPropertyFloatController`,
    /// `BSEffectShaderPropertyColorController`,
    /// `BSLightingShaderPropertyFloatController`,
    /// `BSLightingShaderPropertyColorController`,
    /// `BSLightingShaderPropertyUShortController`.
    pub type_name: &'static str,
    pub base: NiSingleInterpController,
    pub kind: ShaderControllerKind,
}

impl NiObject for BsShaderController {
    fn block_type_name(&self) -> &'static str {
        self.type_name
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsShaderController {
    pub fn parse(stream: &mut NifStream, type_name: &'static str) -> io::Result<Self> {
        let base = NiSingleInterpController::parse(stream)?;
        let controlled_variable = stream.read_u32_le()?;
        let kind = match type_name {
            "BSEffectShaderPropertyFloatController" => {
                ShaderControllerKind::EffectFloat(controlled_variable)
            }
            "BSEffectShaderPropertyColorController" => {
                ShaderControllerKind::EffectColor(controlled_variable)
            }
            "BSLightingShaderPropertyFloatController" => {
                ShaderControllerKind::LightingFloat(controlled_variable)
            }
            "BSLightingShaderPropertyColorController" => {
                ShaderControllerKind::LightingColor(controlled_variable)
            }
            "BSLightingShaderPropertyUShortController" => {
                ShaderControllerKind::LightingUShort(controlled_variable)
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unknown BsShaderController type name: {other}"),
                ));
            }
        };
        Ok(Self {
            type_name,
            base,
            kind,
        })
    }
}

#[derive(Debug)]
pub struct NiMaterialColorController {
    pub base: NiTimeControllerBase,
    pub interpolator_ref: BlockRef,
    pub target_color: u16,
}

impl NiObject for NiMaterialColorController {
    fn block_type_name(&self) -> &'static str {
        "NiMaterialColorController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiMaterialColorController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        let interpolator_ref = if stream.version() >= NifVersion(0x0A010068) {
            stream.read_block_ref()?
        } else {
            BlockRef::NULL
        };
        // MaterialColor enum (ushort `since="10.1.0.0"` inclusive per the
        // version.rs doctrine). Pre-Gamebryo NetImmerse uses the
        // NiTimeController base `flags` bits for slot selection.
        let target_color = if stream.version() >= NifVersion(0x0A010000) {
            stream.read_u16_le()?
        } else {
            0
        };
        Ok(Self {
            base,
            interpolator_ref,
            target_color,
        })
    }
}

#[derive(Debug)]
pub struct NiTextureTransformController {
    pub base: NiTimeControllerBase,
    pub interpolator_ref: BlockRef,
    pub shader_map: bool,
    pub texture_slot: u32,
    pub operation: u32,
}

impl NiObject for NiTextureTransformController {
    fn block_type_name(&self) -> &'static str {
        "NiTextureTransformController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiTextureTransformController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        let interpolator_ref = if stream.version() >= NifVersion(0x0A010068) {
            stream.read_block_ref()?
        } else {
            BlockRef::NULL
        };
        let shader_map = stream.read_byte_bool()?;
        let texture_slot = stream.read_u32_le()?;
        let operation = stream.read_u32_le()?;
        Ok(Self {
            base,
            interpolator_ref,
            shader_map,
            texture_slot,
            operation,
        })
    }
}
