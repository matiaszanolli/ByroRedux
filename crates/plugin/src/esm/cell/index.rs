//! The cell-parse result index and its merge machinery, extracted from
//! ../mod.rs (#5805 type-hub split).
//!
//! Types: `EsmCellIndex` (+ `cell_by_form_id` / `merge_from` /
//! `locate_refr`), `CellLocator`, `CellRef` / `OwnedCellRef`, and the
//! `parse_esm_cells*` entry points.

use super::worldspace::PNAM_INHERIT_LAND;
use super::{CellData, PlacedRef, StaticObject, TextureSet, WorldspaceRecord};
use anyhow::Result;
use byroredux_core::math::coord::EXTERIOR_CELL_UNITS;
use std::collections::{HashMap, HashSet};

/// Result of parsing an ESM file for cell loading.
#[derive(Debug, Default)]
pub struct EsmCellIndex {
    /// Interior cells, keyed by canonical lookup ID (lowercase). Authored
    /// EditorIDs remain the human-facing key; cells without EDID use the
    /// stable `cell_<FORMID>` identity produced by
    /// [`canonical_interior_cell_id`].
    pub cells: HashMap<String, CellData>,
    /// Exterior cells, keyed by (worldspace_name_lowercase, (grid_x, grid_y)).
    pub exterior_cells: HashMap<String, HashMap<(i32, i32), CellData>>,
    /// The one persistent CELL owned by each worldspace, keyed by the
    /// lowercased worldspace editor ID. Persistent exterior references are
    /// not tied to a streamed `(x, y)` tile: quest actors such as Skyrim's
    /// Hadvar and Ralof live here and must remain instantiated while the
    /// worldspace is active.
    pub worldspace_persistent_cells: HashMap<String, CellData>,
    /// All base object records with model paths, keyed by form ID.
    pub statics: HashMap<u32, StaticObject>,
    /// Landscape texture definitions: LTEX form ID → diffuse texture path.
    /// Resolved via LTEX.TNAM → TXST.TX00 (FO3+) or LTEX.ICON (Oblivion).
    pub landscape_textures: HashMap<u32, String>,
    /// Full landscape material sets keyed by LTEX form ID. FO3+ LTEX records
    /// point at TXST records through TNAM; retaining that association lets
    /// LAND consume the normal/specular channels instead of collapsing the
    /// terrain material back to TX00 diffuse-only. Oblivion ICON-authored
    /// LTEX records have no sibling TXST and therefore do not enter this map.
    pub landscape_texture_sets: HashMap<u32, TextureSet>,
    /// LTEX form ID → the TXST form ID its `TNAM` names (FO3+), kept
    /// unresolved so the join that fills `landscape_textures` and
    /// `landscape_texture_sets` runs against the merged load order.
    ///
    /// #6036 — the join used to run per plugin against that plugin's own
    /// TXSTs and then drop this map, so a DLC LTEX naming a master TXST never
    /// resolved: Honest Hearts' `ScrubBarrenWastes02GrassMix` (TNAM =
    /// `FalloutNV.esm` 0x000357BD) lost its 550 ATXT layers in Zion.
    /// [`Self::resolve_landscape_texture_sets`] re-runs it after every merge.
    pub landscape_texture_txsts: HashMap<u32, u32>,
    /// Authored vegetation mapping: LTEX form ID → the LTEX's `GNAM`
    /// grass references, in authored order.
    ///
    /// #4642 — xEdit defines the grasses as an array
    /// (`wbRArrayS('Grasses', wbFormIDCk(GNAM, 'Grass', [GRAS]))`): 151
    /// of 184 grass-bearing vanilla LTEXs author 2–4 species, which the
    /// old single-`u32` map silently collapsed to the last one. The
    /// reference list is kept independently of the texture-set lookup
    /// because a landscape layer can legitimately have associated grass
    /// models while its texture is unavailable (and conversely).
    /// Consumers must remap the LAND layer's LTEX form ID through this
    /// table before selecting authored ground-cover models.
    pub landscape_grasses: HashMap<u32, Vec<u32>>,
    /// LTEX form ID → the LTEX's editor ID (`EDID`).
    ///
    /// #4899 — the ground-cover affinity table (`groundcover_translate`,
    /// `exal-groundcover.md` §12.1) was derived by tokenising LTEX editor
    /// IDs, and its strongest rule is a `NoGrass` suffix. Grass/NoGrass
    /// variants of one landscape share a diffuse texture, so classifying the
    /// texture path instead inverted every such pair. LTEX records without
    /// an `EDID` are absent here.
    pub landscape_texture_names: HashMap<u32, String>,
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
    /// XTNM/XATO/XTXR override that points to a TXST can now apply the
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
    pub scols: HashMap<u32, crate::esm::records::ScolRecord>,
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
    pub packins: HashMap<u32, crate::esm::records::PkinRecord>,
    /// FO4+ MOVS (Movable Static) records keyed by form ID. Visually
    /// identical to STAT (one `MODL` pointer); MOVS distinguishes
    /// itself by being driven by Havok at runtime — the referenced
    /// mesh's `bhk` collision chain produces dynamic rigid-body motion
    /// when the cell is alive. Pre-#588 MOVS was routed through the
    /// MODL-only catch-all alongside STAT/FURN/etc., which preserved
    /// visual placement (REFRs targeting MOVS form IDs still rendered
    /// the right mesh) but silently dropped the distinguishing
    /// `LNAM` / `ZNAM` / `DEST` / `VMAD` sub-records. The dedicated
    /// parser captures those into [`MovableStaticRecord`](crate::esm::records::MovableStaticRecord)
    /// here pending physics / sound / destruction subsystems.
    ///
    /// Vanilla `Fallout4.esm` itself ships zero MOVS records — the
    /// impact is on DLC / mod content authoring breakable furniture,
    /// deployable workshop objects, and physics-puzzle props. See
    /// audit `FO4-DIM4-02` / #588.
    pub movables: HashMap<u32, crate::esm::records::MovableStaticRecord>,
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
    pub material_swaps: HashMap<u32, crate::esm::records::MaterialSwapRecord>,
    /// Starfield LMSW (Layered Material Swap) records keyed by form ID
    /// (#5757) — the swap tables REFR `XLMS` lists point at. Vanilla
    /// `Starfield.esm` ships 12,966; resolve a REFR's list through
    /// [`EsmCellIndex::layered_material_swap_entries`].
    pub layered_material_swaps: HashMap<u32, crate::esm::records::LayeredMaterialSwapRecord>,
    /// LTEX form ID → its Havok data (`HNAM` / `MNAM`, #6048): the
    /// ground material, friction and restitution the terrain collider of a
    /// tile painted with the texture carries.
    pub landscape_texture_havok: HashMap<u32, super::LandscapeTextureHavok>,
    /// Starfield MTPT (Material Path) form ID → its `Material` path
    /// (#5757): the source an LMSW entry names through
    /// `KeyMaterialPathForm`. Vanilla `Starfield.esm` ships 877.
    pub material_path_forms: HashMap<u32, String>,
    /// CELL form ID → where that cell lives in the three CELL maps above,
    /// built on first use by [`EsmCellIndex::cell_by_form_id`] and cleared
    /// by [`EsmCellIndex::merge_from`] (#5489). Crate-private: it is derived
    /// state, so it is neither parsed nor documented as index content.
    pub(crate) cell_form_id_lookup: std::sync::OnceLock<HashMap<u32, CellLocator>>,
    /// Placed REFR form ID → its cell slot and position in that cell's
    /// `references`, built on first use by [`EsmCellIndex::locate_refr`] and
    /// cleared by [`EsmCellIndex::merge_from`] (#6125). Derived state like
    /// `cell_form_id_lookup`; priced at ~13 bytes per placed REFR.
    pub(crate) refr_lookup: std::sync::OnceLock<RefrLookup>,
}

