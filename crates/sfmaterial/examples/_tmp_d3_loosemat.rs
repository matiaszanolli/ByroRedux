use byroredux_bsa::Ba2Archive;
fn main() {
    let d = "/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/";
    for f in [
        "qog-pawnshop - main.ba2",
        "avontechshipyards - main.ba2",
        "sp2_factionrequisitionkiosks - main.ba2",
    ] {
        let a = Ba2Archive::open(format!("{d}{f}")).unwrap();
        let mats: Vec<String> = a
            .list_files()
            .iter()
            .filter(|p| p.to_ascii_lowercase().ends_with(".mat"))
            .map(|s| s.to_string())
            .collect();
        println!("== {f}: {} loose .mat", mats.len());
        for m in mats.iter().take(2) {
            let b = a.extract(m).unwrap();
            let head = String::from_utf8_lossy(&b[..b.len().min(220)]);
            println!(
                "   {m} ({} bytes)\n     {}",
                b.len(),
                head.replace('\n', " ")
            );
        }
    }
}
