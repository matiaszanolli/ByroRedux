//! D5 audit helper: open a BA2 archive, report version+variant, and
//! attempt to extract the first 3 files.
use byroredux_bsa::Ba2Archive;

fn main() {
    for path in std::env::args().skip(1) {
        match Ba2Archive::open(&path) {
            Err(e) => {
                println!("OPEN FAIL {}: {}", path, e);
            }
            Ok(arc) => {
                let files = arc.list_files();
                let n = files.len();
                println!(
                    "OK {} v{} {:?} files={}",
                    path,
                    arc.version(),
                    arc.variant(),
                    n
                );
                let to_check = files
                    .iter()
                    .take(3)
                    .chain(files.iter().rev().take(2))
                    .collect::<Vec<_>>();
                for f in to_check {
                    match arc.extract(f) {
                        Ok(b) => println!("  OK   {} ({} bytes)", f, b.len()),
                        Err(e) => println!("  FAIL {}: {}", f, e),
                    }
                }
            }
        }
    }
}
