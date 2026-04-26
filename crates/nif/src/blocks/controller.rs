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
mod tests {
    use super::*;
    use crate::header::NifHeader;
    use crate::stream::NifStream;
    use crate::version::NifVersion;

    pub(super) fn make_header_fnv() -> NifHeader {
        NifHeader {
            version: NifVersion::V20_2_0_7,
            little_endian: true,
            user_version: 11,
            user_version_2: 34,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: vec![Arc::from("TestName")],
            max_string_length: 8,
            num_groups: 0,
        }
    }

    pub(super) fn write_time_controller_base(data: &mut Vec<u8>) {
        // next_controller_ref: -1
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // flags: 0x000C
        data.extend_from_slice(&0x000Cu16.to_le_bytes());
        // frequency: 1.0
        data.extend_from_slice(&1.0f32.to_le_bytes());
        // phase: 0.0
        data.extend_from_slice(&0.0f32.to_le_bytes());
        // start_time: 0.0
        data.extend_from_slice(&0.0f32.to_le_bytes());
        // stop_time: 1.0
        data.extend_from_slice(&1.0f32.to_le_bytes());
        // target_ref: 0
        data.extend_from_slice(&0i32.to_le_bytes());
    }

    #[test]
    fn parse_ni_time_controller_base_26_bytes() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        assert_eq!(data.len(), 26);
        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiTimeController::parse(&mut stream).unwrap();
        assert_eq!(stream.position(), 26);
        assert!(ctrl.base.next_controller_ref.is_null());
    }

    #[test]
    fn parse_single_interp_controller_30_bytes() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        // interpolator_ref: 5
        data.extend_from_slice(&5i32.to_le_bytes());
        assert_eq!(data.len(), 30);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiSingleInterpController::parse(&mut stream).unwrap();
        assert_eq!(stream.position(), 30);
        assert_eq!(ctrl.interpolator_ref.index(), Some(5));
    }

    /// Regression for #551 — `bhkBlendController` must parse as
    /// `NiTimeController` base (26 B) + `Keys: uint` (4 B) = 30 B
    /// total per nif.xml line 3927. Pre-fix this block had no dispatch
    /// arm and 1,427 FNV+FO3 vanilla blocks fell into NiUnknown.
    ///
    /// Contrary to the audit's suggestion, this is NOT a
    /// NiSingleInterpController — it inherits NiTimeController directly.
    #[test]
    fn parse_bhk_blend_controller_30_bytes() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        // keys: uint — "Seems to be always zero" per nif.xml, but write
        // a non-zero value so the test would catch a u32 vs i32 mix-up.
        data.extend_from_slice(&0x12345678u32.to_le_bytes());
        assert_eq!(data.len(), 30);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = BhkBlendController::parse(&mut stream).unwrap();
        assert_eq!(
            stream.position(),
            30,
            "bhkBlendController must consume exactly NiTimeController(26) + u32(4) = 30 B"
        );
        assert_eq!(ctrl.keys, 0x12345678);
        assert!(ctrl.base.next_controller_ref.is_null());
    }

    /// Regression for #551 — dispatch must route `bhkBlendController`
    /// through `BhkBlendController::parse`, not the `NiTimeController`
    /// fallback. Verifies the block_type_name() round-trip.
    #[test]
    fn bhk_blend_controller_dispatches_via_parse_block() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.extend_from_slice(&0u32.to_le_bytes()); // keys = 0

        let mut stream = NifStream::new(&data, &header);
        let block = crate::blocks::parse_block(
            "bhkBlendController",
            &mut stream,
            Some(data.len() as u32),
        )
        .expect("dispatch must route bhkBlendController — pre-fix it was NiUnknown");
        assert_eq!(block.block_type_name(), "bhkBlendController");
        assert_eq!(
            stream.position() as usize,
            data.len(),
            "dispatcher must consume the full 30-byte body"
        );
        let ctrl = block
            .as_any()
            .downcast_ref::<BhkBlendController>()
            .expect("dispatch type must be BhkBlendController, not NiTimeController");
        assert_eq!(ctrl.keys, 0);
    }

    /// Regression for #552 — `BSNiAlphaPropertyTestRefController` must
    /// dispatch and parse as `NiSingleInterpController` (nif.xml line
    /// 6279: inherits NiFloatInterpController, no extra fields).
    /// Pre-fix 751 Skyrim SE vanilla blocks fell into NiUnknown.
    /// The newtype wrapper preserves the RTTI name so telemetry and
    /// the future alpha-animation importer can match on it.
    #[test]
    fn bs_ni_alpha_property_test_ref_controller_dispatches() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.extend_from_slice(&7i32.to_le_bytes()); // interpolator_ref = 7
        assert_eq!(data.len(), 30);

        let mut stream = NifStream::new(&data, &header);
        let block = crate::blocks::parse_block(
            "BSNiAlphaPropertyTestRefController",
            &mut stream,
            Some(data.len() as u32),
        )
        .expect("dispatch must route BSNiAlphaPropertyTestRefController");
        assert_eq!(
            stream.position() as usize,
            data.len(),
            "must consume 26 B TimeController base + 4 B interpolator_ref"
        );
        assert_eq!(
            block.block_type_name(),
            "BSNiAlphaPropertyTestRefController",
            "newtype wrapper preserves RTTI for downstream dispatch"
        );
        let ctrl = block
            .as_any()
            .downcast_ref::<BsNiAlphaPropertyTestRefController>()
            .expect("dispatch type must be BsNiAlphaPropertyTestRefController");
        assert_eq!(ctrl.base.interpolator_ref.index(), Some(7));
    }

    /// Regression for #553 — `NiFloatExtraDataController` must parse
    /// as `NiTimeController` base (26 B) + `interpolator_ref` (4 B,
    /// since 10.1.0.104) + `extra_data_name` string index (4 B, since
    /// 10.2.0.0) = 34 B on FO3+/FNV/SE. Pre-fix no dispatch arm
    /// existed.
    #[test]
    fn parse_ni_float_extra_data_controller_34_bytes() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.extend_from_slice(&11i32.to_le_bytes()); // interpolator_ref = 11
        data.extend_from_slice(&0u32.to_le_bytes()); // extra_data_name: string idx 0
        assert_eq!(data.len(), 34);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiFloatExtraDataController::parse(&mut stream)
            .expect("NiFloatExtraDataController must parse at FNV bsver");
        assert_eq!(stream.position(), 34);
        assert_eq!(ctrl.interpolator_ref.index(), Some(11));
        assert_eq!(ctrl.extra_data_name.as_deref(), Some("TestName"));
    }

    /// Regression for #553 — dispatcher must route
    /// `NiFloatExtraDataController` through its own parser, not the
    /// `NiTimeController` fallback stub (which would leave interpolator_ref
    /// and extra_data_name unread and drift subsequent blocks).
    #[test]
    fn ni_float_extra_data_controller_dispatches_via_parse_block() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.extend_from_slice(&5i32.to_le_bytes()); // interpolator_ref
        data.extend_from_slice(&0u32.to_le_bytes()); // extra_data_name idx

        let mut stream = NifStream::new(&data, &header);
        let block = crate::blocks::parse_block(
            "NiFloatExtraDataController",
            &mut stream,
            Some(data.len() as u32),
        )
        .expect("dispatch must route NiFloatExtraDataController");
        assert_eq!(block.block_type_name(), "NiFloatExtraDataController");
        assert_eq!(stream.position() as usize, data.len());
        let ctrl = block
            .as_any()
            .downcast_ref::<NiFloatExtraDataController>()
            .expect("dispatch type must be NiFloatExtraDataController");
        assert_eq!(ctrl.interpolator_ref.index(), Some(5));
    }

    /// Regression for #433 — `NiLightColorController` must parse as
    /// `NiTimeController` base (26 B) + `interpolator_ref` (4 B, since
    /// 10.1.0.104) + `target_color: u16` (since 10.1.0.0) = 32 B.
    /// Pre-fix the block had no dispatch arm — every animated light
    /// color (lantern ambient shift, magic-spell glow color cycling)
    /// landed as NiUnknown and silently stopped animating.
    #[test]
    fn parse_ni_light_color_controller_32_bytes() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.extend_from_slice(&9i32.to_le_bytes()); // interpolator_ref = 9
        // target_color: 1 = Ambient (LightColor enum nif.xml line 1241).
        data.extend_from_slice(&1u16.to_le_bytes());
        assert_eq!(data.len(), 32);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiLightColorController::parse(&mut stream)
            .expect("NiLightColorController must parse at FNV bsver");
        assert_eq!(stream.position(), 32);
        assert_eq!(ctrl.interpolator_ref.index(), Some(9));
        assert_eq!(
            ctrl.target_color, 1,
            "target_color = 1 (Ambient) — pre-fix this field was never \
             read and block-size recovery silently elided it"
        );
    }

    /// Regression for #433 — the three plain `NiFloatInterpController`
    /// subclasses (NiLightDimmerController, NiLightIntensityController,
    /// NiLightRadiusController) share the 30-byte `NiSingleInterpController`
    /// layout with no additional fields (nif.xml lines 3750 / 5025 / 8444).
    /// Dispatcher routes them through `NiLightFloatController::parse` so
    /// `block_type_name()` reports the original subclass.
    #[test]
    fn ni_light_float_controller_dispatches_preserving_rtti() {
        for type_name in [
            "NiLightDimmerController",
            "NiLightIntensityController",
            "NiLightRadiusController",
        ] {
            let header = make_header_fnv();
            let mut data = Vec::new();
            write_time_controller_base(&mut data);
            data.extend_from_slice(&7i32.to_le_bytes()); // interpolator_ref

            let mut stream = NifStream::new(&data, &header);
            let block = crate::blocks::parse_block(
                type_name,
                &mut stream,
                Some(data.len() as u32),
            )
            .unwrap_or_else(|e| panic!("{type_name} dispatch failed: {e}"));
            assert_eq!(
                stream.position() as usize,
                data.len(),
                "{type_name} must consume the 30-byte NiSingleInterpController body"
            );
            assert_eq!(
                block.block_type_name(),
                type_name,
                "NiLightFloatController must preserve RTTI via its type_name field"
            );
            let ctrl = block
                .as_any()
                .downcast_ref::<NiLightFloatController>()
                .expect("dispatch type must be NiLightFloatController");
            assert_eq!(ctrl.base.interpolator_ref.index(), Some(7));
        }
    }

    #[test]
    fn parse_bs_refraction_fire_period_controller_30_bytes() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        // interpolator_ref: 3
        data.extend_from_slice(&3i32.to_le_bytes());
        assert_eq!(data.len(), 30);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = BsRefractionFirePeriodController::parse(&mut stream).unwrap();
        assert_eq!(stream.position(), 30);
        assert_eq!(ctrl.interpolator_ref.index(), Some(3));
        assert!(ctrl.base.next_controller_ref.is_null());
    }

    #[test]
    fn parse_material_color_controller_32_bytes() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.extend_from_slice(&3i32.to_le_bytes()); // interpolator_ref
        data.extend_from_slice(&1u16.to_le_bytes()); // target_color
        assert_eq!(data.len(), 32);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiMaterialColorController::parse(&mut stream).unwrap();
        assert_eq!(stream.position(), 32);
        assert_eq!(ctrl.target_color, 1);
    }

    #[test]
    fn parse_multi_target_transform_controller() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        // num_extra_targets: 4
        data.extend_from_slice(&4u16.to_le_bytes());
        // 4 target refs
        for i in 0..4 {
            data.extend_from_slice(&(i as i32).to_le_bytes());
        }
        assert_eq!(data.len(), 44);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiMultiTargetTransformController::parse(&mut stream).unwrap();
        assert_eq!(stream.position(), 44);
        assert_eq!(ctrl.extra_targets.len(), 4);
    }

    #[test]
    fn parse_controller_manager_1_sequence() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.push(1); // cumulative = true (byte bool)
        data.extend_from_slice(&1u32.to_le_bytes()); // num_sequences
        data.extend_from_slice(&7i32.to_le_bytes()); // sequence_refs[0]
        data.extend_from_slice(&8i32.to_le_bytes()); // object_palette_ref
        assert_eq!(data.len(), 39);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiControllerManager::parse(&mut stream).unwrap();
        assert_eq!(stream.position(), 39);
        assert!(ctrl.cumulative);
        assert_eq!(ctrl.sequence_refs.len(), 1);
        assert_eq!(ctrl.sequence_refs[0].index(), Some(7));
        assert_eq!(ctrl.object_palette_ref.index(), Some(8));
    }

    /// Regression: #350 / S5-02. Every BSShaderProperty*Controller
    /// block carries a trailing u32 enum identifying the driven slot.
    /// Pre-fix the dispatch discarded the value (`_controlled_variable`)
    /// and emitted `Box<NiSingleInterpController>`, so the animation
    /// importer had no way to learn which shader uniform to drive. The
    /// typed `BsShaderController` now preserves the enum in
    /// `ShaderControllerKind` and reports its original RTTI name.
    #[test]
    fn parse_bs_shader_controller_preserves_controlled_variable() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data); // 26 bytes
                                                 // NiSingleInterpController: interpolator_ref (since 10.1.0.104,
                                                 // FNV v=20.2.0.7 is above that).
        data.extend_from_slice(&5i32.to_le_bytes()); // interpolator_ref
                                                      // BSShaderController trailing enum.
        data.extend_from_slice(&3u32.to_le_bytes()); // controlled_variable = 3
        assert_eq!(data.len(), 34);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = BsShaderController::parse(&mut stream, "BSEffectShaderPropertyFloatController")
            .expect("shader controller with 4-byte enum tail must parse");
        assert_eq!(stream.position() as usize, data.len());
        assert_eq!(ctrl.type_name, "BSEffectShaderPropertyFloatController");
        assert_eq!(ctrl.base.interpolator_ref.index(), Some(5));
        assert_eq!(ctrl.kind, ShaderControllerKind::EffectFloat(3));
    }

    /// Each of the five controller type names must map to its own
    /// `ShaderControllerKind` variant so downstream dispatch can match
    /// on the kind rather than re-parsing the type string. Verifies the
    /// u32 payload rides through identically on all five.
    #[test]
    fn parse_bs_shader_controller_dispatches_all_five_kinds() {
        let header = make_header_fnv();
        for (type_name, expected) in [
            (
                "BSEffectShaderPropertyFloatController",
                ShaderControllerKind::EffectFloat(7),
            ),
            (
                "BSEffectShaderPropertyColorController",
                ShaderControllerKind::EffectColor(7),
            ),
            (
                "BSLightingShaderPropertyFloatController",
                ShaderControllerKind::LightingFloat(7),
            ),
            (
                "BSLightingShaderPropertyColorController",
                ShaderControllerKind::LightingColor(7),
            ),
            (
                "BSLightingShaderPropertyUShortController",
                ShaderControllerKind::LightingUShort(7),
            ),
        ] {
            let mut data = Vec::new();
            write_time_controller_base(&mut data);
            data.extend_from_slice(&0i32.to_le_bytes()); // interpolator_ref
            data.extend_from_slice(&7u32.to_le_bytes()); // controlled_variable

            let mut stream = NifStream::new(&data, &header);
            let ctrl = BsShaderController::parse(&mut stream, type_name).unwrap_or_else(|e| {
                panic!("{type_name} should parse: {e}");
            });
            assert_eq!(
                stream.position() as usize,
                data.len(),
                "{type_name} must consume all 34 bytes"
            );
            assert_eq!(ctrl.kind, expected, "{type_name} dispatched to wrong kind");
            assert_eq!(ctrl.type_name, type_name);
        }
    }

    #[test]
    fn parse_controller_sequence_no_blocks() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        // NiSequence: name (string table index 0)
        data.extend_from_slice(&0i32.to_le_bytes());
        // num_controlled_blocks: 0
        data.extend_from_slice(&0u32.to_le_bytes());
        // array_grow_by: 0
        data.extend_from_slice(&0u32.to_le_bytes());
        // NiControllerSequence fields:
        data.extend_from_slice(&1.0f32.to_le_bytes()); // weight
        data.extend_from_slice(&(-1i32).to_le_bytes()); // text_keys_ref
        data.extend_from_slice(&0u32.to_le_bytes()); // cycle_type
        data.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
        data.extend_from_slice(&0.0f32.to_le_bytes()); // start_time
        data.extend_from_slice(&1.0f32.to_le_bytes()); // stop_time
        data.extend_from_slice(&(-1i32).to_le_bytes()); // manager_ref
        data.extend_from_slice(&(-1i32).to_le_bytes()); // accum_root_name
                                                        // anim note arrays (BSVER > 28 = yes for FNV)
        data.extend_from_slice(&0u16.to_le_bytes()); // num_anim_note_arrays
        let expected_len = data.len();

        let mut stream = NifStream::new(&data, &header);
        let seq = NiControllerSequence::parse(&mut stream).unwrap();
        assert_eq!(stream.position() as usize, expected_len);
        assert_eq!(seq.name.as_deref(), Some("TestName"));
        assert_eq!(seq.controlled_blocks.len(), 0);
        assert!(seq.text_keys_ref.is_null());
    }

    /// Build an Oblivion-era header (v20.0.0.5, user_version=11, uv2=11).
    /// String table is empty — Oblivion doesn't use it, and per-block
    /// strings go through the NiStringPalette format instead.
    pub(super) fn make_header_oblivion() -> NifHeader {
        NifHeader {
            version: NifVersion::V20_0_0_5,
            little_endian: true,
            user_version: 11,
            user_version_2: 11,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        }
    }

    /// Regression test for issue #107: Oblivion .kf files encode the
    /// ControlledBlock string fields via a NiStringPalette block ref +
    /// five byte offsets (since 10.2.0.0, until 20.1.0.0). The old
    /// parser called `read_string` unconditionally and mis-parsed the
    /// first u32 offset as a string length, shifting the stream and
    /// cascading into corrupted downstream blocks. The fix switches to
    /// a version branch; this test pins the Oblivion path.
    #[test]
    fn parse_controller_sequence_oblivion_string_palette_format() {
        let header = make_header_oblivion();
        let mut data = Vec::new();

        // NiSequence pre-10.1 string encoding: `read_string` returns
        // Ok(None) on len=0, so a 4-byte zero-length acts as an empty
        // "name" header field.
        data.extend_from_slice(&0u32.to_le_bytes()); // name: empty inline string
        data.extend_from_slice(&1u32.to_le_bytes()); // num_controlled_blocks
        data.extend_from_slice(&0u32.to_le_bytes()); // array_grow_by

        // One ControlledBlock in Oblivion palette format:
        //   interpolator_ref (i32)
        //   controller_ref   (i32)
        //   priority         (u8)          — bsver=11 > 0, so present
        //   string_palette_ref (i32)
        //   node_name_offset        (u32)
        //   property_type_offset    (u32)
        //   controller_type_offset  (u32)
        //   controller_id_offset    (u32)
        //   interpolator_id_offset  (u32)
        data.extend_from_slice(&12i32.to_le_bytes()); // interpolator_ref
        data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
        data.push(42); // priority
        data.extend_from_slice(&9i32.to_le_bytes()); // string_palette_ref
        data.extend_from_slice(&0u32.to_le_bytes()); // node_name_offset
        data.extend_from_slice(&6u32.to_le_bytes()); // property_type_offset
        data.extend_from_slice(&11u32.to_le_bytes()); // controller_type_offset
        data.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // controller_id_offset (unset sentinel)
        data.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // interpolator_id_offset

        // NiControllerSequence trailer (same on all post-10.1 paths).
        data.extend_from_slice(&1.0f32.to_le_bytes()); // weight
        data.extend_from_slice(&(-1i32).to_le_bytes()); // text_keys_ref
        data.extend_from_slice(&0u32.to_le_bytes()); // cycle_type
        data.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
        data.extend_from_slice(&0.0f32.to_le_bytes()); // start_time
        data.extend_from_slice(&1.0f32.to_le_bytes()); // stop_time
        data.extend_from_slice(&(-1i32).to_le_bytes()); // manager_ref
        data.extend_from_slice(&0u32.to_le_bytes()); // accum_root_name: empty inline
        // #402 — Oblivion (v ∈ [10.1.0.113, 20.1.0.1)) trails a
        // Ref<NiStringPalette>. Gamebryo 2.3's LoadBinary reads this so
        // the legacy IDTag palette offsets can be converted to
        // NiFixedStrings during link; on-disk it sits between
        // accum_root_name and the anim-note block.
        data.extend_from_slice(&9i32.to_le_bytes()); // deprecated string palette ref

        // Oblivion bsver=11, 11 <= 28 → no anim note list, so don't
        // append anything here.

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let seq = NiControllerSequence::parse(&mut stream)
            .expect("Oblivion NiControllerSequence must parse the palette format");
        assert_eq!(
            stream.position() as usize,
            expected_len,
            "Oblivion parse consumed {} bytes, expected {}",
            stream.position(),
            expected_len,
        );

        assert_eq!(seq.controlled_blocks.len(), 1);
        let cb = &seq.controlled_blocks[0];
        assert_eq!(cb.interpolator_ref.index(), Some(12));
        assert!(cb.controller_ref.is_null());
        assert_eq!(cb.priority, 42);
        // Palette fields must be populated, name fields left None.
        assert_eq!(cb.string_palette_ref.index(), Some(9));
        assert_eq!(cb.node_name_offset, 0);
        assert_eq!(cb.property_type_offset, 6);
        assert_eq!(cb.controller_type_offset, 11);
        assert_eq!(cb.controller_id_offset, 0xFFFF_FFFF);
        assert_eq!(cb.interpolator_id_offset, 0xFFFF_FFFF);
        assert!(cb.node_name.is_none());
        assert!(cb.property_type.is_none());
    }
}

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
mod path_lookat_tests {
    use super::tests::*;
    use super::*;

