use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::interpolator::NiTransformInterpolator;
use byroredux_nif::parse_nif;

fn main() {
    let bsa = std::env::args().nth(1).unwrap();
    let a = BsaArchive::open(&bsa).unwrap();
    let files: Vec<String> = a
        .list_files()
        .iter()
        .filter(|p| {
            let l = p.to_ascii_lowercase();
            l.ends_with(".kf")
        })
        .map(|s| s.to_string())
        .collect();
    let mut total = 0usize;
    let mut null_data = 0usize;
    let mut null_data_finite_pose = 0usize;
    let mut files_with = 0usize;
    let mut sample: Vec<String> = Vec::new();
    for p in &files {
        let Ok(bytes) = a.extract(p) else { continue };
        let Ok(scene) = parse_nif(&bytes) else { continue };
        let mut any = false;
        for b in &scene.blocks {
            if let Some(i) = b.as_any().downcast_ref::<NiTransformInterpolator>() {
                total += 1;
                if i.data_ref.index().is_none() {
                    null_data += 1;
                    let t = &i.transform;
                    let fin = |v: f32| v.is_finite() && v.abs() < 3.0e38;
                    if fin(t.translation.x) || fin(t.rotation[0]) || fin(t.scale) {
                        null_data_finite_pose += 1;
                        any = true;
                    }
                }
            }
        }
        if any {
            files_with += 1;
            if sample.len() < 10 {
                sample.push(p.clone());
            }
        }
    }
    println!("kf files: {}", files.len());
    println!("NiTransformInterpolator total: {total}");
    println!("  null data_ref: {null_data}");
    println!("  null data_ref + finite pose: {null_data_finite_pose}");
    println!("files affected: {files_with}");
    for s in sample {
        println!("  {s}");
    }
}
