//! Region-weather census for the Volumetric Lighting PRD's Phase 0 verify items
//! (`docs/prd/volumetric-lighting.md`: VLM-REG-002, -008, -012, -013) and the interior
//! `XCCM` census (VLM-REG-009).
//!
//! Per master it counts, over every exterior cell of every worldspace:
//!
//! - **REG-002** cells whose `XCLR` regions carry two or more Weather sections and none sets
//!   Override (the case the wikis leave unstated), and how many distinct weather tables that
//!   spans.
//! - **REG-012** cells with two or more overriding Weather sections, split by whether the
//!   highest Priority is shared (a tie) or unique.
//! - **REG-013** cells that carry an `XCCM` climate override *and* any region Weather section.
//! - **REG-008** cells where the `XCLR` set differs from the set of regions whose `RPLD`
//!   polygons overlap the cell's 4096-unit square, counted separately for regions carrying a
//!   Weather section (the only ones that matter to a weather decision).
//! - **REG-009** interior cells that name a region through `XCCM` (Skyrim / FO4), with the
//!   weather-section census of those regions.
//!
//! `cargo run --release -p byroredux-plugin --example region_weather_census -- <master.esm> [-v]`
//!
//! One master at a time: a DLC's cells name base-game regions this probe cannot resolve, so
//! run it on masters only and read DLC rows as unresolved.

use byroredux_plugin::esm::records::{parse_esm, RegionDataKind, RegnRecord};
use std::collections::{BTreeMap, BTreeSet, HashMap};

const CELL_UNITS: f32 = 4096.0;

type Poly = [(f32, f32)];

fn point_in_poly(p: (f32, f32), poly: &Poly) -> bool {
    let mut inside = false;
    let mut j = poly.len().wrapping_sub(1);
    for i in 0..poly.len() {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        if (yi > p.1) != (yj > p.1) && p.0 < (xj - xi) * (p.1 - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn segments_cross(a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32)) -> bool {
    fn orient(p: (f32, f32), q: (f32, f32), r: (f32, f32)) -> f32 {
        (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0)
    }
    let (o1, o2, o3, o4) = (orient(a, b, c), orient(a, b, d), orient(c, d, a), orient(c, d, b));
    (o1 > 0.0) != (o2 > 0.0) && (o3 > 0.0) != (o4 > 0.0)
}

/// Does the polygon overlap the axis-aligned square `[x0,x1]×[y0,y1]`?
fn poly_overlaps_rect(poly: &Poly, x0: f32, y0: f32, x1: f32, y1: f32) -> bool {
    if poly.len() < 3 {
        return false;
    }
    if poly.iter().any(|&(x, y)| x >= x0 && x <= x1 && y >= y0 && y <= y1) {
        return true;
    }
    let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
    if corners.iter().any(|&c| point_in_poly(c, poly)) {
        return true;
    }
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        for k in 0..4 {
            if segments_cross(a, b, corners[k], corners[(k + 1) % 4]) {
                return true;
            }
        }
    }
    false
}

fn region_contains_cell_centre(r: &RegnRecord, gx: i32, gy: i32) -> bool {
    let c = ((gx as f32 + 0.5) * CELL_UNITS, (gy as f32 + 0.5) * CELL_UNITS);
    r.areas.iter().any(|a| point_in_poly(c, &a.points))
}

fn seg_dist(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0) };
    ((p.0 - a.0 - t * dx).powi(2) + (p.1 - a.1 - t * dy).powi(2)).sqrt()
}

/// Smallest distance from any of the cell's centre and corners to any polygon edge, and the
/// region's largest `RPLI` fall-off. Used only to ask whether an XCLR-only pair sits inside the
/// authored edge fall-off.
fn nearest_edge_and_falloff(r: &RegnRecord, gx: i32, gy: i32) -> (f32, f32) {
    let (x0, y0) = (gx as f32 * CELL_UNITS, gy as f32 * CELL_UNITS);
    let pts = [
        (x0, y0), (x0 + CELL_UNITS, y0), (x0 + CELL_UNITS, y0 + CELL_UNITS), (x0, y0 + CELL_UNITS),
        (x0 + CELL_UNITS / 2.0, y0 + CELL_UNITS / 2.0),
    ];
    let mut best = f32::MAX;
    let mut fall = 0.0f32;
    for a in &r.areas {
        fall = fall.max(a.edge_fall_off as f32);
        for i in 0..a.points.len() {
            let (p, q) = (a.points[i], a.points[(i + 1) % a.points.len()]);
            for c in pts {
                best = best.min(seg_dist(c, p, q));
            }
        }
    }
    (best, fall)
}

