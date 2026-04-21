use byroredux_bsa::BsaArchive;
use std::fs;
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bsa = &args[1];
    let target = args[2].to_ascii_lowercase();
    let out = &args[3];
    let archive = BsaArchive::open(bsa).unwrap();
    for f in archive.list_files() {
        if f.to_ascii_lowercase() == target {
            let data = archive.extract(&f).unwrap();
            fs::write(out, &data).unwrap();
            println!("wrote {} bytes to {}", data.len(), out);
            return;
        }
    }
    eprintln!("not found: {}", target);
}
