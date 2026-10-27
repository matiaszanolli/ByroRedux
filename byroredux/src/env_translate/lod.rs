//! EXAL distant terrain / object LOD and worldspace-default water: which
//! baked terrain family a game ships, the LOD texture quads, the default and
//! distant (LOD) water planes a worldspace authors or inherits, and the
//! worldspace whose LOD rings a child draws (`lod_source_worldspace`).

use std::collections::HashMap;

use byroredux_plugin::esm::cell::WorldspaceRecord;
use byroredux_plugin::esm::reader::GameKind;
use super::{inherit_up_chain, pnam};

/// On-disk distant-terrain family selected at the EXAL boundary.
///
/// The two baked families are not interchangeable. Skyrim and FO4 use the
/// combined `.btr`/`.bto` quadtree under `terrain`, while Oblivion and
/// FO3/FNV use older NIF terrain quads under `landscape\\lod` with different
/// texture naming. Keeping the distinction here prevents renderer-facing LOD
/// code from growing per-game filename branches (#3100).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TerrainLodLayout {
    /// Oblivion's FormID-keyed 32-cell NIF/DDS quads.
    OblivionLegacy,
    /// FO3/FNV's editor-ID-keyed level 4/8/16/32 NIF/DDS quadtree.
    FalloutLegacy,
    /// Skyrim/FO4's combined `.btr`/`.bto` quadtree.
    Combined,
    /// No currently supported authored distant-terrain family.
    None,
}

/// Canonical per-game distant-terrain source decision.
pub(crate) const fn terrain_lod_layout(game: GameKind) -> TerrainLodLayout {
    match game {
        GameKind::Oblivion => TerrainLodLayout::OblivionLegacy,
        GameKind::Fallout3NV => TerrainLodLayout::FalloutLegacy,
        GameKind::Skyrim | GameKind::Fallout4 => TerrainLodLayout::Combined,
        GameKind::Fallout76 | GameKind::Starfield => TerrainLodLayout::None,
    }
}

/// Brightness gain applied to a game's baked distant-terrain albedo so the LOD
/// ground meets the full-detail ground it stands in for.
///
/// **Skyrim: 0.5, measured.** Rendering the same Whiterun view with the valley
/// as LOD (radius 1) and as full detail (radius 6) put the baked `.btr` ground
/// at 1.9x and 2.4x the full-detail ground's luminance (87/37 and 102/53) while
/// the rock beside it matched (1.1x); canopy shadowing, the species-detail
/// layer, LAND vertex colours and the vertex normals were each ruled out (every
/// one moved the full-detail ground by ~1 point or less). The baked LOD ground
/// diffuse is simply brighter than the layered ground it represents — a trait
/// LOD generators correct with brightness/contrast settings — and 0.5 is the
/// measured ratio rounded to the nearest simple step. One scene, two regions:
/// a starting point to retune by eye, not a corpus calibration.
///
/// **FNV and FO4: 1.0, measured.** The same A/B (one pose, radius 1 against a
/// larger radius, same screen regions) put FNV's baked ground at 1.04, 0.99
/// and 1.09 of full detail (control region 0.99), and FO4's at 0.98 and 0.99
/// on the ridge and slope (0.80–0.89 where the larger radius adds props;
/// control 1.01) — no brighter than the ground they stand in for, so no
/// correction. FO4 was compared at radius 4: radius 6 exceeds its geometry
/// pool.
///
/// **Oblivion, FO76, Starfield: 1.0, unmeasured.** No guessed gain: the
/// project's no-guessing policy rules out an unmeasured threshold, so these
/// keep the baked look until measured the same way.
pub(crate) const fn terrain_lod_albedo_gain(game: GameKind) -> f32 {
    match game {
        GameKind::Skyrim => 0.5,
        _ => 1.0,
    }
}

/// One authored diffuse quad translated to the common LOD-ring contract.
/// `quad_origin` and `quad_cells` describe the image's world-cell footprint;
/// `u_runs_west` is the image orientation over that footprint. Together they
/// define the UV remap ([`Self::uv_at`]) independently of the source naming
/// scheme.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TranslatedTerrainLodTexture {
    pub(crate) diffuse_path: String,
    pub(crate) normal_path: String,
    pub(crate) quad_origin: (i32, i32),
    pub(crate) quad_cells: i32,
    /// #4898 — image U increases toward world west (FO3/FNV) rather than
    /// east (Oblivion). Image V increases toward world north for both
    /// legacy families. Read from Bethesda's own LOD meshes, which map
    /// their quad corners as follows (S = quad size, Z-up world):
    ///
    /// | Mesh | (0,0) | (S,0) | (0,S) |
    /// |---|---|---|---|
    /// | FNV `wastelandnv.level4.x0.y56`, `x44.y60`; FO3 `wasteland.level4.x0.y0`, `x-20.y16` | (1,0) | (0,0) | (1,1) |
    /// | Oblivion `60.00.00.32.nif` | (0,0) | (1,0) | (0,1) |
    ///
    /// Confirmed independently by LAND height gradients against the decoded
    /// normal texels, and by seam continuity across shared quad edges.
    pub(crate) u_runs_west: bool,
    /// Image V increases toward world **south** rather than north: the
    /// Creation `.btr` quads author V = 1 on a quad's south edge and V = 0 on
    /// its north edge (probed on vanilla Tamriel quads, which map their SW
    /// corner to (0, 1) and their NE corner to (1, 0)).
    pub(crate) v_runs_south: bool,
}