fn region_overlaps_cell(r: &RegnRecord, gx: i32, gy: i32) -> bool {
    let (x0, y0) = (gx as f32 * CELL_UNITS, gy as f32 * CELL_UNITS);
    r.areas
        .iter()
        .any(|a| poly_overlaps_rect(&a.points, x0, y0, x0 + CELL_UNITS, y0 + CELL_UNITS))
}

/// A region's Weather sections: (priority, override flag, weather table).
fn weather_sections(r: &RegnRecord) -> Vec<(u8, bool, BTreeSet<u32>)> {
    r.entries_by_priority(RegionDataKind::Weather)
        .into_iter()
        .map(|e| {
            let table = match &e.payload {
                byroredux_plugin::esm::records::RegionDataPayload::Weather(rows) => {
                    rows.iter().map(|w| w.weather_form).collect()
                }
                _ => BTreeSet::new(),
            };
            (e.priority, e.flags & 1 != 0, table)
        })
        .collect()
}

#[derive(Default)]
struct Tally {
    cells: usize,
    cells_with_xclr: usize,
    with_weather_region: usize,
    multi_weather: usize,
    // REG-002
    multi_nonoverride: usize,
    multi_nonoverride_distinct_tables: usize,
    multi_nonoverride_distinct_prio: usize,
    // REG-012
    multi_override: usize,
    override_top_tie: usize,
    override_top_tie_distinct_tables: usize,
    // mixed override / non-override
    mixed: usize,
    // REG-013
    xccm_and_weather: usize,
    xccm_cells: usize,
    // REG-008
    set_mismatch: usize,
    set_mismatch_weather: usize,
    xclr_only_weather: usize,
    polygon_only_weather: usize,
    // Weather-region (cell, region) pairs by agreement
    pair_both: usize,
    pair_xclr_only: usize,
    pair_xclr_only_centre: usize,
    pair_xclr_only_in_falloff: usize,
    pair_poly_only_centre: usize,
    pair_poly_only_graze: usize,
    // multiple Weather sections inside one region
    multi_section_regions: usize,
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: region_weather_census <master.esm> [-v]");
    let verbose = args.next().as_deref() == Some("-v");
    let index = parse_esm(&std::fs::read(&path)?)?;
    let regions = &index.regions;

    println!("{path}");
    println!(
        "regions {}  with Weather section {}  with polygons {}",
        regions.len(),
        regions.values().filter(|r| !weather_sections(r).is_empty()).count(),
        regions.values().filter(|r| !r.areas.is_empty()).count()
    );
    let mut multi_section = 0;
    for r in regions.values() {
        if weather_sections(r).len() > 1 {
            multi_section += 1;
        }
    }
    println!("regions with more than one Weather section: {multi_section}");

    let mut flags_seen: BTreeMap<u8, usize> = BTreeMap::new();
    for r in regions.values() {
        for e in r.entries.iter().filter(|e| e.kind == RegionDataKind::Weather) {
            *flags_seen.entry(e.flags).or_default() += 1;
        }
    }
    println!("Weather section RDAT flag values: {flags_seen:?}");

