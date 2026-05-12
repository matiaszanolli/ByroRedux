//! Cell, placed reference, and static object extraction from ESM files.
//!
//! Walks the GRUP tree to find interior cells, exterior cells (from WRLD),
//! their placed references (REFR + ACHR), and resolves base form IDs to
//! static/object definitions for NIF paths.

use super::reader::EsmReader;
use anyhow::Result;
use std::collections::HashMap;

mod helpers;
pub(crate) mod support;
pub(crate) mod walkers;
pub(crate) mod wrld;

// Re-exported for `parse_esm_with_load_order` (#527 fused walker).
// Internal callers (`tests`, sibling cell helpers) reach the same
// helpers via `cell::support::*` directly.
pub(crate) use support::build_static_object_from_subs;

/// Interior cell lighting from XCLL subrecord.
///
/// The base fields (ambient through fog_far) are shared across all games
/// (Oblivion, FO3, FNV, Skyrim). Skyrim extends XCLL to 92 bytes with
/// directional ambient, fog far color, light fade distances, etc. — these
/// are stored in the `Option` fields below, which are `None` for pre-Skyrim
/// games.
#[derive(Debug, Clone)]
pub struct CellLighting {
    /// Ambient light color (RGB 0–1).
    pub ambient: [f32; 3],
    /// Directional light color (RGB 0–1).
    pub directional_color: [f32; 3],
    /// Directional light rotation (Euler XY in radians, converted to direction vector).
    pub directional_rotation: [f32; 2],
    /// Fog color (RGB 0–1), from XCLL bytes 8-10.
    pub fog_color: [f32; 3],
    /// Fog near distance (game units), from XCLL bytes 12-15.
    pub fog_near: f32,
    /// Fog far distance (game units), from XCLL bytes 16-19.
    pub fog_far: f32,
    // ── Skyrim+ extended fields (92-byte XCLL) ──────────────────────
    /// Directional light fade multiplier (bytes 28-31).
    pub directional_fade: Option<f32>,
    /// Fog clip distance (bytes 32-35).
    pub fog_clip: Option<f32>,
    /// Fog power exponent (bytes 36-39).
    pub fog_power: Option<f32>,
    /// Fog far color (RGB 0–1, bytes 72-74). Separate from near fog color.
    pub fog_far_color: Option<[f32; 3]>,
    /// Maximum fog opacity (bytes 76-79).
    pub fog_max: Option<f32>,
    /// Light fade begin distance (bytes 80-83).
    pub light_fade_begin: Option<f32>,
    /// Light fade end distance (bytes 84-87).
    pub light_fade_end: Option<f32>,
    /// Directional ambient cube — one RGB triplet per cardinal axis
    /// (bytes 40-63 as 6×RGBA). The face order follows Creation Kit /
    /// niflib convention: `[+X, -X, +Y, -Y, +Z, -Z]`. Alpha bytes are
    /// discarded — vanilla Skyrim stores zero there anyway. Drives the
    /// per-cell ambient probe: ±Z asymmetry is what makes cave floors
    /// read warm while ceilings read cool without a dedicated IBL pass.
    /// Pre-#367 these 24 bytes were parsed-past-but-dropped, so every
    /// Skyrim interior used a single `ambient` color. `None` on pre-
    /// Skyrim XCLL (36 / 40 bytes).
    pub directional_ambient: Option<[[f32; 3]; 6]>,
    /// Specular color (bytes 64-67 as RGBA → RGB triplet). `None` on
    /// pre-Skyrim XCLL.
    pub specular_color: Option<[f32; 3]>,
    /// Specular alpha (byte 67). Stored separately so consumers can
    /// decide whether the RGBA packing was intentional or padding.
    pub specular_alpha: Option<f32>,
    /// Fresnel power exponent (bytes 68-71). `None` on pre-Skyrim XCLL.
    pub fresnel_power: Option<f32>,
}

/// A texture layer within a terrain quadrant.
#[derive(Debug, Clone)]
pub struct TerrainTextureLayer {
    /// LTEX form ID (landscape texture record). 0 = default dirt.
    pub ltex_form_id: u32,
    /// Layer index (0 = base, 1+ = additional blended layers).
    pub layer: u16,
    /// Alpha opacity for each vertex in the 17×17 quadrant grid (sparse).
    /// Only populated for additional layers (ATXT+VTXT), not the base (BTXT).
    /// Index: `[row * 17 + col]`, values 0.0–1.0.
    pub alpha: Option<Vec<f32>>,
}

/// Per-quadrant texture layers. Quadrants: 0=SW, 1=SE, 2=NW, 3=NE.
#[derive(Debug, Clone, Default)]
pub struct TerrainQuadrant {
    /// Base texture (BTXT). Covers the whole quadrant at full opacity.
    pub base: Option<u32>, // LTEX form ID
    /// Additional alpha-blended texture layers (ATXT+VTXT), ordered by layer index.
    pub layers: Vec<TerrainTextureLayer>,
}

