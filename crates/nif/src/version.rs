//! NIF file format version handling.
//!
//! Gamebryo encodes the version as a packed u32: major.minor.patch.build
//! where each component gets 8 bits (except major which is sometimes larger).
//! The actual encoding is: (major << 24) | (minor << 16) | (patch << 8) | build.
//!
//! ## `since=` / `until=` semantic doctrine (#935)
//!
//! nif.xml's `<add since="A" until="B">` attribute pair is **inclusive**
//! on both ends: the field is present at every version `v` such that
//! `A <= v <= B`. niftools' own `verexpr` token table backs this up —
//! `#NI_BS_LTE_FO3#` is documented as "All NI + BS *until* Fallout 3"
//! and uses the operator `<=`. nifly mirrors the same convention with
//! `<=`-comparisons against `V10_0_1_X` enum values.
//!
//! Translate to Rust comparisons:
//!
//! - `since="X"`  →  `stream.version() >= NifVersion(X)`
//! - `until="X"`  →  `stream.version() <= NifVersion(X)`
//!
//! The pre-#935 codebase carried an exclusive interpretation
//! (`stream.version() < NifVersion(X)` for `until="X"`) introduced
//! by the #765 / #769 sweep. That was wrong — every gate would
//! mis-skip its field at the boundary version exactly. Bethesda
//! content is unaffected because every shipping `until=` gate sits
//! at a version older than 20.0.0.5 (Oblivion baseline), so the
//! predicate collapsed to `false` either way. The bug bit on
//! pre-Bethesda Gamebryo / NetImmerse content (Civ4 Colonial Fleet,
//! IndustryGiant 2, Morrowind-era mods).

use std::fmt;

/// NIF file format version, packed as a u32.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NifVersion(pub u32);

impl NifVersion {
    // ── NetImmerse / early Gamebryo (4.x – 5.x) ──────────────────────
    /// Earliest NetImmerse version seen in the wild (Civ4-era content).
    pub const V4_0_0_0: Self = Self(0x04000000);
    /// Morrowind era
    pub const V4_0_0_2: Self = Self(0x04000002);
    /// v4.1.0.1 — endianness flag introduced.
    pub const V4_1_0_1: Self = Self(0x04010001);
    /// First version that authors `NiZBufferProperty.z_function`
    /// (pre-4.1.0.12 NIFs predate the field). No Bethesda title ships
    /// in `< 4.1.0.12` — defensive constant used by the property
    /// parser's pre-V4 fallback branch. See NIF-D4-NEW-09 (audit
    /// 2026-05-12).
    pub const V4_1_0_12: Self = Self(0x0401000C);
    /// v4.2.1.0 — NiNode screen-texture count field introduced.
    pub const V4_2_1_0: Self = Self(0x04020100);
    /// v4.2.2.0 — NiAVObject collision-ref path changes.
    pub const V4_2_2_0: Self = Self(0x04020200);
    /// v5.0.0.1 — block-type table introduced in header.
    pub const V5_0_0_1: Self = Self(0x05000001);
    /// v5.0.0.6 — group table introduced in header.
    pub const V5_0_0_6: Self = Self(0x05000006);

    // ── Gamebryo 10.x ─────────────────────────────────────────────────
    /// v10.0.0.0 — transition from NetImmerse 5.x naming.
    pub const V10_0_0_0: Self = Self(0x0A000000);
    /// v10.0.0.6 — seen in early Gamebryo test/debug content.
    pub const V10_0_0_6: Self = Self(0x0A000006);
    /// v10.0.1.0 — extra-data list, NiAVObject collision-ref, and
    /// NiGeometryData flags appear at `>= V10_0_1_0`. High-frequency
    /// boundary (~20 call sites across block parsers).
    pub const V10_0_1_0: Self = Self(0x0A000100);
    /// v10.0.1.2 — BSStreamHeader appears at this version (Oblivion
    /// tools export); also the string-name field on NiAVObject.
    pub const V10_0_1_2: Self = Self(0x0A000102);
    /// v10.0.1.3 — minor variant of the 10.0.1.x family.
    pub const V10_0_1_3: Self = Self(0x0A000103);
    /// v10.0.1.4 — rare; seen in some Oblivion-era mod content.
    pub const V10_0_1_4: Self = Self(0x0A000104);
    /// v10.0.1.8 — `user_version` field appears in NIF header.
    pub const V10_0_1_8: Self = Self(0x0A000108);
    /// Pre-Gamebryo boundary: Order float present in XYZ-rotation blocks at <= this version
    pub const V10_1_0_0: Self = Self(0x0A010000);
    /// v10.1.0.1 — minor transition version.
    pub const V10_1_0_1: Self = Self(0x0A010001);
    /// v10.1.0.101 — NiSkinInstance `skin_partition_ref` field added.
    pub const V10_1_0_101: Self = Self(0x0A010065);
    /// v10.1.0.103 — NiGeomMorpherController interpolator path.
    pub const V10_1_0_103: Self = Self(0x0A010067);
    /// v10.1.0.104 — NiTimeController `interpolator_ref` field added.
    /// High-frequency boundary (~6 call sites).
    pub const V10_1_0_104: Self = Self(0x0A010068);
    /// v10.1.0.106 — NiLight `switch_state` field added.
    /// High-frequency boundary (~6 call sites).
    pub const V10_1_0_106: Self = Self(0x0A01006A);
    /// v10.1.0.108 — inclusive upper bound of the `NiInterpController`
    /// `Manager Controlled` bool (`since=10.1.0.104 until=10.1.0.108`). See
    /// `has_interp_controller_manager_controlled`. (#1506)
    pub const V10_1_0_108: Self = Self(0x0A01006C);
    /// v10.1.0.109 — inclusive upper bound of the `NiQuatTransform`
    /// `TRS Valid` bool[3] (`until=10.1.0.109`). See
    /// `has_quat_transform_trs_valid`. (#1506)
    pub const V10_1_0_109: Self = Self(0x0A01006D);
    /// v10.1.0.110 — lower bound of the `NiBlendInterpolator` byte-sized
    /// middle band (`Array Size` byte, `Priority` byte). (#1508)
    pub const V10_1_0_110: Self = Self(0x0A01006E);
    /// v10.1.0.111 — inclusive upper bound of the `NiBlendInterpolator`
    /// legacy layout (the modern `Flags`-gated layout starts at
    /// 10.1.0.112). (#1508)
    pub const V10_1_0_111: Self = Self(0x0A01006F);
    /// v10.1.0.112 — lower bound of the modern `NiBlendInterpolator`
    /// layout (`Flags` + `Weight Threshold` + Flags-gated fields). (#1508)
    pub const V10_1_0_112: Self = Self(0x0A010070);
    /// v10.1.0.113 — one below the NiGeometryData bsver gate.
    pub const V10_1_0_113: Self = Self(0x0A010071);
    /// v10.1.0.114 — NiGeometryData data-flags layout changes.
    pub const V10_1_0_114: Self = Self(0x0A010072);
    /// v10.2.0.0 — NiTimeController extra-data name field added.
    /// High-frequency boundary (~8 call sites).
    pub const V10_2_0_0: Self = Self(0x0A020000);
    /// v10.4.0.0 — rare pre-Bethesda 10.4.x family.
    pub const V10_4_0_0: Self = Self(0x0A040000);
    /// v10.4.0.1 — NiSwitchNode / particle fields.
    pub const V10_4_0_1: Self = Self(0x0A040001);
    /// v10.4.0.2 — rare; seen in some Gamebryo SDK samples.
    pub const V10_4_0_2: Self = Self(0x0A040002);