    let mut total = Tally::default();
    let mut by_ws: BTreeMap<&str, Tally> = BTreeMap::new();
    for (ws, cells) in &index.cells.exterior_cells {
        let t = by_ws.entry(ws.as_str()).or_default();
        let ws_form = index.cells.worldspaces.get(ws).map_or(0, |w| w.form_id);
        for (&(gx, gy), cell) in cells {
            t.cells += 1;
            if !cell.regions.is_empty() {
                t.cells_with_xclr += 1;
            }
            if cell.climate_override.is_some() {
                t.xccm_cells += 1;
            }
            // XCLR regions carrying Weather.
            let mut sections: Vec<(u32, u8, bool, BTreeSet<u32>)> = Vec::new();
            for fid in &cell.regions {
                if let Some(r) = regions.get(fid) {
                    for (p, o, tbl) in weather_sections(r) {
                        sections.push((*fid, p, o, tbl));
                    }
                }
            }
            let weather_regions: BTreeSet<u32> = sections.iter().map(|s| s.0).collect();
            if !weather_regions.is_empty() {
                t.with_weather_region += 1;
            }
            if cell.climate_override.is_some() && !weather_regions.is_empty() {
                t.xccm_and_weather += 1;
                if verbose {
                    println!("  REG-013 {ws} ({gx},{gy}) xccm={:08X?} regions={:08X?}", cell.climate_override, weather_regions);
                }
            }
            if weather_regions.len() >= 2 {
                t.multi_weather += 1;
                let over: Vec<_> = sections.iter().filter(|s| s.2).collect();
                let non: Vec<_> = sections.iter().filter(|s| !s.2).collect();
                if over.is_empty() {
                    t.multi_nonoverride += 1;
                    let tables: BTreeSet<_> = non.iter().map(|s| s.3.clone()).collect();
                    if tables.len() > 1 {
                        t.multi_nonoverride_distinct_tables += 1;
                        let prios: BTreeSet<u8> = non.iter().map(|s| s.1).collect();
                        if prios.len() > 1 {
                            t.multi_nonoverride_distinct_prio += 1;
                        }
                        if verbose {
                            println!("  REG-002 {ws} ({gx},{gy}) regions={weather_regions:08X?}");
                        }
                    }
                } else if !non.is_empty() {
                    t.mixed += 1;
                }
                let over_regions: BTreeSet<u32> = over.iter().map(|s| s.0).collect();
                if over_regions.len() >= 2 {
                    t.multi_override += 1;
                    let top = over.iter().map(|s| s.1).max().unwrap();
                    let top_regions: BTreeSet<u32> =
                        over.iter().filter(|s| s.1 == top).map(|s| s.0).collect();
                    if top_regions.len() >= 2 {
                        t.override_top_tie += 1;
                        let tables: BTreeSet<_> =
                            over.iter().filter(|s| s.1 == top).map(|s| s.3.clone()).collect();
                        if tables.len() > 1 {
                            t.override_top_tie_distinct_tables += 1;
                            if verbose {
                                println!("  REG-012 {ws} ({gx},{gy}) top={top} regions={top_regions:08X?}");
                            }
                        }
                    }
                }
            }
            // REG-008: XCLR vs polygon overlap.
            let xclr: BTreeSet<u32> = cell.regions.iter().copied().collect();
            let poly: BTreeSet<u32> = regions
                .iter()
                .filter(|(_, r)| {
                    // A region belongs to one worldspace (WNAM); same-coordinate regions of
                    // another worldspace are not candidates.
                    r.worldspace_form == Some(ws_form) && region_overlaps_cell(r, gx, gy)
                })
                .map(|(&f, _)| f)
                .collect();
            {
                let has_w = |f: &u32| regions.get(f).is_some_and(|r| !weather_sections(r).is_empty());
                for f in xclr.union(&poly).filter(|f| has_w(f)) {
                    let r = &regions[f];
                    match (xclr.contains(f), poly.contains(f)) {
                        (true, true) => t.pair_both += 1,
                        (true, false) => {
                            t.pair_xclr_only += 1;
                            let (d, fall) = nearest_edge_and_falloff(r, gx, gy);
                            if d <= fall {
                                t.pair_xclr_only_in_falloff += 1;
                            }
                            if region_contains_cell_centre(r, gx, gy) {
                                t.pair_xclr_only_centre += 1;
                            }
                        }
                        (false, true) => {
                            if region_contains_cell_centre(r, gx, gy) {
                                t.pair_poly_only_centre += 1;
                                if verbose {
                                    println!("  REG-008c {ws} ({gx},{gy}) region={f:08X} cell-xclr={:08X?}", cell.regions);
                                }
                            } else {
                                t.pair_poly_only_graze += 1;
                            }
                        }
                        _ => {}
                    }
                }
            }
            if xclr != poly {
                t.set_mismatch += 1;
                let has_w = |f: &u32| regions.get(f).is_some_and(|r| !weather_sections(r).is_empty());
                let xo: Vec<u32> = xclr.difference(&poly).copied().filter(has_w).collect();
                let po: Vec<u32> = poly.difference(&xclr).copied().filter(has_w).collect();
                if !xo.is_empty() || !po.is_empty() {
                    t.set_mismatch_weather += 1;
                    t.xclr_only_weather += xo.len();
                    t.polygon_only_weather += po.len();
                    if verbose {
                        println!("  REG-008 {ws} ({gx},{gy}) xclr-only={xo:08X?} polygon-only={po:08X?}");
                    }
                }
            }
        }
    }
    println!("\n{:<22} {:>6} {:>6} {:>6} {:>6} | REG-002 {:>5}/{:>4} | REG-012 {:>5} tie {:>4}/{:>4} | mixed {:>4} | REG-013 {:>4}/{:>4} | REG-008 {:>5} wthr {:>5} (xclr-only {:>4} poly-only {:>4})",
        "worldspace", "cells", "xclr", "w-reg", "multi", "multiNO", "dist", "multiO", "tie", "dist", "mixed", "x&w", "xccm",
        "mism", "cells", "xo", "po");
    for (ws, t) in &by_ws {
        if t.cells == 0 {
            continue;
        }
        println!("{:<22} {:>6} {:>6} {:>6} {:>6} | REG-002 {:>5}/{:>4} | REG-012 {:>5} tie {:>4}/{:>4} | mixed {:>4} | REG-013 {:>4}/{:>4} | REG-008 {:>5} wthr {:>5} (xclr-only {:>4} poly-only {:>4})",
            ws, t.cells, t.cells_with_xclr, t.with_weather_region, t.multi_weather,
            t.multi_nonoverride, t.multi_nonoverride_distinct_tables, t.multi_override,
            t.override_top_tie, t.override_top_tie_distinct_tables, t.mixed,
            t.xccm_and_weather, t.xccm_cells, t.set_mismatch, t.set_mismatch_weather,
            t.xclr_only_weather, t.polygon_only_weather);
        total.cells += t.cells;
        total.cells_with_xclr += t.cells_with_xclr;
        total.with_weather_region += t.with_weather_region;
        total.multi_weather += t.multi_weather;
        total.multi_nonoverride += t.multi_nonoverride;
        total.multi_nonoverride_distinct_tables += t.multi_nonoverride_distinct_tables;
        total.multi_nonoverride_distinct_prio += t.multi_nonoverride_distinct_prio;
        total.multi_override += t.multi_override;
        total.override_top_tie += t.override_top_tie;
        total.override_top_tie_distinct_tables += t.override_top_tie_distinct_tables;
        total.mixed += t.mixed;
        total.xccm_and_weather += t.xccm_and_weather;
        total.xccm_cells += t.xccm_cells;
        total.set_mismatch += t.set_mismatch;
        total.set_mismatch_weather += t.set_mismatch_weather;
        total.xclr_only_weather += t.xclr_only_weather;
        total.polygon_only_weather += t.polygon_only_weather;
        total.pair_both += t.pair_both;
        total.pair_xclr_only += t.pair_xclr_only;
        total.pair_xclr_only_centre += t.pair_xclr_only_centre;
        total.pair_xclr_only_in_falloff += t.pair_xclr_only_in_falloff;
        total.pair_poly_only_centre += t.pair_poly_only_centre;
        total.pair_poly_only_graze += t.pair_poly_only_graze;
    }
    let _ = total.multi_section_regions;
    println!("TOTAL: cells {} with-xclr {} with-weather-region {} multi-weather-region {}",
        total.cells, total.cells_with_xclr, total.with_weather_region, total.multi_weather);
    println!("  REG-002 non-override overlap {} (distinct tables {}, of which Priorities differ {})", total.multi_nonoverride, total.multi_nonoverride_distinct_tables, total.multi_nonoverride_distinct_prio);
    println!("  REG-012 multi-override {} top-priority ties {} (distinct tables {})", total.multi_override, total.override_top_tie, total.override_top_tie_distinct_tables);
    println!("  mixed override + non-override {}", total.mixed);
    println!("  REG-013 XCCM-and-region-weather {} of {} XCCM cells", total.xccm_and_weather, total.xccm_cells);
    println!("  REG-008 XCLR set != polygon-overlap set {} cells; carrying Weather {} (xclr-only {}, polygon-only {})",
        total.set_mismatch, total.set_mismatch_weather, total.xclr_only_weather, total.polygon_only_weather);

