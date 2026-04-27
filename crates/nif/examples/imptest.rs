use byroredux_core::string::StringPool;
use byroredux_nif::{import::import_nif_scene, parse_nif};
fn main() {
    let mut pool = StringPool::new();
    for p in std::env::args().skip(1) {
        let bytes = std::fs::read(&p).expect("read");
        match parse_nif(&bytes) {
            Ok(scene) => {
                let imp = import_nif_scene(&scene, &mut pool);
                println!(
                    "[OK] {}: {} nodes, {} meshes, truncated={}",
                    p,
                    imp.nodes.len(),
                    imp.meshes.len(),
                    scene.truncated
                );
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
