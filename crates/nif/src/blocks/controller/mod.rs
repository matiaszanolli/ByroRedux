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

mod legacy;
mod morph;
mod sequence;
mod shader;

pub use legacy::{NiLookAtController, NiPathController, NiSequenceStreamHelper, NiUVController};
pub use morph::{MorphTarget, MorphWeight, NiGeomMorpherController, NiMorphData};
pub use sequence::{
    BsRefractionFirePeriodController, ControlledBlock, NiControllerManager, NiControllerSequence,
    NiMultiTargetTransformController,
};
pub use shader::{
    BsShaderController, NiLightColorController, NiLightFloatController, NiMaterialColorController,
    NiTextureTransformController, ShaderControllerKind,
};

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

// ── BSLagBoneController ────────────────────────────────────────────────
//
// Skyrim+ controller that trails a bone behind an actor (cape sway,
// hair drag, dragon-wing physics, banner cloth). nif.xml ships three
// trailing floats after the NiTimeController base:
//
//   Linear Velocity   (f32)  — How long it takes to rotate about an
//                              actor back to the rest position.
//   Linear Rotation   (f32)  — How the bone lags rotation.
//   Maximum Distance  (f32)  — How far the bone will tail an actor.
//
// Pre-#837 the type fell through to the NiTimeController base-only
// stub, so the trailing 12 bytes were eaten by `block_size` recovery
// and 42-78 WARN-level realignment events fired per Skyrim Meshes0
// sweep — drowning out real per-block drift bugs (#838).
#[derive(Debug)]
pub struct BsLagBoneController {
    pub base: NiTimeControllerBase,
    pub linear_velocity: f32,
    pub linear_rotation: f32,
    pub maximum_distance: f32,
}

impl NiObject for BsLagBoneController {
    fn block_type_name(&self) -> &'static str {
        "BSLagBoneController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsLagBoneController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        let linear_velocity = stream.read_f32_le()?;
        let linear_rotation = stream.read_f32_le()?;
        let maximum_distance = stream.read_f32_le()?;
        Ok(Self {
            base,
            linear_velocity,
            linear_rotation,
            maximum_distance,
        })
    }
}

// ── BSProceduralLightningController ────────────────────────────────────
//
// Skyrim+ lightning-effect controller paired with dummy TriShapes to
// generate procedural lightning bolt geometry (storm spells, special
// effects). Per nif.xml: 9 interpolator refs driving generation /
// mutation / subdivision / branch count / branch-count variation /
// length / length-variation / width / arc-offset, then bolt-shape
// scalars (subdivisions u16, num_branches u16, num_branches_var u16,
// 6 floats, 3 byte-bools), then a Shader Property ref.
//
// 73 trailing bytes total (Skyrim version) on top of NiTimeController.
// Pre-#837 these all fell into `block_size` recovery and produced
// per-block WARN noise on the 3 Meshes0 instances per sweep (rare
// content — fewer than BSLagBoneController, but on the same channel).
#[derive(Debug)]
pub struct BsProceduralLightningController {
    pub base: NiTimeControllerBase,
    pub interp_generation: BlockRef,
    pub interp_mutation: BlockRef,
    pub interp_subdivision: BlockRef,
    pub interp_num_branches: BlockRef,
    pub interp_num_branches_var: BlockRef,
    pub interp_length: BlockRef,
    pub interp_length_var: BlockRef,
    pub interp_width: BlockRef,
    pub interp_arc_offset: BlockRef,
    pub subdivisions: u16,
    pub num_branches: u16,
    pub num_branches_variation: u16,
    pub length: f32,
    pub length_variation: f32,
    pub width: f32,
    pub child_width_mult: f32,
    pub arc_offset: f32,
    pub fade_main_bolt: bool,
    pub fade_child_bolts: bool,
    pub animate_arc_offset: bool,
    pub shader_property: BlockRef,
}

