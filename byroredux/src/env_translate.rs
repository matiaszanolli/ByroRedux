//! EXAL (Exterior Abstraction Layer) — the exterior **environment**
//! translation boundary.
//!
//! This module is to outdoors rendering what [`crate::material_translate`]
//! is to materials: the single home where per-game [`byroredux_plugin`]
//! ESM records (WRLD / CLMT / WTHR / LAND / WATR / CELL lighting) are
//! resolved into the engine's canonical, game-agnostic representation.
//! Everything downstream — the sky pass, the terrain pass, the water
//! pass, the sun directional light, the LOD ring — consumes the canonical
//! resources identically for every game, with no per-game branches and no
//! render-time fallbacks.
//!
//! Architecture + rollout: see `docs/engine/exal.md`. Per the canonical-
//! type rule it shares with NIFAL (`docs/engine/nifal.md`), the canonical
//! tier is the ECS resource/component that already serves the renderer-
//! facing role ([`WaterMaterial`], `SkyParamsRes`, `WeatherDataRes`,
//! `CellLightingRes`, …); this module is the `translate()` step, not a new
//! type.
//!
//! Step 1 (this slice) establishes the module and gathers the two
//! already-single-site **water** translates here verbatim:
//! [`default_water_for_worldspace`] (worldspace-default water height +
//! type) and [`resolve_water_material`] (WATR → [`WaterMaterial`]). Later
//! steps fold in the scattered sky / sun / weather producers (see the
//! `docs/engine/exal.md` §7 rollout).

use std::collections::HashMap;

use byroredux_core::ecs::components::water::{
    SubmersionState, WaterFlow, WaterKind, WaterMaterial,
};
use byroredux_plugin::esm;
use byroredux_plugin::esm::cell::WorldspaceRecord;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::{ClimateRecord, WeatherRecord};

use crate::components::{CellLightingRes, DalcCubeYup, SkyParamsRes, WeatherDataRes};

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

/// One authored diffuse quad translated to the common LOD-ring contract.
/// `quad_origin` and `quad_cells` describe the image's world-cell footprint
/// and therefore the UV remap independently of its source naming scheme.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TranslatedTerrainLodTexture {
    pub(crate) diffuse_path: String,
    pub(crate) normal_path: String,
    pub(crate) quad_origin: (i32, i32),
    pub(crate) quad_cells: i32,
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
    let water_form = inherit_up_chain(worldspaces, worldspace_key, pnam::INHERIT_WATER, |w| {
        w.water_form
    });
    if game == GameKind::Oblivion {
        // No DNAM on Oblivion WRLD → sea level Z=0, only where the
        // worldspace advertises default water via NAM2. (Oblivion authors no
        // PNAM either, so the chain walk is a no-op there and this stays the
        // pre-#2735 behaviour exactly.)
        return match water_form {
            Some(form) => (Some(0.0), Some(form)),
            None => (None, None),
        };
    }
    // FO3/FNV/Skyrim+/FO4: the DNAM default water height is the signal that
    // the worldspace has default water; pair it with the NAM2 type form.
    match inherit_up_chain(worldspaces, worldspace_key, pnam::INHERIT_LAND, |w| {
        w.default_water_height
    }) {
        Some(height) => (Some(height), water_form),
        None => (None, None),
    }
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
/// # Parent inheritance (#2735)
///
/// `NAM3`/`NAM4` resolve up the `WNAM` chain under `PNAM` bit `0x02`, which
/// correlates exactly with their absence on 7 Skyrim, 4 FO3 and 3 FO4 child
/// worldspaces. Both fields share the one bit — the data shows them always
/// authored and always omitted together.
pub(crate) fn translate_lod_water(
    worldspaces: &HashMap<String, WorldspaceRecord>,
    worldspace_key: &str,
) -> (Option<f32>, Option<u32>) {
    if !worldspaces.contains_key(worldspace_key) {
        return (None, None);
    }
    (
        inherit_up_chain(worldspaces, worldspace_key, pnam::INHERIT_LOD, |w| {
            w.lod_water_height
        }),
        inherit_up_chain(worldspaces, worldspace_key, pnam::INHERIT_LOD, |w| {
            w.lod_water_form
        }),
    )
}

/// `PNAM` parent-use flags — which fields a child worldspace takes from its
/// [`WNAM`](WorldspaceRecord::parent_worldspace) parent.
///
/// # How these were identified
///
/// The bit meanings are not documented in the record; they were established by
/// correlating each bit against the *absence* of the sub-record it would
/// govern, across every child worldspace in the shipped masters (#2735):
///
/// | Bit | Sub-record | Skyrim | FO3 | FO4 | FNV |
/// |---|---|---|---|---|---|
/// | `0x01` | `DNAM` land data | 7/7 | 4/4 | 1/1 | — |
/// | `0x02` | `NAM3`+`NAM4` LOD water | 7/7 | 4/4 | 3/3 | — |
/// | `0x04` | `ICON` map | 34/34 | 23/23 | 3/3 | 10/10 |
/// | `0x08` | `NAM2` water | 6/6 | 1/1 | 1/1 | — |
/// | `0x10` | `CNAM` climate | 20 set / 23 absent | 1/1 | 3/3 | — |
///
/// Every ratio is "bit set" over "own sub-record absent", and they match
/// exactly in every game that uses the bit. The invariant the data supports is
/// therefore **bit set ⟺ no own value, take the parent's**.
///
/// Climate is the one inexact row and it confirms rather than undermines the
/// reading: three Skyrim children lack `CNAM` *without* setting the bit, i.e.
/// they are genuinely climate-less rather than inheriting — which is precisely
/// the distinction [`resolve_worldspace_climate`] already draws.
mod pnam {
    /// `DNAM` — land data, which carries the default water *height*.
    pub(super) const INHERIT_LAND: u16 = 0x01;
    /// `NAM3`/`NAM4` — the distant LOD ring's water type and height.
    pub(super) const INHERIT_LOD: u16 = 0x02;
    /// `ICON` — worldspace map texture. Parsed but currently unconsumed, so
    /// deliberately not wired; add a resolver alongside its first consumer.
    #[allow(dead_code)]
    pub(super) const INHERIT_MAP: u16 = 0x04;
    /// `NAM2` — default water *type*.
    pub(super) const INHERIT_WATER: u16 = 0x08;
    /// `CNAM` — climate.
    pub(super) const INHERIT_CLIMATE: u16 = 0x10;
}

use pnam::INHERIT_CLIMATE as PNAM_INHERIT_CLIMATE;

/// Resolve one inheritable worldspace field, walking the `WNAM` chain.
///
/// A child's own authored value always wins; the chain is consulted only when
/// the field is absent **and** `bit` is set. That ordering is what the
/// correlation above establishes, and it means a worldspace that authors a
/// value can never have it overridden by an ancestor.
///
/// Cycle-guarded: corrupt or adversarial plugin data terminates the walk
/// instead of hanging, matching [`resolve_worldspace_climate`].
fn inherit_up_chain<T, F>(
    worldspaces: &HashMap<String, WorldspaceRecord>,
    start_key: &str,
    bit: u16,
    extract: F,
) -> Option<T>
where
    F: Fn(&WorldspaceRecord) -> Option<T>,
{
    let mut current = start_key.to_string();
    let mut visited = std::collections::HashSet::new();
    loop {
        let record = worldspaces.get(&current)?;
        if let Some(value) = extract(record) {
            return Some(value);
        }
        if record.parent_flags & bit == 0 {
            // No own value and not flagged to inherit — genuinely unauthored,
            // not an inheritance gap.
            return None;
        }
        if !visited.insert(current.clone()) {
            log::warn!(
                "inherit_up_chain: cyclic WNAM chain from '{start_key}' (revisited \
                 '{current}') while resolving PNAM bit {bit:#06X} — treating as unresolved",
            );
            return None;
        }
        let parent_fid = record.parent_worldspace.or_else(|| {
            log::warn!(
                "inherit_up_chain: '{current}' sets PNAM bit {bit:#06X} but authors no WNAM \
                 parent — chain terminates unresolved (from '{start_key}')",
            );
            None
        })?;
        let (parent_key, _) = worldspaces
            .iter()
            .find(|(_, w)| w.form_id == parent_fid)
            .or_else(|| {
                log::warn!(
                    "inherit_up_chain: '{current}'s WNAM parent {parent_fid:08X} is not among \
                     parsed worldspaces — chain terminates unresolved (from '{start_key}')",
                );
                None
            })?;
        current = parent_key.clone();
    }
}

/// Resolve a worldspace's climate FormID (the `CLMT` a `CNAM` sub-record
/// authors), chasing the `WNAM` parent-worldspace chain when the
/// worldspace has no own `CNAM` and its `PNAM` flags opt into climate
/// inheritance (#2450 / EXAL-02).
///
/// Before this, climate resolution was a single flat
/// `worldspace_climates.get(&worldspace_key)` lookup — a child worldspace
/// that relies on parent inheritance (Skyrim's DLC/holdout worlds, FO4
/// sub-worlds, Oblivion-plane worlds) always missed, silently falling back
/// to the procedural-fallback sky at the call site instead of its parent's
/// actual climate.
///
/// `worldspaces` and `worldspace_climates` are both keyed by the same
/// lowercased-editor-id string (`worldspace_key` in the cell loader);
/// `parent_worldspace` is a raw FormID, so each hop needs a reverse lookup
/// by `WorldspaceRecord::form_id` — worldspace counts are small (tens),
/// so a linear scan per hop is fine; this runs once per worldspace entry,
/// not per frame. Guards against a cyclic `WNAM` chain (corrupt/adversarial
/// plugin data) via `visited`. Logs at `warn` when the chain terminates
/// unresolved so a real inheritance gap is diagnosable instead of reading
/// as an unrelated weather bug.
pub(crate) fn resolve_worldspace_climate(
    worldspaces: &HashMap<String, WorldspaceRecord>,
    worldspace_climates: &HashMap<String, u32>,
    start_key: &str,
) -> Option<u32> {
    let mut current_key = start_key;
    let mut visited = std::collections::HashSet::new();
    loop {
        if let Some(&clmt_fid) = worldspace_climates.get(current_key) {
            return Some(clmt_fid);
        }
        if !visited.insert(current_key) {
            log::warn!(
                "resolve_worldspace_climate: cyclic WNAM parent chain detected starting from \
                 '{start_key}' (revisited '{current_key}') — treating as unresolved",
            );
            return None;
        }
        let record = worldspaces.get(current_key)?;
        if record.parent_flags & PNAM_INHERIT_CLIMATE == 0 {
            // No own CNAM, and not flagged to inherit one — this
            // worldspace is genuinely climate-less (falls back to the
            // engine's procedural default), not an inheritance gap.
            return None;
        }
        let Some(parent_fid) = record.parent_worldspace else {
            log::warn!(
                "resolve_worldspace_climate: '{current_key}' sets the PNAM climate-inherit bit \
                 but authors no WNAM parent — chain terminates unresolved (starting from \
                 '{start_key}')",
            );
            return None;
        };
        let Some((parent_key, _)) = worldspaces.iter().find(|(_, w)| w.form_id == parent_fid)
        else {
            log::warn!(
                "resolve_worldspace_climate: '{current_key}'s WNAM parent {parent_fid:08X} was \
                 not found among parsed worldspaces — chain terminates unresolved (starting \
                 from '{start_key}')",
            );
            return None;
        };
        current_key = parent_key.as_str();
    }
}

/// Resolve the climate in effect for one exterior cell: its own `XCCM`
/// override when it authors one, otherwise the worldspace climate
/// [`resolve_worldspace_climate`] settled (#2451 / EXAL-03).
///
/// Skyrim+ cells can pin a CLMT for one cell — scripted weather pockets,
/// boss arenas, exteriors meant to feel enclosed. Pre-fix `XCCM` was
/// parsed onto `CellData::climate_override` and read by nothing, so every
/// such cell silently rendered the worldspace default.
///
/// An override that doesn't resolve to a parsed CLMT (missing master, a
/// FormID the load order never supplied) falls back to the worldspace
/// climate rather than to no climate at all: the alternative would drop a
/// cell with an authored-but-broken override into the procedural fallback
/// sky, which is strictly further from what the author asked for. Logged
/// at `warn` so the broken link is diagnosable.
///
/// Takes the already-extracted `CellData::climate_override` rather than the
/// cell: the decision is about two FormIDs and their resolvability, and
/// keeping the record out of the signature keeps it pure and testable
/// without an ESM or a Vulkan context.
pub(crate) fn resolve_cell_climate(
    cell_climate_override: Option<u32>,
    worldspace_climate: Option<u32>,
    climates: &HashMap<u32, ClimateRecord>,
) -> Option<u32> {
    let Some(override_fid) = cell_climate_override else {
        return worldspace_climate;
    };
    if climates.contains_key(&override_fid) {
        return Some(override_fid);
    }
    log::warn!(
        "resolve_cell_climate: cell XCCM climate {override_fid:08X} is not among parsed CLMT \
         records — falling back to the worldspace climate {worldspace_climate:08X?}",
    );
    worldspace_climate
}