/// The reverse index behind [`EsmCellIndex::locate_refr`]: one entry per
/// placed REFR, pointing into a table with one slot per CELL (so a cell's key
/// is stored once, not once per reference).
#[derive(Debug, Default)]
pub(crate) struct RefrLookup {
    cells: Vec<CellLocator>,
    /// REFR form ID → (index into `cells`, index into that cell's `references`).
    refs: HashMap<u32, (u32, u32)>,
}

/// Where one CELL sits in an [`EsmCellIndex`] (see
/// [`EsmCellIndex::cell_by_form_id`]).
#[derive(Debug)]
pub(crate) enum CellLocator {
    Interior(String),
    Exterior(String, (i32, i32)),
    WorldspacePersistent(String),
}

/// Merge an override cell's REFRs onto its base in place: start from the
/// base cell's `references` and overlay `over`'s by REFR FormID
/// (last-write-wins per REFR), appending FormIDs the base didn't have. The
/// result becomes `over.references`, so the caller can then insert `over`
/// (which keeps the override's CELL-level fields) over the base.
///
/// A Bethesda DLC override CELL is a *partial* record — it re-emits only the
/// REFRs it adds or changes — so the whole-vec replacement this fixes
/// dropped every base REFR the DLC didn't re-emit (#1546). Deleted-REFR
/// tombstones (the 0x20 Deleted flag) are skipped at the REFR walk
/// (`walkers.rs`, `RECORD_FLAG_DELETED`, resolved #1660) and never appear in
/// `over` at all — this paragraph describes PLACEMENTS only; the parallel
/// story for base records is `EsmIndex::prune_deleted_records` (#3543),
/// which is a different mechanism reached through
/// `EsmIndex::deleted_record_metadata` — `deleted` (`over`'s own `CellData::deleted_refs`, EX-09/17
/// item 7 / #2370) is the separate signal that removes any matching FormID
/// `base` still carries, so a DLC that *deletes* a base-master REFR no
/// longer leaves the base copy resident across the merge (pre-fix, only the
/// single-plugin case — nothing to place either way — was actually correct;
/// the cross-plugin case silently kept the base copy, since the deletion
/// left no trace once the DLC's own `over.references` no longer mentioned
/// the FormID at all). FormID `0` (legacy fixtures with no REFR identity) is
/// never keyed — those refs always append, so distinct unnamed refs can't
/// collapse onto each other.
fn merge_placed_references(base: &[PlacedRef], over: &mut Vec<PlacedRef>, deleted: &[u32]) {
    let mut merged = base.to_vec();
    if !deleted.is_empty() {
        let tombstoned: HashSet<u32> = deleted.iter().copied().collect();
        merged.retain(|r| r.form_id == 0 || !tombstoned.contains(&r.form_id));
    }
    let mut by_form: HashMap<u32, usize> = merged
        .iter()
        .enumerate()
        .filter(|(_, r)| r.form_id != 0)
        .map(|(i, r)| (r.form_id, i))
        .collect();
    for over_ref in over.drain(..) {
        if over_ref.form_id != 0 {
            if let Some(&i) = by_form.get(&over_ref.form_id) {
                merged[i] = over_ref;
                continue;
            }
            by_form.insert(over_ref.form_id, merged.len());
        }
        merged.push(over_ref);
    }
    *over = merged;
}

