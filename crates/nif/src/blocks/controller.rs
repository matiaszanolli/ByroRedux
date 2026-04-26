//! NIF animation controller blocks.
//!
//! Covers the NiTimeController hierarchy and NiControllerSequence.
//! Parsed enough to advance the stream correctly; actual animation
//! interpretation comes later.

use super::base::NiObjectNETData;
use super::NiObject;
use crate::stream::NifStream;
use crate::types::BlockRef;
use crate::version::NifVersion;
use std::any::Any;
use std::io;
use std::sync::Arc;

// ── NiTimeController base ──────────────────────────────────────────────

/// Base fields for all NiTimeController subclasses (26 bytes).
#[derive(Debug)]
pub struct NiTimeControllerBase {
    pub next_controller_ref: BlockRef,
    pub flags: u16,
    pub frequency: f32,
    pub phase: f32,
    pub start_time: f32,
    pub stop_time: f32,
    pub target_ref: BlockRef,
}

impl NiTimeControllerBase {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let next_controller_ref = stream.read_block_ref()?;
        let flags = stream.read_u16_le()?;
        let frequency = stream.read_f32_le()?;
        let phase = stream.read_f32_le()?;
        let start_time = stream.read_f32_le()?;
        let stop_time = stream.read_f32_le()?;
        let target_ref = stream.read_block_ref()?;
        Ok(Self {
            next_controller_ref,
            flags,
            frequency,
            phase,
            start_time,
            stop_time,
            target_ref,
        })
    }
}

// ── NiTimeController (fallback for unknown controller subtypes) ────────

/// Stub for unknown controller types. Reads only the base 26 bytes.
#[derive(Debug)]
pub struct NiTimeController {
    pub base: NiTimeControllerBase,
}

impl NiObject for NiTimeController {
    fn block_type_name(&self) -> &'static str {
        "NiTimeController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiTimeController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        Ok(Self {
            base: NiTimeControllerBase::parse(stream)?,
        })
    }
}

// ── NiSingleInterpController ───────────────────────────────────────────
// Adds: interpolator_ref (Ref = i32 = 4 bytes) for version >= 10.1.0.104.
// Subclasses: NiTransformController, NiVisController, NiAlphaController,
//             NiTextureTransformController, NiKeyframeController, etc.

/// Controller with a single interpolator reference.
/// Used for NiTransformController, NiVisController, NiAlphaController,
/// NiTextureTransformController, and BSShader*Controller types.
#[derive(Debug)]
pub struct NiSingleInterpController {
    pub base: NiTimeControllerBase,
    pub interpolator_ref: BlockRef,
}

impl NiObject for NiSingleInterpController {
    fn block_type_name(&self) -> &'static str {
        "NiSingleInterpController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiSingleInterpController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        // NiSingleInterpController: interpolator ref (since 10.1.0.104)
        let interpolator_ref = if stream.version() >= NifVersion(0x0A010068) {
            stream.read_block_ref()?
        } else {
            BlockRef::NULL
        };
        Ok(Self {
            base,
            interpolator_ref,
        })
    }
}

// ── NiFlipController ───────────────────────────────────────────────────
//
// Texture-flip animation controller (flipbook / water-ripple / caustic
// cycling). Subclass of `NiFloatInterpController` →
// `NiSingleInterpController` → `NiTimeController`. nif.xml line 3720.
// On Oblivion (20.0.0.5) the version-gated `Accum Time` and `Delta`
// fields are out (they're `until 10.1.0.103`), so the on-disk layout
// reduces to:
//
//   NiTimeController base              (26 B)
//   NiSingleInterpController           (4 B — interpolator ref)
//   Texture Slot    (TexType u32)      (4 B)
//   Num Sources     (u32)              (4 B)
//   Sources[Num Sources]               (4 B each)
//   = 38 + 4 × N bytes
//
// Pre-#394 this was a terminal parse error on Oblivion creatures
// that ship flipbook textures, since there's no `block_sizes` table
// to fall back on. See audit OBL-D5-H2.

/// `NiFlipController` — flipbook / texture-cycle animation driver.
#[derive(Debug)]
pub struct NiFlipController {
    pub base: NiSingleInterpController,
    /// `TexType` enum — 0=BASE_MAP, 4=GLOW_MAP, etc. Kept as u32 so
    /// the consumer can route to the right texture slot.
    pub texture_slot: u32,
    /// References to the source textures to cycle through. Typically
    /// 2–8 frames for water ripples or fire flicker.
    pub sources: Vec<BlockRef>,
}

impl NiObject for NiFlipController {
    fn block_type_name(&self) -> &'static str {
        "NiFlipController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiFlipController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiSingleInterpController::parse(stream)?;
        // `Accum Time` (f32 since 3.3.0.13 until 10.1.0.103) and
        // `Delta` (f32 until 10.1.0.103) are version-gated off on
        // every supported Bethesda NIF (Oblivion / FO3 / FNV /
        // Skyrim / FO4 / FO76 / Starfield are all >= 10.1.0.104).
        // Nothing to read here.
        let texture_slot = stream.read_u32_le()?;
        let num_sources = stream.read_u32_le()?;
        let mut sources = stream.allocate_vec::<BlockRef>(num_sources)?;
        for _ in 0..num_sources {
            sources.push(stream.read_block_ref()?);
        }
        Ok(Self {
            base,
            texture_slot,
            sources,
        })
    }
}