/// Landscape heightmap data from a LAND record.
///
/// Each exterior cell has a 33×33 vertex grid spanning 4096×4096 game units.
/// Vertex spacing is 128 units. The first row/column overlap with neighboring
/// cells for seamless terrain stitching.
#[derive(Debug, Clone)]
pub struct LandscapeData {
    /// Decoded heightmap: 33×33 heights in game units (Z-up).
    /// Index: `[row * 33 + col]`, row 0 = south edge, col 0 = west edge.
    pub heights: Vec<f32>,
    /// Vertex normals: 33×33 × 3 bytes (X, Y, Z as unsigned bytes 0–255,
    /// mapping to -1.0–1.0 via `(b - 127) / 127`). None if VNML absent.
    pub normals: Option<Vec<u8>>,
    /// Vertex colors: 33×33 × 3 bytes (R, G, B as unsigned bytes 0–255).
    /// None if VCLR absent.
    pub vertex_colors: Option<Vec<u8>>,
    /// Texture layers per quadrant (0=SW, 1=SE, 2=NW, 3=NE).
    pub quadrants: [TerrainQuadrant; 4],
}

/// A cell (interior or exterior) with its placed object references.
#[derive(Debug)]
pub struct CellData {
    pub form_id: u32,
    pub editor_id: String,
    /// Display name from the `FULL` sub-record. On localized plugins
    /// (Skyrim+ with the TES4 `Localized` flag) the on-disk payload
    /// is a 4-byte STRINGS-table index, decoded via
    /// [`read_lstring_or_zstring`](crate::esm::records::common::read_lstring_or_zstring)
    /// into a `<lstring 0xNNNNNNNN>` placeholder until Phase 2 of
    /// #348 wires up the real `.STRINGS` loader. On non-localized
    /// plugins this is the inline cstring (FNV / FO3 / Oblivion /
    /// non-localized Skyrim mods). `None` when the cell has no FULL
    /// (most exterior tiles, debug cells). Skyrim ships FULL on most
    /// named interiors — `WhiterunBanneredMare` carries
    /// `"The Bannered Mare"`. Pre-#624 the sub-record was dropped on
    /// the catch-all match arm.
    pub display_name: Option<String>,
    pub references: Vec<PlacedRef>,
    pub is_interior: bool,
    /// Grid coordinates for exterior cells (None for interior).
    pub grid: Option<(i32, i32)>,
    /// Interior cell lighting (from XCLL subrecord).
    pub lighting: Option<CellLighting>,
    /// Landscape terrain data (from LAND record, exterior cells only).
    pub landscape: Option<LandscapeData>,
    /// Water plane height in Bethesda world units (Z-up), from the
    /// CELL record's XCLW sub-record. `None` when the cell has no
    /// water plane. Critical for flooded Ayleid ruins, sewer interiors,
    /// coastal exterior cells — omitting it makes water geometry either
    /// not render at all or clamp to Y/Z=0. Same f32 semantics across
    /// Oblivion / FO3 / FNV / Skyrim. See #397.
    pub water_height: Option<f32>,