fn merge_cell_references(base: &CellData, over: &mut CellData) {
    merge_placed_references(&base.references, &mut over.references, &over.deleted_refs);
}

/// Canonical human/CLI identity for an interior CELL. FO4 legitimately ships
/// interiors without EDID, so FormID is the stable fallback rather than a
/// reason to discard the record and its child hierarchy.
pub(crate) fn canonical_interior_cell_id(form_id: u32, editor_id: &str) -> String {
    if editor_id.is_empty() {
        format!("cell_{form_id:08X}")
    } else {
        editor_id.to_string()
    }
}

fn is_synthetic_interior_cell_id(form_id: u32, editor_id: &str) -> bool {
    editor_id.eq_ignore_ascii_case(&canonical_interior_cell_id(form_id, ""))
}

fn merge_navmeshes(
    base: &[crate::esm::records::NavmRecord],
    over: &mut Vec<crate::esm::records::NavmRecord>,
) {
    let mut merged = base.to_vec();
    let mut by_form: HashMap<u32, usize> = merged
        .iter()
        .enumerate()
        .filter(|(_, navm)| navm.form_id != 0)
        .map(|(index, navm)| (navm.form_id, index))
        .collect();
    for over_navm in over.drain(..) {
        if over_navm.form_id != 0 {
            if let Some(&index) = by_form.get(&over_navm.form_id) {
                merged[index] = over_navm;
                continue;
            }
            by_form.insert(over_navm.form_id, merged.len());
        }
        merged.push(over_navm);
    }
    *over = merged;
}

/// Merge a path grid list the way [`merge_navmeshes`] does: overlay `over`'s
/// by record FormID on the base's, appending FormIDs the base didn't have.
fn merge_pathgrids(
    base: &[crate::esm::records::PathGridRecord],
    over: &mut Vec<crate::esm::records::PathGridRecord>,
) {
    let mut merged = base.to_vec();
    let mut by_form: HashMap<u32, usize> = merged
        .iter()
        .enumerate()
        .filter(|(_, grid)| grid.form_id != 0)
        .map(|(index, grid)| (grid.form_id, index))
        .collect();
    for over_grid in over.drain(..) {
        if over_grid.form_id != 0 {
            if let Some(&index) = by_form.get(&over_grid.form_id) {
                merged[index] = over_grid;
                continue;
            }
            by_form.insert(over_grid.form_id, merged.len());
        }
        merged.push(over_grid);
    }
    *over = merged;
}

/// Apply Bethesda's CELL override contract at the parser boundary (#6086).
///
/// A plugin that overrides a CELL carries the whole CELL *record* — the CK
/// rewrites it — and the winning override replaces it, so a sub-record the
/// override omits is omitted on purpose: Update.esm drops `XCWT` from 28
/// marsh/river cells, a DLC drops `XCRI` to disable a cell's precombine.
/// Every CELL-record field therefore comes from `over` as parsed.
///
/// Only the cell's *child records* are separate records with their own
/// override chain, so only they inherit from `base` (#1546): the placed
/// references (with the incoming tombstones applied), the landscape, the
/// navmeshes and the pathgrids. The one identity-level exception is an EDID
/// the override doesn't carry (FO4 ships EDID-less interiors, #2911), where
/// the base's name stays so the cell keeps its alias.
///
/// The destructure of `base` has no `..`: a new `CellData` field is a
/// compile error here, forcing the inherit-or-not decision, as in
/// [`EsmCellIndex::merge_from`] (#4904).
pub(super) fn merge_cell_override(base: &CellData, over: &mut CellData) {
    let CellData {
        // Identity — the override is the same CELL by construction.
        form_id: _,
        editor_id: base_editor_id,
        is_interior: _,
        // CELL-record fields: the winning override's, absent or not.
        display_name: _,
        show_sky: _,
        grid: _,
        land_flags: _,
        lighting: _,
        water_height: _,
        water_height_is_explicit: _,
        image_space_form: _,
        water_type_form: _,
        acoustic_space_form: _,
        music_type_form: _,
        music_type_enum: _,
        climate_override: _,
        sky_region: _,
        location_form: _,
        encounter_zone_form: _,
        regions: _,
        lighting_template_form: _,
        ownership: _,
        regional_color_override: _,
        precombined_mesh_hashes: _,
        absorbed_ref_bakes: _,
        // Transient to the one plugin that produced it; consumed by
        // `merge_placed_references` below.
        deleted_refs: _,
        // Child records: separate records with their own override chain.
        references: _,
        landscape: base_landscape,
        navmeshes: base_navmeshes,
        pathgrids: base_pathgrids,
    } = base;
    if is_synthetic_interior_cell_id(over.form_id, &over.editor_id) && !base_editor_id.is_empty() {
        over.editor_id.clone_from(base_editor_id);
    }
    merge_cell_references(base, over);
    if over.landscape.is_none() {
        over.landscape.clone_from(base_landscape);
    }
    merge_navmeshes(base_navmeshes, &mut over.navmeshes);
    merge_pathgrids(base_pathgrids, &mut over.pathgrids);
}

