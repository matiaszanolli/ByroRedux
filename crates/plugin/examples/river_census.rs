//! Census how an exterior worldspace authors moving water: per-cell water
//! height / WATR type, plus every placed reference whose model
//! is water geometry or a river / rapids / waterfall effect mesh.
//!
//! Evidence harness for WATAL W2 (rivers, rapids, falls). Answers, from
//! shipped data rather than assumption: do descending rivers step the cell
//! XCLW height, or ride on placed water meshes above it; and which effect
//! meshes accompany them. (Water-current arrays: `water_current_census.rs`.)
//!
//! Usage:
//!   cargo run --release -p byroredux-plugin --example river_census -- <ESM> [WORLD_SUBSTR]

use byroredux_plugin::esm;
use std::collections::{BTreeMap, HashMap};

/// Coarse category for a placed model path. Tokens mirror the shipped
/// Skyrim mesh census (meshes\water\*, meshes\effects\fx{rapids,waterfall,
/// creek,splash}*, fxwaterstream*); anything else is ignored.
fn category(model: &str) -> Option<&'static str> {
    let m = model.to_ascii_lowercase();
    if m.contains("fxwaterfall") || m.contains("waterfall") {
        Some("waterfall-fx")
    } else if m.contains("fxrapids") || m.contains("rapids") {
        Some("rapids-fx")
    } else if m.contains("fxcreek") || m.contains("fxwaterstream") {
        Some("stream-fx")
    } else if m.contains("fxsplash") || m.contains("churn") {
        Some("splash-fx")
    } else if m.starts_with("water\\") || m.contains("\\water\\") {
        Some("water-mesh")
    } else {
        None
    }
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let esm_path = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: river_census ESM [WORLD_SUBSTR]"))?;
    let world_sub = args.next().unwrap_or_default().to_ascii_lowercase();
    // Optional `X,Y`: print every water/fx placement in that one cell with
    // its world position and rotation, for freezing capture poses.
    let focus: Option<(i32, i32)> = args.next().and_then(|g| {
        let (x, y) = g.split_once(',')?;
        Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
    });

    let bytes = std::fs::read(&esm_path)?;
    let index = esm::records::parse_esm(&bytes)?;

    let watr_edid = |form: Option<u32>| -> String {
        form.map(|f| {
            index
                .waters
                .get(&f)
                .map(|w| w.editor_id.clone())
                .unwrap_or_else(|| format!("{f:08X}?"))
        })
        .unwrap_or_else(|| "-".to_string())
    };

    for (wkey, cells) in &index.cells.exterior_cells {
        if !world_sub.is_empty() && !wkey.to_ascii_lowercase().contains(&world_sub) {
            continue;
        }

        let mut watr_hist: BTreeMap<String, usize> = BTreeMap::new();
        let mut explicit_heights: Vec<f32> = Vec::new();
        // model path → (count, min z, max z, sum of (z - cell water height))
        let mut models: BTreeMap<String, (usize, f32, f32)> = BTreeMap::new();
        let mut cat_totals: BTreeMap<&'static str, usize> = BTreeMap::new();
        let mut rows: Vec<String> = Vec::new();

        let mut grids: Vec<_> = cells.keys().copied().collect();
        grids.sort_by_key(|g| (g.1, g.0));
        for grid in grids {
            let cell = &cells[&grid];
            let watr = watr_edid(cell.water_type_form);
            *watr_hist.entry(watr.clone()).or_default() += 1;
            if cell.water_height_is_explicit {
                if let Some(h) = cell.water_height {
                    explicit_heights.push(h);
                }
            }

            let mut per_cat: HashMap<&'static str, Vec<f32>> = HashMap::new();
            for r in &cell.references {
                let Some(stat) = index.cells.statics.get(&r.base_form_id) else {
                    continue;
                };
                let Some(cat) = category(&stat.model_path) else {
                    continue;
                };
                let z = r.position[2];
                if focus == Some(grid) {
                    println!(
                        "  focus ({},{}) {cat:<12} pos=[{:.0}, {:.0}, {:.0}] base={:08X} {} {}",
                        grid.0,
                        grid.1,
                        r.position[0],
                        r.position[1],
                        r.position[2],
                        r.base_form_id,
                        stat.editor_id,
                        stat.model_path
                    );
                }
                per_cat.entry(cat).or_default().push(z);
                *cat_totals.entry(cat).or_default() += 1;
                let e = models
                    .entry(stat.model_path.to_ascii_lowercase())
                    .or_insert((0, f32::INFINITY, f32::NEG_INFINITY));
                e.0 += 1;
                e.1 = e.1.min(z);
                e.2 = e.2.max(z);
            }
            if per_cat.is_empty() {
                continue;
            }
            let height = cell
                .water_height
                .map(|h| {
                    format!(
                        "{h:9.1}{}",
                        if cell.water_height_is_explicit {
                            ""
                        } else {
                            "i"
                        }
                    )
                })
                .unwrap_or_else(|| "      dry".to_string());
            let mut cats: Vec<_> = per_cat.iter().collect();
            cats.sort_by_key(|(k, _)| **k);
            let cat_text: Vec<String> = cats
                .iter()
                .map(|(k, zs)| {
                    let lo = zs.iter().copied().fold(f32::INFINITY, f32::min);
                    let hi = zs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                    format!("{k}×{} z[{lo:.0}..{hi:.0}]", zs.len())
                })
                .collect();
            rows.push(format!(
                "  ({:>4},{:>4}) h={height} watr={watr}  {}  edid='{}'",
                grid.0,
                grid.1,
                cat_text.join(" "),
                cell.editor_id
            ));
        }

        println!("=== worldspace '{wkey}' ({} ext cells)", cells.len());
        println!("-- WATR type per cell (XCWT; '-' = inherit WRLD default)");
        for (k, n) in &watr_hist {
            println!("  {n:>6}  {k}");
        }
        explicit_heights.sort_by(f32::total_cmp);
        explicit_heights.dedup_by(|a, b| (*a - *b).abs() < 0.5);
        println!(
            "-- distinct explicit XCLW heights: {} (sample: {:?})",
            explicit_heights.len(),
            explicit_heights
                .iter()
                .take(40)
                .map(|h| h.round() as i32)
                .collect::<Vec<_>>()
        );
        println!("-- placed water/fx references by category: {cat_totals:?}");
        println!("-- per model (count, z range)");
        for (m, (n, lo, hi)) in &models {
            println!("  {n:>5}  z[{lo:>8.0} .. {hi:>8.0}]  {m}");
        }
        println!("-- cells with water/fx placements ({})", rows.len());
        for r in &rows {
            println!("{r}");
        }
    }
    Ok(())
}
