//! SF D2: cross-check BSGeometry NIF-level hints + bounding sphere against the
//! resolved external `.mesh` body.
use byroredux_bsa::Ba2Archive;
use byroredux_nif::blocks::bs_geometry::{BSGeometry, BSGeometryMeshData, BSGeometryMeshKind};
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let nif_ar = Ba2Archive::open(&a[1]).expect("open nif archive");
    let mesh_ar = Ba2Archive::open(&a[2]).expect("open mesh archive");
    let stride: usize = a.get(3).map(|s| s.parse().unwrap()).unwrap_or(1);
    let nifs: Vec<String> = nif_ar
        .list_files()
        .into_iter()
        .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
        .map(|s| s.to_string())
        .collect();
    println!("{} nifs", nifs.len());

    let mut slots_hist: BTreeMap<usize, usize> = BTreeMap::new();
    let mut tri_size_eq_bytes = 0usize;
    let mut tri_size_eq_indices = 0usize;
    let mut tri_size_other = 0usize;
    let mut verts_eq = 0usize;
    let mut verts_ne = 0usize;
    let mut descending = 0usize;
    let mut nondescending = 0usize;
    let mut resolved = 0usize;
    let mut unresolved = 0usize;
    let mut internal_blocks = 0usize;
    let mut center_raw_ok = 0usize;
    let mut center_swap_ok = 0usize;
    let mut center_neither = 0usize;
    let mut extent_y_major = 0usize;
    let mut extent_z_major = 0usize;
    let mut extent_x_major = 0usize;
    let mut shown = 0;
    for name in nifs.iter().step_by(stride) {
        let Ok(bytes) = nif_ar.extract(name) else {
            continue;
        };
        let Ok(scene) = parse_nif(&bytes) else {
            continue;
        };
        for blk in &scene.blocks {
            let Some(g) = blk.as_any().downcast_ref::<BSGeometry>() else {
                continue;
            };
            slots_hist
                .entry(g.meshes.len())
                .and_modify(|c| *c += 1)
                .or_insert(1);
            if g.has_internal_geom_data() {
                internal_blocks += 1;
                continue;
            }
            let mut prev: Option<u32> = None;
            let mut first_done = false;
            for m in &g.meshes {
                if let Some(p) = prev {
                    if m.num_verts <= p {
                        descending += 1
                    } else {
                        nondescending += 1
                    }
                }
                prev = Some(m.num_verts);
                let BSGeometryMeshKind::External { mesh_name } = &m.kind else {
                    continue;
                };
                if first_done {
                    continue;
                }
                let canonical = format!("geometries\\{mesh_name}.mesh");
                let Ok(mb) = mesh_ar.extract(&canonical) else {
                    unresolved += 1;
                    continue;
                };
                resolved += 1;
                let Ok(d) = BSGeometryMeshData::parse_from_bytes(&mb) else {
                    continue;
                };
                first_done = true;
                if d.vertices.is_empty() {
                    continue;
                }
                let tc = d.triangles.len() as u32;
                if m.tri_size == tc * 6 {
                    tri_size_eq_bytes += 1
                } else if m.tri_size == tc * 3 {
                    tri_size_eq_indices += 1
                } else {
                    tri_size_other += 1;
                    if shown < 5 {
                        println!(
                            "  tri_size={} tri_count={} nv_hint={} nv={} {}",
                            m.tri_size,
                            tc,
                            m.num_verts,
                            d.vertices.len(),
                            name
                        );
                        shown += 1;
                    }
                }
                if m.num_verts as usize == d.vertices.len() {
                    verts_eq += 1
                } else {
                    verts_ne += 1
                }
                // bounding sphere center check
                let (c, r) = g.bounding_sphere;
                if r > 0.0 {
                    let mut mn = [f32::MAX; 3];
                    let mut mx = [f32::MIN; 3];
                    for p in &d.vertices {
                        for i in 0..3 {
                            mn[i] = mn[i].min(p[i]);
                            mx[i] = mx[i].max(p[i]);
                        }
                    }
                    let ctr = [
                        (mn[0] + mx[0]) * 0.5,
                        (mn[1] + mx[1]) * 0.5,
                        (mn[2] + mx[2]) * 0.5,
                    ];
                    let ext = [mx[0] - mn[0], mx[1] - mn[1], mx[2] - mn[2]];
                    let major = if ext[0] >= ext[1] && ext[0] >= ext[2] {
                        0
                    } else if ext[1] >= ext[2] {
                        1
                    } else {
                        2
                    };
                    match major {
                        0 => extent_x_major += 1,
                        1 => extent_y_major += 1,
                        _ => extent_z_major += 1,
                    }
                    let d_raw = ((c[0] - ctr[0]).powi(2)
                        + (c[1] - ctr[1]).powi(2)
                        + (c[2] - ctr[2]).powi(2))
                    .sqrt();
                    // zup->yup swap of the NIF sphere center: (x,y,z)->(x,z,-y)
                    let cs = [c[0], c[2], -c[1]];
                    let d_swap = ((cs[0] - ctr[0]).powi(2)
                        + (cs[1] - ctr[1]).powi(2)
                        + (cs[2] - ctr[2]).powi(2))
                    .sqrt();
                    let tol = r * 0.25 + 1e-3;
                    if d_raw <= tol && d_swap > tol {
                        center_raw_ok += 1
                    } else if d_swap <= tol && d_raw > tol {
                        center_swap_ok += 1
                    } else if d_raw <= tol && d_swap <= tol {
                        center_raw_ok += 1
                    } else {
                        center_neither += 1
                    }
                }
            }
        }
    }
    println!("slots per BSGeometry: {:?}", slots_hist);
    println!(
        "internal_geom_blocks={} resolved={} unresolved={}",
        internal_blocks, resolved, unresolved
    );
    println!(
        "tri_size == tri_count*6 (bytes): {}  == *3 (indices): {}  other: {}",
        tri_size_eq_bytes, tri_size_eq_indices, tri_size_other
    );
    println!("num_verts hint eq: {} ne: {}", verts_eq, verts_ne);
    println!(
        "slot num_verts descending: {} non-descending: {}",
        descending, nondescending
    );
    println!(
        "bound center raw-ok: {} swap-ok: {} neither: {}",
        center_raw_ok, center_swap_ok, center_neither
    );
    println!(
        "vertex extent major axis  X:{} Y:{} Z:{}",
        extent_x_major, extent_y_major, extent_z_major
    );
}