    // ── Bethesda 20.x ─────────────────────────────────────────────────
    /// v20.0.0.2 — early Oblivion pre-release content.
    pub const V20_0_0_2: Self = Self(0x14000002);
    /// v20.0.0.3 — Oblivion little-endian flag boundary.
    pub const V20_0_0_3: Self = Self(0x14000003);
    /// Oblivion (v20.0.0.4 — most common Oblivion version)
    pub const V20_0_0_4: Self = Self(0x14000004);
    /// Oblivion (v20.0.0.5 — some Oblivion NIFs)
    pub const V20_0_0_5: Self = Self(0x14000005);
    /// v20.1.0.0 — just below the string-table threshold.
    pub const V20_1_0_0: Self = Self(0x14010000);
    /// v20.1.0.2 — rare; used in some Oblivion-era NifSkope exports.
    pub const V20_1_0_2: Self = Self(0x14010002);
    /// v20.1.0.3 — NiGeometryData vertex-count / UV layout changes.
    /// High-frequency boundary (~5 call sites).
    pub const V20_1_0_3: Self = Self(0x14010003);
    /// v20.2.0.5 — block sizes appear in header; NiSpotLight inner
    /// angle field added. High-frequency boundary (~6 call sites).
    pub const V20_2_0_5: Self = Self(0x14020005);
    /// Fallout 3+ (v20.2.0.7)
    pub const V20_2_0_7: Self = Self(0x14020007);
    /// Skyrim / Fallout 4 (alias for V20_2_0_7)
    pub const V20_2_0_7_SSE: Self = Self(0x14020007);
    /// v20.3.0.4 — rare; seen in some Gamebryo 2.6 SDK content.
    pub const V20_3_0_4: Self = Self(0x14030004);
    /// v20.5.0.4 — rare; seen in some Gamebryo 3.x SDK content.
    pub const V20_5_0_4: Self = Self(0x14050004);

    /// v20.1.0.1 — the inclusive boundary at which Gamebryo introduced
    /// the per-file string table. Headers `>= STRING_TABLE_THRESHOLD`
    /// carry `(strings, max_string_length)` after the block table;
    /// stream reads beyond this version follow the indexed-string path,
    /// older versions inline length-prefixed strings.
    ///
    /// MUST be kept in lockstep across `header.rs::NifHeader::parse`
    /// and `stream.rs::NifStream` — drift here misreads every block's
    /// name. See `header.rs:~224` and `stream.rs:~411`.
    pub const STRING_TABLE_THRESHOLD: Self = Self(0x14010001);

    pub fn major(self) -> u8 {
        (self.0 >> 24) as u8
    }
    pub fn minor(self) -> u8 {
        (self.0 >> 16) as u8
    }
    pub fn patch(self) -> u8 {
        (self.0 >> 8) as u8
    }
    pub fn build(self) -> u8 {
        self.0 as u8
    }

    // ── "Old Oblivion" (v10.0.x) layout predicates ──────────────────
    //
    // NIF file versions in `[10.0.0.0, 10.1.0.114)` (nifly/openmw
    // `VER_OB_OLD`) predate several layout additions. These named,
    // version-aware helpers centralize the per-file-version questions so
    // block parsers query *intent* instead of scattering raw
    // `version < V10_1_0_0` literals — the GameVariant doctrine
    // (`format_abstraction.md` / NIFAL). Because `NifVariant::detect`
    // routes both v10.0.x and v20.0.0.5 to the same `Oblivion` variant,
    // these live on `NifVersion` (the per-file version) rather than on
    // the variant. See #1337.

    /// Every `NiObject` in `[10.0.0.0, 10.1.0.114)` carries a leading
    /// 4-byte `groupID` (nifly `NiObject::Get`, `BasicTypes.hpp:972`).
    /// The `bhkRefObject` Havok-serializable subtree is the lone
    /// exception (its Get-chain never reaches `NiObject::Get`), so the
    /// dispatcher pairs this version test with the
    /// `is_havok_serializable` type predicate.
    pub fn has_object_group_id(self) -> bool {
        self >= Self::V10_0_0_0 && self < Self::V10_1_0_114
    }

    /// `hkpMoppCode.Offset` (origin `Vector4`) is `since="10.1.0.0"` —
    /// absent on old-Oblivion `bhkMoppBvTreeShape` (#1329).
    pub fn has_mopp_offset(self) -> bool {
        self >= Self::V10_1_0_0
    }

    /// `bhkNiTriStripsShape.Scale` (`Vector4`) is `since="10.1.0.0"` —
    /// absent on old-Oblivion strips collision shapes (#1337).
    pub fn has_havok_strips_scale(self) -> bool {
        self >= Self::V10_1_0_0
    }

