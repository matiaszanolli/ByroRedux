//! D5 audit helper: extract specific NIFs from a BA2/BSA archive and
//! trace each through `import_nif_scene`. Reports nodes/meshes counts,
//! material_path values, and per-mesh vertex counts.
//!
//! Usage:
//!   d5_starfield_import <archive> <nif-path-1> [<nif-path-2> ...]
use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_core::string::StringPool;
use byroredux_nif::{
    blocks::bs_geometry::{BSGeometry, BSGeometryMeshKind},
    import::{import_nif_scene, import_nif_scene_with_resolver, MeshResolver},
    parse_nif,
};
use std::path::PathBuf;

enum Arc {
    Bsa(BsaArchive),
    Ba2(Ba2Archive),
}

impl Arc {
    fn extract(&self, p: &str) -> std::io::Result<Vec<u8>> {
        match self {
            Arc::Bsa(a) => a.extract(p),
            Arc::Ba2(a) => a.extract(p),
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let archive = PathBuf::from(args.next().expect("usage: <archive> <nif> [...]"));
    let entries: Vec<String> = args.collect();
    if entries.is_empty() {
        eprintln!("no nif paths given");
        std::process::exit(2);
    }
    let arc = if BsaArchive::open(&archive).is_ok() {
        Arc::Bsa(BsaArchive::open(&archive).unwrap())
    } else {
        Arc::Ba2(Ba2Archive::open(&archive).expect("open BA2"))
    };
    let mut pool = StringPool::new();
    for entry in &entries {
        println!("\n=== {} ===", entry);
        let bytes = match arc.extract(entry) {
            Ok(b) => b,
            Err(e) => {
                println!("  EXTRACT FAIL: {}", e);
                continue;
            }
        };
        let scene = match parse_nif(&bytes) {
            Ok(s) => s,
            Err(e) => {
                println!("  PARSE FAIL: {:?}", e);
                continue;
            }
        };
        // Per-block-type histogram for this scene.
        use std::collections::BTreeMap;
        let mut hist: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        for b in &scene.blocks {
            if let Some(u) = b
                .as_any()
                .downcast_ref::<byroredux_nif::blocks::NiUnknown>()
            {
                let e = hist.entry(u.type_name.to_string()).or_insert((0, 0));
                e.1 += 1;
            } else {
                let e = hist
                    .entry(b.block_type_name().to_string())
                    .or_insert((0, 0));
                e.0 += 1;
            }
        }
        let total_blocks = scene.blocks.len();
        let unknown_blocks: usize = hist.values().map(|(_, u)| *u).sum();
        println!(
            "  blocks={} (unknown={}) truncated={} dropped={} recovered={}",
            total_blocks,
            unknown_blocks,
            scene.truncated,
            scene.dropped_block_count,
            scene.recovered_blocks
        );
        // Report any unknowns in this NIF
        for (name, (parsed, unknown)) in &hist {
            if *unknown > 0 {
                println!("  UNK: {} parsed={} unknown={}", name, parsed, unknown);
            }
        }
        // Dump full per-block-type histogram (parsed only) to surface
        // which Starfield-specific geometry types the importer ignores.
        println!("  block-type histogram:");
        for (name, (parsed, unknown)) in &hist {
            println!("    {} parsed={} unknown={}", name, parsed, unknown);
        }
        // Dump BSGeometry mesh-kind details — what the importer is
        // being asked to resolve. The Starfield import-side gap
        // (#1292) hinges on whether external `.mesh` companion
        // names resolve out of the supplied archive.
        let mut external_names: Vec<String> = Vec::new();
        for b in &scene.blocks {
            if let Some(g) = b.as_any().downcast_ref::<BSGeometry>() {
                let internal = g.has_internal_geom_data();
                println!(
                    "  BSGeometry flags=0x{:04x} internal-geom-data={} LOD-slots={}",
                    g.av.flags,
                    internal,
                    g.meshes.len(),
                );
                for (i, m) in g.meshes.iter().enumerate() {
                    match &m.kind {
                        BSGeometryMeshKind::External { mesh_name } => {
                            println!("    LOD[{i}] external mesh_name='{mesh_name}'");
                            external_names.push(mesh_name.clone());
                        }
                        BSGeometryMeshKind::Internal { mesh_data } => {
                            println!(
                                "    LOD[{i}] internal verts={} tris={}",
                                mesh_data.vertices.len(),
                                mesh_data.triangles.len(),
                            );
                        }
                    }
                }
            }
        }

        // Try resolving each external `.mesh` name out of the supplied
        // archive. Reports HIT (with byte count) or MISS — the smoking
        // gun for #1292 is whether the BA2 actually carries the
        // companion files, and whether their layout parses.
        //
        // The CANONICAL Starfield path layout is `geometries\<X>.mesh`
        // where `<X>` is the raw hash-tree path the BSGeometry block
        // stores in its `mesh_name` field. Pre-#1292 the importer
        // called `resolver.resolve(mesh_name)` with the raw hash,
        // which never matched the archive's `geometries\X.mesh` form.
        for mesh_name in &external_names {
            let candidates = [
                format!("geometries\\{mesh_name}.mesh"),
                mesh_name.clone(),
                format!("meshes\\{mesh_name}"),
                mesh_name.replace('/', "\\"),
                format!("meshes\\{}", mesh_name.replace('/', "\\")),
            ];
            let mut hit = false;
            for cand in &candidates {
                if let Ok(bytes) = arc.extract(cand) {
                    println!(
                        "    BA2 HIT  ('{}' as '{}'): {} bytes",
                        mesh_name,
                        cand,
                        bytes.len(),
                    );
                    hit = true;
                    break;
                }
            }
            if !hit {
                println!(
                    "    BA2 MISS ('{}'); tried {} normalisations",
                    mesh_name,
                    candidates.len(),
                );
            }
        }

        // Provide a real resolver and re-import to see if the supplied
        // archive successfully unblocks the external-geometry path.
        struct ArcResolver<'a>(&'a Arc);
        impl<'a> MeshResolver for ArcResolver<'a> {
            fn resolve(&self, mesh_name: &str) -> Option<Vec<u8>> {
                // Canonical Starfield form first — `geometries\X.mesh`.
                let candidates = [
                    format!("geometries\\{mesh_name}.mesh"),
                    mesh_name.to_string(),
                    format!("meshes\\{mesh_name}"),
                    mesh_name.replace('/', "\\"),
                    format!("meshes\\{}", mesh_name.replace('/', "\\")),
                ];
                for cand in &candidates {
                    if let Ok(bytes) = self.0.extract(cand) {
                        return Some(bytes);
                    }
                }
                None
            }
        }
        let resolver = ArcResolver(&arc);
        let imp_with_resolver = import_nif_scene_with_resolver(&scene, &mut pool, Some(&resolver));
        println!(
            "  with-resolver: nodes={} meshes={}",
            imp_with_resolver.nodes.len(),
            imp_with_resolver.meshes.len()
        );

        let imp = import_nif_scene(&scene, &mut pool);
        println!(
            "  no-resolver:   nodes={} meshes={}",
            imp.nodes.len(),
            imp.meshes.len()
        );
        for (i, m) in imp.meshes.iter().enumerate() {
            let name = m.name.as_deref().unwrap_or("<unnamed>");
            let mat_path = m
                .material_path
                .as_ref()
                .and_then(|s| pool.resolve(*s).map(|s| s.to_string()))
                .unwrap_or_else(|| "<none>".to_string());
            let tex = m
                .texture_path
                .as_ref()
                .and_then(|s| pool.resolve(*s).map(|s| s.to_string()))
                .unwrap_or_else(|| "<none>".to_string());
            println!(
                "  mesh[{:>2}] '{}' verts={} tris={} material_path={} texture_path={}",
                i,
                name,
                m.positions.len(),
                m.indices.len() / 3,
                mat_path,
                tex,
            );
        }
    }
}