/// The default weather for `climate`: its highest-chance WTHR entry that
/// resolves to a parsed record.
///
/// Negative chances are mod sentinels / subtractive weights and are
/// filtered before the max (#476). Shared by the once-per-worldspace
/// resolve in `build_exterior_world_context` and the per-cell XCCM
/// re-resolve (#2451), so a cell override picks its weather by exactly the
/// same rule the worldspace default did.
pub(crate) fn resolve_default_weather<'a>(
    climate: &ClimateRecord,
    weathers: &'a HashMap<u32, WeatherRecord>,
) -> Option<(&'a WeatherRecord, i32)> {
    let best = climate
        .weathers
        .iter()
        .filter(|w| w.chance >= 0)
        .max_by_key(|w| w.chance)?;
    weathers
        .get(&best.weather_form_id)
        .map(|wthr| (wthr, best.chance))
}

/// Every worldspace key from `start_key` up its `WNAM` parent chain, most
/// specific first.
///
/// Unlike [`resolve_worldspace_climate`] this walks the chain **unconditionally**
/// rather than gating on `PNAM`'s climate-inherit bit. The two ask different
/// questions: that bit governs whether a child inherits its parent's *weather*,
/// while a child worldspace sits physically inside its parent regardless of how
/// its sky is authored. Ground-cover climate is a geographic property, so the
/// chain is the right input even when weather inheritance is switched off.
///
/// Cycle-guarded like its sibling; a corrupt chain truncates rather than hangs.
pub(crate) fn worldspace_name_chain(
    worldspaces: &HashMap<String, WorldspaceRecord>,
    start_key: &str,
) -> Vec<String> {
    let mut chain = vec![start_key.to_string()];
    let mut visited = std::collections::HashSet::new();
    let mut current = start_key.to_string();
    loop {
        if !visited.insert(current.clone()) {
            log::warn!(
                "worldspace_name_chain: cyclic WNAM chain from '{start_key}' \
                 (revisited '{current}') — truncating",
            );
            return chain;
        }
        let Some(record) = worldspaces.get(&current) else {
            return chain;
        };
        let Some(parent_fid) = record.parent_worldspace else {
            return chain;
        };
        let Some((parent_key, _)) = worldspaces.iter().find(|(_, w)| w.form_id == parent_fid)
        else {
            return chain;
        };
        chain.push(parent_key.clone());
        current = parent_key.clone();
    }
}

/// Shader UV-scroll rate produced by 1 BU/s of canonical [`WaterFlow`]
/// (#2872).
///
/// Calibrated against the two constants the engine already documents, not
/// invented: `WaterMaterial::default().scroll_a == [0.020, 0.011]` — a
/// magnitude of `hypot(0.020, 0.011) ≈ 0.02283` UV/s — is the authored look
/// of the slowest current [`WaterFlow::speed`] documents, its
/// `SPEED_MIN` "calm river" anchor of 0.5 BU/s. So one BU/s of flow scrolls
/// the wave layer `0.02283 / 0.5 ≈ 0.04565` UV/s, and a `River` plane
/// reproduces the default appearance exactly while `Rapids` / `Waterfall`
/// scale up in proportion to the current the physics sink is simulating.
const WATER_SCROLL_UV_PER_BU_PER_S: f32 = 0.045_651;