impl TranslatedTerrainLodTexture {
    /// Image UV for a Z-up world position inside this quad's footprint.
    pub(crate) fn uv_at(&self, world_x: f32, world_y_zup: f32) -> [f32; 2] {
        use byroredux_core::math::coord::EXTERIOR_CELL_UNITS;
        let size = self.quad_cells as f32 * EXTERIOR_CELL_UNITS;
        let s = (world_x - self.quad_origin.0 as f32 * EXTERIOR_CELL_UNITS) / size;
        let t = (world_y_zup - self.quad_origin.1 as f32 * EXTERIOR_CELL_UNITS) / size;
        [
            if self.u_runs_west { 1.0 - s } else { s },
            if self.v_runs_south { 1.0 - t } else { t },
        ]
    }
}

fn fmt_oblivion_lod_coord(coord: i32) -> String {
    if coord == 0 {
        "00".to_string()
    } else {
        coord.to_string()
    }
}

/// Translate a legacy game's authored distant-terrain diffuse/normal pair
/// into the common texture/UV contract used by the synthesized LOD mesh.
///
/// Disk survey on 2026-08-17 confirmed that these are real baked families,
/// not tiled-LTEX-only games: FNV ships 2,663 terrain NIFs and 4,986 DDS
/// diffuse/normal files, FO3 ships 2,153 NIFs and 3,870 DDS files, and
/// Oblivion ships 100 NIFs plus 200 DDS diffuse/normal files. The Fallout
/// layout names each level-sized quad by worldspace EditorID; Oblivion names
/// a fixed 32-cell quad by the load-order-independent low 24 bits of its WRLD
/// FormID. Skyrim/FO4 are intentionally `None` here because their `.btr`
/// provider resolves its own texture siblings (#3100).
pub(crate) fn translate_terrain_lod_textures(
    game: GameKind,
    worldspace_key: &str,
    world_form_id: u32,
    level: i32,
    qx: i32,
    qy: i32,
) -> Option<TranslatedTerrainLodTexture> {
    match terrain_lod_layout(game) {
        TerrainLodLayout::OblivionLegacy => {
            const QUAD_CELLS: i32 = 32;
            let ox = qx.div_euclid(QUAD_CELLS) * QUAD_CELLS;
            let oy = qy.div_euclid(QUAD_CELLS) * QUAD_CELLS;
            Some(TranslatedTerrainLodTexture {
                diffuse_path: format!(
                    "textures\\landscapelod\\generated\\{}.{}.{}.{}.dds",
                    world_form_id & 0x00FF_FFFF,
                    fmt_oblivion_lod_coord(ox),
                    fmt_oblivion_lod_coord(oy),
                    QUAD_CELLS,
                ),
                normal_path: format!(
                    "textures\\landscapelod\\generated\\{}.{}.{}.{}_fn.dds",
                    world_form_id & 0x00FF_FFFF,
                    fmt_oblivion_lod_coord(ox),
                    fmt_oblivion_lod_coord(oy),
                    QUAD_CELLS,
                ),
                quad_origin: (ox, oy),
                quad_cells: QUAD_CELLS,
                u_runs_west: false,
                v_runs_south: false,
            })
        }
        TerrainLodLayout::FalloutLegacy if level > 0 => {
            let world = worldspace_key.to_ascii_lowercase();
            Some(TranslatedTerrainLodTexture {
                diffuse_path: format!(
                    "textures\\landscape\\lod\\{world}\\diffuse\\{world}.n.level{level}.x{qx}.y{qy}.dds"
                ),
                normal_path: format!(
                    "textures\\landscape\\lod\\{world}\\normals\\{world}.n.level{level}.x{qx}.y{qy}.dds"
                ),
                quad_origin: (qx, qy),
                quad_cells: level,
                u_runs_west: true,
                v_runs_south: false,
            })
        }
        TerrainLodLayout::FalloutLegacy | TerrainLodLayout::Combined | TerrainLodLayout::None => {
            None
        }
    }
}

