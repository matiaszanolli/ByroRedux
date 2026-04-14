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
    /// Landscape texture definitions: LTEX form ID → diffuse texture path.
    /// Resolved via LTEX.TNAM → TXST.TX00 (FO3+) or LTEX.ICON (Oblivion).
    pub landscape_textures: HashMap<u32, String>,
    /// Worldspace climate form IDs: worldspace_name_lowercase → CLMT form ID.
    /// Extracted from the WRLD record's CNAM sub-record.
    pub worldspace_climates: HashMap<String, u32>,
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
    let mut landscape_textures: HashMap<u32, String> = HashMap::new();
    let mut worldspace_climates: HashMap<String, u32> = HashMap::new();
    // First pass collects TXST form IDs; second resolves LTEX → TXST → path.
    let mut txst_textures: HashMap<u32, String> = HashMap::new();
    let mut ltex_to_txst: HashMap<u32, u32> = HashMap::new();

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
                parse_wrld_group(
                    &mut reader,
                    end,
                    &mut exterior_cells,
                    &mut worldspace_climates,
                )?;
            }
            b"LTEX" => {
                let end = reader.position() + group.total_size as usize - 24;
                parse_ltex_group(&mut reader, end, &mut ltex_to_txst, &mut landscape_textures)?;
            }
            b"TXST" => {
                let end = reader.position() + group.total_size as usize - 24;
                parse_txst_group(&mut reader, end, &mut txst_textures)?;
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

                for sub in &subs {
                    match &sub.sub_type {
                        b"EDID" => editor_id = read_zstring(&sub.data),
                        b"DATA" if sub.data.len() >= 1 => is_interior = sub.data[0] & 1 != 0,
                        b"XCLL" if sub.data.len() >= 28 => {
                            // XCLL layout (shared prefix for all games):
                            //   0-3:   Ambient RGBA
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
                            let (fog_color, fog_near, fog_far) = if d.len() >= 20 {
                                let fog_r = d[8] as f32 / 255.0;
                                let fog_g = d[9] as f32 / 255.0;
                                let fog_b = d[10] as f32 / 255.0;
                                let fog_near = f32::from_le_bytes([d[12], d[13], d[14], d[15]]);
                                let fog_far = f32::from_le_bytes([d[16], d[17], d[18], d[19]]);
                                ([fog_r, fog_g, fog_b], fog_near, fog_far)
                            } else {
                                ([0.0; 3], 0.0, 0.0)
                            };

                            // Skyrim+ extended XCLL (92 bytes):
                            //   28-31: Directional fade (f32)
                            //   32-35: Fog clip distance (f32)
                            //   36-39: Fog power (f32)
                            //   40-63: Directional ambient 6×RGBA (24 bytes, unused for now)
                            //   64-67: Specular color RGBA (unused)
                            //   68-71: Fresnel power (f32, unused)
                            //   72-75: Fog far color RGBA
                            //   76-79: Fog max (f32)
                            //   80-83: Light fade begin (f32)
                            //   84-87: Light fade end (f32)
                            //   88-91: Inherits flags (u32, unused)
                            let (
                                dir_fade,
                                fog_clip,
                                fog_power,
                                fog_far_color,
                                fog_max,
                                lf_begin,
                                lf_end,
                            ) = if d.len() >= 92 {
                                (
                                    Some(f32::from_le_bytes([d[28], d[29], d[30], d[31]])),
                                    Some(f32::from_le_bytes([d[32], d[33], d[34], d[35]])),
                                    Some(f32::from_le_bytes([d[36], d[37], d[38], d[39]])),
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
                                (None, None, None, None, None, None, None)
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
            let sub_end = reader.position() + sub.total_size as usize - 24;
            parse_refr_group(reader, sub_end, refs, landscape)?;
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
        } else if &header.record_type == b"LAND" {
            // Parse landscape heightmap, normals, and vertex colors.
            if let Ok(land) = parse_land_record(reader, &header) {
                *landscape = Some(land);
            } else {
                log::warn!("LAND record parse failed (form {:08X})", header.form_id);
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
                            landscape: None,
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
            let sub_end = reader.position() + sub.total_size as usize - 24;
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

/// Parse TXST (Texture Set) records. Extracts the TX00 (diffuse) texture path.
fn parse_txst_group(
    reader: &mut EsmReader,
    end: usize,
    txst_textures: &mut HashMap<u32, String>,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub = reader.read_group_header()?;
            let sub_end = reader.position() + sub.total_size as usize - 24;
            parse_txst_group(reader, sub_end, txst_textures)?;
            continue;
        }

        let header = reader.read_record_header()?;
        if &header.record_type == b"TXST" {
            let subs = reader.read_sub_records(&header)?;
            for sub in &subs {
                // TX00 = diffuse/color map (the primary texture).
                if sub.sub_type.as_slice() == b"TX00" {
                    let path = read_zstring(&sub.data);
                    if !path.is_empty() {
                        txst_textures.insert(header.form_id, path);
                    }
                    break;
                }
            }
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
}
