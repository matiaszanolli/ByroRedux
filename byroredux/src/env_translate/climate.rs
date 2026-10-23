//! EXAL worldspace / climate / default-weather inheritance: a worldspace's
//! climate through the `WNAM` chain, the exterior climate rungs (cell
//! `XCCM`, region, Oblivion's named climates), and the default weather a
//! climate resolves to.

use std::collections::HashMap;

use byroredux_plugin::esm::cell::WorldspaceRecord;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::{ClimateRecord, WeatherRecord};
use super::inherit_up_chain;
use super::pnam::INHERIT_CLIMATE as PNAM_INHERIT_CLIMATE;

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
/// lowercased-editor-id string (`worldspace_key` in the cell loader), which
/// is why the CLMT FormID can be read from the side-map by the key
/// [`inherit_up_chain`] is already walking.
///
/// Everything else — the `visited` cycle guard, the linear
/// `WorldspaceRecord::form_id` reverse lookup per hop, the three `warn!`
/// termination cases, and "own value beats the chain" precedence — belongs
/// to that helper and is described there.
pub(crate) fn resolve_worldspace_climate(
    worldspaces: &HashMap<String, WorldspaceRecord>,
    worldspace_climates: &HashMap<String, u32>,
    start_key: &str,
) -> Option<u32> {
    // #2814 — routed through the shared walk rather than a bespoke copy of
    // it. `e681a3c1` introduced `inherit_up_chain` and moved DNAM / NAM3+NAM4
    // / NAM2 onto it, but climate — the highest-traffic PNAM bit, since a
    // missed one downgrades a whole worldspace to the procedural fallback
    // sky — kept its pre-helper loop, duplicating the `visited` cycle guard,
    // the linear form_id reverse lookup, the three `warn!` termination cases
    // and the precedence ordering. A future fix to the walk would have landed
    // in one copy and silently missed this one.
    //
    // The only thing climate needed that the helper lacked was the current
    // key: the CLMT FormID is not a `WorldspaceRecord` field, it lives in a
    // side-map keyed the same way. `extract` now receives both.
    //
    // One deliberate behaviour change comes with the merge: the helper reads
    // `worldspaces.get(current)` *before* extracting, so a key present in
    // `worldspace_climates` but absent from `worldspaces` no longer resolves.
    // Both maps are built from the same WRLD walk in the cell loader, so a
    // climate without its own worldspace record is not a state the parser
    // produces — and if it ever were, resolving a climate for a worldspace
    // the loader never parsed would be the bug, not the fix.
    inherit_up_chain(worldspaces, start_key, PNAM_INHERIT_CLIMATE, |key, _| {
        worldspace_climates.get(key).copied()
    })
}

/// The exterior climate for `worldspace_key` at the streaming center
/// `(center_x, center_y)` — every climate rung settled at this one
/// boundary (#5423; exal.md's single-boundary rule), table-shaped per
/// game:
///
/// 1. the WRLD `CNAM` link, chasing the `WNAM` parent chain when the
///    PNAM climate-inherit bit opts in ([`resolve_worldspace_climate`],
///    #2450/EXAL-02) — every game's primary rung; on Oblivion the
///    #5374 parse stamp gives every child the bit, so this rung also
///    carries pre-FO3 inherit-all;
/// 2. the region chain (the center cell's `XCLR` → REGN `CNAM`; see
///    [`region_climate_for_center`]) — data-driven, all games, and
///    **inert on all vanilla content** (#5421 / #5519): xEdit defines
///    no REGN `CNAM` sub-record, and the four-game census finds zero
///    occurrences (Oblivion 211 REGN, Skyrim 317, FO3 139, FO4 106).
///    Kept for mod data — a plugin can author one even if no stock
///    tool exposes the field;
/// 3. the Oblivion-only naming / richest-climate rung
///    ([`oblivion_climate_rungs`]): vanilla `Oblivion.esm` authors no
///    climate on the Tamriel WRLD (zero `CNAM` bytes) and no REGN
///    `CNAM` exists anywhere, so neither rung above can resolve
///    Cyrodiil's climate — the CS naming convention
///    (`<worldspace>Climate` along the whole `WNAM` chain, #5388) does.
pub(crate) fn resolve_exterior_climate(
    cells: &byroredux_plugin::esm::cell::EsmCellIndex,
    climates: &HashMap<u32, ClimateRecord>,
    regions: &HashMap<u32, byroredux_plugin::esm::records::RegnRecord>,
    worldspace_key: &str,
    center_x: i32,
    center_y: i32,
    game: GameKind,
) -> Option<u32> {
    if let Some(form) =
        resolve_worldspace_climate(&cells.worldspaces, &cells.worldspace_climates, worldspace_key)
    {
        return Some(form);
    }
    let fallback = region_climate_for_center(cells, worldspace_key, center_x, center_y, regions)
        .or_else(|| match game {
            GameKind::Oblivion => oblivion_climate_rungs(cells, climates, worldspace_key),
            _ => None,
        });
    if fallback.is_some() {
        log::info!(
            "Worldspace '{}' has no WRLD climate — resolved through the \
             region chain / Oblivion inheritance rungs ({},{})",
            worldspace_key,
            center_x,
            center_y,
        );
    }
    fallback
}

