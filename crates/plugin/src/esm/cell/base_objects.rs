//! Base-object record types (the records REFRs point at), extracted from
//! ../mod.rs (#5805 type-hub split).
//!
//! Types: `LightData`, `DecalData`, `AddonData`, `StaticObject`,
//! `DestructionData` / `DestructionStage`, `TextureSet`.

use crate::esm::records::script_instance::ScriptInstanceData;

/// Light-specific data extracted from LIGH record DATA subrecord.
#[derive(Debug, Clone)]
pub struct LightData {
    pub radius: f32,
    pub color: [f32; 3],
    pub flags: u32,
    /// FNAM flicker / pulse period in seconds. Authored on most
    /// vanilla Skyrim candles + chandeliers; combined with the
    /// `Flicker` / `Pulse` (Slow) bits in `flags` and
    /// `intensity_amplitude` / `movement_amplitude` to drive the
    /// per-frame light animation. Zero when the LIGH record's DATA
    /// subrecord uses the pre-Skyrim (Oblivion/FO3/FNV) 32-byte
    /// layout, which authors no flicker parameters at all — bytes
    /// 24-31 there hold Value/Weight, not near-clip/flicker-period
    /// (#2478 / REN-D22-03). Phase 17a.
    pub period_secs: f32,
    /// Percent intensity variation (e.g. 0.25 = ±25% around the
    /// authored intensity). Read straight from bytes 32-35 of the
    /// DATA subrecord — Skyrim+ (48-byte) layout only, zero on the
    /// pre-Skyrim 32-byte layout (#2478 / REN-D22-03). Phase 17a.
    pub intensity_amplitude: f32,
    /// Position jitter amplitude in Bethesda units (BU). The
    /// flicker system offsets the light's local translation by a
    /// noise vector scaled by this value. Phase 17a.
    pub movement_amplitude: f32,
    /// LIGH DATA bytes 16-19 — Bethesda's per-light attenuation curve
    /// exponent. The runtime formula is approximately
    /// `atten = clamp(1 - (d/r)^k, 0, 1)` where `k = falloff_exponent`:
    ///   * `k ≈ 1.0` (Skyrim default): near-linear, gentle near radius
    ///   * `k ≈ 2.0` (FO3/FNV common): quadratic, sharper edge
    ///   * `k → 0`:  almost flat in the middle, very sharp cutoff
    ///
    /// Pre-fix this field was dropped on the floor and the shader
    /// used a hardcoded `1/(1 + 0.01*d)` linear term that ignored
    /// the radius entirely — small-radius FO lamps appeared to "pop
    /// off" over absolute distance while large-radius Skyrim torches
    /// looked smooth, because the absolute-distance term dominated
    /// the radius-relative one at small radii. `0.0` is the engine
    /// sentinel for "use default" (1.0 for Skyrim layout, 2.0 for
    /// pre-Skyrim) so downstream consumers can pick a sensible fall-
    /// through without re-reading the LIGH bytes.
    pub falloff_exponent: f32,
    /// #2439 (NIFAL-D2-01) — LIGH DATA bytes 20-23 (both the pre-Skyrim
    /// 32-byte and Skyrim+ 48-byte layouts share this offset — see
    /// `support.rs`'s DATA-arm layout comment) / DAT2 offset 20
    /// (Starfield). The FULL spotlight cone angle in DEGREES — verified
    /// against xEdit's `Core/wbDefinitionsTES5.pas` (dev-4.1.6):
    /// `wbFloat('FOV', ..., wbNormalizeToRange(0.001, 160), 90)`, a
    /// [0.001°, 160°] full-angle range with a 90° default. `0.0` means
    /// "not present in this subrecord" (too-short DATA); the
    /// translation boundary (`byroredux/src/systems/light_anim.rs::
    /// translate_light`) falls back to the xEdit default (90°) rather
    /// than treating `0.0` as an authored zero-degree cone. Meaningful
    /// only when `LIGHT_FLAG_SPOT` is set in `flags` — otherwise unread.
    pub fov_degrees: f32,
    /// FO4 `XPWR` powered-state FormID — references the circuit node
    /// this light connects to (Sanctuary fuse boxes, Vault 111
    /// breaker-panel switch). `None` on every non-FO4 record and on
    /// FO4 records that don't ship XPWR.
    ///
    /// Pre-work capture only (#602 / FO4-DIM6-07): the settlement-
    /// circuit ECS system that consumes this doesn't exist yet, so
    /// the field rides through until that lands. Without capture the
    /// FormID would be silently dropped at cell-load time and every
    /// wired light would render always-on. Conservative 4-byte read
    /// treats the sub-record payload as a bare FormID — the UESP FO4
    /// reference shows XPWR may carry an extra u32 trailer on some
    /// records; that trailer isn't needed for circuit lookup and is
    /// captured as `None` rather than over-reading.
    pub xpwr_form_id: Option<u32>,
    /// #4433's evidence predecessor — Starfield `DAT2+56` "Light Type"
    /// enum (`wbDefinitionsSF1.pas`, dev-4.1.6: `0 = Omnidirectional`,
    /// `1 = Shadow Spotlight`, `2 = NonShadow Spotlight`). Starfield moved
    /// the light-shape choice out of the u16 flags (whose bit 9 is
    /// "Focus Spotlight Beam", NOT a spot-shape bit) into this dedicated
    /// byte, so this field — not `flags & LIGHT_FLAG_SPOT` — is the
    /// Starfield spot-shape authority. `0` on every non-Starfield record
    /// (the `DATA` arm never sets it), which is also the enum's
    /// Omnidirectional value, so the field can ride through generically
    /// and only the Starfield arm of `translate_light` consults it.
    pub starfield_light_type: u8,
}

