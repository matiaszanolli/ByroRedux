//! SF audit D2: survey external `.mesh` companions in Starfield BA2s.
//! Usage: _tmp_sf_d2_meshsurvey <archive> [stride]
use byroredux_bsa::Ba2Archive;
use byroredux_nif::blocks::bs_geometry::BSGeometryMeshData;
use std::collections::BTreeMap;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("archive");
    let stride: usize = args.next().map(|s| s.parse().unwrap()).unwrap_or(1);
    let a = Ba2Archive::open(&path).expect("open");
    let names: Vec<String> = a
        .list_files()
        .into_iter()
        .filter(|n| n.to_ascii_lowercase().ends_with(".mesh"))
        .map(|s| s.to_string())
        .collect();
    println!(
        "{}: {} .mesh entries (of {})",
        path,
        names.len(),
        a.file_count()
    );

    let mut vers: BTreeMap<u32, usize> = BTreeMap::new();
    let mut wpv: BTreeMap<u32, usize> = BTreeMap::new();
    let mut skinned = 0usize;
    let mut skin_rows_eq_verts = 0usize;
    let mut skin_rows_ne_verts = 0usize;
    let mut ragged = 0usize;
    let mut sentinel = 0usize;
    let mut parse_err = 0usize;
    let mut n = 0usize;
    let mut max_verts = 0usize;
    let mut trailing_bytes: BTreeMap<i64, usize> = BTreeMap::new();
    let mut lods_present = 0usize;
    let mut bone_idx_max = 0u16;
    let mut examples_skinned: Vec<String> = Vec::new();
    for name in names.iter().step_by(stride) {
        let Ok(bytes) = a.extract(name) else { continue };
        match BSGeometryMeshData::parse_from_bytes(&bytes) {
            Ok(d) => {
                n += 1;
                *vers.entry(d.version).or_default() += 1;
                if d.scale <= 0.0 || d.vertices.is_empty() {
                    sentinel += 1;
                    continue;
                }
                *wpv.entry(d.weights_per_vert).or_default() += 1;
                max_verts = max_verts.max(d.vertices.len());
                if !d.skin_weights.is_empty() {
                    skinned += 1;
                    if d.skin_weights.len() == d.vertices.len() {
                        skin_rows_eq_verts += 1;
                    } else {
                        skin_rows_ne_verts += 1;
                    }
                    for row in &d.skin_weights {
                        for w in row {
                            bone_idx_max = bone_idx_max.max(w.bone_index);
                        }
                    }
                    if examples_skinned.len() < 5 {
                        examples_skinned.push(format!(
                            "{} verts={} wpv={} rows={}",
                            name,
                            d.vertices.len(),
                            d.weights_per_vert,
                            d.skin_weights.len()
                        ));
                    }
                }
                if !d.lods.is_empty() {
                    lods_present += 1;
                }
                let nv = d.vertices.len();
                let ok = |v: usize| v == 0 || v == nv;
                if !(ok(d.uvs0.len())
                    && ok(d.uvs1.len())
                    && ok(d.colors.len())
                    && ok(d.normals_raw.len())
                    && ok(d.tangents_raw.len()))
                {
                    ragged += 1;
                }
                // trailing-byte check: re-parse and see how much was consumed is
                // not exposed; skip.
                let _ = &mut trailing_bytes;
            }
            Err(e) => {
                parse_err += 1;
                if parse_err <= 5 {
                    println!(
                        "  ERR {} : {} ({} bytes, head {:02x?})",
                        name,
                        e,
                        bytes.len(),
                        &bytes[..bytes.len().min(24)]
                    );
                }
            }
        }
    }
    println!(
        "sampled={} parse_err={} sentinel={} ragged={} lods_present={} max_verts={}",
        n, parse_err, sentinel, ragged, lods_present, max_verts
    );
    println!("versions: {:?}", vers);
    println!("weights_per_vert: {:?}", wpv);
    println!(
        "skinned={} rows==verts:{} rows!=verts:{} bone_idx_max={}",
        skinned, skin_rows_eq_verts, skin_rows_ne_verts, bone_idx_max
    );
    for e in &examples_skinned {
        println!("  ex: {}", e);
    }
}
