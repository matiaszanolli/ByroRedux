//! EXAL region weather resolution (Volumetric Lighting PRD `VLM-REG`): which weather an exterior
//! cell gets when the regions drawn over it author a Weather table, settled once per cell
//! crossing at this boundary (VLM-REG-010). The output is a `WeatherRecord` choice plus a
//! diagnostic [`WeatherSource`] for `env.health`; no region, game or record identity reaches
//! `WeatherDataRes`, `SkyParamsRes` or the renderer.
//!
//! The rules, each a recorded decision rather than an inference:
//!
//! - **Membership is the region polygon** (VLM-REG-008): a region covers a cell when its `RPLD`
//!   polygon overlaps the cell's square ([`RegnRecord::covers_cell`]) and its `WNAM` is the cell's
//!   worldspace. The cell's `XCLR` list is not consulted: the 2026-11-07 census found it stale in
//!   both directions on every title.
//! - **Override sections** (VLM-REG-001, -012): the covering Weather section with the highest
//!   Priority among those with the Override bit wins; when several regions share that Priority none
//!   takes precedence and the next-highest Priority is used (the CK page's tie rule).
//! - **Non-override sections** (VLM-REG-002, an engine choice until #6772 observes the original):
//!   the rows of every covering non-override section form one table, and the highest-chance row
//!   wins (§10 Q2's rule, applied across the merged table); ties go to the higher section Priority,
//!   then the lower region FormID, then the authored row order. Oblivion's 527 equal-Priority
//!   overlapping cells are decided by chance this way, not by record order.
//! - **Chance** (VLM-REG-004, -005): a row's chance is its global's value at resolution time when
//!   it names one, else the authored percentage. A row that is never chosen (chance not above zero,
//!   or a weather the load order lacks) does not count.
//!
//! The cell's own `XCCM` climate, when it authors one, is the most specific link and outranks the
//! region tables. No vanilla Oblivion / FO3 / FNV exterior cell carries both, so the order is moot
//! there (VLM-REG-013, census 2026-11-07); it is fixed here for mod data.

use std::collections::HashMap;

use byroredux_plugin::esm::records::{
    ClimateRecord, RegionDataKind, RegionDataPayload, RegionWeather, RegnRecord, WeatherRecord,
};

/// What kind of record chose the weather in effect (PRD §2 "Weather source").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum WeatherSourceClass {
    /// Nothing has been installed from a record (the procedural fallback, or no environment yet).
    #[default]
    None,
    /// A script forced it (ObScript `ForceWeather` / `SetWeather`).
    Forced,
    /// An Override region Weather section.
    RegionOverride,
    /// The merged table of the covering non-override region sections.
    RegionMerged,
    /// A cell's own `XCCM` climate.
    CellClimate,
    /// The worldspace (or inherited) climate.
    Climate,
    /// A Show-Sky interior's `XCCM` naming a region (Skyrim / FO4, VLM-REG-009).
    InteriorRegion,
}

impl WeatherSourceClass {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Forced => "forced",
            Self::RegionOverride => "region-override",
            Self::RegionMerged => "region-merged",
            Self::CellClimate => "cell-climate",
            Self::Climate => "climate",
            Self::InteriorRegion => "interior-region",
        }
    }
}

/// Which authored record chose the weather, for `env.health` (VLM-REG-011). Diagnostic only: it is
/// recorded beside the installed environment and never reaches a canonical resource.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct WeatherSource {
    pub(crate) class: WeatherSourceClass,
    /// The deciding region, for the region classes.
    pub(crate) region: Option<u32>,
    /// The deciding section's Priority and Override bit, for the region classes.
    pub(crate) priority: Option<u8>,
    pub(crate) override_bit: bool,
    /// The climate that supplied the sky textures and time-of-day breakpoints.
    pub(crate) climate: Option<u32>,
}