    // ── Skyrim+ extended sub-records (#356). FormIDs are stored raw —
    // resolution against the records index happens at the consumer.
    /// Image-space modifier (XCIM, FormID). Skyrim's per-cell tone-
    /// mapping LUT / colour-grading reference. `None` if the cell
    /// doesn't override the worldspace default.
    pub image_space_form: Option<u32>,
    /// Water type (XCWT, FormID — references a WATR record on Skyrim+,
    /// an LTEX form on FNV/Oblivion). Selects the water material to
    /// use when rendering the plane at `water_height`.
    pub water_type_form: Option<u32>,
    /// Acoustic space (XCAS, FormID — references an ASPC record).
    /// Drives reverb / occlusion presets for cell audio.
    pub acoustic_space_form: Option<u32>,
    /// Music type (XCMO, FormID — references an MUSC record). Selects
    /// which music track plays while the player is in this cell.
    pub music_type_form: Option<u32>,
    /// Pre-Skyrim music-type enum (XCMT, single byte). Oblivion / FO3
    /// / FNV interior cells use this instead of the FormID-based XCMO
    /// — values are: 0 = Default, 1 = Public, 2 = Dungeon, 3 = None.
    /// `None` when the cell omits XCMT (Skyrim+ cells, or pre-Skyrim
    /// cells that fall back to the default track). Distinct from
    /// `music_type_form` because they're authored on different game
    /// generations and the consumer (audio system) needs to know
    /// which path produced the value. See #693 / O3-N-05.
    pub music_type_enum: Option<u8>,
    /// Per-cell climate override (XCCM, FormID — references a CLMT
    /// record). Skyrim+ exterior cells can override the worldspace
    /// CLMT default for that one cell; useful for boss arenas /
    /// scripted weather pockets / interior-feeling exteriors. `None`
    /// means inherit from worldspace. Pre-Skyrim cells don't ship
    /// XCCM. See #693 / O3-N-05.
    pub climate_override: Option<u32>,
    /// Location (XLCN, FormID — references an LCTN record). Used by
    /// quest / Story Manager systems for "player is in location X"
    /// conditions.
    pub location_form: Option<u32>,
    /// Region list (XCLR, FormID array — each entry references a REGN
    /// record). Empty when the cell isn't tagged with any regions.
    /// Regions drive ambient SFX, weather overrides, and encounter
    /// tables in worldspace cells.
    pub regions: Vec<u32>,
    /// Lighting template (LTMP, FormID — references an LGTM record).
    /// Skyrim+ cells that omit XCLL inherit lighting from this template
    /// (Solitude inn cluster, Dragonsreach throne room, Markarth cells
    /// all rely on this fallback). Pre-#566 the link was unparsed and
    /// every template-only cell rendered with the engine default
    /// ambient. The cell loader resolves through `EsmIndex.lighting_templates`
    /// when `lighting.is_none() && lighting_template_form.is_some()`.
    /// See SK-D6-02.
    pub lighting_template_form: Option<u32>,
    /// Cell ownership tuple (XOWN / XRNK / XGLB sub-records). When
    /// `Some`, a stealing-detection / property-crime gameplay layer
    /// can resolve the owner (NPC_ or FACT FormID), the minimum
    /// faction rank required to bypass the ownership check, and an
    /// optional global-variable FormID that gates ownership at
    /// runtime. Absent on most cells (public spaces). Same shape on
    /// CELL and REFR records — the REFR-side override scopes
    /// ownership to a single placed object (chest, bed) rather than
    /// the whole cell. Cross-game (Oblivion / FO3 / FNV / Skyrim+).
    /// Parsed for completeness; the gameplay consumer is M47 ECS
    /// runtime work. See #692.
    pub ownership: Option<CellOwnership>,
}

/// Ownership tuple from `XOWN` / `XRNK` / `XGLB` sub-records. Lives
/// on both CELL records (whole-cell ownership) and REFR records
/// (per-placed-object override). All three fields can appear
/// independently — XRNK and XGLB are gates on top of the base XOWN
/// owner, but Bethesda content also ships XOWN-only cells (no rank
/// requirement, no global gate). See #692 / O3-N-04.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellOwnership {
    /// XOWN — owner FormID. References either an NPC_ (individual
    /// owner like a homeowner) or a FACT (faction-owned, like the
    /// Imperial Legion barracks). 0 / `None` would be unowned.
    pub owner_form_id: u32,
    /// XRNK — minimum faction rank required to bypass the ownership
    /// check. Only meaningful when `owner_form_id` references a FACT;
    /// `None` means no rank gate (any faction member counts).
    pub faction_rank: Option<i32>,
    /// XGLB — global variable FormID that controls ownership state
    /// at runtime. When set, the gameplay layer evaluates the global
    /// before applying ownership (e.g. quest-gated houses that the
    /// player can claim only after a stage). `None` means the
    /// ownership is always-active.
    pub global_var_form_id: Option<u32>,
}