// ── NiBSBoneLODController ──────────────────────────────────────────────
//
// Bethesda extension of the deprecated `NiBoneLODController` — a
// creature-skeleton LOD switcher keyed off camera distance. 34 files
// in the Oblivion vanilla sweep tripped the unknown-block dispatch
// before #394. Subclass of `NiBoneLODController` →
// `NiTimeController`. nif.xml line 3836.
//
// Wire layout — three phases, with the shape-group tail gated on
// non-Bethesda content per nif.xml `vercond="#NISTREAM#"`
// (`#NISTREAM#` resolves to `#BSVER# #EQ# 0`, line 10):
//
//   NiTimeController base                   (26 B)
//   LOD             (u32)                   ( 4 B)
//   Num LODs        (u32)                   ( 4 B)
//   Num Node Groups (u32)                   ( 4 B, dropped — see below)
//   Node Groups[Num LODs] — each is:
//       Num Nodes (u32) + Nodes[Num Nodes] (Ptr, 4 B each)
//   ── only when bsver == 0 (Morrowind / Oblivion / non-Bethesda):
//   Num Shape Groups (u32)                  ( 4 B — since 4.2.2.0)
//   Shape Groups 1[Num Shape Groups] — each is:
//       Num Skin Info (u32) + SkinInfo[Num Skin Info] (8 B each)
//   Num Shape Groups 2 (u32)                ( 4 B)
//   Shape Groups 2[Num Shape Groups 2]      (Ref, 4 B each)
//
// Pre-fix the shape-group tail was always read, so on every Bethesda
// game past Morrowind/Oblivion (FO3 bsver=21 onward) the parser ate
// 4+ extra bytes past the block body, hit `0xFFFFFFFF` reading the
// next block's data as `Num Shape Groups`, and bailed via `allocate_vec`
// with "NIF claims 4294967295 elements". 34 instances regressed on
// FNV (`meshes/characters/_male/skeleton.nif` and every creature
// skeleton) — surfaced by the R3 per-block histogram landing in the
// same session. `Num Node Groups` (the unused u32) is preserved on
// the wire for **all** content per nif.xml line 3828: it has no
// `vercond` and no `since` gate, so it's always present even though
// our consumer drops it.
//
// Variable-size with two nested dynamic arrays. Parsed eagerly so
// Oblivion loads don't truncate the rest of the NIF waiting on a
// `block_sizes` hint that never comes.

/// One `NodeSet` entry inside the outer LOD table — a bone-index
/// list that maps distance-LOD-level → which bones to activate.
#[derive(Debug)]
pub struct NodeSet {
    pub nodes: Vec<BlockRef>,
}

/// One `SkinInfo` entry inside the outer `NiBoneLODController`
/// shape-groups table: a (shape, skin_instance) pair.
#[derive(Debug, Clone, Copy)]
pub struct BoneLodSkinInfo {
    pub shape_ptr: BlockRef,
    pub skin_instance_ref: BlockRef,
}

/// One `SkinInfoSet` entry — a group of `BoneLodSkinInfo` pairs.
#[derive(Debug)]
pub struct BoneLodSkinInfoSet {
    pub skin_infos: Vec<BoneLodSkinInfo>,
}

/// `NiBSBoneLODController` — Bethesda creature-skeleton LOD switcher.
#[derive(Debug)]
pub struct NiBsBoneLodController {
    pub base: NiTimeControllerBase,
    /// Current active LOD level (authored initial value).
    pub lod: u32,
    /// Per-LOD bone-index lists; outer length == `num_lods`.
    pub node_groups: Vec<NodeSet>,
    /// Shape groups (primary) — present since 4.2.2.0 on every
    /// Bethesda target version.
    pub shape_groups_1: Vec<BoneLodSkinInfoSet>,
    /// Shape groups (secondary) — refs to `NiTriBasedGeom` parents.
    pub shape_groups_2: Vec<BlockRef>,
}

impl NiObject for NiBsBoneLodController {
    fn block_type_name(&self) -> &'static str {
        "NiBSBoneLODController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiBsBoneLodController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        let lod = stream.read_u32_le()?;
        let num_lods = stream.read_u32_le()?;
        let _num_node_groups = stream.read_u32_le()?;
        let mut node_groups = stream.allocate_vec::<NodeSet>(num_lods)?;
        for _ in 0..num_lods {
            let count = stream.read_u32_le()?;
            let mut nodes = stream.allocate_vec::<BlockRef>(count)?;
            for _ in 0..count {
                nodes.push(stream.read_block_ref()?);
            }
            node_groups.push(NodeSet { nodes });
        }
        // Shape-group tail is non-Bethesda only. nif.xml gates each of
        // these four fields on `vercond="#NISTREAM#"` which expands to
        // `#BSVER# #EQ# 0` — i.e. Morrowind, Oblivion, and pure-Niflib
        // content only. Every Bethesda game past Oblivion (FO3 bsver=21
        // → Starfield bsver=172) ends the block at the close of
        // node_groups; reading further over-consumes by 4+ bytes and
        // trips `allocate_vec` on whatever sentinel the next block
        // happens to start with.
        let (shape_groups_1, shape_groups_2) = if stream.bsver() == 0 {
            let num_shape_groups = stream.read_u32_le()?;
            let mut shape_groups_1 =
                stream.allocate_vec::<BoneLodSkinInfoSet>(num_shape_groups)?;
            for _ in 0..num_shape_groups {
                let count = stream.read_u32_le()?;
                let mut skin_infos = stream.allocate_vec::<BoneLodSkinInfo>(count)?;
                for _ in 0..count {
                    let shape_ptr = stream.read_block_ref()?;
                    let skin_instance_ref = stream.read_block_ref()?;
                    skin_infos.push(BoneLodSkinInfo {
                        shape_ptr,
                        skin_instance_ref,
                    });
                }
                shape_groups_1.push(BoneLodSkinInfoSet { skin_infos });
            }
            let num_shape_groups_2 = stream.read_u32_le()?;
            let mut shape_groups_2 = stream.allocate_vec::<BlockRef>(num_shape_groups_2)?;
            for _ in 0..num_shape_groups_2 {
                shape_groups_2.push(stream.read_block_ref()?);
            }
            (shape_groups_1, shape_groups_2)
        } else {
            (Vec::new(), Vec::new())
        };
        Ok(Self {
            base,
            lod,
            node_groups,
            shape_groups_1,
            shape_groups_2,
        })
    }
}

