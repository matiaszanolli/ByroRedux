use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::controller::NiControllerSequence;
use byroredux_nif::blocks::interpolator::NiTransformInterpolator;
use byroredux_nif::parse_nif;

fn main() {
    let bsa = std::env::args().nth(1).unwrap();
    let a = BsaArchive::open(&bsa).unwrap();
    let files: Vec<String> = a
        .list_files()
        .iter()
        .filter(|p| p.to_ascii_lowercase().ends_with(".kf"))
        .map(|s| s.to_string())
        .collect();
    let mut cb_total = 0usize;
    let mut cb_dropped = 0usize;
    let mut files_all_dropped = 0usize;
    let mut files_any = 0usize;
    let mut sample = Vec::new();
    for p in &files {
        let Ok(bytes) = a.extract(p) else { continue };
        let Ok(scene) = parse_nif(&bytes) else {
            continue;
        };
        let mut f_total = 0usize;
        let mut f_drop = 0usize;
        for b in &scene.blocks {
            let Some(seq) = b.as_any().downcast_ref::<NiControllerSequence>() else {
                continue;
            };
            for cb in &seq.controlled_blocks {
                let Some(idx) = cb.interpolator_ref.index() else {
                    continue;
                };
                let Some(i) = scene.get_as::<NiTransformInterpolator>(idx) else {
                    continue;
                };
                f_total += 1;
                if i.data_ref.index().is_none() {
                    f_drop += 1;
                }
            }
        }
        cb_total += f_total;
        cb_dropped += f_drop;
        if f_drop > 0 {
            files_any += 1;
            if f_drop == f_total {
                files_all_dropped += 1;
                if sample.len() < 8 {
                    sample.push(format!("{p} ({f_drop}/{f_total})"));
                }
            }
        }
    }
    println!("controlled blocks -> NiTransformInterpolator: {cb_total}");
    println!(
        "  dropped (null data_ref): {cb_dropped}  ({:.1}%)",
        100.0 * cb_dropped as f64 / cb_total.max(1) as f64
    );
    println!("files with >=1 dropped: {files_any} / {}", files.len());
    println!("files where ALL transform channels dropped: {files_all_dropped}");
    for s in sample {
        println!("  {s}");
    }
}