/// A weather chosen from region tables.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RegionWeatherChoice<'a> {
    pub(crate) weather: &'a WeatherRecord,
    pub(crate) source: WeatherSource,
}

/// One covering region's Weather section.
struct Section<'a> {
    region: u32,
    priority: u8,
    is_override: bool,
    rows: &'a [RegionWeather],
}

/// Every Weather section of the regions that cover exterior cell `grid` of the worldspace whose
/// `WRLD` FormID is `worldspace_form`, ordered by region FormID so the result is deterministic.
fn covering_sections<'a>(
    regions: &'a HashMap<u32, RegnRecord>,
    worldspace_form: u32,
    grid: (i32, i32),
) -> Vec<Section<'a>> {
    let mut covering: Vec<&RegnRecord> = regions
        .values()
        .filter(|r| r.worldspace_form == Some(worldspace_form) && r.covers_cell(grid.0, grid.1))
        .collect();
    covering.sort_by_key(|r| r.form_id);
    covering
        .into_iter()
        .flat_map(|r| {
            r.entries
                .iter()
                .filter(|e| e.kind == RegionDataKind::Weather)
                .filter_map(move |e| match &e.payload {
                    RegionDataPayload::Weather(rows) => Some(Section {
                        region: r.form_id,
                        priority: e.priority,
                        is_override: e.flags & 1 != 0,
                        rows,
                    }),
                    _ => None,
                })
        })
        .collect()
}

/// The chance a row has right now: its global's value when it names one the caller can read, else
/// the authored percentage (VLM-REG-005). Not above zero means the row is never chosen.
fn row_chance(row: &RegionWeather, global_value: &dyn Fn(u32) -> Option<f32>) -> f32 {
    row.global_form
        .filter(|&g| g != 0)
        .and_then(global_value)
        .unwrap_or(row.chance as f32)
}

/// The highest-chance row among `sections`' rows whose weather exists, with the section it came
/// from. Ties: higher section Priority, then lower region FormID, then authored row order.
fn best_row<'a>(
    sections: &[&Section<'a>],
    weathers: &'a HashMap<u32, WeatherRecord>,
    global_value: &dyn Fn(u32) -> Option<f32>,
) -> Option<(&'a WeatherRecord, u32, u8)> {
    let mut best: Option<(f32, u8, u32, &'a WeatherRecord)> = None;
    for section in sections {
        for row in section.rows {
            let chance = row_chance(row, global_value);
            if chance <= 0.0 || !chance.is_finite() {
                continue;
            }
            let Some(weather) = weathers.get(&row.weather_form) else {
                continue;
            };
            let better = match &best {
                None => true,
                Some((c, p, r, _)) => {
                    chance > *c
                        || (chance == *c
                            && (section.priority > *p
                                || (section.priority == *p && section.region < *r)))
                }
            };
            if better {
                best = Some((chance, section.priority, section.region, weather));
            }
        }
    }
    best.map(|(_, priority, region, weather)| (weather, region, priority))
}

/// Resolve the weather the regions over `grid` choose, or `None` when none authors a usable
/// Weather table (VLM-REG-003: the caller then keeps the climate's weather).
pub(crate) fn resolve_region_weather<'a>(
    regions: &'a HashMap<u32, RegnRecord>,
    weathers: &'a HashMap<u32, WeatherRecord>,
    worldspace_form: u32,
    grid: (i32, i32),
    global_value: &dyn Fn(u32) -> Option<f32>,
) -> Option<RegionWeatherChoice<'a>> {
    let sections = covering_sections(regions, worldspace_form, grid);
    if sections.is_empty() {
        return None;
    }

    // Override sections: walk the Priorities from the highest; a Priority shared by two regions
    // lets neither take precedence (VLM-REG-012), so the walk continues to the next one down.
    let mut priorities: Vec<u8> = sections.iter().filter(|s| s.is_override).map(|s| s.priority).collect();
    priorities.sort_unstable_by(|a, b| b.cmp(a));
    priorities.dedup();
    for priority in priorities {
        let tier: Vec<&Section> = sections
            .iter()
            .filter(|s| s.is_override && s.priority == priority)
            .collect();
        let distinct_regions = {
            let mut ids: Vec<u32> = tier.iter().map(|s| s.region).collect();
            ids.dedup();
            ids.len()
        };
        if distinct_regions != 1 {
            continue;
        }
        if let Some((weather, region, priority)) = best_row(&tier, weathers, global_value) {
            return Some(RegionWeatherChoice {
                weather,
                source: WeatherSource {
                    class: WeatherSourceClass::RegionOverride,
                    region: Some(region),
                    priority: Some(priority),
                    override_bit: true,
                    climate: None,
                },
            });
        }
    }

    // Non-override sections: one merged table (VLM-REG-002's engine choice).
    let merged: Vec<&Section> = sections.iter().filter(|s| !s.is_override).collect();
    best_row(&merged, weathers, global_value).map(|(weather, region, priority)| RegionWeatherChoice {
        weather,
        source: WeatherSource {
            class: WeatherSourceClass::RegionMerged,
            region: Some(region),
            priority: Some(priority),
            override_bit: false,
            climate: None,
        },
    })
}

