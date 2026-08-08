use byroredux_bsa::Ba2Archive;
use byroredux_core::string::StringPool;
use byroredux_nif::import::import_nif_scene;
use byroredux_nif::parse_nif;

fn trace_one(arc: &Ba2Archive, path: &str, pool: &mut StringPool) {
    let Ok(bytes) = arc.extract(path) else { println!("  [MISS] {path}"); return; };
    let scene = match parse_nif(&bytes) { Ok(s) => s, Err(e) => { println!("  [PARSE FAIL] {path}: {e:?}"); return; } };
    let imported = import_nif_scene(&scene, pool);
    println!("=== {path}");
    println!("  bsver=0x{:08x} truncated={} recovered_blocks={}", scene.bsver, scene.truncated, scene.recovered_blocks);
    println!("  meshes={}", imported.meshes.len());
    for (i, m) in imported.meshes.iter().enumerate() {
        let mp = m.material.material_path.and_then(|s| pool.resolve(s)).map(|s| s.to_string()).unwrap_or_else(|| "<null>".to_string());
        println!("    [{i}] name={:?} verts={} tris={} skinned={} bones={} material_path={mp}",
            m.name, m.positions.len(), m.indices.len()/3, m.skin.is_some(),
            m.skin.as_ref().map(|s| s.bones.len()).unwrap_or(0));
    }
    println!("  attach_points(exposed)={:?}", imported.attach_points.as_ref().map(|v| v.iter().map(|p| p.name.clone()).collect::<Vec<_>>()));
    println!("  child_attach_connections={:?}", imported.child_attach_connections.as_ref().map(|c| (&c.point_names, c.skinned)));
    println!("  bsx_flags={:?}", imported.bsx_flags);
    println!("  ragdoll={}", imported.ragdoll.is_some());
    println!();
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let ba2_path = &args[0];
    let paths: Vec<&str> = args[1..].iter().map(|s| s.as_str()).collect();
    let arc = Ba2Archive::open(ba2_path).unwrap();
    let mut pool = StringPool::new();
    for p in paths {
        trace_one(&arc, p, &mut pool);
    }
}
