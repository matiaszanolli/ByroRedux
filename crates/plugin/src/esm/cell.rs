//! Cell, placed reference, and static object extraction from ESM files.
//!
//! Walks the GRUP tree to find interior cells, exterior cells (from WRLD),
//! their placed references (REFR + ACHR), and resolves base form IDs to
//! static/object definitions for NIF paths.

use super::reader::EsmReader;
use anyhow::{Context, Result};
use std::collections::HashMap;

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
    /// Location (XLCN, FormID — references an LCTN record). Used by
    /// quest / Story Manager systems for "player is in location X"
    /// conditions.
    pub location_form: Option<u32>,
    /// Region list (XCLR, FormID array — each entry references a REGN
    /// record). Empty when the cell isn't tagged with any regions.
    /// Regions drive ambient SFX, weather overrides, and encounter
    /// tables in worldspace cells.
    pub regions: Vec<u32>,
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
        self.worldspace_climates.extend(other.worldspace_climates);
        self.texture_sets.extend(other.texture_sets);
        self.scols.extend(other.scols);
        self.packins.extend(other.packins);
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
    let mut reader = EsmReader::new(data);
    if let Some(r) = remap {
        reader.set_form_id_remap(r);
    }
    let file_header = reader
        .read_file_header()
        .context("Failed to read ESM file header")?;

    log::info!(
        "ESM file: {} records, {} master files",
        file_header.record_count,
        file_header.master_files.len(),
    );

    let mut cells = HashMap::new();
    let mut exterior_cells: HashMap<String, HashMap<(i32, i32), CellData>> = HashMap::new();
    let mut statics = HashMap::new();
    let mut landscape_textures: HashMap<u32, String> = HashMap::new();
    let mut worldspace_climates: HashMap<String, u32> = HashMap::new();
    // First pass collects TXST form IDs; second resolves LTEX → TXST → path.
    let mut txst_textures: HashMap<u32, String> = HashMap::new();
    let mut texture_sets: HashMap<u32, TextureSet> = HashMap::new();
    let mut ltex_to_txst: HashMap<u32, u32> = HashMap::new();
    let mut scols: HashMap<u32, super::records::ScolRecord> = HashMap::new();
    let mut packins: HashMap<u32, super::records::PkinRecord> = HashMap::new();
    let mut material_swaps: HashMap<u32, super::records::MaterialSwapRecord> = HashMap::new();

    // Walk top-level groups.
    while reader.remaining() > 0 {
        if !reader.is_group() {
            let header = reader.read_record_header()?;
            reader.skip_record(&header);
            continue;
        }

        let group = reader.read_group_header()?;

        match &group.label {
            b"CELL" => {
                let end = reader.group_content_end(&group);
                parse_cell_group(&mut reader, end, &mut cells)?;
            }
            b"WRLD" => {
                let end = reader.group_content_end(&group);
                parse_wrld_group(
                    &mut reader,
                    end,
                    &mut exterior_cells,
                    &mut worldspace_climates,
                )?;
            }
            b"LTEX" => {
                let end = reader.group_content_end(&group);
                parse_ltex_group(&mut reader, end, &mut ltex_to_txst, &mut landscape_textures)?;
            }
            b"TXST" => {
                let end = reader.group_content_end(&group);
                parse_txst_group(&mut reader, end, &mut txst_textures, &mut texture_sets)?;
            }
            // All record types that have a MODL sub-record (NIF model path).
            // Placed references (REFR/ACHR/ACRE) can point to any of these.
            // TXST is intentionally NOT in this list — it has a dedicated
            // parser at the `b"TXST"` arm above (line 264) that pulls
            // texture paths instead of model paths.
            //
            // CREA — Oblivion/FO3/FNV creature record (#396). FO3+ folded
            // creatures into NPC_ so the MODL match arm never needed it
            // for those games; Oblivion shipped 250+ creatures in a
            // dedicated 440 KB CREA group (goblin/rat/zombie/daedra) and
            // every Ayleid ruin / Oblivion gate / cave placement
            // referenced one. Without CREA in the statics map every
            // ACRE placement failed the base-ref lookup and silently
            // skipped rendering. CREA uses the standard MODL sub-record,
            // identical to STAT — no per-record field work needed.
            b"STAT" | b"MSTT" | b"FURN" | b"DOOR" | b"ACTI" | b"CONT" | b"LIGH" | b"MISC"
            | b"FLOR" | b"TREE" | b"AMMO" | b"WEAP" | b"ARMO" | b"BOOK" | b"KEYM" | b"ALCH"
            | b"INGR" | b"NOTE" | b"TACT" | b"IDLM" | b"BNDS" | b"ADDN" | b"TERM" | b"NPC_"
            | b"CREA" | b"MOVS" => {
                let end = reader.group_content_end(&group);
                parse_modl_group(&mut reader, end, &mut statics)?;
            }
            // SCOL — FO4+ Static Collection. Has a MODL (cached
            // combined mesh) but ALSO carries ONAM/DATA placement
            // arrays that the MODL-only parser would discard. Route
            // through a dedicated parser that captures every child
            // placement while still registering the record in the
            // `statics` map (so REFRs targeting the SCOL still find
            // its cached-mesh model_path). See audit FO4-D4-C2 / #405.
            b"SCOL" => {
                let end = reader.group_content_end(&group);
                parse_scol_group(&mut reader, end, &mut statics, &mut scols)?;
            }
            // PKIN — FO4+ Pack-In bundle. CNAM-driven, no MODL; the
            // MODL-only parser above would silently produce a
            // `StaticObject { model_path: "" }` and discard every
            // content reference. Route through a dedicated parser
            // that captures the full CNAM list into `packins`; the
            // cell loader expands PKIN REFRs into synthetic placements
            // at spawn time (analogous to SCOL). Also register a
            // nominal `StaticObject` entry so REFRs still resolve the
            // base form at load time — the empty `model_path` plus
            // PKIN presence in `packins` is the signal the cell
            // loader keys on. See audit FO4-DIM4-03 / #589.
            b"PKIN" => {
                let end = reader.group_content_end(&group);
                parse_pkin_group(&mut reader, end, &mut statics, &mut packins)?;
            }
            // MSWP — FO4+ Material Swap. Authors a list of source →
            // target material substitutions plus an optional
            // path-prefix filter. REFR `XMSP` sub-records (FO4-DIM6-02
            // stage 2) point at MSWP form IDs; the cell loader
            // resolves them here and produces per-REFR
            // `TextureSlotSwap` overrides. Pre-#590 the type was in
            // `RecordType::MSWP` but had no parser, so all 2,500
            // vanilla Fallout4.esm entries fell into the catch-all
            // `skip_group` below — every Raider armour / station-
            // wagon / vault-decay variant rendered identically.
            // See audit FO4-DIM6-05.
            b"MSWP" => {
                let end = reader.group_content_end(&group);
                parse_mswp_group(&mut reader, end, &mut material_swaps)?;
            }
            _ => {
                reader.skip_group(&group);
            }
        }
    }

    // Resolve LTEX → texture path via TXST indirection.
    // FO3/FNV: LTEX.TNAM → TXST form ID → TXST.TX00 diffuse path.
    // Oblivion: LTEX.ICON is a direct texture path (stored in landscape_textures directly).
    for (ltex_id, txst_id) in &ltex_to_txst {
        if let Some(path) = txst_textures.get(txst_id) {
            landscape_textures.insert(*ltex_id, path.clone());
        }
    }

    let total_exterior: usize = exterior_cells.values().map(|m| m.len()).sum();
    let wrld_names: Vec<&str> = exterior_cells.keys().map(|s| s.as_str()).collect();
    log::info!(
        "ESM parsed: {} interior cells, {} exterior cells across {} worldspaces, {} base objects, {} landscape textures",
        cells.len(),
        total_exterior,
        exterior_cells.len(),
        statics.len(),
        landscape_textures.len(),
    );
    if !wrld_names.is_empty() {
        log::info!("  Worldspaces: {:?}", wrld_names);
    }

    Ok(EsmCellIndex {
        cells,
        exterior_cells,
        statics,
        landscape_textures,
        worldspace_climates,
        texture_sets,
        scols,
        packins,
        material_swaps,
    })
}

