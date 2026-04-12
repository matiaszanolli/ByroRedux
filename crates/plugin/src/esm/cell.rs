//! Cell, placed reference, and static object extraction from ESM files.
//!
//! Walks the GRUP tree to find interior cells, exterior cells (from WRLD),
//! their placed references (REFR + ACHR), and resolves base form IDs to
//! static/object definitions for NIF paths.

use super::reader::EsmReader;
use anyhow::{Context, Result};
use std::collections::HashMap;

/// Interior cell lighting from XCLL subrecord.
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
}

/// Light-specific data extracted from LIGH record DATA subrecord.
#[derive(Debug, Clone)]
pub struct LightData {
    pub radius: f32,
    pub color: [f32; 3],
    pub flags: u32,
}

/// A base form with its NIF model path (STAT, MSTT, FURN, DOOR, LIGH, NPC_, etc.).
#[derive(Debug, Clone)]
pub struct StaticObject {
    pub form_id: u32,
    pub editor_id: String,
    pub model_path: String,
    /// Light properties (only populated for LIGH records).
    pub light_data: Option<LightData>,
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
}

/// Parse an ESM file and extract cells, worldspaces, and base object definitions.
pub fn parse_esm_cells(data: &[u8]) -> Result<EsmCellIndex> {
    let mut reader = EsmReader::new(data);
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
                let end = reader.position() + group.total_size as usize - 24;
                parse_cell_group(&mut reader, end, &mut cells)?;
            }
            b"WRLD" => {
                let end = reader.position() + group.total_size as usize - 24;
                parse_wrld_group(&mut reader, end, &mut exterior_cells)?;
            }
            // All record types that have a MODL sub-record (NIF model path).
            // Placed references (REFR/ACHR) can point to any of these.
            b"STAT" | b"MSTT" | b"FURN" | b"DOOR" | b"ACTI" | b"CONT" | b"LIGH" | b"MISC"
            | b"FLOR" | b"TREE" | b"AMMO" | b"WEAP" | b"ARMO" | b"BOOK" | b"KEYM" | b"ALCH"
            | b"INGR" | b"NOTE" | b"TACT" | b"IDLM" | b"BNDS" | b"ADDN" | b"TERM" | b"NPC_" => {
                let end = reader.position() + group.total_size as usize - 24;
                parse_modl_group(&mut reader, end, &mut statics)?;
            }
            _ => {
                reader.skip_group(&group);
            }
        }
    }

    let total_exterior: usize = exterior_cells.values().map(|m| m.len()).sum();
    let wrld_names: Vec<&str> = exterior_cells.keys().map(|s| s.as_str()).collect();
    log::info!(
        "ESM parsed: {} interior cells, {} exterior cells across {} worldspaces, {} base objects",
        cells.len(),
        total_exterior,
        exterior_cells.len(),
        statics.len(),
    );
    if !wrld_names.is_empty() {
        log::info!("  Worldspaces: {:?}", wrld_names);
    }

    Ok(EsmCellIndex {
        cells,
        exterior_cells,
        statics,
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
            let sub_end = reader.position() + sub_group.total_size as usize - 24;

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
                        parse_refr_group(reader, sub_end, &mut refs)?;
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

                for sub in &subs {
                    match &sub.sub_type {
                        b"EDID" => editor_id = read_zstring(&sub.data),
                        b"DATA" if sub.data.len() >= 1 => is_interior = sub.data[0] & 1 != 0,
                        b"XCLL" if sub.data.len() >= 28 => {
                            // Shared Oblivion / FO3 / FNV XCLL layout
                            // (first 28 bytes — Skyrim+ is incompatible):
                            //   0-3:   Ambient RGBA (4 bytes)
                            //   4-7:   Directional RGBA (4 bytes)
                            //   8-11:  Fog color near RGBA (4 bytes)
                            //   12-15: Fog near (f32)
                            //   16-19: Fog far (f32)
                            //   20-23: Directional rotation X (i32, degrees)
                            //   24-27: Directional rotation Y (i32, degrees)
                            //   28-31: Directional fade (f32)
                            //   32-35: Fog clip distance (f32)
                            //   36-39: Fog power (FNV only)
                            //
                            // Oblivion XCLL is 36 bytes; FNV is 40.
                            // The fields we actually consume (ambient,
                            // directional, rotation) live in the
                            // byte-identical prefix, so a single parser
                            // handles both games without branching.
                            // Validated by the
                            // `oblivion_cells_populate_xcll_lighting`
                            // integration test in this module.
                            let ambient_r = sub.data[0] as f32 / 255.0;
                            let ambient_g = sub.data[1] as f32 / 255.0;
                            let ambient_b = sub.data[2] as f32 / 255.0;
                            let dir_r = sub.data[4] as f32 / 255.0;
                            let dir_g = sub.data[5] as f32 / 255.0;
                            let dir_b = sub.data[6] as f32 / 255.0;
                            // Directional rotation at bytes 20-27 (two i32, degrees)
                            let rot_x = {
                                let raw = i32::from_le_bytes([
                                    sub.data[20],
                                    sub.data[21],
                                    sub.data[22],
                                    sub.data[23],
                                ]);
                                (raw as f32).to_radians()
                            };
                            let rot_y = {
                                let raw = i32::from_le_bytes([
                                    sub.data[24],
                                    sub.data[25],
                                    sub.data[26],
                                    sub.data[27],
                                ]);
                                (raw as f32).to_radians()
                            };
                            // Fog color (RGBA at bytes 8-11) and distances (bytes 12-19).
                            let (fog_color, fog_near, fog_far) = if sub.data.len() >= 20 {
                                let fog_r = sub.data[8] as f32 / 255.0;
                                let fog_g = sub.data[9] as f32 / 255.0;
                                let fog_b = sub.data[10] as f32 / 255.0;
                                let fog_near = f32::from_le_bytes([
                                    sub.data[12], sub.data[13], sub.data[14], sub.data[15],
                                ]);
                                let fog_far = f32::from_le_bytes([
                                    sub.data[16], sub.data[17], sub.data[18], sub.data[19],
                                ]);
                                ([fog_r, fog_g, fog_b], fog_near, fog_far)
                            } else {
                                ([0.0; 3], 0.0, 0.0)
                            };
                            lighting = Some(CellLighting {
                                ambient: [ambient_r, ambient_g, ambient_b],
                                directional_color: [dir_r, dir_g, dir_b],
                                directional_rotation: [rot_x, rot_y],
                                fog_color,
                                fog_near,
                                fog_far,
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

/// Parse REFR records within a cell children group.
fn parse_refr_group(reader: &mut EsmReader, end: usize, refs: &mut Vec<PlacedRef>) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            // Nested groups within cell children — recurse to find REFR records.
            let sub = reader.read_group_header()?;
            let sub_end = reader.position() + sub.total_size as usize - 24;
            parse_refr_group(reader, sub_end, refs)?;
            continue;
        }

        let header = reader.read_record_header()?;
        if &header.record_type == b"REFR" || &header.record_type == b"ACHR" {
            let subs = reader.read_sub_records(&header)?;
            let mut base_form_id = 0u32;
            let mut position = [0.0f32; 3];
            let mut rotation = [0.0f32; 3];
            let mut scale = 1.0f32;

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
                    _ => {}
                }
            }

            if base_form_id != 0 {
                refs.push(PlacedRef {
                    base_form_id,
                    position,
                    rotation,
                    scale,
                });
            }
        } else {
            // Skip other record types (PGRE, PMIS, LAND, NAVM, etc.)
            reader.skip_record(&header);
        }
    }
    Ok(())
}

/// Walk the WRLD group hierarchy to find exterior cells and their placed references.
fn parse_wrld_group(
    reader: &mut EsmReader,
    end: usize,
    all_exterior_cells: &mut HashMap<String, HashMap<(i32, i32), CellData>>,
) -> Result<()> {
    let mut current_wrld_name: Option<String> = None;

    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub_group = reader.read_group_header()?;
            let sub_end = reader.position() + sub_group.total_size as usize - 24;

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
            // WRLD record — extract worldspace name.
            let header = reader.read_record_header()?;
            if &header.record_type == b"WRLD" {
                let subs = reader.read_sub_records(&header)?;
                for sub in &subs {
                    if &sub.sub_type == b"EDID" {
                        let name = read_zstring(&sub.data);
                        log::info!("Found worldspace: '{}' (form {:08X})", name, header.form_id);
                        current_wrld_name = Some(name);
                    }
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
            let sub_end = reader.position() + sub_group.total_size as usize - 24;

            match sub_group.group_type {
                // Exterior block (4) and sub-block (5): recurse.
                4 | 5 => {
                    parse_wrld_children(reader, sub_end, exterior_cells)?;
                }
                // Cell children (6=temporary, 8=persistent, 9=visible distant).
                6 | 8 | 9 => {
                    if let Some(grid) = current_cell {
                        let mut refs = Vec::new();
                        parse_refr_group(reader, sub_end, &mut refs)?;
                        if let Some(cell) = exterior_cells.get_mut(&grid) {
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
                let mut grid = None;

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
            let sub_end = reader.position() + sub.total_size as usize - 24;
            parse_modl_group(reader, sub_end, statics)?;
            continue;
        }

        let header = reader.read_record_header()?;
        {
            let is_ligh = &header.record_type == b"LIGH";
            let subs = reader.read_sub_records(&header)?;
            let mut editor_id = String::new();
            let mut model_path = String::new();
            let mut light_data = None;

            for sub in &subs {
                match &sub.sub_type {
                    b"EDID" => editor_id = read_zstring(&sub.data),
                    b"MODL" => model_path = read_zstring(&sub.data),
                    b"DATA" if is_ligh && sub.data.len() >= 12 => {
                        // LIGH DATA: time(u32), radius(u32), color(RGBA u8×4), flags(u32), ...
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
                    _ => {}
                }
            }

            // Insert if we have a model path, or if it's a LIGH with light data
            // (some lights have no mesh — they're just point lights).
            if !model_path.is_empty() || light_data.is_some() {
                statics.insert(
                    header.form_id,
                    StaticObject {
                        form_id: header.form_id,
                        editor_id,
                        model_path,
                        light_data,
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
        let end = reader.position() + gh.total_size as usize - 24;
        let mut statics = HashMap::new();
        parse_modl_group(&mut reader, end, &mut statics).unwrap();

        assert_eq!(statics.len(), 1);
        let s = statics.get(&0x1234).unwrap();
        assert_eq!(s.editor_id, "TestWall");
        assert_eq!(s.model_path, "meshes\\architecture\\wall01.nif");
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
        parse_refr_group(&mut reader, end, &mut refs).unwrap();

        assert_eq!(refs.len(), 1);
        let r = &refs[0];
        assert_eq!(r.base_form_id, 0xABCD);
        assert!((r.position[0] - 100.0).abs() < 1e-6);
        assert!((r.position[1] - 200.0).abs() < 1e-6);
        assert!((r.position[2] - 300.0).abs() < 1e-6);
        assert!((r.rotation[1] - 1.57).abs() < 0.01);
        assert!((r.scale - 2.0).abs() < 1e-6);
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
        let path =
            "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/Oblivion.esm";
        if !std::path::Path::new(path).exists() {
            eprintln!("Skipping: Oblivion.esm not found");
            return;
        }
        let data = std::fs::read(path).unwrap();
        let idx = parse_esm_cells(&data).expect("Oblivion walker");

        let total = idx.cells.len();
        let with_lighting = idx
            .cells
            .values()
            .filter(|c| c.lighting.is_some())
            .count();
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
            .filter_map(|c| c.lighting.as_ref().map(|l| (c.editor_id.clone(), l.clone())))
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
        let path =
            "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/Oblivion.esm";
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
                    idx.cells.values().filter(|c| !c.references.is_empty()).count(),
                );
            }
            Err(e) => panic!("parse_esm_cells failed on Oblivion.esm: {e:#}"),
        }
    }

    #[test]
    fn read_zstring_handles_null_terminator() {
        assert_eq!(read_zstring(b"Hello\0"), "Hello");
        assert_eq!(read_zstring(b"NoNull"), "NoNull");
        assert_eq!(read_zstring(b"\0"), "");
        assert_eq!(read_zstring(b""), "");
    }
}
