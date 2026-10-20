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
    /// CELL form ID → where that cell lives in the three CELL maps above,
    /// built on first use by [`EsmCellIndex::cell_by_form_id`] and cleared
    /// by [`EsmCellIndex::merge_from`] (#5489). Crate-private: it is derived
    /// state, so it is neither parsed nor documented as index content.
    pub(crate) cell_form_id_lookup: std::sync::OnceLock<HashMap<u32, CellLocator>>,
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

/// Apply Bethesda's partial-CELL override contract at the parser boundary.
/// A child plugin only re-emits fields and child records it changes; absent
/// payloads inherit from the base instead of erasing terrain, navmeshes,
/// precombines, lighting, or other canonical cell state.
pub(super) fn merge_cell_override(base: &CellData, over: &mut CellData) {
    if is_synthetic_interior_cell_id(over.form_id, &over.editor_id) && !base.editor_id.is_empty() {
        over.editor_id.clone_from(&base.editor_id);
    }
    if over.display_name.is_none() {
        over.display_name.clone_from(&base.display_name);
    }
    if over.show_sky.is_none() {
        over.show_sky = base.show_sky;
    }
    merge_cell_references(base, over);
    if over.grid.is_none() {
        over.grid = base.grid;
        over.land_flags = base.land_flags;
    }
    if over.lighting.is_none() {
        over.lighting.clone_from(&base.lighting);
    }
    if over.landscape.is_none() {
        over.landscape.clone_from(&base.landscape);
    }
    if !over.water_height_is_explicit {
        over.water_height = base.water_height;
        over.water_height_is_explicit = base.water_height_is_explicit;
    }
    if over.image_space_form.is_none() {
        over.image_space_form = base.image_space_form;
    }
    if over.water_type_form.is_none() {
        over.water_type_form = base.water_type_form;
    }
    if over.acoustic_space_form.is_none() {
        over.acoustic_space_form = base.acoustic_space_form;
    }
    if over.music_type_form.is_none() {
        over.music_type_form = base.music_type_form;
    }
    if over.music_type_enum.is_none() {
        over.music_type_enum = base.music_type_enum;
    }
    if over.climate_override.is_none() {
        over.climate_override = base.climate_override;
    }
    if over.sky_region.is_none() {
        over.sky_region = base.sky_region;
    }
    if over.location_form.is_none() {
        over.location_form = base.location_form;
    }
    if over.regions.is_empty() {
        over.regions.clone_from(&base.regions);
    }
    if over.lighting_template_form.is_none() {
        over.lighting_template_form = base.lighting_template_form;
    }
    if over.ownership.is_none() {
        over.ownership.clone_from(&base.ownership);
    }
    if over.regional_color_override.is_none() {
        over.regional_color_override = base.regional_color_override;
    }
    if over.precombined_mesh_hashes.is_empty() {
        over.precombined_mesh_hashes
            .clone_from(&base.precombined_mesh_hashes);
    }
    over.absorbed_ref_bakes
        .extend(base.absorbed_ref_bakes.iter().copied());
    merge_navmeshes(&base.navmeshes, &mut over.navmeshes);
}

impl EsmCellIndex {
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
    /// * A DLC override cell wins with fields it actually authors; absent
    ///   CELL payloads inherit from the base. Child collections are merged
    ///   by their stable identities. In particular, `references` merge per
    ///   REFR FormID rather than being whole-value replaced — Dawnguard's
    ///   `kagrenzel01` override carries 0 REFRs over a 1017-REFR base, and
    ///   replacing the vector made the whole cell empty. See
    ///   [`merge_cell_override`], #1546, and #2910.
    /// * Exterior cells merge per-worldspace so a DLC adding a new
    ///   worldspace doesn't stomp the base game's exterior table.
    ///   Within a shared worldspace, per-(x, y) overrides apply through the
    ///   same partial-CELL merge as interiors.
    ///
    /// **`worldspaces` is deliberately NOT given the same partial-field
    /// treatment** (EX-09/17 item 6, #2370): `self.worldspaces.extend(...)`
    /// below is whole-record last-write-wins, same as `statics` and the
    /// other flat record maps — an override WRLD entirely replaces the
    /// base's `WorldspaceRecord`, field-by-field, rather than inheriting
    /// absent fields the way `merge_cell_override` does for CELL. This
    /// matches vanilla's typical authoring pattern (a WRLD override
    /// re-declares the fields it changes AND the ones it doesn't, unlike a
    /// partial-CELL override), so it's flagged rather than fixed — low risk
    /// today, but NOT the same partial-inherit guarantee CELL gets. Revisit
    /// only if a real sparse-WRLD-override mod turns up (one that omits a
    /// field expecting it to inherit from the base, the way Dawnguard's
    /// `kagrenzel01` omitted REFRs above); until then a speculative fix has
    /// no real fixture to validate against.
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
            cell_form_id_lookup: _,
        } = other;
        // The lookup is derived from the cell maps this merge is about to
        // change; rebuild it on next use.
        self.cell_form_id_lookup = std::sync::OnceLock::new();
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
        if !tombstones.is_empty() {
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
                cell.references
                    .retain(|r| r.form_id == 0 || !tombstones.contains(&r.form_id));
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
    /// cell to load. Today it scans interior, exterior, and worldspace-
    /// persistent maps linearly
    /// — vanilla `Skyrim.esm` carries ~63 k REFRs across ~3 k cells,
    /// so the worst-case lookup is ~63 k comparisons. That's fine for
    /// console-driven testing; Phase 2 Stage 2 will materialise a
    /// `HashMap<u32, CellRef>` reverse index built once at plugin-load
    /// so the F-key path stays cheap.
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
        for cell in self.cells.values() {
            if let Some(reference) = cell.references.iter().find(|r| r.form_id == refr_form_id) {
                return Some((
                    CellRef::Interior {
                        editor_id: &cell.editor_id,
                    },
                    reference,
                ));
            }
        }
        for (worldspace, grids) in &self.exterior_cells {
            for ((gx, gy), cell) in grids {
                if let Some(reference) = cell.references.iter().find(|r| r.form_id == refr_form_id)
                {
                    return Some((
                        CellRef::Exterior {
                            worldspace,
                            grid: (*gx, *gy),
                        },
                        reference,
                    ));
                }
            }
        }
        for (worldspace, cell) in &self.worldspace_persistent_cells {
            if let Some(reference) = cell
                .references
                .iter()
                .find(|reference| reference.form_id == refr_form_id)
            {
                let grid = (
                    (reference.position[0] / EXTERIOR_CELL_UNITS).floor() as i32,
                    (reference.position[1] / EXTERIOR_CELL_UNITS).floor() as i32,
                );
                return Some((CellRef::Exterior { worldspace, grid }, reference));
            }
        }
        None
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