    /// `NiSkinData.Skin Partition` is a `Ref` carried inline (between
    /// `Num Bones` and `Has Vertex Weights`) `until="10.1.0.0"`; later
    /// versions moved it to `NiSkinInstance`. Old-Oblivion creature skin
    /// data still has it (#1337).
    pub fn has_skin_data_partition_ref(self) -> bool {
        self <= Self::V10_1_0_0
    }

    /// `NiKeyframeController.Data` ref (a `NiKeyframeData`) is
    /// `until="10.1.0.103"` — old-Oblivion keyframe controllers store
    /// the data ref directly; `since="10.1.0.104"` it is replaced by the
    /// `NiSingleInterpController.Interpolator` ref. (#1337)
    pub fn has_keyframe_controller_data(self) -> bool {
        self <= Self::V10_1_0_103
    }

    /// `NiInterpController.Manager Controlled` is a 1-byte `bool` present
    /// only `since="10.1.0.104" until="10.1.0.108"` (nif.xml, both ends
    /// inclusive). It sits between the `NiTimeController` base and the
    /// `NiSingleInterpController.Interpolator` ref, so every
    /// NiInterpController descendant in that narrow band carries it.
    /// Retail Bethesda content (Oblivion 20.0.0.x, FO3+ 20.2.0.7) is far
    /// above the upper bound, so this is false everywhere except old
    /// Gamebryo content (e.g. Oblivion's `_1stperson\skeleton.nif`,
    /// file-version 10.1.0.106). (#1506)
    pub fn has_interp_controller_manager_controlled(self) -> bool {
        self >= Self::V10_1_0_104 && self <= Self::V10_1_0_108
    }

    /// `NiQuatTransform.TRS Valid` is a trailing `bool[3]` (3 × 1-byte
    /// bool, one per Translation/Rotation/Scale component) present only
    /// `until="10.1.0.109"` (nif.xml, inclusive). After it the struct is
    /// just translation + rotation + scale. Retail Bethesda content
    /// (Oblivion 20.0.0.x, FO3+ 20.2.0.7) is above the bound, so the bytes
    /// are absent there; they appear on old Gamebryo content such as
    /// Oblivion's `_1stperson\skeleton.nif` (10.1.0.106), where every
    /// `NiTransformInterpolator` carries them. (#1506)
    pub fn has_quat_transform_trs_valid(self) -> bool {
        self <= Self::V10_1_0_109
    }

    /// `bhkRigidBody` below `10.1.0.0` uses a distinct wire layout: it
    /// predates the `since="10.1.0.0"` CInfo additions (the duplicated
    /// filter/entity prefix and the max-velocity / penetration triple)
    /// and carries the `<= VER_OB_OLD` `bhkWorldObject` Unknown. (#1329)
    pub fn uses_old_rigid_body_layout(self) -> bool {
        self < Self::V10_1_0_0
    }
}

impl fmt::Display for NifVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}.{}.{}.{}",
            self.major(),
            self.minor(),
            self.patch(),
            self.build()
        )
    }
}

