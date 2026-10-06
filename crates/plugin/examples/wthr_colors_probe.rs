//! Print a weather's decoded NAM0 sky colours, cloud layers and fog, by EDID
//! substring. Usage: wthr_colors_probe <esm> <edid-substr>...
use byroredux_plugin::esm::records::parse_esm;
fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("esm");
    let filters: Vec<String> = args.collect();
    let index = parse_esm(&std::fs::read(&path).unwrap()).unwrap();
    let groups = ["sky_upper","fog","clouds_lower","ambient","sunlight","sun","stars","sky_lower","horizon","clouds_upper"];
    for w in index.weathers.values() {
        if !filters.iter().any(|f| w.editor_id.contains(f.as_str())) { continue; }
        println!("== {} class={:#x} fog day {}..{} night {}..{}", w.editor_id, w.classification, w.fog_day_near, w.fog_day_far, w.fog_night_near, w.fog_night_far);
        for (g, row) in w.sky_colors.iter().enumerate() {
            let cells: Vec<String> = row.iter().map(|c| format!("{:3},{:3},{:3}", c.r, c.g, c.b)).collect();
            println!("  {:13} {}", groups.get(g).unwrap_or(&"?"), cells.join(" | "));
        }
        println!("  clouds {:?}", w.cloud_textures);
        for (l, row) in w.cloud_layer_colors.iter().enumerate() {
            let cells: Vec<String> = row.iter().map(|c| format!("{:3},{:3},{:3},{:3}", c.r, c.g, c.b, c.a)).collect();
            println!("  cloud{l} {}", cells.join(" | "));
        }
    }
}
