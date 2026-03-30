//! Cell, placed reference, and static object extraction from ESM files.
//!
//! Walks the GRUP tree to find interior cells, their placed references (REFR),
//! and resolves base form IDs to static object definitions (STAT) for NIF paths.

use super::reader::{EsmReader, GroupHeader, RecordHeader};
use anyhow::{Context, Result};
use std::collections::HashMap;

/// An interior cell with its placed object references.
#[derive(Debug)]
pub struct CellData {
    pub form_id: u32,
    pub editor_id: String,
    pub references: Vec<PlacedRef>,
}

/// A placed object reference within a cell.
#[derive(Debug, Clone)]
pub struct PlacedRef {
    pub base_form_id: u32,
    /// Position in Bethesda units (Z-up).
    pub position: [f32; 3],
    /// Euler rotation in radians (X, Y, Z).
    pub rotation: [f32; 3],
    pub scale: f32,
}

/// A static object base form with its NIF model path.
#[derive(Debug, Clone)]
pub struct StaticObject {
    pub form_id: u32,
    pub editor_id: String,
    pub model_path: String,
}

/// Result of parsing an ESM file for cell loading.
#[derive(Debug)]
pub struct EsmCellIndex {
    /// All interior cells found, keyed by editor ID (lowercase).
    pub cells: HashMap<String, CellData>,
    /// All STAT records, keyed by form ID.
    pub statics: HashMap<u32, StaticObject>,
}

/// Parse an ESM file and extract all interior cells and static object definitions.
pub fn parse_esm_cells(data: &[u8]) -> Result<EsmCellIndex> {
    let mut reader = EsmReader::new(data);
    let file_header = reader.read_file_header()
        .context("Failed to read ESM file header")?;

    log::info!(
        "ESM file: {} records, {} master files",
        file_header.record_count,
        file_header.master_files.len(),
    );

    let mut cells = HashMap::new();
    let mut statics = HashMap::new();

    // Walk top-level groups.
    while reader.remaining() > 0 {
        if !reader.is_group() {
            // Skip non-group records at top level (shouldn't happen, but defensive).
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
            b"STAT" => {
                let end = reader.position() + group.total_size as usize - 24;
                parse_stat_group(&mut reader, end, &mut statics)?;
            }
            _ => {
                reader.skip_group(&group);
            }
        }
    }

    log::info!(
        "ESM parsed: {} interior cells, {} static objects",
        cells.len(),
        statics.len(),
    );

    Ok(EsmCellIndex { cells, statics })
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

                for sub in &subs {
                    match &sub.sub_type {
                        b"EDID" => editor_id = read_zstring(&sub.data),
                        b"DATA" if sub.data.len() >= 1 => is_interior = sub.data[0] & 1 != 0,
                        _ => {}
                    }
                }

                if is_interior && !editor_id.is_empty() {
                    let key = editor_id.to_ascii_lowercase();
                    cells.insert(key, CellData {
                        form_id: header.form_id,
                        editor_id: editor_id.clone(),
                        references: Vec::new(),
                    });
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
fn parse_refr_group(
    reader: &mut EsmReader,
    end: usize,
    refs: &mut Vec<PlacedRef>,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            // Nested groups within cell children — recurse to find REFR records.
            let sub = reader.read_group_header()?;
            let sub_end = reader.position() + sub.total_size as usize - 24;
            parse_refr_group(reader, sub_end, refs)?;
            continue;
        }

        let header = reader.read_record_header()?;
        if &header.record_type == b"REFR" {
            let subs = reader.read_sub_records(&header)?;
            let mut base_form_id = 0u32;
            let mut position = [0.0f32; 3];
            let mut rotation = [0.0f32; 3];
            let mut scale = 1.0f32;

            for sub in &subs {
                match &sub.sub_type {
                    b"NAME" if sub.data.len() >= 4 => {
                        base_form_id = u32::from_le_bytes([
                            sub.data[0], sub.data[1], sub.data[2], sub.data[3],
                        ]);
                    }
                    b"DATA" if sub.data.len() >= 24 => {
                        // 6 floats: posX, posY, posZ, rotX, rotY, rotZ
                        for i in 0..3 {
                            let off = i * 4;
                            position[i] = f32::from_le_bytes([
                                sub.data[off], sub.data[off + 1],
                                sub.data[off + 2], sub.data[off + 3],
                            ]);
                        }
                        for i in 0..3 {
                            let off = 12 + i * 4;
                            rotation[i] = f32::from_le_bytes([
                                sub.data[off], sub.data[off + 1],
                                sub.data[off + 2], sub.data[off + 3],
                            ]);
                        }
                    }
                    b"XSCL" if sub.data.len() >= 4 => {
                        scale = f32::from_le_bytes([
                            sub.data[0], sub.data[1], sub.data[2], sub.data[3],
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
            // Skip non-REFR records (ACHR, PGRE, PMIS, etc.)
            reader.skip_record(&header);
        }
    }
    Ok(())
}

/// Walk the top-level STAT group to collect static object definitions.
fn parse_stat_group(
    reader: &mut EsmReader,
    end: usize,
    statics: &mut HashMap<u32, StaticObject>,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            // STAT top group may contain sub-groups (shouldn't, but handle it).
            let sub = reader.read_group_header()?;
            let sub_end = reader.position() + sub.total_size as usize - 24;
            parse_stat_group(reader, sub_end, statics)?;
            continue;
        }

        let header = reader.read_record_header()?;
        if &header.record_type == b"STAT" {
            let subs = reader.read_sub_records(&header)?;
            let mut editor_id = String::new();
            let mut model_path = String::new();

            for sub in &subs {
                match &sub.sub_type {
                    b"EDID" => editor_id = read_zstring(&sub.data),
                    b"MODL" => model_path = read_zstring(&sub.data),
                    _ => {}
                }
            }

            if !model_path.is_empty() {
                statics.insert(header.form_id, StaticObject {
                    form_id: header.form_id,
                    editor_id,
                    model_path,
                });
            }
        } else {
            reader.skip_record(&header);
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
    use super::*;
    use super::super::reader::EsmReader;

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
        parse_stat_group(&mut reader, end, &mut statics).unwrap();

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
        sub_data.extend_from_slice(&0.0f32.to_le_bytes());   // rot x
        sub_data.extend_from_slice(&1.57f32.to_le_bytes());  // rot y
        sub_data.extend_from_slice(&0.0f32.to_le_bytes());   // rot z
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
        assert!(index.cells.len() > 100, "Expected >100 cells, got {}", index.cells.len());
        assert!(index.statics.len() > 1000, "Expected >1000 statics, got {}", index.statics.len());

        // Check which cells have refs.
        let cells_with_refs = index.cells.values().filter(|c| !c.references.is_empty()).count();
        eprintln!("Cells with refs: {}", cells_with_refs);

        // Check the Prospector Saloon specifically.
        let saloon = index.cells.get("gsprospectorsalooninterior").unwrap();
        eprintln!("Saloon: {} refs", saloon.references.len());
        assert!(saloon.references.len() > 100, "Saloon should have >100 refs");

        // Look for the Prospector Saloon.
        let saloon_keys: Vec<&str> = index.cells.keys()
            .filter(|k| k.contains("goodsprings") || k.contains("saloon") || k.contains("prospector"))
            .map(|k| k.as_str())
            .collect();
        eprintln!("Goodsprings/saloon cells: {:?}", saloon_keys);

        // Print a few cells for debugging.
        for (key, cell) in index.cells.iter().take(10) {
            eprintln!("  Cell '{}': {} refs", key, cell.references.len());
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
