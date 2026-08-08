//! D7 audit helper: full extraction sweep over a BA2 archive, all files.
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
                let mut ok = 0usize;
                let mut fail = 0usize;
                let mut fails: Vec<String> = Vec::new();
                for f in files.iter() {
                    match arc.extract(f) {
                        Ok(_) => ok += 1,
                        Err(e) => {
                            fail += 1;
                            if fails.len() < 10 {
                                fails.push(format!("{}: {}", f, e));
                            }
                        }
                    }
                }
                println!(
                    "{} v{} {:?} files={} ok={} fail={} rate={:.4}%",
                    path,
                    arc.version(),
                    arc.variant(),
                    n,
                    ok,
                    fail,
                    100.0 * ok as f64 / n.max(1) as f64
                );
                for f in fails {
                    println!("  FAIL {}", f);
                }
            }
        }
    }
}
