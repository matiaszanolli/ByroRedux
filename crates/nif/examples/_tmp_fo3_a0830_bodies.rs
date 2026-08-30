//! TEMP scratch (audit 2026-08-30): FO3 humanoid body/hand/skeleton asset
//! existence + parse check against the real archive.
use byroredux_bsa::BsaArchive;
fn main() {
    let arc = BsaArchive::open(
        "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data/Fallout - Meshes.bsa",
    )
    .unwrap();
    let files = arc.list_files();
    for want in [
        r"meshes\characters\_male\skeleton.nif",
        r"meshes\characters\_male\upperbody.nif",
        r"meshes\characters\_male\lefthand.nif",
        r"meshes\characters\_male\righthand.nif",
        r"meshes\characters\_male\femaleupperbody.nif",
        r"meshes\characters\_male\femalelefthand.nif",
        r"meshes\characters\_male\femalerighthand.nif",
        r"meshes\characters\_male\childupperbody.nif",
        r"meshes\characters\_male\childfemaleupperbody.nif",
    ] {
        let w = want.to_ascii_lowercase();
        match files.iter().find(|f| f.to_ascii_lowercase() == w) {
            Some(n) => {
                let b = arc.extract(n).unwrap();
                let s = byroredux_nif::parse_nif(&b);
                let (ok, skinned) = match &s {
                    Ok(sc) => {
                        let mut pool = byroredux_core::string::StringPool::new();
                        let ms = byroredux_nif::import::import_nif(sc, &mut pool);
                        (true, ms.iter().filter(|m| m.skin.is_some()).count())
                    }
                    Err(_) => (false, 0),
                };
                println!(
                    "{want}: PRESENT {} B parse_ok={ok} skinned_meshes={skinned}",
                    b.len()
                );
            }
            None => println!("{want}: MISSING"),
        }
    }
}
