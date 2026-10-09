//! Walker functions extracted from ../mod.rs (stage B refactor).
//!
//! Functions: parse_wrld_group, parse_wrld_children.

use super::helpers::{read_form_id, read_zstring};
use super::walkers::parse_refr_group;
use crate::esm::reader::GameKind;
use super::*;

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
    game: GameKind,
) -> Result<()> {
    let mut current_wrld_name: Option<String> = None;

    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub_group = reader.read_group_header()?;
            // `.min(end)` is #3721's *how-far* half, which that commit
            // threaded through all 13 `bounded_group_content_end` sites and
            // missed here — this is the one production caller still on the
            // raw accessor for a group it then recurses into. Without it a
            // WRLD whose type-1 world-children group declares a `total_size`
            // larger than the enclosing top-level GRUP's remaining content
            // makes `parse_wrld_children` consume records belonging to the
            // *next* top-level group and file them as exterior cells of this
            // worldspace. The top-level dispatcher does not re-seek after a
            // walker returns, so that desynchronises the whole remaining walk,
            // not just this worldspace. No vanilla master triggers it (#4076).
            let sub_end = reader.group_content_end(&sub_group).min(end);

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
                        parse_wrld_children(reader, sub_end, cells, &mut persistent_cell, true, game)?;
                        if let Some(cell) = persistent_cell {
                            all_persistent_cells.insert(key, cell);
                        }
                    } else {
                        // #4644 — seek to the parent-clamped sub_end, not
                        // the child's own declared size.
                        reader.seek_to(sub_end);
                    }
                }
                _ => {
                    // #4644 — same clamp as the arm above.
                    reader.seek_to(sub_end);
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
                        // #4416 — INAM, the worldspace's IMGS image space
                        // (FO3/FNV only; xEdit `wbDefinitionsFNV.pas`
                        // WRLD). Inherited through PNAM bit 5, "Use Image
                        // Space Data".
                        b"INAM" if sub.data.len() >= 4 => {
                            record.image_space_form = read_form_id(reader, &sub.data);
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
                        // Routed through the same finite/sentinel gate as
                        // XCLW: a corrupt NaN/huge default must not become a
                        // canonical height that every water-less cell of the
                        // worldspace inherits (#4487 / #1305 follow-up).
                        b"DNAM" if sub.data.len() >= 8 => {
                            record.default_water_height =
                                super::helpers::gated_water_height(&sub.data[4..]);
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
                            // Same gate as XCLW / DNAM — the LOD ring uploads
                            // its plane straight to the GPU, so a NaN or
                            // sentinel height must decode to "no LOD water"
                            // rather than NaN vertex Y positions (#4487).
                            record.lod_water_height =
                                super::helpers::gated_water_height(&sub.data);
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
                // #5374 — pre-FO3 inheritance is "child ⟹ inherit
                // everything": Oblivion authors no PNAM, so a child
                // worldspace's zero `parent_flags` means "absent", not
                // "inherit nothing". Stamp the inherit-all word at the
                // parse boundary (the same shape the #2735 fix
                // suggested) so the bit-gated walks resolve the
                // parent's water/climate/LOD without every consumer
                // carrying a second, game-aware walk. Census premise:
                // 54/54 Oblivion roots author NAM2, 0/30 children do,
                // and the below-sea-level cells cluster exactly in the
                // children (Bravil, Leyawiin, the Imperial City, New
                // Sheoth). An authored zero PNAM on FO3+ stays a real
                // zero — this arm only runs for Oblivion.
                if game == GameKind::Oblivion && record.parent_worldspace.is_some() {
                    const INHERIT_LAND: u16 = 0x01;
                    const INHERIT_LOD: u16 = 0x02;
                    const INHERIT_WATER: u16 = 0x08;
                    const INHERIT_CLIMATE: u16 = 0x10;
                    record.parent_flags |=
                        INHERIT_LAND | INHERIT_LOD | INHERIT_WATER | INHERIT_CLIMATE;
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
    game: GameKind,
) -> Result<()> {
    parse_wrld_children_inner(
        reader,
        end,
        exterior_cells,
        persistent_cell,
        force_persistent,
        game,
        0,
    )
}

fn parse_wrld_children_inner(
    reader: &mut EsmReader,
    end: usize,
    exterior_cells: &mut HashMap<(i32, i32), CellData>,
    persistent_cell: &mut Option<CellData>,
    force_persistent: bool,
    game: GameKind,
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
                        game,
                        depth + 1,
                    )?;
                }
                // #4077 — a `6 if current_cell.is_none()` arm used to sit
                // here, on the premise that "Skyrim wraps the worldspace
                // persistent CELL in an outer type-6 group". No shipped
                // plugin does that. A CELL-record parent-group-type census
                // over every `.esm`/`.esl` in all seven game Data dirs found
                // parents of exactly types 1, 3 and 5 — never 6
                // (`Skyrim.esm` {3: 590, 1: 36, 5: 16942};
                // `Fallout4.esm` {3: 1195, 1: 5, 5: 38965}). The real
                // topology is that the persistent CELL is a *direct* child of
                // the type-1 World Children group and its type-6 children
                // group follows as a sibling — which is the type-1 → type-6
                // edge that does exist in the data and is handled by the arm
                // below. The persistent cell is captured by
                // `force_persistent = true` from `parse_wrld_group`, not by
                // any type-6 special case.
                //
                // Deleting it also removed a mis-promotion vector: a crafted
                // plugin opening a type-6 group before any CELL at the type-1
                // level would have had whatever CELL it contained silently
                // promoted to worldspace-persistent. Type 6 with no current
                // CELL now falls through to the guarded arm below and is
                // skipped.
                //
                // Cell children. #4169 — corrected legend: 6 is the
                // `Cell Children` *container* (no records of its own),
                // and the membership types nest inside it (8 = Persistent,
                // 9 = Temporary, 10 = Visible Distant). `parse_refr_group`
                // re-derives the per-placement value during that descent;
                // the value handed in here is only the entry scope.
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
                            sub_group.group_type as u8,
                            game,
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
                        // #4644 — seek to the parent-clamped sub_end, not
                        // the child's own declared size.
                        reader.seek_to(sub_end);
                    }
                }
                _ => {
                    // #4644 — same clamp as the arm above.
                    reader.seek_to(sub_end);
                }
            }
        } else {
            let header = reader.read_record_header()?;
            if &header.record_type == b"CELL" {
                let subs = reader.read_sub_records(&header)?;
                // #5309 — every walker-shared CELL sub-record decodes in
                // `CellSubrecordFields::absorb` (see the interior walker);
                // this walker keeps only its exterior-specific XCLC arm.
                let mut fields = super::helpers::CellSubrecordFields::default();
                let mut grid = None;

                for sub in &subs {
                    match &sub.sub_type {
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
                        // Everything walker-shared falls through to
                        // `CellSubrecordFields::absorb` (#5309).
                        _ => {
                            fields.absorb(reader, header.form_id, sub);
                        }
                    }
                }

                let ownership = fields.ownership();
                let super::helpers::CellSubrecordFields {
                    editor_id,
                    display_name,
                    water_height,
                    water_height_is_explicit,
                    image_space_form,
                    water_type_form,
                    acoustic_space_form,
                    music_type_form,
                    music_type_enum,
                    climate_override,
                    location_form,
                    encounter_zone_form,
                    regions,
                    lighting_template_form,
                    ownership_owner: _,
                    ownership_rank: _,
                    ownership_global: _,
                    regional_color_override,
                    precombined_mesh_hashes,
                    absorbed_refs,
                } = fields;

                let cell = CellData {
                    form_id: header.form_id,
                    editor_id,
                    display_name,
                    references: Vec::new(),
                    is_interior: false,
                    show_sky: None,
                    grid,
                    lighting: None,
                    landscape: None,
                    water_height,
                    water_height_is_explicit,
                    image_space_form,
                    water_type_form,
                    acoustic_space_form,
                    music_type_form,
                    music_type_enum,
                    climate_override,
                    location_form,
                    encounter_zone_form,
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