/// FO4/Skyrim TXST decal-data sub-record (`DODT`). Fixed 36-byte
/// layout per UESP / xEdit `wbDefinitionsFO4` covering the geometry
/// authoring of decal placements (blood splatters, scorch marks,
/// posters, graffiti) emitted from a TXST. 207 of 382 vanilla
/// `Fallout4.esm` TXST records ship a DODT payload; pre-fix every
/// one of those was silently dropped at the catch-all `_ => {}` arm
/// in `parse_txst_group`. See #813 / FO4-D4-NEW-01.
///
/// #3638 (FO4-2026-08-30-D6-02) — **no consumer exists**. This struct's
/// own doc previously named a specific future milestone as the eventual
/// reader, using the wrong milestone number — `M28` in this project's
/// own tracking (`ROADMAP.md`) is the Rapier3D physics bridge, unrelated
/// to decal rendering, and no decal-consuming milestone is tracked
/// anywhere. 303 vanilla FO4 `DODT` payloads
/// (width/height/depth/shininess/parallax/flags/RGB) parse cleanly and
/// ride through unused on the `TextureSet` — the DNAM sibling from the
/// same #813/#814 pair IS consumed (`TXST_FLAG_MODEL_SPACE_NORMALS` in
/// `byroredux/src/cell_loader/refr.rs`), so this is a real, measured
/// asymmetry, not a design choice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecalData {
    pub min_width: f32,
    pub max_width: f32,
    pub min_height: f32,
    pub max_height: f32,
    pub depth: f32,
    pub shininess: f32,
    pub parallax_scale: f32,
    pub parallax_passes: u8,
    /// Decal flag byte. Bit 0 = Parallax, bit 1 = Alpha-Blending,
    /// bit 2 = Alpha-Testing, bit 3 = No Subtextures.
    pub flags: u8,
    /// Decal tint colour (RGBA, 0..=255). Multiplied with the diffuse
    /// slot at decal-pass blend time.
    pub color: [u8; 4],
}

/// Addon-node record data extracted from ADDN sub-records.
///
/// Skyrim/FO3/FNV addon nodes are particle emitters / auxiliary visual
/// effects (moth swarms, ash motes, torch flames) that placed REFRs
/// reference via `XADN` to select one of the ADDN pool. The renderer
/// doesn't yet instantiate particle systems (tracked separately), but
/// the parsed data is kept so a future particle subsystem can resolve
/// the XADN → ADDN → master-particle-cap chain. See #370.
#[derive(Debug, Clone, Copy)]
pub struct AddonData {
    /// Signed 4-byte index (`DATA`). Negative indexes are reserved by
    /// the engine; positive values key into the master particle pool.
    pub addon_index: i32,
    /// Master particle cap (`DNAM` bytes 0..2). Upper bound on the
    /// number of particle instances spawned from this addon.
    pub master_particle_cap: u16,
    /// Addon flags (`DNAM` bytes 2..4). Bit 0 = "always loaded".
    pub flags: u16,
}