/// Resolve the worldspace-default water for exterior cells with no XCLW.
/// Returns `(default height, default water-type form)`.
///
/// Two sources, by game:
/// - **Oblivion**: WRLD carries no DNAM (verified: 0 across all 84 WRLD in
///   `Oblivion.esm`), so the default height is the global Tamriel sea level
///   Z=0 (user-confirmed), gated on the worldspace having a NAM2 water form.
/// - **FO3/FNV/Skyrim+/FO4**: the default height comes from the WRLD DNAM
///   "Land Data" second f32 (`WorldspaceRecord::default_water_height`) — it
///   is game/worldspace-specific and NOT 0 (e.g. WastelandNV -2300, Skyrim
///   Tamriel -14000), so Z=0 would be wrong. The 8-byte DNAM layout is
///   stable across Gamebryo (FNV) and Creation (Skyrim) — verified against
///   both masters. The NAM2 `water_form` supplies the water type/appearance.
///
/// Both `None` when the worldspace has no default water (no NAM2 on
/// Oblivion / no DNAM elsewhere). #1305 / OBL-D6-NEW-02.
///
/// This is the prototype of the EXAL GameVariant table (`docs/engine/exal.md`
/// §4): the one place the per-`GameKind` default-water decision lives.
///
/// # Parent inheritance (#2735)
///
/// Both halves resolve up the `WNAM` chain when the child authors neither and
/// sets the matching `PNAM` bit — `0x08` for the `NAM2` type, `0x01` for the
/// `DNAM` height. Six Skyrim, one FO3 and one FO4 worldspace author no `NAM2`
/// while explicitly flagging that they inherit it; before this they resolved
/// to no water at all, which renders as a dry ocean.
pub(crate) fn default_water_for_worldspace(
    worldspaces: &HashMap<String, WorldspaceRecord>,
    worldspace_key: &str,
    game: GameKind,
) -> (Option<f32>, Option<u32>) {
    if !worldspaces.contains_key(worldspace_key) {
        return (None, None);
    }
    let water_form = inherit_up_chain(worldspaces, worldspace_key, pnam::INHERIT_WATER, |_, w| {
        w.water_form
    });
    if game == GameKind::Oblivion {
        // No DNAM on Oblivion WRLD → sea level Z=0, only where the
        // worldspace advertises default water via NAM2. #5374: Oblivion
        // authors no PNAM, so the parse boundary stamps the inherit-all
        // word onto every child (wrld.rs — LAND|LOD|WATER|CLIMATE); the
        // bit-gated walk above is therefore the whole resolution, with
        // no second, game-aware copy of it here (#5516 removed the
        // flag-less fallback that only hand-built zero-flag records
        // could reach). BravilWorld, LeyawiinWorld, the Imperial City
        // districts and the New Sheoth worldspaces all resolve Tamriel's /
        // SEWorld's water form through the stamp.
        return match water_form {
            Some(form) => (Some(0.0), Some(form)),
            None => (None, None),
        };
    }
    // FO3/FNV/Skyrim+/FO4: the DNAM default water height is the signal that
    // the worldspace has default water; pair it with the NAM2 type form.
    match inherit_up_chain(worldspaces, worldspace_key, pnam::INHERIT_LAND, |_, w| {
        w.default_water_height
    }) {
        Some(height) => (Some(height), water_form),
        None => (None, None),
    }
}

/// What the distant (LOD) water mesh draws from, settled at the translate
/// boundary (#5335 / #5337).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DistantWaterSource {
    /// Height an exterior cell with no XCLW inherits — the SAME value the
    /// full-detail cell resolves (`default_water_for_worldspace`'s `DNAM`, or
    /// Oblivion's sea level), so an inherit cell's water does not step in
    /// height at the streaming boundary. `None` = inherit cells are dry, as
    /// they are near.
    pub inherit_height: Option<f32>,
    /// The single height `water.dump` reports as the worldspace default.
    pub display_height: f32,
    /// WATR form for the distant plane's appearance: the authored LOD form
    /// (`NAM3`) when there is one, else the full-detail default (`NAM2`).
    pub form: Option<u32>,
}

/// Combine the LOD-ring pair ([`translate_lod_water`]) with the full-detail
/// pair ([`default_water_for_worldspace`]) into what the distant mesh uses.
/// `None` when the worldspace authors neither — no water at distance.
///
/// Two quantities, two sources, one rule each:
/// * **Inherit height** is the full-detail default. `NAM4` is *not* it: the
///   two are genuinely distinct on real content (NAM4 ≠ DNAM on 22/30
///   `Skyrim.esm` worldspaces; FO3 MegatonWorld inherits Wasteland's DNAM
///   10500 but owns NAM4 = 0), and a near cell draws at DNAM.
/// * **Form** prefers the authored LOD form, falling back to the full-detail
///   one — which is how Oblivion, whose WRLD authors `NAM2` but no
///   `NAM3`/`NAM4`/`DNAM`, gets distant water at all (#5335).
pub(crate) fn resolve_distant_water(
    lod: (Option<f32>, Option<u32>),
    default: (Option<f32>, Option<u32>),
) -> Option<DistantWaterSource> {
    let (lod_height, lod_form) = lod;
    let (default_height, default_form) = default;
    let display_height = default_height.or(lod_height)?;
    Some(DistantWaterSource {
        inherit_height: default_height,
        display_height,
        form: lod_form.or(default_form),
    })
}

/// Resolve a worldspace's LOD-ring water (`NAM3`/`NAM4`) — the distant
/// counterpart of [`default_water_for_worldspace`]'s `NAM2`/`DNAM`.
/// Returns `(LOD water height, LOD water type form)`.
///
/// Unlike `default_water_for_worldspace`, no per-game branch is needed:
/// `NAM3`/`NAM4` are simply absent from Oblivion's WRLD sub-record set
/// (FO3-and-later only — disk-sampled across Oblivion.esm / Fallout3.esm /
/// FalloutNV.esm / Skyrim.esm / Fallout4.esm), so `lod_water_form` /
/// `lod_water_height` are already `None` on that era's parsed records — the
/// "Oblivion has neither" sentinel lives at the parse boundary, not here.
/// Both `None` when the worldspace authors no LOD water at all. Genuinely
/// independent of the full-detail pair on real content — NAM3≠NAM2 on 18 of
/// 28 `Fallout3.esm` worldspaces, NAM4≠DNAM on 22 of 30 `Skyrim.esm`
/// worldspaces — so a consumer must not substitute one pair for the other.
/// #2449 / EXAL-01.
///
/// # Parent inheritance (#2735 / #5496)
///
/// `NAM3`/`NAM4` resolve up the `WNAM` chain under `PNAM` bit `0x02`, which
/// correlates exactly with their absence on 7 Skyrim, 4 FO3 and 3 FO4 child
/// worldspaces. Both fields share the one bit — the data shows them always
/// authored and always omitted together. The bit is the GECK's whole-LOD
/// "Use LOD Data" checkbox: the same walk keys the distant terrain/object
/// rings themselves on the inheriting ancestor
/// ([`lod_source_worldspace`], #5496) — the bit-inheriting children ship
/// no LOD of their own, so keying the rings on the child left Statesman
/// Hotel's roof, GNR's roof, and Skyrim's walled cities with no distant
/// land or buildings at all, with inherited LOD water drawing alone.
pub(crate) fn translate_lod_water(
    worldspaces: &HashMap<String, WorldspaceRecord>,
    worldspace_key: &str,
) -> (Option<f32>, Option<u32>) {
    if !worldspaces.contains_key(worldspace_key) {
        return (None, None);
    }
    (
        inherit_up_chain(worldspaces, worldspace_key, pnam::INHERIT_LOD, |_, w| {
            w.lod_water_height
        }),
        inherit_up_chain(worldspaces, worldspace_key, pnam::INHERIT_LOD, |_, w| {
            w.lod_water_form
        }),
    )
}

