use byroredux_bsa::BsaArchive;
fn main() {
    let a = BsaArchive::open(
        "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/Oblivion - Meshes.bsa",
    )
    .unwrap();
    let f = std::env::args().nth(1).unwrap();
    let b = a.extract(&f).unwrap();
    std::fs::write("/tmp/audit/nif/d1/dump.bin", &b).unwrap();
    println!("len={}", b.len());
}