// ── bhkBlendController ─────────────────────────────────────────────────
//
// Havok ragdoll blend controller — drives blend weights between multiple
// Havok animations (typically skeleton.nif files per nif.xml line 3927).
// 1,427 blocks across vanilla FNV (845) + FO3 (582) fell into NiUnknown
// pre-#551 because no dispatch arm existed.
//
// Wire layout (nif.xml line 3927):
//   NiTimeController base (26 B — nif.xml line 3600)
//   Keys: uint (4 B — "Seems to be always zero.")
//
// Note: contrary to the audit's suggestion, this is NOT a
// NiSingleInterpController — it inherits NiTimeController directly, with
// NO interpolator ref. The trailing `keys` u32 is the only field.

/// bhkBlendController — Havok blend-weight controller for ragdoll /
/// animation layering on FO3 + FNV skeletons. See #551.
#[derive(Debug)]
pub struct BhkBlendController {
    pub base: NiTimeControllerBase,
    /// Per nif.xml always zero on disk, but preserved so a future
    /// importer can branch if Bethesda ever shipped a non-zero value.
    pub keys: u32,
}

impl NiObject for BhkBlendController {
    fn block_type_name(&self) -> &'static str {
        "bhkBlendController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkBlendController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        let keys = stream.read_u32_le()?;
        Ok(Self { base, keys })
    }
}

// ── BSNiAlphaPropertyTestRefController ─────────────────────────────────
//
// Skyrim SE controller that animates the alpha-test threshold on
// `NiAlphaProperty` (dissolve effects, fade transitions, ghost-reveal
// VFX). Wire layout inherits `NiFloatInterpController` → `NiSingleInterpController`
// with no additional fields (nif.xml line 6279). Pre-#552, 751 SE
// blocks fell into NiUnknown because no dispatch arm existed.
//
// Wrapped in a dedicated newtype so telemetry and downstream importers
// see the original RTTI name via `block_type_name()` — the bulk of the
// `NiSingleInterpController`-aliased family currently erases its RTTI
// (separate tech debt, not in scope here).

/// Skyrim SE animated alpha-test threshold controller. See #552.
#[derive(Debug)]
pub struct BsNiAlphaPropertyTestRefController {
    pub base: NiSingleInterpController,
}

impl NiObject for BsNiAlphaPropertyTestRefController {
    fn block_type_name(&self) -> &'static str {
        "BSNiAlphaPropertyTestRefController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsNiAlphaPropertyTestRefController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        Ok(Self {
            base: NiSingleInterpController::parse(stream)?,
        })
    }
}

// ── NiFloatExtraDataController ─────────────────────────────────────────
//
// Animates a NiFloatExtraData tag attached to an NiAVObject (FOV
// multipliers, scale overrides, wetness levels — any tool-authored
// engine hook that exposes a float knob). nif.xml line 3797:
//
//   NiTimeController base                       (26 B)
//   NiSingleInterpController.interpolator_ref   (4 B, since 10.1.0.104)
//   NiExtraDataController.extra_data_name       (4 B string index,
//                                                since 10.2.0.0)
//   (pre-10.1.0.0 extras gated out on FO3+)
//
// 1,657 blocks across SE (180 controller + 1,312 data) + FO3/FNV fell
// into NiUnknown pre-#553 because no dispatch existed. `NiFloatExtraData`
// itself rides the shared `NiExtraData` dispatch; this is the controller
// that ticks it over time.

/// FO3+ animated NiFloatExtraData controller. See #553.
#[derive(Debug)]
pub struct NiFloatExtraDataController {
    pub base: NiTimeControllerBase,
    pub interpolator_ref: BlockRef,
    /// Name of the NiFloatExtraData tag this controller animates.
    /// Resolved against the header string table at 20.1+.
    pub extra_data_name: Option<Arc<str>>,
}

impl NiObject for NiFloatExtraDataController {
    fn block_type_name(&self) -> &'static str {
        "NiFloatExtraDataController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiFloatExtraDataController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        // NiSingleInterpController.interpolator_ref (since 10.1.0.104).
        let interpolator_ref = if stream.version() >= NifVersion(0x0A010068) {
            stream.read_block_ref()?
        } else {
            BlockRef::NULL
        };
        // NiExtraDataController.extra_data_name (since 10.2.0.0).
        let extra_data_name = if stream.version() >= NifVersion(0x0A020000) {
            stream.read_string()?
        } else {
            None
        };
        Ok(Self {
            base,
            interpolator_ref,
            extra_data_name,
        })
    }
}