/// #5496 — the worldspace the distant LOD rings are authored on: the
/// child itself, or the nearest `WNAM` ancestor reached while the child
/// chain sets `PNAM` bit `0x02`.
///
/// The GECK's checkbox is whole-LOD inheritance — "Use LOD Data: Check
/// this box to inherit LOD data from the parent world space"
/// (`geck-uesp-wiki/Category/World Spaces.wiki:17-22`; LOD water height
/// and type are sub-fields of that group). Every child that sets the bit
/// ships no LOD of its own (all four FO3 roof/vista children and all
/// seven bit-setting Skyrim children author zero quads and have no
/// `meshes\terrain\<world>` folder), so the walk needs no "has own
/// LOD" predicate: the first worldspace up the chain that does NOT set
/// the bit is the one authoring the LOD. Returns the start key when no
/// ancestor qualifies (no bit, no parent, unknown parent, or a cycle —
/// the same guards [`inherit_up_chain`] logs).
pub(crate) fn lod_source_worldspace<'a>(
    worldspaces: &'a HashMap<String, WorldspaceRecord>,
    start_key: &'a str,
) -> &'a str {
    let mut current = start_key;
    let mut visited = std::collections::HashSet::new();
    while let Some(record) = worldspaces.get(current) {
        if record.parent_flags & pnam::INHERIT_LOD == 0 {
            return current;
        }
        if !visited.insert(current) {
            log::warn!(
                "lod_source_worldspace: cyclic WNAM chain from '{start_key}' (revisited \
                 '{current}') while resolving PNAM bit {:#06X} — LOD stays on '{start_key}'",
                pnam::INHERIT_LOD,
            );
            return start_key;
        }
        let Some(parent_fid) = record.parent_worldspace else {
            log::warn!(
                "lod_source_worldspace: '{current}' sets the LOD-inherit bit but authors \
                 no WNAM parent — LOD stays on '{start_key}'",
            );
            return start_key;
        };
        let Some((parent_key, _)) = worldspaces
            .iter()
            .find(|(_, worldspace)| worldspace.form_id == parent_fid)
        else {
            log::warn!(
                "lod_source_worldspace: '{current}'s WNAM parent {parent_fid:08X} is not \
                 among parsed worldspaces — LOD stays on '{start_key}'",
            );
            return start_key;
        };
        current = parent_key;
    }
    start_key
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_skyrim_has_a_terrain_lod_albedo_gain() {
        use super::*;
        assert_eq!(terrain_lod_albedo_gain(GameKind::Skyrim), 0.5);
        for game in [
            GameKind::Oblivion,
            GameKind::Fallout3NV,
            GameKind::Fallout4,
            GameKind::Fallout76,
            GameKind::Starfield,
        ] {
            assert_eq!(
                terrain_lod_albedo_gain(game),
                1.0,
                "{game:?}: measured at 1.0 (FNV, FO4) or unmeasured (the rest)"
            );
        }
    }

    use super::*;
    use crate::env_translate::test_support::parent_child;

    // ── default_water_for_worldspace ──────────────────────────────

    /// Wrap a standalone record as the single-worldspace map the resolver
    /// takes. Keyed `"w"`; no parent, so the chain walk is inert and these
    /// tests keep measuring exactly what they measured pre-#2735.
    fn solo(wrld: WorldspaceRecord) -> HashMap<String, WorldspaceRecord> {
        HashMap::from([("w".to_string(), wrld)])
    }

    /// #3100 — FNV's real archive path must survive the EXAL translation
    /// byte-for-byte. This exact quad was resolved from Fallout - Textures2.bsa
    /// during the audit; before the fix the LOD ring only tried Oblivion's
    /// FormID-based path and silently fell through to a tiled LTEX.
    #[test]
    fn fnv_baked_lod_textures_translate_to_the_shipped_paths() {
        let translated = translate_terrain_lod_textures(
            GameKind::Fallout3NV,
            "WastelandNV",
            0x000D_DDDA,
            4,
            4,
            0,
        )
        .expect("FO3/FNV have a legacy baked terrain layout");
        assert_eq!(
            translated.diffuse_path,
            "textures\\landscape\\lod\\wastelandnv\\diffuse\\wastelandnv.n.level4.x4.y0.dds"
        );
        assert_eq!(
            translated.normal_path,
            "textures\\landscape\\lod\\wastelandnv\\normals\\wastelandnv.n.level4.x4.y0.dds"
        );
        assert_eq!(translated.quad_origin, (4, 0));
        assert_eq!(translated.quad_cells, 4);
    }

    #[test]
    fn oblivion_baked_lod_textures_keep_form_id_and_32_cell_alignment() {
        let translated =
            translate_terrain_lod_textures(GameKind::Oblivion, "Tamriel", 0xAB00_003C, 4, 17, -1)
                .expect("Oblivion has its own legacy baked terrain layout");
        assert_eq!(
            translated.diffuse_path,
            "textures\\landscapelod\\generated\\60.00.-32.32.dds"
        );
        assert_eq!(
            translated.normal_path,
            "textures\\landscapelod\\generated\\60.00.-32.32_fn.dds"
        );
        assert_eq!(translated.quad_origin, (0, -32));
        assert_eq!(translated.quad_cells, 32);
    }

    /// #4898 — the UV contract reproduces the corner UVs of Bethesda's own
    /// LOD meshes (quad-local Z-up offsets over a quad of size S).
    fn assert_lod_corners(lod: &TranslatedTerrainLodTexture, corners: [([f32; 2], [f32; 2]); 3]) {
        use byroredux_core::math::coord::EXTERIOR_CELL_UNITS;
        let size = lod.quad_cells as f32 * EXTERIOR_CELL_UNITS;
        let ox = lod.quad_origin.0 as f32 * EXTERIOR_CELL_UNITS;
        let oy = lod.quad_origin.1 as f32 * EXTERIOR_CELL_UNITS;
        for (local, expected) in corners {
            let uv = lod.uv_at(ox + local[0] * size, oy + local[1] * size);
            assert!(
                (uv[0] - expected[0]).abs() < 1e-6 && (uv[1] - expected[1]).abs() < 1e-6,
                "{}: quad-local {local:?} maps to {uv:?}, Bethesda's mesh authors {expected:?}",
                lod.diffuse_path
            );
        }
    }

    #[test]
    fn fallout_lod_quads_run_u_west_and_v_north_like_bethesdas_meshes() {
        // FNV wastelandnv.level4.x0.y56 / x44.y60, FO3 wasteland.level4.x0.y0:
        // (0,0)→(1,0), (S,0)→(0,0), (0,S)→(1,1).
        for (world, qx, qy) in [("WastelandNV", 0, 56), ("WastelandNV", 44, 60), ("Wasteland", 0, 0)] {
            let lod = translate_terrain_lod_textures(GameKind::Fallout3NV, world, 0, 4, qx, qy)
                .expect("Fallout legacy layout");
            assert_lod_corners(&lod, [([0.0, 0.0], [1.0, 0.0]), ([1.0, 0.0], [0.0, 0.0]), ([0.0, 1.0], [1.0, 1.0])]);
        }
    }

    #[test]
    fn oblivion_lod_quads_run_u_east_and_v_north_like_bethesdas_meshes() {
        // Oblivion 60.00.00.32.nif: (0,0)→(0,0), (S,0)→(1,0), (0,S)→(0,1).
        let lod = translate_terrain_lod_textures(GameKind::Oblivion, "Tamriel", 0x3C, 32, 0, 0)
            .expect("Oblivion legacy layout");
        assert_lod_corners(&lod, [([0.0, 0.0], [0.0, 0.0]), ([1.0, 0.0], [1.0, 0.0]), ([0.0, 1.0], [0.0, 1.0])]);
    }

    #[test]
    fn combined_and_unsupported_layouts_do_not_claim_legacy_texture_paths() {
        for game in [
            GameKind::Skyrim,
            GameKind::Fallout4,
            GameKind::Fallout76,
            GameKind::Starfield,
        ] {
            assert_eq!(
                translate_terrain_lod_textures(game, "world", 1, 4, 0, 0),
                None,
                "{game:?} must not be routed through a legacy LOD path"
            );
        }
    }

    /// #1305 / OBL-D6-NEW-02 — an Oblivion worldspace with a NAM2 default
    /// water form makes its no-XCLW cells default to water at the Tamriel
    /// sea level Z=0 (Oblivion WRLD has no DNAM height field, so the
    /// constant is load-bearing). Pins both the gate (only when water_form
    /// present) and the user-confirmed 0.0 height.
    #[test]
    fn oblivion_worldspace_with_water_form_defaults_to_sea_level() {
        let wrld = WorldspaceRecord {
            water_form: Some(0x0000_1234),
            ..Default::default()
        };
        assert_eq!(
            default_water_for_worldspace(&solo(wrld.clone()), "w", GameKind::Oblivion),
            (Some(0.0), Some(0x0000_1234)),
            "Oblivion worldspace advertising default water → no-XCLW cells get Z=0 water"
        );
    }

    #[test]
    fn worldspace_without_water_form_has_no_default_water() {
        let wrld = WorldspaceRecord {
            water_form: None,
            ..Default::default()
        };
        assert_eq!(
            default_water_for_worldspace(&solo(wrld.clone()), "w", GameKind::Oblivion),
            (None, None)
        );
        // A missing worldspace record likewise yields no default water.
        assert_eq!(
            default_water_for_worldspace(&HashMap::new(), "missing", GameKind::Oblivion),
            (None, None)
        );
    }

    /// Non-Oblivion games must NOT be forced to Z=0: a NAM2 water_form
    /// alone (no DNAM default height parsed) yields no default water, so
    /// the loader never invents sea level for FO3/FNV/Skyrim+ where the
    /// real default lives in DNAM (e.g. WastelandNV -2300, Skyrim Tamriel
    /// -14000). Pins that the Oblivion Z=0 path does not leak to them.
    #[test]
    fn non_oblivion_without_dnam_gets_no_default() {
        let wrld = WorldspaceRecord {
            water_form: Some(0x0000_1234),
            default_water_height: None,
            ..Default::default()
        };
        for game in [
            GameKind::Fallout3NV,
            GameKind::Skyrim,
            GameKind::Fallout4,
            GameKind::Fallout76,
            GameKind::Starfield,
        ] {
            assert_eq!(
                default_water_for_worldspace(&solo(wrld.clone()), "w", game),
                (None, None),
                "{game:?} with no DNAM default height must NOT be forced to Z=0"
            );
        }
    }

    /// Non-Oblivion games use the WRLD DNAM default water height (second
    /// f32 of "Land Data"), paired with the NAM2 type form — NOT Z=0.
    /// Pins the #1305 follow-up (FO3/FNV/Skyrim+ default-water inheritance).
    #[test]
    fn non_oblivion_uses_dnam_default_water_height() {
        let wrld = WorldspaceRecord {
            water_form: Some(0x0000_00AB),
            default_water_height: Some(-2300.0), // e.g. WastelandNV
            ..Default::default()
        };
        for game in [GameKind::Fallout3NV, GameKind::Skyrim, GameKind::Fallout4] {
            assert_eq!(
                default_water_for_worldspace(&solo(wrld.clone()), "w", game),
                (Some(-2300.0), Some(0x0000_00AB)),
                "{game:?} no-XCLW cells inherit the DNAM default water height + NAM2 type"
            );
        }
        // Oblivion ignores DNAM (it has none) and stays on Z=0.
        assert_eq!(
            default_water_for_worldspace(&solo(wrld.clone()), "w", GameKind::Oblivion),
            (Some(0.0), Some(0x0000_00AB))
        );
    }

    /// #5335 — Oblivion's WRLD authors `NAM2` and no `NAM3`/`NAM4`/`DNAM`.
    /// Distant water must still exist: sea level 0 and the NAM2 form come
    /// from the full-detail resolver, through the one translate-boundary
    /// rule — not a game branch in the streamer.
    #[test]
    fn oblivion_gets_distant_water_from_the_full_detail_default() {
        let wrld = WorldspaceRecord {
            water_form: Some(0x0000_0A11),
            ..Default::default()
        };
        let worldspaces = solo(wrld);
        let lod = translate_lod_water(&worldspaces, "w");
        assert_eq!(lod, (None, None), "Oblivion authors no LOD water fields");
        let default = default_water_for_worldspace(&worldspaces, "w", GameKind::Oblivion);
        assert_eq!(
            resolve_distant_water(lod, default),
            Some(DistantWaterSource {
                inherit_height: Some(0.0),
                display_height: 0.0,
                form: Some(0x0000_0A11),
            })
        );
    }

    /// #5337 — an inherit cell far-draws at the height the near cell uses
    /// (`DNAM`), not `NAM4`. FO3 MegatonWorld: inherits Wasteland's DNAM
    /// 10500 but owns NAM4 = 0.
    #[test]
    fn distant_inherit_height_is_the_full_detail_default_not_nam4() {
        let source = resolve_distant_water(
            (Some(0.0), Some(0x0000_00B2)),       // NAM4 / NAM3
            (Some(10500.0), Some(0x0000_00B1)),   // DNAM / NAM2
        )
        .expect("water");
        assert_eq!(source.inherit_height, Some(10500.0));
        assert_eq!(source.form, Some(0x0000_00B2), "authored LOD form wins");
        // A worldspace with LOD water but no default water: inherit cells
        // are dry at distance exactly as they are near.
        let lod_only = resolve_distant_water((Some(-500.0), None), (None, None)).expect("water");
        assert_eq!(lod_only.inherit_height, None);
        assert_eq!(lod_only.display_height, -500.0);
        // Neither authored: no distant water.
        assert_eq!(resolve_distant_water((None, None), (None, None)), None);
    }

    // ── PNAM parent inheritance (#2735) ──────────────────────────

    /// The measured defect: 6 Skyrim + 1 FO3 + 1 FO4 worldspaces author no
    /// NAM2, set PNAM 0x08, and rendered no water at all before this.
    #[test]
    fn child_inherits_the_parents_water_type_and_height() {
        let map = parent_child(
            WorldspaceRecord {
                water_form: Some(0x0000_00AB),
                default_water_height: Some(-14000.0), // e.g. Skyrim Tamriel
                ..Default::default()
            },
            pnam::INHERIT_WATER | pnam::INHERIT_LAND,
            WorldspaceRecord::default(),
        );
        assert_eq!(
            default_water_for_worldspace(&map, "c", GameKind::Skyrim),
            (Some(-14000.0), Some(0x0000_00AB)),
        );
    }

    #[test]
    fn a_childs_own_water_always_beats_the_parents() {
        // Authored value wins even with the inherit bit set — the bit means
        // "I have none", not "override me".
        let map = parent_child(
            WorldspaceRecord {
                water_form: Some(0x0000_00AB),
                default_water_height: Some(-14000.0),
                ..Default::default()
            },
            pnam::INHERIT_WATER | pnam::INHERIT_LAND,
            WorldspaceRecord {
                water_form: Some(0x0000_00CD),
                default_water_height: Some(-2300.0),
                ..Default::default()
            },
        );
        assert_eq!(
            default_water_for_worldspace(&map, "c", GameKind::Skyrim),
            (Some(-2300.0), Some(0x0000_00CD)),
        );
    }

    #[test]
    fn without_the_bit_a_child_stays_waterless() {
        // Absent value AND no inherit bit is "genuinely unauthored", which
        // must not silently pick up the parent's ocean.
        let map = parent_child(
            WorldspaceRecord {
                water_form: Some(0x0000_00AB),
                default_water_height: Some(-14000.0),
                ..Default::default()
            },
            0,
            WorldspaceRecord::default(),
        );
        assert_eq!(
            default_water_for_worldspace(&map, "c", GameKind::Skyrim),
            (None, None),
        );
    }

    #[test]
    fn the_two_water_bits_are_independent() {
        // 0x08 governs NAM2, 0x01 governs DNAM. A child inheriting only the
        // type must not also acquire the parent's height.
        let map = parent_child(
            WorldspaceRecord {
                water_form: Some(0x0000_00AB),
                default_water_height: Some(-14000.0),
                ..Default::default()
            },
            pnam::INHERIT_WATER,
            WorldspaceRecord::default(),
        );
        // No own DNAM and no 0x01 → no height → no default water at all.
        assert_eq!(
            default_water_for_worldspace(&map, "c", GameKind::Skyrim),
            (None, None),
        );
    }

    #[test]
    fn lod_water_inherits_under_its_own_bit() {
        let map = parent_child(
            WorldspaceRecord {
                lod_water_form: Some(0x0000_0777),
                lod_water_height: Some(-9000.0),
                ..Default::default()
            },
            pnam::INHERIT_LOD,
            WorldspaceRecord::default(),
        );
        assert_eq!(
            translate_lod_water(&map, "c"),
            (Some(-9000.0), Some(0x0000_0777))
        );
        // …and not under the full-detail water bit.
        let wrong_bit = parent_child(
            WorldspaceRecord {
                lod_water_form: Some(0x0000_0777),
                lod_water_height: Some(-9000.0),
                ..Default::default()
            },
            pnam::INHERIT_WATER,
            WorldspaceRecord::default(),
        );
        assert_eq!(translate_lod_water(&wrong_bit, "c"), (None, None));
    }

    #[test]
    fn inheritance_walks_more_than_one_hop() {
        // Skyrim holdout worlds nest more than one level deep.
        let grandparent = 0x0000_0100;
        let parent = 0x0000_0200;
        let map = HashMap::from([
            (
                "gp".to_string(),
                WorldspaceRecord {
                    form_id: grandparent,
                    water_form: Some(0x0000_00AB),
                    default_water_height: Some(-14000.0),
                    ..Default::default()
                },
            ),
            (
                "p".to_string(),
                WorldspaceRecord {
                    form_id: parent,
                    parent_worldspace: Some(grandparent),
                    parent_flags: pnam::INHERIT_WATER | pnam::INHERIT_LAND,
                    ..Default::default()
                },
            ),
            (
                "c".to_string(),
                WorldspaceRecord {
                    form_id: 0x0000_0300,
                    parent_worldspace: Some(parent),
                    parent_flags: pnam::INHERIT_WATER | pnam::INHERIT_LAND,
                    ..Default::default()
                },
            ),
        ]);
        assert_eq!(
            default_water_for_worldspace(&map, "c", GameKind::Skyrim),
            (Some(-14000.0), Some(0x0000_00AB)),
        );
    }

    #[test]
    fn a_cyclic_chain_terminates_instead_of_hanging() {
        // Corrupt/adversarial plugin data must not spin the loader.
        let a = 0x0000_0100;
        let b = 0x0000_0200;
        let map = HashMap::from([
            (
                "a".to_string(),
                WorldspaceRecord {
                    form_id: a,
                    parent_worldspace: Some(b),
                    parent_flags: pnam::INHERIT_WATER | pnam::INHERIT_LAND,
                    ..Default::default()
                },
            ),
            (
                "b".to_string(),
                WorldspaceRecord {
                    form_id: b,
                    parent_worldspace: Some(a),
                    parent_flags: pnam::INHERIT_WATER | pnam::INHERIT_LAND,
                    ..Default::default()
                },
            ),
        ]);
        assert_eq!(
            default_water_for_worldspace(&map, "a", GameKind::Skyrim),
            (None, None),
        );
    }

    #[test]
    fn a_dangling_parent_reference_resolves_to_nothing() {
        // WNAM pointing outside the parsed set (missing master) must not panic.
        let map = HashMap::from([(
            "c".to_string(),
            WorldspaceRecord {
                form_id: 0x0000_0200,
                parent_worldspace: Some(0x0DEA_DBEE),
                parent_flags: pnam::INHERIT_WATER | pnam::INHERIT_LAND,
                ..Default::default()
            },
        )]);
        assert_eq!(
            default_water_for_worldspace(&map, "c", GameKind::Skyrim),
            (None, None),
        );
    }

    #[test]
    fn oblivion_child_inherits_parents_water_through_the_parse_stamp() {
        // #5374 / #5516 — Oblivion authors no PNAM, so the parse
        // boundary stamps the inherit-all word onto every child; the
        // bit-gated walk is the whole resolution (the census: 54/54
        // roots author NAM2, 0/30 children do; the below-sea-level
        // cells cluster in the children — BravilWorld, LeyawiinWorld,
        // the IC districts, New Sheoth). A child with NO own NAM2
        // resolves the parent's water form; a child with its own keeps
        // it. `0x08` is the stamped `INHERIT_WATER` bit a parsed child
        // carries.
        let map = parent_child(
            WorldspaceRecord {
                water_form: Some(0x0000_00AB),
                ..Default::default()
            },
            pnam::INHERIT_WATER,
            WorldspaceRecord::default(),
        );
        assert_eq!(
            default_water_for_worldspace(&map, "c", GameKind::Oblivion),
            (Some(0.0), Some(0x0000_00AB)),
            "a child worldspace without its own NAM2 inherits the parent's \
             water through the stamped inherit bit"
        );
        // A root worldspace with no own water still resolves nothing —
        // the walk only follows a real WNAM.
        let root = HashMap::from([(
            "r".to_string(),
            WorldspaceRecord {
                form_id: 0x0000_0300,
                ..Default::default()
            },
        )]);
        assert_eq!(
            default_water_for_worldspace(&root, "r", GameKind::Oblivion),
            (None, None)
        );
        // An authored own value still wins over the parent.
        let both = parent_child(
            WorldspaceRecord {
                water_form: Some(0x0000_00AB),
                ..Default::default()
            },
            pnam::INHERIT_WATER,
            WorldspaceRecord {
                water_form: Some(0x0000_00CD),
                ..Default::default()
            },
        );
        assert_eq!(
            default_water_for_worldspace(&both, "c", GameKind::Oblivion),
            (Some(0.0), Some(0x0000_00CD)),
        );
    }
}