impl EsmCellIndex {
    /// #5757 — flatten a REFR's `XLMS` list into MSWP-shaped swap
    /// entries for the cell loader's shared per-shape swap path, in
    /// application order (the consumer applies later-wins): for each
    /// listed LMSW, its `SwapsCollection` members (recursively, in
    /// order), then its own entries. An entry's source is its inline
    /// `Key`, else its MTPT's `Material`; an entry with neither, and an
    /// unknown LMSW form, contribute nothing. A collection that names an
    /// LMSW already on the current chain, or nests past
    /// `MAX_LMSW_NESTING`, is cut there.
    ///
    /// Members-before-own is not sourced: it only decides which swap wins
    /// when a record's own entry and a member's share a source, and 13 of
    /// vanilla's 12,966 records carry both lists.
    pub fn layered_material_swap_entries(
        &self,
        forms: &[u32],
    ) -> Vec<crate::esm::records::MaterialSwapEntry> {
        const MAX_LMSW_NESTING: usize = 16;
        fn visit(
            index: &EsmCellIndex,
            form: u32,
            chain: &mut Vec<u32>,
            out: &mut Vec<crate::esm::records::MaterialSwapEntry>,
        ) {
            if chain.len() >= MAX_LMSW_NESTING || chain.contains(&form) {
                return;
            }
            let Some(record) = index.layered_material_swaps.get(&form) else {
                return;
            };
            chain.push(form);
            for &member in &record.swaps_collection {
                visit(index, member, chain, out);
            }
            chain.pop();
            for entry in &record.entries {
                let source = if entry.key.is_empty() {
                    entry
                        .key_material_path_form
                        .and_then(|mtpt| index.material_path_forms.get(&mtpt))
                        .cloned()
                        .unwrap_or_default()
                } else {
                    entry.key.clone()
                };
                if source.is_empty() {
                    continue;
                }
                out.push(crate::esm::records::MaterialSwapEntry {
                    source,
                    target: entry.override_material_path.clone(),
                    color_remap_index: None,
                });
            }
        }
        let mut out = Vec::new();
        let mut chain = Vec::new();
        for &form in forms {
            visit(self, form, &mut chain, &mut out);
        }
        out
    }

    /// Join `landscape_texture_txsts` against `texture_sets`: every
    /// TNAM-authored LTEX gets its TXST's full set and its TX00 diffuse path.
    ///
    /// Authoritative for those LTEXs, so an override that re-points `TNAM`
    /// replaces what an earlier plugin's TNAM resolved — including dropping it
    /// when the new TXST has no diffuse or is not loaded. ICON-authored
    /// (Oblivion) LTEXs are not in the map and keep their direct path.
    pub(crate) fn resolve_landscape_texture_sets(&mut self) {
        for (&ltex_id, txst_id) in &self.landscape_texture_txsts {
            let set = self.texture_sets.get(txst_id);
            match set.and_then(|s| s.diffuse.as_ref()) {
                Some(diffuse) => {
                    self.landscape_textures.insert(ltex_id, diffuse.clone());
                }
                None => {
                    self.landscape_textures.remove(&ltex_id);
                }
            }
            match set {
                Some(set) => {
                    self.landscape_texture_sets.insert(ltex_id, set.clone());
                }
                None => {
                    self.landscape_texture_sets.remove(&ltex_id);
                }
            }
        }
    }

