use byroredux_bsa::Ba2Archive;
fn main() {
    let mut args = std::env::args().skip(1);
    let ba2 = args.next().unwrap();
    let a = Ba2Archive::open(&ba2).unwrap();
    for f in args {
        match a.extract(&f) {
            Ok(b) => {
                println!("--- {f} ({} bytes) ---", b.len());
                let s = String::from_utf8_lossy(&b[..b.len().min(600)]);
                println!("{s}");
            }
            Err(e) => println!("--- {f}: ERR {e}"),
        }
    }
}