/// A placed object reference within a cell (REFR or ACHR).
#[derive(Debug, Clone)]
pub struct PlacedRef {
    pub base_form_id: u32,
    /// Position in Bethesda units (Z-up).
    pub position: [f32; 3],
    /// Euler rotation in radians (X, Y, Z).
    pub rotation: [f32; 3],
    pub scale: f32,
    /// Enable-parent gating from the REFR's `XESP` sub-record. When
    /// `Some`, the REFR's spawn visibility is controlled by another
    /// REFR's enable state (and optionally inverted). `None` means the
    /// REFR is unconditionally placed at cell-load time.
    ///
    /// The cell loader uses this to skip REFRs that are default-
    /// disabled (XESP set + not inverted) — pre-#349 every "spawn after
    /// quest stage" REFR rendered immediately on cell load. See #349.
    pub enable_parent: Option<EnableParent>,
    /// Teleport destination from the REFR's `XTEL` sub-record. When
    /// present, this REFR is a door whose activation transports the
    /// player to the destination REFR's position and rotation. Pre-#412
    /// every interior door was dead on activation. See audit FO4-D6-B1.
    pub teleport: Option<TeleportDest>,
    /// Primitive bounds / shape from the REFR's `XPRM` sub-record.
    /// Trigger volumes and invisible activators carry no MODL and are
    /// defined entirely by this primitive (box / sphere / plane / line).
    /// Pre-#412 triggers were invisible in the editor view and had no
    /// activation volume at runtime.
    pub primitive: Option<PrimitiveBounds>,
    /// Linked refs from the REFR's `XLKR` sub-records. Multiple allowed
    /// per REFR. Each pair is `(keyword_form_id, target_ref_form_id)`;
    /// the keyword (`null` if untyped) tags the relationship kind (e.g.
    /// NPC ↔ patrol marker, door ↔ teleport partner, switch ↔ light).
    /// Pre-#412 NPCs didn't patrol and doors didn't pair.
    pub linked_refs: Vec<LinkedRef>,
    /// Room membership form IDs from the REFR's `XRMR` sub-record —
    /// the room(s) this ref belongs to for FO4 cell-subdivided interior
    /// culling. Empty when the REFR is not room-scoped. See #412.
    pub rooms: Vec<u32>,
    /// Portal connections from the REFR's `XPOD` sub-record. Each pair
    /// is `(origin_room_ref, destination_room_ref)`. Portal REFRs gate
    /// the interior room-to-room visibility graph. Pre-#412 this was
    /// silently dropped so any vault using portals couldn't cull.
    pub portals: Vec<PortalLink>,
    /// LIGH radius override from the REFR's `XRDS` sub-record. When
    /// `Some`, overrides the base LIGH record's radius for per-ref
    /// light tuning (bulb placement, lantern fine-tune).
    pub radius_override: Option<f32>,
    /// Alternate texture set from the REFR's `XATO` sub-record — a
    /// TXST FormID that overrides the base mesh's NIF-authored textures
    /// at this placement. Common on FO4 weapons / armor / signage that
    /// share one model but ship multiple TXST variants. The cell loader
    /// resolves this against `EsmCellIndex.texture_sets` and overlays
    /// the 8 TX00–TX07 slots (plus MNAM → BGSM chain) onto the spawned
    /// mesh. Pre-#584 37 % of vanilla FO4 TXSTs (140 / 382) were parsed
    /// but never consumed because the REFR parser dropped this field.
    /// See audit FO4-DIM6-02.
    pub alt_texture_ref: Option<u32>,
    /// Land-scoped TXST override from the REFR's `XTNM` sub-record —
    /// a TXST FormID that overrides an LTEX default on landscape
    /// references. Wire layout is identical to XATO (single u32 FormID)
    /// but targets LAND records, not models. Parsed for completeness;
    /// the LAND-override consumer path is separate from the mesh path
    /// wired for XATO in this issue. See audit FO4-DIM6-02.
    pub land_texture_ref: Option<u32>,
    /// Per-slot texture swaps from the REFR's `XTXR` sub-records. Each
    /// pair is `(txst_form_id, slot_index)` where `slot_index` picks
    /// one of TX00..TX07 — letting a REFR override a single slot of
    /// the base mesh's texture set without replacing the whole TXST.
    /// Multiple XTXR sub-records allowed per REFR; the cell loader
    /// applies them in authoring order (later wins for the same slot).
    pub texture_slot_swaps: Vec<TextureSlotSwap>,
    /// LIGH FormID attached via the REFR's `XEMI` sub-record — a
    /// per-placement emissive light that the renderer should spawn on
    /// top of (or instead of) the base LIGH. Parsed for completeness;
    /// the consumer wiring (per-REFR emissive light spawn) is follow-up
    /// work. See audit FO4-DIM6-02.
    pub emissive_light_ref: Option<u32>,
    /// MSWP FormID attached via the REFR's `XMSP` sub-record — a
    /// material-swap table the cell loader resolves against
    /// `EsmCellIndex.material_swaps` to produce per-REFR BGSM/BGEM
    /// substitutions on the base mesh's authored material slots.
    /// Pre-#971 every Raider armour colour-variant, settlement clutter
    /// variation, station-wagon rust pattern, and Vault decay overlay
    /// rendered with the base mesh's textures because this field was
    /// silently dropped at parse time. See audit FO4-D4-NEW-08.
    pub material_swap_ref: Option<u32>,
    /// Per-REFR ownership override (XOWN / XRNK / XGLB). When `Some`,
    /// the placed object (chest, bed, individual storage) is owned
    /// independently of the parent cell's ownership — stealing the
    /// item is a crime even in a public cell. See `CellOwnership`
    /// docs and #692.
    pub ownership: Option<CellOwnership>,
}

/// Per-slot texture swap from one `XTXR` sub-record — a TXST form ID
/// paired with the index of the slot (0..=7) it replaces on the host
/// mesh's texture set. Multiple XTXR sub-records allowed per REFR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureSlotSwap {
    /// TXST FormID supplying the replacement texture.
    pub texture_set: u32,
    /// Slot index to replace — 0 = TX00 (diffuse), 1 = TX01 (normal),
    /// … 7 = TX07 (specular). Values outside 0..=7 are clamped away at
    /// consumer side so a malformed XTXR can't crash the loader.
    pub slot_index: u32,
}

