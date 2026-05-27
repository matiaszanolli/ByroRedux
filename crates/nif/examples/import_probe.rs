//! Scratch: parse + import a NIF and dump counts of meshes /
//! collisions / lights extracted. Used to investigate F4
//! (FO4 NIF parse/import failure rate, 2026-05-26).

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: import_probe <path.nif>");
    let bytes = std::fs::read(&path).expect("read");
    // Probe the header separately — the cell-loader's F4 fix
    // re-parses this to get bs_version for game-aware filtering.
    match byroredux_nif::header::NifHeader::parse(&bytes) {
        Ok((h, _)) => {
            println!("Header parse OK: user_version_2 (BSVER) = {}", h.user_version_2);
        }
        Err(e) => println!("Header parse FAIL: {}", e),
    }
    let scene = match byroredux_nif::parse_nif(&bytes) {
        Ok(s) => s,
        Err(e) => {
            println!("parse FAIL: {}", e);
            return;
        }
    };
    println!("parse OK: {} blocks", scene.blocks.len());

    let bsx = byroredux_nif::import::extract_bsx_flags(&scene);
    let root_flags = byroredux_nif::import::extract_root_flags(&scene);
    println!("BSXFlags:        0x{:08X}  (bit 5 = editor marker)", bsx);
    println!("Root NiAVObject.flags: 0x{:08X}", root_flags);

    let mut pool = byroredux_core::string::StringPool::new();
    let imported_scene = byroredux_nif::import::import_nif_scene(&scene, &mut pool);

    println!("nodes:           {}", imported_scene.nodes.len());
    println!("meshes:          {}", imported_scene.meshes.len());
    let nodes_with_collision = imported_scene
        .nodes
        .iter()
        .filter(|n| n.collision.is_some())
        .count();
    println!("collisions on nodes:  {}", nodes_with_collision);

    let lights = byroredux_nif::import::import_nif_lights(&scene);
    println!("lights:          {}", lights.len());

    let emitters = byroredux_nif::import::import_nif_particle_emitters(&scene);
    println!("particle emitters: {}", emitters.len());
}
