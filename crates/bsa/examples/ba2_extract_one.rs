use byroredux_bsa::Ba2Archive;
use std::fs;
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let archive = Ba2Archive::open(&args[1]).unwrap();
    let target = args[2].to_ascii_lowercase();
    for f in archive.list_files() {
        if f.to_ascii_lowercase() == target {
            let data = archive.extract(&f).unwrap();
            fs::write(&args[3], &data).unwrap();
            println!("wrote {} bytes", data.len());
            return;
        }
    }
    eprintln!("not found: {}", target);
}