/// Teleport destination payload for `XTEL` — a door ref's target.
///
/// Wire layout (32 bytes): destination_ref(u32) + position(3×f32) +
/// rotation(3×f32) + flags(u32). Flags trail on Skyrim+ and may be
/// absent on older games; parsed conservatively to the first 28 bytes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TeleportDest {
    /// FormID of the destination REFR (another door, usually) in the
    /// target cell. The cell loader resolves this to coordinates and
    /// the player's teleport target.
    pub destination: u32,
    /// Destination position in Bethesda units (Z-up).
    pub position: [f32; 3],
    /// Destination Euler rotation in radians (X, Y, Z).
    pub rotation: [f32; 3],
}

/// Primitive shape payload for `XPRM` — invisible trigger / activator
/// geometry that doesn't ship a MODL. Conservative layout: first three
/// f32s are box bounds, next three are the editor visualization color,
/// then a trailing f32 + u32 (shape type enum). All in 32 bytes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrimitiveBounds {
    /// Box / shape extents in Bethesda units.
    pub bounds: [f32; 3],
    /// Editor visualization color (R, G, B) in 0..=1 range. Mostly
    /// useful for debug rendering; preserved so a future editor view
    /// can display the exact tint the original author chose.
    pub color: [f32; 3],
    /// Trailing f32 per UESP — semantic unclear (possibly an alpha or
    /// a second-axis parameter). Preserved verbatim for future use.
    pub unknown: f32,
    /// Shape type enum per UESP: 1=Box, 2=Line, 3=Sphere, 4=Portal,
    /// 5=Plane. Other values indicate a game-specific extension.
    pub shape_type: u32,
}

/// Linked-ref pair from a single `XLKR` sub-record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinkedRef {
    /// Keyword form ID categorizing the link (e.g. `LinkCarryable`,
    /// `LinkPatrol`). `0` / `0xFFFFFFFF` means untyped.
    pub keyword: u32,
    /// Target REFR form ID the keyword links this REFR to.
    pub target: u32,
}

/// Portal pair from a single `XPOD` sub-record. Portal REFRs connect
/// two rooms for FO4 / Skyrim-SE interior cell-subdivision culling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PortalLink {
    pub origin: u32,
    pub destination: u32,
}

/// REFR enable-parent gating from the `XESP` sub-record (Skyrim+).
/// Layout: 4-byte parent FormID + 1-byte flags. Bit 0 of the flags
/// inverts the enable state (so `inverted = true` means the REFR is
/// visible when the parent is *disabled*).
#[derive(Debug, Clone, Copy)]
pub struct EnableParent {
    /// FormID of the parent REFR whose enable state controls this
    /// REFR's visibility.
    pub form_id: u32,
    /// When `true`, this REFR is visible when the parent is disabled
    /// (and hidden when the parent is enabled).
    pub inverted: bool,
}

impl EnableParent {
    /// Returns `true` if the REFR should be skipped at cell-load time
    /// because its parent's assumed initial state would hide it.
    ///
    /// Interim predicate (#471): without a two-pass loader that can
    /// consult the parent REFR's own "initial disabled" flag
    /// (bit 0x0800), we assume the common case — parents are
    /// persistent, always-enabled actors / statics. Under that
    /// assumption:
    ///   - non-inverted XESP: child visible iff parent enabled → render
    ///   - inverted XESP:     child visible iff parent disabled → skip
    ///
    /// #349's original predicate had the sense flipped and hid every
    /// non-inverted XESP child, wiping out most quest-enabled clutter,
    /// shop inventory, and patrol markers on FNV / FO3 cells. False
    /// negatives on the rarer "supposed to be hidden" XESP chains are
    /// visually less bad than wholesale invisibility.
    ///
    /// The long-term fix is a two-pass loader that builds a REFR flag
    /// table first, then applies XESP gating against each parent's real
    /// 0x0800 bit.
    pub fn default_disabled(self) -> bool {
        self.form_id != 0 && self.inverted
    }
}

/// Light-specific data extracted from LIGH record DATA subrecord.
#[derive(Debug, Clone)]
pub struct LightData {
    pub radius: f32,
    pub color: [f32; 3],
    pub flags: u32,
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
}

/// FO4/Skyrim TXST decal-data sub-record (`DODT`). Fixed 36-byte
/// layout per UESP / xEdit `wbDefinitionsFO4` covering the geometry
/// authoring of decal placements (blood splatters, scorch marks,
/// posters, graffiti) emitted from a TXST. 207 of 382 vanilla
/// `Fallout4.esm` TXST records ship a DODT payload; pre-fix every
/// one of those was silently dropped at the catch-all `_ => {}` arm
/// in `parse_txst_group`. See #813 / FO4-D4-NEW-01.
///
/// Renderer-side decal rendering (`RenderLayer::Decal`) consumes the
/// width / depth / parallax / colour fields once the M28 decal
/// pipeline extension lands; until then the parsed payload rides
/// through unused on the `TextureSet`.
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
}