    /// The CELL record with `form_id`, wherever it is filed (interior,
    /// exterior grid tile, or a worldspace's persistent cell), in O(1).
    ///
    /// No CELL map is keyed by form ID — `cells` is keyed by canonical lookup
    /// ID, `exterior_cells` by worldspace + grid — so a form-ID reference to
    /// a CELL (a PKIN's `CNAM` template cell, #5489) used to cost a scan of
    /// every CELL per lookup. The first call builds the lookup; it is stale
    /// if the CELL maps are edited afterwards, so build the index first
    /// ([`Self::merge_from`] clears it).
    pub fn cell_by_form_id(&self, form_id: u32) -> Option<&CellData> {
        let lookup = self.cell_form_id_lookup.get_or_init(|| {
            let mut map = HashMap::with_capacity(
                self.cells.len()
                    + self
                        .exterior_cells
                        .values()
                        .map(HashMap::len)
                        .sum::<usize>()
                    + self.worldspace_persistent_cells.len(),
            );
            for (key, cell) in &self.cells {
                map.insert(cell.form_id, CellLocator::Interior(key.clone()));
            }
            for (world, grid) in &self.exterior_cells {
                for (&xy, cell) in grid {
                    map.insert(cell.form_id, CellLocator::Exterior(world.clone(), xy));
                }
            }
            for (world, cell) in &self.worldspace_persistent_cells {
                map.insert(
                    cell.form_id,
                    CellLocator::WorldspacePersistent(world.clone()),
                );
            }
            map
        });
        match lookup.get(&form_id)? {
            CellLocator::Interior(key) => self.cells.get(key),
            CellLocator::Exterior(world, xy) => self.exterior_cells.get(world)?.get(xy),
            CellLocator::WorldspacePersistent(world) => self.worldspace_persistent_cells.get(world),
        }
    }

    /// Give every LAND-less exterior cell of a land-inheriting worldspace the
    /// landscape of its `WNAM` parent at the same grid coordinate (#5505).
    ///
    /// The CS: "If you select NONE [as parent], the worldspace will have its
    /// own, editable landscape. Otherwise, this worldspace will use the
    /// landscape of its parent." An Oblivion city worldspace authors LAND only
    /// for its own footprint (BrumaWorld 13 of 57 cells) and a ring of empty
    /// cells around it; those cells are the parent's countryside. The grids
    /// are co-located, not local: BrumaWorld's LAND sits at cells (4..8,
    /// 34..38), which is where Bruma stands in Tamriel.
    ///
    /// Only cells the child already authors are filled — coordinates it has no
    /// CELL for stay with the parent-keyed distant-LOD ring. A cell's own
    /// LAND always wins, and the chain is followed only while each link
    /// carries [`PNAM_INHERIT_LAND`]. Run once after the load order is merged
    /// (the parent may live in a master). Returns the cells filled.
    ///
    /// The caller gates on the game: the co-location above was measured on
    /// Oblivion only, and FO3+ child grids are not assumed to share the
    /// parent's coordinates.
    pub fn inherit_parent_landscapes(&mut self) -> usize {
        // A WNAM chain is a few links long; the cap only stops a cycle.
        const MAX_CHAIN: usize = 16;
        let key_of_form: HashMap<u32, &String> = self
            .worldspaces
            .iter()
            .filter(|(_, w)| w.form_id != 0)
            .map(|(key, w)| (w.form_id, key))
            .collect();

        let mut grants = Vec::new();
        for (key, worldspace) in &self.worldspaces {
            if worldspace.parent_flags & PNAM_INHERIT_LAND == 0 {
                continue;
            }
            let Some(cells) = self.exterior_cells.get(key) else { continue };
            for grid in cells.iter().filter(|(_, cell)| cell.landscape.is_none()).map(|(grid, _)| grid) {
                let mut link = worldspace;
                let land = (0..MAX_CHAIN).find_map(|_| {
                    if link.parent_flags & PNAM_INHERIT_LAND == 0 {
                        return Some(None);
                    }
                    let parent_key = *key_of_form.get(&link.parent_worldspace?)?;
                    if let Some(land) = self
                        .exterior_cells
                        .get(parent_key)
                        .and_then(|parent_cells| parent_cells.get(grid))
                        .and_then(|parent_cell| parent_cell.landscape.as_ref())
                    {
                        return Some(Some(land.clone()));
                    }
                    link = self.worldspaces.get(parent_key)?;
                    None
                });
                if let Some(Some(land)) = land {
                    grants.push((key.clone(), *grid, land));
                }
            }
        }

        let filled = grants.len();
        for (key, grid, land) in grants {
            if let Some(cell) = self.exterior_cells.get_mut(&key).and_then(|c| c.get_mut(&grid)) {
                cell.landscape = Some(land);
            }
        }
        filled
    }

