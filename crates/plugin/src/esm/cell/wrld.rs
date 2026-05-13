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
                            climate_fid = read_form_id(&sub.data);
                        }
                        // WNAM — parent worldspace FormID (cross-game).
                        b"WNAM" if sub.data.len() >= 4 => {
                            record.parent_worldspace = read_form_id(&sub.data);
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
                            record.water_form = read_form_id(&sub.data);
                        }
                        // ZNAM — default music FormID (MUSC).
                        b"ZNAM" if sub.data.len() >= 4 => {
                            record.default_music = read_form_id(&sub.data);
                        }
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
                         (cells {:?}), flags: 0x{:02X})",
                        record.editor_id,
                        header.form_id,
                        climate_fid,
                        record.parent_worldspace,
                        record.usable_min,
                        record.usable_max,
                        cell_bounds,
                        record.flags,
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
                // #624 / SK-D6-NEW-02 — exterior CELLs also ship FULL
                // (named worldspace tiles like SolitudeWorld, the cell
                // covering Whiterun's market district). Pre-fix the
                // sub-record was dropped on the catch-all `_` arm.
                let mut display_name: Option<String> = None;
                let mut grid = None;
                let mut water_height: Option<f32> = None;
                let mut image_space_form: Option<u32> = None;
                let mut water_type_form: Option<u32> = None;
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
                        // #693 / O3-N-05 — see interior walker for
                        // semantics. XCMT is rare on exterior cells
                        // (most exteriors use the worldspace default
                        // music) but pinned for completeness; XCCM
                        // is the load-bearing one here.
                        b"XCMT" if !sub.data.is_empty() => {
                            music_type_enum = Some(sub.data[0]);
                        }
                        b"XCCM" => climate_override = read_form_id(&sub.data),
                        b"XLCN" => location_form = read_form_id(&sub.data),
                        b"XCLR" => regions = read_form_id_array(&sub.data),
                        // LTMP — lighting template FormID (SK-D6-02 / #566).
                        b"LTMP" => lighting_template_form = read_form_id(&sub.data),
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
                        // #970 / OBL-D3-NEW-06 — see interior walker
                        // for semantics. Oblivion exterior cells are
                        // the dominant authoring site for this tag.
                        b"RCLR" if sub.data.len() >= 3 => {
                            regional_color_override =
                                Some([sub.data[0], sub.data[1], sub.data[2]]);
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
                            display_name,
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
                            music_type_enum,
                            climate_override,
                            location_form,
                            regions,
                            lighting_template_form,
                            ownership,
                            regional_color_override,
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
