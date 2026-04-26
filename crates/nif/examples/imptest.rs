use byroredux_nif::{parse_nif, import::import_nif_scene};
fn main() {
    for p in std::env::args().skip(1) {
        let bytes = std::fs::read(&p).expect("read");
        match parse_nif(&bytes) {
            Ok(scene) => {
                let imp = import_nif_scene(&scene);
                println!("[OK] {}: {} nodes, {} meshes, truncated={}",
                          p, imp.nodes.len(), imp.meshes.len(), scene.truncated);
                for m in &imp.meshes {
                    if m.positions.is_empty() {
                        println!("  [WARN] mesh '{:?}' has 0 vertices", m.name);
                    }
                }
            }
            Err(e) => println!("[PARSE-FAIL] {}: {:?}", p, e),
        }
    }
}