/// Walk the CELL group hierarchy to find interior cells and their placed references.
fn parse_cell_group(
    reader: &mut EsmReader,
    end: usize,
    cells: &mut HashMap<String, CellData>,
) -> Result<()> {
    // Track the last parsed interior cell so we can attach children groups to it.
    let mut current_cell: Option<(u32, String)> = None;

    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub_group = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub_group);

            match sub_group.group_type {
                // Interior cell block (2) and sub-block (3): recurse.
                2 | 3 => {
                    parse_cell_group(reader, sub_end, cells)?;
                }
                // Cell children groups (6=temporary, 8=persistent, 9=visible distant).
                6 | 8 | 9 => {
                    if let Some((_, ref editor_id)) = current_cell {
                        let key = editor_id.to_ascii_lowercase();
                        let mut refs = Vec::new();
                        let mut _land = None; // Interior cells don't have LAND records
                        parse_refr_group(reader, sub_end, &mut refs, &mut _land)?;
                        if let Some(cell) = cells.get_mut(&key) {
                            cell.references.extend(refs);
                        }
                    } else {
                        reader.skip_group(&sub_group);
                    }
                }
                _ => {
                    reader.skip_group(&sub_group);
                }
            }
        } else {
            let header = reader.read_record_header()?;
            if &header.record_type == b"CELL" {
                let subs = reader.read_sub_records(&header)?;
                let mut editor_id = String::new();
                let mut is_interior = false;
                let mut lighting = None;
                let mut water_height: Option<f32> = None;
                let mut image_space_form: Option<u32> = None;
                let mut water_type_form: Option<u32> = None;
                let mut acoustic_space_form: Option<u32> = None;
                let mut music_type_form: Option<u32> = None;
                let mut location_form: Option<u32> = None;
                let mut regions: Vec<u32> = Vec::new();
                // #692 — XOWN / XRNK / XGLB ownership tuple. All three
                // sub-records optional; cell ends up with `Some` only
                // when at least XOWN is present. XRNK and XGLB without
                // XOWN are nonsensical and dropped (the consumer would
                // have nothing to gate against).
                let mut ownership_owner: Option<u32> = None;
                let mut ownership_rank: Option<i32> = None;
                let mut ownership_global: Option<u32> = None;

                for sub in &subs {
                    match &sub.sub_type {
                        b"EDID" => editor_id = read_zstring(&sub.data),
                        b"DATA" if sub.data.len() >= 1 => is_interior = sub.data[0] & 1 != 0,
                        b"XCLW" if sub.data.len() >= 4 => {
                            // XCLW: f32 water plane height in world units
                            // (Z-up). Same layout across Oblivion / FO3 / FNV
                            // / Skyrim — the cell's water surface sits at
                            // this Z (interior) or Z-in-worldspace (exterior).
                            // See #397 / #356.
                            water_height = Some(f32::from_le_bytes([
                                sub.data[0],
                                sub.data[1],
                                sub.data[2],
                                sub.data[3],
                            ]));
                        }
                        // Skyrim extended CELL sub-records (#356). Each is
                        // a 4-byte FormID; the walker previously dropped
                        // them on the `_` arm so the renderer / audio /
                        // quest system had no per-cell context.
                        b"XCIM" => image_space_form = read_form_id(&sub.data),
                        b"XCWT" => water_type_form = read_form_id(&sub.data),
                        b"XCAS" => acoustic_space_form = read_form_id(&sub.data),
                        b"XCMO" => music_type_form = read_form_id(&sub.data),
                        b"XLCN" => location_form = read_form_id(&sub.data),
                        // XCLR is a packed FormID array — region tags
                        // referenced by REGN records. Variable length;
                        // empty list is normal.
                        b"XCLR" => regions = read_form_id_array(&sub.data),
                        // #692 — XOWN owner, XRNK faction-rank gate,
                        // XGLB global-variable FormID. Same shape on
                        // CELL + REFR. Cross-game (Oblivion / FO3 /
                        // FNV / Skyrim+).
                        b"XOWN" if sub.data.len() >= 4 => {
                            ownership_owner = read_form_id(&sub.data);
                        }
                        b"XRNK" if sub.data.len() >= 4 => {
                            ownership_rank = Some(i32::from_le_bytes([
                                sub.data[0],
                                sub.data[1],
                                sub.data[2],
                                sub.data[3],
                            ]));
                        }
                        b"XGLB" if sub.data.len() >= 4 => {
                            ownership_global = read_form_id(&sub.data);
                        }
                        b"XCLL" if sub.data.len() >= 28 => {
                            // XCLL layout (shared prefix for all games):
                            //   0-3:   Ambient RGBA (byte 0=R, 1=G, 2=B, 3=unused)
                            //   4-7:   Directional RGBA
                            //   8-11:  Fog color near RGBA
                            //   12-15: Fog near (f32)
                            //   16-19: Fog far (f32)
                            //   20-23: Directional rotation X (i32, degrees)
                            //   24-27: Directional rotation Y (i32, degrees)
                            //
                            // Oblivion: 36 bytes. FNV: 40 bytes.
                            // Skyrim+: 92 bytes with extended fields.
                            // Detect by length — unambiguous.
                            //
                            // Byte order is RGB (not D3DCOLOR/BGR): xEdit
                            // defines XCLL color fields as (Red, Green, Blue,
                            // Unknown). This matches the LIGH DATA byte order
                            // after the #389 revert. A dim cool-blue ambient
                            // on a saloon interior is correct — the warm look
                            // comes from the placed LIGH oil-lanterns, not
                            // from the cell fill.
                            let d = &sub.data;
                            let ambient = [
                                d[0] as f32 / 255.0,
                                d[1] as f32 / 255.0,
                                d[2] as f32 / 255.0,
                            ];
                            let directional_color = [
                                d[4] as f32 / 255.0,
                                d[5] as f32 / 255.0,
                                d[6] as f32 / 255.0,
                            ];
                            let rot_x = {
                                let raw = i32::from_le_bytes([d[20], d[21], d[22], d[23]]);
                                (raw as f32).to_radians()
                            };
                            let rot_y = {
                                let raw = i32::from_le_bytes([d[24], d[25], d[26], d[27]]);
                                (raw as f32).to_radians()
                            };
                            // Fog fields land in bytes 8..20, always
                            // present under the outer `>= 28` gate. Pre-#483
                            // a redundant `if d.len() >= 20` nested here
                            // with a dead-code `else` branch that couldn't
                            // be reached (the outer gate already proves
                            // `len >= 28 > 20`). See FNV-2-L3.
                            // RGB byte order (bytes 8=R, 9=G, 10=B).
                            let fog_color = [
                                d[8] as f32 / 255.0,
                                d[9] as f32 / 255.0,
                                d[10] as f32 / 255.0,
                            ];
                            let fog_near = f32::from_le_bytes([d[12], d[13], d[14], d[15]]);
                            let fog_far = f32::from_le_bytes([d[16], d[17], d[18], d[19]]);

                            // XCLL extended layout — per UESP + Gamebryo 2.3
                            // `NiDirectionalLight` source + nif.xml:
                            //   28-31: Directional fade (f32)      — FNV + Skyrim
                            //   32-35: Fog clip distance (f32)     — FNV + Skyrim
                            //   36-39: Fog power (f32)             — FNV + Skyrim
                            //   40-63: Directional ambient 6×RGBA  — Skyrim only
                            //   64-67: Specular color RGBA         — Skyrim only
                            //   68-71: Fresnel power (f32)         — Skyrim only
                            //   72-75: Fog far color RGBA          — Skyrim only
                            //   76-79: Fog max (f32)               — Skyrim only
                            //   80-83: Light fade begin (f32)      — Skyrim only
                            //   84-87: Light fade end (f32)        — Skyrim only
                            //   88-91: Inherits flags (u32, unused)
                            //
                            // FNV XCLL is 40 bytes — it carries `dir_fade`,
                            // `fog_clip`, `fog_power` in the 28..40 tail. Pre-
                            // #379 the whole block 28-87 was gated on `>= 92`
                            // so FNV cells reported all three as `None` and
                            // the renderer fell back to a flat 0.6× fill.
                            // Two separate gates now mirror the on-disk shape.
                            let (dir_fade, fog_clip, fog_power) = if d.len() >= 40 {
                                (
                                    Some(f32::from_le_bytes([d[28], d[29], d[30], d[31]])),
                                    Some(f32::from_le_bytes([d[32], d[33], d[34], d[35]])),
                                    Some(f32::from_le_bytes([d[36], d[37], d[38], d[39]])),
                                )
                            } else {
                                (None, None, None)
                            };
                            let (
                                directional_ambient,
                                specular_color,
                                specular_alpha,
                                fresnel_power,
                                fog_far_color,
                                fog_max,
                                lf_begin,
                                lf_end,
                            ) = if d.len() >= 92 {
                                // Unpack the 6 × RGBA ambient cube (#367). RGB
                                // byte order (o=R, o+1=G, o+2=B); the alpha/
                                // padding byte at o+3 is ignored.
                                let mut ambient_cube = [[0.0f32; 3]; 6];
                                for (face, out) in ambient_cube.iter_mut().enumerate() {
                                    let o = 40 + face * 4;
                                    *out = [
                                        d[o] as f32 / 255.0,
                                        d[o + 1] as f32 / 255.0,
                                        d[o + 2] as f32 / 255.0,
                                    ];
                                }
                                (
                                    Some(ambient_cube),
                                    Some([
                                        d[64] as f32 / 255.0,
                                        d[65] as f32 / 255.0,
                                        d[66] as f32 / 255.0,
                                    ]),
                                    Some(d[67] as f32 / 255.0),
                                    Some(f32::from_le_bytes([d[68], d[69], d[70], d[71]])),
                                    Some([
                                        d[72] as f32 / 255.0,
                                        d[73] as f32 / 255.0,
                                        d[74] as f32 / 255.0,
                                    ]),
                                    Some(f32::from_le_bytes([d[76], d[77], d[78], d[79]])),
                                    Some(f32::from_le_bytes([d[80], d[81], d[82], d[83]])),
                                    Some(f32::from_le_bytes([d[84], d[85], d[86], d[87]])),
                                )
                            } else {
                                (None, None, None, None, None, None, None, None)
                            };

                            lighting = Some(CellLighting {
                                ambient,
                                directional_color,
                                directional_rotation: [rot_x, rot_y],
                                fog_color,
                                fog_near,
                                fog_far,
                                directional_fade: dir_fade,
                                fog_clip,
                                fog_power,
                                fog_far_color,
                                fog_max,
                                light_fade_begin: lf_begin,
                                light_fade_end: lf_end,
                                directional_ambient,
                                specular_color,
                                specular_alpha,
                                fresnel_power,
                            });
                        }
                        _ => {}
                    }
                }

                if is_interior && !editor_id.is_empty() {
                    let key = editor_id.to_ascii_lowercase();
                    let ownership = ownership_owner.map(|owner| CellOwnership {
                        owner_form_id: owner,
                        faction_rank: ownership_rank,
                        global_var_form_id: ownership_global,
                    });
                    cells.insert(
                        key,
                        CellData {
                            form_id: header.form_id,
                            editor_id: editor_id.clone(),
                            references: Vec::new(),
                            is_interior: true,
                            grid: None,
                            lighting: lighting.clone(),
                            landscape: None,
                            water_height,
                            image_space_form,
                            water_type_form,
                            acoustic_space_form,
                            music_type_form,
                            location_form,
                            regions: regions.clone(),
                            ownership,
                        },
                    );
                    current_cell = Some((header.form_id, editor_id));
                } else {
                    current_cell = None;
                }
            } else {
                reader.skip_record(&header);
            }
        }
    }
    Ok(())
}

