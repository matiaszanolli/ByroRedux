//! The parsed CELL record, extracted from ../mod.rs (#5805 type-hub split).
//!
//! Types: `CellData`.

use super::{CellLighting, CellOwnership, LandscapeData, PlacedRef};

/// A cell (interior or exterior) with its placed object references.
///
/// `Default` is the absent-everything cell: no DATA/XCLW/XCLL/LAND, no
/// references, every optional link `None`. Fixtures build cells as
/// `CellData { form_id, ..Default::default() }` so a new field is a
/// one-site edit (#5581; #4480 was a bin-crate test-build break from six
/// full literals missing one). The production walkers (`walkers.rs`,
/// `wrld.rs`) keep exhaustive literals on purpose: they destructure the
/// parsed sub-record bundle field by field, and the compiler checking
/// that coverage is the point there.
#[derive(Debug, Default)]
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
    /// CELL DATA bit 7: Show Sky / Behave Like Exterior. An interior with
    /// this flag deliberately exposes the outdoor sky through open geometry.
    /// `None` when DATA is absent in a partial override; merge inherits the
    /// base cell's flag in that case.
    pub show_sky: Option<bool>,
    /// Grid coordinates for exterior cells (None for interior).
    pub grid: Option<(i32, i32)>,
    /// Interior cell lighting (from XCLL subrecord).
    pub lighting: Option<CellLighting>,
    /// Landscape terrain data (from LAND record, exterior cells only).
    pub landscape: Option<LandscapeData>,
    /// Water plane height in Bethesda world units (Z-up), from the
    /// CELL record's XCLW sub-record. `None` when XCLW is absent or
    /// carries an explicit no-water sentinel; use
    /// [`water_height_is_explicit`](Self::water_height_is_explicit) to
    /// distinguish those cases before applying a worldspace fallback.
    /// Critical for flooded Ayleid ruins, sewer interiors, and coastal
    /// exterior cells. Same f32 semantics across Oblivion / FO3 / FNV /
    /// Skyrim. See #397.
    pub water_height: Option<f32>,
    /// Whether the CELL authored an XCLW sub-record, including an explicit
    /// no-water sentinel. This preserves the third state that `Option<f32>`
    /// alone cannot represent: absent XCLW inherits WRLD water, while an
    /// authored sentinel keeps the tile dry.
    pub water_height_is_explicit: bool,

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
    /// record) on TES4 / FO3 / FNV. A cell can override the worldspace
    /// CLMT default for itself. `None` means inherit from worldspace.
    /// TES5 / FO4 author the same tag as a REGN reference instead — see
    /// [`CellData::sky_region`] (#5463). See #693 / O3-N-05.
    pub climate_override: Option<u32>,
    /// "Sky/Weather from Region" (XCCM, FormID — references a REGN
    /// record) on TES5 / FO4 (#5463). Vanilla Skyrim authors it on
    /// interior cells only (214 in `Skyrim.esm`): an interior that shows
    /// the sky takes its sky and weather from this region. Decoded but not
    /// yet consumed — the interior sky-from-region consumer is forward
    /// scope. Never a CLMT, so it must not feed `climate_override`.
    pub sky_region: Option<u32>,
    /// Location (XLCN, FormID — references an LCTN record). Used by
    /// quest / Story Manager systems for "player is in location X"
    /// conditions.
    pub location_form: Option<u32>,
    /// Encounter zone (XEZN, FormID — references an ECZN record).
    /// Governs spawn scaling / faction ownership for the cell; the ECZN
    /// records themselves live in `EsmIndex.encounter_zones`. #4173 —
    /// pre-fix the sub-record fell to the CELL walker's catch-all, so the
    /// zone map was populated but un-linkable to any cell.
    pub encounter_zone_form: Option<u32>,
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
    /// Oblivion-era cell-level regional fog/sky colour tint from the
    /// `RCLR` sub-record (3-byte RGB). FO3+ replaced this concept with
    /// the LGTM / CLMT chain — vanilla post-Oblivion cells don't ship
    /// RCLR, but the field is kept cross-game so modded data parses
    /// cleanly everywhere. `None` for any cell that omits RCLR (the
    /// dominant case even on Oblivion — RCLR is editor-authored, rare
    /// in vanilla, occasional in mods). The renderer hasn't wired
    /// this into the LGTM fallback chain yet; see #970 / OBL-D3-NEW-06.
    pub regional_color_override: Option<[u8; 3]>,
    /// FO4+ PreCombined Mesh hashes from the cell's `XCRI` sub-record.
    /// Each hash points at a `meshes\precombined\<cell_formid:08x>_<hash:08x>_oc.nif`
    /// file (typically in `Fallout4 - MeshesExtra.ba2`). Empty for
    /// pre-FO4 cells and FO4 cells that don't use the precombined
    /// optimization (debug cells, some mod content).
    ///
    /// XCRI format (#1188 / #5484, per xEdit `wbDefinitionsFO4.pas`):
    /// `u32 mesh_count + u32 ref_count + N × u32 hashes +
    /// (ref_count / 2) × (u32 refr formid, u32 combined-mesh hash)`.
    /// The hashes here are the first half; the pair tail is
    /// [`absorbed_ref_bakes`].
    pub precombined_mesh_hashes: Vec<u32>,
    /// FO4+ REFRs the CK combined into the cell's precombined meshes —
    /// the XCRI pair tail, as (remapped REFR form id, combined-mesh
    /// hash). For the STAT/SCOL subset the cell loader skips individual
    /// placement *when that ref's bake actually spawned* (the hash key
    /// is what lets one missing `_oc.nif` un-skip only its own refs,
    /// #5484); spawning a baked ref individually would double-draw and
    /// z-fight. Non-STAT/SCOL types never appear in XCRI (the #5484
    /// census: 959,238 refs across Fallout4.esm + DLCs, 100% STAT/SCOL),
    /// so the #2699 "is the furniture baked?" question is closed: it is
    /// not — XPRI is the disjoint previs participant list and those
    /// placements always spawn individually (see the XPRI arm in
    /// `helpers.rs`). Empty for non-FO4 cells. #1188 / #5484.
    pub absorbed_ref_bakes: Vec<(u32, u32)>,
    /// Per-cell navmesh records (`NAVM`) collected from the cell's
    /// `Cell Persistent Children` (group_type 8) and `Cell Temporary
    /// Children` (group_type 9) GRUPs — both reached through the type-6
    /// `Cell Children` container, never as its siblings (#4169). In the
    /// shipped masters every authored NAVM is in fact a type-9 child
    /// (Skyrim 15 966, FO4 7 894, FNV 4 771; zero under type 8). NAVMs never appear at top level in vanilla Bethesda
    /// masters (`parse_esm`'s top-level dispatch arm exists for non-
    /// vanilla flatten-out mods only) — every authored NAVM nests
    /// inside one of these child GRUPs. The records here populate
    /// `EsmIndex.navmeshes` post-cell-walk via a drain step in
    /// `parse_esm`. Empty when the cell has no navmesh children, which
    /// is the common case for tiny interior cells. See #1272.
    pub navmeshes: Vec<crate::esm::records::NavmRecord>,
    /// `PGRD` pathgrids attached to this cell (#3598).
    ///
    /// Oblivion's ONLY navigation format — it authors zero `NAVI` and zero
    /// `NAVM`, so before this field every one of its 8,228 pathgrids was
    /// dropped and the title had no navigation data at all while every
    /// other supported game did. A cell carries at most one in vanilla, but
    /// this is a `Vec` to match `navmeshes` and to tolerate a plugin that
    /// authors more.
    pub pathgrids: Vec<crate::esm::records::PathGridRecord>,
    /// REFR/ACHR/ACRE FormIDs this specific plugin's own parse marked
    /// Deleted (the tombstone flag, `RECORD_FLAG_DELETED`) within this
    /// cell (EX-09/17 item 7, #2370). Deliberately transient, unlike
    /// every other field `merge_cell_override` inherits from `base` when
    /// `over` leaves it empty: this list only means something for the ONE
    /// override round that produced it (a later plugin re-declaring the
    /// same FormID as a live REFR is a legitimate un-delete, not a bug),
    /// so `merge_cell_override` deliberately does NOT extend it forward
    /// the way it does `absorbed_ref_bakes`. Consumed by
    /// `merge_placed_references`, which removes any matching FormID from
    /// the base cell's inherited `references` before folding this
    /// plugin's own additions/changes in. Empty for a cell no plugin in
    /// the load order ever deletes from (the common case).
    pub deleted_refs: Vec<u32>,
}