impl NiObject for BsProceduralLightningController {
    fn block_type_name(&self) -> &'static str {
        "BSProceduralLightningController"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsProceduralLightningController {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiTimeControllerBase::parse(stream)?;
        let interp_generation = stream.read_block_ref()?;
        let interp_mutation = stream.read_block_ref()?;
        let interp_subdivision = stream.read_block_ref()?;
        let interp_num_branches = stream.read_block_ref()?;
        let interp_num_branches_var = stream.read_block_ref()?;
        let interp_length = stream.read_block_ref()?;
        let interp_length_var = stream.read_block_ref()?;
        let interp_width = stream.read_block_ref()?;
        let interp_arc_offset = stream.read_block_ref()?;
        let subdivisions = stream.read_u16_le()?;
        let num_branches = stream.read_u16_le()?;
        let num_branches_variation = stream.read_u16_le()?;
        let length = stream.read_f32_le()?;
        let length_variation = stream.read_f32_le()?;
        let width = stream.read_f32_le()?;
        let child_width_mult = stream.read_f32_le()?;
        let arc_offset = stream.read_f32_le()?;
        let fade_main_bolt = stream.read_bool()?;
        let fade_child_bolts = stream.read_bool()?;
        let animate_arc_offset = stream.read_bool()?;
        let shader_property = stream.read_block_ref()?;
        Ok(Self {
            base,
            interp_generation,
            interp_mutation,
            interp_subdivision,
            interp_num_branches,
            interp_num_branches_var,
            interp_length,
            interp_length_var,
            interp_width,
            interp_arc_offset,
            subdivisions,
            num_branches,
            num_branches_variation,
            length,
            length_variation,
            width,
            child_width_mult,
            arc_offset,
            fade_main_bolt,
            fade_child_bolts,
            animate_arc_offset,
            shader_property,
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
            let mut shape_groups_1 = stream.allocate_vec::<BoneLodSkinInfoSet>(num_shape_groups)?;
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

// ── NiMaterialColorController ──────────────────────────────────────────
// Inherits NiSingleInterpController, adds: target_color (MaterialColor enum, u16).

// ── NiTextureTransformController ───────────────────────────────────────
// Inherits NiFloatInterpController → NiSingleInterpController, adds:
// shader_map (bool), texture_slot (u32 TexType), operation (u32 TransformMember).

// ── NiMultiTargetTransformController ───────────────────────────────────
// Inherits NiInterpController (which adds nothing for FNV), adds:
// num_extra_targets (u16) + extra_targets (Ptr[]).

// ── NiControllerManager ────────────────────────────────────────────────
// Inherits NiTimeController, adds: cumulative (bool, 1 byte), sequences, palette.

// ── NiControllerSequence ───────────────────────────────────────────────
// Does NOT inherit NiTimeController. Inherits NiSequence → NiObject.

// ── BSRefractionFirePeriodController ──────────────────────────────────
// Inherits NiTimeController (not NiSingleInterpController).
// Adds one explicit Ref<NiInterpolator> field per nif.xml line 6832.
// Versions: FO3 (v20.2.0.7 / bsver 21).

#[cfg(test)]
mod tests;

// ── NiGeomMorpherController ──────────────────────────────────────────

// ── NiMorphData ──────────────────────────────────────────────────────

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

// ── NiLookAtController ────────────────────────────────────────────────
// Inherits NiTimeController. DEPRECATED (10.2), REMOVED (20.5) — appears
// in Oblivion/FO3/FNV/Skyrim-LE but never in Skyrim-SE+. Orients a target
// NiNode at a follow target; the engine later replaced this with
// NiLookAtInterpolator on a plain NiTransformController. See #228.

// ── NiPathController ──────────────────────────────────────────────────
// Inherits NiTimeController. DEPRECATED (10.2), REMOVED (20.5) — cutscene
// and environmental animation spline follower. The engine later replaced
// this with NiPathInterpolator on a plain NiTransformController. See #228.

#[cfg(test)]
mod path_lookat_tests;