/// Parse REFR and LAND records within a cell children group.
fn parse_refr_group(
    reader: &mut EsmReader,
    end: usize,
    refs: &mut Vec<PlacedRef>,
    landscape: &mut Option<LandscapeData>,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            // Nested groups within cell children — recurse.
            let sub = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub);
            parse_refr_group(reader, sub_end, refs, landscape)?;
            continue;
        }

        let header = reader.read_record_header()?;
        // ACRE — Oblivion-only placed-creature reference (#396). FO3+
        // folded creature placements into ACHR; ACRE's wire layout
        // matches ACHR byte-for-byte on Oblivion (NAME/DATA/XSCL/XESP),
        // so it routes through the same handler.
        if &header.record_type == b"REFR"
            || &header.record_type == b"ACHR"
            || &header.record_type == b"ACRE"
        {
            let subs = reader.read_sub_records(&header)?;
            let mut base_form_id = 0u32;
            let mut position = [0.0f32; 3];
            let mut rotation = [0.0f32; 3];
            let mut scale = 1.0f32;
            let mut enable_parent: Option<EnableParent> = None;
            let mut teleport: Option<TeleportDest> = None;
            let mut primitive: Option<PrimitiveBounds> = None;
            let mut linked_refs: Vec<LinkedRef> = Vec::new();
            let mut rooms: Vec<u32> = Vec::new();
            let mut portals: Vec<PortalLink> = Vec::new();
            let mut radius_override: Option<f32> = None;
            let mut alt_texture_ref: Option<u32> = None;
            let mut land_texture_ref: Option<u32> = None;
            let mut texture_slot_swaps: Vec<TextureSlotSwap> = Vec::new();
            let mut emissive_light_ref: Option<u32> = None;
            // #692 — per-REFR ownership override. Scopes ownership to a
            // single placed object (chest, bed, locker) when the parent
            // cell is public. Same XOWN / XRNK / XGLB layout as CELL.
            let mut ownership_owner: Option<u32> = None;
            let mut ownership_rank: Option<i32> = None;
            let mut ownership_global: Option<u32> = None;

            // Helpers to decode LE values from sub-record slices. Kept
            // local so the REFR match arm doesn't sprout u32 / f32
            // shuffling at each new sub-type.
            let read_u32 = |bytes: &[u8], off: usize| -> u32 {
                u32::from_le_bytes([
                    bytes[off],
                    bytes[off + 1],
                    bytes[off + 2],
                    bytes[off + 3],
                ])
            };
            let read_f32 = |bytes: &[u8], off: usize| -> f32 {
                f32::from_le_bytes([
                    bytes[off],
                    bytes[off + 1],
                    bytes[off + 2],
                    bytes[off + 3],
                ])
            };

            for sub in &subs {
                match &sub.sub_type {
                    b"NAME" if sub.data.len() >= 4 => {
                        base_form_id = read_u32(&sub.data, 0);
                    }
                    b"DATA" if sub.data.len() >= 24 => {
                        // 6 floats: posX, posY, posZ, rotX, rotY, rotZ
                        for i in 0..3 {
                            position[i] = read_f32(&sub.data, i * 4);
                        }
                        for i in 0..3 {
                            rotation[i] = read_f32(&sub.data, 12 + i * 4);
                        }
                    }
                    b"XSCL" if sub.data.len() >= 4 => {
                        scale = read_f32(&sub.data, 0);
                    }
                    // XESP — enable-parent gating (Skyrim+). 4-byte
                    // parent FormID + 1-byte flags; bit 0 = inverted.
                    // Pre-#349 every default-disabled "spawn after
                    // quest stage" REFR rendered immediately on cell
                    // load; the cell loader now skips these via
                    // `enable_parent.default_disabled()`.
                    b"XESP" if sub.data.len() >= 5 => {
                        let form_id = read_u32(&sub.data, 0);
                        let inverted = sub.data[4] & 1 != 0;
                        enable_parent = Some(EnableParent { form_id, inverted });
                    }
                    // XTEL — Teleport destination (doors). Layout per
                    // UESP: DestRef(u32) + pos(3×f32) + rot(3×f32) +
                    // optional flags(u32) = 28 or 32 bytes. We read the
                    // first 28 bytes and ignore the trailing flags so
                    // FO3/FNV (sometimes 28-byte variant) parses the
                    // same as Skyrim/FO4. Pre-#412 every interior door
                    // was dead on activation.
                    b"XTEL" if sub.data.len() >= 28 => {
                        let destination = read_u32(&sub.data, 0);
                        let dest_pos = [
                            read_f32(&sub.data, 4),
                            read_f32(&sub.data, 8),
                            read_f32(&sub.data, 12),
                        ];
                        let dest_rot = [
                            read_f32(&sub.data, 16),
                            read_f32(&sub.data, 20),
                            read_f32(&sub.data, 24),
                        ];
                        teleport = Some(TeleportDest {
                            destination,
                            position: dest_pos,
                            rotation: dest_rot,
                        });
                    }
                    // XPRM — Primitive bounds for trigger / activator
                    // volumes. Layout per UESP: bounds(3×f32) +
                    // color(3×f32) + unknown(f32) + shape_type(u32) =
                    // 32 bytes. Pre-#412 triggers were invisible.
                    b"XPRM" if sub.data.len() >= 32 => {
                        let bounds = [
                            read_f32(&sub.data, 0),
                            read_f32(&sub.data, 4),
                            read_f32(&sub.data, 8),
                        ];
                        let color = [
                            read_f32(&sub.data, 12),
                            read_f32(&sub.data, 16),
                            read_f32(&sub.data, 20),
                        ];
                        let unknown = read_f32(&sub.data, 24);
                        let shape_type = read_u32(&sub.data, 28);
                        primitive = Some(PrimitiveBounds {
                            bounds,
                            color,
                            unknown,
                            shape_type,
                        });
                    }
                    // XLKR — Linked refs. Layout: keyword(u32) +
                    // target(u32) = 8 bytes. Multiple XLKR allowed per
                    // REFR (patrol markers, door pairs, activator
                    // targets). Pre-#412 NPCs didn't patrol and doors
                    // didn't pair.
                    b"XLKR" if sub.data.len() >= 8 => {
                        let keyword = read_u32(&sub.data, 0);
                        let target = read_u32(&sub.data, 4);
                        linked_refs.push(LinkedRef { keyword, target });
                    }
                    // XRMR — Room membership. Layout per UESP:
                    // count(u32) + count × room_ref(u32). Pre-#412 FO4
                    // cell-subdivided interior culling couldn't work.
                    // Guard the count against the remaining bytes so a
                    // corrupt record can't over-read.
                    b"XRMR" if sub.data.len() >= 4 => {
                        let count = read_u32(&sub.data, 0) as usize;
                        let max = (sub.data.len() - 4) / 4;
                        let n = count.min(max);
                        rooms.reserve(n);
                        for i in 0..n {
                            rooms.push(read_u32(&sub.data, 4 + i * 4));
                        }
                    }
                    // XPOD — Portal origin/destination pair. 8 bytes.
                    // Multiple XPOD sub-records allowed (one per portal
                    // connected to this REFR).
                    b"XPOD" if sub.data.len() >= 8 => {
                        let origin = read_u32(&sub.data, 0);
                        let destination = read_u32(&sub.data, 4);
                        portals.push(PortalLink { origin, destination });
                    }
                    // XRDS — Light radius override. Single f32.
                    b"XRDS" if sub.data.len() >= 4 => {
                        radius_override = Some(read_f32(&sub.data, 0));
                    }
                    // XATO — alternate TXST form ID. 4 bytes. When this
                    // REFR places a base mesh whose textures should be
                    // swapped for another TXST (the 140 MNAM-only vanilla
                    // FO4 TXSTs land here), the cell loader resolves
                    // this against `EsmCellIndex.texture_sets` and
                    // overlays the 8 slot paths + MNAM onto the spawned
                    // mesh. See audit FO4-DIM6-02 / #584.
                    b"XATO" if sub.data.len() >= 4 => {
                        alt_texture_ref = Some(read_u32(&sub.data, 0));
                    }
                    // XTNM — landscape TXST override form ID. Same
                    // 4-byte layout as XATO but scopes the override to
                    // LAND references (LTEX default swap). Kept for
                    // completeness; the LAND-side consumer path is
                    // separate from the mesh path wired for XATO. #584.
                    b"XTNM" if sub.data.len() >= 4 => {
                        land_texture_ref = Some(read_u32(&sub.data, 0));
                    }
                    // XTXR — per-slot texture swap. 8 bytes: TXST
                    // FormID(u32) + slot_index(u32). Lets a REFR
                    // override a single slot of the host mesh's texture
                    // set without replacing the whole TXST. Multiple
                    // XTXR sub-records allowed per REFR; we collect
                    // them in authoring order. #584.
                    b"XTXR" if sub.data.len() >= 8 => {
                        let texture_set = read_u32(&sub.data, 0);
                        let slot_index = read_u32(&sub.data, 4);
                        texture_slot_swaps.push(TextureSlotSwap {
                            texture_set,
                            slot_index,
                        });
                    }
                    // XEMI — per-REFR emissive LIGH FormID. 4 bytes.
                    // Used by FO4 signage / floodlights to attach a
                    // placement-specific light to the base mesh. Parsed
                    // for completeness; consumer wiring (per-REFR
                    // emissive light spawn) is follow-up work. #584.
                    b"XEMI" if sub.data.len() >= 4 => {
                        emissive_light_ref = Some(read_u32(&sub.data, 0));
                    }
                    // #692 — per-REFR ownership override (mirrors the
                    // CELL walker arms). XOWN owner FormID, XRNK
                    // faction-rank gate, XGLB global-var gate. Same
                    // 4-byte layout as the CELL form. Cross-game.
                    b"XOWN" if sub.data.len() >= 4 => {
                        ownership_owner = Some(read_u32(&sub.data, 0));
                    }
                    b"XRNK" if sub.data.len() >= 4 => {
                        ownership_rank = Some(i32::from_le_bytes([
                            sub.data[0],
                            sub.data[1],
                            sub.data[2],
                            sub.data[3],
                        ]));
                    }
                    b"XGLB" if sub.data.len() >= 4 => {
                        ownership_global = Some(read_u32(&sub.data, 0));
                    }
                    _ => {}
                }
            }

            if base_form_id != 0 {
                let ownership = ownership_owner.map(|owner| CellOwnership {
                    owner_form_id: owner,
                    faction_rank: ownership_rank,
                    global_var_form_id: ownership_global,
                });
                refs.push(PlacedRef {
                    base_form_id,
                    position,
                    rotation,
                    scale,
                    enable_parent,
                    teleport,
                    primitive,
                    linked_refs,
                    rooms,
                    portals,
                    radius_override,
                    alt_texture_ref,
                    land_texture_ref,
                    texture_slot_swaps,
                    emissive_light_ref,
                    ownership,
                });
            }
        } else if &header.record_type == b"LAND" {
            // Parse landscape heightmap, normals, and vertex colors.
            //
            // At least one vanilla FNV LAND record (form `0x00150FC0`)
            // reliably fails the body read on every ESM open — cause
            // not yet identified, single cell affected. Observable
            // symptom is a flat/untextured tile if that cell is ever
            // rendered. Demoted from `warn` to `debug` per the audit's
            // soft-fail guidance (#385 / D5-F5); the error context
            // rides through so anyone investigating sees the real
            // failure mode instead of a generic message.
            match parse_land_record(reader, &header) {
                Ok(land) => *landscape = Some(land),
                Err(e) => log::debug!(
                    "LAND record parse failed (form {:08X}): {e:#}",
                    header.form_id
                ),
            }
        } else {
            // Skip other record types (PGRE, PMIS, NAVM, etc.)
            reader.skip_record(&header);
        }
    }
    Ok(())
}