/// Named `user_version_2` (BSVER) thresholds.
///
/// Bethesda's NIFs carry a `user_version_2` byte that disambiguates
/// the engine variant when the wire-format version alone is not
/// enough (every Bethesda title since Oblivion ships `V20_2_0_7`).
/// Block parsers gate field reads on `stream.bsver()` against these
/// boundaries; the canonical retail values per game are documented
/// on [`NifVariant::bsver`].
///
/// Use these constants in raw `bsver` comparisons instead of bare
/// decimal literals — the symbols survive cross-file refactors and
/// document intent at the call site. See [`NifVariant`] for the
/// feature-flag helpers that wrap most common predicates.
pub mod bsver {
    /// Pre-Bethesda authoring tools (Morrowind era, NifSkope older
    /// builds) — every Bethesda title is `>= FO3_FNV`.
    pub const PRE_BETHESDA: u32 = 0;
    /// `bhkRigidBody` carries two trailing `Unknown Float 1/2` fields on
    /// content with `bsver < RIGID_BODY_EXTRA_FLOATS` (nif.xml
    /// `#BSVER# #LT# 9`). Pre-collision-v2 Oblivion dev builds
    /// (e.g. boxtest skeleton.nif, bsver=6) hit this; standard Oblivion
    /// (bsver=11) does not. See #549.
    pub const RIGID_BODY_EXTRA_FLOATS: u32 = 9;
    /// Oblivion BSVER (v20.0.0.4 / v20.0.0.5 or v20.2.0.7 with uv=11).
    /// Pre-collision v2 content ships `bsver < RIGID_BODY_EXTRA_FLOATS`;
    /// standard Oblivion content ships at exactly 11.
    pub const OBLIVION: u32 = 11;
    /// Upper bound of the nif.xml `#NI_BS_LTE_16#` verset ("All NI + BS
    /// until BSVER 16"). Content with `bsver > NI_BS_LTE_16` carries the
    /// post-Oblivion additions gated on `!#NI_BS_LTE_16#` — e.g.
    /// `NiPSysGravityModifier.World Aligned` (the byte that over-read on
    /// Oblivion bsver=11 when this was mis-gated on NIF version, #1306-sib)
    /// and the `bhkRagdollConstraint` motor fields. Oblivion (bsver 11) is
    /// `<= 16`; FO3-era and later (bsver 34+) is `> 16`.
    pub const NI_BS_LTE_16: u32 = 16;
    /// `NiMaterialProperty` carries the `Emissive Mult` float only on
    /// content with `bsver > MATERIAL_EMISSIVE_MULT` (nif.xml
    /// `#BSVER# #GT# 21`). Strict `>`, so FO3-era content at exactly 21
    /// is excluded and defaults emissive_mult to 1.0. See #323.
    pub const MATERIAL_EMISSIVE_MULT: u32 = 21;
    /// Pre-retail FO3 dev builds that first added refraction fields
    /// (`refraction_strength`, `refraction_fire_period`) to
    /// BSShaderPPLightingProperty. Content with `bsver > FO3_REFRACTION`
    /// carries these fields.
    pub const FO3_REFRACTION: u32 = 14;
    /// Pre-retail FO3 dev builds that added parallax fields
    /// (`parallax_max_passes`, `parallax_scale`). Content with
    /// `bsver > FO3_PARALLAX` carries these fields.
    pub const FO3_PARALLAX: u32 = 24;
    /// BSVER threshold at which NiAVObject `flags` widens from u16 to
    /// u32 (`bsver > FLAGS_U32_THRESHOLD`). Corresponds to nif.xml's
    /// `#BS_GTE_26#` predicate.
    pub const FLAGS_U32_THRESHOLD: u32 = 26;
    /// BSVER threshold at which NiControllerSequence gains an animation-
    /// notes list (`bsver > ANIM_NOTES_THRESHOLD`). Content with
    /// `bsver <= 28` (Oblivion and early FO3 dev) omits the list.
    pub const ANIM_NOTES_THRESHOLD: u32 = 28;
    /// Fallout 3 retail + Fallout New Vegas (binary-identical at
    /// BSVER 34 per nif.xml `V20_2_0_7_FO3`).
    pub const FO3_FNV: u32 = 34;
    /// bhkRigidBody `body_flags` switches from u32 (`bsver < 76`) to
    /// u16 (`bsver >= 76`). Sits between FO3_FNV and SKYRIM_LE in the
    /// dev-build BSVER timeline.
    pub const RIGID_BODY_FLAGS16: u32 = 76;
    /// Skyrim LE.
    pub const SKYRIM_LE: u32 = 83;
    /// Skyrim Special Edition.
    pub const SKYRIM_SE: u32 = 100;
    /// Fallout 4 (also covers 132 / 139 patch BSVERs in shipping
    /// content per the `Fallout4` variant fan-out; this constant is
    /// the lower bound).
    pub const FALLOUT4: u32 = 130;
    /// Intentional gap: no retail game shipped at BSVER 131.
    /// BSLightingShaderProperty / BSEffectShaderProperty flag arrays
    /// and CRC fields are BOTH absent at this value — parsers must
    /// handle it as a no-op for both branches.
    pub const FO4_SHADER_GAP: u32 = 131;
    /// FO4 patch — shader flags switch to CRC hash arrays
    /// (`bsver >= FO4_CRC_FLAGS`). Corresponds to nif.xml's
    /// `#BS_GTE_132#` predicate.
    pub const FO4_CRC_FLAGS: u32 = 132;
    /// Exclusive upper bound of the FO4-DLC range that authors the
    /// `BSEffectShaderProperty` SSR / skin-tint trailing bools — i.e.
    /// the range `FALLOUT4..FO4_DLC_UPPER` (BSVER 130..=139) carries
    /// `use_ssr` / `wetness_use_ssr` after `env_map_scale` on
    /// shader_type = 1 (EnvironmentMap) and `skin_tint_alpha` after
    /// the RGB triple on shader_type = 5 (SkinTint). FO76+
    /// (`bsver >= FO76 = 155`) does NOT carry these fields.
    ///
    /// The previous interpretation (`env_map_scale` "moves inside
    /// wetness" at this BSVER) was empirically wrong — gating wetness
    /// env_map_scale on this threshold dropped Starfield Meshes01
    /// parse rate from 97.21% → 95.77%. See `shader.rs:943-957` for
    /// the empirical evidence and #1223 for the fix that obsoleted
    /// the wetness claim.
    pub const FO4_DLC_UPPER: u32 = 140;
    /// Lower bound for FO76 SF2 CRC arrays (`bsver >= FO76_SF2_CRCS`).
    /// The `num_sf2` count field is only present at this BSVER and
    /// above; below it the SF2 array is always empty.
    pub const FO76_SF2_CRCS: u32 = 152;
    /// Fallout 76 (lower bound; FO76 spans 152..=167 in shipping
    /// content).
    pub const FO76: u32 = 155;
    /// Starfield (lower bound — `>=168` in retail; 170 is the
    /// historical cutoff vs FO76 dev builds).
    pub const STARFIELD: u32 = 172;
    /// Starfield dev+ builds that carry a `form_id` field in some
    /// blocks (e.g. NiNode). Content with `bsver >= SF_FORM_ID` has
    /// this field; retail Starfield at 172 does not.
    pub const SF_FORM_ID: u32 = 173;
}

/// Which game generation produced this NIF.
///
/// Derived once from the header's (version, user_version, user_version_2) triplet.
/// Block parsers query semantic feature flags on this enum instead of comparing
/// raw version numbers, keeping game-specific quirks in one place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NifVariant {
    /// Morrowind (NIF ≤ 4.x, NetImmerse era)
    Morrowind,
    /// Oblivion (NIF 20.0.0.5, user_version < 11)
    Oblivion,
    /// Fallout 3 dev/mod NIFs (NIF 20.2.0.7, uv=11, uv2 < 34 — fans out
    /// across bsver 14, 16, 21, 24-33 in pre-retail authoring tools).
    /// Retail FO3 ships at bsver=34 and detects as `FalloutNV` instead
    /// (the two are binary-identical at that BSVER per nif.xml line 208:
    /// `<version id="V20_2_0_7_FO3" num="20.2.0.7" user="11" bsver="34"
    /// ext="rdt">Fallout 3, Fallout NV</version>`).
    Fallout3,
    /// Fallout New Vegas (NIF 20.2.0.7, uv=11, uv2=34)
    FalloutNV,
    /// Skyrim LE (NIF 20.2.0.7, uv=12, uv2=83)
    SkyrimLE,
    /// Skyrim SE (NIF 20.2.0.7, uv=12, uv2=100)
    SkyrimSE,
    /// Fallout 4 (NIF 20.2.0.7, uv=12, uv2=130)
    Fallout4,
    /// Fallout 76 (NIF 20.2.0.7, uv=12, uv2=155)
    Fallout76,
    /// Starfield (NIF 20.2.0.7, uv=12, uv2≥170)
    Starfield,
    /// Unknown version — parse with best effort.
    Unknown,
}