// ── BSShaderController family ──────────────────────────────────────────
//
// The four (+1) Bethesda shader property controllers each wrap
// `NiSingleInterpController` with a trailing `uint` enum identifying
// which shader slot the animation drives. Pre-#407 the trailing u32
// was unconsumed and block_size recovery seeked past; #407 added the
// read but dropped the value on the floor. This block preserves the
// value on a typed `BsShaderController` so the animation importer
// can route key streams to the correct shader uniform once the
// animated-shader pipeline lands. See #350 / audit S5-02.

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

// ── NiLight controller family ──────────────────────────────────────────
//
// Animated controllers on NiLight/NiPointLight/NiSpotLight:
//   - NiLightColorController: ambient/diffuse color animation.
//     Inherits NiPoint3InterpController + `target_color: LightColor (u16)`.
//   - NiLightDimmerController: overall dimmer value.
//     Inherits NiFloatInterpController, no extra fields.
//   - NiLightIntensityController (FO3+): HDR intensity.
//     Inherits NiFloatInterpController, no extra fields.
//   - NiLightRadiusController (FO4+): light radius.
//     Inherits NiFloatInterpController, no extra fields.
//
// Pre-#433 all four were missing from dispatch — every lantern flicker,
// campfire pulse, torch flicker, magic-spell glow, terminal-screen bloom,
// and plasma weapon effect in Bethesda content landed as NiUnknown and
// silently stopped animating. Per nif.xml lines 3776 / 3750 / 5025 /
// 8444.

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
        // adds `Target Color: LightColor` (u16, since 10.1.0.0). FO3+ all
        // satisfy the version gate.
        let target_color = stream.read_u16_le()?;
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

// ── NiMaterialColorController ──────────────────────────────────────────
// Inherits NiSingleInterpController, adds: target_color (MaterialColor enum, u16).

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
        // MaterialColor enum (ushort since 10.1.0.0)
        let target_color = stream.read_u16_le()?;
        Ok(Self {
            base,
            interpolator_ref,
            target_color,
        })
    }
}

// ── NiTextureTransformController ───────────────────────────────────────
// Inherits NiFloatInterpController → NiSingleInterpController, adds:
// shader_map (bool), texture_slot (u32 TexType), operation (u32 TransformMember).

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

// ── NiMultiTargetTransformController ───────────────────────────────────
// Inherits NiInterpController (which adds nothing for FNV), adds:
// num_extra_targets (u16) + extra_targets (Ptr[]).

#[derive(Debug)]
pub struct NiMultiTargetTransformController {
    pub base: NiTimeControllerBase,
    pub extra_targets: Vec<BlockRef>,
}

impl NiObject for NiMultiTargetTransformController {
    fn block_type_name(&self) -> &'static str {
        "NiMultiTargetTransformController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiMultiTargetTransformController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        let num_extra_targets = stream.read_u16_le()? as u32;
        let mut extra_targets = stream.allocate_vec(num_extra_targets)?;
        for _ in 0..num_extra_targets {
            extra_targets.push(stream.read_block_ref()?);
        }
        Ok(Self {
            base,
            extra_targets,
        })
    }
}

// ── NiControllerManager ────────────────────────────────────────────────
// Inherits NiTimeController, adds: cumulative (bool, 1 byte), sequences, palette.

#[derive(Debug)]
pub struct NiControllerManager {
    pub base: NiTimeControllerBase,
    pub cumulative: bool,
    pub sequence_refs: Vec<BlockRef>,
    pub object_palette_ref: BlockRef,
}

impl NiObject for NiControllerManager {
    fn block_type_name(&self) -> &'static str {
        "NiControllerManager"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiControllerManager {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        // cumulative is a byte bool based on observed block sizes
        let cumulative = stream.read_byte_bool()?;
        let num_sequences = stream.read_u32_le()?;
        let mut sequence_refs = stream.allocate_vec(num_sequences)?;
        for _ in 0..num_sequences {
            sequence_refs.push(stream.read_block_ref()?);
        }
        let object_palette_ref = stream.read_block_ref()?;
        Ok(Self {
            base,
            cumulative,
            sequence_refs,
            object_palette_ref,
        })
    }
}

// ── NiControllerSequence ───────────────────────────────────────────────
// Does NOT inherit NiTimeController. Inherits NiSequence → NiObject.