/// Decode a LAND record's VHGT, VNML, and VCLR sub-records.
///
/// VHGT encoding (from UESP): the heightmap is delta-encoded with a
/// column-then-row accumulator scheme. See the UESP wiki "Vertex Height
/// Data" section for the canonical algorithm.
fn parse_land_record(
    reader: &mut EsmReader,
    header: &super::reader::RecordHeader,
) -> Result<LandscapeData> {
    let subs = reader.read_sub_records(header)?;

    let mut heights = vec![0.0f32; 33 * 33];
    let mut normals: Option<Vec<u8>> = None;
    let mut vertex_colors: Option<Vec<u8>> = None;
    let mut quadrants: [TerrainQuadrant; 4] = Default::default();
    // Track the most recently parsed ATXT header so we can attach the
    // following VTXT alpha data to it. ESM sub-records are ordered:
    // ...ATXT, VTXT, ATXT, VTXT... (each ATXT is followed by its VTXT).
    let mut pending_atxt: Option<(usize, u32, u16)> = None; // (quadrant, ltex_form_id, layer)

    for sub in &subs {
        match sub.sub_type.as_slice() {
            b"VHGT" if sub.data.len() >= 1093 => {
                // UESP VHGT algorithm: delta-encoded heightmap.
                let base_offset =
                    f32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                let mut offset = base_offset * 8.0;
                for row in 0..33usize {
                    let first_delta = sub.data[4 + row * 33] as i8 as f32 * 8.0;
                    offset += first_delta;
                    heights[row * 33] = offset;
                    let mut col_accum = offset;
                    for col in 1..33usize {
                        let d = sub.data[4 + row * 33 + col] as i8 as f32 * 8.0;
                        col_accum += d;
                        heights[row * 33 + col] = col_accum;
                    }
                }
            }
            b"VNML" if sub.data.len() >= 3267 => {
                normals = Some(sub.data[..3267].to_vec());
            }
            b"VCLR" if sub.data.len() >= 3267 => {
                vertex_colors = Some(sub.data[..3267].to_vec());
            }
            b"BTXT" if sub.data.len() >= 8 => {
                // Base texture: formid(4) + quadrant(1) + unused(3).
                let ltex_id =
                    u32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                let quadrant = sub.data[4] as usize;
                if quadrant < 4 {
                    quadrants[quadrant].base = Some(ltex_id);
                }
            }
            b"ATXT" if sub.data.len() >= 8 => {
                // Additional texture header: formid(4) + quadrant(1) + unused(1) + layer(2).
                let ltex_id =
                    u32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                let quadrant = sub.data[4] as usize;
                let layer = u16::from_le_bytes([sub.data[6], sub.data[7]]);
                if quadrant < 4 {
                    pending_atxt = Some((quadrant, ltex_id, layer));
                }
            }
            b"VTXT" => {
                // Alpha layer data for the preceding ATXT.
                // Array of 8-byte entries: position(u16) + unused(u16) + opacity(f32).
                if let Some((quadrant, ltex_id, layer)) = pending_atxt.take() {
                    let mut alpha = vec![0.0f32; 17 * 17];
                    let entry_count = sub.data.len() / 8;
                    for i in 0..entry_count {
                        let off = i * 8;
                        if off + 8 > sub.data.len() {
                            break;
                        }
                        let pos = u16::from_le_bytes([sub.data[off], sub.data[off + 1]]) as usize;
                        let opacity = f32::from_le_bytes([
                            sub.data[off + 4],
                            sub.data[off + 5],
                            sub.data[off + 6],
                            sub.data[off + 7],
                        ]);
                        if pos < 17 * 17 {
                            alpha[pos] = opacity;
                        }
                    }
                    quadrants[quadrant].layers.push(TerrainTextureLayer {
                        ltex_form_id: ltex_id,
                        layer,
                        alpha: Some(alpha),
                    });
                }
            }
            _ => {}
        }
    }

    // Sort additional layers by layer index within each quadrant.
    for q in &mut quadrants {
        q.layers.sort_by_key(|l| l.layer);
    }

    Ok(LandscapeData {
        heights,
        normals,
        vertex_colors,
        quadrants,
    })
}