impl NifVariant {
    /// Determine the game variant from the NIF header version triplet.
    pub fn detect(version: NifVersion, user_version: u32, user_version_2: u32) -> Self {
        if version.0 <= 0x04000002 {
            return Self::Morrowind;
        }
        // V20.0.0.4 and V20.0.0.5 are exclusively Oblivion — no other game uses these.
        // Check before the uv/uv2 match to avoid misidentifying as FO3/FNV.
        //
        // #943 / NIF-D2-NEW-03 considered routing `V20_0_0_4 user=11`
        // to `Fallout3` because nif.xml's `#FO3#` verset (line 44)
        // includes `V20_0_0_4__11`. nif.xml line 196 itself lists the
        // version as "Oblivion, Fallout 3" — genuinely ambiguous.
        // Sample data would be needed to settle which side wins; the
        // existing test `detect_oblivion_edge_cases` pins the prior
        // decision that `(V20_0_0_4, 11, _)` is Oblivion, and no
        // retail FO3 NIF ships at v20.0.0.4 so the impact is confined
        // to pre-release / mod content. Leave as Oblivion until sample
        // data justifies the flip.
        //
        // #1219 — fire a process-lifetime one-shot warn when the
        // ambiguous `(V20_0_0_4, 11, _)` tuple is hit, so any modded
        // FO3 corpus that exercises this case surfaces in the log
        // (with the tuple values) and we can collect the sample data
        // the audit asks for. Vanilla content never hits it; the warn
        // is silent on every supported game today.
        if version == NifVersion::V20_0_0_4 && user_version == 11 {
            static ONCE: std::sync::Once = std::sync::Once::new();
            ONCE.call_once(|| {
                log::warn!(
                    "NifVariant::detect: ambiguous tuple (V20_0_0_4, \
                     user_version=11, user_version_2={}) routed as Oblivion \
                     pending sample-data disambiguation — nif.xml lists \
                     v20.0.0.4 as \"Oblivion, Fallout 3\" and the `#FO3#` \
                     verset includes V20_0_0_4__11. File a sample to flip \
                     the routing (#1219).",
                    user_version_2,
                );
            });
        }
        if version == NifVersion::V20_0_0_4 || version == NifVersion::V20_0_0_5 {
            return Self::Oblivion;
        }
        // V20.2.0.7+ — disambiguate by user_version and user_version_2 (BSVER).
        match (user_version, user_version_2) {
            // user_version < 11: Oblivion exports on v20.2.0.7 (NifSkope, older tools)
            (uv, _) if uv < 11 => Self::Oblivion,
            (11, uv2) if uv2 < 34 => Self::Fallout3,
            (11, 34) => Self::FalloutNV,
            (12, uv2) if uv2 <= 83 => Self::SkyrimLE,
            (12, uv2) if uv2 <= 100 => Self::SkyrimSE,
            // 101-129: unknown gap, treat as SkyrimSE (closest known)
            (12, uv2) if uv2 < 130 => Self::SkyrimSE,
            (12, uv2) if uv2 < 155 => Self::Fallout4,
            // Fallout 76 retail ships BSVER 155–167. Starfield dev/retail
            // ships 168+. The 155..170 window below is the currently-
            // known FO76 range; 168/169 in the wild are reportedly early
            // Starfield dev builds that we classify as FO76 anyway.
            // Cosmetic distinction today — every shader / block
            // conditional we care about is gated on `bsver >= 132`,
            // which covers both identically. Re-tighten to `< 168` once
            // a confirmed Starfield dev-build corpus lands (#173).
            (12, uv2) if uv2 < 170 => Self::Fallout76,
            (12, _) => Self::Starfield,
            _ => Self::Unknown,
        }
    }

    /// Single representative `user_version_2` per game variant.
    /// Hard-pinned to one value each — `FO3=34, FNV=34, SK=83,
    /// SK_SE=100, FO4=130, FO76=155, SF=172` — for callers that need
    /// "the" BSVER associated with a game (e.g. fixture builders,
    /// telemetry buckets, the version-detection unit tests).
    ///
    /// **This value is one representative, not an exhaustive span.**
    /// Shipping content covers wider BSVER ranges per game:
    /// FO4 ships at `{130, 132, 139}`, FO76 spans `152..=167`, and
    /// Starfield is unbounded `≥168`. Parser callers that need to
    /// honour the file's actual `user_version_2` (e.g. nif.xml's
    /// per-field `#BSVER#` conditionals) MUST use `stream.bsver()`,
    /// not this method.
    ///
    /// `Fallout3` returns 34 to match the retail value even though
    /// the variant itself matches dev/mod pre-retail builds (in-file
    /// bsver < 34). The variant exists for parser routing — the
    /// retail bsver is the right answer to "what BSVER does FO3
    /// canonically ship at?"
    ///
    /// See #937 / NIF-D2-NEW-01 for the audit history (pre-fix
    /// `Fallout3.bsver()` returned 21, one of many dev-tool BSVERs in
    /// the [0, 33] fan-out, which contradicted the hard-pin) and
    /// NIF-D2-NEW-09 (audit 2026-05-12) for the FO4/FO76/SF span
    /// clarification.
    pub fn bsver(self) -> u32 {
        match self {
            Self::Morrowind | Self::Oblivion => 0,
            // Retail FO3 ships at bsver=34, identical to FNV. See #937.
            Self::Fallout3 | Self::FalloutNV => 34,
            Self::SkyrimLE => 83,
            Self::SkyrimSE => 100,
            Self::Fallout4 => 130,
            Self::Fallout76 => 155,
            Self::Starfield => 172,
            Self::Unknown => 0,
        }
    }

    // ── Feature flags ──────────────────────────────────────────────
    // Each method documents which games have the feature and why.
    // Parsers call these instead of raw `user_version_2 >= N` checks.
    //
    // #938 / NIF-D2-NEW-04 — three predicates (compact_material,
    // has_emissive_mult, has_shader_emissive_color) were deleted here.
    // They had zero production call sites — every parser queried
    // `stream.bsver()` directly — and their own doc comments told
    // callers to prefer that path. Keeping them around as "approved
    // helpers" alongside the ones that ARE called (`has_properties_list`,
    // `has_material_crc`, etc.) was an architectural foot-gun: no way
    // for a future contributor to know which family is blessed without
    // grepping. The Fallout3 vs FalloutNV boundary made the deleted
    // predicates disagree with the parse path by one bsver step at
    // the v20.2.0.7 boundary, compounding the foot-gun.

    /// NiGeometryData has a material CRC field after data_flags.
    /// Present in Skyrim+ (user_version >= 12).
    pub fn has_material_crc(self) -> bool {
        matches!(
            self,
            Self::SkyrimLE | Self::SkyrimSE | Self::Fallout4 | Self::Fallout76 | Self::Starfield
        )
    }