/// A single controlled block entry within a NiControllerSequence.
///
/// There are two disjoint on-disk layouts for the string fields, and
/// which one a file uses depends on its NIF version:
///
/// - **v ≥ 20.1.0.1** (FNV, Skyrim, FO4+): each string is an index into
///   the file's global string table. The importer resolves them to the
///   `node_name` / `property_type` / `controller_type` / `controller_id`
///   / `interpolator_id` `Option<Arc<str>>` fields during parse.
///
/// - **10.2.0.0 ≤ v ≤ 20.1.0.0** (Oblivion, Morrowind BBBB-era content):
///   the block has no strings inline; instead it carries a
///   `string_palette_ref` pointing at an `NiStringPalette` block plus
///   five `u32` byte offsets into that palette. The palette itself
///   stores the concatenated UTF-8 names; a downstream importer pass
///   slices them out (see [`NiStringPalette::get_string`]). The
///   `Option<Arc<str>>` name fields stay `None` on this path — the
///   parser does not cross-link blocks.
///
/// Both layouts are present in the struct to keep the type simple;
/// callers pick whichever set is populated based on
/// `string_palette_ref.is_null()`. See issue #107.
#[derive(Debug)]
pub struct ControlledBlock {
    pub interpolator_ref: BlockRef,
    pub controller_ref: BlockRef,
    pub priority: u8,
    /// Resolved string (modern format) or `None` (palette format or
    /// unresolved).
    pub node_name: Option<Arc<str>>,
    pub property_type: Option<Arc<str>>,
    pub controller_type: Option<Arc<str>>,
    pub controller_id: Option<Arc<str>>,
    pub interpolator_id: Option<Arc<str>>,
    /// Palette-format fields (Oblivion / Morrowind BBBB era). Null ref
    /// on the modern string-table path.
    pub string_palette_ref: BlockRef,
    pub node_name_offset: u32,
    pub property_type_offset: u32,
    pub controller_type_offset: u32,
    pub controller_id_offset: u32,
    pub interpolator_id_offset: u32,
}

#[derive(Debug)]
pub struct NiControllerSequence {
    // NiSequence fields
    pub name: Option<Arc<str>>,
    pub controlled_blocks: Vec<ControlledBlock>,
    pub array_grow_by: u32,
    // NiControllerSequence fields
    pub weight: f32,
    pub text_keys_ref: BlockRef,
    pub cycle_type: u32,
    pub frequency: f32,
    /// Phase offset within the cycle (radians). Present on
    /// v ∈ [10.1.0.106, 10.4.0.1]; defaults to 0 on later content.
    pub phase: f32,
    pub start_time: f32,
    pub stop_time: f32,
    pub manager_ref: BlockRef,
    pub accum_root_name: Option<Arc<str>>,
    pub anim_note_refs: Vec<BlockRef>,
}

