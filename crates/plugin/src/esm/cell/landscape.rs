//! Landscape (LAND) record types, extracted from ../mod.rs (#5805
//! type-hub split).
//!
//! Types: `TerrainTextureLayer`, `TerrainQuadrant`, `LandscapeData`,
//! `LandscapeTextureHavok`.

/// An LTEX record's Havok data (#6048): what physics knows about ground
/// painted with this texture. Layouts per xEdit `wbDefinitions*.pas`:
///
/// - Oblivion / FO3 / FNV — `HNAM` = `u8` Material Type, `u8` Friction,
///   `u8` Restitution. The material is the game's Havok material enum,
///   the same `OB_HAV_MAT_*` / `FO_HAV_MAT_*` value a NIF `bhk` shape
///   stores (0 Stone, 2 Dirt, 4 Grass, 14 Snow, FO3 18 Sand, …).
/// - Skyrim / FO4 — `MNAM` = a `MATT` FormID (the material), `HNAM` =
///   `u8` Friction, `u8` Restitution.
///
/// Friction and restitution are kept as authored (vanilla default 30): no
/// source states the scale the engine applies to turn them into Havok
/// coefficients, so they are not translated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LandscapeTextureHavok {
    /// `HNAM` Material Type — Oblivion / FO3 / FNV only.
    pub material: Option<u8>,
    /// `MNAM` `MATT` FormID (load-order remapped) — Skyrim / FO4 only.
    pub material_type_form: Option<u32>,
    /// `HNAM` Friction, as authored.
    pub friction: u8,
    /// `HNAM` Restitution, as authored.
    pub restitution: u8,
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
    /// Index: `[row * 17 + col]`. Gated at parse time (#4484): finite wire
    /// opacities are clamped into 0.0–1.0, non-finite ones decode to 0.0 —
    /// the splat sorter's "NaN cannot appear" premise holds by construction.
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
    /// Vertex normals: 33×33 × 3 bytes (X, Y, Z as **signed** `i8`,
    /// mapping to -1.0–1.0 via `i8 as f32 / 127.0`). The signed reading
    /// is deliberate — #4059 corrected the earlier unsigned `(b - 127)
    /// / 127` decode, which tilted every normal; the consume-side
    /// decoder is `cell_loader::terrain::decode_vnml_normal`. Stored
    /// raw here; decode happens at consume time. None if VNML absent.
    pub normals: Option<Vec<u8>>,
    /// Vertex colors: 33×33 × 3 bytes (R, G, B as unsigned bytes 0–255).
    /// None if VCLR absent.
    pub vertex_colors: Option<Vec<u8>>,
    /// Texture layers per quadrant (0=SW, 1=SE, 2=NW, 3=NE).
    pub quadrants: [TerrainQuadrant; 4],
}
