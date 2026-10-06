//! Print the climate-resolution chain for one exterior cell: its XCLR
//! regions, each region's CNAM climate, and the climate's weathers — the
//! lane `build_exterior_world_context` resolves a worldspace's climate
//! through when the WRLD authors none (vanilla Oblivion Tamriel).
//!
//! Usage: cell_climate_probe <esm> <worldspace> <grid-x> <grid-y>
use byroredux_plugin::esm::records::parse_esm;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("esm");
    let wrld = args.next().expect("worldspace").to_ascii_lowercase();
    let gx: i32 = args.next().expect("grid-x").parse().expect("int");
    let gy: i32 = args.next().expect("grid-y").parse().expect("int");
    let index = parse_esm(&std::fs::read(&path).unwrap()).unwrap();

    let worldspace_climate = index.cells.worldspace_climates.get(&wrld);
    println!("WRLD {wrld} own climate: {worldspace_climate:?}");
    let Some(cells) = index.cells.exterior_cells.get(&wrld) else {
        println!("no exterior cells for {wrld}");
        return;
    };
    println!("cells in worldspace: {}", cells.len());
    let mut weather_regions = 0; let mut climate_regions = 0;
    for r in index.regions.values() {
        if r.weather_form.is_some() || !r.entries_by_priority(byroredux_plugin::esm::records::RegionDataKind::Weather).is_empty() { weather_regions += 1; }
        if r.climate_form.is_some() { climate_regions += 1; }
    }
    println!("regions total={} with-weather-data={} with-climate={}", index.regions.len(), weather_regions, climate_regions);
    for w in index.weathers.values() {
        println!("  WTHR {:08X} {}", w.form_id, w.editor_id);
    }
    for offset in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
        let key = (gx + offset.0, gy + offset.1);
        let Some(cell) = cells.get(&key) else {
            println!("cell {key:?}: absent");
            continue;
        };
        println!("cell {key:?}: {} regions", cell.regions.len());
        for region_fid in &cell.regions {
            match index.regions.get(region_fid) {
                Some(region) => {
                    let climate = region
                        .climate_form
                        .and_then(|fid| index.climates.get(&fid));
                    let weather = region
                        .weather_form
                        .and_then(|fid| index.weathers.get(&fid));
                    let rdwt: Vec<String> = region
                        .entries_by_priority(byroredux_plugin::esm::records::RegionDataKind::Weather)
                        .iter()
                        .map(|e| format!("{:?}", e))
                        .collect();
                    println!(
                        "  REGN {:08X} '{}' climate={} wnam={} rdwt={}",
                        region_fid,
                        region.editor_id,
                        climate
                            .map(|c| format!(
                                "Some({:08X} {} with {} weathers)",
                                c.form_id,
                                c.editor_id,
                                c.weathers.len()
                            ))
                            .unwrap_or_else(|| format!("None (form {:08X?})", region.climate_form)),
                        weather
                            .map(|w| format!("Some({} {:08X})", w.editor_id, w.form_id))
                            .unwrap_or_else(|| format!("None (form {:08X?})", region.weather_form)),
                        rdwt.join(","),
                    );
                }
                None => println!("  REGN {region_fid:08X}: unparsed"),
            }
        }
    }
}
