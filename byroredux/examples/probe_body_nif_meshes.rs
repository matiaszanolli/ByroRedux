// #6090 probe: dump each mesh's node name + authored base-color texture
// for the FO3/FNV body NIFs, so the race-skin override filter is keyed to
// evidence instead of a remembered mesh-name list.
//
// Usage: cargo run -p byroredux --example probe_body_nif_meshes -- <bsa> <mesh_path>...

use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <bsa_path> <mesh_path> [mesh_path...]", args[0]);
        std::process::exit(1);
    }
    let bsa_path = &args[1];
    let bsa = byroredux_bsa::BsaArchive::open(std::path::Path::new(bsa_path))
        .expect("Failed to open BSA");
    for mesh_path in &args[2..] {
        let Ok(nif_data) = bsa.extract(mesh_path) else {
            println!("{mesh_path}: <not in archive>");
            continue;
        };
        let parsed = byroredux_nif::parse_nif(&nif_data).expect("Failed to parse NIF");
        let mut pool = byroredux_core::string::StringPool::new();
        let scene = byroredux_nif::import::import_nif_scene(&parsed, &mut pool);
        let base_of = |mesh: &byroredux_nif::import::ImportedMesh| {
            mesh.material
                .textures
                .base_color
                .and_then(|sym| pool.resolve(sym))
                .unwrap_or("<none>")
                .to_string()
        };
        println!("{mesh_path}:");
        for mesh in &scene.meshes {
            println!("  name={:?} base_color={:?}", mesh.name.as_deref().unwrap_or("<none>"), base_of(mesh));
        }
    }
}