    println!("  REG-008 Weather-region (cell, region) pairs: in both {} | XCLR only {} (centre inside polygon {}, within RPLI fall-off of an edge {}) | polygon only: centre inside {} , edge-grazing {}",
        total.pair_both, total.pair_xclr_only, total.pair_xclr_only_centre, total.pair_xclr_only_in_falloff, total.pair_poly_only_centre, total.pair_poly_only_graze);

    // REG-009: interior XCCM naming a region.
    let mut interior: HashMap<bool, usize> = HashMap::new();
    let mut with_weather = 0;
    let mut tied_overrides = 0;
    for cell in index.cells.cells.values() {
        if let Some(fid) = cell.sky_region {
            *interior.entry(regions.contains_key(&fid)).or_default() += 1;
            if let Some(r) = regions.get(&fid) {
                let s = weather_sections(r);
                if !s.is_empty() {
                    with_weather += 1;
                }
                if s.iter().filter(|x| x.1).count() > 1 {
                    tied_overrides += 1;
                }
            }
        }
    }
    println!("  REG-009 interior cells naming a region via XCCM: resolved {} unresolved {}; the region carries Weather {} ; >1 overriding section {}",
        interior.get(&true).unwrap_or(&0), interior.get(&false).unwrap_or(&0), with_weather, tied_overrides);
    let interior_climate = index.cells.cells.values().filter(|c| c.climate_override.is_some()).count();
    println!("  interior cells with XCCM climate: {interior_climate} of {}", index.cells.cells.len());
    Ok(())
}
