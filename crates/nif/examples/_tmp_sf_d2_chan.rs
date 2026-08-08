use byroredux_bsa::Ba2Archive;
use byroredux_nif::blocks::bs_geometry::{unpack_udec3_xyzw, BSGeometryMeshData};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ar = Ba2Archive::open(&a[1]).expect("open");
    let stride: usize = a.get(2).map(|s| s.parse().unwrap()).unwrap_or(1);
    let names: Vec<String> = ar.list_files().into_iter()
        .filter(|n| n.to_ascii_lowercase().ends_with(".mesh")).map(|s| s.to_string()).collect();
    let (mut no_n, mut no_t, mut no_uv, mut no_c, mut n) = (0, 0, 0, 0, 0usize);
    let (mut nlen_min, mut nlen_max, mut nlen_sum, mut nlen_cnt) = (f32::MAX, 0f32, 0f64, 0usize);
    let (mut tlen_min, mut tlen_max) = (f32::MAX, 0f32);
    let mut w_hist = [0usize; 4];
    let mut idx_oob = 0usize;
    let mut u16_overflow = 0usize;
    for name in names.iter().step_by(stride) {
        let Ok(b) = ar.extract(name) else { continue };
        let Ok(d) = BSGeometryMeshData::parse_from_bytes(&b) else { continue };
        if d.vertices.is_empty() { continue; }
        n += 1;
        if d.normals_raw.is_empty() { no_n += 1 }
        if d.tangents_raw.is_empty() { no_t += 1 }
        if d.uvs0.is_empty() { no_uv += 1 }
        if d.colors.is_empty() { no_c += 1 }
        if d.vertices.len() > 65536 { u16_overflow += 1 }
        let nv = d.vertices.len();
        if d.triangles.iter().any(|t| t.iter().any(|&i| i as usize >= nv)) { idx_oob += 1 }
        for &r in d.normals_raw.iter().step_by(101) {
            let v = unpack_udec3_xyzw(r);
            let l = (v[0]*v[0]+v[1]*v[1]+v[2]*v[2]).sqrt();
            nlen_min = nlen_min.min(l); nlen_max = nlen_max.max(l);
            nlen_sum += l as f64; nlen_cnt += 1;
        }
        for &r in d.tangents_raw.iter().step_by(101) {
            let v = unpack_udec3_xyzw(r);
            let l = (v[0]*v[0]+v[1]*v[1]+v[2]*v[2]).sqrt();
            tlen_min = tlen_min.min(l); tlen_max = tlen_max.max(l);
            w_hist[((r >> 30) & 3) as usize] += 1;
        }
    }
    println!("n={} no_normals={} no_tangents={} no_uv0={} no_colors={} u16_overflow={} idx_oob={}",
        n, no_n, no_t, no_uv, no_c, u16_overflow, idx_oob);
    println!("normal len min={} max={} mean={}", nlen_min, nlen_max, nlen_sum / nlen_cnt.max(1) as f64);
    println!("tangent len min={} max={}  W 2-bit histogram (0,1,2,3)={:?}", tlen_min, tlen_max, w_hist);
}