    // ── NiAVObject feature flags (from nif.xml) ───────────────────

    /// NiAVObject has Num Properties + Properties list.
    /// nif.xml: `#NI_BS_LTE_FO3#` (BSVER ≤ 34). Removed in Skyrim+.
    pub fn has_properties_list(self) -> bool {
        matches!(
            self,
            Self::Morrowind | Self::Oblivion | Self::Fallout3 | Self::FalloutNV
        )
    }

    /// NiAVObject flags field is u32 (BSVER > 26). Older versions use u16.
    /// nif.xml: flags is uint for BSVER > 26, ushort otherwise.
    pub fn avobject_flags_u32(self) -> bool {
        matches!(
            self,
            Self::Fallout3
                | Self::FalloutNV
                | Self::SkyrimLE
                | Self::SkyrimSE
                | Self::Fallout4
                | Self::Fallout76
                | Self::Starfield
        )
    }

    // ── NiGeometry feature flags ──────────────────────────────────

    /// NiGeometry has Shader Property + Alpha Property refs.
    /// nif.xml: `#BS_GT_FO3#` (BSVER > 34). Present in Skyrim+, NOT in FNV.
    pub fn has_shader_alpha_refs(self) -> bool {
        matches!(
            self,
            Self::SkyrimLE | Self::SkyrimSE | Self::Fallout4 | Self::Fallout76 | Self::Starfield
        )
    }

    // ── NiNode feature flags ──────────────────────────────────────

    /// NiNode has Num Effects + Effects list.
    /// nif.xml: `#NI_BS_LT_FO4#` (BSVER < 130). Present in everything pre-FO4.
    pub fn has_effects_list(self) -> bool {
        matches!(
            self,
            Self::Morrowind
                | Self::Oblivion
                | Self::Fallout3
                | Self::FalloutNV
                | Self::SkyrimLE
                | Self::SkyrimSE
        )
    }

    // ── BSShaderProperty feature flags ────────────────────────────

    /// BSShaderProperty has ShaderType, ShaderFlags, ShaderFlags2, EnvMapScale.
    /// nif.xml: `#NI_BS_LTE_FO3#` (BSVER ≤ 34). Only FO3/FNV.
    pub fn has_shader_property_fo3_fields(self) -> bool {
        matches!(self, Self::Fallout3 | Self::FalloutNV)
    }

    // ── BSTriShape (SSE+ specific geometry) ───────────────────────

    /// Uses BSTriShape instead of NiTriShape for geometry.
    /// nif.xml: `#SSE# #FO4# #F76#`. SSE and later.
    pub fn uses_bs_tri_shape(self) -> bool {
        matches!(
            self,
            Self::SkyrimSE | Self::Fallout4 | Self::Fallout76 | Self::Starfield
        )
    }

    // ── NiNode feature flags (#1277 Task 5) ──────────────────────

