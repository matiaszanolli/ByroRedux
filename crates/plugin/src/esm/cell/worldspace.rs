//! The WRLD (worldspace) record, extracted from ../mod.rs (#5805 type-hub
//! split).
//!
//! Types: `worldspace_flags`, `WorldspaceRecord`.

use byroredux_core::math::coord::EXTERIOR_CELL_UNITS;

/// Decoded fields from a WRLD (worldspace) record. Captures the
/// exterior-render-critical bits that pre-#965 fell on the catch-all
/// `_ => {}` arm of `parse_wrld_group`: parent worldspace, usable
/// cell-grid bounds, default water / music / map references, and the
/// 1-byte flags byte.
///
/// Wire layout (cross-game, sub-record dispatch):
/// - `EDID` — editor ID (zstring, lowercased into the index key)
/// - `WNAM` — parent worldspace FormID (u32). `None` on root
///   worldspaces (Tamriel, Wasteland, Tamriel-of-Skyrim) since the
///   sub-record is omitted entirely; populated on derived
///   worldspaces (Shivering Isles → Tamriel, Solstheim → Skyrim).
/// - `PNAM` — parent-use flags (u16). Bit set means the field is
///   inherited from the parent worldspace: 0x01 Land, 0x02 LOD,
///   0x04 Map, 0x08 Water, 0x10 Climate, 0x20 Imagespace (pre-TES5),
///   0x40 SkyCell. Absent on TES4 (Oblivion).
/// - `NAM0` — object bounds south-west corner (2 × f32 in Bethesda
///   world units, Z-up). Disk-sampled on `Oblivion.esm` Tamriel:
///   `-262144.0, -253952.0` ≈ cell `(-64, -62)`. The audit text
///   originally called these "i32 cell-grid pairs"; the wire form
///   is float world units per UESP and xEdit. The cell loader can
///   convert to cell coords via `floor(value / 4096.0)`.
/// - `NAM9` — object bounds north-east corner (2 × f32, same units).
/// - `NAM2` — default water FormID (u32, WATR on FO3+/Skyrim,
///   LTEX/WATR on Oblivion).
/// - `ICON` — map texture path (zstring), the worldspace pause-menu
///   map background.
/// - `DATA` — worldspace flags byte (u8). **Two layouts**, and the
///   names do not line up between them — per OpenMW
///   `components/esm4/loadwrld.hpp:46-56`, transcribed verbatim
///   (a blank cell means that family names no flag at that bit):
///
///   | bit | TES4 — Oblivion / FO3 / FNV | TES5 — Skyrim+ / FO4+ |
///   |---|---|---|
///   | 0x01 | Small World | Small World |
///   | 0x02 | Can't Fast Travel | Can't Fast Travel |
///   | 0x04 | Oblivion worldspace | |
///   | 0x08 | | No LOD Water |
///   | 0x10 | No LOD Water | No Landscape |
///   | 0x20 | | No Sky |
///   | 0x40 | | Fixed Dimensions |
///   | 0x80 | | No Grass |
///
///   #3506 — this docblock previously carried the TES5 *names* at
///   TES4-shaped *positions* (every entry from "no LOD water" up
///   shifted one bit down), which was wrong for both families. On
///   Gamebryo-era data the error is semantic, not just positional:
///   FO3's `0x04` is *Oblivion worldspace* and its `0x10` is *No LOD
///   Water*, not "no sky". Live census of `Fallout3.esm` (32
///   worldspaces): `MegatonWorld` = 0x51, i.e. bit 6 + No LOD Water +
///   Small World under TES4 — which matches a crater settlement with
///   no distant water plane, where the old reading ("no sky") is
///   falsified by the fact that Megaton's exterior renders one.
///   `CitadelWorld` / `MonumentWorld` / `WashMonTop` / `tLandscape` =
///   0x11, `ParadiseFalls` / `TranquilityLane` = 0x13,
///   `StatesmanRoofWorld` = 0x03, `Wasteland` = 0x00.
///
///   **No engine code reads [`Self::flags`] yet** — only the parser
///   writes it and the tests assert it. The first consumer (an EXAL
///   sky / landscape / LOD-water gate is the obvious candidate, and
///   `0x10` is exactly the bit such a gate would reach for) must
///   branch on the plugin's game before interpreting any bit above
///   `0x02`.
/// - `ZNAM` — default music FormID (u32, MUSC record).
/// - `DNAM` — default land/water height (2 × f32); only the second
///   (water) is kept, as [`default_water_height`](Self::default_water_height).
/// - `NAM3`/`NAM4` — LOD water type FormID + LOD water height (#1849).
///
/// Sub-records not consumed here (parsed-past by the walker so the
/// next record still aligns): `WCTR` (TES5 centre cell), `MNAM` (map
/// camera data), `RNAM` (region overrides), `MHDT` (Skyrim+ map
/// height data), `CLSZ`/`WLEV` (FO4), and `OFST` (per-cell offset
/// table — see below). See OpenMW reference
/// `components/esm4/loadwrld.cpp` and audit OBL-D3-NEW-01 / #965.
///
/// `OFST` was captured verbatim as raw `u32` words by #1849, on the
/// stated premise that a future LAND streamer would interpret the
/// table. That streamer exists now (`byroredux::cell_loader`) and
/// enumerates `EsmIndex.cells.exterior_cells` keys instead — it never
/// needed the offsets, because this engine indexes the CELL records it
/// has already parsed rather than seeking into the plugin by file
/// offset the way the original engine's streamer did. The capture was
/// therefore pure cost: up to ~44k `u32` per worldspace held for the
/// process lifetime (the sub-record runs to 177 KB on FalloutNV.esm),
/// plus a parsed field that read as a live capability. Dropped by
/// #2454 / EXAL-08; restore it alongside a real consumer, not ahead of
/// The two [`WorldspaceRecord::flags`] bit layouts (#3506).
///
/// Split into two modules rather than one flat list precisely because the
/// names do not line up: everything from "no LOD water" upward sits at a
/// different bit in each family, so a single `NO_LOD_WATER` constant would
/// be wrong for one of them no matter which value it took. Transcribed from
/// OpenMW `components/esm4/loadwrld.hpp:46-56`; bits neither column names
/// are simply absent here rather than guessed at.
///
/// Nothing in the engine reads these yet — they exist so the first consumer
/// picks a column deliberately instead of inheriting the docblock's old,
/// wrong single list.
pub mod worldspace_flags {
    /// Oblivion / FO3 / FNV. `0x08`, `0x20`, `0x40` and `0x80` are unnamed
    /// in the reference for this family.
    pub mod tes4 {
        pub const SMALL_WORLD: u8 = 0x01;
        pub const NO_FAST_TRAVEL: u8 = 0x02;
        pub const OBLIVION_WORLDSPACE: u8 = 0x04;
        pub const NO_LOD_WATER: u8 = 0x10;
    }

