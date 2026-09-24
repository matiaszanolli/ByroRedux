//! Count placed light-beam effect models by interior/exterior cell.
//!
//! `cargo run -p byroredux-plugin --example baked_beam_census -- <game.esm> [sample-model-filter] [sample-cell-filter]`

use std::collections::HashMap;

#[derive(Default)]
struct Counts {
    placed: usize,
    light_within_256: usize,
    light_within_512: usize,
    show_sky: usize,
}

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .expect("usage: baked_beam_census <game.esm>");
    let sample_filter = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "windowlightbeam".to_string())
        .to_ascii_lowercase();
    let sample_cell_filter = std::env::args()
        .nth(3)
        .map(|value| value.to_ascii_lowercase());
    let bytes = std::fs::read(path)?;
    let index = byroredux_plugin::esm::parse_esm(&bytes)?;
    let beams: HashMap<_, _> = index
        .cells
        .statics
        .iter()
        .filter(|(_, base)| {
            let path = base.model_path.to_ascii_lowercase();
            path.contains("lightbeam") || path.contains("godray")
        })
        .map(|(&fid, base)| (fid, base.model_path.as_str()))
        .collect();
    println!("{} light-beam base forms", beams.len());
    let mut base_forms: Vec<_> = beams.iter().collect();
    base_forms.sort_unstable_by_key(|(fid, _)| **fid);
    for (fid, path) in base_forms {
        println!("  {fid:08X} {path}");
    }
    let mut counts: HashMap<(&str, bool), Counts> = HashMap::new();
    let mut model_samples = 0usize;
    let mut busy_cells = Vec::new();
    let mut record = |cell: &byroredux_plugin::esm::cell::CellData, interior: bool| {
        let beam_refs: Vec<_> = cell
            .references
            .iter()
            .filter(|reference| beams.contains_key(&reference.base_form_id))
            .collect();
        let beam_count = beam_refs.len();
        if beam_count > 0 {
            let neighbors = [256.0f32, 512.0, 1000.0].map(|radius| {
                beam_refs
                    .iter()
                    .map(|center| {
                        beam_refs
                            .iter()
                            .filter(|other| {
                                center
                                    .position
                                    .iter()
                                    .zip(other.position)
                                    .map(|(a, b)| (a - b) * (a - b))
                                    .sum::<f32>()
                                    <= radius * radius
                            })
                            .count()
                    })
                    .max()
                    .unwrap_or(0)
            });
            busy_cells.push((
                beam_count,
                neighbors,
                interior,
                cell.show_sky,
                cell.editor_id.clone(),
                cell.lighting
                    .as_ref()
                    .map(|lighting| (lighting.fog_near, lighting.fog_far)),
            ));
        }
        let local_lights: Vec<_> = cell
            .references
            .iter()
            .filter(|reference| {
                index
                    .cells
                    .statics
                    .get(&reference.base_form_id)
                    .is_some_and(|base| base.light_data.is_some())
            })
            .collect();
        for reference in &cell.references {
            if let Some(&path) = beams.get(&reference.base_form_id) {
                let count = counts.entry((path, interior)).or_default();
                count.placed += 1;
                count.show_sky += usize::from(cell.show_sky == Some(true));
                let nearest_squared = local_lights
                    .iter()
                    .map(|light| {
                        light
                            .position
                            .iter()
                            .zip(reference.position)
                            .map(|(a, b)| (a - b) * (a - b))
                            .sum::<f32>()
                    })
                    .fold(f32::INFINITY, f32::min);
                count.light_within_256 += usize::from(nearest_squared <= 256.0 * 256.0);
                count.light_within_512 += usize::from(nearest_squared <= 512.0 * 512.0);
                if interior
                    && path.to_ascii_lowercase().contains(&sample_filter)
                    && sample_cell_filter
                        .as_ref()
                        .is_none_or(|filter| cell.editor_id.to_ascii_lowercase().contains(filter))
                    && model_samples < 20
                {
                    let nearest = local_lights.iter().min_by(|a, b| {
                        let distance = |light: &byroredux_plugin::esm::cell::PlacedRef| {
                            light
                                .position
                                .iter()
                                .zip(reference.position)
                                .map(|(x, y)| (x - y) * (x - y))
                                .sum::<f32>()
                        };
                        distance(a).total_cmp(&distance(b))
                    });
                    if let Some(light) = nearest {
                        let base = &index.cells.statics[&light.base_form_id];
                        println!(
                            "sample cell={} show_sky={:?} beam={:08X} pos={:?} rot={:?} nearest LIGH={:08X} {} dist={:.0}",
                            cell.editor_id,
                            cell.show_sky,
                            reference.form_id,
                            reference.position,
                            reference.rotation,
                            light.base_form_id,
                            base.editor_id,
                            nearest_squared.sqrt(),
                        );
                        model_samples += 1;
                    }
                }
            }
        }
    };
    for cell in index.cells.cells.values() {
        record(cell, true);
    }
    for cells in index.cells.exterior_cells.values() {
        for cell in cells.values() {
            record(cell, false);
        }
    }
    for cell in index.cells.worldspace_persistent_cells.values() {
        record(cell, false);
    }
    let mut counts: Vec<_> = counts.into_iter().collect();
    counts.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    for ((path, interior), count) in counts {
        println!(
            "{:>5} {} near LIGH 256/512: {:>5}/{:>5} show sky: {:>5} {path}",
            count.placed,
            if interior { "interior" } else { "exterior" },
            count.light_within_256,
            count.light_within_512,
            count.show_sky,
        );
    }
    busy_cells.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    println!("busiest beam cells:");
    for (count, neighbors, interior, show_sky, editor_id, fog) in busy_cells.into_iter().take(10) {
        println!(
            "  {count:>4} nearby-256/512/1000={neighbors:?} fog={fog:?} show_sky={show_sky:?} {} {editor_id}",
            if interior { "interior" } else { "exterior" }
        );
    }

    // Skyrim in particular authors indoor sunlight as LIGH placements
    // instead of STATIC beam cards. Keep these separate from the painted
    // mesh tally: they already supply real local illumination, but their
    // presence identifies cells where aperture-driven scattering matters.
    let sunlight_lights: HashMap<_, _> = index
        .cells
        .statics
        .iter()
        .filter(|(_, base)| {
            base.light_data.is_some() && base.editor_id.to_ascii_lowercase().contains("sunlight")
        })
        .map(|(&fid, base)| (fid, base))
        .collect();
    let mut sunlight_placements = Vec::new();
    for cell in index.cells.cells.values() {
        for reference in &cell.references {
            if let Some(base) = sunlight_lights.get(&reference.base_form_id) {
                sunlight_placements.push((
                    cell.editor_id.as_str(),
                    cell.show_sky,
                    base.editor_id.as_str(),
                    reference.form_id,
                    reference.position,
                    reference.rotation,
                    base.light_data.as_ref().unwrap().radius,
                    base.light_data.as_ref().unwrap().flags,
                    base.light_data.as_ref().unwrap().fov_degrees,
                ));
            }
        }
    }
    sunlight_placements.sort_unstable_by(|a, b| a.0.cmp(b.0).then(a.2.cmp(b.2)));
    println!(
        "{} sunlight LIGH bases, {} interior placements",
        sunlight_lights.len(),
        sunlight_placements.len()
    );
    for (cell, show_sky, light, fid, position, rotation, radius, flags, fov) in
        sunlight_placements
            .iter()
            .filter(|placement| {
                sample_cell_filter
                    .as_ref()
                    .is_none_or(|filter| placement.0.to_ascii_lowercase().contains(filter))
            })
            .take(30)
    {
        println!(
            "  {cell} show_sky={show_sky:?} {light} REFR={fid:08X} pos={position:?} rot={rotation:?} radius={radius:.0} flags={flags:08X} fov={fov:.1}"
        );
    }
    Ok(())
}
