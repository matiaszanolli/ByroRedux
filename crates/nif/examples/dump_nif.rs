// Throwaway: extract a NIF from BSA and write to disk.
use byroredux_bsa::BsaArchive;
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let archive = BsaArchive::open(&args[1]).expect("open");
    let data = archive.extract(&args[2]).expect("extract");
    std::fs::write(&args[3], data).expect("write");
}