impl NiObject for NiControllerSequence {
    fn block_type_name(&self) -> &'static str {
        "NiControllerSequence"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiControllerSequence {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiSequence fields (for v >= 20.1.0.1, string table format)
        let name = stream.read_string()?;
        let num_controlled_blocks = stream.read_u32_le()?;

        // Array Grow By (since 10.1.0.106)
        let array_grow_by = if stream.version() >= NifVersion(0x0A01006A) {
            stream.read_u32_le()?
        } else {
            0
        };

        // ControlledBlock array. The layout of the per-block string
        // fields switches twice across the version range:
        //
        //   v >= 20.1.0.1              → modern string-table format
        //                                (FNV, Skyrim, FO4+)
        //   10.2.0.0 <= v <= 20.1.0.0  → string-palette format
        //                                (Oblivion, pre-FNV Bethesda)
        //                                BlockRef + 5 × u32 offsets
        //   v < 10.2.0.0               → inline strings (Morrowind
        //                                BBBB era, handled by
        //                                read_string's pre-20.1 branch)
        //
        // The old code unconditionally called read_string() even on the
        // Oblivion path, where that helper reads a u32 length prefix
        // followed by bytes. Against real Oblivion .kf files, the first
        // u32 is actually a palette offset (typically a small value like
        // 0x00000006), which read_string happily treated as a 6-byte
        // inline string and then went 5 more bytes past the descriptor,
        // corrupting the stream for every subsequent block. See #107.
        let bsver = stream.bsver();
        let uses_string_palette =
            stream.version() >= NifVersion(0x0A020000) && stream.version() < NifVersion(0x14010001);
        let mut controlled_blocks = stream.allocate_vec(num_controlled_blocks)?;
        for _ in 0..num_controlled_blocks {
            let interpolator_ref = stream.read_block_ref()?;
            let controller_ref = stream.read_block_ref()?;
            // Priority byte (BSVER > 0, i.e. any Bethesda game)
            let priority = if bsver > 0 { stream.read_u8()? } else { 0 };

            if uses_string_palette {
                // Oblivion-era: palette ref + 5 byte offsets.
                let string_palette_ref = stream.read_block_ref()?;
                let node_name_offset = stream.read_u32_le()?;
                let property_type_offset = stream.read_u32_le()?;
                let controller_type_offset = stream.read_u32_le()?;
                let controller_id_offset = stream.read_u32_le()?;
                let interpolator_id_offset = stream.read_u32_le()?;
                controlled_blocks.push(ControlledBlock {
                    interpolator_ref,
                    controller_ref,
                    priority,
                    node_name: None,
                    property_type: None,
                    controller_type: None,
                    controller_id: None,
                    interpolator_id: None,
                    string_palette_ref,
                    node_name_offset,
                    property_type_offset,
                    controller_type_offset,
                    controller_id_offset,
                    interpolator_id_offset,
                });
            } else {
                // Modern string-table (or pre-10.2 inline) format.
                let node_name = stream.read_string()?;
                let property_type = stream.read_string()?;
                let controller_type = stream.read_string()?;
                let controller_id = stream.read_string()?;
                let interpolator_id = stream.read_string()?;
                controlled_blocks.push(ControlledBlock {
                    interpolator_ref,
                    controller_ref,
                    priority,
                    node_name,
                    property_type,
                    controller_type,
                    controller_id,
                    interpolator_id,
                    string_palette_ref: BlockRef::NULL,
                    node_name_offset: 0,
                    property_type_offset: 0,
                    controller_type_offset: 0,
                    controller_id_offset: 0,
                    interpolator_id_offset: 0,
                });
            }
        }

        // NiControllerSequence fields
        let weight = stream.read_f32_le()?;
        let text_keys_ref = stream.read_block_ref()?;
        let cycle_type = stream.read_u32_le()?;
        let frequency = stream.read_f32_le()?;

        // Phase — only present in v ∈ [10.1.0.106, 10.4.0.1]. nif.xml:
        //   <field name="Phase" type="float" since="10.1.0.106"
        //          until="10.4.0.1" />
        // Skipping it on pre-Oblivion content (e.g. Oblivion's
        // v=10.2.0.0 / bsver=9 ships in `meshes/dungeons/ayleidruins/
        // interior/traps/artrapchannelspikes01.nif`) misaligned
        // start_time/stop_time/manager_ref by 4 bytes, then read
        // `accum_root_name`'s u32 length from the stop_time slot.
        // The downstream block read mid-string and the file truncated
        // after kept block 8 with 233 dropped (audit O5-2 / #687).
        let phase = if stream.version() >= NifVersion(0x0A01006A)
            && stream.version() <= NifVersion(0x0A040001)
        {
            stream.read_f32_le()?
        } else {
            0.0
        };

        let start_time = stream.read_f32_le()?;
        let stop_time = stream.read_f32_le()?;

        // Play Backwards — exactly v=10.1.0.106. None of our targets
        // ship content at that exact version (Oblivion is 20.0.0.x,
        // pre-Oblivion sample files we've seen are 10.2.0.0), so this
        // is a no-op today; left in for completeness against nif.xml.
        if stream.version() == NifVersion(0x0A01006A) {
            let _play_backwards = stream.read_u8()?;
        }

        let manager_ref = stream.read_block_ref()?;
        let accum_root_name = stream.read_string()?;

        // Deprecated string-palette link (Gamebryo 2.3
        // `NiControllerSequence::LoadBinary`, v ∈ [10.1.0.113, 20.1.0.1)):
        // a trailing Ref<NiStringPalette> that was kept so the conversion
        // code could resolve the IDTag handle offsets into real strings
        // when loading older content. Oblivion (20.0.0.4 / 20.0.0.5) sits
        // in that window; skipping this field left a 4-byte drift that
        // mis-started every block after block 0 in every Oblivion KF —
        // `NiTransformInterpolator` and `NiStringPalette` then read
        // garbage counts and aborted the parse, so `import_kf` returned
        // zero clips on all 1843 Oblivion KF files. FO3/FNV (v20.0.0.5+
        // with BSVER >= 24) use the modern string-table layout and
        // skip this field. See #402 (audit premise was wrong — Oblivion
        // uses NiControllerSequence, not NiSequenceStreamHelper).
        if stream.version() >= NifVersion(0x0A010071)
            && stream.version() < NifVersion(0x14010001)
        {
            let _deprecated_string_palette_ref = stream.read_block_ref()?;
        }

        // Anim notes — layout diverges by BSVER (#432):
        //   FO3/FNV (BSVER 24–28):  single Ref<BSAnimNotes>
        //   Skyrim+ (BSVER > 28):   u16 count + Vec<Ref<BSAnimNotes>>
        // Normalise both into the same Vec so downstream consumers only
        // see one shape. Older BSVERs (< 24) carry no anim notes at all.
        let anim_note_refs = if bsver > 28 {
            let num = stream.read_u16_le()? as u32;
            let mut refs = stream.allocate_vec(num)?;
            for _ in 0..num {
                refs.push(stream.read_block_ref()?);
            }
            refs
        } else if (24..=28).contains(&bsver) {
            vec![stream.read_block_ref()?]
        } else {
            Vec::new()
        };

        Ok(Self {
            name,
            controlled_blocks,
            array_grow_by,
            weight,
            text_keys_ref,
            cycle_type,
            frequency,
            phase,
            start_time,
            stop_time,
            manager_ref,
            accum_root_name,
            anim_note_refs,
        })
    }
}

// ── BSRefractionFirePeriodController ──────────────────────────────────
// Inherits NiTimeController (not NiSingleInterpController).
// Adds one explicit Ref<NiInterpolator> field per nif.xml line 6832.
// Versions: FO3 (v20.2.0.7 / bsver 21).

/// Animates the fire-period of refraction shader effects (FO3).
#[derive(Debug)]
pub struct BsRefractionFirePeriodController {
    pub base: NiTimeControllerBase,
    pub interpolator_ref: BlockRef,
}

impl NiObject for BsRefractionFirePeriodController {
    fn block_type_name(&self) -> &'static str {
        "BSRefractionFirePeriodController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsRefractionFirePeriodController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        let interpolator_ref = stream.read_block_ref()?;
        Ok(Self {
            base,
            interpolator_ref,
        })
    }
}

#[cfg(test)]
#[path = "controller_tests.rs"]
mod tests;

// ── NiGeomMorpherController ──────────────────────────────────────────