/// Walk the WRLD group hierarchy to find exterior cells and their placed references.
fn parse_wrld_group(
    reader: &mut EsmReader,
    end: usize,
    all_exterior_cells: &mut HashMap<String, HashMap<(i32, i32), CellData>>,
    worldspace_climates: &mut HashMap<String, u32>,
) -> Result<()> {
    let mut current_wrld_name: Option<String> = None;

    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub_group = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub_group);

            match sub_group.group_type {
                // World children (type 1): contains exterior cell blocks for the current WRLD.
                1 => {
                    if let Some(ref name) = current_wrld_name {
                        let cells = all_exterior_cells
                            .entry(name.to_ascii_lowercase())
                            .or_insert_with(HashMap::new);
                        parse_wrld_children(reader, sub_end, cells)?;
                    } else {
                        reader.skip_group(&sub_group);
                    }
                }
                _ => {
                    reader.skip_group(&sub_group);
                }
            }
        } else {
            // WRLD record — extract worldspace name + climate form ID.
            let header = reader.read_record_header()?;
            if &header.record_type == b"WRLD" {
                let subs = reader.read_sub_records(&header)?;
                let mut name_opt: Option<String> = None;
                let mut climate_fid: Option<u32> = None;
                for sub in &subs {
                    match &sub.sub_type {
                        b"EDID" => {
                            name_opt = Some(read_zstring(&sub.data));
                        }
                        b"CNAM" if sub.data.len() >= 4 => {
                            climate_fid = Some(u32::from_le_bytes([
                                sub.data[0],
                                sub.data[1],
                                sub.data[2],
                                sub.data[3],
                            ]));
                        }
                        _ => {}
                    }
                }
                if let Some(ref name) = name_opt {
                    log::info!(
                        "Found worldspace: '{}' (form {:08X}, climate: {:08X?})",
                        name,
                        header.form_id,
                        climate_fid,
                    );
                    if let Some(clmt_fid) = climate_fid {
                        worldspace_climates.insert(name.to_ascii_lowercase(), clmt_fid);
                    }
                    current_wrld_name = name_opt;
                }
            } else {
                reader.skip_record(&header);
            }
        }
    }
    Ok(())
}

