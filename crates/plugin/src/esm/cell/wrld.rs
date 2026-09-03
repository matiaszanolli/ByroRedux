//! Walker functions extracted from ../mod.rs (stage B refactor).
//!
//! Functions: parse_wrld_group, parse_wrld_children.

use super::helpers::{read_form_id, read_form_id_array, read_zstring};
use super::walkers::parse_refr_group;
use super::*;
use crate::esm::records::common::read_lstring_or_zstring;

/// Walk the WRLD group hierarchy to find exterior cells and their placed references.
///
/// Populates both `worldspaces` (full WRLD record per #965) and
/// `worldspace_climates` (CLMT FormID lookup preserved for back-compat
/// with the cell loader — see byroredux/src/cell_loader.rs:778).
pub(crate) fn parse_wrld_group(
    reader: &mut EsmReader,
    end: usize,
    all_exterior_cells: &mut HashMap<String, HashMap<(i32, i32), CellData>>,
    all_persistent_cells: &mut HashMap<String, CellData>,
    worldspaces: &mut HashMap<String, WorldspaceRecord>,
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
                        let key = name.to_ascii_lowercase();
                        let cells = all_exterior_cells.entry(key.clone()).or_default();
                        let mut persistent_cell = all_persistent_cells.remove(&key);
                        // A type-1 world-children group begins with the
                        // WRLD's structurally persistent CELL. It can still
                        // author XCLC=(0,0), so topology—not XCLC absence—
                        // distinguishes it from the streamed tile at (0,0).
                        parse_wrld_children(reader, sub_end, cells, &mut persistent_cell, true)?;
                        if let Some(cell) = persistent_cell {
                            all_persistent_cells.insert(key, cell);
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
            // WRLD record — extract worldspace name + every authored
            // exterior-render-critical sub-record (#965).
            let header = reader.read_record_header()?;
            if &header.record_type == b"WRLD" {
                let subs = reader.read_sub_records(&header)?;
                let mut record = WorldspaceRecord {
                    form_id: header.form_id,
                    ..WorldspaceRecord::default()
                };
                let mut climate_fid: Option<u32> = None;
                for sub in &subs {
                    match &sub.sub_type {
                        b"EDID" => {
                            record.editor_id = read_zstring(&sub.data);
                        }
                        // Climate — kept as a separate scalar so the
                        // cell loader can resolve CLMT without
                        // walking the worldspaces map. Same FormID
                        // also lives on the record for completeness
                        // via the parent-flag inheritance bit.
                        b"CNAM" if sub.data.len() >= 4 => {
                            climate_fid = read_form_id(reader, &sub.data);
                        }
                        // WNAM — parent worldspace FormID (cross-game).
                        b"WNAM" if sub.data.len() >= 4 => {
                            record.parent_worldspace = read_form_id(reader, &sub.data);
                        }
                        // PNAM — parent-use flags (FO3+/Skyrim, 1 or
                        // 2 bytes). Read the available prefix as a
                        // u16; pre-FO3 omits the sub-record entirely.
                        b"PNAM" if !sub.data.is_empty() => {
                            record.parent_flags = if sub.data.len() >= 2 {
                                u16::from_le_bytes([sub.data[0], sub.data[1]])
                            } else {
                                sub.data[0] as u16
                            };
                        }
                        // NAM0 / NAM9 — object-bounds SW / NE
                        // corners (2 × f32 in Bethesda world units,
                        // Z-up). xEdit / UESP / disk-sampled
                        // Oblivion.esm Tamriel all agree on the f32
                        // wire form; OpenMW reads as i32 but never
                        // consumes the value so it doesn't notice.
                        // See WorldspaceRecord::usable_cell_bounds.
                        b"NAM0" if sub.data.len() >= 8 => {
                            let x = f32::from_le_bytes([
                                sub.data[0],
                                sub.data[1],
                                sub.data[2],
                                sub.data[3],
                            ]);
                            let y = f32::from_le_bytes([
                                sub.data[4],
                                sub.data[5],
                                sub.data[6],
                                sub.data[7],
                            ]);
                            record.usable_min = (x, y);
                        }
                        b"NAM9" if sub.data.len() >= 8 => {
                            let x = f32::from_le_bytes([
                                sub.data[0],
                                sub.data[1],
                                sub.data[2],
                                sub.data[3],
                            ]);
                            let y = f32::from_le_bytes([
                                sub.data[4],
                                sub.data[5],
                                sub.data[6],
                                sub.data[7],
                            ]);
                            record.usable_max = (x, y);
                        }
                        // NAM2 — default water FormID.
                        b"NAM2" if sub.data.len() >= 4 => {
                            record.water_form = read_form_id(reader, &sub.data);
                        }
                        // DNAM "Land Data": [default_land_height: f32,
                        // default_water_height: f32]. The second f32 is the
                        // worldspace-default water-plane Z for cells with no
                        // XCLW (FO3/FNV/Skyrim+; Oblivion ships no DNAM and
                        // defaults to sea level Z=0 in the loader). 8-byte
                        // layout verified against FalloutNV.esm + Skyrim.esm.
                        // #1305 follow-up.
                        b"DNAM" if sub.data.len() >= 8 => {
                            record.default_water_height = Some(f32::from_le_bytes([
                                sub.data[4],
                                sub.data[5],
                                sub.data[6],
                                sub.data[7],
                            ]));
                        }
                        // ZNAM — default music FormID (MUSC).
                        b"ZNAM" if sub.data.len() >= 4 => {
                            record.default_music = read_form_id(reader, &sub.data);
                        }
                        // NAM3 / NAM4 — LOD-water type FormID + LOD-water
                        // height, the distant-LOD-ring counterparts of
                        // NAM2 / DNAM. FO3-and-later; Oblivion authors
                        // neither. Both are genuinely distinct from their
                        // full-detail siblings on real content (see the
                        // per-game divergence counts on WorldspaceRecord),
                        // so they get their own fields rather than folding
                        // into water_form / default_water_height. #1849.
                        b"NAM3" if sub.data.len() >= 4 => {
                            record.lod_water_form = read_form_id(reader, &sub.data);
                        }
                        b"NAM4" if sub.data.len() >= 4 => {
                            record.lod_water_height = Some(f32::from_le_bytes([
                                sub.data[0],
                                sub.data[1],
                                sub.data[2],
                                sub.data[3],
                            ]));
                        }
                        // OFST — per-cell offset table. Deliberately NOT
                        // captured: #1849 stored the raw u32 words for a
                        // future LAND streamer, and the streamer that
                        // arrived enumerates parsed CELL records instead of
                        // seeking by file offset, so the words had no
                        // reader. Falls to the `_ => {}` arm below like
                        // every other unconsumed sub-record — it is still
                        // walked past correctly (routinely oversized at
                        // 177 KB in FalloutNV.esm and therefore arriving
                        // through the XXXX extended-size escape, which
                        // `read_sub_records` handles either way). #2454 /
                        // EXAL-08; see `WorldspaceRecord` docs.
                        //
                        // ICON — pause-menu map texture (zstring).
                        b"ICON" => {
                            record.map_texture = read_zstring(&sub.data);
                        }
                        // DATA — single-byte worldspace flags.
                        b"DATA" if !sub.data.is_empty() => {
                            record.flags = sub.data[0];
                        }
                        _ => {}
                    }
                }
                if !record.editor_id.is_empty() {
                    let key = record.editor_id.to_ascii_lowercase();
                    let cell_bounds = record.usable_cell_bounds();
                    log::info!(
                        "Found worldspace: '{}' (form {:08X}, climate: {:08X?}, \
                         parent: {:08X?}, world bounds: {:?}..{:?} \
                         (cells {:?}), flags: 0x{:02X}, parent_flags: 0x{:04X})",
                        record.editor_id,
                        header.form_id,
                        climate_fid,
                        record.parent_worldspace,
                        record.usable_min,
                        record.usable_max,
                        cell_bounds,
                        record.flags,
                        record.parent_flags,
                    );
                    if let Some(clmt_fid) = climate_fid {
                        worldspace_climates.insert(key.clone(), clmt_fid);
                    }
                    current_wrld_name = Some(record.editor_id.clone());
                    worldspaces.insert(key, record);
                }
            } else {
                reader.skip_record(&header);
            }
        }
    }
    Ok(())
}