/// #5388 — the Oblivion-only climate rungs, in order (each
/// data-anchored before the next): the CS naming convention along the
/// whole `WNAM` chain, root included — "<key>Climate" (Tamriel's
/// children find `TamrielClimate` through the root; vanilla authors no
/// Tamriel WRLD `CNAM` at all); then, as a LOGGED last resort, the
/// climate carrying the most authored weathers. Pre-fix the naming rung
/// tested only the child's own key ("ICMarketDistrictClimate" does not
/// exist) and every one of the 30 child worldspaces fell to the richest
/// climate, `AllWeather` — New Sheoth rendered Tamriel's Clear palette
/// instead of the Shivering Isles fog.
///
/// #5516 removed a first rung that re-walked the chain for authored
/// `CNAM`s unconditionally: the parse-time #5374 stamp gives every
/// Oblivion child the inherit-all word, so
/// [`resolve_worldspace_climate`]'s bit-gated walk — which
/// [`resolve_exterior_climate`] always runs first — already resolves
/// inherited climates (SEWorld's children resolve SEWorld's authored
/// `CNAM` = SEWorldClimate, whose default is SEFog 30). The removed
/// rung could only fire on hand-built records with zero flags, i.e.
/// test fixtures.
fn oblivion_climate_rungs(
    cells: &byroredux_plugin::esm::cell::EsmCellIndex,
    climates: &HashMap<u32, ClimateRecord>,
    worldspace_key: &str,
) -> Option<u32> {
    let chain = worldspace_name_chain(&cells.worldspaces, worldspace_key);
    if let Some(form) = chain.iter().find_map(|key| named_climate(climates, key)) {
        return Some(form);
    }
    let richest = climates
        .iter()
        .max_by_key(|(fid, climate)| (climate.weathers.len(), std::cmp::Reverse(**fid)))
        .map(|(fid, _)| *fid);
    if richest.is_some() {
        log::warn!(
            "Worldspace '{}' resolved no climate through inheritance, the \
             region chain or the CS naming convention — falling back to the \
             richest-authored climate (a heuristic, not authored data; #5388)",
            worldspace_key,
        );
    }
    richest
}

/// The CS naming convention for ONE worldspace key: the climate named
/// `"<worldspace>Climate"` (case-insensitive — Tamriel →
/// TamrielClimate, SEWorld → SEWorldClimate).
fn named_climate(climates: &HashMap<u32, ClimateRecord>, worldspace_key: &str) -> Option<u32> {
    let convention = format!("{}climate", worldspace_key.to_ascii_lowercase());
    climates
        .iter()
        .find(|(_, climate)| climate.editor_id.to_ascii_lowercase() == convention)
        .map(|(fid, _)| *fid)
}

