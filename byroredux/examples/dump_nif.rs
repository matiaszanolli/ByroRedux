// Quick diagnostic: dump NIF scene graph transforms
// Usage: cargo run --example dump_nif -- <bsa_path> <mesh_path>

use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <bsa_path> <mesh_path>", args[0]);
        std::process::exit(1);
    }

    let bsa_path = &args[1];
    let mesh_path = &args[2];

    // Open BSA and extract NIF
    let bsa = byroredux_bsa::BsaArchive::open(std::path::Path::new(bsa_path))
        .expect("Failed to open BSA");

    let nif_data = bsa.extract(mesh_path)
        .expect("Failed to extract mesh from BSA");

    println!("Extracted '{}': {} bytes", mesh_path, nif_data.len());

    // Parse NIF
    let scene = byroredux_nif::parse_nif(&nif_data)
        .expect("Failed to parse NIF");

    println!("\n=== NIF Scene Graph ===");
    println!("Root index: {:?}", scene.root_index);
    println!("Total blocks: {}", scene.blocks.len());

    // Dump each block's type and transform
    for (i, block) in scene.blocks.iter().enumerate() {
        let type_name = block.block_type_name();
        print!("\n[{}] {}", i, type_name);

        // Try as NiNode
        if let Some(node) = block.as_any().downcast_ref::<byroredux_nif::blocks::node::NiNode>() {
            println!(" name={:?}", node.av.net.name);
            println!("  flags: 0x{:04X}", node.av.flags);
            dump_transform(&node.av.transform);
            println!("  children: {:?}", node.children.iter()
                .map(|r| format!("{:?}", r.index()))
                .collect::<Vec<_>>());
        }
        // Try as NiTriShape
        else if let Some(shape) = block.as_any().downcast_ref::<byroredux_nif::blocks::tri_shape::NiTriShape>() {
            println!(" name={:?}", shape.av.net.name);
            println!("  flags: 0x{:04X}", shape.av.flags);
            dump_transform(&shape.av.transform);
            println!("  data_ref: {:?}", shape.data_ref.index());
            println!("  properties: {:?}", shape.av.properties.iter()
                .map(|r| format!("{:?}", r.index()))
                .collect::<Vec<_>>());
        }
        // Try as NiTriShapeData
        else if let Some(data) = block.as_any().downcast_ref::<byroredux_nif::blocks::tri_shape::NiTriShapeData>() {
            println!();
            println!("  vertices: {}", data.vertices.len());
            println!("  triangles: {}", data.triangles.len());
            if !data.vertices.is_empty() {
                let (mut min, mut max) = ([f32::INFINITY;3], [f32::NEG_INFINITY;3]);
                for v in &data.vertices {
                    min[0] = min[0].min(v.x); min[1] = min[1].min(v.y); min[2] = min[2].min(v.z);
                    max[0] = max[0].max(v.x); max[1] = max[1].max(v.y); max[2] = max[2].max(v.z);
                }
                println!("  vtx_min: ({:.1}, {:.1}, {:.1})", min[0], min[1], min[2]);
                println!("  vtx_max: ({:.1}, {:.1}, {:.1})", max[0], max[1], max[2]);
                println!("  vtx_size: ({:.1}, {:.1}, {:.1})", max[0]-min[0], max[1]-min[1], max[2]-min[2]);
            }
        }
        // Try as NiTriStripsData
        else if let Some(data) = block.as_any().downcast_ref::<byroredux_nif::blocks::tri_shape::NiTriStripsData>() {
            println!();
            println!("  vertices: {}", data.vertices.len());
            println!("  strips: {}", data.strips.len());
            let tri_count: usize = data.strips.iter().map(|s| if s.len() >= 2 { s.len() - 2 } else { 0 }).sum();
            println!("  triangles (from strips): ~{}", tri_count);
            if !data.vertices.is_empty() {
                let (mut min, mut max) = ([f32::INFINITY;3], [f32::NEG_INFINITY;3]);
                for v in &data.vertices {
                    min[0] = min[0].min(v.x); min[1] = min[1].min(v.y); min[2] = min[2].min(v.z);
                    max[0] = max[0].max(v.x); max[1] = max[1].max(v.y); max[2] = max[2].max(v.z);
                }
                println!("  vtx_min: ({:.1}, {:.1}, {:.1})", min[0], min[1], min[2]);
                println!("  vtx_max: ({:.1}, {:.1}, {:.1})", max[0], max[1], max[2]);
                println!("  vtx_size: ({:.1}, {:.1}, {:.1})", max[0]-min[0], max[1]-min[1], max[2]-min[2]);
            }
        }
        else if let Some(unknown) = block.as_any().downcast_ref::<byroredux_nif::blocks::NiUnknown>() {
            println!(" actual_type='{}' data_len={}", unknown.type_name, unknown.data.len());
        }
        else {
            println!();
        }
    }

    // Also run the import pipeline and show results
    println!("\n=== Import Results (Y-up) ===");
    let meshes = byroredux_nif::import::import_nif(&scene);
    for (i, m) in meshes.iter().enumerate() {
        println!("\nMesh {}: name={:?}", i, m.name);
        println!("  translation: ({:.2}, {:.2}, {:.2})", m.translation[0], m.translation[1], m.translation[2]);
        println!("  rotation (quat xyzw): ({:.4}, {:.4}, {:.4}, {:.4})", m.rotation[0], m.rotation[1], m.rotation[2], m.rotation[3]);
        println!("  scale: {:.4}", m.scale);
        println!("  vertices: {}, indices: {}", m.positions.len(), m.indices.len());
        if !m.positions.is_empty() {
            let (mut min, mut max) = ([f32::INFINITY;3], [f32::NEG_INFINITY;3]);
            for p in &m.positions {
                for j in 0..3 { min[j] = min[j].min(p[j]); max[j] = max[j].max(p[j]); }
            }
            println!("  Y-up vtx_min: ({:.1}, {:.1}, {:.1})", min[0], min[1], min[2]);
            println!("  Y-up vtx_max: ({:.1}, {:.1}, {:.1})", max[0], max[1], max[2]);
            println!("  Y-up vtx_size: ({:.1}, {:.1}, {:.1})", max[0]-min[0], max[1]-min[1], max[2]-min[2]);
        }

        // Winding order check: compare cross product of first triangle's edges
        // with the vertex normal direction.
        if m.indices.len() >= 3 && !m.normals.is_empty() {
            let i0 = m.indices[0] as usize;
            let i1 = m.indices[1] as usize;
            let i2 = m.indices[2] as usize;
            let p0 = m.positions[i0];
            let p1 = m.positions[i1];
            let p2 = m.positions[i2];
            let e1 = [p1[0]-p0[0], p1[1]-p0[1], p1[2]-p0[2]];
            let e2 = [p2[0]-p0[0], p2[1]-p0[1], p2[2]-p0[2]];
            let cross = [
                e1[1]*e2[2] - e1[2]*e2[1],
                e1[2]*e2[0] - e1[0]*e2[2],
                e1[0]*e2[1] - e1[1]*e2[0],
            ];
            let n = m.normals[i0];
            let dot = cross[0]*n[0] + cross[1]*n[1] + cross[2]*n[2];
            let winding = if dot > 0.0 { "CCW (OpenGL)" } else { "CW (D3D)" };
            println!("  winding: {} (cross·normal = {:.4})", winding, dot);
        }
    }
}

fn dump_transform(t: &byroredux_nif::types::NiTransform) {
    let r = &t.rotation.rows;
    println!("  translation: ({:.2}, {:.2}, {:.2})", t.translation.x, t.translation.y, t.translation.z);
    println!("  rotation:");
    println!("    [{:8.4}, {:8.4}, {:8.4}]", r[0][0], r[0][1], r[0][2]);
    println!("    [{:8.4}, {:8.4}, {:8.4}]", r[1][0], r[1][1], r[1][2]);
    println!("    [{:8.4}, {:8.4}, {:8.4}]", r[2][0], r[2][1], r[2][2]);
    let det = r[0][0]*(r[1][1]*r[2][2]-r[1][2]*r[2][1])
            - r[0][1]*(r[1][0]*r[2][2]-r[1][2]*r[2][0])
            + r[0][2]*(r[1][0]*r[2][1]-r[1][1]*r[2][0]);
    println!("  det: {:.6}", det);
    println!("  scale: {:.4}", t.scale);
}