/// The weather an exterior cell resolves to, before any script-forced weather replaces it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CellWeather<'a> {
    /// The climate supplying sky textures and time-of-day breakpoints.
    pub(crate) climate: Option<&'a ClimateRecord>,
    /// `None` when no climate or region resolves a weather: the procedural fallback sky.
    pub(crate) weather: Option<&'a WeatherRecord>,
    pub(crate) source: WeatherSource,
}

/// Everything [`resolve_cell_weather`] reads, so one value is built per call site.
pub(crate) struct CellWeatherInputs<'a> {
    pub(crate) regions: &'a HashMap<u32, RegnRecord>,
    pub(crate) climates: &'a HashMap<u32, ClimateRecord>,
    pub(crate) weathers: &'a HashMap<u32, WeatherRecord>,
    /// The `WRLD` FormID of the cell's worldspace (region `WNAM` is compared against it).
    pub(crate) worldspace_form: u32,
    /// The worldspace's own climate (the inherited one, resolved at the worldspace boundary).
    pub(crate) worldspace_climate: Option<u32>,
}

/// Resolve one exterior cell's weather: its own `XCCM` climate, else the regions' weather tables,
/// else the worldspace climate's default weather. `cell_climate_override` is the cell's `XCCM`.
pub(crate) fn resolve_cell_weather<'a>(
    inputs: &CellWeatherInputs<'a>,
    cell_climate_override: Option<u32>,
    grid: (i32, i32),
    global_value: &dyn Fn(u32) -> Option<f32>,
) -> CellWeather<'a> {
    let climate_form = super::resolve_cell_climate(
        cell_climate_override,
        inputs.worldspace_climate,
        inputs.climates,
    );
    let climate = climate_form.and_then(|f| inputs.climates.get(&f));
    let cell_pinned = cell_climate_override.is_some() && cell_climate_override == climate_form;
    let climate_default = |class| {
        climate.and_then(|c| {
            super::resolve_default_weather(c, inputs.weathers).map(|(weather, _)| CellWeather {
                climate,
                weather: Some(weather),
                source: WeatherSource { class, climate: climate_form, ..Default::default() },
            })
        })
    };
    if cell_pinned {
        if let Some(resolved) = climate_default(WeatherSourceClass::CellClimate) {
            return resolved;
        }
    }
    if let Some(choice) = resolve_region_weather(
        inputs.regions,
        inputs.weathers,
        inputs.worldspace_form,
        grid,
        global_value,
    ) {
        return CellWeather {
            climate,
            weather: Some(choice.weather),
            source: WeatherSource { climate: climate_form, ..choice.source },
        };
    }
    climate_default(if cell_pinned { WeatherSourceClass::CellClimate } else { WeatherSourceClass::Climate })
        .unwrap_or(CellWeather {
            climate,
            weather: None,
            source: WeatherSource { climate: climate_form, ..Default::default() },
        })
}

