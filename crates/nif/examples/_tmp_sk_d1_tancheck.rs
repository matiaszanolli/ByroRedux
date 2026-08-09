//! TEMP: cross-validate imported BSTriShape tangents (authored path) against
//! an independent dP/dU synthesis on the same Y-up imported data.
use byroredux_bsa::BsaArchive;
use byroredux_core::string::StringPool;
use byroredux_nif::parse_nif;

fn norm(v: &mut [f32; 3]) {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l > 1e-9 {
        v[0] /= l;
        v[1] /= l;
        v[2] /= l;
    }
}

fn main() {
    let mut pool = StringPool::new();
    let mut agree = 0u64;
    let mut disagree = 0u64;
    let mut skipped = 0u64;
    let mut sign_agree = 0u64;
    let mut sign_disagree = 0u64;
    let mut meshes = 0u64;
    let limit: usize = std::env::var("LIMIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);
    let mut done = 0usize;
    for path in std::env::args().skip(1) {
        let Ok(arc) = BsaArchive::open(&path) else {
            continue;
        };
        let names: Vec<String> = arc
            .list_files()
            .into_iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
            .map(|s| s.to_string())
            .collect();
        for name in &names {
            if done >= limit {
                break;
            }
            done += 1;
            let Ok(bytes) = arc.extract(name) else {
                continue;
            };
            let Ok(scene) = parse_nif(&bytes) else {
                continue;
            };
            for m in byroredux_nif::import::import_nif(&scene, &mut pool) {
                let n = m.positions.len();
                if n == 0 || m.tangents.len() != n || m.uvs.len() != n || m.normals.len() != n {
                    continue;
                }
                if std::env::var("SKINNED").is_ok() && m.skin.is_none() {
                    continue;
                }
                meshes += 1;
                // accumulate dP/dU per vertex
                let mut tu = vec![[0f32; 3]; n];
                let mut tv = vec![[0f32; 3]; n];
                for c in m.indices.chunks_exact(3) {
                    let (i1, i2, i3) = (c[0] as usize, c[1] as usize, c[2] as usize);
                    if i1 >= n || i2 >= n || i3 >= n {
                        continue;
                    }
                    let (v1, v2, v3) = (m.positions[i1], m.positions[i2], m.positions[i3]);
                    let (w1, w2, w3) = (m.uvs[i1], m.uvs[i2], m.uvs[i3]);
                    let x1 = v2[0] - v1[0];
                    let x2 = v3[0] - v1[0];
                    let y1 = v2[1] - v1[1];
                    let y2 = v3[1] - v1[1];
                    let z1 = v2[2] - v1[2];
                    let z2 = v3[2] - v1[2];
                    let s1 = w2[0] - w1[0];
                    let s2 = w3[0] - w1[0];
                    let t1 = w2[1] - w1[1];
                    let t2 = w3[1] - w1[1];
                    let det = s1 * t2 - s2 * t1;
                    let r = if det >= 0.0 { 1.0 } else { -1.0 };
                    let mut sd = [
                        (t2 * x1 - t1 * x2) * r,
                        (t2 * y1 - t1 * y2) * r,
                        (t2 * z1 - t1 * z2) * r,
                    ];
                    let mut td = [
                        (s1 * x2 - s2 * x1) * r,
                        (s1 * y2 - s2 * y1) * r,
                        (s1 * z2 - s2 * z1) * r,
                    ];
                    norm(&mut sd);
                    norm(&mut td);
                    for &i in &[i1, i2, i3] {
                        for k in 0..3 {
                            tu[i][k] += sd[k];
                            tv[i][k] += td[k];
                        }
                    }
                }
                for i in 0..n {
                    let mut a = tu[i];
                    if a[0] * a[0] + a[1] * a[1] + a[2] * a[2] < 1e-12 {
                        skipped += 1;
                        continue;
                    }
                    norm(&mut a);
                    let t = m.tangents[i];
                    let mut b = [t[0], t[1], t[2]];
                    if b[0] * b[0] + b[1] * b[1] + b[2] * b[2] < 1e-12 {
                        skipped += 1;
                        continue;
                    }
                    norm(&mut b);
                    let d = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
                    if d > 0.5 {
                        agree += 1;
                    } else if d < -0.5 {
                        disagree += 1;
                    } else {
                        skipped += 1;
                    }

                    // independent sign: sign(dot(B_synth, cross(N, T_authored)))
                    let nn = m.normals[i];
                    let cr = [
                        nn[1] * b[2] - nn[2] * b[1],
                        nn[2] * b[0] - nn[0] * b[2],
                        nn[0] * b[1] - nn[1] * b[0],
                    ];
                    let mut bs = tv[i];
                    norm(&mut bs);
                    let sd2 = bs[0] * cr[0] + bs[1] * cr[1] + bs[2] * cr[2];
                    if sd2.abs() > 0.2 {
                        let want = if sd2 >= 0.0 { 1.0 } else { -1.0 };
                        if (want - t[3]).abs() < 0.5 {
                            sign_agree += 1
                        } else {
                            sign_disagree += 1
                        }
                    }
                }
            }
        }
    }
    println!("meshes compared = {meshes}");
    println!("tangent dir agree (dot>+0.5)   = {agree}");
    println!("tangent dir OPPOSED (dot<-0.5) = {disagree}");
    println!("ambiguous / skipped            = {skipped}");
    let tot = (agree + disagree) as f64;
    if tot > 0.0 {
        println!("agreement = {:.3}%", agree as f64 * 100.0 / tot);
    }
    println!("bitangent sign agree = {sign_agree}, disagree = {sign_disagree}");
    let st = (sign_agree + sign_disagree) as f64;
    if st > 0.0 {
        println!("sign agreement = {:.3}%", sign_agree as f64 * 100.0 / st);
    }
}