    /// Merge `other` into `self` with last-write-wins semantics on
    /// every map. Mirrors `EsmIndex::merge_from`'s contract — the
    /// caller parses plugins in load order and folds each into the
    /// running index.
    ///
    /// Interior cells (`HashMap<String, _>`) and exterior_cells
    /// (`HashMap<String, HashMap<(i32,i32), _>>`) need slightly
    /// different treatment than the flat record maps:
    ///
    /// * An override cell is the whole CELL record, so every CELL-record
    ///   field comes from it as authored (a sub-record it drops stays
    ///   dropped, #6086). The cell's child records — placements, landscape,
    ///   navmeshes, pathgrids — are separate records and are merged by their
    ///   stable identities. In particular, `references` merge per
    ///   REFR FormID rather than being whole-value replaced — Dawnguard's
    ///   `kagrenzel01` override carries 0 REFRs over a 1017-REFR base, and
    ///   replacing the vector made the whole cell empty. See
    ///   [`merge_cell_override`], #1546, and #2910.
    /// * Exterior cells merge per-worldspace so a DLC adding a new
    ///   worldspace doesn't stomp the base game's exterior table.
    ///   Within a shared worldspace, per-(x, y) overrides apply through the
    ///   same CELL override merge as interiors.
    ///
    /// **`worldspaces` is whole-record last-write-wins too** (EX-09/17
    /// item 6, #2370): `self.worldspaces.extend(...)` below is the same as
    /// `statics` and the other flat record maps — an override WRLD entirely
    /// replaces the base's `WorldspaceRecord`, as an override CELL replaces
    /// the base's CELL-record fields. Only child records (the placements
    /// above) have a separate override chain that inherits; a WRLD has no
    /// such children here, so nothing is inherited.
    ///
    /// See M46.0 / #561.
    pub fn merge_from(&mut self, other: EsmCellIndex) {
        // #4904 — exhaustive destructuring (no `..`): a new `EsmCellIndex`
        // field is a compile error here instead of a silently unmerged map.
        // The hand-written extend list dropped `landscape_grasses` (#4642's
        // LTEX→GRAS arrays), so every production load — which starts from
        // `EsmIndex::default()` and merges each plugin, master included —
        // ended with an empty map.
        let EsmCellIndex {
            cells: other_cells,
            exterior_cells: other_exterior_cells,
            worldspace_persistent_cells: other_worldspace_persistent_cells,
            statics,
            landscape_textures,
            landscape_texture_sets,
            landscape_texture_txsts,
            landscape_grasses,
            landscape_texture_names,
            worldspaces,
            worldspace_climates,
            texture_sets,
            scols,
            packins,
            movables,
            material_swaps,
            layered_material_swaps,
            material_path_forms,
            landscape_texture_havok,
            cell_form_id_lookup: _,
            refr_lookup: _,
        } = other;
        // The lookup is derived from the cell maps this merge is about to
        // change; rebuild it on next use.
        self.cell_form_id_lookup = std::sync::OnceLock::new();
        self.refr_lookup = std::sync::OnceLock::new();
        // #3362 — a REFR is a globally-unique FormID: deleting it removes the
        // object wherever it lives, and the CELL GRUP the tombstone happens
        // to be authored under is not part of its identity.
        //
        // `merge_cell_references` only ever applies a cell's own
        // `deleted_refs` against that same cell's base references, so a
        // tombstone authored under a DIFFERENT cell than the base placement
        // matched nothing and the object stayed resident. Bethesda's own DLCs
        // do this routinely — a REFR moved across a cell boundary before
        // being deleted, or a home interior a later master re-authored. On a
        // full Skyrim SE + 3 DLC load, 197 of 202 tombstones were honoured
        // and 5 were not; the user-visible one is HearthFires deleting a
        // cooking pot from Proudspire Manor, which still spawned.
        //
        // Applied BEFORE the per-cell merge loops, while `self` still holds
        // only earlier plugins' data. That is what makes load order come out
        // right with no special-casing: the tombstone clears what came
        // before, and anything this plugin (or a later one) places is merged
        // in afterwards and survives untouched — the un-delete semantics
        // `three_plugin_chain_composes_refr_merge_and_cross_plugin_delete`
        // pins. Doing it after the loops would need a "but not what `other`
        // re-placed" guard, which cannot tell a stale base copy from a fresh
        // placement.
        let tombstones: HashSet<u32> = other_cells
            .values()
            .chain(other_worldspace_persistent_cells.values())
            .chain(other_exterior_cells.values().flat_map(|g| g.values()))
            .flat_map(|c| c.deleted_refs.iter().copied())
            .collect();
        // #6084 — the same identity rule for a MOVE. An override plugin that
        // re-authors a placement under a different CELL (the CK regroups a
        // moved object under its new cell) leaves the base copy in the old
        // cell's list: the per-cell merge below only ever pairs a cell with
        // its own base, so both copies spawned (22 Skyrim SE, 16 FO3, 7 FO4
        // vanilla placements; unofficial patches move objects constantly).
        // Every FormID `other` authors under cell X is therefore removed from
        // every OTHER cell `self` holds; X itself is exempt so the per-cell
        // merge still folds the override over its base copy there. Cells are
        // identified by CELL FormID — the key shapes differ per map but the
        // FormID is the record's identity.
        let mut authored_ids: HashSet<u32> = HashSet::new();
        let mut authored_in: HashSet<(u32, u32)> = HashSet::new();
        if !(self.cells.is_empty()
            && self.exterior_cells.is_empty()
            && self.worldspace_persistent_cells.is_empty())
        {
            for cell in other_cells
                .values()
                .chain(other_worldspace_persistent_cells.values())
                .chain(other_exterior_cells.values().flat_map(|g| g.values()))
            {
                for placed in cell.references.iter().filter(|r| r.form_id != 0) {
                    authored_ids.insert(placed.form_id);
                    authored_in.insert((placed.form_id, cell.form_id));
                }
            }
        }
        if !tombstones.is_empty() || !authored_ids.is_empty() {
            for cell in self
                .cells
                .values_mut()
                .chain(self.worldspace_persistent_cells.values_mut())
                .chain(
                    self.exterior_cells
                        .values_mut()
                        .flat_map(|g| g.values_mut()),
                )
            {
                let cell_id = cell.form_id;
                cell.references.retain(|r| {
                    r.form_id == 0
                        || (!tombstones.contains(&r.form_id)
                            && !(authored_ids.contains(&r.form_id)
                                && !authored_in.contains(&(r.form_id, cell_id))))
                });
            }
        }

        let mut interior_keys_by_form: HashMap<u32, String> = self
            .cells
            .iter()
            .map(|(key, cell)| (cell.form_id, key.clone()))
            .collect();
        for (key, mut over_cell) in other_cells {
            let previous_key = interior_keys_by_form.get(&over_cell.form_id).cloned();
            let base = previous_key
                .as_ref()
                .and_then(|existing_key| self.cells.remove(existing_key));
            if let Some(base) = &base {
                merge_cell_override(base, &mut over_cell);
            }
            let insert_key =
                if is_synthetic_interior_cell_id(over_cell.form_id, &over_cell.editor_id) {
                    previous_key.unwrap_or(key)
                } else {
                    over_cell.editor_id.to_ascii_lowercase()
                };
            interior_keys_by_form.insert(over_cell.form_id, insert_key.clone());
            self.cells.insert(insert_key, over_cell);
        }

        for (worldspace, grids) in other_exterior_cells {
            let entry = self.exterior_cells.entry(worldspace).or_default();
            for (coord, mut over_cell) in grids {
                if let Some(base) = entry.get(&coord) {
                    merge_cell_override(base, &mut over_cell);
                }
                entry.insert(coord, over_cell);
            }
        }

        for (worldspace, mut over_cell) in other_worldspace_persistent_cells {
            if let Some(base) = self.worldspace_persistent_cells.get(&worldspace) {
                merge_cell_override(base, &mut over_cell);
            }
            self.worldspace_persistent_cells
                .insert(worldspace, over_cell);
        }

        self.statics.extend(statics);
        // An LTEX this plugin re-authors with a direct ICON path and no TNAM
        // must not be re-resolved through an earlier plugin's TNAM below.
        for ltex_id in landscape_textures.keys() {
            if !landscape_texture_txsts.contains_key(ltex_id)
                && self.landscape_texture_txsts.remove(ltex_id).is_some()
            {
                self.landscape_texture_sets.remove(ltex_id);
            }
        }
        self.landscape_textures.extend(landscape_textures);
        self.landscape_texture_sets.extend(landscape_texture_sets);
        self.landscape_texture_txsts.extend(landscape_texture_txsts);
        // Last writer per LTEX wins: an override re-authors the whole GNAM
        // array, so replacing (not appending to) the list is correct.
        self.landscape_grasses.extend(landscape_grasses);
        self.landscape_texture_names.extend(landscape_texture_names);
        self.worldspaces.extend(worldspaces);
        self.worldspace_climates.extend(worldspace_climates);
        self.texture_sets.extend(texture_sets);
        self.scols.extend(scols);
        self.packins.extend(packins);
        self.movables.extend(movables);
        self.material_swaps.extend(material_swaps);
        self.layered_material_swaps.extend(layered_material_swaps);
        self.material_path_forms.extend(material_path_forms);
        self.landscape_texture_havok.extend(landscape_texture_havok);
        // #6036 — after `texture_sets` holds every plugin so far, so an LTEX
        // naming a master's TXST (or a master LTEX whose TXST a later plugin
        // overrides) resolves against the merged maps.
        self.resolve_landscape_texture_sets();
    }