/// What [`resolve_interior_region_weather`] reads.
pub(crate) struct InteriorSkyInputs<'a> {
    pub(crate) regions: &'a HashMap<u32, RegnRecord>,
    pub(crate) climates: &'a HashMap<u32, ClimateRecord>,
    pub(crate) weathers: &'a HashMap<u32, WeatherRecord>,
    pub(crate) worldspaces: &'a HashMap<String, byroredux_plugin::esm::cell::WorldspaceRecord>,
    pub(crate) worldspace_climates: &'a HashMap<String, u32>,
}

/// Resolve an interior's weather from the region its `XCCM` names ("Sky/Weather from Region",
/// Skyrim / FO4, VLM-REG-009). The region's own Weather table decides, with no polygon and no other
/// region involved: its highest-chance row, ties to the higher section Priority. The climate that
/// supplies the sky textures and time-of-day breakpoints is the climate of the worldspace the
/// region belongs to (`WNAM`). `None` when the region is unknown or authors no usable table, so the
/// caller keeps whatever sky is showing.
pub(crate) fn resolve_interior_region_weather<'a>(
    inputs: &InteriorSkyInputs<'a>,
    sky_region: u32,
    global_value: &dyn Fn(u32) -> Option<f32>,
) -> Option<CellWeather<'a>> {
    let region = inputs.regions.get(&sky_region)?;
    let sections: Vec<Section> = region
        .entries
        .iter()
        .filter(|e| e.kind == RegionDataKind::Weather)
        .filter_map(|e| match &e.payload {
            RegionDataPayload::Weather(rows) => Some(Section {
                region: region.form_id,
                priority: e.priority,
                is_override: e.flags & 1 != 0,
                rows,
            }),
            _ => None,
        })
        .collect();
    let refs: Vec<&Section> = sections.iter().collect();
    let (weather, region_form, priority) = best_row(&refs, inputs.weathers, global_value)?;
    let override_bit = sections.iter().any(|s| s.is_override && s.priority == priority);
    let climate_form = region.worldspace_form.and_then(|form| {
        let key = inputs.worldspaces.iter().find(|(_, w)| w.form_id == form).map(|(k, _)| k)?;
        super::resolve_worldspace_climate(inputs.worldspaces, inputs.worldspace_climates, key)
    });
    Some(CellWeather {
        climate: climate_form.and_then(|f| inputs.climates.get(&f)),
        weather: Some(weather),
        source: WeatherSource {
            class: WeatherSourceClass::InteriorRegion,
            region: Some(region_form),
            priority: Some(priority),
            override_bit,
            climate: climate_form,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::{ClimateWeather, RegionArea, RegionDataEntry};

    const WS: u32 = 0x3C;
    const U: f32 = 4096.0;

    fn weather(form: u32) -> WeatherRecord {
        WeatherRecord { form_id: form, editor_id: format!("W{form:X}"), ..Default::default() }
    }

    fn weathers(forms: &[u32]) -> HashMap<u32, WeatherRecord> {
        forms.iter().map(|&f| (f, weather(f))).collect()
    }

    /// A region covering grid cells `(0,0)..=(2,2)`, with one Weather section.
    fn region(form: u32, priority: u8, is_override: bool, rows: &[(u32, u32, Option<u32>)]) -> RegnRecord {
        RegnRecord {
            form_id: form,
            worldspace_form: Some(WS),
            areas: vec![RegionArea {
                edge_fall_off: 0,
                points: vec![(0.0, 0.0), (3.0 * U, 0.0), (3.0 * U, 3.0 * U), (0.0, 3.0 * U)],
            }],
            entries: vec![RegionDataEntry {
                kind: RegionDataKind::Weather,
                flags: u8::from(is_override),
                priority,
                payload: RegionDataPayload::Weather(
                    rows.iter()
                        .map(|&(weather_form, chance, global_form)| RegionWeather {
                            weather_form,
                            chance,
                            global_form,
                        })
                        .collect(),
                ),
            }],
            ..Default::default()
        }
    }

    fn map(regions: Vec<RegnRecord>) -> HashMap<u32, RegnRecord> {
        regions.into_iter().map(|r| (r.form_id, r)).collect()
    }

    fn no_globals(_: u32) -> Option<f32> {
        None
    }

    fn pick(regions: &HashMap<u32, RegnRecord>, weathers: &HashMap<u32, WeatherRecord>) -> Option<(u32, WeatherSourceClass)> {
        resolve_region_weather(regions, weathers, WS, (1, 1), &no_globals)
            .map(|c| (c.weather.form_id, c.source.class))
    }

    /// VLM-REG-001.
    #[test]
    fn region_weather_override_priority_wins() {
        let w = weathers(&[0xA, 0xB, 0xC]);
        let regions = map(vec![
            region(1, 40, true, &[(0xA, 100, None)]),
            region(2, 70, true, &[(0xB, 10, None)]),
            // A higher Priority without the Override bit is not compared (the wiki's rule).
            region(3, 99, false, &[(0xC, 100, None)]),
        ]);
        let choice = resolve_region_weather(&regions, &w, WS, (1, 1), &no_globals).unwrap();
        assert_eq!(choice.weather.form_id, 0xB, "Priority 70 beats 40; chance does not enter");
        assert_eq!(choice.source.class, WeatherSourceClass::RegionOverride);
        assert_eq!((choice.source.region, choice.source.priority), (Some(2), Some(70)));
        assert!(choice.source.override_bit);
    }

    /// VLM-REG-012: a shared top Priority lets neither region take precedence.
    #[test]
    fn region_weather_priority_tie_falls_through() {
        let w = weathers(&[0xA, 0xB, 0xC]);
        let tied = map(vec![
            region(1, 50, true, &[(0xA, 100, None)]),
            region(2, 50, true, &[(0xB, 100, None)]),
            region(3, 30, true, &[(0xC, 100, None)]),
        ]);
        assert_eq!(pick(&tied, &w), Some((0xC, WeatherSourceClass::RegionOverride)), "the next Priority down");
        let all_tied = map(vec![
            region(1, 50, true, &[(0xA, 100, None)]),
            region(2, 50, true, &[(0xB, 100, None)]),
        ]);
        assert_eq!(pick(&all_tied, &w), None, "nobody takes precedence and no non-override table exists");
    }

    /// VLM-REG-002 (the recorded engine choice): non-override rows merge, highest chance wins.
    #[test]
    fn non_override_regions_merge_and_the_highest_chance_wins() {
        let w = weathers(&[0xA, 0xB, 0xC]);
        let regions = map(vec![
            region(1, 50, false, &[(0xA, 40, None), (0xC, 10, None)]),
            region(2, 50, false, &[(0xB, 85, None)]),
        ]);
        assert_eq!(pick(&regions, &w), Some((0xB, WeatherSourceClass::RegionMerged)));
    }

    /// Equal chance, equal Priority (Oblivion's 527 cells): the lower region FormID, not map order.
    #[test]
    fn a_merged_tie_goes_to_priority_then_the_lower_region_form() {
        let w = weathers(&[0xA, 0xB]);
        let same = map(vec![
            region(9, 50, false, &[(0xB, 50, None)]),
            region(4, 50, false, &[(0xA, 50, None)]),
        ]);
        assert_eq!(pick(&same, &w).map(|p| p.0), Some(0xA), "region 4 < region 9");
        let ranked = map(vec![
            region(9, 70, false, &[(0xB, 50, None)]),
            region(4, 50, false, &[(0xA, 50, None)]),
        ]);
        assert_eq!(pick(&ranked, &w).map(|p| p.0), Some(0xB), "Priority 70 first");
    }

    /// An Override section outranks every non-override one.
    #[test]
    fn an_override_section_outranks_the_merged_table() {
        let w = weathers(&[0xA, 0xB]);
        let regions = map(vec![
            region(1, 10, true, &[(0xA, 1, None)]),
            region(2, 90, false, &[(0xB, 100, None)]),
        ]);
        assert_eq!(pick(&regions, &w), Some((0xA, WeatherSourceClass::RegionOverride)));
    }

    /// VLM-REG-005.
    #[test]
    fn a_rows_global_replaces_its_authored_chance() {
        let w = weathers(&[0xA, 0xB]);
        let regions = map(vec![region(1, 50, true, &[(0xA, 90, Some(0x77)), (0xB, 60, None)])]);
        let winner = |value: Option<f32>| {
            resolve_region_weather(&regions, &w, WS, (1, 1), &move |g| (g == 0x77).then_some(value).flatten())
                .unwrap()
                .weather
                .form_id
        };
        assert_eq!(winner(None), 0xA, "a global that cannot be read leaves the authored 90");
        assert_eq!(winner(Some(10.0)), 0xB, "the global now says 10");
        assert_eq!(winner(Some(0.0)), 0xB, "a zero chance is never chosen");
    }

    /// A weather the load order lacks is skipped, not the whole table.
    #[test]
    fn a_missing_weather_falls_to_the_next_row() {
        let w = weathers(&[0xB]);
        let regions = map(vec![region(1, 50, true, &[(0xDEAD, 100, None), (0xB, 20, None)])]);
        assert_eq!(pick(&regions, &w).map(|p| p.0), Some(0xB));
    }

    /// Membership is the polygon within the region's own worldspace.
    #[test]
    fn only_regions_covering_the_cell_in_its_worldspace_count() {
        let w = weathers(&[0xA, 0xB]);
        let mut elsewhere = region(2, 99, true, &[(0xB, 100, None)]);
        elsewhere.worldspace_form = Some(WS + 1);
        let regions = map(vec![region(1, 10, true, &[(0xA, 100, None)]), elsewhere]);
        assert_eq!(pick(&regions, &w).map(|p| p.0), Some(0xA), "the other worldspace's region is ignored");
        assert_eq!(
            resolve_region_weather(&regions, &w, WS, (7, 7), &no_globals).map(|c| c.weather.form_id),
            None,
            "outside every polygon"
        );
    }

    fn climate(form: u32, weather_form: u32) -> ClimateRecord {
        ClimateRecord {
            form_id: form,
            editor_id: format!("C{form:X}"),
            weathers: vec![ClimateWeather { weather_form_id: weather_form, chance: 100 }],
            ..Default::default()
        }
    }

    fn inputs<'a>(
        regions: &'a HashMap<u32, RegnRecord>,
        climates: &'a HashMap<u32, ClimateRecord>,
        weathers: &'a HashMap<u32, WeatherRecord>,
    ) -> CellWeatherInputs<'a> {
        CellWeatherInputs { regions, climates, weathers, worldspace_form: WS, worldspace_climate: Some(0xC1) }
    }

    /// VLM-REG-003.
    #[test]
    fn cell_without_region_weather_keeps_climate_weather() {
        let w = weathers(&[0xA, 0xB]);
        let climates = HashMap::from([(0xC1, climate(0xC1, 0xA))]);
        let regions = map(vec![region(1, 50, true, &[(0xB, 100, None)])]);
        let resolved = resolve_cell_weather(&inputs(&regions, &climates, &w), None, (9, 9), &no_globals);
        assert_eq!(resolved.weather.map(|x| x.form_id), Some(0xA), "outside the polygon: the climate default");
        assert_eq!(resolved.source.class, WeatherSourceClass::Climate);
        assert_eq!(resolved.source.climate, Some(0xC1));
        let inside = resolve_cell_weather(&inputs(&regions, &climates, &w), None, (1, 1), &no_globals);
        assert_eq!(inside.weather.map(|x| x.form_id), Some(0xB));
        assert_eq!(inside.climate.map(|c| c.form_id), Some(0xC1), "the climate still supplies the sky");
        let no_regions = HashMap::new();
        let plain = resolve_cell_weather(&inputs(&no_regions, &climates, &w), None, (1, 1), &no_globals);
        assert_eq!(plain.weather.map(|x| x.form_id), Some(0xA));
    }

    /// The cell's own `XCCM` is the most specific link (moot on vanilla Oblivion / FO3 / FNV).
    #[test]
    fn a_cell_climate_override_outranks_the_regions() {
        let w = weathers(&[0xA, 0xB, 0xD]);
        let climates = HashMap::from([(0xC1, climate(0xC1, 0xA)), (0xC2, climate(0xC2, 0xD))]);
        let regions = map(vec![region(1, 50, true, &[(0xB, 100, None)])]);
        let resolved = resolve_cell_weather(&inputs(&regions, &climates, &w), Some(0xC2), (1, 1), &no_globals);
        assert_eq!(resolved.weather.map(|x| x.form_id), Some(0xD));
        assert_eq!(resolved.source.class, WeatherSourceClass::CellClimate);
        // An override naming no parsed climate resolves like no override: the region applies.
        let broken = resolve_cell_weather(&inputs(&regions, &climates, &w), Some(0xBAD), (1, 1), &no_globals);
        assert_eq!(broken.weather.map(|x| x.form_id), Some(0xB));
    }

    /// VLM-REG-009: an interior naming a region takes that region's own table, the sky and
    /// time-of-day of the climate of the region's worldspace.
    #[test]
    fn an_interior_region_takes_its_own_table_and_its_worldspaces_climate() {
        use byroredux_plugin::esm::cell::WorldspaceRecord;
        let w = weathers(&[0xA, 0xB]);
        let climates = HashMap::from([(0xC1, climate(0xC1, 0xA))]);
        let mut hold = region(5, 50, false, &[(0xA, 30, None), (0xB, 70, None)]);
        // A polygon far from anywhere: the interior path must not need one.
        hold.areas.clear();
        let regions = map(vec![hold]);
        let worldspaces = HashMap::from([(
            "tamriel".to_string(),
            WorldspaceRecord { form_id: WS, editor_id: "Tamriel".into(), ..Default::default() },
        )]);
        let worldspace_climates = HashMap::from([("tamriel".to_string(), 0xC1)]);
        let inputs = InteriorSkyInputs {
            regions: &regions,
            climates: &climates,
            weathers: &w,
            worldspaces: &worldspaces,
            worldspace_climates: &worldspace_climates,
        };
        let resolved = resolve_interior_region_weather(&inputs, 5, &no_globals).unwrap();
        assert_eq!(resolved.weather.map(|x| x.form_id), Some(0xB), "the region's highest-chance row");
        assert_eq!(resolved.climate.map(|c| c.form_id), Some(0xC1), "the region's worldspace climate");
        assert_eq!(resolved.source.class, WeatherSourceClass::InteriorRegion);
        assert_eq!(resolved.source.region, Some(5));
        assert!(resolve_interior_region_weather(&inputs, 0xDEAD, &no_globals).is_none(), "unknown region");
        let empty = map(vec![RegnRecord { form_id: 6, worldspace_form: Some(WS), ..Default::default() }]);
        let none = InteriorSkyInputs { regions: &empty, ..inputs };
        assert!(resolve_interior_region_weather(&none, 6, &no_globals).is_none(), "a region with no Weather table");
    }

    /// A2 / VLM-REG coverage (PRD §8): over every exterior cell of a real master, the share whose
    /// weather the resolver takes from a region, among the cells a Weather-bearing region polygon
    /// covers. Prints one line per master and the first few cells that do not resolve to a region,
    /// with the sections that cover them.
    ///
    /// `BYROREDUX_REGION_CENSUS_ESM=<path to a master>  cargo test -p byroredux --bin byroredux
    /// --release -- --ignored --nocapture region_weather_coverage_census`
    #[test]
    #[ignore = "needs a game master: BYROREDUX_REGION_CENSUS_ESM"]
    fn region_weather_coverage_census() {
        let Some(path) = std::env::var_os("BYROREDUX_REGION_CENSUS_ESM").filter(|v| !v.is_empty()) else {
            panic!("set BYROREDUX_REGION_CENSUS_ESM to a master .esm path");
        };
        let index = byroredux_plugin::esm::records::parse_esm(&std::fs::read(&path).unwrap()).unwrap();
        let authored_global = |form: u32| index.globals.get(&form).map(|g| g.value.as_f32());
        let (mut covered, mut from_region, mut override_src, mut merged_src) = (0usize, 0usize, 0usize, 0usize);
        let (mut no_weather, mut shown) = (0usize, 0usize);
        for (key, cells) in &index.cells.exterior_cells {
            let Some(ws) = index.cells.worldspaces.get(key) else { continue };
            let worldspace_climate = crate::env_translate::resolve_worldspace_climate(
                &index.cells.worldspaces,
                &index.cells.worldspace_climates,
                key,
            );
            let inputs = CellWeatherInputs {
                regions: &index.regions,
                climates: &index.climates,
                weathers: &index.weathers,
                worldspace_form: ws.form_id,
                worldspace_climate,
            };
            for (&grid, cell) in cells {
                let covering: Vec<&RegnRecord> = index
                    .regions
                    .values()
                    .filter(|r| {
                        r.worldspace_form == Some(ws.form_id)
                            && r.entries.iter().any(|e| e.kind == RegionDataKind::Weather)
                            && r.covers_cell(grid.0, grid.1)
                    })
                    .collect();
                if covering.is_empty() {
                    continue;
                }
                covered += 1;
                let resolved = resolve_cell_weather(&inputs, cell.climate_override, grid, &authored_global);
                match resolved.source.class {
                    WeatherSourceClass::RegionOverride => {
                        from_region += 1;
                        override_src += 1;
                    }
                    WeatherSourceClass::RegionMerged => {
                        from_region += 1;
                        merged_src += 1;
                    }
                    other => {
                        no_weather += usize::from(resolved.weather.is_none());
                        if shown < 4 {
                            shown += 1;
                            let sections: Vec<String> = covering
                                .iter()
                                .flat_map(|r| {
                                    r.entries.iter().filter(|e| e.kind == RegionDataKind::Weather).map(move |e| {
                                        let rows = match &e.payload {
                                            RegionDataPayload::Weather(rows) => rows
                                                .iter()
                                                .map(|w| format!("{:X}x{}", w.weather_form, w.chance))
                                                .collect::<Vec<_>>()
                                                .join(","),
                                            _ => String::new(),
                                        };
                                        format!("region {:X} prio {} ovr {} rows [{rows}]", r.form_id, e.priority, e.flags & 1)
                                    })
                                })
                                .collect();
                            println!("  unresolved {key} {grid:?} -> {}: {sections:?}", other.label());
                        }
                    }
                }
            }
        }
        println!(
            "region-weather-coverage {}: {from_region} of {covered} cells covered by a Weather-bearing region \
             resolve to a region ({override_src} override, {merged_src} merged); {no_weather} resolve to no weather",
            std::path::Path::new(&path).file_name().unwrap().to_string_lossy(),
        );
    }
}