    /// NiNode has the `culling_mode` field (Skyrim LE+). FO3/FNV stop
    /// after `multi_bound_ref`; Skyrim added `culling_mode` as a new
    /// trailing field. nif.xml: `#SKY_AND_LATER#` on the field.
    ///
    /// Source: `crates/nif/src/blocks/node.rs:257` previously inlined
    /// `stream.bsver() >= crate::version::bsver::SKYRIM_LE`.
    pub fn has_culling_mode(self) -> bool {
        matches!(
            self,
            Self::SkyrimLE | Self::SkyrimSE | Self::Fallout4 | Self::Fallout76 | Self::Starfield
        )
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_components() {
        let v = NifVersion::V20_2_0_7;
        assert_eq!(v.major(), 0x14); // 20
        assert_eq!(v.minor(), 0x02);
        assert_eq!(v.patch(), 0x00);
        assert_eq!(v.build(), 0x07);
    }

    #[test]
    fn version_display() {
        assert_eq!(NifVersion::V20_2_0_7.to_string(), "20.2.0.7");
        assert_eq!(NifVersion::V4_0_0_2.to_string(), "4.0.0.2");
        assert_eq!(NifVersion::V20_0_0_5.to_string(), "20.0.0.5");
    }

    /// #1337 — the "old Oblivion" (v10.0.x) layout predicates. v10.0.1.0
    /// / v10.0.1.2 take the old paths; standard Oblivion v20.0.0.5 and
    /// every later title (FO3/FNV/Skyrim/FO4) take the modern paths.
    #[test]
    fn old_oblivion_layout_predicates() {
        let v10 = NifVersion::V10_0_1_2; // VER_OB_OLD band
        let v20 = NifVersion::V20_0_0_5; // standard Oblivion / modern band

        // groupID version band [10.0.0.0, 10.1.0.114): v10.0.x in, v20 out.
        assert!(v10.has_object_group_id());
        assert!(!v20.has_object_group_id());

        // since 10.1.0.0 fields — present on v20, absent on v10.0.x.
        assert!(!v10.has_mopp_offset() && v20.has_mopp_offset());
        assert!(!v10.has_havok_strips_scale() && v20.has_havok_strips_scale());

        // until-boundary fields — present on v10.0.x, absent on v20.
        assert!(v10.has_skin_data_partition_ref() && !v20.has_skin_data_partition_ref());
        assert!(v10.has_keyframe_controller_data() && !v20.has_keyframe_controller_data());

        // old rigid-body layout: v10.0.x only.
        assert!(v10.uses_old_rigid_body_layout() && !v20.uses_old_rigid_body_layout());

        // boundary: exactly 10.1.0.0 is "modern" for the scale/offset
        // (since=10.1.0.0) but still carries the skin-partition ref
        // (until=10.1.0.0, inclusive).
        let b = NifVersion::V10_1_0_0;
        assert!(b.has_mopp_offset() && b.has_skin_data_partition_ref());
    }

    /// #1506 — the two narrow 10.1.0.x sub-band predicates that gate the
    /// `NiInterpController.Manager Controlled` bool (since=10.1.0.104
    /// until=10.1.0.108) and the `NiQuatTransform.TRS Valid` bool[3]
    /// (until=10.1.0.109). Both must be true on the first-person
    /// skeleton's 10.1.0.106 and false on all retail Bethesda versions.
    #[test]
    fn old_gamebryo_10_1_sub_band_predicates() {
        let skel = NifVersion::V10_1_0_106; // _1stperson\skeleton.nif
        let obliv = NifVersion::V20_0_0_5;
        let fo3 = NifVersion::V20_2_0_7;

        // Manager Controlled: inclusive [10.1.0.104, 10.1.0.108].
        assert!(skel.has_interp_controller_manager_controlled());
        assert!(NifVersion::V10_1_0_104.has_interp_controller_manager_controlled());
        assert!(NifVersion::V10_1_0_108.has_interp_controller_manager_controlled());
        assert!(!NifVersion::V10_1_0_109.has_interp_controller_manager_controlled());
        assert!(!NifVersion::V10_1_0_103.has_interp_controller_manager_controlled());
        assert!(!obliv.has_interp_controller_manager_controlled());
        assert!(!fo3.has_interp_controller_manager_controlled());

        // TRS Valid: inclusive upper bound 10.1.0.109.
        assert!(skel.has_quat_transform_trs_valid());
        assert!(NifVersion::V10_1_0_109.has_quat_transform_trs_valid());
        assert!(!NifVersion::V10_1_0_113.has_quat_transform_trs_valid());
        assert!(!obliv.has_quat_transform_trs_valid());
        assert!(!fo3.has_quat_transform_trs_valid());
    }

    #[test]
    fn version_ordering() {
        assert!(NifVersion::V4_0_0_2 < NifVersion::V20_0_0_5);
        assert!(NifVersion::V20_0_0_5 < NifVersion::V20_2_0_7);
        assert_eq!(NifVersion::V20_2_0_7, NifVersion::V20_2_0_7_SSE);
    }

    #[test]
    fn version_constants_match_packed() {
        assert_eq!(NifVersion::V4_0_0_2.0, 0x04000002);
        assert_eq!(NifVersion::V20_0_0_5.0, 0x14000005);
        assert_eq!(NifVersion::V20_2_0_7.0, 0x14020007);
    }

    #[test]
    fn detect_morrowind() {
        assert_eq!(
            NifVariant::detect(NifVersion::V4_0_0_2, 0, 0),
            NifVariant::Morrowind,
        );
    }

    #[test]
    fn detect_oblivion() {
        // Standard Oblivion v20.0.0.4: most common Oblivion NIF version
        assert_eq!(
            NifVariant::detect(NifVersion::V20_0_0_4, 11, 11),
            NifVariant::Oblivion,
        );
        // Standard Oblivion v20.0.0.5
        assert_eq!(
            NifVariant::detect(NifVersion::V20_0_0_5, 0, 0),
            NifVariant::Oblivion,
        );
        // Oblivion on v20.2.0.7 with low user_version
        assert_eq!(
            NifVariant::detect(NifVersion::V20_2_0_7, 10, 0),
            NifVariant::Oblivion,
        );
    }

    #[test]
    fn detect_oblivion_edge_cases() {
        // v20.0.0.4 is always Oblivion regardless of user_version/user_version_2
        assert_eq!(
            NifVariant::detect(NifVersion::V20_0_0_4, 0, 0),
            NifVariant::Oblivion,
        );
        assert_eq!(
            NifVariant::detect(NifVersion::V20_0_0_4, 11, 34),
            NifVariant::Oblivion,
        );
        // v20.0.0.5 is always Oblivion regardless of user_version/user_version_2
        assert_eq!(
            NifVariant::detect(NifVersion::V20_0_0_5, 11, 34),
            NifVariant::Oblivion,
        );
        assert_eq!(
            NifVariant::detect(NifVersion::V20_0_0_5, 12, 100),
            NifVariant::Oblivion,
        );
        assert_eq!(
            NifVariant::detect(NifVersion::V20_0_0_5, 0, 25),
            NifVariant::Oblivion,
        );
        // v20.2.0.7 with user_version=0 (NifSkope export)
        assert_eq!(
            NifVariant::detect(NifVersion::V20_2_0_7, 0, 0),
            NifVariant::Oblivion,
        );
        // v20.2.0.7 with user_version=10 (some Oblivion mods)
        assert_eq!(
            NifVariant::detect(NifVersion::V20_2_0_7, 10, 25),
            NifVariant::Oblivion,
        );
    }

    /// #1219 — the ambiguous `(V20_0_0_4, user_version=11, _)` tuple
    /// is the nif.xml "Oblivion, Fallout 3" case. Current routing
    /// pins to Oblivion (no retail FO3 NIF ships at this version, so
    /// any hit is pre-release / mod content). The one-shot warn at
    /// `detect:307` surfaces the tuple in logs without breaking the
    /// routing pin; this test guards the routing stays Oblivion until
    /// sample data justifies flipping.
    #[test]
    fn detect_ambiguous_v20_0_0_4_uv11_stays_oblivion_pending_sample() {
        // The audit's ambiguous tuple — primary case the warn fires on.
        assert_eq!(
            NifVariant::detect(NifVersion::V20_0_0_4, 11, 0),
            NifVariant::Oblivion,
            "ambiguous (V20_0_0_4, 11, 0) must route Oblivion until \
             sample data justifies the flip",
        );
        assert_eq!(
            NifVariant::detect(NifVersion::V20_0_0_4, 11, 34),
            NifVariant::Oblivion,
            "ambiguous (V20_0_0_4, 11, 34) must route Oblivion until \
             sample data justifies the flip",
        );
        // Non-ambiguous neighbours (user_version != 11) must continue
        // routing Oblivion via the unconditional v20.0.0.4 branch — the
        // warn path must NOT shadow them.
        assert_eq!(
            NifVariant::detect(NifVersion::V20_0_0_4, 0, 0),
            NifVariant::Oblivion,
        );
        assert_eq!(
            NifVariant::detect(NifVersion::V20_0_0_4, 10, 0),
            NifVariant::Oblivion,
        );
    }

    #[test]
    fn detect_gap_ranges() {
        // BSVER 101-129 (between SkyrimSE and FO4) → SkyrimSE
        assert_eq!(
            NifVariant::detect(NifVersion::V20_2_0_7, 12, 110),
            NifVariant::SkyrimSE,
        );
        // BSVER 156-169 (between FO76 and Starfield) → FO76
        assert_eq!(
            NifVariant::detect(NifVersion::V20_2_0_7, 12, 160),
            NifVariant::Fallout76,
        );
        // BSVER 170+ → Starfield
        assert_eq!(
            NifVariant::detect(NifVersion::V20_2_0_7, 12, 200),
            NifVariant::Starfield,
        );
    }

    #[test]
    fn detect_fallout3() {
        assert_eq!(
            NifVariant::detect(NifVersion::V20_2_0_7, 11, 21),
            NifVariant::Fallout3,
        );
    }

    #[test]
    fn detect_fallout_nv() {
        assert_eq!(
            NifVariant::detect(NifVersion::V20_2_0_7, 11, 34),
            NifVariant::FalloutNV,
        );
    }

    #[test]
    fn detect_skyrim_le() {
        assert_eq!(
            NifVariant::detect(NifVersion::V20_2_0_7, 12, 83),
            NifVariant::SkyrimLE,
        );
    }

    #[test]
    fn detect_skyrim_se() {
        assert_eq!(
            NifVariant::detect(NifVersion::V20_2_0_7, 12, 100),
            NifVariant::SkyrimSE,
        );
    }

    #[test]
    fn detect_fallout4() {
        assert_eq!(
            NifVariant::detect(NifVersion::V20_2_0_7, 12, 130),
            NifVariant::Fallout4,
        );
    }

    #[test]
    fn detect_fallout76() {
        assert_eq!(
            NifVariant::detect(NifVersion::V20_2_0_7, 12, 155),
            NifVariant::Fallout76,
        );
    }

    #[test]
    fn detect_starfield() {
        assert_eq!(
            NifVariant::detect(NifVersion::V20_2_0_7, 12, 172),
            NifVariant::Starfield,
        );
    }

    /// AUDIT_NIF Dim 2 hard-pin: `bsver()` must return the canonical
    /// retail BSVER per nif.xml — `FO3=34, FNV=34, SK=83, SK_SE=100,
    /// FO4=130, FO76=155, SF=172`. `Fallout3` returns 34 even though
    /// the variant matches pre-retail dev/mod files; retail FO3 ships
    /// at bsver=34 and detects as `FalloutNV` (same wire BSVER), so 34
    /// is the right "what does this game canonically ship at?" value.
    /// Pre-#937 the FO3 arm returned 21 (one of many dev-tool BSVERs)
    /// which contradicted the hard-pin. NIF-D2-NEW-02 adds explicit
    /// Starfield + Unknown asserts so the full enum surface is pinned.
    #[test]
    fn bsver_values() {
        assert_eq!(NifVariant::Morrowind.bsver(), 0);
        assert_eq!(NifVariant::Oblivion.bsver(), 0);
        assert_eq!(NifVariant::Fallout3.bsver(), 34);
        assert_eq!(NifVariant::FalloutNV.bsver(), 34);
        assert_eq!(NifVariant::SkyrimLE.bsver(), 83);
        assert_eq!(NifVariant::SkyrimSE.bsver(), 100);
        assert_eq!(NifVariant::Fallout4.bsver(), 130);
        assert_eq!(NifVariant::Fallout76.bsver(), 155);
        assert_eq!(NifVariant::Starfield.bsver(), 172);
        assert_eq!(NifVariant::Unknown.bsver(), 0);
    }

    #[test]
    fn feature_properties_list() {
        // FNV and earlier have properties list on NiAVObject
        assert!(NifVariant::Morrowind.has_properties_list());
        assert!(NifVariant::Oblivion.has_properties_list());
        assert!(NifVariant::FalloutNV.has_properties_list());
        // Skyrim+ removed it
        assert!(!NifVariant::SkyrimLE.has_properties_list());
        assert!(!NifVariant::SkyrimSE.has_properties_list());
        assert!(!NifVariant::Fallout4.has_properties_list());
    }

    #[test]
    fn feature_shader_alpha_refs() {
        // Skyrim+ has dedicated shader/alpha property refs on NiGeometry
        assert!(!NifVariant::FalloutNV.has_shader_alpha_refs());
        assert!(NifVariant::SkyrimLE.has_shader_alpha_refs());
        assert!(NifVariant::SkyrimSE.has_shader_alpha_refs());
        assert!(NifVariant::Fallout4.has_shader_alpha_refs());
    }

    #[test]
    fn feature_effects_list() {
        // Everything before FO4 has effects list on NiNode
        assert!(NifVariant::FalloutNV.has_effects_list());
        assert!(NifVariant::SkyrimSE.has_effects_list());
        assert!(!NifVariant::Fallout4.has_effects_list());
    }

    // #938 — `feature_compact_material` / `feature_has_emissive_mult`
    // were deleted alongside the predicates they exercised. No
    // production code queried either; the in-file `stream.bsver()`
    // path was the authority. See the comment block on the
    // `Feature flags` section in the impl.

    #[test]
    fn feature_material_crc() {
        assert!(!NifVariant::FalloutNV.has_material_crc());
        assert!(NifVariant::SkyrimLE.has_material_crc());
        assert!(NifVariant::Fallout4.has_material_crc());
        assert!(NifVariant::Fallout76.has_material_crc());
        assert!(NifVariant::Starfield.has_material_crc());
    }

    // ── #1277 Task 5: new helper coverage ──────────────────────────

    #[test]
    fn feature_culling_mode() {
        // Skyrim+ only — pre-Skyrim NiNodes stop after multi_bound_ref.
        assert!(!NifVariant::Morrowind.has_culling_mode());
        assert!(!NifVariant::Oblivion.has_culling_mode());
        assert!(!NifVariant::Fallout3.has_culling_mode());
        assert!(!NifVariant::FalloutNV.has_culling_mode());
        assert!(NifVariant::SkyrimLE.has_culling_mode());
        assert!(NifVariant::SkyrimSE.has_culling_mode());
        assert!(NifVariant::Fallout4.has_culling_mode());
        assert!(NifVariant::Fallout76.has_culling_mode());
        assert!(NifVariant::Starfield.has_culling_mode());
        assert!(!NifVariant::Unknown.has_culling_mode());
    }
}
