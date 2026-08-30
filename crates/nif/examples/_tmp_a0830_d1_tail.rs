use byroredux_bsa::BsaArchive;
fn main() {
    let a = BsaArchive::open(
        "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/Oblivion - Meshes.bsa",
    )
    .unwrap();
    for f in [
        "meshes\\landscape\\lod\\60.-96.-64.32.nif",
        "meshes\\landscape\\lod\\60.-32.00.32.nif",
        "meshes\\landscape\\lod\\60.-64.-64.32.nif",
        "meshes\\landscape\\lod\\60.00.00.32.nif",
    ] {
        match a.extract(f) {
            Ok(b) => {
                let n = b.len();
                println!("{f} len={n}");
                println!("  head: {:02x?}", &b[..24.min(n)]);
                println!("  tail64: {:02x?}", &b[n.saturating_sub(64)..]);
            }
            Err(e) => println!("{f} ERR {e}"),
        }
    }
}
