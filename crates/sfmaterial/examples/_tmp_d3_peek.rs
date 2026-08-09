use byroredux_bsa::Ba2Archive;
use byroredux_sfmaterial::ComponentDatabaseFile;
fn main() {
    let d = "/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/";
    // real loose Starfield .mat (JSON)
    let a = Ba2Archive::open(format!("{d}qog-pawnshop - main.ba2")).unwrap();
    let m = a
        .extract("materials\\qog\\pawnshop\\galacticpawnshopterminal_screen.mat")
        .unwrap();
    println!(
        "loose .mat  peek_magic = {} (first4 = {:?})",
        ComponentDatabaseFile::peek_magic(&m),
        &m[..4]
    );
    // real FO4 BGSM
    let fo4 = "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data/Fallout4 - Materials.ba2";
    if let Ok(f) = Ba2Archive::open(fo4) {
        let first = f
            .list_files()
            .iter()
            .find(|p| p.to_ascii_lowercase().ends_with(".bgsm"))
            .map(|s| s.to_string());
        if let Some(p) = first {
            let b = f.extract(&p).unwrap();
            println!(
                "FO4 bgsm    peek_magic = {} (first4 = {:?}, {p})",
                ComponentDatabaseFile::peek_magic(&b),
                &b[..4]
            );
        }
        let firste = f
            .list_files()
            .iter()
            .find(|p| p.to_ascii_lowercase().ends_with(".bgem"))
            .map(|s| s.to_string());
        if let Some(p) = firste {
            let b = f.extract(&p).unwrap();
            println!(
                "FO4 bgem    peek_magic = {} (first4 = {:?})",
                ComponentDatabaseFile::peek_magic(&b),
                &b[..4]
            );
        }
    }
    // real base CDB
    let mba2 = Ba2Archive::open(format!("{d}Starfield - Materials.ba2")).unwrap();
    let c = mba2.extract("materials\\materialsbeta.cdb").unwrap();
    println!(
        "real CDB    peek_magic = {} (first4 = {:?})",
        ComponentDatabaseFile::peek_magic(&c),
        &c[..4]
    );
    // truncated CDB: header only
    println!(
        "CDB[..16]   peek_magic = {} probe = {:?}",
        ComponentDatabaseFile::peek_magic(&c[..16]),
        ComponentDatabaseFile::probe_header(&c[..16])
    );
}
