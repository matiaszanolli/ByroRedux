// Dimension 6 throwaway: verify NiSkinData skinning extraction on a real
// FO3 creature mesh (deathclaw.nif).
fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: _tmp_fo3_deathclaw_skin <path.nif>");
    let bytes = std::fs::read(&path).expect("read");
    let scene = byroredux_nif::parse_nif(&bytes).expect("parse");
    println!("parse OK: {} blocks, truncated={}", scene.blocks.len(), scene.truncated);

    let mut pool = byroredux_core::string::StringPool::new();
    let imported = byroredux_nif::import::import_nif_scene(&scene, &mut pool);
    println!("meshes: {}", imported.meshes.len());
    for (i, m) in imported.meshes.iter().enumerate() {
        let verts = m.positions.len();
        match &m.skin {
            Some(skin) => {
                let nonzero_weighted = m
                    .skin
                    .as_ref()
                    .map(|s| {
                        s.vertex_bone_weights
                            .iter()
                            .filter(|w| w.iter().sum::<f32>() > 0.0)
                            .count()
                    })
                    .unwrap_or(0);
                println!(
                    "  mesh[{i}] name={:?} verts={verts} bones={} skeleton_root={:?} weighted_verts={}/{}",
                    m.name,
                    skin.bones.len(),
                    skin.skeleton_root,
                    nonzero_weighted,
                    skin.vertex_bone_weights.len(),
                );
                // Spot-check bone indices are in-range.
                let max_idx = skin
                    .vertex_bone_indices
                    .iter()
                    .flat_map(|arr| arr.iter())
                    .max()
                    .copied()
                    .unwrap_or(0);
                println!(
                    "    max bone index referenced = {} (bones.len()={})",
                    max_idx,
                    skin.bones.len()
                );
                if (max_idx as usize) >= skin.bones.len() && !skin.bones.is_empty() {
                    println!("    !! OUT OF RANGE bone index");
                }
            }
            None => println!("  mesh[{i}] name={:?} verts={verts} NO SKIN", m.name),
        }
    }
}