/// A base form with its NIF model path (STAT, MSTT, FURN, DOOR, LIGH, NPC_, etc.).
#[derive(Debug, Clone)]
pub struct StaticObject {
    pub form_id: u32,
    pub editor_id: String,
    pub model_path: String,
    /// The base record's four-CC type (STAT, MSTT, NPC_, …). Drives
    /// [`RenderLayer`](byroredux_core::ecs::components::RenderLayer)
    /// classification at cell-load time via
    /// [`crate::record::RecordType::render_layer`]. Game-invariant —
    /// the enum value is captured at parse time and the classifier
    /// produces the same RenderLayer regardless of which game's ESM
    /// emitted it. See #renderlayer.
    pub record_type: crate::record::RecordType,
    /// Light properties (only populated for LIGH records).
    pub light_data: Option<LightData>,
    /// Addon-node properties (only populated for ADDN records). See #370.
    pub addon_data: Option<AddonData>,
    /// True when the record carries a `VMAD` sub-record (Skyrim+
    /// Papyrus VM attached-script blob). Presence flag only — full
    /// VMAD decoding (script names + property bindings) is gated on
    /// the scripting-as-ECS work tracked at M30.2 / M48; this flag
    /// at least makes the count of script-bearing records discoverable.
    /// See #369.
    pub has_script: bool,
    /// Decoded `VMAD` script attachments + property bindings (Skyrim+),
    /// mirroring [`crate::esm::records::common::CommonNamedFields::script_instance`].
    /// `None` when the record has no `VMAD` (or wasn't built by
    /// `build_static_object_from_subs`, which is the only populator today
    /// — #2663 / SCR-D7-NEW11-02). Before this field existed, the whole
    /// MODL-only world-placement family (STAT/MSTT/FURN/DOOR/LIGH/FLOR/
    /// IDLM/BNDS/ADDN/TACT) decoded `VMAD` as a presence-only flag with the
    /// payload dropped, and `Index::base_record_script_instance` had no
    /// typed map to consult even if it hadn't been — the exact sibling gap
    /// #2189 fixed for the item family. Measured impact: 42 unreachable
    /// scripted base records in `Skyrim.esm`, 442 in `Fallout4.esm`.
    pub script_instance: Option<ScriptInstanceData>,
    /// The legacy `SCRI` → SCPT form id (Oblivion / FO3 / FNV ObScript), or
    /// `0` for "no script attached". The pre-Skyrim sibling of
    /// [`script_instance`](Self::script_instance) above: that field carries
    /// Skyrim+ inline Papyrus, this one the older external-script reference.
    ///
    /// #3941 — before this field existed the same MODL-only world-placement
    /// family landed in `cells.statics` with no `script_form_id` anywhere on
    /// the type, so [`crate::esm::records::index::Index::base_record_script`]
    /// had no map to consult and returned `None` for every one of them. That
    /// is 560 scripted base records across the three ObScript games (174 FNV,
    /// 202 FO3, 184 Oblivion by raw sub-record census) reaching neither the
    /// ObScript attach lane nor the extender-compatibility census — the
    /// `SCRI` half of exactly the gap #2663 closed for `VMAD`.
    pub script_form_id: u32,
    /// True when the base record's header carries the **Visible-When-Distant**
    /// / "Has Distant LOD" flag (`0x00010000`,
    /// [`crate::esm::reader::FLAG_VISIBLE_WHEN_DISTANT`], surfaced via
    /// [`crate::esm::reader::RecordHeader::is_visible_when_distant`]). It marks a
    /// base object the world's distant-LOD baker folded into its `.bto` /
    /// `DistantLOD\*.lod` proxies. The cell loader materialises it onto the
    /// spawned placement root as a `VisibleWhenDistant` marker so a per-record
    /// full-model cull has the signal without re-reading the ESM — the runtime
    /// consumer #1731 parsed the flag for. See #1889 / EXAL §5.2.
    pub visible_when_distant: bool,
    /// The record's destructible data (`DEST` + `DSTD` stages), when authored.
    pub destruction: Option<DestructionData>,
}

/// A base object's destructible data (xEdit `wbDEST`): its health and the
/// stages it passes through as damage lowers that health.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestructionData {
    /// `DEST` health: how much damage the object takes to reach 0%.
    pub health: i32,
    /// `DSTD` stages, in authored order.
    pub stages: Vec<DestructionStage>,
}

