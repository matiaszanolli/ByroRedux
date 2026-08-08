//! TEMP scratch: Skyrim SE dim-1 import-side probe for BSTriShape /
//! SSE skinned reconstruction. Measures tangent coverage + TBN sanity.
use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::tri_shape::{BsTriShape, BsTriShapeKind};
use byroredux_nif::parse_nif;
use byroredux_core::string::StringPool;

fn main() {
    let mut pool = StringPool::new();
    let mut shapes_total = 0usize;
    let mut recon_candidates = 0usize;      // data_size==0 && has skin_ref
    let mut dyn_shapes = 0usize;

    let mut meshes = 0usize;
    let mut meshes_skinned = 0usize;
    let mut mesh_no_tangents = 0usize;
    let mut mesh_tan_len_mismatch = 0usize;
    let mut mesh_uv_len_mismatch = 0usize;
    let mut mesh_norm_len_mismatch = 0usize;
    let mut mesh_col_len_mismatch = 0usize;
    let mut bad_tan_norm = 0usize;      // |T| far from 1
    let mut bad_tan_ortho = 0usize;     // |dot(N,T)| large
    let mut zero_normals = 0usize;
    let mut sign_neg = 0usize;
    let mut sign_pos = 0usize;
    let mut skin_w_missing = 0usize;
    let mut skin_idx_missing = 0usize;
    let mut skin_len_mismatch = 0usize;
    let mut oob_index = 0usize;

    for path in std::env::args().skip(1) {
        let Ok(arc) = BsaArchive::open(&path) else { continue };
        let names: Vec<String> = arc.list_files().into_iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
            .map(|s| s.to_string()).collect();
        eprintln!("{path}: {} nifs", names.len());
        for name in &names {
            let Ok(bytes) = arc.extract(name) else { continue };
            let Ok(scene) = parse_nif(&bytes) else { continue };
            for block in scene.blocks.iter() {
                if let Some(s) = block.as_any().downcast_ref::<BsTriShape>() {
                    shapes_total += 1;
                    if matches!(s.kind, BsTriShapeKind::Dynamic { .. }) { dyn_shapes += 1; }
                    if s.data_size == 0 && s.skin_ref.index().is_some() { recon_candidates += 1; }
                }
            }
            let imported = byroredux_nif::import::import_nif(&scene, &mut pool);
            for m in &imported {
                meshes += 1;
                let n = m.positions.len();
                if n == 0 { continue; }
                if m.normals.len() != n { mesh_norm_len_mismatch += 1; }
                if !m.uvs.is_empty() && m.uvs.len() != n { mesh_uv_len_mismatch += 1; }
                if m.uvs.is_empty() { mesh_uv_len_mismatch += 0; }
                if m.colors.len() != n { mesh_col_len_mismatch += 1; }
                if m.tangents.is_empty() { mesh_no_tangents += 1; }
                else if m.tangents.len() != n { mesh_tan_len_mismatch += 1; }
                else {
                    for i in 0..n {
                        let t = m.tangents[i];
                        let nl = m.normals[i];
                        let tl = (t[0]*t[0]+t[1]*t[1]+t[2]*t[2]).sqrt();
                        if tl > 1e-4 && (tl - 1.0).abs() > 0.05 { bad_tan_norm += 1; }
                        let nn = (nl[0]*nl[0]+nl[1]*nl[1]+nl[2]*nl[2]).sqrt();
                        if nn < 1e-4 { zero_normals += 1; continue; }
                        if tl > 1e-4 {
                            let d = (nl[0]*t[0]+nl[1]*t[1]+nl[2]*t[2]) / (nn*tl);
                            if d.abs() > 0.3 { bad_tan_ortho += 1; }
                        }
                        if t[3] < 0.0 { sign_neg += 1; } else { sign_pos += 1; }
                    }
                }
                for &idx in &m.indices { if idx as usize >= n { oob_index += 1; break; } }
                if let Some(sk) = &m.skin {
                    meshes_skinned += 1;
                    if sk.vertex_bone_weights.is_empty() { skin_w_missing += 1; }
                    else if sk.vertex_bone_weights.len() != n { skin_len_mismatch += 1; }
                    if sk.vertex_bone_indices.is_empty() { skin_idx_missing += 1; }
                }
            }
        }
    }
    println!("BsTriShape blocks       = {shapes_total}");
    println!("  Dynamic               = {dyn_shapes}");
    println!("  recon candidates      = {recon_candidates}  (data_size==0 && skin_ref)");
    println!("imported meshes         = {meshes}");
    println!("  skinned               = {meshes_skinned}");
    println!("  normals len mismatch  = {mesh_norm_len_mismatch}");
    println!("  uvs len mismatch      = {mesh_uv_len_mismatch}");
    println!("  colors len mismatch   = {mesh_col_len_mismatch}");
    println!("  NO tangents           = {mesh_no_tangents}");
    println!("  tangent len mismatch  = {mesh_tan_len_mismatch}");
    println!("  vertices |T| != 1     = {bad_tan_norm}");
    println!("  vertices |dot(N,T)|>.3= {bad_tan_ortho}");
    println!("  zero normals          = {zero_normals}");
    println!("  sign + / -            = {sign_pos} / {sign_neg}");
    println!("  skin weights missing  = {skin_w_missing}");
    println!("  skin indices missing  = {skin_idx_missing}");
    println!("  skin len mismatch     = {skin_len_mismatch}");
    println!("  meshes w/ OOB index   = {oob_index}");
}