/// Walk exterior cell hierarchy within a worldspace (group types 1, 4, 5).
fn parse_wrld_children(
    reader: &mut EsmReader,
    end: usize,
    exterior_cells: &mut HashMap<(i32, i32), CellData>,
) -> Result<()> {
    let mut current_cell: Option<(i32, i32)> = None;

    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub_group = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub_group);

            match sub_group.group_type {
                // Exterior block (4) and sub-block (5): recurse.
                4 | 5 => {
                    parse_wrld_children(reader, sub_end, exterior_cells)?;
                }
                // Cell children (6=temporary, 8=persistent, 9=visible distant).
                6 | 8 | 9 => {
                    if let Some(grid) = current_cell {
                        let mut refs = Vec::new();
                        let mut land = None;
                        parse_refr_group(reader, sub_end, &mut refs, &mut land)?;
                        if let Some(cell) = exterior_cells.get_mut(&grid) {
                            cell.references.extend(refs);
                            if land.is_some() && cell.landscape.is_none() {
                                cell.landscape = land;
                            }
                        }
                    } else {
                        reader.skip_group(&sub_group);
                    }
                }
                _ => {
                    reader.skip_group(&sub_group);
                }
            }
        } else {
            let header = reader.read_record_header()?;
            if &header.record_type == b"CELL" {
                let subs = reader.read_sub_records(&header)?;
                let mut editor_id = String::new();
                let mut grid = None;
                let mut water_height: Option<f32> = None;
                let mut image_space_form: Option<u32> = None;
                let mut water_type_form: Option<u32> = None;
                let mut acoustic_space_form: Option<u32> = None;
                let mut music_type_form: Option<u32> = None;
                let mut location_form: Option<u32> = None;
                let mut regions: Vec<u32> = Vec::new();
                // #692 — exterior CELL ownership (worldspace owner +
                // faction-rank gate + global-var gate). Same layout as
                // interior CELL above; cross-game.
                let mut ownership_owner: Option<u32> = None;
                let mut ownership_rank: Option<i32> = None;
                let mut ownership_global: Option<u32> = None;

                for sub in &subs {
                    match &sub.sub_type {
                        b"EDID" => editor_id = read_zstring(&sub.data),
                        b"XCLC" if sub.data.len() >= 8 => {
                            let grid_x = i32::from_le_bytes([
                                sub.data[0],
                                sub.data[1],
                                sub.data[2],
                                sub.data[3],
                            ]);
                            let grid_y = i32::from_le_bytes([
                                sub.data[4],
                                sub.data[5],
                                sub.data[6],
                                sub.data[7],
                            ]);
                            grid = Some((grid_x, grid_y));
                        }
                        b"XCLW" if sub.data.len() >= 4 => {
                            water_height = Some(f32::from_le_bytes([
                                sub.data[0],
                                sub.data[1],
                                sub.data[2],
                                sub.data[3],
                            ]));
                        }
                        // Skyrim extended sub-records — see the interior
                        // walker above for semantics. Exterior cells use
                        // the same encoding. #356.
                        b"XCIM" => image_space_form = read_form_id(&sub.data),
                        b"XCWT" => water_type_form = read_form_id(&sub.data),
                        b"XCAS" => acoustic_space_form = read_form_id(&sub.data),
                        b"XCMO" => music_type_form = read_form_id(&sub.data),
                        b"XLCN" => location_form = read_form_id(&sub.data),
                        b"XCLR" => regions = read_form_id_array(&sub.data),
                        // #692 — exterior CELL ownership tuple (mirrors
                        // the interior walker arms above).
                        b"XOWN" if sub.data.len() >= 4 => {
                            ownership_owner = read_form_id(&sub.data);
                        }
                        b"XRNK" if sub.data.len() >= 4 => {
                            ownership_rank = Some(i32::from_le_bytes([
                                sub.data[0],
                                sub.data[1],
                                sub.data[2],
                                sub.data[3],
                            ]));
                        }
                        b"XGLB" if sub.data.len() >= 4 => {
                            ownership_global = read_form_id(&sub.data);
                        }
                        _ => {}
                    }
                }

                if let Some(g) = grid {
                    let ownership = ownership_owner.map(|owner| CellOwnership {
                        owner_form_id: owner,
                        faction_rank: ownership_rank,
                        global_var_form_id: ownership_global,
                    });
                    exterior_cells.insert(
                        g,
                        CellData {
                            form_id: header.form_id,
                            editor_id,
                            references: Vec::new(),
                            is_interior: false,
                            grid: Some(g),
                            lighting: None,
                            landscape: None,
                            water_height,
                            image_space_form,
                            water_type_form,
                            acoustic_space_form,
                            music_type_form,
                            location_form,
                            regions,
                            ownership,
                        },
                    );
                    current_cell = Some(g);
                } else {
                    current_cell = None;
                }
            } else {
                reader.skip_record(&header);
            }
        }
    }
    Ok(())
}