/// The center cell's region-chain climate: walk the `XCLR` FormID list in
/// authored order and return the first region's `CNAM` that resolves in
/// `regions`. The exact center cell is preferred; if the grid has no cell
/// there (a wilderness gap), the four orthogonal neighbours are probed —
/// Oblivion's regions span dozens of cells, so the nearest cell's region
/// set is the same set.
pub(crate) fn region_climate_for_center(
    index: &byroredux_plugin::esm::cell::EsmCellIndex,
    worldspace_key: &str,
    center_x: i32,
    center_y: i32,
    regions: &HashMap<u32, byroredux_plugin::esm::records::RegnRecord>,
) -> Option<u32> {
    let cells = index.exterior_cells.get(worldspace_key)?;
    let probes = [(center_x, center_y), (center_x + 1, center_y), (center_x - 1, center_y), (center_x, center_y + 1), (center_x, center_y - 1)];
    for (gx, gy) in probes {
        let Some(cell) = cells.get(&(gx, gy)) else {
            continue;
        };
        for region_fid in &cell.regions {
            if let Some(climate) = regions
                .get(region_fid)
                .and_then(|region| region.climate_form)
            {
                return Some(climate);
            }
        }
    }
    None
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

/// The default weather for `climate`, by decreasing authoring
/// preference: its highest-chance WTHR (`WLST`) entry that resolves,
/// then its highest-chance WSLT row that resolves to a (synthesized)
/// weather, and only when NOTHING resolves — the #5363 stand-in.
///
/// Negative chances are mod sentinels / subtractive weights and are
/// filtered before the max (#476). Shared by the once-per-worldspace
/// resolve in `build_exterior_world_context` and the per-cell XCCM
/// re-resolve (#2451), so a cell override picks its weather by exactly
/// the same rule the worldspace default did — #5424 moved the stand-in
/// here from the worldspace path so BOTH callers share the whole rule:
/// an XCCM pocket on a WTHS-only climate applies the same pick (and its
/// TNAM clock), and leaving the pocket restores it.
///
/// #5363 — 26 of vanilla Starfield's 47 climates author only the WSLT
/// seasonal table, whose rows reference WTHS records (SF's replacement
/// weather type — an EDID + REFL parameter blob, not the WTHR schema;
/// see `ClimateRecord::seasonal_weathers`). #5627 closed the gap: those
/// records now synthesize WTHR-shaped entries into the same map
/// (`EsmIndex::synthesize_wths_weathers`), so a WSLT row resolves like
/// any other weather. The authored `DefaultWeather` WTHR — the game's
/// canonical default, a full NAM0 palette — is DEMOTED to the fallback:
/// it fires only when a climate's seasonal rows exist but none of them
/// synthesized (a dangling chain or a declined instance decode), which
/// vanilla never trips. Its chance slot still carries `i32::MAX` (the
/// canonical-default sentinel, not a lottery weight).
pub(crate) fn resolve_default_weather<'a>(
    climate: &ClimateRecord,
    weathers: &'a HashMap<u32, WeatherRecord>,
) -> Option<(&'a WeatherRecord, i32)> {
    if let Some(best) = climate
        .weathers
        .iter()
        .filter(|w| w.chance >= 0)
        .max_by_key(|w| w.chance)
    {
        if let Some(wthr) = weathers.get(&best.weather_form_id) {
            return Some((wthr, best.chance));
        }
    }
    if let Some((wthr, chance)) = climate
        .seasonal_weathers
        .iter()
        .filter(|w| w.chance >= 0)
        .max_by_key(|w| w.chance)
        .and_then(|row| weathers.get(&row.weather_form_id).map(|w| (w, row.chance)))
    {
        log::debug!(
            "Default weather: climate '{}' resolves its highest-chance WSLT row to \
             '{} ({:08X})' (synthesized WTHS, chance {chance}) (#5627)",
            climate.editor_id,
            wthr.editor_id,
            wthr.form_id,
        );
        return Some((wthr, chance));
    }
    if !climate.seasonal_weathers.is_empty() {
        if let Some(wthr) =
            byroredux_plugin::esm::records::weather::default_weather_by_edid(weathers)
        {
            log::info!(
                "Default weather: climate '{}' carries {} WSLT rows but none resolved \
                 (no synthesized WTHS for them) — falling back to the authored \
                 DefaultWeather '{} ({:08X})' (#5363, demoted to fallback by #5627)",
                climate.editor_id,
                climate.seasonal_weathers.len(),
                wthr.editor_id,
                wthr.form_id,
            );
            return Some((wthr, i32::MAX));
        }
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;

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
}

#[cfg(test)]
mod default_weather_rule_tests {
    use super::resolve_default_weather;
    use byroredux_plugin::esm::records::climate::{ClimateRecord, ClimateWeather};
    use byroredux_plugin::esm::records::weather::WeatherRecord;
    use std::collections::HashMap;

    /// #5424/#5363 — ONE default-weather rule serves both the
    /// worldspace resolve and the per-cell XCCM re-resolve: a climate
    /// whose only weather table is the WTHS-referencing WSLT rows
    /// resolves to the authored `DefaultWeather` WTHR stand-in, so an
    /// XCCM pocket on a WTHS-only climate applies (and leaving it
    /// restores) the stand-in's weather instead of keeping the pocket's
    /// sky for the rest of the session.
    #[test]
    fn wths_only_climate_resolves_the_defaultweather_stand_in() {
        let mut weathers = HashMap::new();
        weathers.insert(
            0x0001_0000,
            WeatherRecord {
                form_id: 0x0001_0000,
                editor_id: "DefaultWeather".to_owned(),
                ..Default::default()
            },
        );
        let climate = ClimateRecord {
            form_id: 0x0002_0000,
            editor_id: "AkilaClimate".to_owned(),
            weathers: Vec::new(),
            seasonal_weathers: vec![ClimateWeather {
                weather_form_id: 0x0003_0000, // a WTHS — not in the WTHR map
                chance: 100,
            }],
            ..Default::default()
        };

        let resolved = resolve_default_weather(&climate, &weathers);
        assert_eq!(
            resolved.map(|(wthr, _)| wthr.form_id),
            Some(0x0001_0000),
            "the WTHS-only climate resolves the authored DefaultWeather stand-in"
        );
        assert_eq!(
            resolved.map(|(_, chance)| chance),
            Some(i32::MAX),
            "the stand-in carries the canonical-default sentinel, not a lottery weight"
        );

        // #5517 — "both callers share the rule" cannot be pinned by
        // calling the function twice (that compared it with itself), so
        // pin it on the production text instead: both caller files
        // route their default-weather pick through
        // `resolve_default_weather`, and neither re-inlines the
        // stand-in pick (`default_weather_by_edid`) — the re-inlining
        // that #5424 fixed. `strip_test_modules` because exterior.rs
        // interleaves test modules with production code.
        let exterior_src = crate::cell_loader::exterior_production_source();
        let world_setup_src = byroredux_core::source_scan::strip_test_modules(include_str!(
            "../scene/world_setup.rs"
        ));
        for (name, src) in [
            ("cell_loader/exterior.rs", exterior_src.as_str()),
            ("scene/world_setup.rs", world_setup_src.as_str()),
        ] {
            assert!(
                src.contains("resolve_default_weather("),
                "{name} must route its default-weather pick through the shared rule"
            );
            assert!(
                !src.contains("default_weather_by_edid"),
                "{name} re-inlines the DefaultWeather stand-in pick — the #5424 regression"
            );
        }
    }

    /// #5627 — the demotion: a WSLT row that RESOLVES (its synthesized
    /// WTHS weather landed in the map under the row's FormID) beats the
    /// stand-in and carries its own authored chance, not the
    /// canonical-default sentinel. The stand-in now fires only when no
    /// seasonal row resolves — the test above's shape.
    #[test]
    fn resolvable_wslt_row_beats_the_demoted_stand_in() {
        let mut weathers = HashMap::new();
        weathers.insert(
            0x0001_0000,
            WeatherRecord {
                form_id: 0x0001_0000,
                editor_id: "DefaultWeather".to_owned(),
                ..Default::default()
            },
        );
        // The synthesized WTHS weather, keyed by the WTHS's own FormID —
        // exactly what `EsmIndex::synthesize_wths_weathers` inserts.
        weathers.insert(
            0x0003_0000,
            WeatherRecord {
                form_id: 0x0003_0000,
                editor_id: "Weather_Clear_Template_Synth".to_owned(),
                ..Default::default()
            },
        );
        let climate = ClimateRecord {
            form_id: 0x0002_0000,
            editor_id: "AkilaClimate".to_owned(),
            weathers: Vec::new(),
            seasonal_weathers: vec![ClimateWeather {
                weather_form_id: 0x0003_0000,
                chance: 100,
            }],
            ..Default::default()
        };
        let resolved = resolve_default_weather(&climate, &weathers);
        assert_eq!(
            resolved.map(|(wthr, _)| wthr.form_id),
            Some(0x0003_0000),
            "the synthesized WTHS weather wins over the stand-in"
        );
        assert_eq!(
            resolved.map(|(_, chance)| chance),
            Some(100),
            "the WSLT row's authored chance, not the stand-in sentinel"
        );
    }

    /// The normal path is unchanged beside the stand-in: the
    /// highest-chance WTHR entry wins, negative sentinel chances are
    /// filtered, and a climate with a resolvable WLST never consults
    /// the stand-in even when WSLT rows exist too.
    #[test]
    fn resolvable_wlst_wins_over_the_stand_in() {
        let mut weathers = HashMap::new();
        weathers.insert(
            0x0001_0000,
            WeatherRecord {
                form_id: 0x0001_0000,
                editor_id: "DefaultWeather".to_owned(),
                ..Default::default()
            },
        );
        weathers.insert(
            0x0004_0000,
            WeatherRecord {
                form_id: 0x0004_0000,
                editor_id: "Clear".to_owned(),
                ..Default::default()
            },
        );
        let climate = ClimateRecord {
            weathers: vec![
                ClimateWeather { weather_form_id: 0x0004_0000, chance: 60 },
                ClimateWeather { weather_form_id: 0x0004_0000, chance: -1 },
            ],
            seasonal_weathers: vec![ClimateWeather {
                weather_form_id: 0x0003_0000,
                chance: 100,
            }],
            ..Default::default()
        };
        assert_eq!(
            resolve_default_weather(&climate, &weathers).map(|(wthr, chance)| (
                wthr.form_id,
                chance
            )),
            Some((0x0004_0000, 60)),
            "the resolvable WLST entry wins; the negative sentinel never does"
        );
    }
}

#[cfg(test)]
mod oblivion_climate_rung_tests {
    use super::super::pnam::INHERIT_CLIMATE;
    use super::resolve_exterior_climate;
    use byroredux_plugin::esm::cell::EsmCellIndex;
    use byroredux_plugin::esm::reader::GameKind;
    use byroredux_plugin::esm::records::climate::ClimateRecord;
    use byroredux_plugin::esm::records::RegnRecord;
    use byroredux_plugin::esm::cell::WorldspaceRecord;
    use std::collections::HashMap;

    fn climate(form_id: u32, editor_id: &str, weathers: usize) -> ClimateRecord {
        ClimateRecord {
            form_id,
            editor_id: editor_id.to_owned(),
            weathers: vec![
                byroredux_plugin::esm::records::climate::ClimateWeather {
                    weather_form_id: form_id,
                    chance: 100,
                };
                weathers
            ],
            ..Default::default()
        }
    }

    fn child_world_index(parent_key: &str, parent: WorldspaceRecord) -> EsmCellIndex {
        let mut index = EsmCellIndex::default();
        index.worldspaces.insert(
            "seworld".to_owned(),
            WorldspaceRecord {
                form_id: 0x01,
                ..Default::default()
            },
        );
        index.worldspaces.insert(
            parent_key.to_owned(),
            WorldspaceRecord {
                form_id: 0x02,
                parent_worldspace: Some(0x01),
                ..parent
            },
        );
        // #5516 — the stamped inherit word a parsed Oblivion child
        // carries (the CLIMATE bit is what the climate walk keys on).
        index.worldspaces.insert(
            "sethefringe".to_owned(),
            WorldspaceRecord {
                form_id: 0x03,
                parent_worldspace: Some(0x02),
                parent_flags: INHERIT_CLIMATE,
                ..Default::default()
            },
        );
        index
    }

    /// #5388 — the inheritance rung, now carried by the #5374 parse
    /// stamp: an SEWorld child with no own CNAM resolves the PARENT's
    /// authored `CNAM` (SEWorldClimate) through
    /// `resolve_worldspace_climate`'s bit-gated walk, not the richest
    /// climate. Pre-fix every child fell to `AllWeather` and New Sheoth
    /// rendered Tamriel's palette.
    #[test]
    fn oblivion_child_inherits_the_parent_authored_climate() {
        let mut index = child_world_index(
            "seworld",
            WorldspaceRecord {
                form_id: 0x02,
                ..Default::default()
            },
        );
        index.worldspace_climates.insert("seworld".to_owned(), 0x0004_0001);
        let mut climates = HashMap::new();
        climates.insert(0x0004_0001, climate(0x0004_0001, "SEWorldClimate", 2));
        climates.insert(0x0004_00FF, climate(0x0004_00FF, "AllWeather", 7));

        assert_eq!(
            resolve_exterior_climate(
                &index, &climates, &HashMap::<u32, RegnRecord>::new(),
                "sethefringe", 0, 0, GameKind::Oblivion,
            ),
            Some(0x0004_0001),
            "the SE child resolves SEWorldClimate through inherit-all, not the richest"
        );
    }

    /// #5388 — the naming rung runs along the WHOLE chain, root
    /// included: a Tamriel child with no authored CNAM anywhere finds
    /// `TamrielClimate` through the chain root (vanilla authors no
    /// Tamriel WRLD CNAM; the CS names the climate after the
    /// worldspace). Pre-fix the rung tested only the child's own key.
    #[test]
    fn oblivion_child_finds_the_named_climate_through_the_chain_root() {
        let mut index = EsmCellIndex::default();
        index.worldspaces.insert(
            "tamriel".to_owned(),
            WorldspaceRecord { form_id: 0x01, ..Default::default() },
        );
        index.worldspaces.insert(
            "icmarketdistrict".to_owned(),
            WorldspaceRecord {
                form_id: 0x02,
                parent_worldspace: Some(0x01),
                ..Default::default()
            },
        );
        let mut climates = HashMap::new();
        climates.insert(0x0004_0002, climate(0x0004_0002, "TamrielClimate", 3));
        climates.insert(0x0004_00FF, climate(0x0004_00FF, "AllWeather", 7));

        assert_eq!(
            resolve_exterior_climate(
                &index, &climates, &HashMap::<u32, RegnRecord>::new(),
                "icmarketdistrict", 0, 0, GameKind::Oblivion,
            ),
            Some(0x0004_0002),
            "'<root>Climate' resolves through the chain, not the richest heuristic"
        );
    }

    /// #5388 — the richest climate stays the LOGGED last resort: with no
    /// inheritance, region or naming data it still resolves (the
    /// pre-fix behaviour for genuinely data-less worldspaces) rather
    /// than dropping the sky to the procedural fallback.
    #[test]
    fn richest_climate_is_the_logged_last_resort() {
        let index = child_world_index(
            "seworld",
            WorldspaceRecord { form_id: 0x02, ..Default::default() },
        );
        let mut climates = HashMap::new();
        climates.insert(0x0004_0002, climate(0x0004_0002, "TamrielClimate", 3));
        climates.insert(0x0004_00FF, climate(0x0004_00FF, "AllWeather", 7));

        assert_eq!(
            resolve_exterior_climate(
                &index, &climates, &HashMap::<u32, RegnRecord>::new(),
                "sethefringe", 0, 0, GameKind::Oblivion,
            ),
            Some(0x0004_00FF),
            "with no anchored rung the richest climate still resolves"
        );
    }
}