/// Resolve a cell's `XCWT` FormID to an engine [`WaterMaterial`]
/// plus a [`WaterKind`] (currently always `Calm`) plus an optional
/// [`WaterFlow`] and an optional normal-texture path the cell loader
/// should attempt to bind.
///
/// `xcwt_form == None` (no WATR reference on the cell) falls back to
/// engine defaults — same shape Skyrim uses for unmodded cells that
/// rely on the worldspace water-default cascade.
pub(crate) fn resolve_water_material(
    waters: &HashMap<u32, esm::records::misc::WatrRecord>,
    xcwt_form: Option<u32>,
) -> (
    WaterMaterial,
    WaterKind,
    Option<WaterFlow>,
    Option<String>,
    [Option<String>; 3],
) {
    let mut mat = WaterMaterial::default();
    let mut kind = WaterKind::Calm;
    let mut flow: Option<WaterFlow> = None;
    let mut normal_path: Option<String> = None;
    let mut noise_paths: [Option<String>; 3] = [None, None, None];

    if let Some(form) = xcwt_form {
        if let Some(rec) = waters.get(&form) {
            mat.shallow_color = rec.params.shallow_color;
            mat.deep_color = rec.params.deep_color;
            mat.underwater_color = rec.params.underwater_color;
            mat.fog_near = rec.params.fog_near;
            mat.fog_far = rec.params.fog_far;
            mat.underwater_fog_near = rec.params.underwater_fog_near;
            mat.underwater_fog_far = rec.params.underwater_fog_far;
            mat.underwater_fog_amount = rec.params.underwater_fog_amount.clamp(0.0, 8.0);
            // GNAM's third related-water link is the authored underwater
            // variant. Resolve only one hop and reject self-links so malformed
            // mod chains cannot recurse or replace the parent surface optics.
            if let Some(underwater_form) = rec
                .related_waters
                .get(2)
                .copied()
                .filter(|form| *form != 0 && *form != rec.form_id)
            {
                if let Some(underwater) = waters.get(&underwater_form) {
                    mat.underwater_color = underwater.params.underwater_color;
                    mat.underwater_fog_near = underwater.params.underwater_fog_near;
                    mat.underwater_fog_far = underwater.params.underwater_fog_far;
                    mat.underwater_fog_amount =
                        underwater.params.underwater_fog_amount.clamp(0.0, 8.0);
                }
            }
            // FO4/FO76 authored suspended silt is a water-column color
            // contribution, not a separate shader branch. Blend it once at
            // translation so every consumer (surface, refraction, and
            // underwater presentation) sees the same resolved palette.
            if rec.params.silt_amount.is_finite() && rec.params.silt_amount > 0.0 {
                let silt = rec.params.silt_amount.clamp(0.0, 1.0);
                for (dst, src) in mat
                    .shallow_color
                    .iter_mut()
                    .zip(rec.params.silt_light_color)
                {
                    *dst = (*dst * (1.0 - silt * 0.25) + src * (silt * 0.25)).clamp(0.0, 1.0);
                }
                for (dst, src) in mat
                    .deep_color
                    .iter_mut()
                    .zip(rec.params.silt_dark_color)
                {
                    *dst = (*dst * (1.0 - silt) + src * silt).clamp(0.0, 1.0);
                }
                for (dst, src) in mat
                    .underwater_color
                    .iter_mut()
                    .zip(rec.params.silt_dark_color)
                {
                    *dst = (*dst * (1.0 - silt) + src * silt).clamp(0.0, 1.0);
                }
            }
            // GNAM slots 0/1 are the authored daytime/nighttime surface
            // variants. Keep them as compact material-side palettes so the
            // renderer can blend them from the live climate clock without
            // growing the per-draw GPU ABI. Missing or malformed links fall
            // back to the fully translated parent palette.
            mat.day_shallow_color = mat.shallow_color;
            mat.day_deep_color = mat.deep_color;
            mat.day_fog_near = mat.fog_near;
            mat.day_fog_far = mat.fog_far;
            mat.day_reflection_tint = rec.params.reflection_color;
            mat.night_shallow_color = mat.shallow_color;
            mat.night_deep_color = mat.deep_color;
            mat.night_fog_near = mat.fog_near;
            mat.night_fog_far = mat.fog_far;
            mat.night_reflection_tint = rec.params.reflection_color;
            let variant_shallow = |base: [f32; 3], light: [f32; 3], amount: f32| {
                if amount.is_finite() && amount > 0.0 {
                    let silt = amount.clamp(0.0, 1.0);
                    std::array::from_fn(|i| {
                        (base[i] * (1.0 - silt * 0.25) + light[i] * (silt * 0.25)).clamp(0.0, 1.0)
                    })
                } else {
                    base
                }
            };
            let variant_deep = |base: [f32; 3], dark: [f32; 3], amount: f32| {
                if amount.is_finite() && amount > 0.0 {
                    let silt = amount.clamp(0.0, 1.0);
                    std::array::from_fn(|i| {
                        (base[i] * (1.0 - silt) + dark[i] * silt).clamp(0.0, 1.0)
                    })
                } else {
                    base
                }
            };
            for (slot, target) in [
                (
                    0usize,
                    (
                        &mut mat.day_shallow_color,
                        &mut mat.day_deep_color,
                        &mut mat.day_fog_near,
                        &mut mat.day_fog_far,
                        &mut mat.day_reflection_tint,
                    ),
                ),
                (
                    1usize,
                    (
                        &mut mat.night_shallow_color,
                        &mut mat.night_deep_color,
                        &mut mat.night_fog_near,
                        &mut mat.night_fog_far,
                        &mut mat.night_reflection_tint,
                    ),
                ),
            ] {
                let Some(form) = rec
                    .related_waters
                    .get(slot)
                    .copied()
                    .filter(|form| *form != 0 && *form != rec.form_id)
                else {
                    continue;
                };
                let Some(variant) = waters.get(&form) else {
                    continue;
                };
                *target.0 = variant_shallow(
                    variant.params.shallow_color,
                    variant.params.silt_light_color,
                    variant.params.silt_amount,
                );
                *target.1 = variant_deep(
                    variant.params.deep_color,
                    variant.params.silt_dark_color,
                    variant.params.silt_amount,
                );
                *target.2 = variant.params.fog_near;
                *target.3 = variant.params.fog_far;
                *target.4 = variant.params.reflection_color;
            }
            if rec.opacity.is_finite() && rec.opacity > 0.0 {
                mat.opacity = rec.opacity.clamp(0.0, 1.0);
            } else if rec.opacity_authored && rec.opacity.is_finite() {
                mat.opacity = rec.opacity.clamp(0.0, 1.0);
            }
            mat.fresnel_f0 = rec.params.fresnel.clamp(0.001, 0.20);
            mat.reflectivity = rec.params.reflectivity;
            // FO3/FNV's legacy FNAM bit 0x02 is an explicit reflective
            // surface gate. Preserve authored reflectivity when the flag is
            // present; when it is explicitly absent, suppress the RT
            // reflection contribution instead of making non-reflective
            // sludge/puddle records mirror-bright. Records from newer games
            // leave `legacy_flags` unset and retain their authored scalar.
            if rec.legacy_flags.is_some_and(|flags| flags & 0x02 == 0) {
                mat.reflectivity = 0.0;
            }
            mat.reflection_tint = rec.params.reflection_color;
            // WATAL Phase 1: carry the wave fields that were previously
            // parsed into WaterParams but dropped here. AUTHORED for all
            // eras (Oblivion/FO3/FNV/Skyrim DATA all encode amp+freq);
            // see docs/engine/watal.md §4.
            mat.wave_amplitude = rec.params.wave_amplitude;
            mat.wave_frequency = rec.params.wave_frequency;
            mat.sun_specular_power = rec.params.sun_specular_power;
            // Starfield replaces the legacy sun-power scalar with a physical
            // surface roughness. Convert the authored roughness to the
            // Blinn exponent consumed by the shared direct-sun path; older
            // records retain their authored/default exponent unchanged.
            if rec.params.roughness.is_finite() && rec.params.roughness > 0.0 {
                let roughness = rec.params.roughness.clamp(0.02, 1.0);
                mat.sun_specular_power = (2.0 / (roughness * roughness) - 2.0)
                    .clamp(1.0, 2048.0);
            }
            // FO3/FNV long DATA records carry independent authored tiling
            // controls for the first two noise layers. Keep the canonical
            // defaults for short/Oblivion records, and clamp only finite
            // positive values so malformed tails cannot create shimmering
            // sub-texel bands or near-uniform surfaces.
            if rec.params.noise_uv_scale_a.is_finite() && rec.params.noise_uv_scale_a > 0.0 {
                mat.uv_scale_a = rec.params.noise_uv_scale_a.clamp(1.0 / 4096.0, 1.0 / 8.0);
            }
            if rec.params.noise_uv_scale_b.is_finite() && rec.params.noise_uv_scale_b > 0.0 {
                mat.uv_scale_b = rec.params.noise_uv_scale_b.clamp(1.0 / 4096.0, 1.0 / 8.0);
            }
            if rec.params.noise_uv_scale_c.is_finite() && rec.params.noise_uv_scale_c > 0.0 {
                mat.uv_scale_c = rec.params.noise_uv_scale_c.clamp(1.0 / 4096.0, 1.0 / 8.0);
            }
            for (dst, src) in mat
                .noise_amplitude_scales
                .iter_mut()
                .zip(rec.params.noise_amplitude_scales)
            {
                if src.is_finite() && src > 0.0 {
                    *dst = src.clamp(0.05, 4.0);
                }
            }
            // Skyrim/FO4 author an additional physical normal magnitude
            // alongside the per-layer amplitudes. Fold it into the
            // canonical amplitudes so the compact GPU ABI stays unchanged.
            // A zero/invalid value is treated as the neutral legacy fallback.
            let normal_magnitude =
                if rec.params.normal_magnitude.is_finite() && rec.params.normal_magnitude > 0.0 {
                    rec.params.normal_magnitude.clamp(0.01, 8.0)
                } else {
                    1.0
                };
            for amplitude in &mut mat.noise_amplitude_scales {
                *amplitude = (*amplitude * normal_magnitude).clamp(0.0, 8.0);
            }
            mat.above_water_fog_amount = if rec.params.above_water_fog_amount.is_finite() {
                rec.params.above_water_fog_amount.clamp(0.0, 8.0)
            } else {
                1.0
            };
            for (dst, src) in mat.depth_weights.iter_mut().zip(rec.params.depth_weights) {
                if src.is_finite() && src > 0.0 {
                    *dst = src.clamp(0.0, 4.0);
                }
            }
            // The compact shader already uses depth.y as the Beer-Lambert
            // refraction weight. Fold Skyrim's separate above-water fog
            // amount into that slot instead of growing the UBO ABI.
            mat.depth_weights[1] =
                (mat.depth_weights[1] * mat.above_water_fog_amount).clamp(0.0, 8.0);
            for (index, (dst, src)) in mat
                .effect_controls
                .iter_mut()
                .zip(rec.params.effect_controls)
                .enumerate()
            {
                if src.is_finite() && src > 0.0 {
                    *dst = if index == 1 {
                        src.clamp(1.0, 2048.0)
                    } else {
                        src.clamp(0.0, 8.0)
                    };
                }
            }
            if rec.params.flowmap_scale.is_finite() && rec.params.flowmap_scale > 0.0 {
                mat.flowmap_scale = rec.params.flowmap_scale.clamp(0.05, 8.0);
            }
            for (dst, src) in mat
                .absorption_ranges
                .iter_mut()
                .zip(rec.params.absorption_ranges)
            {
                if src.is_finite() && src > 0.0 {
                    *dst = src.clamp(0.01, 100_000.0);
                }
            }
            if rec.params.specular_magnitude.is_finite() && rec.params.specular_magnitude > 0.0 {
                mat.specular_magnitude = rec.params.specular_magnitude.clamp(0.0, 8.0);
            }
            let authored_wind = rec.params.noise_wind_speeds;
            let authored_dirs = rec.params.noise_wind_directions;
            let authored_layer_scroll = |layer: usize| {
                let speed = authored_wind[layer];
                if speed.is_finite() && speed > 0.0 && authored_dirs[layer].is_finite() {
                    let (sin_theta, cos_theta) = authored_dirs[layer].sin_cos();
                    [cos_theta * speed, sin_theta * speed]
                } else {
                    [0.0, 0.0]
                }
            };
            mat.source_form = rec.form_id;

            // ── WaterKind heuristic from EDID naming convention ──
            //
            // Cell-level water planes are **always horizontal**
            // (XCLW provides a Y height; the mesh is a flat quad).
            // The `Waterfall` kind in the shader is for vertical
            // sheet geometry (cliff-side falling water), which the
            // cell loader does NOT spawn — those land as standalone
            // mesh refs through the regular NIF import path. So
            // any EDID match that would otherwise promote a cell
            // plane to `Waterfall` is demoted to `River` here: the
            // horizontal plane below a waterfall is a fast,
            // turbulent pool, not a falling sheet, and the River
            // shader path is the correct visual.
            //
            // Skyrim has many WATR records whose names contain
            // "fall"/"waterfall" but are applied to horizontal
            // bodies of water (e.g. `DLC2WaterFallingStream`,
            // `WaterFallingPool`, `WaterRiverFallingSlow`). The
            // pre-fix heuristic mis-classified these and the
            // shader's Waterfall mode painted heavy fizz foam
            // across whole exterior cells — see the May 2026
            // smoke-test screenshot reported alongside this
            // change.
            let lowered = rec.editor_id.to_ascii_lowercase();
            let has_authored_linear_flow = rec.linear_velocity.is_some();
            let authored_flow_speed = rec
                .linear_velocity
                .map(|velocity| velocity[0].hypot(velocity[1]))
                .unwrap_or(0.0);
            if lowered.contains("rapid")
                || (has_authored_linear_flow
                    && authored_flow_speed >= WaterFlow::SPEED_RAPIDS)
            {
                kind = WaterKind::Rapids;
                mat.foam_strength = 0.85;
            } else if lowered.contains("waterfall")
                || lowered.contains("falls")
                || lowered.contains("river")
                || lowered.contains("stream")
                // Skyrim SE's NAM5 is explicitly the flow-normal texture.
                // Treat its presence as an authored flow signal even when
                // the EDID is localized or uses a neutral name.
                || !rec.flow_noise_texture_path.is_empty()
                // NAM0 is stronger evidence than naming: it is the record's
                // explicit linear current and must classify a neutral or
                // localized EDID as flowing water.
                || has_authored_linear_flow
            {
                kind = WaterKind::River;
                mat.foam_strength = 0.20;
            }
            // Synthesise a flow vector from WATR's wind direction + the
            // canonical per-`WaterKind` current speed when the kind
            // implies flow. Bethesda's wind_direction is in radians from
            // north (UESP).
            //
            // #2872 — a WATR normal-layer speed is visual UV motion, not a
            // physics current. `WaterFlow::for_kind` supplies a bounded
            // gameplay current for named/flagged flowing bodies, while an
            // explicit NAM0 linear velocity remains authoritative when the
            // record provides one. The shader scroll below is derived from
            // that canonical flow and then adds authored normal-layer motion;
            // one scalar never serves two incompatible unit systems.
            if !matches!(kind, WaterKind::Calm) {
                let theta = rec.params.wind_direction;
                // Compute once — cos/sin were duplicated pre-#1068 (F-WAT-06).
                let (sin_theta, cos_theta) = theta.sin_cos();
                let canonical = rec
                    .linear_velocity
                    .map(|velocity| {
                        let magnitude = velocity[0].hypot(velocity[1]);
                        let direction = if magnitude.is_finite() && magnitude > 1.0e-5 {
                            [velocity[0] / magnitude, 0.0, velocity[1] / magnitude]
                        } else {
                            [cos_theta, 0.0, sin_theta]
                        };
                        WaterFlow::new(
                            direction,
                            magnitude,
                        )
                    })
                    .unwrap_or_else(|| WaterFlow::for_kind(kind, [cos_theta, 0.0, sin_theta]));
                // Rebuild scroll vectors to bias along the flow axis,
                // converting out of BU/s at the documented rate. The
                // authored flow-map scale is intentionally applied once at
                // the render upload boundary; keeping it out of this
                // canonical material vector prevents a double scale on every
                // flowing surface. Preserve authored per-layer motion as an
                // additional component; otherwise Skyrim/FO4 velocity tails
                // silently disappear whenever a water record is classified
                // as flowing.
                let scroll = canonical.speed * WATER_SCROLL_UV_PER_BU_PER_S;
                let authored_a = authored_layer_scroll(0);
                let authored_b = authored_layer_scroll(1);
                let authored_c = authored_layer_scroll(2);
                let flow_x = canonical.direction[0];
                let flow_z = canonical.direction[2];
                mat.scroll_a = [
                    flow_x * scroll + authored_a[0],
                    flow_z * scroll + authored_a[1],
                ];
                // Perpendicular shear at half rate for the second layer.
                mat.scroll_b = [
                    -flow_z * scroll * 0.5 + authored_b[0],
                    flow_x * scroll * 0.5 + authored_b[1],
                ];
                mat.scroll_c = if authored_c != [0.0, 0.0] {
                    authored_c
                } else {
                    mat.scroll_a
                };
                flow = Some(canonical);
            }
            // Calm bodies have no canonical current. Preserve authored
            // per-layer normal motion (Skyrim/FO4) when present; this is a
            // visual UV velocity only and must not replace the shared
            // weather wind later used by SpeedTree vegetation.
            if matches!(kind, WaterKind::Calm) {
                let authored_a = authored_layer_scroll(0);
                let authored_b = authored_layer_scroll(1);
                let authored_c = authored_layer_scroll(2);
                if authored_a != [0.0, 0.0] {
                    mat.scroll_a = authored_a;
                }
                if authored_b != [0.0, 0.0] {
                    mat.scroll_b = authored_b;
                }
                if authored_c != [0.0, 0.0] {
                    mat.scroll_c = authored_c;
                }
            }
            // TNAM is the diffuse / noise texture — used as the
            // bindless normal map for the shader. Empty path =
            // procedural fallback.
            if !rec.texture_path.is_empty() {
                normal_path = Some(rec.texture_path.clone());
            }
            for (dst, path) in noise_paths.iter_mut().zip(&rec.noise_texture_paths) {
                if !path.is_empty() {
                    *dst = Some(path.clone());
                }
            }
            // Skyrim SE's NAM5 is a flow-normal texture. Preserve the
            // compact three-layer GPU ABI by promoting it over NAM4 only
            // for flowing bodies; calm water keeps its authored NAM4 layer.
            if !matches!(kind, WaterKind::Calm) && !rec.flow_noise_texture_path.is_empty() {
                noise_paths[2] = Some(rec.flow_noise_texture_path.clone());
            }
        }
    }

    // SubmersionState is per-actor, not per-plane — but seed a
    // sentinel value on the material itself so debug overlays can
    // see "water without a parsed XCWT" cells.
    let _ = SubmersionState::default();

    (mat, kind, flow, normal_path, noise_paths)
}

// ───────────────────────────────────────────────────────────────────────
// Exterior sky / sun / weather / lighting translation (EXAL step 3)
//
// The WTHR-driven canonical resources (`CellLightingRes`, `SkyParamsRes`,
// `WeatherDataRes`) and the no-climate procedural fallback are built here,
// behind the single boundary. The functions are **pure** — the caller
// (`scene::world_setup::apply_worldspace_weather`) pre-resolves the
// `VulkanContext`-coupled cloud / sun textures into a [`SkyTextures`] and
// hands them in, mirroring `material_translate`'s `ResolvedPaths`. World
// insertion, the bindless-handle lifecycle, and the WTHR cross-fade-vs-insert
// decision stay in the caller (orchestration, not translation).
// ───────────────────────────────────────────────────────────────────────

/// Cos-threshold of the rendered sun-disc half-angle (~1.8°). Matches the
/// pre-EXAL hardcoded `SkyParamsRes` literal.
const SUN_SIZE_COS: f32 = 0.9995;
/// Directional-light intensity at full day. The per-frame `weather_system`
/// re-derives the live value from the TOD arc; this is the bootstrap seed.
const SUN_INTENSITY: f32 = 4.0;
/// Tangent-plane half-radius of the directional disk in radians (~1.15°);
/// drives PCSS-lite shadow jitter (#1023). Pre-EXAL hardcoded constant.
const SUN_ANGULAR_RADIUS: f32 = 0.020;

/// Pre-resolved cloud + sun-sprite bindless handles for [`translate_sky`].
/// The caller resolves these through the texture registry (the only
/// `VulkanContext`-coupled step); the translate stays pure.
pub(crate) struct SkyTextures {
    /// `(bindless handle, tile_scale)` per WTHR cloud layer 0..=3.
    /// `(0, 0.0)` = layer disabled (the shader branch-skips it).
    pub(crate) cloud_layers: [(u32, f32); 4],
    /// CLMT FNAM sun-sprite handle. `0` = composite shader's procedural disc.
    pub(crate) sun_sprite: u32,
}

