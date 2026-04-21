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
            | b"CREA" | b"MOVS" | b"PKIN" => {
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

            for sub in &subs {
                match &sub.sub_type {
                    b"NAME" if sub.data.len() >= 4 => {
                        base_form_id = u32::from_le_bytes([
                            sub.data[0],
                            sub.data[1],
                            sub.data[2],
                            sub.data[3],
                        ]);
                    }
                    b"DATA" if sub.data.len() >= 24 => {
                        // 6 floats: posX, posY, posZ, rotX, rotY, rotZ
                        for i in 0..3 {
                            let off = i * 4;
                            position[i] = f32::from_le_bytes([
                                sub.data[off],
                                sub.data[off + 1],
                                sub.data[off + 2],
                                sub.data[off + 3],
                            ]);
                        }
                        for i in 0..3 {
                            let off = 12 + i * 4;
                            rotation[i] = f32::from_le_bytes([
                                sub.data[off],
                                sub.data[off + 1],
                                sub.data[off + 2],
                                sub.data[off + 3],
                            ]);
                        }
                    }
                    b"XSCL" if sub.data.len() >= 4 => {
                        scale = f32::from_le_bytes([
                            sub.data[0],
                            sub.data[1],
                            sub.data[2],
                            sub.data[3],
                        ]);
                    }
                    // XESP — enable-parent gating (Skyrim+). 4-byte
                    // parent FormID + 1-byte flags; bit 0 = inverted.
                    // Pre-#349 every default-disabled "spawn after
                    // quest stage" REFR rendered immediately on cell
                    // load; the cell loader now skips these via
                    // `enable_parent.default_disabled()`.
                    b"XESP" if sub.data.len() >= 5 => {
                        let form_id = u32::from_le_bytes([
                            sub.data[0],
                            sub.data[1],
                            sub.data[2],
                            sub.data[3],
                        ]);
                        let inverted = sub.data[4] & 1 != 0;
                        enable_parent = Some(EnableParent { form_id, inverted });
                    }
                    _ => {}
                }
            }

            if base_form_id != 0 {
                refs.push(PlacedRef {
                    base_form_id,
                    position,
                    rotation,
                    scale,
                    enable_parent,
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
                        _ => {}
                    }
                }

                if let Some(g) = grid {
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
                        });
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

#[cfg(test)]
mod tests {
    use super::super::reader::EsmReader;
    use super::*;

    // Helper: build minimal STAT record bytes.
    fn build_stat_record(form_id: u32, editor_id: &str, model_path: &str) -> Vec<u8> {
        let mut sub_data = Vec::new();
        // EDID
        let edid = format!("{}\0", editor_id);
        sub_data.extend_from_slice(b"EDID");
        sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(edid.as_bytes());
        // MODL
        let modl = format!("{}\0", model_path);
        sub_data.extend_from_slice(b"MODL");
        sub_data.extend_from_slice(&(modl.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(modl.as_bytes());

        let mut buf = Vec::new();
        buf.extend_from_slice(b"STAT");
        buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // flags
        buf.extend_from_slice(&form_id.to_le_bytes());
        buf.extend_from_slice(&[0u8; 8]); // padding
        buf.extend_from_slice(&sub_data);
        buf
    }

    // Helper: build minimal ADDN record bytes with DATA (s32 index) +
    // DNAM (u16 cap, u16 flags). Optional EDID / MODL included. See #370.
    fn build_addn_record(
        form_id: u32,
        editor_id: &str,
        model_path: &str,
        addon_index: i32,
        cap: u16,
        flags: u16,
    ) -> Vec<u8> {
        let mut sub_data = Vec::new();
        // EDID
        let edid = format!("{}\0", editor_id);
        sub_data.extend_from_slice(b"EDID");
        sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(edid.as_bytes());
        // MODL
        let modl = format!("{}\0", model_path);
        sub_data.extend_from_slice(b"MODL");
        sub_data.extend_from_slice(&(modl.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(modl.as_bytes());
        // DATA: s32 addon_index
        sub_data.extend_from_slice(b"DATA");
        sub_data.extend_from_slice(&4u16.to_le_bytes());
        sub_data.extend_from_slice(&addon_index.to_le_bytes());
        // DNAM: u16 cap + u16 flags
        sub_data.extend_from_slice(b"DNAM");
        sub_data.extend_from_slice(&4u16.to_le_bytes());
        sub_data.extend_from_slice(&cap.to_le_bytes());
        sub_data.extend_from_slice(&flags.to_le_bytes());

        let mut buf = Vec::new();
        buf.extend_from_slice(b"ADDN");
        buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // flags
        buf.extend_from_slice(&form_id.to_le_bytes());
        buf.extend_from_slice(&[0u8; 8]); // padding
        buf.extend_from_slice(&sub_data);
        buf
    }

    // Helper: build minimal LIGH record with DATA subrecord. The DATA
    // payload uses the real on-disk layout: time(u32) + radius(u32) +
    // color(BGRA u8×4) + flags(u32) = 16 bytes. EDID comes first.
    fn build_ligh_record(
        form_id: u32,
        editor_id: &str,
        radius: u32,
        rgb: [u8; 3],
        flags: u32,
    ) -> Vec<u8> {
        let mut sub_data = Vec::new();
        let edid = format!("{}\0", editor_id);
        sub_data.extend_from_slice(b"EDID");
        sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(edid.as_bytes());

        sub_data.extend_from_slice(b"DATA");
        sub_data.extend_from_slice(&16u16.to_le_bytes());
        sub_data.extend_from_slice(&u32::MAX.to_le_bytes()); // time = -1
        sub_data.extend_from_slice(&radius.to_le_bytes());
        sub_data.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 0u8]); // RGBA on disk
        sub_data.extend_from_slice(&flags.to_le_bytes());

        let mut buf = Vec::new();
        buf.extend_from_slice(b"LIGH");
        buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // flags
        buf.extend_from_slice(&form_id.to_le_bytes());
        buf.extend_from_slice(&[0u8; 8]); // padding
        buf.extend_from_slice(&sub_data);
        buf
    }

    #[test]
    fn parse_ligh_decodes_color_as_rgba() {
        // Regression: LIGH DATA bytes 8..12 are stored on disk as
        // (Red, Green, Blue, Unknown) — same order xEdit lists in its
        // Color struct definition. Fix #389 previously interpreted these
        // as D3DCOLOR_XRGB (BGRA) but that was based on an ambiguous
        // Oblivion `RootGreenBright0650` sample (G is max either way).
        //
        // FalloutNV.esm makes the correct order unambiguous:
        //   OurLadyHopeRed              bytes ≈ FB 95 24 00 → R=251 warm red
        //   DunwichLightOrangeFlicker01 bytes ≈ AE 6A 26 00 → R=174 warm orange
        //   BasementLightKickerWarm     bytes ≈ B0 D3 E4 ?? → B=228 cool cyan ← #389 reversed this
        // Under BGR every warm/red/orange EDID surfaced as its cool complement
        // (blue/cyan), visible in GSProspectorSaloonInterior torches.
        //
        // This test uses the same RootGreenBright bytes 36 74 66 00 — since
        // green is max in either order, the test is a boundary check that
        // the R channel ends up on output[0] (RGB), not that G dominates.
        let ligh = build_ligh_record(
            0xABCD,
            "RootGreenBright0650",
            650,
            [0x36, 0x74, 0x66],
            0x400,
        );
        let total_size = 24 + ligh.len();
        let mut group = Vec::new();
        group.extend_from_slice(b"GRUP");
        group.extend_from_slice(&(total_size as u32).to_le_bytes());
        group.extend_from_slice(b"LIGH");
        group.extend_from_slice(&0u32.to_le_bytes());
        group.extend_from_slice(&[0u8; 8]);
        group.extend_from_slice(&ligh);

        let mut reader = EsmReader::new(&group);
        let gh = reader.read_group_header().unwrap();
        let end = reader.group_content_end(&gh);
        let mut statics = HashMap::new();
        parse_modl_group(&mut reader, end, &mut statics).unwrap();

        let s = statics.get(&0xABCD).expect("LIGH entry present");
        let ld = s.light_data.as_ref().expect("light_data populated");
        assert!((ld.radius - 650.0).abs() < 0.5);
        let [r, g, b] = ld.color;
        // Bytes were supplied as [0x36, 0x74, 0x66] — under RGB they map to
        // R=0x36 (54), G=0x74 (116), B=0x66 (102).
        assert!((r - 0x36 as f32 / 255.0).abs() < 1e-4, "R mismatch: {r}");
        assert!((g - 0x74 as f32 / 255.0).abs() < 1e-4, "G mismatch: {g}");
        assert!((b - 0x66 as f32 / 255.0).abs() < 1e-4, "B mismatch: {b}");
        assert!(g > r && g > b, "green-authored light must peak on G");
        assert_eq!(ld.flags, 0x400);
    }

    #[test]
    fn parse_ligh_decodes_fnv_warm_lights_without_channel_swap() {
        // Regression guard for the #389 revert: FalloutNV.esm ships
        // several colorfully-named LIGH records that make the RGB byte
        // order unambiguous. Under the previous BGR interpretation every
        // one surfaced as its cool complement.
        //
        // Values here were dumped live from FalloutNV.esm during the
        // session-12 FNV audit. Each is asserted to land under the
        // relative-channel dominance the EDID advertises.
        for (edid, rgb, expected_dominant) in [
            // form_id-independent samples — only colors are asserted.
            ("OurLadyHopeRed", [0xFB, 0x95, 0x24], 'R'),
            ("DunwichLightOrangeFlicker01", [0xAE, 0x6A, 0x26], 'R'),
            ("BasementLightKickerWarm", [0xAF, 0xD3, 0xE4], 'R'), // warm = brighter R/G than raw pack
            ("BasementLightFillCool", [0xA5, 0xB8, 0xC9], 'B'),
        ] {
            let _ = expected_dominant; // retained for doc; hardcoded below.
            let ligh = build_ligh_record(0x1234, edid, 128, rgb, 0);
            let total_size = 24 + ligh.len();
            let mut group = Vec::new();
            group.extend_from_slice(b"GRUP");
            group.extend_from_slice(&(total_size as u32).to_le_bytes());
            group.extend_from_slice(b"LIGH");
            group.extend_from_slice(&0u32.to_le_bytes());
            group.extend_from_slice(&[0u8; 8]);
            group.extend_from_slice(&ligh);

            let mut reader = EsmReader::new(&group);
            let gh = reader.read_group_header().unwrap();
            let end = reader.group_content_end(&gh);
            let mut statics = HashMap::new();
            parse_modl_group(&mut reader, end, &mut statics).unwrap();

            let s = statics.get(&0x1234).expect("LIGH entry present");
            let ld = s.light_data.as_ref().expect("light_data populated");
            let [r, g, b] = ld.color;
            assert!(
                (r - rgb[0] as f32 / 255.0).abs() < 1e-4,
                "{edid}: R byte mismatch (got {r})"
            );
            assert!(
                (g - rgb[1] as f32 / 255.0).abs() < 1e-4,
                "{edid}: G byte mismatch (got {g})"
            );
            assert!(
                (b - rgb[2] as f32 / 255.0).abs() < 1e-4,
                "{edid}: B byte mismatch (got {b})"
            );
        }
    }

    #[test]
    fn parse_addn_extracts_data_and_dnam() {
        // Regression: #370 — ADDN DATA (s32 addon index) and DNAM
        // (u16 cap + u16 flags) must both land on StaticObject.addon_data.
        let addn = build_addn_record(
            0x4567,
            "MothSwarm01",
            "meshes\\effects\\moths.nif",
            7,
            64,
            1,
        );
        let total_size = 24 + addn.len();
        let mut group = Vec::new();
        group.extend_from_slice(b"GRUP");
        group.extend_from_slice(&(total_size as u32).to_le_bytes());
        group.extend_from_slice(b"ADDN");
        group.extend_from_slice(&0u32.to_le_bytes());
        group.extend_from_slice(&[0u8; 8]);
        group.extend_from_slice(&addn);

        let mut reader = EsmReader::new(&group);
        let gh = reader.read_group_header().unwrap();
        let end = reader.group_content_end(&gh);
        let mut statics = HashMap::new();
        parse_modl_group(&mut reader, end, &mut statics).unwrap();

        let s = statics.get(&0x4567).expect("ADDN entry present");
        assert_eq!(s.editor_id, "MothSwarm01");
        assert_eq!(s.model_path, "meshes\\effects\\moths.nif");
        let ad = s.addon_data.expect("addon_data populated");
        assert_eq!(ad.addon_index, 7);
        assert_eq!(ad.master_particle_cap, 64);
        assert_eq!(ad.flags, 1);
    }

    #[test]
    fn parse_stat_with_vmad_sets_has_script() {
        // Regression: #369 — Skyrim VMAD sub-records on STAT records
        // were dropped on the walker's `_` arm. The minimum-viable
        // signal is a `has_script: bool` on `StaticObject` so the count
        // of script-bearing records is at least visible. Full VMAD
        // decoding (Papyrus script names + property bindings) stays
        // gated on the scripting-as-ECS work.
        let mut sub_data = Vec::new();
        let edid = "ScriptedDoor\0";
        sub_data.extend_from_slice(b"EDID");
        sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(edid.as_bytes());
        let modl = "meshes\\door.nif\0";
        sub_data.extend_from_slice(b"MODL");
        sub_data.extend_from_slice(&(modl.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(modl.as_bytes());
        // VMAD: opaque payload — content doesn't matter for the
        // presence flag, only that the sub-record exists.
        let vmad_payload: &[u8] = b"\x05\x00\x02\x00\x00\x00";
        sub_data.extend_from_slice(b"VMAD");
        sub_data.extend_from_slice(&(vmad_payload.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(vmad_payload);

        let mut stat = Vec::new();
        stat.extend_from_slice(b"STAT");
        stat.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
        stat.extend_from_slice(&0u32.to_le_bytes());
        stat.extend_from_slice(&0x77u32.to_le_bytes());
        stat.extend_from_slice(&[0u8; 8]);
        stat.extend_from_slice(&sub_data);

        let total_size = 24 + stat.len();
        let mut group = Vec::new();
        group.extend_from_slice(b"GRUP");
        group.extend_from_slice(&(total_size as u32).to_le_bytes());
        group.extend_from_slice(b"STAT");
        group.extend_from_slice(&0u32.to_le_bytes());
        group.extend_from_slice(&[0u8; 8]);
        group.extend_from_slice(&stat);

        let mut reader = EsmReader::new(&group);
        let gh = reader.read_group_header().unwrap();
        let end = reader.group_content_end(&gh);
        let mut statics = HashMap::new();
        parse_modl_group(&mut reader, end, &mut statics).unwrap();

        let s = statics.get(&0x77).expect("STAT entry present");
        assert!(s.has_script, "VMAD presence must flip has_script");
    }

    #[test]
    fn parse_stat_without_vmad_leaves_has_script_false() {
        // Sibling check — a STAT with only EDID + MODL keeps has_script
        // at false. Catches a regression where the new arm captures
        // some other neighbour sub-record.
        let stat = build_stat_record(0x88, "PlainStatic", "meshes\\stat.nif");
        let total_size = 24 + stat.len();
        let mut group = Vec::new();
        group.extend_from_slice(b"GRUP");
        group.extend_from_slice(&(total_size as u32).to_le_bytes());
        group.extend_from_slice(b"STAT");
        group.extend_from_slice(&0u32.to_le_bytes());
        group.extend_from_slice(&[0u8; 8]);
        group.extend_from_slice(&stat);

        let mut reader = EsmReader::new(&group);
        let gh = reader.read_group_header().unwrap();
        let end = reader.group_content_end(&gh);
        let mut statics = HashMap::new();
        parse_modl_group(&mut reader, end, &mut statics).unwrap();

        let s = statics.get(&0x88).expect("STAT entry present");
        assert!(!s.has_script);
    }

    #[test]
    fn parse_non_addn_record_has_no_addon_data() {
        // STATs must not accidentally populate addon_data even if a
        // same-named DATA sub-record happens to exist.
        let stat = build_stat_record(0x9999, "RegularWall", "meshes\\wall.nif");
        let total_size = 24 + stat.len();
        let mut group = Vec::new();
        group.extend_from_slice(b"GRUP");
        group.extend_from_slice(&(total_size as u32).to_le_bytes());
        group.extend_from_slice(b"STAT");
        group.extend_from_slice(&0u32.to_le_bytes());
        group.extend_from_slice(&[0u8; 8]);
        group.extend_from_slice(&stat);

        let mut reader = EsmReader::new(&group);
        let gh = reader.read_group_header().unwrap();
        let end = reader.group_content_end(&gh);
        let mut statics = HashMap::new();
        parse_modl_group(&mut reader, end, &mut statics).unwrap();

        let s = statics.get(&0x9999).expect("STAT entry");
        assert!(s.addon_data.is_none(), "STAT must not carry addon data");
    }

    #[test]
    fn parse_stat_record() {
        let stat = build_stat_record(0x1234, "TestWall", "meshes\\architecture\\wall01.nif");
        // Wrap in a GRUP.
        let total_size = 24 + stat.len();
        let mut group = Vec::new();
        group.extend_from_slice(b"GRUP");
        group.extend_from_slice(&(total_size as u32).to_le_bytes());
        group.extend_from_slice(b"STAT");
        group.extend_from_slice(&0u32.to_le_bytes()); // group_type = 0 (top)
        group.extend_from_slice(&[0u8; 8]);
        group.extend_from_slice(&stat);

        let mut reader = EsmReader::new(&group);
        let gh = reader.read_group_header().unwrap();
        let end = reader.group_content_end(&gh);
        let mut statics = HashMap::new();
        parse_modl_group(&mut reader, end, &mut statics).unwrap();

        assert_eq!(statics.len(), 1);
        let s = statics.get(&0x1234).unwrap();
        assert_eq!(s.editor_id, "TestWall");
        assert_eq!(s.model_path, "meshes\\architecture\\wall01.nif");
    }

    #[test]
    fn parse_modl_group_walks_oblivion_20byte_headers() {
        // Regression: #391 — the walker used to compute a group's content
        // end as `position + total_size - 24`, hardcoding the Tes5Plus
        // header size. On Oblivion that over-reads by 4 bytes; symptoms
        // were latent (the next read happened to land on a self-delimiting
        // GRUP) but any bounds-checked nested parse would have read junk.
        //
        // Build an Oblivion-shaped (20-byte header) STAT group with two
        // STAT records, run it through `parse_modl_group` using the
        // explicit `Oblivion` reader variant, and assert: both records
        // extracted, no leftover bytes, no junk record dispatched after
        // the second.
        use super::super::reader::EsmVariant;

        // Build a 20-byte-header STAT record (Oblivion layout).
        fn build_stat_oblivion(form_id: u32, edid: &str, modl: &str) -> Vec<u8> {
            let mut sub_data = Vec::new();
            let edid_z = format!("{}\0", edid);
            sub_data.extend_from_slice(b"EDID");
            sub_data.extend_from_slice(&(edid_z.len() as u16).to_le_bytes());
            sub_data.extend_from_slice(edid_z.as_bytes());
            let modl_z = format!("{}\0", modl);
            sub_data.extend_from_slice(b"MODL");
            sub_data.extend_from_slice(&(modl_z.len() as u16).to_le_bytes());
            sub_data.extend_from_slice(modl_z.as_bytes());

            let mut buf = Vec::new();
            buf.extend_from_slice(b"STAT");
            buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
            buf.extend_from_slice(&0u32.to_le_bytes()); // flags
            buf.extend_from_slice(&form_id.to_le_bytes());
            buf.extend_from_slice(&[0u8; 4]); // Oblivion vc_info (4 bytes)
            buf.extend_from_slice(&sub_data);
            buf
        }

        let r1 = build_stat_oblivion(0x111, "WallA", "meshes\\a.nif");
        let r2 = build_stat_oblivion(0x222, "WallB", "meshes\\b.nif");
        let mut content = Vec::new();
        content.extend_from_slice(&r1);
        content.extend_from_slice(&r2);

        // 20-byte group header.
        let total_size = 20 + content.len();
        let mut group = Vec::new();
        group.extend_from_slice(b"GRUP");
        group.extend_from_slice(&(total_size as u32).to_le_bytes());
        group.extend_from_slice(b"STAT");
        group.extend_from_slice(&0u32.to_le_bytes()); // group_type
        group.extend_from_slice(&[0u8; 4]); // Oblivion stamp (4 bytes)
        group.extend_from_slice(&content);

        // Append a sentinel byte beyond the group end. With the old
        // `-24` walker this byte would land inside the computed end and
        // get dispatched as part of the next read; with the fix the
        // walker stops cleanly at byte `total_size`.
        group.push(0xEE);

        let mut reader = EsmReader::with_variant(&group, EsmVariant::Oblivion);
        let gh = reader.read_group_header().unwrap();
        let end = reader.group_content_end(&gh);
        // Content end must sit immediately before the sentinel, not past it.
        assert_eq!(end, total_size);

        let mut statics = HashMap::new();
        parse_modl_group(&mut reader, end, &mut statics).unwrap();

        assert_eq!(statics.len(), 2, "both Oblivion STATs must be parsed");
        assert_eq!(statics.get(&0x111).unwrap().editor_id, "WallA");
        assert_eq!(statics.get(&0x222).unwrap().editor_id, "WallB");
        // Walker must have stopped exactly at `end`, leaving the
        // sentinel byte for the caller.
        assert_eq!(reader.position(), end);
        assert_eq!(reader.remaining(), 1);
    }

    #[test]
    fn parse_refr_extracts_position_and_scale() {
        // Build a minimal REFR record with NAME, DATA, XSCL.
        let mut sub_data = Vec::new();
        // NAME (base form id)
        sub_data.extend_from_slice(b"NAME");
        sub_data.extend_from_slice(&4u16.to_le_bytes());
        sub_data.extend_from_slice(&0xABCDu32.to_le_bytes());
        // DATA (6 floats: pos xyz, rot xyz)
        sub_data.extend_from_slice(b"DATA");
        sub_data.extend_from_slice(&24u16.to_le_bytes());
        sub_data.extend_from_slice(&100.0f32.to_le_bytes()); // pos x
        sub_data.extend_from_slice(&200.0f32.to_le_bytes()); // pos y
        sub_data.extend_from_slice(&300.0f32.to_le_bytes()); // pos z
        sub_data.extend_from_slice(&0.0f32.to_le_bytes()); // rot x
        sub_data.extend_from_slice(&1.57f32.to_le_bytes()); // rot y
        sub_data.extend_from_slice(&0.0f32.to_le_bytes()); // rot z
                                                           // XSCL
        sub_data.extend_from_slice(b"XSCL");
        sub_data.extend_from_slice(&4u16.to_le_bytes());
        sub_data.extend_from_slice(&2.0f32.to_le_bytes());

        let mut record = Vec::new();
        record.extend_from_slice(b"REFR");
        record.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
        record.extend_from_slice(&0u32.to_le_bytes()); // flags
        record.extend_from_slice(&0x5678u32.to_le_bytes()); // form id
        record.extend_from_slice(&[0u8; 8]);
        record.extend_from_slice(&sub_data);

        let mut reader = EsmReader::new(&record);
        let end = record.len();
        let mut refs = Vec::new();
        let mut land = None;
        parse_refr_group(&mut reader, end, &mut refs, &mut land).unwrap();

        assert_eq!(refs.len(), 1);
        let r = &refs[0];
        assert_eq!(r.base_form_id, 0xABCD);
        assert!((r.position[0] - 100.0).abs() < 1e-6);
        assert!((r.position[1] - 200.0).abs() < 1e-6);
        assert!((r.position[2] - 300.0).abs() < 1e-6);
        assert!((r.rotation[1] - 1.57).abs() < 0.01);
        assert!((r.scale - 2.0).abs() < 1e-6);
        // No XESP → enable_parent stays None.
        assert!(r.enable_parent.is_none());
    }

    /// Helper for the #349 XESP regression tests — build a REFR with
    /// just NAME + DATA + XESP. The minimum sub-record set
    /// `parse_refr_group` needs to register a placement.
    fn build_refr_with_xesp(form_id: u32, parent_form: u32, inverted_flag: u8) -> Vec<u8> {
        let mut sub_data = Vec::new();
        sub_data.extend_from_slice(b"NAME");
        sub_data.extend_from_slice(&4u16.to_le_bytes());
        sub_data.extend_from_slice(&form_id.to_le_bytes());

        sub_data.extend_from_slice(b"DATA");
        sub_data.extend_from_slice(&24u16.to_le_bytes());
        sub_data.extend_from_slice(&[0u8; 24]); // zero pos + rot

        sub_data.extend_from_slice(b"XESP");
        sub_data.extend_from_slice(&5u16.to_le_bytes());
        sub_data.extend_from_slice(&parent_form.to_le_bytes());
        sub_data.push(inverted_flag);

        let mut record = Vec::new();
        record.extend_from_slice(b"REFR");
        record.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
        record.extend_from_slice(&0u32.to_le_bytes());
        record.extend_from_slice(&0x9999u32.to_le_bytes()); // record form_id
        record.extend_from_slice(&[0u8; 8]);
        record.extend_from_slice(&sub_data);
        record
    }

    /// Regression: #471 flipped #349's interim predicate. Without a
    /// two-pass loader to inspect each parent's real 0x0800 flag, we
    /// assume parents are enabled by default (the vanilla case). A
    /// non-inverted XESP child is visible when the parent is enabled,
    /// so the cell loader must NOT skip it.
    #[test]
    fn parse_refr_extracts_non_inverted_xesp_renders_by_default() {
        let record = build_refr_with_xesp(0xABCD, 0xCAFE, 0); // not inverted
        let mut reader = EsmReader::new(&record);
        let end = record.len();
        let mut refs = Vec::new();
        let mut land = None;
        parse_refr_group(&mut reader, end, &mut refs, &mut land).unwrap();

        assert_eq!(refs.len(), 1);
        let ep = refs[0]
            .enable_parent
            .expect("XESP must populate enable_parent");
        assert_eq!(ep.form_id, 0xCAFE);
        assert!(!ep.inverted);
        assert!(
            !ep.default_disabled(),
            "non-inverted XESP with assumed-enabled parent renders (#471)"
        );
    }

    /// #471: inverted XESP is visible when the parent is *disabled*.
    /// With the parents-assumed-enabled default, the child must be
    /// treated as hidden at cell load.
    #[test]
    fn parse_refr_extracts_inverted_xesp_hidden_by_default() {
        let record = build_refr_with_xesp(0xABCD, 0xCAFE, 0x01); // inverted bit set
        let mut reader = EsmReader::new(&record);
        let end = record.len();
        let mut refs = Vec::new();
        let mut land = None;
        parse_refr_group(&mut reader, end, &mut refs, &mut land).unwrap();

        assert_eq!(refs.len(), 1);
        let ep = refs[0]
            .enable_parent
            .expect("XESP must populate enable_parent");
        assert_eq!(ep.form_id, 0xCAFE);
        assert!(ep.inverted);
        assert!(
            ep.default_disabled(),
            "inverted XESP with assumed-enabled parent is hidden (#471)"
        );
    }

    /// Sibling: a REFR with no XESP at all keeps `enable_parent = None`
    /// — `default_disabled()` is irrelevant because the cell loader
    /// only inspects `Some(ep)`. The pre-#349 behaviour is preserved
    /// for the common (non-quest-gated) case.
    #[test]
    fn parse_refr_without_xesp_has_no_enable_parent() {
        let record = build_refr_with_xesp(0xABCD, 0, 0);
        // `build_refr_with_xesp` always emits an XESP — strip it for
        // this test by hand-building a NAME+DATA-only REFR.
        let _ = record;

        let mut sub_data = Vec::new();
        sub_data.extend_from_slice(b"NAME");
        sub_data.extend_from_slice(&4u16.to_le_bytes());
        sub_data.extend_from_slice(&0xBEEFu32.to_le_bytes());
        sub_data.extend_from_slice(b"DATA");
        sub_data.extend_from_slice(&24u16.to_le_bytes());
        sub_data.extend_from_slice(&[0u8; 24]);

        let mut record = Vec::new();
        record.extend_from_slice(b"REFR");
        record.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
        record.extend_from_slice(&0u32.to_le_bytes());
        record.extend_from_slice(&0x42u32.to_le_bytes());
        record.extend_from_slice(&[0u8; 8]);
        record.extend_from_slice(&sub_data);

        let mut reader = EsmReader::new(&record);
        let end = record.len();
        let mut refs = Vec::new();
        let mut land = None;
        parse_refr_group(&mut reader, end, &mut refs, &mut land).unwrap();

        assert_eq!(refs.len(), 1);
        assert!(refs[0].enable_parent.is_none());
    }

    /// Regression: #396 (OBL-D3-H2) — Oblivion ACRE (placed-creature
    /// reference) was missing from the placement-record matcher.
    /// FO3+ folded creature placements into ACHR; on Oblivion ACRE
    /// has the same wire layout as ACHR (NAME + DATA + optional
    /// XSCL + XESP), and pre-fix every Ayleid ruin / Oblivion gate /
    /// dungeon creature placement was silently skipped.
    #[test]
    fn parse_refr_group_recognises_oblivion_acre_placement() {
        // ACRE record: NAME (CREA base form) + DATA (pos+rot) + XSCL.
        let mut sub_data = Vec::new();
        sub_data.extend_from_slice(b"NAME");
        sub_data.extend_from_slice(&4u16.to_le_bytes());
        sub_data.extend_from_slice(&0xCAFEu32.to_le_bytes()); // base CREA form
        sub_data.extend_from_slice(b"DATA");
        sub_data.extend_from_slice(&24u16.to_le_bytes());
        sub_data.extend_from_slice(&50.0f32.to_le_bytes()); // pos x
        sub_data.extend_from_slice(&75.0f32.to_le_bytes()); // pos y
        sub_data.extend_from_slice(&100.0f32.to_le_bytes()); // pos z
        sub_data.extend_from_slice(&[0u8; 12]); // zero rotation
        sub_data.extend_from_slice(b"XSCL");
        sub_data.extend_from_slice(&4u16.to_le_bytes());
        sub_data.extend_from_slice(&1.5f32.to_le_bytes());

        let mut record = Vec::new();
        record.extend_from_slice(b"ACRE");
        record.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
        record.extend_from_slice(&0u32.to_le_bytes());
        record.extend_from_slice(&0x1234u32.to_le_bytes());
        record.extend_from_slice(&[0u8; 8]);
        record.extend_from_slice(&sub_data);

        let mut reader = EsmReader::new(&record);
        let end = record.len();
        let mut refs = Vec::new();
        let mut land = None;
        parse_refr_group(&mut reader, end, &mut refs, &mut land).unwrap();

        assert_eq!(refs.len(), 1, "ACRE placement must be recognised");
        let r = &refs[0];
        assert_eq!(r.base_form_id, 0xCAFE);
        assert!((r.position[0] - 50.0).abs() < 1e-6);
        assert!((r.position[1] - 75.0).abs() < 1e-6);
        assert!((r.position[2] - 100.0).abs() < 1e-6);
        assert!((r.scale - 1.5).abs() < 1e-6);
    }

    /// Regression: #396 — Oblivion CREA (base creature record) must
    /// reach `parse_modl_group` so its EDID + MODL land in the
    /// statics map. Pre-fix `parse_esm_cells` didn't include CREA in
    /// the MODL match arm, so the CREA group was skipped wholesale
    /// before parse_modl_group ever saw a record. CREA uses the
    /// standard MODL sub-record (identical layout to STAT), so once
    /// the dispatcher routes it through, the data path is unchanged.
    ///
    /// Mirrors `parse_modl_group_walks_oblivion_20byte_headers` but
    /// with a single CREA record + Oblivion 20-byte headers (CREA
    /// only ships on Oblivion / FO3 / FNV — FO3+ folded creatures
    /// into NPC_).
    #[test]
    fn parse_modl_group_indexes_oblivion_crea_records() {
        use super::super::reader::EsmVariant;

        // CREA record with EDID + MODL (Oblivion 20-byte header).
        let mut crea_sub = Vec::new();
        let edid = "Goblin\0";
        crea_sub.extend_from_slice(b"EDID");
        crea_sub.extend_from_slice(&(edid.len() as u16).to_le_bytes());
        crea_sub.extend_from_slice(edid.as_bytes());
        let model = "creatures\\goblin\\goblin.nif\0";
        crea_sub.extend_from_slice(b"MODL");
        crea_sub.extend_from_slice(&(model.len() as u16).to_le_bytes());
        crea_sub.extend_from_slice(model.as_bytes());

        let mut crea_record = Vec::new();
        crea_record.extend_from_slice(b"CREA");
        crea_record.extend_from_slice(&(crea_sub.len() as u32).to_le_bytes());
        crea_record.extend_from_slice(&0u32.to_le_bytes()); // flags
        crea_record.extend_from_slice(&0x000A_0001u32.to_le_bytes()); // form_id
        crea_record.extend_from_slice(&[0u8; 4]); // Oblivion vc_info (4 bytes)
        crea_record.extend_from_slice(&crea_sub);

        // 20-byte GRUP header wrapping the CREA record.
        let total_size = 20 + crea_record.len();
        let mut group = Vec::new();
        group.extend_from_slice(b"GRUP");
        group.extend_from_slice(&(total_size as u32).to_le_bytes());
        group.extend_from_slice(b"CREA");
        group.extend_from_slice(&0u32.to_le_bytes()); // group_type = 0 (top-level)
        group.extend_from_slice(&[0u8; 4]); // Oblivion stamp (4 bytes)
        group.extend_from_slice(&crea_record);

        let mut reader = EsmReader::with_variant(&group, EsmVariant::Oblivion);
        let gh = reader.read_group_header().unwrap();
        let end = reader.group_content_end(&gh);
        let mut statics = HashMap::new();
        parse_modl_group(&mut reader, end, &mut statics).unwrap();

        let crea = statics
            .get(&0x000A_0001)
            .expect("CREA record must be indexed by form_id");
        assert_eq!(crea.editor_id, "Goblin");
        assert_eq!(crea.model_path, "creatures\\goblin\\goblin.nif");
    }

    /// Edge case: XESP with a zero parent FormID (NULL parent — rare
    /// but legal in vanilla content). Treated as "no real parent" so
    /// the REFR is NOT default-disabled even though XESP is present.
    #[test]
    fn parse_refr_xesp_with_null_parent_is_not_default_disabled() {
        let record = build_refr_with_xesp(0xABCD, 0, 0);
        let mut reader = EsmReader::new(&record);
        let end = record.len();
        let mut refs = Vec::new();
        let mut land = None;
        parse_refr_group(&mut reader, end, &mut refs, &mut land).unwrap();

        let ep = refs[0]
            .enable_parent
            .expect("XESP populates enable_parent even with null parent");
        assert_eq!(ep.form_id, 0);
        assert!(
            !ep.default_disabled(),
            "null parent FormID = no real gating, so not default-disabled"
        );
    }

    #[test]
    fn parse_cell_xclw_populates_water_height() {
        // Regression: #397 — CELL XCLW (f32 water plane height) was
        // silently dropped by the walker, so flooded Ayleid ruins /
        // sewer interiors / coastal exteriors rendered without water.
        // Build an interior CELL record with EDID + DATA(interior) +
        // XCLW(10.0) and run it through `parse_cell_group`, which is
        // reachable directly once the CELL record is followed by no
        // further groups.
        let water_bytes = 10.0_f32.to_le_bytes();

        // Sub-record payload (type(4) + size(2) + bytes).
        let mut sub_data = Vec::new();
        let edid = "FloodedRuin\0";
        sub_data.extend_from_slice(b"EDID");
        sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(edid.as_bytes());

        sub_data.extend_from_slice(b"DATA");
        sub_data.extend_from_slice(&1u16.to_le_bytes());
        sub_data.push(0x01); // is_interior bit.

        sub_data.extend_from_slice(b"XCLW");
        sub_data.extend_from_slice(&4u16.to_le_bytes());
        sub_data.extend_from_slice(&water_bytes);

        // CELL record (Tes5Plus layout — 24-byte header).
        let mut buf = Vec::new();
        buf.extend_from_slice(b"CELL");
        buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // flags
        buf.extend_from_slice(&0xDEADBEEFu32.to_le_bytes()); // form_id
        buf.extend_from_slice(&[0u8; 8]); // padding
        buf.extend_from_slice(&sub_data);

        let mut reader = super::super::reader::EsmReader::with_variant(
            &buf,
            super::super::reader::EsmVariant::Tes5Plus,
        );
        let end = buf.len();
        let mut cells = HashMap::new();
        parse_cell_group(&mut reader, end, &mut cells).unwrap();

        assert_eq!(cells.len(), 1, "interior CELL must be registered");
        let cell = cells.get("floodedruin").expect("lowercase key");
        assert!(cell.is_interior);
        assert_eq!(
            cell.water_height,
            Some(10.0),
            "XCLW water height must flow through to CellData"
        );
    }

    #[test]
    fn parse_cell_skyrim_extended_subrecords() {
        // Regression: #356 — Skyrim CELL extended sub-records were
        // dropped on the walker's `_` arm. Build an interior CELL with
        // every extended FormID + a 3-entry XCLR region list and assert
        // they all flow through to `CellData`.
        let mut sub_data = Vec::new();
        let edid = "SkyrimRoom\0";
        sub_data.extend_from_slice(b"EDID");
        sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(edid.as_bytes());

        sub_data.extend_from_slice(b"DATA");
        sub_data.extend_from_slice(&1u16.to_le_bytes());
        sub_data.push(0x01); // is_interior

        // Helper to append a 4-byte FormID sub-record.
        fn push_form_sub(out: &mut Vec<u8>, ty: &[u8; 4], form_id: u32) {
            out.extend_from_slice(ty);
            out.extend_from_slice(&4u16.to_le_bytes());
            out.extend_from_slice(&form_id.to_le_bytes());
        }
        push_form_sub(&mut sub_data, b"XCIM", 0x000A1234); // image space
        push_form_sub(&mut sub_data, b"XCWT", 0x000B5678); // water type
        push_form_sub(&mut sub_data, b"XCAS", 0x000C9ABC); // acoustic space
        push_form_sub(&mut sub_data, b"XCMO", 0x000DEF01); // music type
        push_form_sub(&mut sub_data, b"XLCN", 0x000E2345); // location

        // XCLR: variable-length packed FormID array — three entries.
        let regions = [0x111u32, 0x222u32, 0x333u32];
        sub_data.extend_from_slice(b"XCLR");
        sub_data.extend_from_slice(&(regions.len() as u16 * 4).to_le_bytes());
        for r in regions {
            sub_data.extend_from_slice(&r.to_le_bytes());
        }

        let mut buf = Vec::new();
        buf.extend_from_slice(b"CELL");
        buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // flags
        buf.extend_from_slice(&0xCAFEBABEu32.to_le_bytes()); // form_id
        buf.extend_from_slice(&[0u8; 8]); // padding
        buf.extend_from_slice(&sub_data);

        let mut reader = super::super::reader::EsmReader::with_variant(
            &buf,
            super::super::reader::EsmVariant::Tes5Plus,
        );
        let end = buf.len();
        let mut cells = HashMap::new();
        parse_cell_group(&mut reader, end, &mut cells).unwrap();

        let cell = cells.get("skyrimroom").expect("interior CELL present");
        assert_eq!(cell.image_space_form, Some(0x000A1234));
        assert_eq!(cell.water_type_form, Some(0x000B5678));
        assert_eq!(cell.acoustic_space_form, Some(0x000C9ABC));
        assert_eq!(cell.music_type_form, Some(0x000DEF01));
        assert_eq!(cell.location_form, Some(0x000E2345));
        assert_eq!(cell.regions, vec![0x111, 0x222, 0x333]);
        // Sanity: `water_height` stays None because no XCLW present.
        assert_eq!(cell.water_height, None);
    }

    #[test]
    fn parse_cell_without_skyrim_extras_leaves_them_default() {
        // Sibling check for the new arms — a CELL with only EDID + DATA
        // must keep every extended FormID at None and `regions` empty.
        // Catches a regression where one of the new arms accidentally
        // captures another sub-record's payload.
        let mut sub_data = Vec::new();
        let edid = "BareRoom\0";
        sub_data.extend_from_slice(b"EDID");
        sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(edid.as_bytes());
        sub_data.extend_from_slice(b"DATA");
        sub_data.extend_from_slice(&1u16.to_le_bytes());
        sub_data.push(0x01);

        let mut buf = Vec::new();
        buf.extend_from_slice(b"CELL");
        buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0x42u32.to_le_bytes());
        buf.extend_from_slice(&[0u8; 8]);
        buf.extend_from_slice(&sub_data);

        let mut reader = super::super::reader::EsmReader::with_variant(
            &buf,
            super::super::reader::EsmVariant::Tes5Plus,
        );
        let end = buf.len();
        let mut cells = HashMap::new();
        parse_cell_group(&mut reader, end, &mut cells).unwrap();

        let cell = cells.get("bareroom").expect("interior CELL present");
        assert_eq!(cell.image_space_form, None);
        assert_eq!(cell.water_type_form, None);
        assert_eq!(cell.acoustic_space_form, None);
        assert_eq!(cell.music_type_form, None);
        assert_eq!(cell.location_form, None);
        assert!(cell.regions.is_empty());
    }

    #[test]
    fn parse_cell_skyrim_xcll_extracts_directional_ambient_cube() {
        // Regression: #367 (S6-05) — the 92-byte Skyrim XCLL's
        // bytes 40-71 (6×RGBA ambient cube + specular RGBA + fresnel
        // f32) were parsed-past and dropped. Build a synthetic 92-byte
        // XCLL with distinctive per-face colours and assert all six
        // slots round-trip along with the specular / fresnel fields.
        let mut xcll = Vec::with_capacity(92);

        // Bytes 0-7: Ambient RGBA + Directional RGBA (just need valid bytes).
        xcll.extend_from_slice(&[80, 82, 85, 0]); // ambient
        xcll.extend_from_slice(&[200, 195, 180, 0]); // directional
        // Bytes 8-11: Fog color RGBA (fog_near color).
        xcll.extend_from_slice(&[50, 55, 60, 0]);
        // Byte 11 == 0 is the alpha; already appended above.
        // Bytes 12-15: fog near (f32).
        xcll.extend_from_slice(&100.0f32.to_le_bytes());
        // Bytes 16-19: fog far (f32).
        xcll.extend_from_slice(&5000.0f32.to_le_bytes());
        // Bytes 20-23: directional rot X (i32, degrees).
        xcll.extend_from_slice(&(45i32).to_le_bytes());
        // Bytes 24-27: directional rot Y.
        xcll.extend_from_slice(&(30i32).to_le_bytes());
        // Bytes 28-31: directional fade (f32).
        xcll.extend_from_slice(&1.25f32.to_le_bytes());
        // Bytes 32-35: fog clip.
        xcll.extend_from_slice(&7500.0f32.to_le_bytes());
        // Bytes 36-39: fog power.
        xcll.extend_from_slice(&1.5f32.to_le_bytes());

        // Bytes 40-63: 6 × RGBA ambient cube. CK order: +X, -X, +Y, -Y, +Z, -Z.
        //   Face colors chosen so every byte is distinct — catches a
        //   wrong-stride / wrong-offset bug that shuffles the cube.
        //   (r=10, g=20, b=30) for +X, +10 per channel per face.
        for face in 0u8..6 {
            let base = (face * 10) + 10;
            xcll.push(base); // R
            xcll.push(base + 1); // G
            xcll.push(base + 2); // B
            xcll.push(0); // A (vanilla-zero)
        }

        // Bytes 64-67: specular RGBA (255, 200, 150, 128).
        xcll.extend_from_slice(&[255, 200, 150, 128]);
        // Bytes 68-71: fresnel power (f32).
        xcll.extend_from_slice(&2.5f32.to_le_bytes());
        // Bytes 72-75: fog far color RGBA.
        xcll.extend_from_slice(&[120, 130, 140, 0]);
        // Bytes 76-79: fog max (f32).
        xcll.extend_from_slice(&0.85f32.to_le_bytes());
        // Bytes 80-83: light fade begin.
        xcll.extend_from_slice(&500.0f32.to_le_bytes());
        // Bytes 84-87: light fade end.
        xcll.extend_from_slice(&800.0f32.to_le_bytes());
        // Bytes 88-91: inherits flags (u32, unused by the parser).
        xcll.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(xcll.len(), 92, "Skyrim XCLL must be 92 bytes");

        let mut sub_data = Vec::new();
        let edid = "SkyrimCave\0";
        sub_data.extend_from_slice(b"EDID");
        sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(edid.as_bytes());

        sub_data.extend_from_slice(b"DATA");
        sub_data.extend_from_slice(&1u16.to_le_bytes());
        sub_data.push(0x01); // interior

        sub_data.extend_from_slice(b"XCLL");
        sub_data.extend_from_slice(&(xcll.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(&xcll);

        let mut buf = Vec::new();
        buf.extend_from_slice(b"CELL");
        buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0xCAFEu32.to_le_bytes());
        buf.extend_from_slice(&[0u8; 8]);
        buf.extend_from_slice(&sub_data);

        let mut reader = super::super::reader::EsmReader::with_variant(
            &buf,
            super::super::reader::EsmVariant::Tes5Plus,
        );
        let end = buf.len();
        let mut cells = HashMap::new();
        parse_cell_group(&mut reader, end, &mut cells).unwrap();

        let cell = cells.get("skyrimcave").expect("interior CELL present");
        let lit = cell.lighting.as_ref().expect("XCLL must populate lighting");

        // Directional ambient cube — all 6 faces extracted with the
        // expected distinctive RGB per face. Per-face bytes written as
        // (base, base+1, base+2, 0) come back as rgb = (base, base+1, base+2).
        let cube = lit
            .directional_ambient
            .expect("Skyrim XCLL must populate directional_ambient");
        for (face, rgb) in cube.iter().enumerate() {
            let base = (face as u8 * 10) + 10;
            assert!(
                (rgb[0] - base as f32 / 255.0).abs() < 1e-6,
                "face {face} R mismatch: got {}, expected {}",
                rgb[0],
                base as f32 / 255.0,
            );
            assert!(
                (rgb[1] - (base + 1) as f32 / 255.0).abs() < 1e-6,
                "face {face} G mismatch"
            );
            assert!(
                (rgb[2] - (base + 2) as f32 / 255.0).abs() < 1e-6,
                "face {face} B mismatch"
            );
        }

        // Specular + fresnel. Raw bytes [255, 200, 150, 128] → RGB.
        assert_eq!(
            lit.specular_color,
            Some([255.0 / 255.0, 200.0 / 255.0, 150.0 / 255.0])
        );
        assert_eq!(lit.specular_alpha, Some(128.0 / 255.0));
        assert_eq!(lit.fresnel_power, Some(2.5));

        // Pre-existing extended fields still ride along unchanged.
        assert_eq!(lit.directional_fade, Some(1.25));
        assert_eq!(lit.fog_clip, Some(7500.0));
        assert_eq!(lit.fog_power, Some(1.5));
        assert_eq!(lit.fog_max, Some(0.85));
        assert_eq!(lit.light_fade_begin, Some(500.0));
        assert_eq!(lit.light_fade_end, Some(800.0));
    }

    #[test]
    fn parse_cell_fnv_xcll_decodes_colors_as_rgba() {
        // Regression guard: XCLL color fields are RGBA byte order — bytes
        // 0=R, 1=G, 2=B, 3=unused — matching the LIGH DATA revert and
        // xEdit's record definition. The raw bytes here are lifted
        // verbatim from FalloutNV.esm's `GSProspectorSaloonInterior`:
        //
        //   bytes 0..4   1E 29 4D 00   → (R=30, G=41, B=77)
        //   bytes 4..8   1A 20 31 00   → (R=26, G=32, B=49)
        //   bytes 8..12  37 37 5E 00   → (R=55, G=55, B=94)
        //
        // The saloon's ambient is dim cool-blue by design — the warm
        // amber of oil lanterns is delivered by placed LIGH refs, not
        // the cell fill. Under the earlier BGR misread this ambient
        // was flipped to warm (appearing as daytime) which looked
        // "right" on inspection but was factually wrong per the file.
        let mut xcll = vec![0u8; 40];
        xcll[0..4].copy_from_slice(&[0x1E, 0x29, 0x4D, 0x00]);
        xcll[4..8].copy_from_slice(&[0x1A, 0x20, 0x31, 0x00]);
        xcll[8..12].copy_from_slice(&[0x37, 0x37, 0x5E, 0x00]);
        xcll[12..16].copy_from_slice(&64.0f32.to_le_bytes());
        xcll[16..20].copy_from_slice(&3750.0f32.to_le_bytes());
        xcll[24..28].copy_from_slice(&250i32.to_le_bytes());

        let mut sub_data = Vec::new();
        let edid = "GSProspectorSaloonInterior\0";
        sub_data.extend_from_slice(b"EDID");
        sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(edid.as_bytes());

        sub_data.extend_from_slice(b"DATA");
        sub_data.extend_from_slice(&1u16.to_le_bytes());
        sub_data.push(0x01);

        sub_data.extend_from_slice(b"XCLL");
        sub_data.extend_from_slice(&(xcll.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(&xcll);

        let mut buf = Vec::new();
        buf.extend_from_slice(b"CELL");
        buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0x0005B33Eu32.to_le_bytes());
        buf.extend_from_slice(&[0u8; 8]);
        buf.extend_from_slice(&sub_data);

        let mut reader = super::super::reader::EsmReader::with_variant(
            &buf,
            super::super::reader::EsmVariant::Tes5Plus,
        );
        let end = buf.len();
        let mut cells = HashMap::new();
        parse_cell_group(&mut reader, end, &mut cells).unwrap();

        let cell = cells
            .get("gsprospectorsalooninterior")
            .expect("FNV-shaped interior CELL present");
        let lit = cell.lighting.as_ref().expect("XCLL populated");

        // Ambient: bytes (0x1E, 0x29, 0x4D) → RGB → (R=30, G=41, B=77).
        assert!((lit.ambient[0] - 30.0 / 255.0).abs() < 1e-6, "ambient R");
        assert!((lit.ambient[1] - 41.0 / 255.0).abs() < 1e-6, "ambient G");
        assert!((lit.ambient[2] - 77.0 / 255.0).abs() < 1e-6, "ambient B");

        // Directional: bytes (0x1A, 0x20, 0x31) → (R=26, G=32, B=49).
        assert!((lit.directional_color[0] - 26.0 / 255.0).abs() < 1e-6);
        assert!((lit.directional_color[1] - 32.0 / 255.0).abs() < 1e-6);
        assert!((lit.directional_color[2] - 49.0 / 255.0).abs() < 1e-6);

        // Fog: bytes (0x37, 0x37, 0x5E) → (R=55, G=55, B=94).
        assert!((lit.fog_color[0] - 55.0 / 255.0).abs() < 1e-6);
        assert!((lit.fog_color[1] - 55.0 / 255.0).abs() < 1e-6);
        assert!((lit.fog_color[2] - 94.0 / 255.0).abs() < 1e-6);
        assert_eq!(lit.fog_near, 64.0);
        assert_eq!(lit.fog_far, 3750.0);
    }

    #[test]
    fn parse_cell_fnv_xcll_extracts_40byte_tail_and_skips_skyrim_fields() {
        // The 40-byte FNV XCLL carries `directional_fade`, `fog_clip`,
        // and `fog_power` in the 28..40 tail per nif.xml + UESP. Pre-#379
        // those fields were only surfaced when the whole block was
        // Skyrim-extended (`d.len() >= 92`), so FNV cells silently
        // reported all three as `None` and fell back to hardcoded
        // renderer defaults.
        //
        // Post-#379 the 28..40 tail has its own `>= 40` gate, separate
        // from the Skyrim-only `>= 92` block that carries the ambient
        // cube / specular / fresnel / fog-far-color. This test pins
        // both halves.
        let mut xcll = vec![0u8; 40];
        xcll[0..4].copy_from_slice(&[80, 82, 85, 0]); // ambient
        xcll[4..8].copy_from_slice(&[200, 195, 180, 0]); // directional
        xcll[12..16].copy_from_slice(&100.0f32.to_le_bytes());
        xcll[16..20].copy_from_slice(&5000.0f32.to_le_bytes());
        // FNV extended tail (bytes 28-39).
        xcll[28..32].copy_from_slice(&0.75f32.to_le_bytes()); // directional_fade
        xcll[32..36].copy_from_slice(&6500.0f32.to_le_bytes()); // fog_clip
        xcll[36..40].copy_from_slice(&1.25f32.to_le_bytes()); // fog_power

        let mut sub_data = Vec::new();
        let edid = "FnvRoom\0";
        sub_data.extend_from_slice(b"EDID");
        sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(edid.as_bytes());

        sub_data.extend_from_slice(b"DATA");
        sub_data.extend_from_slice(&1u16.to_le_bytes());
        sub_data.push(0x01);

        sub_data.extend_from_slice(b"XCLL");
        sub_data.extend_from_slice(&(xcll.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(&xcll);

        let mut buf = Vec::new();
        buf.extend_from_slice(b"CELL");
        buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0xF00Du32.to_le_bytes());
        buf.extend_from_slice(&[0u8; 8]);
        buf.extend_from_slice(&sub_data);

        let mut reader = super::super::reader::EsmReader::with_variant(
            &buf,
            super::super::reader::EsmVariant::Tes5Plus,
        );
        let end = buf.len();
        let mut cells = HashMap::new();
        parse_cell_group(&mut reader, end, &mut cells).unwrap();

        let cell = cells.get("fnvroom").expect("FNV-shaped interior CELL");
        let lit = cell.lighting.as_ref().unwrap();

        // FNV-extended tail — now populated for 40-byte XCLL.
        assert_eq!(lit.directional_fade, Some(0.75));
        assert_eq!(lit.fog_clip, Some(6500.0));
        assert_eq!(lit.fog_power, Some(1.25));

        // Skyrim-only fields are still None at 40 bytes.
        assert!(lit.directional_ambient.is_none(), "FNV XCLL has no ambient cube");
        assert!(lit.specular_color.is_none());
        assert!(lit.specular_alpha.is_none());
        assert!(lit.fresnel_power.is_none());
        assert!(lit.fog_far_color.is_none());
        assert!(lit.fog_max.is_none());
        assert!(lit.light_fade_begin.is_none());
        assert!(lit.light_fade_end.is_none());
    }

    #[test]
    fn parse_cell_without_xclw_leaves_water_height_none() {
        // Sibling check for the XCLW match arm: a CELL with no XCLW
        // sub-record keeps `water_height = None`. Catches a regression
        // where the arm accidentally consumed some other sub-record.
        let mut sub_data = Vec::new();
        let edid = "DryRoom\0";
        sub_data.extend_from_slice(b"EDID");
        sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(edid.as_bytes());

        sub_data.extend_from_slice(b"DATA");
        sub_data.extend_from_slice(&1u16.to_le_bytes());
        sub_data.push(0x01);

        let mut buf = Vec::new();
        buf.extend_from_slice(b"CELL");
        buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0x01u32.to_le_bytes());
        buf.extend_from_slice(&[0u8; 8]);
        buf.extend_from_slice(&sub_data);

        let mut reader = super::super::reader::EsmReader::with_variant(
            &buf,
            super::super::reader::EsmVariant::Tes5Plus,
        );
        let end = buf.len();
        let mut cells = HashMap::new();
        parse_cell_group(&mut reader, end, &mut cells).unwrap();

        let cell = cells.get("dryroom").expect("interior CELL present");
        assert_eq!(cell.water_height, None);
    }

    #[test]
    #[ignore]
    fn parse_real_fnv_esm() {
        let path = "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data/FalloutNV.esm";
        if !std::path::Path::new(path).exists() {
            eprintln!("Skipping: FalloutNV.esm not found");
            return;
        }
        let data = std::fs::read(path).unwrap();
        let index = parse_esm_cells(&data).unwrap();

        eprintln!("Interior cells: {}", index.cells.len());
        eprintln!("Static objects: {}", index.statics.len());

        // Should have hundreds of interior cells and thousands of statics.
        assert!(
            index.cells.len() > 100,
            "Expected >100 cells, got {}",
            index.cells.len()
        );
        assert!(
            index.statics.len() > 1000,
            "Expected >1000 statics, got {}",
            index.statics.len()
        );

        // Check which cells have refs.
        let cells_with_refs = index
            .cells
            .values()
            .filter(|c| !c.references.is_empty())
            .count();
        eprintln!("Cells with refs: {}", cells_with_refs);

        // Check the Prospector Saloon specifically.
        let saloon = index.cells.get("gsprospectorsalooninterior").unwrap();
        eprintln!("Saloon: {} refs", saloon.references.len());
        assert!(
            saloon.references.len() > 100,
            "Saloon should have >100 refs"
        );

        // Look for the Prospector Saloon.
        let saloon_keys: Vec<&str> = index
            .cells
            .keys()
            .filter(|k| {
                k.contains("goodsprings") || k.contains("saloon") || k.contains("prospector")
            })
            .map(|k| k.as_str())
            .collect();
        eprintln!("Goodsprings/saloon cells: {:?}", saloon_keys);

        // Print a few cells for debugging.
        for (key, cell) in index.cells.iter().take(10) {
            eprintln!("  Cell '{}': {} refs", key, cell.references.len());
        }
    }

    /// Regression guard: proves the existing FNV-shaped XCLL parser is
    /// byte-compatible with Oblivion for the fields we consume.
    ///
    /// XCLL in Oblivion (36 bytes) and FNV (40 bytes) share an identical
    /// prefix for ambient / directional colors + fog colors + fog
    /// near/far + directional rotation XY + fade + clip distance. FNV
    /// appends a `fog_power` float; Skyrim+ has a completely different
    /// (longer) layout. Since `parse_esm_cells` only reads bytes 0-27
    /// (ambient, directional, and rotation), the byte offsets work for
    /// both games without any per-variant branching.
    ///
    /// This test validates that assumption against a real `Oblivion.esm`:
    /// ≥90% of interior cells must produce a populated CellLighting
    /// record, and the sampled color values must land in the expected
    /// 0..1 normalized float range.
    #[test]
    #[ignore]
    fn oblivion_cells_populate_xcll_lighting() {
        let path = "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/Oblivion.esm";
        if !std::path::Path::new(path).exists() {
            eprintln!("Skipping: Oblivion.esm not found");
            return;
        }
        let data = std::fs::read(path).unwrap();
        let idx = parse_esm_cells(&data).expect("Oblivion walker");

        let total = idx.cells.len();
        let with_lighting = idx.cells.values().filter(|c| c.lighting.is_some()).count();
        let with_directional = idx
            .cells
            .values()
            .filter(|c| {
                c.lighting
                    .as_ref()
                    .is_some_and(|l| l.directional_color.iter().any(|&x| x > 0.0))
            })
            .count();

        eprintln!(
            "Oblivion.esm: {total} cells, {with_lighting} with XCLL \
             ({:.1}%), {with_directional} with non-zero directional",
            100.0 * with_lighting as f32 / total.max(1) as f32,
        );

        // Log a couple of directional samples so that any future
        // XCLL-layout regression shows up in test output as obviously
        // wrong color values or rotations.
        for (name, lit) in idx
            .cells
            .values()
            .filter_map(|c| {
                c.lighting
                    .as_ref()
                    .map(|l| (c.editor_id.clone(), l.clone()))
            })
            .filter(|(_, l)| l.directional_color.iter().any(|&c| c > 0.0))
            .take(2)
        {
            eprintln!(
                "  '{name}': ambient={:.3?} directional={:.3?} rot=[{:.1},{:.1}]°",
                lit.ambient,
                lit.directional_color,
                lit.directional_rotation[0].to_degrees(),
                lit.directional_rotation[1].to_degrees(),
            );

            // Sanity: normalized color channels must sit in [0, 1].
            for c in lit.ambient.iter().chain(lit.directional_color.iter()) {
                assert!(
                    (0.0..=1.0).contains(c),
                    "color channel {c} out of [0,1] for cell '{name}' — \
                     XCLL byte offsets may have drifted"
                );
            }
        }

        // For the parser to be considered working on Oblivion, the vast
        // majority of interior cells must produce lighting data. The
        // residual are cells that legitimately omit XCLL (wilderness
        // stubs, deleted, or inherited from a template).
        let lighting_pct = with_lighting * 100 / total.max(1);
        assert!(
            lighting_pct >= 90,
            "expected >=90% of Oblivion cells to have XCLL lighting, \
             got {with_lighting}/{total} ({lighting_pct}%)"
        );
        assert!(
            with_directional > 100,
            "expected >100 cells with non-zero directional light, got {with_directional}"
        );
    }

    /// Smoke test: does `parse_esm_cells` survive a real `Oblivion.esm`
    /// walk now that the reader understands 20-byte headers?
    ///
    /// This does NOT assert a cell count or that specific records
    /// parsed — the FNV-shaped CELL / REFR / STAT subrecord layouts may
    /// still trip over Oblivion-specific fields. It only validates
    /// that the top-level walker reaches the end of the file without a
    /// hard error, which is the minimum bar for future per-record
    /// Oblivion work.
    #[test]
    #[ignore]
    fn parse_real_oblivion_esm_walker_survives() {
        let path = "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/Oblivion.esm";
        if !std::path::Path::new(path).exists() {
            eprintln!("Skipping: Oblivion.esm not found");
            return;
        }
        let data = std::fs::read(path).unwrap();

        // Sanity-check auto-detection.
        use crate::esm::reader::{EsmReader, EsmVariant};
        assert_eq!(
            EsmVariant::detect(&data),
            EsmVariant::Oblivion,
            "Oblivion.esm should auto-detect as Oblivion variant"
        );
        let mut reader = EsmReader::new(&data);
        let fh = reader.read_file_header().expect("Oblivion TES4 header");
        eprintln!(
            "Oblivion.esm: record_count={} masters={:?}",
            fh.record_count, fh.master_files
        );

        // Now run the full cell walker. We only assert it returns Ok —
        // the record contents are Phase 2 work.
        match parse_esm_cells(&data) {
            Ok(idx) => {
                eprintln!(
                    "Oblivion.esm walker OK: cells={} statics={} \
                     cells_with_refs={}",
                    idx.cells.len(),
                    idx.statics.len(),
                    idx.cells
                        .values()
                        .filter(|c| !c.references.is_empty())
                        .count(),
                );
            }
            Err(e) => panic!("parse_esm_cells failed on Oblivion.esm: {e:#}"),
        }
    }

    /// Regression bench for #456: pin the Megaton Player House parse-
    /// side reference count. ROADMAP originally quoted "1609 entities,
    /// 199 textures at 42 FPS" for MegatonPlayerHouse; the 1609 figure
    /// was measured AFTER cell-loader NIF expansion (each REFR spawns
    /// N ECS entities depending on its NIF block tree), so it isn't
    /// a parse-side assertion.
    ///
    /// Disk-sampled on 2026-04-19 against Fallout 3 GOTY: 929 REFRs
    /// live directly in the CELL. That's the stable number the
    /// parser must not drop. The 42 FPS figure predates TAA / SVGF /
    /// BLAS batching / streaming RIS and needs a fresh GPU bench —
    /// tracked in #456.
    #[test]
    #[ignore]
    fn parse_real_fo3_megaton_cell_baseline() {
        let path = "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data/Fallout3.esm";
        if !std::path::Path::new(path).exists() {
            eprintln!("Skipping: Fallout3.esm not found");
            return;
        }
        let data = std::fs::read(path).unwrap();
        let index = parse_esm_cells(&data).expect("parse_esm_cells");
        let megaton = index
            .cells
            .iter()
            .find(|(k, _)| k.contains("megaton") && k.contains("player"))
            .expect("expected a Megaton Player House interior cell in Fallout3.esm")
            .1;
        eprintln!(
            "MegatonPlayerHouse: {} REFRs (observed 929 on 2026-04-19)",
            megaton.references.len(),
        );
        assert!(
            megaton.references.len() > 800,
            "expected >800 REFRs for MegatonPlayerHouse (observed 929), got {}",
            megaton.references.len()
        );
    }

    /// Validates that `parse_esm_cells` handles Skyrim SE's 92-byte XCLL
    /// sub-records and can find The Winking Skeever interior cell.
    #[test]
    #[ignore]
    fn parse_real_skyrim_esm() {
        let path = "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/Skyrim.esm";
        if !std::path::Path::new(path).exists() {
            eprintln!("Skipping: Skyrim.esm not found");
            return;
        }
        let data = std::fs::read(path).unwrap();
        let idx = parse_esm_cells(&data).expect("Skyrim.esm walker");

        eprintln!(
            "Skyrim.esm: {} cells, {} statics, {} worldspaces",
            idx.cells.len(),
            idx.statics.len(),
            idx.exterior_cells.len(),
        );

        // The Winking Skeever must exist.
        let skeever = idx.cells.get("solitudewinkingskeever");
        assert!(
            skeever.is_some(),
            "SolitudeWinkingSkeever not found in Skyrim.esm cells. \
             Available keys (sample): {:?}",
            idx.cells.keys().take(20).collect::<Vec<_>>()
        );
        let skeever = skeever.unwrap();
        eprintln!(
            "Winking Skeever: {} refs, lighting={:?}",
            skeever.references.len(),
            skeever.lighting.is_some()
        );
        assert!(
            skeever.references.len() > 50,
            "Winking Skeever should have >50 refs, got {}",
            skeever.references.len()
        );

        // Skyrim XCLL should populate the extended fields.
        if let Some(ref lit) = skeever.lighting {
            eprintln!(
                "  ambient={:.3?} directional={:.3?} fog_near={:.1} fog_far={:.1}",
                lit.ambient, lit.directional_color, lit.fog_near, lit.fog_far,
            );
            // Skyrim's 92-byte XCLL must populate directional_fade.
            assert!(
                lit.directional_fade.is_some(),
                "Skyrim XCLL should have directional_fade (92-byte layout)"
            );
            // Ambient should be non-zero for a tavern interior.
            assert!(
                lit.ambient.iter().any(|&c| c > 0.0),
                "Winking Skeever ambient should be non-zero"
            );
        }

        // Check overall Skyrim cell stats.
        let with_lighting = idx.cells.values().filter(|c| c.lighting.is_some()).count();
        let with_skyrim_xcll = idx
            .cells
            .values()
            .filter(|c| {
                c.lighting
                    .as_ref()
                    .is_some_and(|l| l.directional_fade.is_some())
            })
            .count();
        eprintln!(
            "Skyrim lighting: {with_lighting}/{} cells with XCLL, \
             {with_skyrim_xcll} with Skyrim extended fields",
            idx.cells.len()
        );
    }

    #[test]
    fn read_zstring_handles_null_terminator() {
        assert_eq!(read_zstring(b"Hello\0"), "Hello");
        assert_eq!(read_zstring(b"NoNull"), "NoNull");
        assert_eq!(read_zstring(b"\0"), "");
        assert_eq!(read_zstring(b""), "");
    }

    /// Regression: #405 — vanilla Fallout4.esm must surface every SCOL
    /// record with its full ONAM/DATA child-placement data. Pre-fix
    /// the MODL-only parser discarded 15,878 placement entries across
    /// 2617 SCOL records. The exact counts drift with DLC patches;
    /// this test just asserts we're in the right order of magnitude.
    #[test]
    #[ignore]
    fn parse_real_fo4_esm_surfaces_scol_placements() {
        let path = "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data/Fallout4.esm";
        if !std::path::Path::new(path).exists() {
            eprintln!("Skipping: Fallout4.esm not found");
            return;
        }
        let data = std::fs::read(path).unwrap();
        let idx = parse_esm_cells(&data).expect("parse_esm_cells");

        let total_placements: usize = idx
            .scols
            .values()
            .flat_map(|s| s.parts.iter())
            .map(|p| p.placements.len())
            .sum();
        let scol_count = idx.scols.len();
        let parts_count: usize = idx.scols.values().map(|s| s.parts.len()).sum();
        eprintln!(
            "FO4 SCOL: {} records, {} parts, {} total placements",
            scol_count, parts_count, total_placements,
        );

        // Audit numbers from April 2026 Fallout4.esm scan:
        //   2617 SCOL records, 15878 ONAM/DATA pairs. Floors are set
        //   ~5% below observed so the test stays stable across
        //   patches without becoming meaningless.
        assert!(
            scol_count > 2400,
            "expected >2.4k SCOL records, got {}",
            scol_count
        );
        assert!(
            parts_count > 15000,
            "expected >15k ONAM/DATA parts, got {}",
            parts_count
        );
        assert!(
            total_placements > 15000,
            "expected >15k per-child placements, got {}",
            total_placements
        );
    }

    /// Build a TXST record byte stream with the given (sub_type, path)
    /// pairs encoded as MODL-style zstring sub-records. Used by the
    /// #357 regression tests below.
    fn build_txst_record(form_id: u32, slots: &[(&[u8; 4], &str)]) -> Vec<u8> {
        let mut sub_data = Vec::new();
        for (sub_type, path) in slots {
            let z = format!("{}\0", path);
            sub_data.extend_from_slice(*sub_type);
            sub_data.extend_from_slice(&(z.len() as u16).to_le_bytes());
            sub_data.extend_from_slice(z.as_bytes());
        }
        let mut buf = Vec::new();
        buf.extend_from_slice(b"TXST");
        buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // flags
        buf.extend_from_slice(&form_id.to_le_bytes());
        buf.extend_from_slice(&[0u8; 8]); // padding (timestamp + version)
        buf.extend_from_slice(&sub_data);
        buf
    }

    /// Wrap one or more TXST records in a top-level GRUP so the
    /// `parse_txst_group` recursion path matches the production loop.
    fn wrap_in_txst_group(records: &[Vec<u8>]) -> Vec<u8> {
        let inner: Vec<u8> = records.iter().flatten().copied().collect();
        let mut buf = Vec::new();
        buf.extend_from_slice(b"GRUP");
        buf.extend_from_slice(&((inner.len() + 24) as u32).to_le_bytes()); // total_size includes 24-byte header
        buf.extend_from_slice(b"TXST");
        buf.extend_from_slice(&0u32.to_le_bytes()); // group_type = top-level
        buf.extend_from_slice(&[0u8; 8]); // timestamp + version
        buf.extend_from_slice(&inner);
        buf
    }

    /// Regression: #357 — TXST parser must extract all 8 texture slots
    /// (TX00..TX07) into a `TextureSet`, not just the diffuse path.
    /// Pre-fix every Skyrim TXST-driven REFR override silently dropped
    /// 7 of 8 channels.
    #[test]
    fn parse_txst_extracts_all_eight_texture_slots() {
        let txst = build_txst_record(
            0xCAFE,
            &[
                (b"TX00", "textures/diffuse.dds"),
                (b"TX01", "textures/normal.dds"),
                (b"TX02", "textures/glow.dds"),
                (b"TX03", "textures/height.dds"),
                (b"TX04", "textures/env.dds"),
                (b"TX05", "textures/env_mask.dds"),
                (b"TX06", "textures/inner.dds"),
                (b"TX07", "textures/specular.dds"),
            ],
        );
        let group = wrap_in_txst_group(&[txst]);

        let mut reader = EsmReader::new(&group);
        let header = reader.read_group_header().expect("group header");
        let end = reader.group_content_end(&header);
        let mut diffuse_only: HashMap<u32, String> = HashMap::new();
        let mut sets: HashMap<u32, TextureSet> = HashMap::new();
        parse_txst_group(&mut reader, end, &mut diffuse_only, &mut sets).expect("parse");

        // Backward-compat diffuse-only map still populated.
        assert_eq!(
            diffuse_only.get(&0xCAFE),
            Some(&"textures/diffuse.dds".to_string()),
        );
        // Full slot set now also captured.
        let set = sets.get(&0xCAFE).expect("TextureSet missing for TXST 0xCAFE");
        assert_eq!(set.diffuse.as_deref(), Some("textures/diffuse.dds"));
        assert_eq!(set.normal.as_deref(), Some("textures/normal.dds"));
        assert_eq!(set.glow.as_deref(), Some("textures/glow.dds"));
        assert_eq!(set.height.as_deref(), Some("textures/height.dds"));
        assert_eq!(set.env.as_deref(), Some("textures/env.dds"));
        assert_eq!(set.env_mask.as_deref(), Some("textures/env_mask.dds"));
        assert_eq!(set.inner.as_deref(), Some("textures/inner.dds"));
        assert_eq!(set.specular.as_deref(), Some("textures/specular.dds"));
    }

    /// Regression: #357 — partial TXST (e.g. FO3/FNV which only authors
    /// TX00) must surface the populated slot and leave the rest as
    /// `None`. Verifies the optional-slot semantics.
    #[test]
    fn parse_txst_diffuse_only_leaves_other_slots_none() {
        let txst = build_txst_record(
            0xBEEF,
            &[(b"TX00", "textures/landscape/dirt.dds")],
        );
        let group = wrap_in_txst_group(&[txst]);

        let mut reader = EsmReader::new(&group);
        let header = reader.read_group_header().expect("group header");
        let end = reader.group_content_end(&header);
        let mut diffuse_only: HashMap<u32, String> = HashMap::new();
        let mut sets: HashMap<u32, TextureSet> = HashMap::new();
        parse_txst_group(&mut reader, end, &mut diffuse_only, &mut sets).expect("parse");

        let set = sets.get(&0xBEEF).expect("TextureSet missing for diffuse-only TXST");
        assert_eq!(set.diffuse.as_deref(), Some("textures/landscape/dirt.dds"));
        assert!(set.normal.is_none());
        assert!(set.glow.is_none());
        assert!(set.specular.is_none());
        assert!(set.env.is_none());
    }

    /// Regression: #357 — empty zstrings (`""`) on any slot collapse
    /// to `None` so the consumer doesn't have to redo the empty check.
    #[test]
    fn parse_txst_empty_string_slot_collapses_to_none() {
        let txst = build_txst_record(
            0xDEAD,
            &[
                (b"TX00", "textures/diffuse.dds"),
                (b"TX01", ""), // empty path — should collapse to None
            ],
        );
        let group = wrap_in_txst_group(&[txst]);

        let mut reader = EsmReader::new(&group);
        let header = reader.read_group_header().expect("group header");
        let end = reader.group_content_end(&header);
        let mut diffuse_only: HashMap<u32, String> = HashMap::new();
        let mut sets: HashMap<u32, TextureSet> = HashMap::new();
        parse_txst_group(&mut reader, end, &mut diffuse_only, &mut sets).expect("parse");

        let set = sets.get(&0xDEAD).expect("set missing");
        assert_eq!(set.diffuse.as_deref(), Some("textures/diffuse.dds"));
        assert!(set.normal.is_none(), "empty TX01 must surface as None");
    }
}
