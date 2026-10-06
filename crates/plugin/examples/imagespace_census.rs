//! Image-space census for the FO3/FNV per-worldspace colour grade.
//!
//! Prints, per worldspace: its `INAM` image space (decoded IMGS grade), its
//! climate's weathers, and each weather's per-time-of-day IMAD slots with the
//! IMAD's cinematic (saturation / brightness / contrast mult+add) and tint
//! keys — the data that gives Anchorage, The Pitt, Point Lookout and the
//! Mojave their authored look.
//!
//! Each file is parsed on its own (no load-order remap). A DLC's own forms
//! carry its master count as the top byte; forms it borrows from the master
//! carry `00` and are looked up in the master index (the first argument).
//!
//! Usage:
//!   cargo run --release -p byroredux-plugin --example imagespace_census -- <master.esm> [dlc.esm...]

use byroredux_plugin::esm::records::{parse_esm, EsmIndex};

fn keys(k: &[byroredux_plugin::esm::records::ImadScalarKey]) -> String {
    k.iter()
        .map(|k| format!("{:.2}@{:.2}", k.value, k.time))
        .collect::<Vec<_>>()
        .join(",")
}

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    assert!(!paths.is_empty(), "usage: imagespace_census <master.esm> [dlc.esm...]");
    let indices: Vec<EsmIndex> = paths
        .iter()
        .map(|p| parse_esm(&std::fs::read(p).expect("read esm")).expect("parse esm"))
        .collect();
    let master = &indices[0];

    for (path, index) in paths.iter().zip(&indices) {
        println!("\n=== {path} ===");
        let mut worlds: Vec<_> = index.cells.worldspaces.values().collect();
        worlds.sort_by(|a, b| a.editor_id.cmp(&b.editor_id));
        for w in worlds {
            let key = w.editor_id.to_ascii_lowercase();
            let imgs = w.image_space_form.and_then(|f| {
                index.image_spaces.get(&f).or_else(|| master.image_spaces.get(&f))
            });
            println!(
                "WRLD {:28} parent={:08X?} pnam={:04X} INAM={:08X?} {} {:?}",
                w.editor_id,
                w.parent_worldspace,
                w.parent_flags,
                w.image_space_form,
                imgs.map(|i| i.editor_id.as_str()).unwrap_or("-"),
                imgs.and_then(|i| i.image_space),
            );
            let Some(clmt_id) = index.cells.worldspace_climates.get(&key) else {
                continue;
            };
            let clmt = index.climates.get(clmt_id).or_else(|| master.climates.get(clmt_id));
            let Some(clmt) = clmt else {
                println!("   CLMT {clmt_id:08X} unresolved");
                continue;
            };
            println!("   CLMT {} ({} weathers)", clmt.editor_id, clmt.weathers.len());
            for cw in &clmt.weathers {
                let wthr = index
                    .weathers
                    .get(&cw.weather_form_id)
                    .or_else(|| master.weathers.get(&cw.weather_form_id));
                let Some(wthr) = wthr else {
                    println!("     WTHR {:08X} unresolved", cw.weather_form_id);
                    continue;
                };
                println!("     WTHR {:32} chance={}", wthr.editor_id, cw.chance);
                for (slot, form) in wthr.image_space_modifiers.iter().enumerate() {
                    let Some(form) = form else { continue };
                    let imad = index
                        .imagespace_modifiers
                        .get(form)
                        .or_else(|| master.imagespace_modifiers.get(form));
                    match imad {
                        Some(m) => println!(
                            "       slot{slot} {:28} flags={} dur={:.2} sat*[{}]+[{}] bri*[{}]+[{}] con*[{}]+[{}] tint={:?}",
                            m.editor_id,
                            m.flags,
                            m.duration_seconds,
                            keys(&m.saturation_mult),
                            keys(&m.saturation_add),
                            keys(&m.brightness_mult),
                            keys(&m.brightness_add),
                            keys(&m.contrast_mult),
                            keys(&m.contrast_add),
                            m.tint_color.iter().map(|k| (k.time, k.color)).collect::<Vec<_>>(),
                        ),
                        None => println!("       slot{slot} {form:08X} unresolved"),
                    }
                }
            }
        }
    }
}