/// One `DSTD` stage. CK *Destructible Objects*: a stage with Health Percentage
/// 90 is entered when the object reaches 90% health; its Model Damage Stage is
/// the number scripts reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DestructionStage {
    pub health_percent: u8,
    pub index: u8,
    pub model_damage_stage: u8,
    /// Bit 0 Cap Damage, 1 Disable, 2 Destroy, 3 Ignore External Dmg.
    pub flags: u8,
}

impl DestructionStage {
    pub const CAP_DAMAGE: u8 = 1 << 0;
    pub const DISABLE: u8 = 1 << 1;
    pub const DESTROY: u8 = 1 << 2;
    pub const IGNORE_EXTERNAL_DAMAGE: u8 = 1 << 3;
}

/// Full Skyrim+ Texture Set — eight named slots from a single TXST
/// record. Pre-#357 the parser kept only `diffuse` (TX00); REFR
/// XTNM/XATO/XTXR overrides referencing a TXST silently degraded normal /
/// glow / parallax / env / specular back to the host mesh's textures.
///
/// This is a canonical, named-role view of an ESM `TXST` record.  The
/// on-disk TXnn order is deliberately resolved by `parse_txst_group`; it is
/// *not* the `BSShaderTextureSet` order used by NIFs:
/// - **TX00** — diffuse / albedo
/// - **TX01** — normal / gloss
/// - **TX02** — environment mask / subsurface tint through Skyrim;
///   wrinkles in FO4/FO76
/// - **TX03** — glow
/// - **TX04** — height / parallax
/// - **TX05** — environment cubemap
/// - **TX06** — multi-layer parallax inner layer
/// - **TX07** — back-lighting / smooth specular
///
/// FO4+ TXST records often use `MNAM` (a path to a BGSM material file)
/// instead of populating the TXnn slots directly. 37 % of vanilla
/// `Fallout4.esm` TXST records (140 / 382) are MNAM-only with no TX00.
/// The BGSM parser is a separate issue; we just capture the path here.
/// See audit FO4-D4-C3 / #406.
///
/// All slots are optional; an empty path is normalized to `None`.
/// Pre-Skyrim TXST records (FO3 / FNV) author only TX00 in shipped
/// content, so the other slots will simply read as `None` there.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextureSet {
    pub diffuse: Option<String>,
    pub normal: Option<String>,
    pub glow: Option<String>,
    pub height: Option<String>,
    pub env: Option<String>,
    pub env_mask: Option<String>,
    pub inner: Option<String>,
    pub specular: Option<String>,
    /// FO4/FO76 TX02. Kept separate from the pre-FO4 environment-mask role
    /// so a shared parser cannot silently bind a wrinkle texture as an
    /// environment mask.
    pub wrinkle: Option<String>,
    /// FO4+ MNAM sub-record — path to a BGSM material file whose
    /// embedded TX00..TX07 override the direct-slot path. When present
    /// it typically replaces TX00 entirely (the 140 MNAM-only vanilla
    /// TXSTs carry no TX00 at all). Resolution against the BGSM parser
    /// is tracked separately; this field preserves the raw path so
    /// texture resolution can route through BGSM when that lands. #406.
    pub material_path: Option<String>,
    /// TXST flags (`DNAM` sub-record). FO4 ships 2 bytes (u16),
    /// Skyrim 1 byte; we capture as u16 with the Skyrim path
    /// landing in the low byte. Bit 0 = NoSpecular, bit 1 =
    /// FaceGenTinting, bit 2 (FO4) = HasModelSpaceNormals.
    /// 100 % of vanilla `Fallout4.esm` TXST records (382 / 382)
    /// ship a DNAM payload — pre-fix every flag bit was silently
    /// dropped. The renderer's normal-map decode path branches on
    /// `HasModelSpaceNormals` once it consumes this field.
    /// See #814 / FO4-D4-NEW-02.
    pub flags: u16,
    /// TXST decal-data (`DODT` sub-record). 207 of 382 vanilla
    /// `Fallout4.esm` TXST records carry a DODT payload — every
    /// decal-bearing TXST. See [`DecalData`] and #813 / FO4-D4-NEW-01.
    /// #3638 — parsed here, but no renderer consumer exists yet; see
    /// [`DecalData`]'s own doc for the measured 303-payload gap.
    pub decal_data: Option<DecalData>,
}