/// WTHR → exterior [`CellLightingRes`] (the day-TOD-slot snapshot the
/// per-frame `weather_system` then animates through the stored NAM0 table).
/// Raw monitor-space colours (commit 0e8efc6) — no sRGB decode.
pub(crate) fn translate_exterior_cell_lighting(
    wthr: &WeatherRecord,
    sun_dir: [f32; 3],
) -> CellLightingRes {
    use byroredux_plugin::esm::records::weather::{SKY_AMBIENT, SKY_FOG, SKY_SUNLIGHT, TOD_DAY};
    CellLightingRes {
        ambient: wthr.sky_colors[SKY_AMBIENT][TOD_DAY].to_rgb_f32(),
        directional_color: wthr.sky_colors[SKY_SUNLIGHT][TOD_DAY].to_rgb_f32(),
        directional_dir: sun_dir,
        is_interior: false,
        fog_color: wthr.sky_colors[SKY_FOG][TOD_DAY].to_rgb_f32(),
        fog_near: wthr.fog_day_near,
        fog_far: wthr.fog_day_far,
        fog_medium: crate::fog::FogMedium::from_legacy_ramp(
            wthr.fog_day_near,
            wthr.fog_day_far,
            Some(wthr.fog_day_max),
        ),
        // WTHR-driven exterior lighting; the extended XCLL tail applies to
        // interior cells (and not-yet-wired exterior lighting overrides). #861.
        directional_fade: None,
        fog_clip: None,
        fog_power: None,
        fog_far_color: None,
        fog_max: None,
        light_fade_begin: None,
        light_fade_end: None,
        directional_ambient: None,
        specular_color: None,
        specular_alpha: None,
        fresnel_power: None,
        inheritance_flags: None,
    }
}

/// WTHR + pre-resolved [`SkyTextures`] → [`SkyParamsRes`]. `current_dalc_cube`
/// is seeded `None`; `weather_system` populates it per-frame from
/// [`WeatherDataRes::skyrim_dalc_per_tod`] (#993).
pub(crate) fn translate_sky(
    wthr: &WeatherRecord,
    sun_dir: [f32; 3],
    textures: SkyTextures,
) -> SkyParamsRes {
    use byroredux_plugin::esm::records::weather::{
        SKY_HORIZON, SKY_LOWER, SKY_SUN, SKY_UPPER, TOD_DAY,
    };
    let [(c0, s0), (c1, s1), (c2, s2), (c3, s3)] = textures.cloud_layers;
    SkyParamsRes {
        zenith_color: wthr.sky_colors[SKY_UPPER][TOD_DAY].to_rgb_f32(),
        horizon_color: wthr.sky_colors[SKY_HORIZON][TOD_DAY].to_rgb_f32(),
        // #541 — real Sky-Lower (NAM0 slot 7) drives composite.frag's
        // below-horizon branch instead of the pre-fix `horizon * 0.3` fake.
        lower_color: wthr.sky_colors[SKY_LOWER][TOD_DAY].to_rgb_f32(),
        sun_direction: sun_dir,
        sun_color: wthr.sky_colors[SKY_SUN][TOD_DAY].to_rgb_f32(),
        sun_size: SUN_SIZE_COS,
        sun_intensity: SUN_INTENSITY,
        sun_angular_radius: SUN_ANGULAR_RADIUS,
        is_exterior: true,
        cloud_tile_scale: s0,
        cloud_texture_index: c0,
        sun_texture_index: textures.sun_sprite,
        cloud_tile_scale_1: s1,
        cloud_texture_index_1: c1,
        cloud_tile_scale_2: s2,
        cloud_texture_index_2: c2,
        cloud_tile_scale_3: s3,
        cloud_texture_index_3: c3,
        current_dalc_cube: None,
    }
}

/// Convert Bethesda WTHR classification flags into the procedural medium's
/// occupancy control. Precipitation takes priority when records combine flags;
/// unclassified legacy weather retains the historical neutral coverage.
fn fog_coverage_from_weather(classification: u8) -> f32 {
    use byroredux_plugin::esm::records::weather::{
        WTHR_CLOUDY, WTHR_PLEASANT, WTHR_RAINY, WTHR_SNOW,
    };
    if classification & WTHR_RAINY != 0 {
        0.86
    } else if classification & WTHR_SNOW != 0 {
        0.80
    } else if classification & WTHR_CLOUDY != 0 {
        0.70
    } else if classification & WTHR_PLEASANT != 0 {
        0.40
    } else {
        0.55
    }
}

fn precipitation_from_weather(classification: u8) -> f32 {
    use byroredux_plugin::esm::records::weather::{WTHR_RAINY, WTHR_SNOW};
    if classification & WTHR_RAINY != 0 {
        1.0
    } else if classification & WTHR_SNOW != 0 {
        0.12
    } else {
        0.0
    }
}

/// Per-climate sunrise/sunset breakpoints in hours. CLMT TNAM bytes
/// are in 10-min units (`hour = byte / 6`); the valid authored range is
/// `1..=144` (`1` = 0:10, `144` = 24:00). Returns the pre-#463 hardcoded
/// `[6.0, 10.0, 18.0, 22.0]` fallback when:
///   * the worldspace has no climate (stub or unresolved record),
///   * the CLMT TNAM is all-zero (a stub field, not authored data),
///   * any of the four bytes lies outside `1..=144` — corruption guard
///     for modded ESMs that ship out-of-range bytes (e.g.
///     `[0, 0, 0, 0xFF]` would otherwise pass the pre-#530 OR-of-bytes
///     filter and produce a sunset_end of 42.5h, breaking the TOD
///     interpolator). See #530 / FNV-CELL-8.
pub(crate) fn climate_tod_hours(
    climate: Option<&byroredux_plugin::esm::records::ClimateRecord>,
) -> [f32; 4] {
    const FALLBACK: [f32; 4] = [6.0, 10.0, 18.0, 22.0];
    let Some(c) = climate else {
        return FALLBACK;
    };
    let valid = |b: u8| (1..=144).contains(&b);
    if valid(c.sunrise_begin)
        && valid(c.sunrise_end)
        && valid(c.sunset_begin)
        && valid(c.sunset_end)
    {
        [
            c.sunrise_begin as f32 / 6.0,
            c.sunrise_end as f32 / 6.0,
            c.sunset_begin as f32 / 6.0,
            c.sunset_end as f32 / 6.0,
        ]
    } else {
        FALLBACK
    }
}

/// WTHR (+ climate for TOD breakpoints) → [`WeatherDataRes`], the full NAM0
/// table the per-frame interpolator walks. `skyrim_dalc_per_tod` is `Some`
/// only for Skyrim WTHR (converted Z-up → Y-up once here); `None` elsewhere.
pub(crate) fn translate_weather(
    wthr: &WeatherRecord,
    climate: Option<&ClimateRecord>,
) -> WeatherDataRes {
    use byroredux_plugin::esm::records::weather::{SKY_COLOR_GROUPS, SKY_TIME_SLOTS};
    let mut sky_colors = [[[0.0f32; 3]; SKY_TIME_SLOTS]; SKY_COLOR_GROUPS];
    for (dst_group, src_group) in sky_colors.iter_mut().zip(wthr.sky_colors.iter()) {
        for (dst, src) in dst_group.iter_mut().zip(src_group.iter()) {
            *dst = src.to_rgb_f32();
        }
    }
    let skyrim_dalc_per_tod = wthr.skyrim_ambient_cube.as_ref().map(|cubes| {
        [
            DalcCubeYup::from_skyrim_zup(&cubes[0]),
            DalcCubeYup::from_skyrim_zup(&cubes[1]),
            DalcCubeYup::from_skyrim_zup(&cubes[2]),
            DalcCubeYup::from_skyrim_zup(&cubes[3]),
        ]
    });
    let coverage = fog_coverage_from_weather(wthr.classification);
    let mut fog_media = [
        crate::fog::FogMedium::from_legacy_ramp(
            wthr.fog_day_near,
            wthr.fog_day_far,
            Some(wthr.fog_day_max),
        ),
        crate::fog::FogMedium::from_legacy_ramp(
            wthr.fog_night_near,
            wthr.fog_night_far,
            Some(wthr.fog_night_max),
        ),
    ];
    for medium in &mut fog_media {
        medium.coverage = coverage;
    }

    WeatherDataRes {
        sky_colors,
        fog: [
            wthr.fog_day_near,
            wthr.fog_day_far,
            wthr.fog_night_near,
            wthr.fog_night_far,
        ],
        fog_media,
        // #463 — per-climate sunrise/sunset breakpoints (validated helper).
        tod_hours: climate_tod_hours(climate),
        skyrim_dalc_per_tod,
        // #1033 — WTHR DATA wind_speed drives per-weather cloud-scroll rate.
        wind_speed: wthr.wind_speed,
        precipitation: precipitation_from_weather(wthr.classification),
    }
}

// ── Procedural fallback (no resolved climate / weather) ──
//
// Warm Mojave-style desert sky. Same values the bulk loader used pre-#M40;
// kept here as the canonical no-data default rather than an inline block in
// the render-setup path (EXAL §3: the fallback is an explicit canonical
// constructor, not a render-time heuristic).
const FB_AMBIENT: [f32; 3] = [0.15, 0.14, 0.12];
const FB_SUNLIGHT: [f32; 3] = [1.0, 0.95, 0.8];
const FB_FOG_COLOR: [f32; 3] = [0.65, 0.7, 0.8];
const FB_ZENITH: [f32; 3] = [0.15, 0.3, 0.65];
const FB_HORIZON: [f32; 3] = [0.55, 0.5, 0.42];
// Pre-#541 the `compute_sky` below-horizon branch faked the ground tint as
// `horizon * 0.3`; matching that keeps the procedural look unchanged.
const FB_LOWER: [f32; 3] = [
    FB_HORIZON[0] * 0.3,
    FB_HORIZON[1] * 0.3,
    FB_HORIZON[2] * 0.3,
];
const FB_SUN_COLOR: [f32; 3] = [1.0, 0.95, 0.8];
const FB_FOG_NEAR: f32 = 15000.0;
const FB_FOG_FAR: f32 = 80000.0;

/// Procedural-fallback exterior lighting (no plugin data → engine defaults).
pub(crate) fn procedural_fallback_cell_lighting(sun_dir: [f32; 3]) -> CellLightingRes {
    CellLightingRes {
        ambient: FB_AMBIENT,
        directional_color: FB_SUNLIGHT,
        directional_dir: sun_dir,
        is_interior: false,
        fog_color: FB_FOG_COLOR,
        fog_near: FB_FOG_NEAR,
        fog_far: FB_FOG_FAR,
        fog_medium: crate::fog::FogMedium::from_legacy_ramp(FB_FOG_NEAR, FB_FOG_FAR, None),
        directional_fade: None,
        fog_clip: None,
        fog_power: None,
        fog_far_color: None,
        fog_max: None,
        light_fade_begin: None,
        light_fade_end: None,
        directional_ambient: None,
        specular_color: None,
        specular_alpha: None,
        fresnel_power: None,
        inheritance_flags: None,
    }
}

/// Procedural-fallback sky (no clouds, procedural sun disc).
pub(crate) fn procedural_fallback_sky(sun_dir: [f32; 3]) -> SkyParamsRes {
    SkyParamsRes {
        zenith_color: FB_ZENITH,
        horizon_color: FB_HORIZON,
        lower_color: FB_LOWER,
        sun_direction: sun_dir,
        sun_color: FB_SUN_COLOR,
        sun_size: SUN_SIZE_COS,
        sun_intensity: SUN_INTENSITY,
        sun_angular_radius: SUN_ANGULAR_RADIUS,
        is_exterior: true,
        cloud_tile_scale: 0.0,
        cloud_texture_index: 0,
        sun_texture_index: 0,
        cloud_tile_scale_1: 0.0,
        cloud_texture_index_1: 0,
        cloud_tile_scale_2: 0.0,
        cloud_texture_index_2: 0,
        cloud_tile_scale_3: 0.0,
        cloud_texture_index_3: 0,
        current_dalc_cube: None,
    }
}

/// Synthetic [`WeatherDataRes`] for the fallback: every TOD slot of the six
/// groups `weather_system` reads carries the same procedural colour, so the
/// TOD lerp re-writes the same values each frame while still advancing
/// `sun_direction` / `sun_intensity`. See #542 / M33-10.
pub(crate) fn procedural_fallback_weather() -> WeatherDataRes {
    use byroredux_plugin::esm::records::weather::{
        SKY_AMBIENT, SKY_COLOR_GROUPS, SKY_FOG, SKY_HORIZON, SKY_LOWER, SKY_SUN, SKY_SUNLIGHT,
        SKY_TIME_SLOTS, SKY_UPPER,
    };
    let mut sky_colors = [[[0.0f32; 3]; SKY_TIME_SLOTS]; SKY_COLOR_GROUPS];
    let synthetic = [
        (SKY_UPPER, FB_ZENITH),
        (SKY_FOG, FB_FOG_COLOR),
        (SKY_AMBIENT, FB_AMBIENT),
        (SKY_SUNLIGHT, FB_SUNLIGHT),
        (SKY_SUN, FB_SUN_COLOR),
        // #541 — `weather_system` also reads SKY_LOWER for the below-horizon
        // branch; synthetic value matches the procedural `FB_LOWER`.
        (SKY_LOWER, FB_LOWER),
        (SKY_HORIZON, FB_HORIZON),
    ];
    for (group, color) in synthetic {
        sky_colors[group].fill(color);
    }
    WeatherDataRes {
        sky_colors,
        // Day/night fog distances kept identical — no authored night distance.
        fog: [FB_FOG_NEAR, FB_FOG_FAR, FB_FOG_NEAR, FB_FOG_FAR],
        fog_media: [
            crate::fog::FogMedium::from_legacy_ramp(FB_FOG_NEAR, FB_FOG_FAR, None),
            crate::fog::FogMedium::from_legacy_ramp(FB_FOG_NEAR, FB_FOG_FAR, None),
        ],
        // Pre-#463 hardcoded TOD breakpoints.
        tod_hours: [6.0, 10.0, 18.0, 22.0],
        skyrim_dalc_per_tod: None,
        wind_speed: 0,
        precipitation: 0.0,
    }
}