/// Morph target controller — drives facial animation and mesh deformation.
///
/// References NiMorphData (vertex deltas per morph target) and an array
/// of interpolators that control the blend weights over time.
#[derive(Debug)]
pub struct NiGeomMorpherController {
    pub base: NiTimeControllerBase,
    pub morpher_flags: u16,
    pub data_ref: BlockRef,
    pub always_update: u8,
    pub interpolator_weights: Vec<MorphWeight>,
}

/// An interpolator reference + weight for morph blending.
#[derive(Debug)]
pub struct MorphWeight {
    pub interpolator_ref: BlockRef,
    pub weight: f32,
}

impl NiObject for NiGeomMorpherController {
    fn block_type_name(&self) -> &'static str {
        "NiGeomMorpherController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiGeomMorpherController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        let morpher_flags = stream.read_u16_le()?;
        let data_ref = stream.read_block_ref()?;
        let always_update = stream.read_u8()?;
        let num_interpolators = stream.read_u32_le()?;

        let mut interpolator_weights = stream.allocate_vec(num_interpolators)?;
        for _ in 0..num_interpolators {
            let interpolator_ref = stream.read_block_ref()?;
            let weight = stream.read_f32_le()?;
            interpolator_weights.push(MorphWeight {
                interpolator_ref,
                weight,
            });
        }

        // Trailing Num Unknown Ints + Unknown Ints array. nif.xml:
        //   <field name="Num Unknown Ints" type="uint"
        //          since="10.2.0.0" until="20.1.0.3"
        //          vercond="(#BSVER# #LE# 11) #AND# (#BSVER# #NE# 0)" />
        //   <field name="Unknown Ints" type="uint"
        //          length="Num Unknown Ints" since="10.2.0.0" until="20.1.0.3"
        //          vercond="(#BSVER# #LE# 11) #AND# (#BSVER# #NE# 0)" />
        // Targets Bethesda content with bsver in 1..=11 — Oblivion
        // (bsver 11) hits this; FNV/FO3 (bsver 24+) and Skyrim+ skip
        // it entirely. Pre-fix the 4-byte u32 (typically 0) was left
        // unread, which misaligned the next block. On `meshes/oblivion/
        // gate/obgatemini01.nif` the trailing bytes were `0x00000000`,
        // so the next block (NiMorphData) read num_morphs from the
        // wrong slot, parsed as a 9-byte stub, and downstream
        // interpolator blocks tripped the alloc cap with billions of
        // ghost morph keys (audit O5-2 / #687).
        let version = stream.version();
        let bsver = stream.bsver();
        if version >= NifVersion(0x0A020000)
            && version <= NifVersion(0x14010003)
            && bsver != 0
            && bsver <= 11
        {
            let num_unknown_ints = stream.read_u32_le()?;
            // Sanity bound: `num_unknown_ints` is a count that has
            // never been observed > a handful in practice. A drifted
            // u32 here would otherwise allocate gigabytes; the
            // `allocate_vec` cap also bounds it but a tighter early
            // return makes the failure mode obvious if upstream drift
            // ever puts garbage here.
            if num_unknown_ints > 65_536 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "NiGeomMorpherController: implausible \
                         num_unknown_ints={num_unknown_ints} — \
                         upstream drift (Oblivion bsver={bsver})"
                    ),
                ));
            }
            for _ in 0..num_unknown_ints {
                let _ = stream.read_u32_le()?;
            }
        }

        Ok(Self {
            base,
            morpher_flags,
            data_ref,
            always_update,
            interpolator_weights,
        })
    }
}

// ── NiMorphData ──────────────────────────────────────────────────────

/// A single morph target: name + vertex deltas.
#[derive(Debug)]
pub struct MorphTarget {
    /// Name of this morph frame (e.g., "Blink", "JawOpen").
    pub name: Option<Arc<str>>,
    /// Vertex position deltas (one per mesh vertex).
    pub vectors: Vec<[f32; 3]>,
}

/// Morph target data — vertex deltas for facial animation.
#[derive(Debug)]
pub struct NiMorphData {
    pub num_vertices: u32,
    pub relative_targets: u8,
    pub morphs: Vec<MorphTarget>,
}

impl NiObject for NiMorphData {
    fn block_type_name(&self) -> &'static str {
        "NiMorphData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiMorphData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let num_morphs = stream.read_u32_le()? as usize;
        let num_vertices = stream.read_u32_le()?;
        let relative_targets = stream.read_u8()?;