    #[test]
    fn parse_look_at_controller_32_bytes() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        // look_at_flags = LOOK_Y_AXIS (bit 1)
        data.extend_from_slice(&0x0002u16.to_le_bytes());
        // look_at_ref = 7
        data.extend_from_slice(&7i32.to_le_bytes());
        assert_eq!(data.len(), 32);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiLookAtController::parse(&mut stream).unwrap();
        assert_eq!(stream.position(), 32);
        assert_eq!(ctrl.look_at_flags, 0x0002);
        assert_eq!(ctrl.look_at_ref.index(), Some(7));
        assert!(ctrl.base.next_controller_ref.is_null());
    }

    #[test]
    fn parse_path_controller_48_bytes() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        // path_flags
        data.extend_from_slice(&0x0000u16.to_le_bytes());
        // bank_dir = 1 (positive)
        data.extend_from_slice(&1i32.to_le_bytes());
        // max_bank_angle = 0.5 rad
        data.extend_from_slice(&0.5f32.to_le_bytes());
        // smoothing = 0.25
        data.extend_from_slice(&0.25f32.to_le_bytes());
        // follow_axis = 1 (Y)
        data.extend_from_slice(&1i16.to_le_bytes());
        // path_data_ref = 11
        data.extend_from_slice(&11i32.to_le_bytes());
        // percent_data_ref = 12
        data.extend_from_slice(&12i32.to_le_bytes());
        // 26 (base) + 2 + 4 + 4 + 4 + 2 + 4 + 4 = 50
        assert_eq!(data.len(), 50);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiPathController::parse(&mut stream).unwrap();
        assert_eq!(stream.position(), 50);
        assert_eq!(ctrl.path_flags, 0);
        assert_eq!(ctrl.bank_dir, 1);
        assert_eq!(ctrl.max_bank_angle, 0.5);
        assert_eq!(ctrl.smoothing, 0.25);
        assert_eq!(ctrl.follow_axis, 1);
        assert_eq!(ctrl.path_data_ref.index(), Some(11));
        assert_eq!(ctrl.percent_data_ref.index(), Some(12));
    }

    #[test]
    fn dispatch_routes_path_and_look_at_controllers() {
        use crate::blocks::parse_block;
        let header = make_header_fnv();

        // ── NiLookAtController ───────────
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.extend_from_slice(&0x0004u16.to_le_bytes()); // LOOK_Z_AXIS
        data.extend_from_slice(&3i32.to_le_bytes());
        let size = data.len() as u32;
        let mut stream = NifStream::new(&data, &header);
        let block = parse_block("NiLookAtController", &mut stream, Some(size))
            .expect("NiLookAtController dispatch");
        let c = block.as_any().downcast_ref::<NiLookAtController>().unwrap();
        assert_eq!(c.look_at_flags, 0x0004);
        assert_eq!(c.look_at_ref.index(), Some(3));

        // ── NiPathController ─────────────
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.extend_from_slice(&0x0000u16.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes()); // bank_dir = Negative
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&0.1f32.to_le_bytes());
        data.extend_from_slice(&2i16.to_le_bytes()); // Z
        data.extend_from_slice(&5i32.to_le_bytes());
        data.extend_from_slice(&6i32.to_le_bytes());
        let size = data.len() as u32;
        let mut stream = NifStream::new(&data, &header);
        let block = parse_block("NiPathController", &mut stream, Some(size))
            .expect("NiPathController dispatch");
        let c = block.as_any().downcast_ref::<NiPathController>().unwrap();
        assert_eq!(c.bank_dir, -1);
        assert_eq!(c.follow_axis, 2);
        assert_eq!(c.path_data_ref.index(), Some(5));
        assert_eq!(c.percent_data_ref.index(), Some(6));
    }

    // ── #687 regression guards ────────────────────────────────────────
    //
    // Both perpetrators identified by tracing audit-O5-2 example files
    // — `obgatemini01.nif` (NiGeomMorpherController missing trailing
    // bsver-gated u32 array) and `artrapchannelspikes01.nif`
    // (NiControllerSequence missing the v∈[10.1.0.106,10.4.0.1]
    // `Phase` field). The fix recovered 83 of the 384 truncated
    // Oblivion files (95.21% → 96.24% clean).

    use crate::header::NifHeader;

    fn make_header_pre_oblivion_v10_2() -> NifHeader {
        // Pre-Gamebryo content shipped in Oblivion's BSA — v=10.2.0.0
        // bsver=9 hits the `Phase` window in NiControllerSequence.
        NifHeader {
            version: NifVersion(0x0A020000),
            little_endian: true,
            user_version: 10,
            user_version_2: 9,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        }
    }

    #[test]
    fn nigeommorpher_oblivion_consumes_trailing_unknown_ints() {
        // Layout for v=20.0.0.5 / bsver=11:
        //   NiTimeControllerBase (26 B)
        //   morpher_flags u16 (2 B) + data_ref i32 (4 B) +
        //   always_update u8 (1 B) + num_interpolators u32 (4 B) = 11 B
        //   no interpolator weights for this test (num=0)
        //   trailing num_unknown_ints u32 (4 B) — array empty
        let header = make_header_oblivion();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.extend_from_slice(&0u16.to_le_bytes()); // morpher_flags
        data.extend_from_slice(&(-1i32).to_le_bytes()); // data_ref null
        data.push(1); // always_update
        data.extend_from_slice(&0u32.to_le_bytes()); // num_interpolators
        data.extend_from_slice(&0u32.to_le_bytes()); // num_unknown_ints (TRAILING)
        assert_eq!(data.len(), 26 + 11 + 4);

        let mut stream = NifStream::new(&data, &header);
        let _block = NiGeomMorpherController::parse(&mut stream)
            .expect("Oblivion NiGeomMorpherController parses with trailing field");
        assert_eq!(
            stream.position(),
            data.len() as u64,
            "must consume the full Oblivion-trailing layout, not stop at the \
             interpolator-weights end (pre-fix #687 stopped 4 bytes early, \
             cascading drift into NiMorphData)"
        );
    }

    #[test]
    fn nigeommorpher_fnv_skips_trailing_unknown_ints() {
        // FNV bsver=34 — the (BSVER <= 11) gate excludes the trailing
        // u32. Confirms the fix is Oblivion-only and doesn't regress
        // FNV/FO3 (clean rate must remain 100%).
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.push(1);
        data.extend_from_slice(&0u32.to_le_bytes());
        // No trailing field — FNV layout ends here.
        let original_len = data.len();
        // Pad with 4 sentinel bytes that MUST NOT be consumed.
        data.extend_from_slice(&0xDEADBEEFu32.to_le_bytes());

        let mut stream = NifStream::new(&data, &header);
        NiGeomMorpherController::parse(&mut stream).expect("FNV path parses");
        assert_eq!(
            stream.position(),
            original_len as u64,
            "FNV (bsver=34) must NOT read the bsver<=11-gated trailing field \
             — over-consuming would shift downstream blocks"
        );
    }

    #[test]
    fn nicontrollersequence_v10_2_reads_phase() {
        // Pre-Oblivion v=10.2.0.0 content. Layout for the trailing
        // fields: weight + text_keys + cycle_type + frequency +
        // **phase** (here) + start_time + stop_time + manager +
        // accum_root_name + deprecated_string_palette_ref.
        //
        // Pre-fix #687 the parser jumped from `frequency` straight
        // to `start_time`, reading the on-disk `phase` slot as
        // `start_time` and shifting every later field by 4 bytes.
        // accum_root_name's u32 length was then read from
        // stop_time, decoding the first 3 chars of the real
        // accum_root_name and bleeding the rest into the next block.
        let header = make_header_pre_oblivion_v10_2();
        let mut data = Vec::new();
        // name (empty inline)
        data.extend_from_slice(&0u32.to_le_bytes());
        // num_controlled_blocks = 0
        data.extend_from_slice(&0u32.to_le_bytes());
        // array_grow_by (since 10.1.0.106) = 1
        data.extend_from_slice(&1u32.to_le_bytes());
        // weight=1.0, text_keys=null, cycle_type=2 (LOOP), frequency=1.0
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.extend_from_slice(&2u32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        // phase=0.5 — distinctive sentinel
        data.extend_from_slice(&0.5f32.to_le_bytes());
        // start_time=0.0, stop_time=7.36
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&7.36f32.to_le_bytes());
        // manager_ref=3
        data.extend_from_slice(&3u32.to_le_bytes());
        // accum_root_name = "Root" (4 chars)
        data.extend_from_slice(&4u32.to_le_bytes());
        data.extend_from_slice(b"Root");
        // deprecated_string_palette_ref (since 10.1.0.113) = -1
        data.extend_from_slice(&(-1i32).to_le_bytes());

        let mut stream = NifStream::new(&data, &header);
        let seq = NiControllerSequence::parse(&mut stream)
            .expect("v=10.2.0.0 NiControllerSequence parses with phase");
        assert_eq!(
            stream.position(),
            data.len() as u64,
            "must consume the full v=10.2.0.0 layout including the Phase field"
        );
        assert!(
            (seq.phase - 0.5).abs() < 1e-6,
            "phase routes to its own struct field, not start_time"
        );
        assert_eq!(seq.start_time, 0.0, "start_time stays at 0 (not the phase value)");
        assert!(
            (seq.stop_time - 7.36).abs() < 1e-6,
            "stop_time follows phase, not the manager_ref slot"
        );
        assert_eq!(
            seq.accum_root_name.as_deref(),
            Some("Root"),
            "accum_root_name reads its own string, not part of stop_time"
        );
    }

    #[test]
    fn nicontrollersequence_oblivion_skips_phase() {
        // Oblivion v=20.0.0.5 is past the Phase window's `until="10.4.0.1"`.
        // Layout has no phase field — confirming the fix doesn't
        // over-consume on Oblivion's NiControllerSequence (which is the
        // primary KF-file consumer and was previously working).
        let header = make_header_oblivion();
        let mut data = Vec::new();
        // name empty + num_controlled=0 + array_grow_by=1
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        // weight + text_keys + cycle_type + frequency
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        // (no phase on Oblivion)
        data.extend_from_slice(&0.0f32.to_le_bytes()); // start_time
        data.extend_from_slice(&1.0f32.to_le_bytes()); // stop_time
        data.extend_from_slice(&(-1i32).to_le_bytes()); // manager
        data.extend_from_slice(&0u32.to_le_bytes()); // accum_root_name empty
        // deprecated_string_palette_ref (within the [10.1.0.113, 20.1.0.1) window)
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // anim notes: bsver=11 — `(24..=28).contains(&bsver)` false,
        // bsver > 28 false → empty Vec (no bytes read).

        let original_len = data.len();
        // Sentinel that MUST NOT be consumed — over-consuming would
        // mean the Oblivion path is reading a phase field it shouldn't.
        data.extend_from_slice(&0xDEADBEEFu32.to_le_bytes());

        let mut stream = NifStream::new(&data, &header);
        let seq = NiControllerSequence::parse(&mut stream)
            .expect("Oblivion NiControllerSequence parses without phase");
        assert_eq!(
            stream.position(),
            original_len as u64,
            "Oblivion (v=20.0.0.5) must NOT read Phase — that field is \
             gated to v ≤ 10.4.0.1"
        );
        assert_eq!(seq.phase, 0.0, "phase defaults to 0 outside the gated window");
    }
}