/// Full Skyrim+ Texture Set — eight named slots from a single TXST
/// record. Pre-#357 the parser kept only `diffuse` (TX00); REFR
/// XTNM/XPRD overrides referencing a TXST silently degraded normal /
/// glow / parallax / env / specular back to the host mesh's textures.
///
/// Slot meanings per `docs/legacy/nif.xml` and the Skyrim Creation Kit:
/// - **TX00** — diffuse / albedo
/// - **TX01** — normal / tangent space
/// - **TX02** — glow / skin / detail (Skyrim shader-type-dependent)
/// - **TX03** — height / parallax
/// - **TX04** — environment cubemap
/// - **TX05** — environment mask
/// - **TX06** — multi-layer parallax inner layer
/// - **TX07** — specular / back-lighting
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
    pub decal_data: Option<DecalData>,
}

/// Decoded fields from a WRLD (worldspace) record. Captures the
/// exterior-render-critical bits that pre-#965 fell on the catch-all
/// `_ => {}` arm of `parse_wrld_group`: parent worldspace, usable
/// cell-grid bounds, default water / music / map references, and the
/// 1-byte flags byte.
///
/// Wire layout (cross-game, sub-record dispatch):
/// - `EDID` — editor ID (zstring, lowercased into the index key)
/// - `WNAM` — parent worldspace FormID (u32). `None` on root
///   worldspaces (Tamriel, Wasteland, Tamriel-of-Skyrim) since the
///   sub-record is omitted entirely; populated on derived
///   worldspaces (Shivering Isles → Tamriel, Solstheim → Skyrim).
/// - `PNAM` — parent-use flags (u16). Bit set means the field is
///   inherited from the parent worldspace: 0x01 Land, 0x02 LOD,
///   0x04 Map, 0x08 Water, 0x10 Climate, 0x20 Imagespace (pre-TES5),
///   0x40 SkyCell. Absent on TES4 (Oblivion).
/// - `NAM0` — object bounds south-west corner (2 × f32 in Bethesda
///   world units, Z-up). Disk-sampled on `Oblivion.esm` Tamriel:
///   `-262144.0, -253952.0` ≈ cell `(-64, -62)`. The audit text
///   originally called these "i32 cell-grid pairs"; the wire form
///   is float world units per UESP and xEdit. The cell loader can
///   convert to cell coords via `floor(value / 4096.0)`.
/// - `NAM9` — object bounds north-east corner (2 × f32, same units).
/// - `NAM2` — default water FormID (u32, WATR on FO3+/Skyrim,
///   LTEX/WATR on Oblivion).
/// - `ICON` — map texture path (zstring), the worldspace pause-menu
///   map background.
/// - `DATA` — worldspace flags byte (u8): 0x01 small-world, 0x02
///   can't fast travel, 0x04 no LOD water, 0x08 no landscape, 0x10
///   no sky, 0x20 fixed dimensions, 0x40 no grass.
/// - `ZNAM` — default music FormID (u32, MUSC record).
///
/// Sub-records not consumed here (parsed-past by the walker so the
/// next record still aligns): `WCTR` (TES5 centre cell), `MNAM` (map
/// camera data), `DNAM` (default land/water height for streaming),
/// `NAM3/NAM4` (LOD water type/height), `RNAM` (region overrides),
/// `OFST` (per-cell LAND offset table — perf optimisation for
/// streaming, can land in a follow-up). See OpenMW reference
/// `components/esm4/loadwrld.cpp` and audit OBL-D3-NEW-01 / #965.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorldspaceRecord {
    /// FormID of the WRLD record itself.
    pub form_id: u32,
    /// EDID — preserved verbatim. The index keys by `.to_ascii_lowercase()`.
    pub editor_id: String,
    /// Object-bounds SW corner (NAM0) in Bethesda world units (Z-up).
    /// Defaults to `(0.0, 0.0)` when absent. Combined with
    /// `usable_max` to bound the exterior iteration radius in the
    /// cell loader; use `usable_cell_bounds()` to convert to
    /// inclusive cell-grid coordinates.
    pub usable_min: (f32, f32),
    /// Object-bounds NE corner (NAM9) in Bethesda world units. When
    /// both `usable_min` and `usable_max` are the default `(0.0,
    /// 0.0)`, the consumer should fall back to the explicit cells
    /// map rather than iterate an empty rectangle.
    pub usable_max: (f32, f32),
    /// Parent worldspace FormID (WNAM). `None` on root worldspaces.
    pub parent_worldspace: Option<u32>,
    /// Parent-use flags (PNAM). Bits select which fields inherit
    /// from the parent worldspace. Zero on TES4 (no PNAM authored).
    pub parent_flags: u16,
    /// Default music FormID (ZNAM, MUSC record). `None` when the
    /// worldspace defers to the DefaultObjectManager music.
    pub default_music: Option<u32>,
    /// Default water FormID (NAM2). `None` when the worldspace has
    /// no authored default water plane.
    pub water_form: Option<u32>,
    /// Worldspace map-texture path (ICON). Empty when not authored.
    pub map_texture: String,
    /// Worldspace flags byte (DATA). See struct docs for bit layout.
    pub flags: u8,
}

