//! Find open-water capture poses in an exterior worldspace from shipped data.
//!
//! WATAL W0/W2 fixtures need a camera column that is genuinely wet: the LAND
//! heightfield must sit well below the cell's resolved water height there and
//! around it. Teleport-probing columns in a live engine answers this one
//! column at a time; this reads the same LAND/XCLW/WRLD bytes the cell loader
//! does and ranks every cell at once.
//!
//! For each cell with resolved water (explicit XCLW, else the WRLD default —
//! Oblivion's NAM2-only worldspaces resolve to Z=0 as in
//! `default_water_for_worldspace`), it counts LAND vertices whose whole 3×3
//! neighbourhood is at least `MIN_DEPTH` below the surface ("open water") and
//! reports the deepest such vertex in *renderer* coordinates
//! (`x`, `y = water height`, `z = -world_y`) — the form `cam.pos` takes.
//!
//! Usage:
//!   cargo run --release -p byroredux-plugin --example water_fixture_probe -- \
//!       <ESM> <WORLD_SUBSTR> [MIN_DEPTH=200] [TOP=20] [X0,Y0,X1,Y1]

use byroredux_plugin::esm;

const SIDE: usize = 33;
const SPACING: f32 = 128.0;
const CELL: f32 = 4096.0;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(esm_path), Some(world_sub)) = (args.first(), args.get(1)) else {
        anyhow::bail!(
            "usage: water_fixture_probe ESM WORLD_SUBSTR [MIN_DEPTH] [TOP] [X0,Y0,X1,Y1]"
        );
    };
    let world_sub = world_sub.to_ascii_lowercase();
    let min_depth: f32 = args.get(2).map_or(Ok(200.0), |s| s.parse())?;
    let top: usize = args.get(3).map_or(Ok(20), |s| s.parse())?;
    let window: Option<[i32; 4]> = args.get(4).map(|s| {
        let v: Vec<i32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
        [v[0], v[1], v[2], v[3]]
    });

    let bytes = std::fs::read(esm_path)?;
    let index = esm::records::parse_esm(&bytes)?;

    for (wkey, cells) in &index.cells.exterior_cells {
        if !wkey.to_ascii_lowercase().contains(&world_sub) {
            continue;
        }
        let wrld = index.cells.worldspaces.get(wkey);
        let default_height =
            wrld.and_then(|w| w.default_water_height.or_else(|| w.water_form.map(|_| 0.0)));

        // (open vertices, grid, deepest depth, renderer pos, water form, explicit)
        let mut rows = Vec::new();
        let (mut with_land, mut with_water, mut explicit_water) = (0usize, 0usize, 0usize);
        for (&(gx, gy), cell) in cells {
            with_land += usize::from(cell.landscape.is_some());
            with_water += usize::from(cell.water_height.is_some());
            explicit_water += usize::from(cell.water_height_is_explicit);
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
            let (Some(surface), Some(land)) = (surface, cell.landscape.as_ref()) else {
                continue;
            };
            if land.heights.len() < SIDE * SIDE {
                continue;
            }
            let depth = |r: usize, c: usize| surface - land.heights[r * SIDE + c];
            let mut open = 0usize;
            let mut best: Option<(f32, usize, usize)> = None;
            for r in 1..SIDE - 1 {
                for c in 1..SIDE - 1 {
                    let shallowest = (r - 1..=r + 1)
                        .flat_map(|rr| (c - 1..=c + 1).map(move |cc| (rr, cc)))
                        .map(|(rr, cc)| depth(rr, cc))
                        .fold(f32::INFINITY, f32::min);
                    if shallowest < min_depth {
                        continue;
                    }
                    open += 1;
                    if best.is_none_or(|(d, _, _)| shallowest > d) {
                        best = Some((shallowest, r, c));
                    }
                }
            }
            let Some((d, r, c)) = best else { continue };
            // LAND row 0 is the south edge; world Y grows north, renderer Z = -Y.
            let wx = gx as f32 * CELL + c as f32 * SPACING;
            let wy = gy as f32 * CELL + r as f32 * SPACING;
            rows.push((
                open,
                (gx, gy),
                d,
                [wx, surface, -wy],
                cell.water_type_form,
                cell.water_height_is_explicit,
            ));
        }
        rows.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        println!(
            "=== {wkey}: {} cells with open water >= {min_depth} deep (WRLD default {:?} watr {}; {} cells, {with_land} LAND, {with_water} water heights, {explicit_water} explicit XCLW)",
            rows.len(),
            default_height,
            wrld.and_then(|w| w.water_form)
                .map_or_else(|| "-".to_string(), |f| format!("{f:08X}")),
            cells.len()
        );
        for (open, (gx, gy), d, p, watr, explicit) in rows.iter().take(top) {
            let watr = watr
                .and_then(|f| {
                    index
                        .waters
                        .get(&f)
                        .map(|w| format!("{f:08X} {}", w.editor_id))
                })
                .unwrap_or_else(|| "inherit".into());
            println!(
                "  ({gx:4},{gy:4}) open={open:4} deepest={d:7.0} cam.pos {:.0} {:.0} {:.0}  surface={}{} watr={watr}",
                p[0],
                p[1],
                p[2],
                p[1],
                if *explicit { "" } else { " (inherited)" }
            );
        }
    }
    Ok(())
}