    /// Locate the parent cell of a placed REFR by its placement-level
    /// FormID. Persistent-worldspace REFRs are assigned to the exterior
    /// grid containing their authored world position because their owning
    /// persistent CELL has no `(x, y)` tile of its own. Returns `None` when
    /// no loaded cell contains a REFR with
    /// that FormID — typically because the destination lives in an
    /// unloaded master plugin, a DLC that wasn't passed on the CLI, or
    /// the FormID is malformed.
    ///
    /// M40 Phase 2 Stage 1 (door-teleport plumbing): the `door.teleport`
    /// console command and gameplay activate system feed an
    /// XTEL destination FormID through this helper to decide which
    /// cell to load. The first call builds a reverse index over the interior,
    /// exterior and worldspace-persistent maps (#6125); every lookup after it
    /// is O(1), a miss included. Like [`Self::cell_by_form_id`]'s, it is stale
    /// if the cell maps are edited afterwards — [`Self::merge_from`] clears it.
    pub fn cell_for_refr_form_id(&self, refr_form_id: u32) -> Option<CellRef<'_>> {
        self.locate_refr(refr_form_id).map(|(cell, _)| cell)
    }

    /// Locate a placed REFR by its placement-level FormID, returning both
    /// its parent cell (same assignment rules as
    /// [`Self::cell_for_refr_form_id`]) and the placement record itself.
    ///
    /// The interior spawn ladder reads a door's *partner* through this: the
    /// partner's own `XTEL` is the floor-level pose the engine places the
    /// player at when they walk through it into the door's cell.
    pub fn locate_refr(&self, refr_form_id: u32) -> Option<(CellRef<'_>, &PlacedRef)> {
        let lookup = self.refr_lookup.get_or_init(|| {
            let mut lookup = RefrLookup::default();
            let add = |lookup: &mut RefrLookup, locator: CellLocator, references: &[PlacedRef]| {
                let slot = lookup.cells.len() as u32;
                lookup.cells.push(locator);
                for (position, reference) in references.iter().enumerate() {
                    // The first cell holding a form ID keeps it, in the
                    // interior, exterior, persistent order the scan used.
                    lookup.refs.entry(reference.form_id).or_insert((slot, position as u32));
                }
            };
            for (key, cell) in &self.cells {
                add(&mut lookup, CellLocator::Interior(key.clone()), &cell.references);
            }
            for (worldspace, grids) in &self.exterior_cells {
                for (&grid, cell) in grids {
                    add(&mut lookup, CellLocator::Exterior(worldspace.clone(), grid), &cell.references);
                }
            }
            for (worldspace, cell) in &self.worldspace_persistent_cells {
                add(&mut lookup, CellLocator::WorldspacePersistent(worldspace.clone()), &cell.references);
            }
            lookup
        });
        let &(slot, position) = lookup.refs.get(&refr_form_id)?;
        let (cell_ref, references) = match lookup.cells.get(slot as usize)? {
            CellLocator::Interior(key) => {
                let cell = self.cells.get(key)?;
                (CellRef::Interior { editor_id: &cell.editor_id }, &cell.references)
            }
            CellLocator::Exterior(worldspace, grid) => {
                let (worldspace, grids) = self.exterior_cells.get_key_value(worldspace)?;
                (CellRef::Exterior { worldspace, grid: *grid }, &grids.get(grid)?.references)
            }
            CellLocator::WorldspacePersistent(worldspace) => {
                let (worldspace, cell) = self.worldspace_persistent_cells.get_key_value(worldspace)?;
                let reference = cell.references.get(position as usize)?;
                let grid = (
                    (reference.position[0] / EXTERIOR_CELL_UNITS).floor() as i32,
                    (reference.position[1] / EXTERIOR_CELL_UNITS).floor() as i32,
                );
                return (reference.form_id == refr_form_id)
                    .then_some((CellRef::Exterior { worldspace, grid }, reference));
            }
        };
        let reference = references.get(position as usize)?;
        // A stale entry (the maps were edited after the first lookup) must
        // miss, not name another reference.
        (reference.form_id == refr_form_id).then_some((cell_ref, reference))
    }
}