impl WorldspaceRecord {
    /// One Bethesda exterior cell spans 4096 world units on each
    /// side. The cell loader keys exterior_cells by `(i32, i32)`
    /// cell-grid coordinates; this helper turns the f32 world-unit
    /// `usable_min`/`usable_max` into the inclusive grid rectangle
    /// the loader can iterate. Returns `None` when both corners are
    /// at the default `(0.0, 0.0)` (no NAM0/NAM9 authored — the
    /// consumer should fall back to the explicit `exterior_cells`
    /// keys).
    pub fn usable_cell_bounds(&self) -> Option<((i32, i32), (i32, i32))> {
        if self.usable_min == (0.0, 0.0) && self.usable_max == (0.0, 0.0) {
            return None;
        }
        const CELL_SIZE: f32 = 4096.0;
        let min = (
            (self.usable_min.0 / CELL_SIZE).floor() as i32,
            (self.usable_min.1 / CELL_SIZE).floor() as i32,
        );
        let max = (
            (self.usable_max.0 / CELL_SIZE).floor() as i32,
            (self.usable_max.1 / CELL_SIZE).floor() as i32,
        );
        Some((min, max))
    }
}

/// Result of parsing an ESM file for cell loading.
#[derive(Debug, Default)]
pub struct EsmCellIndex {
    /// Interior cells, keyed by editor ID (lowercase).
    pub cells: HashMap<String, CellData>,
    /// Exterior cells, keyed by (worldspace_name_lowercase, (grid_x, grid_y)).
    pub exterior_cells: HashMap<String, HashMap<(i32, i32), CellData>>,
    /// All base object records with model paths, keyed by form ID.
    pub statics: HashMap<u32, StaticObject>,
    /// Landscape texture definitions: LTEX form ID → diffuse texture path.
    /// Resolved via LTEX.TNAM → TXST.TX00 (FO3+) or LTEX.ICON (Oblivion).
    pub landscape_textures: HashMap<u32, String>,
    /// Full decoded WRLD records, keyed by lowercased EDID. The
    /// `worldspace_climates` map below is preserved for back-compat
    /// with the cell loader's CLMT lookup; `worldspaces` is the
    /// canonical exterior-render entry point and carries every other
    /// authored field (parent, bounds, flags, water, music, map). See
    /// audit OBL-D3-NEW-01 / #965.
    pub worldspaces: HashMap<String, WorldspaceRecord>,
    /// Worldspace climate form IDs: worldspace_name_lowercase → CLMT form ID.
    /// Extracted from the WRLD record's CNAM sub-record.
    pub worldspace_climates: HashMap<String, u32>,
    /// Full TXST records keyed by form ID — all 8 texture slots
    /// (diffuse/normal/glow/parallax/env/env_mask/inner/specular).
    /// Pre-#357 only the diffuse slot was retained (via the
    /// `landscape_textures` LTEX→TXST.TX00 path), so any future REFR
    /// XTNM/XPRD override that points to a TXST can now apply the
    /// full set instead of silently dropping 7 of 8 channels. See
    /// audit S6-11.
    pub texture_sets: HashMap<u32, TextureSet>,
    /// FO4+ SCOL (Static Collection) records keyed by form ID —
    /// each packages N child base forms + per-child placement arrays.
    /// Pre-#405 every SCOL was routed through the MODL-only parser
    /// and the 15,878 ONAM/DATA placement entries in vanilla
    /// Fallout4.esm were silently discarded. Vanilla rendering
    /// mostly worked via the cached combined mesh at
    /// `SCOL\Fallout4.esm\CM*.NIF`, but mod-added SCOLs (which
    /// rarely ship a CM-file — previsibine is author-gated) rendered
    /// as nothing. The cell loader expands an SCOL REFR into N
    /// synthetic placed refs when the cached NIF is unavailable.
    /// See audit FO4-D4-C2.
    pub scols: HashMap<u32, super::records::ScolRecord>,
    /// FO4+ pack-in (PKIN) bundles keyed by form ID. PKIN records
    /// group LVLI / CONT / STAT / MSTT children via `CNAM` sub-records
    /// so level designers can drop a reusable "generic workbench loot"
    /// bundle as a single REFR. The cell loader expands a PKIN REFR
    /// into one synthetic placement per `contents` entry at the outer
    /// REFR's transform (analogous to the SCOL expansion in
    /// `expand_scol_placements`). Pre-#589 all 872 vanilla Fallout4.esm
    /// PKIN records were silently dropped because they were routed
    /// through the MODL-only parser (PKIN carries no MODL).
    /// See audit FO4-DIM4-03 / #589.
    pub packins: HashMap<u32, super::records::PkinRecord>,
    /// FO4+ MOVS (Movable Static) records keyed by form ID. Visually
    /// identical to STAT (one `MODL` pointer); MOVS distinguishes
    /// itself by being driven by Havok at runtime — the referenced
    /// mesh's `bhk` collision chain produces dynamic rigid-body motion
    /// when the cell is alive. Pre-#588 MOVS was routed through the
    /// MODL-only catch-all alongside STAT/FURN/etc., which preserved
    /// visual placement (REFRs targeting MOVS form IDs still rendered
    /// the right mesh) but silently dropped the distinguishing
    /// `LNAM` / `ZNAM` / `DEST` / `VMAD` sub-records. The dedicated
    /// parser captures those into [`MovableStaticRecord`](super::records::MovableStaticRecord)
    /// here pending physics / sound / destruction subsystems.
    ///
    /// Vanilla `Fallout4.esm` itself ships zero MOVS records — the
    /// impact is on DLC / mod content authoring breakable furniture,
    /// deployable workshop objects, and physics-puzzle props. See
    /// audit `FO4-DIM4-02` / #588.
    pub movables: HashMap<u32, super::records::MovableStaticRecord>,
    /// FO4+ MSWP (Material Swap) records keyed by form ID. Each
    /// authors a list of `(source.bgsm/.bgem → target.bgsm/.bgem,
    /// optional intensity)` substitutions plus an optional path-prefix
    /// filter. REFR `XMSP` sub-records (FO4-DIM6-02 stage 2) point at
    /// an MSWP form ID and the cell loader looks the table up here to
    /// produce per-REFR `TextureSlotSwap` overrides. Without MSWP every
    /// vanilla Raider armour, settlement clutter colour-variant, and
    /// Vault decay overlay renders identically across REFRs.
    ///
    /// Vanilla `Fallout4.esm` ships ~2,500 MSWP records — see audit
    /// FO4-DIM6-05 / #590.
    pub material_swaps: HashMap<u32, super::records::MaterialSwapRecord>,
}