/// Walk exterior cell hierarchy within a worldspace (group types 1, 4, 5).
pub(crate) fn parse_wrld_children(
    reader: &mut EsmReader,
    end: usize,
    exterior_cells: &mut HashMap<(i32, i32), CellData>,
    persistent_cell: &mut Option<CellData>,
    force_persistent: bool,
) -> Result<()> {
    parse_wrld_children_inner(
        reader,
        end,
        exterior_cells,
        persistent_cell,
        force_persistent,
        0,
    )
}

fn parse_wrld_children_inner(
    reader: &mut EsmReader,
    end: usize,
    exterior_cells: &mut HashMap<(i32, i32), CellData>,
    persistent_cell: &mut Option<CellData>,
    force_persistent: bool,
    depth: u32,
) -> Result<()> {
    // `Some(Some(grid))` is a normal streamed tile, while `Some(None)` is
    // the worldspace's persistent CELL. Plain `None` means no CELL record
    // has established ownership for a following child group.
    let mut current_cell: Option<Option<(i32, i32)>> = None;

    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub_group = reader.read_group_header()?;
            let Some(sub_end) =
                reader.bounded_group_content_end(&sub_group, depth, end, "parse_wrld_children")
            else {
                continue;
            };

            match sub_group.group_type {
                // Exterior block (4) and sub-block (5): recurse.
                4 | 5 => {
                    parse_wrld_children_inner(
                        reader,
                        sub_end,
                        exterior_cells,
                        persistent_cell,
                        false,
                        depth + 1,
                    )?;
                }
                // Skyrim wraps the worldspace persistent CELL in an outer
                // type-6 group (labelled with that CELL's FormID), then
                // places the CELL record and its type-8 actor children
                // inside it. At this level there is no current CELL yet, so
                // recurse and mark the enclosed CELL as world-persistent.
                6 if current_cell.is_none() => {
                    parse_wrld_children_inner(
                        reader,
                        sub_end,
                        exterior_cells,
                        persistent_cell,
                        true,
                        depth + 1,
                    )?;
                }
                // Cell children (6=temporary, 8=persistent, 9=visible distant).
                6 | 8 | 9 => {
                    if let Some(cell_target) = current_cell {
                        let mut refs = Vec::new();
                        let mut land = None;
                        let mut navmeshes = Vec::new();
                        let mut pathgrids = Vec::new();
                        let mut deleted = Vec::new();
                        parse_refr_group(
                            reader,
                            sub_end,
                            &mut refs,
                            &mut land,
                            &mut navmeshes,
                            &mut pathgrids,
                            &mut deleted,
                        )?;
                        let cell = match cell_target {
                            Some(grid) => exterior_cells.get_mut(&grid),
                            None => persistent_cell.as_mut(),
                        };
                        if let Some(cell) = cell {
                            cell.references.extend(refs);
                            cell.navmeshes.extend(navmeshes);
                            cell.pathgrids.extend(pathgrids);
                            cell.deleted_refs.extend(deleted);
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
                // #624 / SK-D6-NEW-02 — exterior CELLs also ship FULL
                // (named worldspace tiles like SolitudeWorld, the cell
                // covering Whiterun's market district). Pre-fix the
                // sub-record was dropped on the catch-all `_` arm.
                let mut display_name: Option<String> = None;
                let mut grid = None;
                let mut water_height: Option<f32> = None;
                let mut water_height_is_explicit = false;
                let mut image_space_form: Option<u32> = None;
                let mut water_type_form: Option<u32> = None;
                let mut water_velocity: Option<[f32; 3]> = None;
                let mut acoustic_space_form: Option<u32> = None;
                let mut music_type_form: Option<u32> = None;
                // #693 / O3-N-05 — pre-Skyrim XCMT (1-byte enum) and
                // Skyrim XCCM (4-byte CLMT FormID, the per-cell
                // climate override). Both fell to the catch-all `_`
                // arm pre-fix; XCCM is the more impactful one on
                // exterior cells (boss arenas, scripted-weather
                // pockets, interior-feeling exteriors).
                let mut music_type_enum: Option<u8> = None;
                let mut climate_override: Option<u32> = None;
                let mut location_form: Option<u32> = None;
                let mut regions: Vec<u32> = Vec::new();
                // SK-D6-02 / #566 — exterior cells can also carry an
                // LTMP lighting-template FormID. Same fallback semantics
                // as interior cells: XCLL wins, LGTM fills in.
                let mut lighting_template_form: Option<u32> = None;
                // #692 — exterior CELL ownership (worldspace owner +
                // faction-rank gate + global-var gate). Same layout as
                // interior CELL above; cross-game.
                let mut ownership_owner: Option<u32> = None;
                let mut ownership_rank: Option<i32> = None;
                let mut ownership_global: Option<u32> = None;
                // #970 / OBL-D3-NEW-06 — exterior CELL RCLR. The audit
                // observed this on Oblivion only; FO3+ vanilla uses
                // LGTM/CLMT instead. Parse cross-game so modded
                // exterior cells in any era still surface the override.
                let mut regional_color_override: Option<[u8; 3]> = None;
                // #1220 / D3-NEW-01 — FO4+ PreCombined Mesh references
                // on EXTERIOR cells. Commonwealth open-world tiles
                // (Concord, Sanctuary Hills, Boston, Diamond City
                // Marketplace) ship per-tile precombined NIFs — this
                // is FO4's headline performance feature, and the
                // vast majority of the 124,871 entries in
                // `Fallout4 - MeshesExtra.ba2` are exterior tiles.
                // Pre-#1220 the exterior walker hardcoded empty,
                // masquerading as "interior-only"; that left the
                // optimisation unreachable for the cells it was
                // designed for. Sub-record layout mirrors the
                // interior path verbatim — see `walkers.rs:158-204`.
                let mut precombined_mesh_hashes: Vec<u32> = Vec::new();
                let mut absorbed_refs: std::collections::HashSet<u32> =
                    std::collections::HashSet::new();

                for sub in &subs {
                    match &sub.sub_type {
                        b"EDID" => editor_id = read_zstring(&sub.data),
                        // #624 — auto-routes the localized 4-byte
                        // STRINGS-table case via the lstring helper.
                        b"FULL" => display_name = Some(read_lstring_or_zstring(&sub.data)),
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
                        // XCLW water-plane height. `xclw_water_height`
                        // returns None for the `#INT_MIN#` / FLT_MAX
                        // "no water" sentinels; the explicit bit stops
                        // those dry cells inheriting WRLD water (#1305 /
                        // OBL-D6-NEW-02).
                        b"XCLW" => {
                            water_height_is_explicit = true;
                            water_height = super::helpers::xclw_water_height(&sub.data);
                        }
                        // Skyrim extended sub-records — see the interior
                        // walker above for semantics. Exterior cells use
                        // the same encoding. #356.
                        b"XCIM" => image_space_form = read_form_id(reader, &sub.data),
                        b"XCWT" => water_type_form = read_form_id(reader, &sub.data),
                        b"XWCU" if sub.data.len() >= 12 => {
                            let mut values = [0.0; 3];
                            for (slot, bytes) in
                                values.iter_mut().zip(sub.data.chunks_exact(4).take(3))
                            {
                                *slot = f32::from_le_bytes(bytes.try_into().unwrap());
                            }
                            if values.iter().all(|value| value.is_finite()) {
                                water_velocity = Some(values);
                            }
                        }
                        b"XCAS" => acoustic_space_form = read_form_id(reader, &sub.data),
                        b"XCMO" => music_type_form = read_form_id(reader, &sub.data),
                        // #693 / O3-N-05 — see interior walker for
                        // semantics. XCMT is rare on exterior cells
                        // (most exteriors use the worldspace default
                        // music) but pinned for completeness; XCCM
                        // is the load-bearing one here.
                        b"XCMT" if !sub.data.is_empty() => {
                            music_type_enum = Some(sub.data[0]);
                        }
                        b"XCCM" => climate_override = read_form_id(reader, &sub.data),
                        b"XLCN" => location_form = read_form_id(reader, &sub.data),
                        b"XCLR" => regions = read_form_id_array(reader, &sub.data),
                        // LTMP — lighting template FormID (SK-D6-02 / #566).
                        b"LTMP" => lighting_template_form = read_form_id(reader, &sub.data),
                        // #692 — exterior CELL ownership tuple (mirrors
                        // the interior walker arms above).
                        b"XOWN" if sub.data.len() >= 4 => {
                            ownership_owner = read_form_id(reader, &sub.data);
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
                            ownership_global = read_form_id(reader, &sub.data);
                        }
                        // #970 / OBL-D3-NEW-06 — see interior walker
                        // for semantics. Oblivion exterior cells are
                        // the dominant authoring site for this tag.
                        b"RCLR" if sub.data.len() >= 3 => {
                            regional_color_override = Some([sub.data[0], sub.data[1], sub.data[2]]);
                        }
                        // #1220 / D3-NEW-01 — XCRI: FO4+ PreCombined
                        // Mesh hash list + visibility-group tail.
                        // Layout exact mirror of the interior walker
                        // at `walkers.rs:158-190`. The `ref_count`
                        // tail is the visibility group, NOT the
                        // skip-placement set — that's XPRI's job.
                        b"XCRI" if sub.data.len() >= 8 => {
                            let mesh_count =
                                u32::from_le_bytes(sub.data[0..4].try_into().unwrap()) as usize;
                            let ref_count =
                                u32::from_le_bytes(sub.data[4..8].try_into().unwrap()) as usize;
                            let expected =
                                8 + mesh_count.saturating_mul(4) + ref_count.saturating_mul(4);
                            if expected != sub.data.len() {
                                log::warn!(
                                    "CELL {:08X} XCRI size mismatch: hdr={}+{} expected_payload={} \
                                     actual={} — skipping",
                                    header.form_id,
                                    mesh_count,
                                    ref_count,
                                    expected,
                                    sub.data.len(),
                                );
                            } else {
                                precombined_mesh_hashes.reserve(mesh_count);
                                let mut off = 8;
                                for _ in 0..mesh_count {
                                    let h = u32::from_le_bytes(
                                        sub.data[off..off + 4].try_into().unwrap(),
                                    );
                                    precombined_mesh_hashes.push(h);
                                    off += 4;
                                }
                                // Visibility-group tail intentionally
                                // not consumed — see XPRI below.
                            }
                        }
                        // #1220 / D3-NEW-01 — XPRI: REFR formids
                        // absorbed into the precombines. The cell
                        // loader honours this set only when the
                        // precombined-spawn pass produced > 0
                        // entities (conditional-absorption gate in
                        // `load.rs:170` for interiors; exterior
                        // wiring lands separately under #1221).
                        b"XPRI" if sub.data.len() % 4 == 0 => {
                            absorbed_refs.reserve(sub.data.len() / 4);
                            for chunk in sub.data.chunks_exact(4) {
                                let fid = u32::from_le_bytes(chunk.try_into().unwrap());
                                absorbed_refs.insert(reader.remap_form_id(fid));
                            }
                        }
                        _ => {}
                    }
                }

                let ownership = ownership_owner.map(|owner| CellOwnership {
                    owner_form_id: owner,
                    faction_rank: ownership_rank,
                    global_var_form_id: ownership_global,
                });
                let cell = CellData {
                    form_id: header.form_id,
                    editor_id,
                    display_name,
                    references: Vec::new(),
                    is_interior: false,
                    grid,
                    lighting: None,
                    landscape: None,
                    water_height,
                    water_height_is_explicit,
                    image_space_form,
                    water_type_form,
                    water_velocity,
                    acoustic_space_form,
                    music_type_form,
                    music_type_enum,
                    climate_override,
                    location_form,
                    regions,
                    lighting_template_form,
                    ownership,
                    regional_color_override,
                    // #1220 / D3-NEW-01 — FO4+ PreCombined Mesh
                    // refs on exterior cells. The cell loader's
                    // conditional-absorption gate ties XPRI
                    // honour-vs-ignore to the precombined-spawn
                    // count; exterior call-site wiring landed
                    // under #1221/#1222 ("third leg") and the gate
                    // was later shared between the interior and
                    // exterior loaders under #2063 (see
                    // `byroredux::cell_loader::precombined::
                    // absorbed_refs_or_empty`). Live today: when
                    // the precombine spawns, these fields suppress
                    // per-REFR rendering of the baked REFRs on
                    // exterior cells the same way they already did
                    // on interior ones.
                    precombined_mesh_hashes,
                    absorbed_refs,
                    navmeshes: Vec::new(),
                    pathgrids: Vec::new(),
                    deleted_refs: Vec::new(),
                };
                if force_persistent {
                    if persistent_cell.is_none() {
                        *persistent_cell = Some(cell);
                        current_cell = Some(None);
                    } else {
                        log::warn!(
                            "Skipping duplicate structurally persistent exterior CELL {:08X}",
                            header.form_id,
                        );
                        current_cell = None;
                    }
                } else if let Some(g) = grid {
                    exterior_cells.insert(g, cell);
                    current_cell = Some(Some(g));
                } else {
                    log::warn!(
                        "Skipping nested exterior CELL {:08X} without XCLC; \
                         only the first CELL structurally owned by the type-1 \
                         world group may be persistent",
                        header.form_id,
                    );
                    current_cell = None;
                }
            } else {
                reader.skip_record(&header);
            }
        }
    }
    Ok(())
}