/// #5496 — the distant-LOD source walk: the first worldspace up the
/// WNAM chain that does NOT set the "Use LOD Data" bit authors the
/// rings. The corpus shapes: FO3's StatesmanRoofWorld / GNRroofWorld
/// (bit set, zero own quads) inherit Wasteland; MegatonWorld (bit
/// clear, no quads) inherits nothing; DCworld01 (bit clear) keeps its
/// own; Skyrim's walled cities chain to Tamriel.
#[cfg(test)]
mod lod_source_tests {
    use super::lod_source_worldspace;
    use super::pnam::INHERIT_LOD;
    use std::collections::HashMap;

    fn world(form_id: u32, parent: Option<u32>, flags: u16) -> super::WorldspaceRecord {
        super::WorldspaceRecord {
            form_id,
            parent_worldspace: parent,
            parent_flags: flags,
            ..Default::default()
        }
    }

    fn chain() -> HashMap<String, super::WorldspaceRecord> {
        let mut map = HashMap::new();
        map.insert("wasteland".to_string(), world(0x01, None, 0x00));
        map.insert("statesmanroofworld".to_string(), world(0x02, Some(0x01), 0xC6));
        map.insert("megatonworld".to_string(), world(0x03, Some(0x01), 0xC5));
        map.insert("dcworld01".to_string(), world(0x04, Some(0x01), 0xC4));
        map.insert("whiterunworld".to_string(), world(0x05, Some(0x06), 0x7F));
        map.insert("tamriel".to_string(), world(0x06, None, 0x00));
        map
    }