        // Sanity cap: a real NIF never has more than a few thousand
        // vertices per morph target (the Oblivion face morph data tops
        // out around 1k verts). If we see something absurd, the block
        // has drifted — bail out rather than allocate several GB. The
        // caller's per-block recovery path will seek past the block.
        if num_morphs > 65_536 || num_vertices > 65_536 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "NiMorphData: implausible num_morphs={num_morphs} \
                     num_vertices={num_vertices} — block drifted"
                ),
            ));
        }

        // Morph element layout per nif.xml (see <struct name="Morph">):
        //
        //   since 10.1.0.106:          frame_name: string
        //   until 10.1.0.0:            num_keys: u32
        //                              interpolation: KeyType (u32)
        //                              keys: Key<float>[num_keys]
        //   since 10.1.0.104
        //     until 20.1.0.2
        //     && BSVER < 10:           legacy_weight: f32
        //   (always):                  vectors: Vec3[num_vertices]
        //
        // The "until 10.1.0.0" branch is pre-NetImmerse legacy content
        // — NONE of the games Redux targets (Morrowind 4.0.0.0 included)
        // fall into it, because the type was deprecated well before
        // 10.1. The previous implementation read those fields
        // unconditionally, which walked off the end of a valid Oblivion
        // morph and allocated a ~118 GB vector when a garbage num_keys
        // happened to be a huge number.
        //
        // Oblivion (v20.0.0.5, BSVER in 0..=11) hits the legacy_weight
        // window. FNV / FO3 (BSVER 34) and everything later do not.
        let version = stream.version();
        let bsver = stream.bsver();
        let has_keys = version <= NifVersion(0x0A010000);
        let has_legacy_weight =
            version >= NifVersion(0x0A010068) && version <= NifVersion(0x14010002) && bsver < 10;

        // Already bounded by the 65_536 sanity check above; route
        // through allocate_vec for consistency with #408 sweep.
        let mut morphs = stream.allocate_vec(num_morphs as u32)?;
        for _ in 0..num_morphs {
            // Frame name (string table indexed from 10.1.0.106).
            let name = if version >= NifVersion(0x0A01006A) {
                stream.read_string()?
            } else {
                None
            };

            if has_keys {
                let num_keys = stream.read_u32_le()? as u64;
                let interpolation = stream.read_u32_le()?;
                let key_size: u64 = match interpolation {
                    1 | 5 => 8, // LINEAR / CONSTANT: time(f32) + value(f32)
                    2 => 16,    // QUADRATIC: time + value + fwd + bwd
                    3 => 20,    // TBC: time + value + tension + bias + continuity
                    other => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "NiMorphData: unknown float key interpolation {other} \
                                 with {num_keys} keys — stream position unreliable"
                            ),
                        ));
                    }
                };
                stream.skip(key_size * num_keys)?;
            }

            if has_legacy_weight {
                let _legacy_weight = stream.read_f32_le()?;
            }

            // Vertex deltas — guarded against an absurd num_vertices
            // that would otherwise OOM the process on a corrupt block.
            // The hard cap stays as defensive belt; allocate_vec also
            // bounds against remaining stream bytes (#408).
            stream.allocate_vec::<[f32; 3]>((num_vertices as u32).min(1_000_000))?;
            let points = stream.read_ni_point3_array(num_vertices as usize)?;
            let vectors: Vec<[f32; 3]> = points.into_iter().map(|p| [p.x, p.y, p.z]).collect();

            morphs.push(MorphTarget { name, vectors });
        }

        Ok(Self {
            num_vertices,
            relative_targets,
            morphs,
        })
    }
}

// ── NiSequenceStreamHelper ─────────────────────────────────────────────
//
// Morrowind / NetImmerse-era animation root. Inherits from NiObjectNET
// with no extra fields: the per-bone drivers hang off the controller
// chain (NiKeyframeController instances) and the text keys hang off the
// extra_data list.
//
// Empirically not used by any vanilla content we ship support for: a
// 47,934-NIF sweep across Oblivion + FNV + Skyrim SE meshes BSAs found
// zero references — pinned by `vanilla_archives_have_zero_nisequencestreamhelper`
// in `crates/nif/tests/parse_real_nifs.rs` (see also
// `.claude/issues/689/INVESTIGATION.md`). All vanilla animated content
// uses NiControllerSequence, which the importer already handles via
// `import_kf` Path 2.
//
// Bethesda kept the block parseable in their Gamebryo runtime for
// backwards-compat with Morrowind / very-early mod content, so we mirror
// that — the parser accepts it (no hard-fail on unknown types in
// v20.0.0.5 which has no block_sizes recovery), but the importer has no
// consumer for it. An importer path will be needed when Morrowind ESM
// support lands; estimate is 1-2 days against real Morrowind .kf files.

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

// ── NiUVController ────────────────────────────────────────────────────
//
// DEPRECATED (pre-10.1), REMOVED (20.3). The last Bethesda game that
// ships with NiUVController is Oblivion (v20.0.0.5) — water, fire, and
// banner meshes rely on it to scroll texture coordinates. Inherits
// from NiTimeController with two trailing fields: target_attribute (u16)
// and data ref (NiUVData). See issue #156... wait #154.
//
// The parser is stateless beyond the NiTimeController base; the actual
// keyframe data lives in the referenced NiUVData block. The UV channel
// extractor in anim.rs can pick it up later — parsing is the blocker.

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

// ── NiLookAtController ────────────────────────────────────────────────
// Inherits NiTimeController. DEPRECATED (10.2), REMOVED (20.5) — appears
// in Oblivion/FO3/FNV/Skyrim-LE but never in Skyrim-SE+. Orients a target
// NiNode at a follow target; the engine later replaced this with
// NiLookAtInterpolator on a plain NiTransformController. See #228.

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
        let look_at_flags = if stream.version() >= NifVersion(0x0A010000) {
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

// ── NiPathController ──────────────────────────────────────────────────
// Inherits NiTimeController. DEPRECATED (10.2), REMOVED (20.5) — cutscene
// and environmental animation spline follower. The engine later replaced
// this with NiPathInterpolator on a plain NiTransformController. See #228.

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
        let path_flags = if stream.version() >= NifVersion(0x0A010000) {
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

#[cfg(test)]
#[path = "controller_path_lookat_tests.rs"]
mod path_lookat_tests;