/// Walk a top-level record group and extract any record with a MODL sub-record.
/// Works for STAT, MSTT, FURN, DOOR, ACTI, CONT, LIGH, MISC, etc.
fn parse_modl_group(
    reader: &mut EsmReader,
    end: usize,
    statics: &mut HashMap<u32, StaticObject>,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub);
            parse_modl_group(reader, sub_end, statics)?;
            continue;
        }

        let header = reader.read_record_header()?;
        {
            let is_ligh = &header.record_type == b"LIGH";
            let is_addn = &header.record_type == b"ADDN";
            let subs = reader.read_sub_records(&header)?;
            let mut editor_id = String::new();
            let mut model_path = String::new();
            let mut light_data = None;
            let mut addon_index: Option<i32> = None;
            let mut addon_dnam: Option<(u16, u16)> = None;
            let mut has_script = false;
            // FO4 LIGH power-circuit sub-record. Captured alongside
            // LightData so the future settlement-circuit ECS system
            // can resolve wired fixtures. See #602.
            let mut xpwr_form_id: Option<u32> = None;

            for sub in &subs {
                match &sub.sub_type {
                    b"EDID" => editor_id = read_zstring(&sub.data),
                    b"MODL" => model_path = read_zstring(&sub.data),
                    // VMAD presence-only flag — see `has_script` field doc on
                    // `StaticObject`. Full decoding deferred to scripting-as-
                    // ECS work. See #369.
                    b"VMAD" => has_script = true,
                    b"DATA" if is_ligh && sub.data.len() >= 12 => {
                        // LIGH DATA: time(u32), radius(u32), color(RGBA u8×4), flags(u32), ...
                        //
                        // Bytes 8..12 are Red, Green, Blue, Unknown/alpha in that
                        // order — xEdit defines LIGH DATA `Color` as a
                        // { Red: u8; Green: u8; Blue: u8; Unknown: u8 } struct.
                        //
                        // Fix #389 previously flipped this to BGR after
                        // cross-checking Oblivion's `RootGreenBright0650`
                        // (bytes 36 74 66 00 → G=116 is max either way), but
                        // that test case was ambiguous. FalloutNV.esm makes
                        // the correct byte order unambiguous — every warm/
                        // amber/red/orange EDID came out as its complement
                        // (blue/cyan) under BGR:
                        //   OurLadyHopeRed              [0.14, 0.58, 0.98]  ✗
                        //   DunwichLightOrangeFlicker01 [0.15, 0.42, 0.68]  ✗
                        //   BasementLightKickerWarm     [0.69, 0.83, 0.89]  ✗
                        //   BasementLightFillCool       [0.79, 0.72, 0.65]  ✗  (should be cool)
                        // Reading as RGB puts each one where its EDID says
                        // it should land. See docs/engine/lighting-from-cells.md.
                        let radius = u32::from_le_bytes([
                            sub.data[4],
                            sub.data[5],
                            sub.data[6],
                            sub.data[7],
                        ]) as f32;
                        let r = sub.data[8] as f32 / 255.0;
                        let g = sub.data[9] as f32 / 255.0;
                        let b = sub.data[10] as f32 / 255.0;
                        let flags = if sub.data.len() >= 16 {
                            u32::from_le_bytes([
                                sub.data[12],
                                sub.data[13],
                                sub.data[14],
                                sub.data[15],
                            ])
                        } else {
                            0
                        };
                        light_data = Some(LightData {
                            radius,
                            color: [r, g, b],
                            flags,
                            // xpwr_form_id populated by the separate
                            // XPWR sub-record arm below, if present.
                            xpwr_form_id: None,
                        });
                    }
                    // FO4 power-circuit FormID. Pre-work capture for
                    // #602 — no consumer today, but we preserve the
                    // raw reference so the future settlement-circuit
                    // system can resolve wired lights. See audit
                    // FO4-DIM6-07.
                    b"XPWR" if is_ligh && sub.data.len() >= 4 => {
                        xpwr_form_id = Some(u32::from_le_bytes([
                            sub.data[0],
                            sub.data[1],
                            sub.data[2],
                            sub.data[3],
                        ]));
                    }
                    b"DATA" if is_addn && sub.data.len() >= 4 => {
                        // ADDN DATA: signed 4-byte addon index. Negative
                        // values are engine-reserved; positive indexes
                        // select a master particle pool slot (#370).
                        addon_index = Some(i32::from_le_bytes([
                            sub.data[0],
                            sub.data[1],
                            sub.data[2],
                            sub.data[3],
                        ]));
                    }
                    b"DNAM" if is_addn && sub.data.len() >= 4 => {
                        // ADDN DNAM: u16 master_particle_cap + u16 flags.
                        let cap = u16::from_le_bytes([sub.data[0], sub.data[1]]);
                        let flags = u16::from_le_bytes([sub.data[2], sub.data[3]]);
                        addon_dnam = Some((cap, flags));
                    }
                    _ => {}
                }
            }

            // Merge XPWR into LightData post-loop — sub-record authoring
            // order isn't fixed so DATA and XPWR can appear in either
            // sequence. The inner arm stores into a local; we fold it
            // onto the finalised `LightData` here. See #602.
            if let (Some(ref mut ld), Some(form)) = (&mut light_data, xpwr_form_id) {
                ld.xpwr_form_id = Some(form);
            }

            let addon_data = if is_addn && (addon_index.is_some() || addon_dnam.is_some()) {
                let (master_particle_cap, flags) = addon_dnam.unwrap_or((0, 0));
                Some(AddonData {
                    addon_index: addon_index.unwrap_or(0),
                    master_particle_cap,
                    flags,
                })
            } else {
                None
            };

            // Insert if we have a model path, a LIGH with light data
            // (some lights have no mesh — just point lights), or an
            // ADDN with its addon-data payload (some ADDN records are
            // pure particle emitters with no MODL).
            if !model_path.is_empty() || light_data.is_some() || addon_data.is_some() {
                statics.insert(
                    header.form_id,
                    StaticObject {
                        form_id: header.form_id,
                        editor_id,
                        model_path,
                        light_data,
                        addon_data,
                        has_script,
                    },
                );
            }
        }
    }
    Ok(())
}

/// Read a null-terminated string from sub-record data.
fn read_zstring(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).to_string()
}

/// Read a 4-byte FormID from a sub-record payload. Returns `None` when
/// the payload is too short to hold a u32 — defensive against truncated
/// records the walker would otherwise pass through. Used by the
/// Skyrim-extended CELL sub-record arms (XCIM / XCWT / XCAS / XCMO /
/// XLCN — see #356).
fn read_form_id(data: &[u8]) -> Option<u32> {
    (data.len() >= 4).then(|| u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

/// Read an array of 4-byte FormIDs packed back-to-back. Used for XCLR
/// (region list) and any other list-of-FormIDs sub-record. Trailing
/// bytes that don't make a full FormID are silently dropped — they're
/// always alignment padding rather than a partial entry.
fn read_form_id_array(data: &[u8]) -> Vec<u32> {
    data.chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Parse LTEX (Landscape Texture) records.
///
/// FO3/FNV: LTEX has a TNAM sub-record pointing to a TXST form ID.
/// Oblivion: LTEX has an ICON sub-record with a direct texture path.
fn parse_ltex_group(
    reader: &mut EsmReader,
    end: usize,
    ltex_to_txst: &mut HashMap<u32, u32>,
    direct_paths: &mut HashMap<u32, String>,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub);
            parse_ltex_group(reader, sub_end, ltex_to_txst, direct_paths)?;
            continue;
        }

        let header = reader.read_record_header()?;
        if &header.record_type == b"LTEX" {
            let subs = reader.read_sub_records(&header)?;
            for sub in &subs {
                match sub.sub_type.as_slice() {
                    // FO3/FNV/Skyrim: TNAM → TXST form ID.
                    b"TNAM" if sub.data.len() >= 4 => {
                        let txst_id = u32::from_le_bytes([
                            sub.data[0],
                            sub.data[1],
                            sub.data[2],
                            sub.data[3],
                        ]);
                        ltex_to_txst.insert(header.form_id, txst_id);
                    }
                    // Oblivion: ICON → direct texture path.
                    b"ICON" => {
                        let path = read_zstring(&sub.data);
                        if !path.is_empty() {
                            direct_paths.insert(header.form_id, path);
                        }
                    }
                    _ => {}
                }
            }
        } else {
            reader.skip_record(&header);
        }
    }
    Ok(())
}

