use byroredux_bsa::BsaArchive;
fn main() {
    let dir = std::env::var("BYROREDUX_OBLIVION_DATA")
        .unwrap_or_else(|_| "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data".into());
    let misc = BsaArchive::open(std::path::Path::new(&dir).join("Oblivion - Misc.bsa")).unwrap();
    let bytes = misc.extract("menus\\main\\hud_main_menu.xml").unwrap();
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].contains("hudmain_health_empty") {
            for l in &lines[i..(i + 40).min(lines.len())] {
                println!("{l}");
                if l.contains("hudmain_health_edge") { break; }
            }
            break;
        }
        i += 1;
    }
}