    #[test]
    fn inheriting_child_resolves_to_the_parent() {
        let map = chain();
        assert_eq!(lod_source_worldspace(&map, "statesmanroofworld"), "wasteland");
        assert_eq!(lod_source_worldspace(&map, "whiterunworld"), "tamriel");
    }

    #[test]
    fn oblivion_inherit_all_parse_stamp_routes_child_lod_to_tamriel() {
        // The plugin parser pins that an Oblivion WNAM child with no PNAM
        // receives the 0x1B inherit-all stamp. Feed that parsed shape through
        // the production source walk: terrain LOD must use the root's quads.
        let mut map = HashMap::new();
        map.insert("tamriel".to_string(), world(0x01, None, 0));
        map.insert(
            "bravilworld".to_string(),
            world(0x02, Some(0x01), 0x1B),
        );
        assert_eq!(lod_source_worldspace(&map, "bravilworld"), "tamriel");
    }

    #[test]
    fn child_without_the_bit_keeps_itself() {
        let map = chain();
        assert_eq!(lod_source_worldspace(&map, "dcworld01"), "dcworld01");
        // MegatonWorld: bit clear, ships no LOD, inherits none — the
        // source is itself either way (an empty distant ring).
        assert_eq!(lod_source_worldspace(&map, "megatonworld"), "megatonworld");
    }