/// Regression tests for [`climate_tod_hours`] — #530 / FNV-CELL-8. Lives
/// beside the function under the EXAL boundary (#2453).
#[cfg(test)]
mod climate_tod_hours_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::misc::{WaterParams, WatrRecord};

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

    // ── PNAM parent inheritance (#2735) ──────────────────────────

    /// Two-worldspace fixture: child `"c"` under parent `"p"`, with the
    /// child's PNAM bits set to `child_flags`.
    fn parent_child(
        parent: WorldspaceRecord,
        child_flags: u16,
        child: WorldspaceRecord,
    ) -> HashMap<String, WorldspaceRecord> {
        let parent_fid = 0x0000_0100;
        HashMap::from([
            (
                "p".to_string(),
                WorldspaceRecord {
                    form_id: parent_fid,
                    ..parent
                },
            ),
            (
                "c".to_string(),
                WorldspaceRecord {
                    form_id: 0x0000_0200,
                    parent_worldspace: Some(parent_fid),
                    parent_flags: child_flags,
                    ..child
                },
            ),
        ])
    }

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
    fn oblivion_is_unaffected_by_inheritance() {
        // Oblivion authors no PNAM at all, so the walk is inert and the
        // sea-level path is byte-for-byte its pre-#2735 behaviour.
        let map = parent_child(
            WorldspaceRecord {
                water_form: Some(0x0000_00AB),
                ..Default::default()
            },
            0,
            WorldspaceRecord {
                water_form: Some(0x0000_00CD),
                ..Default::default()
            },
        );
        assert_eq!(
            default_water_for_worldspace(&map, "c", GameKind::Oblivion),
            (Some(0.0), Some(0x0000_00CD)),
        );
    }

    // ── resolve_cell_climate (#2451 / EXAL-03) ────────────────────

    /// The parsed CLMT set the override has to resolve against.
    fn climate_records(form_ids: &[u32]) -> HashMap<u32, ClimateRecord> {
        form_ids
            .iter()
            .map(|&form_id| {
                (
                    form_id,
                    ClimateRecord {
                        form_id,
                        editor_id: format!("CLMT{form_id:08X}"),
                        ..Default::default()
                    },
                )
            })
            .collect()
    }

    /// The finding itself: a cell that authors XCCM must resolve to the
    /// override, not to the worldspace default. Pre-#2451
    /// `CellData::climate_override` was parsed on both CELL walk paths
    /// and read by nothing, so every such cell rendered the worldspace
    /// sky.
    #[test]
    fn cell_xccm_override_wins_over_the_worldspace_climate() {
        let climates = climate_records(&[0x0001_A2B3, 0x0000_1234]);
        assert_eq!(
            resolve_cell_climate(Some(0x0001_A2B3), Some(0x0000_1234), &climates),
            Some(0x0001_A2B3),
        );
    }

    /// No XCCM — inherit the worldspace climate unchanged. This is the
    /// overwhelmingly common case (vanilla content authors XCCM on a
    /// handful of cells), so it must stay a pure pass-through.
    #[test]
    fn cell_without_xccm_inherits_the_worldspace_climate() {
        let climates = climate_records(&[0x0000_1234]);
        assert_eq!(
            resolve_cell_climate(None, Some(0x0000_1234), &climates),
            Some(0x0000_1234),
        );
        // Climateless worldspace + no override stays climateless (the
        // caller's procedural-fallback sky), not fabricated.
        assert_eq!(resolve_cell_climate(None, None, &climates), None);
    }

    /// An override pointing at a CLMT the load order never supplied
    /// (missing master, stale mod edit) falls back to the worldspace
    /// climate rather than to `None` — dropping into the procedural
    /// fallback sky would be further from the author's intent than the
    /// worldspace's own weather.
    #[test]
    fn unresolvable_override_falls_back_to_the_worldspace_climate() {
        let climates = climate_records(&[0x0000_1234]);
        assert_eq!(
            resolve_cell_climate(Some(0xDEAD_BEEF), Some(0x0000_1234), &climates),
            Some(0x0000_1234),
        );
        assert_eq!(
            resolve_cell_climate(Some(0xDEAD_BEEF), None, &climates),
            None,
        );
    }

    // ── resolve_default_weather (#2451) ───────────────────────────

    /// Highest chance wins, negative chances are mod sentinels and are
    /// filtered out entirely (#476), and an entry whose WTHR the load
    /// order never supplied yields `None` rather than a lower-chance
    /// substitute — the caller keeps the sky it has instead.
    #[test]
    fn default_weather_picks_the_highest_resolvable_chance() {
        use byroredux_plugin::esm::records::climate::ClimateWeather;

        let weathers = HashMap::from([
            (
                0x0000_0011,
                WeatherRecord {
                    form_id: 0x0000_0011,
                    editor_id: "Clear".to_string(),
                    ..Default::default()
                },
            ),
            (
                0x0000_0022,
                WeatherRecord {
                    form_id: 0x0000_0022,
                    editor_id: "Storm".to_string(),
                    ..Default::default()
                },
            ),
        ]);
        let climate = ClimateRecord {
            weathers: vec![
                ClimateWeather {
                    weather_form_id: 0x0000_0011,
                    chance: 30,
                },
                ClimateWeather {
                    weather_form_id: 0x0000_0022,
                    chance: 70,
                },
                // Negative sentinel — must never win despite being last.
                ClimateWeather {
                    weather_form_id: 0x0000_0099,
                    chance: -1,
                },
            ],
            ..Default::default()
        };
        let (wthr, chance) = resolve_default_weather(&climate, &weathers).expect("resolves");
        assert_eq!(wthr.form_id, 0x0000_0022);
        assert_eq!(chance, 70);

        // Winner unresolvable → None, not the runner-up.
        let dangling = ClimateRecord {
            weathers: vec![
                ClimateWeather {
                    weather_form_id: 0x0000_0011,
                    chance: 30,
                },
                ClimateWeather {
                    weather_form_id: 0xDEAD_BEEF,
                    chance: 90,
                },
            ],
            ..Default::default()
        };
        assert!(resolve_default_weather(&dangling, &weathers).is_none());
    }

    // ── resolve_worldspace_climate ────────────────────────────────

    /// A worldspace with its own CNAM resolves to that climate directly —
    /// no parent chase needed even if a (bogus) parent chain exists.
    #[test]
    fn own_cnam_resolves_directly() {
        let worldspaces = HashMap::from([(
            "tamriel".to_string(),
            WorldspaceRecord {
                form_id: 0x0000_003C,
                editor_id: "Tamriel".to_string(),
                ..Default::default()
            },
        )]);
        let climates = HashMap::from([("tamriel".to_string(), 0x0000_1234)]);
        assert_eq!(
            resolve_worldspace_climate(&worldspaces, &climates, "tamriel"),
            Some(0x0000_1234)
        );
    }

    /// Regression for #2450 / EXAL-02: a childless-CNAM worldspace with a
    /// valid WNAM chain and the PNAM climate-inherit bit set must resolve
    /// to its parent's climate, not `None` (the pre-fix flat-lookup miss
    /// that fell through to the procedural fallback sky).
    #[test]
    fn child_with_no_cnam_inherits_parent_climate_when_flagged() {
        let worldspaces = HashMap::from([
            (
                "tamriel".to_string(),
                WorldspaceRecord {
                    form_id: 0x0000_003C,
                    editor_id: "Tamriel".to_string(),
                    ..Default::default()
                },
            ),
            (
                "solstheim".to_string(),
                WorldspaceRecord {
                    form_id: 0x0000_0042,
                    editor_id: "Solstheim".to_string(),
                    parent_worldspace: Some(0x0000_003C),
                    parent_flags: PNAM_INHERIT_CLIMATE,
                    ..Default::default()
                },
            ),
        ]);
        // Only the parent authors a CNAM.
        let climates = HashMap::from([("tamriel".to_string(), 0x0000_1234)]);
        assert_eq!(
            resolve_worldspace_climate(&worldspaces, &climates, "solstheim"),
            Some(0x0000_1234),
            "child must inherit the parent's climate via the WNAM chain"
        );
    }

    /// A childless-CNAM worldspace whose PNAM does NOT set the
    /// climate-inherit bit must NOT chase its parent — it's genuinely
    /// climate-less, not an inheritance gap.
    #[test]
    fn child_with_no_cnam_and_no_inherit_flag_stays_unresolved() {
        let worldspaces = HashMap::from([
            (
                "tamriel".to_string(),
                WorldspaceRecord {
                    form_id: 0x0000_003C,
                    editor_id: "Tamriel".to_string(),
                    ..Default::default()
                },
            ),
            (
                "child".to_string(),
                WorldspaceRecord {
                    form_id: 0x0000_0099,
                    editor_id: "Child".to_string(),
                    parent_worldspace: Some(0x0000_003C),
                    parent_flags: 0, // no PNAM bits set
                    ..Default::default()
                },
            ),
        ]);
        let climates = HashMap::from([("tamriel".to_string(), 0x0000_1234)]);
        assert_eq!(
            resolve_worldspace_climate(&worldspaces, &climates, "child"),
            None
        );
    }

    /// A multi-hop chain (grandchild → child → root) resolves all the way
    /// up when every intermediate level is flagged to inherit and none
    /// authors its own CNAM.
    #[test]
    fn multi_hop_parent_chain_resolves() {
        let worldspaces = HashMap::from([
            (
                "root".to_string(),
                WorldspaceRecord {
                    form_id: 1,
                    editor_id: "Root".to_string(),
                    ..Default::default()
                },
            ),
            (
                "mid".to_string(),
                WorldspaceRecord {
                    form_id: 2,
                    editor_id: "Mid".to_string(),
                    parent_worldspace: Some(1),
                    parent_flags: PNAM_INHERIT_CLIMATE,
                    ..Default::default()
                },
            ),
            (
                "leaf".to_string(),
                WorldspaceRecord {
                    form_id: 3,
                    editor_id: "Leaf".to_string(),
                    parent_worldspace: Some(2),
                    parent_flags: PNAM_INHERIT_CLIMATE,
                    ..Default::default()
                },
            ),
        ]);
        let climates = HashMap::from([("root".to_string(), 0xAAAA_AAAA)]);
        assert_eq!(
            resolve_worldspace_climate(&worldspaces, &climates, "leaf"),
            Some(0xAAAA_AAAA)
        );
    }

    /// A cyclic WNAM chain (corrupt/adversarial plugin data) must not
    /// infinite-loop — resolves to `None` instead.
    #[test]
    fn cyclic_parent_chain_terminates() {
        let worldspaces = HashMap::from([
            (
                "a".to_string(),
                WorldspaceRecord {
                    form_id: 1,
                    editor_id: "A".to_string(),
                    parent_worldspace: Some(2),
                    parent_flags: PNAM_INHERIT_CLIMATE,
                    ..Default::default()
                },
            ),
            (
                "b".to_string(),
                WorldspaceRecord {
                    form_id: 2,
                    editor_id: "B".to_string(),
                    parent_worldspace: Some(1),
                    parent_flags: PNAM_INHERIT_CLIMATE,
                    ..Default::default()
                },
            ),
        ]);
        assert_eq!(
            resolve_worldspace_climate(&worldspaces, &HashMap::new(), "a"),
            None,
            "a cyclic WNAM chain must terminate, not infinite-loop"
        );
    }

    /// A WNAM parent FormID that doesn't resolve to any parsed worldspace
    /// (dangling / cross-plugin-missing-master reference) terminates
    /// unresolved rather than panicking.
    #[test]
    fn dangling_parent_form_id_terminates_unresolved() {
        let worldspaces = HashMap::from([(
            "child".to_string(),
            WorldspaceRecord {
                form_id: 1,
                editor_id: "Child".to_string(),
                parent_worldspace: Some(0xDEAD_BEEF),
                parent_flags: PNAM_INHERIT_CLIMATE,
                ..Default::default()
            },
        )]);
        assert_eq!(
            resolve_worldspace_climate(&worldspaces, &HashMap::new(), "child"),
            None
        );
    }

    // ── resolve_water_material ────────────────────────────────────

    /// Regression for #1069 / F-WAT-09 — `reflection_color` parsed from
    /// WATR DATA must reach `WaterMaterial.reflection_tint` via
    /// `resolve_water_material`. Pre-fix the field was silently dropped.
    #[test]
    fn resolve_water_material_transfers_reflection_color() {
        let lava_tint = [0.85_f32, 0.30, 0.10]; // orange-red lava pool

        let rec = WatrRecord {
            form_id: 0x000A_BCDE,
            editor_id: "LavaPool01".to_string(),
            full_name: "Lava Pool".to_string(),
            opacity: 0.75,
            opacity_authored: false,
            legacy_flags: None,
            texture_path: String::new(),
            noise_texture_paths: Default::default(),
            flow_noise_texture_path: String::new(),
            linear_velocity: None,
            related_waters: [0; 3],
            params: WaterParams {
                shallow_color: [1.0, 0.4, 0.1],
                deep_color: [0.6, 0.1, 0.0],
                underwater_color: [0.6, 0.1, 0.0],
                reflection_color: lava_tint,
                fog_near: 20.0,
                fog_far: 80.0,
                underwater_fog_near: 0.0,
                underwater_fog_far: 0.0,
                underwater_fog_amount: 1.0,
                reflectivity: 0.40,
                fresnel: 0.04,
                wind_speed: 0.0,
                wind_direction: 0.0,
                wave_amplitude: 0.0,
                wave_frequency: 0.0,
                sun_specular_power: 90.0,
                noise_uv_scale_a: 0.0,
                noise_uv_scale_b: 0.0,
                noise_uv_scale_c: 0.0,
                noise_amplitude_scales: [0.0; 3],
                normal_magnitude: 1.0,
                above_water_fog_amount: 1.0,
                depth_weights: [0.0; 4],
                effect_controls: [0.0; 4],
                flowmap_scale: 0.0,
                specular_magnitude: 0.0,
                noise_wind_directions: [0.0; 3],
                noise_wind_speeds: [0.0; 3],
                absorption_ranges: [0.0; 3],
                roughness: 0.0,
                silt_amount: 0.0,
                silt_light_color: [0.0; 3],
                silt_dark_color: [0.0; 3],
            },
            raw_dnam: Vec::new(),
            raw_data: Vec::new(),
        };

        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);

        let (mat, _kind, _flow, _normal, _noise) =
            resolve_water_material(&waters, Some(0x000A_BCDE));

        assert_eq!(
            mat.reflection_tint, lava_tint,
            "reflection_tint must round-trip from WATR DATA reflection_color"
        );
        assert_eq!(mat.sun_specular_power, 90.0);
    }

    #[test]
    fn resolve_water_material_honors_legacy_non_reflective_flag() {
        let mut rec = calm_watr(
            0x000A_BEEF,
            "NVCleanWater",
            WaterParams {
                reflectivity: 0.8,
                ..WaterParams::default()
            },
        );
        rec.legacy_flags = Some(0x01); // damage only; FNAM 0x02 is absent
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);

        let (mat, _, _, _, _) = resolve_water_material(&waters, Some(0x000A_BEEF));
        assert_eq!(mat.reflectivity, 0.0);
    }

    #[test]
    fn resolve_water_material_uses_gnam_underwater_variant() {
        let parent_id = 0x000A_1000;
        let underwater_id = 0x000A_1001;
        let mut parent = calm_watr(parent_id, "LakeSurface", WaterParams::default());
        parent.related_waters[2] = underwater_id;
        let underwater = calm_watr(
            underwater_id,
            "LakeUnderwater",
            WaterParams {
                underwater_color: [0.08, 0.16, 0.30],
                underwater_fog_near: 12.0,
                underwater_fog_far: 340.0,
                underwater_fog_amount: 0.65,
                ..WaterParams::default()
            },
        );
        let mut waters = HashMap::new();
        waters.insert(parent_id, parent);
        waters.insert(underwater_id, underwater);

        let (mat, _, _, _, _) = resolve_water_material(&waters, Some(parent_id));
        assert_eq!(mat.underwater_color, [0.08, 0.16, 0.30]);
        assert_eq!(mat.underwater_fog_near, 12.0);
        assert_eq!(mat.underwater_fog_far, 340.0);
        assert_eq!(mat.underwater_fog_amount, 0.65);
    }

    #[test]
    fn resolve_water_material_preserves_gnam_day_night_surface_variants() {
        let parent_id = 0x000A_1100;
        let day_id = 0x000A_1101;
        let night_id = 0x000A_1102;
        let mut parent = calm_watr(parent_id, "LakeSurface", WaterParams::default());
        parent.related_waters[0] = day_id;
        parent.related_waters[1] = night_id;
        let day = calm_watr(
            day_id,
            "LakeDay",
            WaterParams {
                shallow_color: [0.20, 0.45, 0.55],
                deep_color: [0.03, 0.10, 0.16],
                reflection_color: [0.75, 0.80, 0.85],
                fog_near: 30.0,
                fog_far: 300.0,
                ..WaterParams::default()
            },
        );
        let night = calm_watr(
            night_id,
            "LakeNight",
            WaterParams {
                shallow_color: [0.02, 0.05, 0.10],
                deep_color: [0.005, 0.01, 0.03],
                reflection_color: [0.12, 0.16, 0.24],
                fog_near: 8.0,
                fog_far: 90.0,
                ..WaterParams::default()
            },
        );
        let mut waters = HashMap::new();
        waters.insert(parent_id, parent);
        waters.insert(day_id, day);
        waters.insert(night_id, night);

        let (mat, _, _, _, _) = resolve_water_material(&waters, Some(parent_id));
        assert_eq!(mat.day_shallow_color, [0.20, 0.45, 0.55]);
        assert_eq!(mat.day_fog_far, 300.0);
        assert_eq!(mat.night_deep_color, [0.005, 0.01, 0.03]);
        assert_eq!(mat.night_reflection_tint, [0.12, 0.16, 0.24]);
        assert_eq!(mat.night_fog_near, 8.0);
    }

    /// Default WaterMaterial (no XCWT / no WATR record) uses the neutral
    /// grey that matches the pre-#1069 hard-coded shader value.
    #[test]
    fn default_water_material_has_neutral_reflection_tint() {
        let (mat, _, _, _, _) = resolve_water_material(&HashMap::new(), None);
        assert_eq!(
            mat.reflection_tint,
            [0.65, 0.70, 0.75],
            "default reflection_tint must match the pre-fix shader hard-code"
        );
    }

    /// Minimal calm WATR with only the named appearance fields authored;
    /// everything else left at `WaterParams::default`. Helper for the
    /// WATAL translate-up tests below.
    fn calm_watr(form: u32, edid: &str, params: WaterParams) -> WatrRecord {
        WatrRecord {
            form_id: form,
            editor_id: edid.to_string(),
            full_name: String::new(),
            opacity: 0.75,
            opacity_authored: false,
            legacy_flags: None,
            texture_path: String::new(),
            noise_texture_paths: Default::default(),
            flow_noise_texture_path: String::new(),
            linear_velocity: None,
            related_waters: [0; 3],
            params,
            raw_dnam: Vec::new(),
            raw_data: Vec::new(),
        }
    }

    /// WATAL Phase 1: `wave_amplitude` / `wave_frequency` are parsed into
    /// `WaterParams` for every era but were dropped at the translate
    /// boundary pre-WATAL. They must now reach the canonical material.
    #[test]
    fn resolve_water_material_carries_wave_params() {
        let rec = calm_watr(
            0x000A_0001,
            "DefaultWater",
            WaterParams {
                wave_amplitude: 1.5,
                wave_frequency: 2.0,
                ..WaterParams::default()
            },
        );
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);

        let (mat, kind, _flow, _normal, _noise) =
            resolve_water_material(&waters, Some(0x000A_0001));
        assert_eq!(
            mat.wave_amplitude, 1.5,
            "wave_amplitude must round-trip from WATR"
        );
        assert_eq!(
            mat.wave_frequency, 2.0,
            "wave_frequency must round-trip from WATR"
        );
        assert!(matches!(kind, WaterKind::Calm));
    }

    #[test]
    fn flowing_water_preserves_authored_layer_motion() {
        let mut rec = calm_watr(
            0x000A_0002,
            "LocalizedWater",
            WaterParams {
                wind_direction: 0.0,
                noise_wind_directions: [0.0, std::f32::consts::FRAC_PI_2, 0.25],
                noise_wind_speeds: [0.10, 0.20, 0.30],
                ..WaterParams::default()
            },
        );
        // NAM5 is an authored flow signal even when the EDID is localized or
        // otherwise gives no English river/stream hint.
        rec.flow_noise_texture_path = "textures\\water\\flow.dds".to_string();
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);

        let (mat, kind, flow, _, _) = resolve_water_material(&waters, Some(0x000A_0002));
        assert!(matches!(kind, WaterKind::River));
        assert!(flow.is_some());
        assert!(mat.scroll_a[0] > 0.10);
        assert!(mat.scroll_b[1] > 0.20);
        assert!((mat.scroll_c[0] - 0.30 * 0.25_f32.cos()).abs() < 1e-6);
        assert!((mat.scroll_c[1] - 0.30 * 0.25_f32.sin()).abs() < 1e-6);
    }

    #[test]
    fn flowmap_scale_is_not_baked_into_canonical_flow_scroll() {
        let mut rec = calm_watr(
            0x000A_0004,
            "LocalizedRiver",
            WaterParams {
                flowmap_scale: 3.0,
                wind_direction: 0.0,
                ..WaterParams::default()
            },
        );
        rec.flow_noise_texture_path = "textures\\water\\flow.dds".to_string();
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);

        let (mat, kind, flow, _, _) = resolve_water_material(&waters, Some(0x000A_0004));
        let speed = flow.expect("flowing water must have a canonical current").speed;
        assert!(matches!(kind, WaterKind::River));
        assert_eq!(mat.flowmap_scale, 3.0);
        assert!((mat.scroll_a[0] - speed * WATER_SCROLL_UV_PER_BU_PER_S).abs() < 1.0e-6);
    }

    #[test]
    fn resolve_water_material_carries_starfield_absorption_ranges() {
        let rec = calm_watr(
            0x000A_0008,
            "StarfieldOcean",
            WaterParams {
                absorption_ranges: [12.0, 34.0, 56.0],
                ..WaterParams::default()
            },
        );
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (mat, _, _, _, _) = resolve_water_material(&waters, Some(0x000A_0008));
        assert_eq!(mat.absorption_ranges, [12.0, 34.0, 56.0]);
    }

    #[test]
    fn resolve_water_material_maps_starfield_roughness_to_sun_exponent() {
        let rec = calm_watr(
            0x000A_0009,
            "StarfieldOcean",
            WaterParams {
                roughness: 0.5,
                ..WaterParams::default()
            },
        );
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (mat, _, _, _, _) = resolve_water_material(&waters, Some(0x000A_0009));
        assert!((mat.sun_specular_power - 6.0).abs() < 1e-6);
    }

    #[test]
    fn resolve_water_material_blends_fo4_silt_into_water_palette() {
        let rec = calm_watr(
            0x000A_000A,
            "MuddyWater",
            WaterParams {
                shallow_color: [0.8, 0.8, 0.8],
                deep_color: [0.8, 0.8, 0.8],
                underwater_color: [0.8, 0.8, 0.8],
                silt_amount: 0.5,
                silt_light_color: [1.0, 0.0, 0.0],
                silt_dark_color: [0.0, 0.0, 0.0],
                ..WaterParams::default()
            },
        );
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (mat, _, _, _, _) = resolve_water_material(&waters, Some(0x000A_000A));
        assert!(mat.shallow_color[0] > mat.shallow_color[1]);
        assert!(mat.deep_color.iter().all(|channel| *channel < 0.8));
        assert_eq!(mat.deep_color, mat.underwater_color);
    }

    #[test]
    fn resolve_water_material_carries_authored_noise_uv_scales() {
        let rec = calm_watr(
            0x000A_0003,
            "DefaultWater",
            WaterParams {
                noise_uv_scale_a: 1.0 / 320.0,
                noise_uv_scale_b: 1.0 / 760.0,
                ..WaterParams::default()
            },
        );
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);

        let (mat, _, _, _, _) = resolve_water_material(&waters, Some(0x000A_0003));
        assert!((mat.uv_scale_a - 1.0 / 320.0).abs() < 1e-6);
        assert!((mat.uv_scale_b - 1.0 / 760.0).abs() < 1e-6);
    }

    #[test]
    fn resolve_water_material_applies_authored_normal_magnitude() {
        let rec = calm_watr(
            0x000A_0004,
            "SkyrimWater",
            WaterParams {
                normal_magnitude: 0.5,
                above_water_fog_amount: 0.5,
                noise_amplitude_scales: [0.8, 0.6, 0.4],
                depth_weights: [1.0, 1.0, 1.0, 1.0],
                ..WaterParams::default()
            },
        );
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);

        let (mat, _, _, _, _) = resolve_water_material(&waters, Some(0x000A_0004));
        assert_eq!(mat.noise_amplitude_scales, [0.4, 0.3, 0.2]);
        assert_eq!(mat.depth_weights[1], 0.5);
    }

    #[test]
    fn resolve_water_material_carries_authored_opacity() {
        let mut rec = calm_watr(0x000A_0005, "OpaqueWater", WaterParams::default());
        rec.opacity = 0.62;
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (mat, _, _, _, _) = resolve_water_material(&waters, Some(0x000A_0005));
        assert!((mat.opacity - 0.62).abs() < 1e-6);
    }

    #[test]
    fn resolve_water_material_honors_authored_zero_opacity() {
        let mut rec = calm_watr(0x000A_0006, "InvisibleWater", WaterParams::default());
        rec.opacity = 0.0;
        rec.opacity_authored = true;
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (mat, _, _, _, _) = resolve_water_material(&waters, Some(0x000A_0006));
        assert_eq!(mat.opacity, 0.0);
    }

    #[test]
    fn resolve_water_material_preserves_low_authored_opacity() {
        let mut rec = calm_watr(0x000A_0007, "ThinWater", WaterParams::default());
        rec.opacity = 0.01;
        rec.opacity_authored = true;
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (mat, _, _, _, _) = resolve_water_material(&waters, Some(0x000A_0007));
        assert!((mat.opacity - 0.01).abs() < 1.0e-6);
    }

    #[test]
    fn resolve_water_material_carries_authored_noise_paths() {
        let mut rec = calm_watr(0x000A_0002, "DefaultWater", WaterParams::default());
        rec.noise_texture_paths = [
            "textures/water/noise_a.dds".into(),
            String::new(),
            "textures/water/noise_c.dds".into(),
        ];
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (_, _, _, _, noise) = resolve_water_material(&waters, Some(0x000A_0002));
        assert_eq!(noise[0].as_deref(), Some("textures/water/noise_a.dds"));
        assert!(noise[1].is_none());
        assert_eq!(noise[2].as_deref(), Some("textures/water/noise_c.dds"));
    }

    #[test]
    fn flowing_water_promotes_skyrim_flow_normal_over_nam4() {
        let mut rec = calm_watr(0x000A_0006, "RiverWater", WaterParams::default());
        rec.noise_texture_paths[2] = "textures/water/noise_c.dds".into();
        rec.flow_noise_texture_path = "textures/water/flow.dds".into();
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (_, kind, _, _, noise) = resolve_water_material(&waters, Some(0x000A_0006));
        assert!(matches!(kind, WaterKind::River));
        assert_eq!(noise[2].as_deref(), Some("textures/water/flow.dds"));
    }

    #[test]
    fn authored_flow_normal_promotes_neutral_water_to_river() {
        let mut rec = calm_watr(0x000A_0007, "DefaultWater", WaterParams::default());
        rec.noise_texture_paths[2] = "textures/water/noise_c.dds".into();
        rec.flow_noise_texture_path = "textures/water/flow.dds".into();
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (_, kind, _, _, noise) = resolve_water_material(&waters, Some(0x000A_0007));
        assert!(matches!(kind, WaterKind::River));
        assert_eq!(noise[2].as_deref(), Some("textures/water/flow.dds"));
    }

    #[test]
    fn authored_nam0_velocity_promotes_localized_water_to_river() {
        let mut rec = calm_watr(0x000A_0008, "AguaPrincipal", WaterParams::default());
        rec.linear_velocity = Some([3.0, 0.0]);
        // The authored current points along renderer +X; weather wind points
        // along +Z. Translation must preserve NAM0 rather than substitute the
        // atmospheric direction.
        rec.params.wind_direction = std::f32::consts::FRAC_PI_2;
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (material, kind, flow, _, _) = resolve_water_material(&waters, Some(0x000A_0008));
        assert!(matches!(kind, WaterKind::River));
        let flow = flow.expect("NAM0 velocity must produce a canonical current");
        assert!(flow.direction[0] > 0.99);
        assert!((flow.speed - 3.0).abs() < 1.0e-6);
        assert!(material.scroll_a[0] > 0.0);
        assert!(material.scroll_a[1].abs() < 1.0e-6);
        assert!(material.scroll_b[0].abs() < 1.0e-6);
        assert!(material.scroll_b[1] > 0.0);
    }

    #[test]
    fn authored_nam0_rapid_velocity_selects_rapids_profile() {
        let mut rec = calm_watr(0x000A_0009, "AguaRapida", WaterParams::default());
        rec.linear_velocity = Some([0.0, 12.0]);
        rec.params.wind_direction = std::f32::consts::FRAC_PI_2;
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (mat, kind, flow, _, _) = resolve_water_material(&waters, Some(0x000A_0009));
        assert!(matches!(kind, WaterKind::Rapids));
        assert_eq!(mat.foam_strength, 0.85);
        assert!((flow.expect("rapid NAM0 flow").speed - 12.0).abs() < 1.0e-6);
    }

    #[test]
    fn calm_water_uses_authored_normal_layer_wind_without_touching_flow() {
        let mut params = WaterParams::default();
        params.noise_wind_directions = [std::f32::consts::FRAC_PI_2, 0.0, 0.0];
        params.noise_wind_speeds = [0.03, 0.02, 0.0];
        let rec = calm_watr(0x000A_0003, "DefaultWater", params);
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);
        let (mat, kind, flow, _, _) = resolve_water_material(&waters, Some(0x000A_0003));
        assert!(matches!(kind, WaterKind::Calm));
        assert!(flow.is_none());
        assert!(mat.scroll_a[0].abs() < 1e-6);
        assert!((mat.scroll_a[1] - 0.03).abs() < 1e-6);
        assert!((mat.scroll_b[0] - 0.02).abs() < 1e-6);
    }

    // ── #2872 — WaterFlow.speed unit ──────────────────────────────

    /// The regression itself. Every vanilla FO3 / FNV / Skyrim WATR whose
    /// DATA is 196 bytes (or DNAM 228) carries `90.0` in the float the
    /// parser reads as `wind_speed` — a constant, the same value the shorter
    /// legacy layouts put in the *direction* slot. Feeding it straight into
    /// `WaterFlow::speed` made the physics current's terminal velocity 90
    /// BU/s (3.6× the documented ceiling) and blew `scroll_a` out to ~45
    /// against a documented default of `[0.020, 0.011]`.
    ///
    /// The canonical speed must now come from the `WaterKind` band and be
    /// completely independent of that field.
    #[test]
    fn flow_speed_ignores_the_watr_wind_field_and_stays_in_band() {
        // The real vanilla value, and two adversarial ones.
        for wind_speed in [90.0_f32, 0.0, -1e9] {
            let rec = calm_watr(
                0x000B_0001,
                "WaterRiverFallingSlow",
                WaterParams {
                    wind_speed,
                    wind_direction: 0.0,
                    ..WaterParams::default()
                },
            );
            let mut waters = HashMap::new();
            waters.insert(rec.form_id, rec);

            let (mat, kind, flow, _, _) = resolve_water_material(&waters, Some(0x000B_0001));
            let flow = flow.expect("a river EDID must synthesize a flow");
            assert!(matches!(kind, WaterKind::River));
            assert_eq!(
                flow.speed,
                WaterFlow::speed_for_kind(WaterKind::River),
                "speed must come from the kind, not from wind_speed={wind_speed}"
            );
            assert!(
                (WaterFlow::SPEED_MIN..=WaterFlow::SPEED_MAX).contains(&flow.speed),
                "speed {} escaped the documented band",
                flow.speed
            );
            // The shader scroll is derived from the same canonical scalar,
            // so it can no longer diverge from the physics current.
            let scroll = (mat.scroll_a[0].powi(2) + mat.scroll_a[1].powi(2)).sqrt();
            assert!(
                scroll < 0.05,
                "scroll_a magnitude {scroll} is nowhere near the documented \
                 [0.020, 0.011] default — the pre-fix value was ~45"
            );
        }
    }

    /// Rapids are faster than rivers, and the whole ladder stays inside the
    /// band `WaterFlow::speed` documents — so `current_force`'s terminal
    /// velocity target is bounded for every kind the translate site emits.
    #[test]
    fn flow_speed_ladder_is_ordered_and_bounded() {
        let mut waters = HashMap::new();
        for (form, edid) in [(0x000C_0001, "WhiteRapidsFast"), (0x000C_0002, "RiverSlow")] {
            waters.insert(form, calm_watr(form, edid, WaterParams::default()));
        }
        let (_, rapids_kind, rapids, _, _) = resolve_water_material(&waters, Some(0x000C_0001));
        let (_, river_kind, river, _, _) = resolve_water_material(&waters, Some(0x000C_0002));
        assert!(matches!(rapids_kind, WaterKind::Rapids));
        assert!(matches!(river_kind, WaterKind::River));

        let rapids = rapids.expect("rapids flow");
        let river = river.expect("river flow");
        assert!(
            rapids.speed > river.speed,
            "rapids ({}) must run faster than a river ({})",
            rapids.speed,
            river.speed
        );
        for speed in [rapids.speed, river.speed] {
            assert!((WaterFlow::SPEED_MIN..=WaterFlow::SPEED_MAX).contains(&speed));
        }
    }

    /// Calm water carries no flow at all, so the physics current never
    /// engages on a lake and the material keeps its default scroll.
    #[test]
    fn calm_water_carries_no_flow_and_default_scroll() {
        let rec = calm_watr(0x000D_0001, "DefaultWater", WaterParams::default());
        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);

        let (mat, kind, flow, _, _) = resolve_water_material(&waters, Some(0x000D_0001));
        assert!(matches!(kind, WaterKind::Calm));
        assert!(flow.is_none());
        assert_eq!(mat.scroll_a, WaterMaterial::default().scroll_a);
    }

    /// WATAL translate-up contract (docs/engine/watal.md §3/§4): a poorer
    /// (Oblivion-shaped) record and a richer (Skyrim-shaped) record that
    /// author *different* appearance values must still resolve to
    /// **identical SENTINEL fields** — the engine-default fields no game
    /// authors. They may differ ONLY in AUTHORED fields. This is what lets
    /// the renderer + solver consume one `WaterMaterial` regardless of the
    /// source game.
    #[test]
    fn resolve_water_material_sentinels_are_game_invariant() {
        // "Oblivion-shaped": sparse DATA — colours + a short fog only.
        let oblivion = calm_watr(
            0x0001_0000,
            "OblivionLake",
            WaterParams {
                shallow_color: [0.08, 0.20, 0.24],
                deep_color: [0.01, 0.04, 0.06],
                fog_near: 80.0,
                fog_far: 600.0,
                ..WaterParams::default()
            },
        );
        // "Skyrim-shaped": richer authored colours/fog/reflectivity.
        let skyrim = calm_watr(
            0x0002_0000,
            "SkyrimLake",
            WaterParams {
                shallow_color: [0.12, 0.36, 0.42],
                deep_color: [0.02, 0.07, 0.11],
                fog_near: 120.0,
                fog_far: 1200.0,
                reflectivity: 0.92,
                ..WaterParams::default()
            },
        );

        let mut waters = HashMap::new();
        waters.insert(oblivion.form_id, oblivion);
        waters.insert(skyrim.form_id, skyrim);

        let (ob, ob_kind, ob_flow, _, _) = resolve_water_material(&waters, Some(0x0001_0000));
        let (sk, _, _, _, _) = resolve_water_material(&waters, Some(0x0002_0000));
        let def = WaterMaterial::default();

        // AUTHORED fields differ (proves the two records are distinct).
        assert_ne!(
            ob.fog_far, sk.fog_far,
            "authored fog must differ between the two records"
        );

        // SENTINEL fields no game authors must be identical across games
        // AND equal to the canonical default — the translate-up invariant.
        for (label, a, b, d) in [
            ("ior", ob.ior, sk.ior, def.ior),
            (
                "shoreline_width",
                ob.shoreline_width,
                sk.shoreline_width,
                def.shoreline_width,
            ),
            ("uv_scale_a", ob.uv_scale_a, sk.uv_scale_a, def.uv_scale_a),
            ("uv_scale_b", ob.uv_scale_b, sk.uv_scale_b, def.uv_scale_b),
            (
                "foam_strength",
                ob.foam_strength,
                sk.foam_strength,
                def.foam_strength,
            ),
        ] {
            assert_eq!(a, b, "SENTINEL `{label}` must be game-invariant");
            assert_eq!(a, d, "SENTINEL `{label}` must equal the canonical default");
        }
        assert_eq!(
            ob.normal_map_index,
            u32::MAX,
            "no texture authored → procedural sentinel"
        );
        assert!(
            (def.foam_strength - 0.65).abs() < f32::EPSILON,
            "calm-water sentinel must retain shoreline foam"
        );
        assert_eq!(
            sk.normal_map_index,
            u32::MAX,
            "no texture authored → procedural sentinel"
        );
        // Calm water authors no flow — a real distinction, not a leak.
        assert!(matches!(ob_kind, WaterKind::Calm));
        assert!(ob_flow.is_none(), "calm water has no synthesized flow");
    }

    /// Regression pin for #1997 (REN-D15-01) — the returned `normal_path`
    /// (4th tuple element) is what the caller (`spawn_water_plane`) uses
    /// to decide whether a water plane takes the procedural shader branch
    /// vs. the bound-texture branch. Three ways a plane can fall through
    /// to the procedural default, all of which the shader-side fix
    /// (render-origin-relative hashing) now assumes is the COMMON case,
    /// not an edge case:
    ///
    /// 1. no XCWT on the cell at all (`xcwt_form = None`)
    /// 2. XCWT present but the form doesn't resolve in `waters`
    /// 3. XCWT resolves but the WATR's `texture_path` is empty (e.g. a
    ///    lava pool — mirrors the `LavaPool01` fixture used above)
    ///
    /// A fourth case with a populated `texture_path` proves the positive
    /// side: normal_path IS produced when the format actually authors one.
    #[test]
    fn resolve_water_material_procedural_default_classification() {
        // Case 1: no XCWT at all.
        let (_, _, _, normal_none, _) = resolve_water_material(&HashMap::new(), None);
        assert!(
            normal_none.is_none(),
            "no XCWT must classify as procedural default"
        );

        // Case 2: XCWT present but unresolvable form.
        let (_, _, _, normal_unresolved, _) =
            resolve_water_material(&HashMap::new(), Some(0x00DE_AD00));
        assert!(
            normal_unresolved.is_none(),
            "unresolvable XCWT must classify as procedural default"
        );

        // Case 3: XCWT resolves, WATR has an empty texture_path.
        let no_tex = calm_watr(0x000B_0001, "LavaPool01", WaterParams::default());
        let mut waters = HashMap::new();
        waters.insert(no_tex.form_id, no_tex);
        let (_, _, _, normal_empty_tex, _) = resolve_water_material(&waters, Some(0x000B_0001));
        assert!(
            normal_empty_tex.is_none(),
            "WATR with empty texture_path must classify as procedural default"
        );

        // Case 4 (contrast): non-empty texture_path must resolve to Some.
        let mut with_tex = calm_watr(0x000B_0002, "DefaultWater", WaterParams::default());
        with_tex.texture_path = "textures\\water\\defaultwater.dds".to_string();
        let mut waters2 = HashMap::new();
        waters2.insert(with_tex.form_id, with_tex);
        let (_, _, _, normal_with_tex, _) = resolve_water_material(&waters2, Some(0x000B_0002));
        assert_eq!(
            normal_with_tex,
            Some("textures\\water\\defaultwater.dds".to_string()),
            "WATR with a texture_path must NOT classify as procedural default"
        );
    }

    // ── exterior sky / sun / weather translation (EXAL step 3) ────

    use byroredux_plugin::esm::records::weather::{
        SkyColor, SKY_AMBIENT, SKY_FOG, SKY_HORIZON, SKY_LOWER, SKY_SUN, SKY_SUNLIGHT, SKY_UPPER,
        TOD_DAY,
    };

    /// Build a WTHR with a distinct colour in one (group, day-slot) cell.
    fn wthr_with(group: usize, c: SkyColor) -> WeatherRecord {
        let mut w = WeatherRecord::default();
        w.sky_colors[group][TOD_DAY] = c;
        w
    }

    #[test]
    fn translate_cell_lighting_reads_day_slot_and_marks_exterior() {
        let mut w = WeatherRecord::default();
        w.sky_colors[SKY_AMBIENT][TOD_DAY] = SkyColor {
            r: 51,
            g: 0,
            b: 0,
            a: 255,
        };
        w.sky_colors[SKY_SUNLIGHT][TOD_DAY] = SkyColor {
            r: 0,
            g: 102,
            b: 0,
            a: 255,
        };
        w.sky_colors[SKY_FOG][TOD_DAY] = SkyColor {
            r: 0,
            g: 0,
            b: 204,
            a: 255,
        };
        w.fog_day_near = 1500.0;
        w.fog_day_far = 9000.0;

        let sun_dir = [0.1, 0.9, 0.2];
        let cl = translate_exterior_cell_lighting(&w, sun_dir);
        assert_eq!(cl.ambient, [51.0 / 255.0, 0.0, 0.0]);
        assert_eq!(cl.directional_color, [0.0, 102.0 / 255.0, 0.0]);
        assert_eq!(cl.fog_color, [0.0, 0.0, 204.0 / 255.0]);
        assert_eq!(cl.directional_dir, sun_dir);
        assert_eq!((cl.fog_near, cl.fog_far), (1500.0, 9000.0));
        assert!(!cl.is_interior);
    }

    #[test]
    fn translate_sky_routes_each_cloud_layer_and_sun_sprite() {
        // Distinct day-slot colours so the slot→field mapping is pinned.
        let mut w = wthr_with(
            SKY_UPPER,
            SkyColor {
                r: 10,
                g: 0,
                b: 0,
                a: 255,
            },
        );
        w.sky_colors[SKY_HORIZON][TOD_DAY] = SkyColor {
            r: 0,
            g: 20,
            b: 0,
            a: 255,
        };
        w.sky_colors[SKY_LOWER][TOD_DAY] = SkyColor {
            r: 0,
            g: 0,
            b: 30,
            a: 255,
        };
        w.sky_colors[SKY_SUN][TOD_DAY] = SkyColor {
            r: 40,
            g: 40,
            b: 0,
            a: 255,
        };

        let textures = SkyTextures {
            cloud_layers: [(11, 0.11), (22, 0.22), (33, 0.33), (44, 0.44)],
            sun_sprite: 99,
        };
        let sun_dir = [0.0, 1.0, 0.0];
        let sky = translate_sky(&w, sun_dir, textures);

        // Colour slot routing.
        assert_eq!(sky.zenith_color, [10.0 / 255.0, 0.0, 0.0]);
        assert_eq!(sky.horizon_color, [0.0, 20.0 / 255.0, 0.0]);
        assert_eq!(sky.lower_color, [0.0, 0.0, 30.0 / 255.0]);
        assert_eq!(sky.sun_color, [40.0 / 255.0, 40.0 / 255.0, 0.0]);
        // Cloud-layer handle/scale routing — each of the 4 lands in its slot.
        assert_eq!((sky.cloud_texture_index, sky.cloud_tile_scale), (11, 0.11));
        assert_eq!(
            (sky.cloud_texture_index_1, sky.cloud_tile_scale_1),
            (22, 0.22)
        );
        assert_eq!(
            (sky.cloud_texture_index_2, sky.cloud_tile_scale_2),
            (33, 0.33)
        );
        assert_eq!(
            (sky.cloud_texture_index_3, sky.cloud_tile_scale_3),
            (44, 0.44)
        );
        assert_eq!(sky.sun_texture_index, 99);
        // Canonical seeds.
        assert_eq!(sky.sun_direction, sun_dir);
        assert_eq!(sky.sun_size, 0.9995);
        assert_eq!(sky.sun_intensity, 4.0);
        assert_eq!(sky.sun_angular_radius, 0.020);
        assert!(sky.is_exterior);
        // DALC is populated per-frame by weather_system, not at translate.
        assert!(sky.current_dalc_cube.is_none());
    }

    #[test]
    fn translate_weather_copies_fog_wind_and_falls_back_tod_without_climate() {
        let mut w = WeatherRecord {
            fog_day_near: 100.0,
            fog_day_far: 200.0,
            fog_night_near: 300.0,
            fog_night_far: 400.0,
            fog_day_max: 0.9,
            fog_night_max: 0.4,
            wind_speed: 7,
            ..Default::default()
        };
        w.sky_colors[SKY_UPPER][TOD_DAY] = SkyColor {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        };

        let wd = translate_weather(&w, None);
        assert_eq!(wd.fog, [100.0, 200.0, 300.0, 400.0]);
        assert_eq!(
            wd.fog_media[0],
            crate::fog::FogMedium::from_legacy_ramp(100.0, 200.0, Some(0.9))
        );
        assert_eq!(
            wd.fog_media[1],
            crate::fog::FogMedium::from_legacy_ramp(300.0, 400.0, Some(0.4))
        );
        assert_eq!(wd.wind_speed, 7);
        // No climate → the validated `climate_tod_hours` fallback.
        assert_eq!(wd.tod_hours, [6.0, 10.0, 18.0, 22.0]);
        // FNV/FO3/Oblivion WTHR (no DALC sub-records) → None.
        assert!(wd.skyrim_dalc_per_tod.is_none());
        // The NAM0 table round-trips to f32.
        assert_eq!(wd.sky_colors[SKY_UPPER][TOD_DAY], [1.0, 0.0, 0.0]);
    }

    #[test]
    fn weather_classification_drives_density_coverage_with_precipitation_priority() {
        use byroredux_plugin::esm::records::weather::{
            WTHR_CLOUDY, WTHR_PLEASANT, WTHR_RAINY, WTHR_SNOW,
        };
        for (classification, expected) in [
            (0, 0.55),
            (WTHR_PLEASANT, 0.40),
            (WTHR_CLOUDY, 0.70),
            (WTHR_SNOW, 0.80),
            (WTHR_RAINY, 0.86),
            (WTHR_PLEASANT | WTHR_RAINY, 0.86),
        ] {
            assert_eq!(fog_coverage_from_weather(classification), expected);
            let weather = WeatherRecord {
                classification,
                ..WeatherRecord::default()
            };
            let translated = translate_weather(&weather, None);
            assert_eq!(translated.fog_media[0].coverage, expected);
            assert_eq!(translated.fog_media[1].coverage, expected);
            let expected_precipitation = if classification & WTHR_RAINY != 0 {
                1.0
            } else if classification & WTHR_SNOW != 0 {
                0.12
            } else {
                0.0
            };
            assert_eq!(translated.precipitation, expected_precipitation);
        }
    }

    #[test]
    fn procedural_fallback_pins_mojave_defaults() {
        let sun_dir = [-0.4, 0.8, -0.45];
        let cl = procedural_fallback_cell_lighting(sun_dir);
        assert_eq!(cl.ambient, [0.15, 0.14, 0.12]);
        assert_eq!(cl.directional_dir, sun_dir);
        assert_eq!((cl.fog_near, cl.fog_far), (15000.0, 80000.0));
        assert!(cl.fog_medium.extinction_per_meter > 0.0);
        assert!(!cl.is_interior);

        let sky = procedural_fallback_sky(sun_dir);
        assert_eq!(sky.zenith_color, [0.15, 0.3, 0.65]);
        // Below-horizon ground tint matches the pre-#541 `horizon * 0.3`.
        assert_eq!(sky.lower_color, [0.55 * 0.3, 0.5 * 0.3, 0.42 * 0.3]);
        assert_eq!(sky.cloud_tile_scale, 0.0); // no clouds in the fallback
        assert_eq!(sky.sun_texture_index, 0); // procedural disc

        let wd = procedural_fallback_weather();
        assert_eq!(wd.tod_hours, [6.0, 10.0, 18.0, 22.0]);
        assert_eq!(wd.wind_speed, 0);
        assert_eq!(wd.fog_media, [cl.fog_medium; 2]);
        assert!(wd.skyrim_dalc_per_tod.is_none());
        // Synthetic table: the day slot of the read groups carries the
        // procedural colour, and the lerp endpoints are equal.
        assert_eq!(wd.sky_colors[SKY_AMBIENT][TOD_DAY], [0.15, 0.14, 0.12]);
        assert_eq!(
            wd.sky_colors[SKY_HORIZON][0],
            wd.sky_colors[SKY_HORIZON][TOD_DAY]
        );
    }
}
