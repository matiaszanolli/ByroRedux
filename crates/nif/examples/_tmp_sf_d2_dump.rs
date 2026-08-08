use byroredux_bsa::Ba2Archive;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ar = Ba2Archive::open(&a[1]).expect("open");
    let d = ar.extract(&a[2]).expect("extract");
    std::fs::write(&a[3], d).expect("write");
}