    /// Skyrim+ / FO4+. `0x04` is unnamed in the reference for this family.
    pub mod tes5 {
        pub const SMALL_WORLD: u8 = 0x01;
        pub const NO_FAST_TRAVEL: u8 = 0x02;
        pub const NO_LOD_WATER: u8 = 0x08;
        pub const NO_LANDSCAPE: u8 = 0x10;
        pub const NO_SKY: u8 = 0x20;
        pub const FIXED_DIMENSIONS: u8 = 0x40;
        pub const NO_GRASS: u8 = 0x80;
    }
}

/// WRLD `PNAM` bit `0x01` — "Use Land Data": the worldspace's landscape comes
/// from its `WNAM` parent (the GECK/CS "Parent Worldspace" rule). Oblivion
/// authors no `PNAM`; the parse stamps this bit on every Oblivion child (see
/// `wrld.rs`).
pub const PNAM_INHERIT_LAND: u16 = 0x01;

/// one. See exal.md §5.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorldspaceRecord {
    /// FormID of the WRLD record itself.
    pub form_id: u32,
    /// EDID — preserved verbatim. The index keys by `.to_ascii_lowercase()`.
    pub editor_id: String,
    /// Object-bounds SW corner (NAM0) in Bethesda world units (Z-up).
    /// Defaults to `(0.0, 0.0)` when absent. Combined with
    /// `usable_max` to bound the exterior iteration radius in the
    /// cell loader; use `usable_cell_bounds()` to convert to
    /// inclusive cell-grid coordinates.
    pub usable_min: (f32, f32),
    /// Object-bounds NE corner (NAM9) in Bethesda world units. When
    /// both `usable_min` and `usable_max` are the default `(0.0,
    /// 0.0)`, the consumer should fall back to the explicit cells
    /// map rather than iterate an empty rectangle.
    pub usable_max: (f32, f32),
    /// Parent worldspace FormID (WNAM). `None` on root worldspaces.
    pub parent_worldspace: Option<u32>,
    /// Parent-use flags (PNAM). Bits select which fields inherit
    /// from the parent worldspace. Zero on TES4 (no PNAM authored).
    pub parent_flags: u16,
    /// Image space (`INAM`, an IMGS FormID) — FO3/FNV exteriors' base
    /// colour grade (#4416). `None` when unauthored; later games take
    /// theirs from the weather's `IMSP` instead.
    pub image_space_form: Option<u32>,
    /// Default music FormID (ZNAM, MUSC record). `None` when the
    /// worldspace defers to the DefaultObjectManager music.
    pub default_music: Option<u32>,
    /// Default water FormID (NAM2). `None` when the worldspace has
    /// no authored default water plane.
    pub water_form: Option<u32>,
    /// Default water height (DNAM "Land Data" second f32, Z-up world
    /// units) — the water-plane Z for exterior cells with no XCLW.
    /// `None` on Oblivion (whose WRLD carries no DNAM; the cell loader
    /// falls back to sea level Z=0 there) and when DNAM is absent or
    /// truncated. Layout `[default_land_height, default_water_height]`
    /// (8 bytes) is stable across FO3/FNV (Gamebryo) and Skyrim+/FO4
    /// (Creation Engine) — verified against FalloutNV.esm and Skyrim.esm.
    /// See #1305 follow-up.
    pub default_water_height: Option<f32>,
    /// Worldspace map-texture path (ICON). Empty when not authored.
    pub map_texture: String,
    /// Worldspace flags byte (DATA). See the struct docs for the bit
    /// layout — it differs between the TES4 (Oblivion / FO3 / FNV) and
    /// TES5 (Skyrim+ / FO4+) families and the two name sets are offset
    /// from one another, so this byte is **not** interpretable without
    /// knowing the plugin's game (#3506). No consumer exists yet.
    pub flags: u8,
    /// LOD water type FormID (NAM3, WATR record) — the water material
    /// the *distant* LOD ring uses. Genuinely independent of the
    /// full-detail [`water_form`](Self::water_form) (NAM2): the two
    /// differ on 18 of 28 Fallout3.esm worldspaces, 5 of 30
    /// Skyrim.esm, and 2 of 14 FalloutNV.esm, so a consumer must not
    /// substitute one for the other. `None` on Oblivion, whose WRLD
    /// carries no NAM3 at all (the sub-record is FO3-and-later —
    /// disk-sampled across Oblivion.esm / Fallout3.esm / FalloutNV.esm
    /// / Skyrim.esm / Fallout4.esm). See #1849.
    pub lod_water_form: Option<u32>,
    /// LOD water height (NAM4, f32, Z-up world units) — the water
    /// plane Z for the distant LOD ring. Also independent of DNAM's
    /// default water height ([`default_water_height`](Self::default_water_height)):
    /// they differ on 22 of 30 Skyrim.esm worldspaces and 6 of 28
    /// Fallout3.esm. `None` on Oblivion (no NAM4). See #1849.
    pub lod_water_height: Option<f32>,
}

impl WorldspaceRecord {
    /// One Bethesda exterior cell spans 4096 world units on each
    /// side. The cell loader keys exterior_cells by `(i32, i32)`
    /// cell-grid coordinates; this helper turns the f32 world-unit
    /// `usable_min`/`usable_max` into the inclusive grid rectangle
    /// the loader can iterate. Returns `None` when both corners are
    /// at the default `(0.0, 0.0)` (no NAM0/NAM9 authored — the
    /// consumer should fall back to the explicit `exterior_cells`
    /// keys).
    pub fn usable_cell_bounds(&self) -> Option<((i32, i32), (i32, i32))> {
        if self.usable_min == (0.0, 0.0) && self.usable_max == (0.0, 0.0) {
            return None;
        }
        let min = (
            (self.usable_min.0 / EXTERIOR_CELL_UNITS).floor() as i32,
            (self.usable_min.1 / EXTERIOR_CELL_UNITS).floor() as i32,
        );
        let max = (
            (self.usable_max.0 / EXTERIOR_CELL_UNITS).floor() as i32,
            (self.usable_max.1 / EXTERIOR_CELL_UNITS).floor() as i32,
        );
        Some((min, max))
    }
}