impl EsmCellIndex {
    /// Merge `other` into `self` with last-write-wins semantics on
    /// every map. Mirrors `EsmIndex::merge_from`'s contract — the
    /// caller parses plugins in load order and folds each into the
    /// running index.
    ///
    /// Interior cells (`HashMap<String, _>`) and exterior_cells
    /// (`HashMap<String, HashMap<(i32,i32), _>>`) need slightly
    /// different treatment than the flat record maps:
    ///
    /// * Interior cells just `extend` — DLC redefining a base cell
    ///   wins the entire CellData (REFRs, lighting, water level).
    /// * Exterior cells merge per-worldspace so a DLC adding a new
    ///   worldspace doesn't stomp the base game's exterior table.
    ///   Within a shared worldspace, per-(x, y) overrides apply.
    ///
    /// See M46.0 / #561.
    pub fn merge_from(&mut self, other: EsmCellIndex) {
        self.cells.extend(other.cells);

        for (worldspace, grids) in other.exterior_cells {
            self.exterior_cells
                .entry(worldspace)
                .or_default()
                .extend(grids);
        }

        self.statics.extend(other.statics);
        self.landscape_textures.extend(other.landscape_textures);
        self.worldspaces.extend(other.worldspaces);
        self.worldspace_climates.extend(other.worldspace_climates);
        self.texture_sets.extend(other.texture_sets);
        self.scols.extend(other.scols);
        self.packins.extend(other.packins);
        self.movables.extend(other.movables);
        self.material_swaps.extend(other.material_swaps);
    }
}

/// Parse an ESM file and extract cells, worldspaces, and base object definitions.
pub fn parse_esm_cells(data: &[u8]) -> Result<EsmCellIndex> {
    parse_esm_cells_with_load_order(data, None)
}

/// Parse an ESM file's cells with an explicit FormID load-order remap.
///
/// Pass `None` for single-plugin loads (the default). Pass
/// `Some(FormIdRemap { ... })` when loading a DLC / mod in a
/// multi-plugin stack so record FormIDs get rewritten to global
/// load-order indices before they land in `EsmCellIndex` maps. Without
/// this remap Anchorage's static 0x01002345 and BrokenSteel's
/// 0x01002345 would collide in `statics`. See #445.
pub fn parse_esm_cells_with_load_order(
    data: &[u8],
    remap: Option<super::reader::FormIdRemap>,
) -> Result<EsmCellIndex> {
    // #527 — fused single-pass walker. Pre-fix this function was the
    // first of two top-level walks over the whole ESM byte slice; the
    // typed-records walker in `super::records::parse_esm_with_load_order`
    // ran the same loop again to populate items / containers / NPCs
    // / etc. The fused walker now lives in records/mod.rs and produces
    // both maps in one pass — this entry point becomes a thin wrapper
    // that discards the typed maps for callers that only want cells.
    super::records::parse_esm_with_load_order(data, remap).map(|i| i.cells)
}

#[cfg(test)]
mod tests;