/// Parse TXST (Texture Set) records. Extracts all 8 texture slots
/// (TX00..TX07) into a [`TextureSet`] entry, plus the legacy
/// `txst_textures: form_id → diffuse_path` map kept for the LTEX
/// resolver downstream. Pre-#357 only TX00 was retained — REFR
/// XTNM/XPRD overrides referencing a TXST silently dropped 7 of 8
/// channels (visible on Skyrim re-skinned statics as "wrong material
/// on a re-textured prop"). See audit S6-11.
fn parse_txst_group(
    reader: &mut EsmReader,
    end: usize,
    txst_textures: &mut HashMap<u32, String>,
    texture_sets: &mut HashMap<u32, TextureSet>,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub);
            parse_txst_group(reader, sub_end, txst_textures, texture_sets)?;
            continue;
        }

        let header = reader.read_record_header()?;
        if &header.record_type == b"TXST" {
            let subs = reader.read_sub_records(&header)?;
            let mut set = TextureSet::default();
            for sub in &subs {
                // Helper: extract a non-empty zstring path for one slot.
                let extract = |bytes: &[u8]| -> Option<String> {
                    let s = read_zstring(bytes);
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                };
                match sub.sub_type.as_slice() {
                    b"TX00" => set.diffuse = extract(&sub.data),
                    b"TX01" => set.normal = extract(&sub.data),
                    b"TX02" => set.glow = extract(&sub.data),
                    b"TX03" => set.height = extract(&sub.data),
                    b"TX04" => set.env = extract(&sub.data),
                    b"TX05" => set.env_mask = extract(&sub.data),
                    b"TX06" => set.inner = extract(&sub.data),
                    b"TX07" => set.specular = extract(&sub.data),
                    // FO4+ BGSM material path. 37 % of vanilla
                    // `Fallout4.esm` TXST records (140 / 382) are
                    // MNAM-only with no TX00 at all; pre-#406 they were
                    // silently dropped because the outer `if set !=
                    // default()` guard would fail and `txst_textures`
                    // never got a diffuse fallback either. See #406.
                    b"MNAM" => set.material_path = extract(&sub.data),
                    _ => {}
                }
            }
            // Backward-compat LTEX resolver: legacy diffuse-only map.
            if let Some(diffuse) = set.diffuse.as_ref() {
                txst_textures.insert(header.form_id, diffuse.clone());
            }
            // Skip the all-empty case (a TXST with no readable slots
            // is uninteresting and would just bloat the map).
            if set != TextureSet::default() {
                texture_sets.insert(header.form_id, set);
            }
        } else {
            reader.skip_record(&header);
        }
    }
    Ok(())
}

/// Parse SCOL (Static Collection) records. Each record is captured
/// both in the legacy `statics` map (so REFRs targeting the SCOL
/// still resolve its cached combined mesh via MODL) and in the new
/// `scols` map which carries the full ONAM/DATA child-placement
/// data the cell loader needs to expand mod-added SCOLs whose
/// cached `CM*.NIF` isn't shipped. Pre-#405 SCOLs were routed
/// through `parse_modl_group` and the placement arrays were
/// discarded. See audit FO4-D4-C2.
fn parse_scol_group(
    reader: &mut EsmReader,
    end: usize,
    statics: &mut HashMap<u32, StaticObject>,
    scols: &mut HashMap<u32, super::records::ScolRecord>,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub);
            parse_scol_group(reader, sub_end, statics, scols)?;
            continue;
        }

        let header = reader.read_record_header()?;
        if &header.record_type == b"SCOL" {
            let subs = reader.read_sub_records(&header)?;
            let record = super::records::parse_scol(header.form_id, &subs);
            // Preserve the MODL-backed StaticObject entry so REFR
            // resolution against the SCOL form ID keeps finding the
            // cached combined mesh. Mirror `parse_modl_group`'s
            // (empty light_data / empty addon_data / has_script)
            // defaults — SCOL carries none of those.
            if !record.model_path.is_empty() || !record.editor_id.is_empty() {
                statics.insert(
                    header.form_id,
                    StaticObject {
                        form_id: header.form_id,
                        editor_id: record.editor_id.clone(),
                        model_path: record.model_path.clone(),
                        light_data: None,
                        addon_data: None,
                        // `parse_scol` doesn't currently capture VMAD
                        // presence — vanilla FO4 has no script-bearing
                        // SCOLs; revisit if mods add them.
                        has_script: false,
                    },
                );
            }
            scols.insert(header.form_id, record);
        } else {
            reader.skip_record(&header);
        }
    }
    Ok(())
}

/// Parse PKIN (Pack-In) records. Each record is captured in the
/// `packins` map with its CNAM-driven content-reference list, and
/// also gets a nominal `StaticObject` entry with an empty
/// `model_path` so REFR resolution still finds the base form at cell
/// load time — the cell loader uses "statics[base].model_path empty
/// AND base in packins" as the signal to expand into synthetic
/// placements.
///
/// Pre-#589 PKIN records were routed through the MODL-only parser
/// (which only pulls EDID when MODL is absent) and the CNAM content
/// list was silently dropped. Vanilla Fallout4.esm ships 872 PKIN
/// records — every FO4 workshop-content bundle REFR rendered as
/// nothing. See audit FO4-DIM4-03.
fn parse_pkin_group(
    reader: &mut EsmReader,
    end: usize,
    statics: &mut HashMap<u32, StaticObject>,
    packins: &mut HashMap<u32, super::records::PkinRecord>,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub);
            parse_pkin_group(reader, sub_end, statics, packins)?;
            continue;
        }

        let header = reader.read_record_header()?;
        if &header.record_type == b"PKIN" {
            let subs = reader.read_sub_records(&header)?;
            let record = super::records::parse_pkin(header.form_id, &subs);
            // Register a nominal StaticObject so REFR base-form lookup
            // succeeds. Empty `model_path` + `contents.len() > 0` is
            // the cell loader's expansion trigger (see
            // `expand_pkin_placements`). Keeping the `editor_id`
            // populated lets debug logging surface the PKIN name when
            // a spawn fails to find the base.
            if !record.editor_id.is_empty() {
                statics.insert(
                    header.form_id,
                    StaticObject {
                        form_id: header.form_id,
                        editor_id: record.editor_id.clone(),
                        model_path: String::new(),
                        light_data: None,
                        addon_data: None,
                        has_script: false,
                    },
                );
            }
            packins.insert(header.form_id, record);
        } else {
            reader.skip_record(&header);
        }
    }
    Ok(())
}

/// Walk an MSWP group and parse every `MSWP` record into the
/// `material_swaps` map. Sub-groups (rare in vanilla but common in
/// mods that nest under MSWP) recurse like every other group walker
/// in this file. Pre-#590 the entire group was `skip_group`'d so all
/// ~2,500 vanilla Fallout4.esm material-swap tables were silently
/// discarded — every Raider armour, station-wagon rust variant, and
/// vault-decay overlay rendered identically across REFRs.
///
/// Stores nothing on `statics` — MSWP isn't a placeable base form,
/// only a substitution table consumed at REFR-spawn time when the
/// REFR carries `XMSP`. See audit FO4-DIM6-05.
fn parse_mswp_group(
    reader: &mut EsmReader,
    end: usize,
    material_swaps: &mut HashMap<u32, super::records::MaterialSwapRecord>,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub);
            parse_mswp_group(reader, sub_end, material_swaps)?;
            continue;
        }

        let header = reader.read_record_header()?;
        if &header.record_type == b"MSWP" {
            let subs = reader.read_sub_records(&header)?;
            let record = super::records::parse_mswp(header.form_id, &subs);
            material_swaps.insert(header.form_id, record);
        } else {
            reader.skip_record(&header);
        }
    }
    Ok(())
}


#[cfg(test)]
#[path = "cell_tests.rs"]
mod tests;
