//! D7 audit helper: extract a specific NIF path from a set of BA2
//! archives, parse it, and run it through import_nif_scene, printing
//! a summary of nodes/meshes/skin data/materials/block histogram.
use byroredux_bsa::Ba2Archive;
use byroredux_core::string::StringPool;
use byroredux_nif::import::{import_nif_scene_with_resolver, MeshResolver};
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;

struct MultiArchiveResolver {
    archives: Vec<Ba2Archive>,
}

impl MeshResolver for MultiArchiveResolver {
    fn resolve(&self, mesh_name: &str) -> Option<Vec<u8>> {
        for a in &self.archives {
            if let Ok(b) = a.extract(mesh_name) {
                return Some(b);
            }
        }
        None
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let target_path = args
        .next()
        .expect("usage: <nif-path-in-archive> <archive1> [archive2 ...]");
    let archive_paths: Vec<String> = args.collect();

    let mut archives = Vec::new();
    for p in &archive_paths {
        match Ba2Archive::open(p) {
            Ok(a) => archives.push(a),
            Err(e) => println!("OPEN FAIL {}: {}", p, e),
        }
    }

    let raw = archives.iter().find_map(|a| a.extract(&target_path).ok());
    let Some(raw) = raw else {
        println!("NOT FOUND: {}", target_path);
        return;
    };
    println!("=== {} ({} bytes raw) ===", target_path, raw.len());

    let scene = match parse_nif(&raw) {
        Ok(s) => s,
        Err(e) => {
            println!("PARSE FAIL: {}", e);
            return;
        }
    };
    println!(
        "parse: truncated={} dropped_blocks={} block_count={}",
        scene.truncated,
        scene.dropped_block_count,
        scene.blocks.len()
    );

    // Block-type histogram, flagging NiUnknown.
    let mut hist: BTreeMap<String, usize> = BTreeMap::new();
    let mut unknown_count = 0usize;
    for b in scene.blocks.iter() {
        let name = b.block_type_name().to_string();
        if name == "NiUnknown" {
            unknown_count += 1;
        }
        *hist.entry(name).or_insert(0) += 1;
    }
    println!(
        "block histogram ({} distinct types, {} NiUnknown):",
        hist.len(),
        unknown_count
    );
    for (k, v) in &hist {
        println!("  {:>5}  {}", v, k);
    }

    let resolver = MultiArchiveResolver { archives };
    let mut pool = StringPool::new();
    let imported = import_nif_scene_with_resolver(&scene, &mut pool, Some(&resolver));

    println!(
        "imported: nodes={} meshes={}",
        imported.nodes.len(),
        imported.meshes.len()
    );
    for (i, m) in imported.meshes.iter().enumerate() {
        let has_skin = m.skin.is_some();
        let (bone_count, vbi_len, vbw_len, weights_nonzero) = match &m.skin {
            Some(s) => {
                let nonzero = s
                    .vertex_bone_weights
                    .iter()
                    .filter(|w| w.iter().any(|x| *x != 0.0))
                    .count();
                (
                    s.bones.len(),
                    s.vertex_bone_indices.len(),
                    s.vertex_bone_weights.len(),
                    nonzero,
                )
            }
            None => (0, 0, 0, 0),
        };
        let material_path = m
            .material
            .material_path
            .and_then(|s| pool.resolve(s))
            .map(|s| s.to_string());
        println!(
            "  mesh[{}] name={:?} verts={} idx={} uvs={} tangents={} has_skin={} skin_bones={} vbi_len={} vbw_len={} vbw_nonzero={} material.diffuse={:?} material_path={:?}",
            i,
            m.name,
            m.positions.len(),
            m.indices.len(),
            m.uvs.len(),
            m.tangents.len(),
            has_skin,
            bone_count,
            vbi_len,
            vbw_len,
            weights_nonzero,
            m.material.textures.base_color.is_some(),
            material_path,
        );
    }
}