    #[test]
    fn unknown_parent_or_cycle_stays_on_the_start() {
        let mut map = chain();
        map.insert(
            "orphanworld".to_string(),
            world(0x07, Some(0xFF), INHERIT_LOD),
        );
        assert_eq!(lod_source_worldspace(&map, "orphanworld"), "orphanworld");

        // A two-node cycle under the bit must terminate, not hang.
        let mut cycle = HashMap::new();
        cycle.insert("a".to_string(), world(0x10, Some(0x11), INHERIT_LOD));
        cycle.insert("b".to_string(), world(0x11, Some(0x10), INHERIT_LOD));
        assert_eq!(lod_source_worldspace(&cycle, "a"), "a");
    }

    /// Real-data pin: the FO3 chain on the installed master.
    /// StatesmanRoofWorld's PNAM is 0xC6 (bit set) with WNAM
    /// Wasteland — the Reilly's Rangers vista whose distant ring was
    /// empty pre-fix.
    #[test]
    #[ignore = "needs FO3 game data on disk"]
    fn installed_fo3_statesman_roof_inherits_wasteland_lod() {
        let esm = std::fs::read(byroredux_plugin::esm::test_paths::fo3_esm())
            .expect("read Fallout3.esm");
        let index = byroredux_plugin::esm::records::parse_esm(&esm)
            .expect("parse");
        let worldspaces = &index.cells.worldspaces;
        assert_eq!(
            lod_source_worldspace(worldspaces, "statesmanroofworld"),
            "wasteland",
            "the roof vista must draw Wasteland's distant rings (#5496)"
        );
        assert_eq!(
            lod_source_worldspace(worldspaces, "megatonworld"),
            "megatonworld",
            "MegatonWorld clears the bit and owns (no) LOD itself"
        );
    }
}
