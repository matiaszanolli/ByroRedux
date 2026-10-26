//! Find walkable shore transects into swimmable water, from shipped data.
//!
//! The WATAL W1 gate (`docs/smoke-tests/w1-water-traversal.sh`) walks a real
//! character capsule from dry land into water, swims and dives, climbs back
//! out, and then keeps walking inland across a cell edge. A title only gets a
//! route once one has been *measured*, and the measurable part of "a shore a
//! capsule can use" is the LAND heightfield: a gentle monotone descent from dry
//! ground into water deep enough to swim, with gentle dry ground continuing
//! inland past a cell border. This reads the LAND/XCLW/WRLD bytes the cell
//! loader reads and ranks every candidate transect at once. Statics (rocks,
//! trees, bank clutter) are not in the heightfield, so a result is a candidate
//! to *walk live*, not a verdict (`docs/smoke-tests/fixtures/skyrim_se.env`
//! records what a clutter-wedged bank looks like).
//!
//! Coordinates in the output are renderer coordinates (`x`, `y`, `z = -world_y`),
//! the form `--camera-pos` / `cam.pos` take; `fwd` is the unit direction from
//! the dry start toward the water in the same frame.
//!
//! Usage:
//!   cargo run --release -p byroredux-plugin --example shore_probe -- \
//!       <ESM> <WORLD_SUBSTR> [TOP=20] [MIN_DEPTH=160] [MAX_SLOPE=0.3] [X0,Y0,X1,Y1]

use byroredux_plugin::esm;
use std::collections::HashMap;

const SIDE: usize = 33;
const SPACING: f32 = 128.0;
const CELL: f32 = 4096.0;
/// Steps between the dry start and the first wet vertex we accept, at most.
const MAX_APPROACH_STEPS: usize = 6;
/// Inland steps that must stay gentle and dry (1 024 BU), crossing a cell edge.
const INLAND_STEPS: usize = 8;

type Vertex = (i32, i32);

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(esm_path), Some(world_sub)) = (args.first(), args.get(1)) else {
        anyhow::bail!("usage: shore_probe ESM WORLD_SUBSTR [TOP] [MIN_DEPTH] [MAX_SLOPE] [X0,Y0,X1,Y1]");
    };
    let world_sub = world_sub.to_ascii_lowercase();
    let top: usize = args.get(2).map_or(Ok(20), |s| s.parse())?;
    let min_depth: f32 = args.get(3).map_or(Ok(160.0), |s| s.parse())?;
    let max_slope: f32 = args.get(4).map_or(Ok(0.3), |s| s.parse())?;
    let window: Option<[i32; 4]> = args.get(5).map(|s| {
        let v: Vec<i32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
        [v[0], v[1], v[2], v[3]]
    });

    let bytes = std::fs::read(esm_path)?;
    let index = esm::records::parse_esm(&bytes)?;

    for (wkey, cells) in &index.cells.exterior_cells {
        if !wkey.to_ascii_lowercase().contains(&world_sub) {
            continue;
        }
        let default_height = index
            .cells
            .worldspaces
            .get(wkey)
            .and_then(|w| w.default_water_height.or_else(|| w.water_form.map(|_| 0.0)));

        // World vertex lattice (32 vertices per cell; a cell's last row/column
        // is the neighbour's first, so first writer wins).
        let mut height: HashMap<Vertex, f32> = HashMap::new();
        let mut surface_at: HashMap<Vertex, f32> = HashMap::new();
        for (&(gx, gy), cell) in cells {
            let Some(land) = cell.landscape.as_ref() else { continue };
            if land.heights.len() < SIDE * SIDE {
                continue;
            }
            let surface = if cell.water_height_is_explicit {
                cell.water_height
            } else {
                cell.water_height.or(default_height)
            };
            for r in 0..SIDE {
                for c in 0..SIDE {
                    let v = (gx * 32 + c as i32, gy * 32 + r as i32);
                    height.entry(v).or_insert(land.heights[r * SIDE + c]);
                    if let Some(s) = surface {
                        surface_at.entry(v).or_insert(s);
                    }
                }
            }
        }

        // (score, start vertex, direction (dx, dy) in lattice steps, lake surface, depth reached)
        let mut found: Vec<(f32, Vertex, (i32, i32), f32, f32, usize)> = Vec::new();
        for (&(gx, gy), cell) in cells {
            if let Some([x0, y0, x1, y1]) = window {
                if gx < x0 || gx > x1 || gy < y0 || gy > y1 {
                    continue;
                }
            }
            let surface = if cell.water_height_is_explicit {
                cell.water_height
            } else {
                cell.water_height.or(default_height)
            };
            let (Some(surface), Some(_)) = (surface, cell.landscape.as_ref()) else {
                continue;
            };
            for r in 0..SIDE as i32 - 1 {
                for c in 0..SIDE as i32 - 1 {
                    let start = (gx * 32 + c, gy * 32 + r);
                    let Some(&h0) = height.get(&start) else { continue };
                    // A dry start a little above the waterline.
                    if !(surface + 24.0..=surface + 140.0).contains(&h0) {
                        continue;
                    }
                    for dir in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        if let Some(t) =
                            evaluate(&height, start, dir, surface, min_depth, max_slope)
                        {
                            found.push((t.0, start, dir, surface, t.1, t.2));
                        }
                    }
                }
            }
        }
        found.sort_by(|a, b| b.0.total_cmp(&a.0));
        println!("=== {wkey}: {} shore transects (depth >= {min_depth}, slope <= {max_slope})", found.len());
        let mut shown_cells: HashMap<(i32, i32), usize> = HashMap::new();
        let mut printed = 0usize;
        for (score, (vx, vy), (dx, dy), surface, depth, wet_steps) in &found {
            // At most 2 per cell so one lake edge does not fill the list.
            let cell = (vx.div_euclid(32), vy.div_euclid(32));
            let n = shown_cells.entry(cell).or_insert(0);
            if *n >= 2 {
                continue;
            }
            *n += 1;
            let wx = *vx as f32 * SPACING;
            let wy = *vy as f32 * SPACING;
            let h0 = height[&(*vx, *vy)];
            println!(
                "  cell ({:3},{:3}) score={score:5.2} start x={wx:.0} y={:.0} z={:.0} ground={h0:.0} fwd=({},{},{}) surface={surface:.0} depth={depth:.0} wet_steps={wet_steps}",
                cell.0,
                cell.1,
                surface,
                -wy,
                dx,
                0,
                -dy,
            );
            printed += 1;
            if printed >= top {
                break;
            }
        }
    }
    Ok(())
}