/// Result of `EsmCellIndex::cell_for_refr_form_id` — either an interior
/// cell (load via `--cell <editor_id>`) or an exterior cell (load the
/// containing worldspace's grid via `--wrld <name> --grid <gx>,<gy>`).
///
/// Lifetime is tied to the `EsmCellIndex` whose maps own the strings.
/// Callers that need owned data should clone the editor_id / worldspace
/// strings explicitly via [`CellRef::to_owned`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellRef<'a> {
    Interior {
        editor_id: &'a str,
    },
    Exterior {
        worldspace: &'a str,
        grid: (i32, i32),
    },
}

impl<'a> CellRef<'a> {
    /// Materialise an owned (`String`-backed) copy so the result can
    /// outlive the borrowing `EsmCellIndex`. Console commands that
    /// log the result rely on this.
    pub fn to_owned(&self) -> OwnedCellRef {
        match self {
            CellRef::Interior { editor_id } => OwnedCellRef::Interior {
                editor_id: (*editor_id).to_string(),
            },
            CellRef::Exterior { worldspace, grid } => OwnedCellRef::Exterior {
                worldspace: (*worldspace).to_string(),
                grid: *grid,
            },
        }
    }
}

/// Owned-string variant of [`CellRef`] for sites that need to outlive
/// the borrowing `EsmCellIndex` (logs, returned values, resource
/// queries). Produced by [`CellRef::to_owned`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnedCellRef {
    Interior {
        editor_id: String,
    },
    Exterior {
        worldspace: String,
        grid: (i32, i32),
    },
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
    remap: Option<crate::esm::reader::FormIdRemap>,
) -> Result<EsmCellIndex> {
    // #527 — fused single-pass walker. Pre-fix this function was the
    // first of two top-level walks over the whole ESM byte slice; the
    // typed-records walker in `crate::esm::records::parse_esm_with_load_order`
    // ran the same loop again to populate items / containers / NPCs
    // / etc. The fused walker now lives in records/mod.rs and produces
    // both maps in one pass — this entry point becomes a thin wrapper
    // that discards the typed maps for callers that only want cells.
    crate::esm::records::parse_esm_with_load_order(data, remap).map(|i| i.cells)
}
