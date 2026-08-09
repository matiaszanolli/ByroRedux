use byroredux_bsa::Ba2Archive;
use byroredux_sfmaterial::ComponentDatabaseFile;
fn main() {
    let a = std::env::args().nth(1).unwrap();
    let i = std::env::args().nth(2).unwrap();
    let ba2 = Ba2Archive::open(&a).unwrap();
    let bytes = ba2.extract(&i).unwrap();
    let n = bytes.len();
    let t = std::time::Instant::now();
    let cdb = ComponentDatabaseFile::parse(&bytes).unwrap();
    eprintln!(
        "parse-only: {n} bytes -> {} instances in {:?}",
        cdb.instances.len(),
        t.elapsed()
    );
    std::mem::forget(cdb);
}