/// Walk `dir` from `start` and score it as a shore, or `None`.
///
/// Requires, in order: a gentle monotone descent into water at least
/// `min_depth` below the lake surface, then — walking the *opposite* way from
/// `start` — `INLAND_STEPS` of gentle ground that stay above the waterline and
/// cross a cell edge.
fn evaluate(
    height: &HashMap<Vertex, f32>,
    start: Vertex,
    dir: (i32, i32),
    surface: f32,
    min_depth: f32,
    max_slope: f32,
) -> Option<(f32, f32, usize)> {
    let at = |k: i32| -> Option<f32> { height.get(&(start.0 + dir.0 * k, start.1 + dir.1 * k)).copied() };

    // Seaward: strictly gentle, never rising by more than a hair.
    let mut prev = at(0)?;
    let mut first_wet = None;
    let mut worst = 0.0f32;
    let mut deepest = 0.0f32;
    for k in 1..=24 {
        let h = at(k)?;
        let slope = (prev - h) / SPACING; // positive = descending
        if slope > max_slope || slope < -0.05 {
            return None;
        }
        worst = worst.max(slope.abs());
        if first_wet.is_none() && h < surface {
            first_wet = Some(k as usize);
        }
        if let Some(fw) = first_wet {
            if k as usize - fw >= 1 && h > surface - 8.0 {
                return None; // it came back above the water: not a basin
            }
        }
        deepest = deepest.max(surface - h);
        prev = h;
    }
    let first_wet = first_wet?;
    if first_wet > MAX_APPROACH_STEPS || deepest < min_depth {
        return None;
    }

    // Inland: gentle, dry, and crossing a cell edge.
    let mut prev = at(0)?;
    let mut crosses = false;
    for k in 1..=INLAND_STEPS as i32 {
        let v = (start.0 - dir.0 * k, start.1 - dir.1 * k);
        let h = *height.get(&v)?;
        if h < surface + 8.0 || ((h - prev) / SPACING).abs() > max_slope * 1.5 {
            return None;
        }
        let before = (start.0 - dir.0 * (k - 1), start.1 - dir.1 * (k - 1));
        if (before.0.div_euclid(32), before.1.div_euclid(32)) != (v.0.div_euclid(32), v.1.div_euclid(32)) {
            crosses = true;
        }
        worst = worst.max(((h - prev) / SPACING).abs());
        prev = h;
    }
    if !crosses {
        return None;
    }
    // Gentler is better; deeper (up to a swim-worthy 400 BU) is better.
    let score = 2.0 - worst / max_slope + (deepest.min(400.0) / 400.0);
    Some((score, deepest, first_wet))
}
