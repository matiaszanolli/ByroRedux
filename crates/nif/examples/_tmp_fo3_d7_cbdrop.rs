use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::controller::NiControllerSequence;
use byroredux_nif::blocks::interpolator::NiTransformInterpolator;
use byroredux_nif::parse_nif;
use byroredux_nif::anim::import_kf;

fn main() {
    let bsa = std::env::args().nth(1).unwrap();
    let a = BsaArchive::open(&bsa).unwrap();
    let files: Vec<String> = a
        .list_files()
        .iter()
        .filter(|p| p.to_ascii_lowercase().ends_with(".kf"))
        .map(|s| s.to_string())
        .collect();
    let mut seq_total = 0usize;
    let mut cb_transform = 0usize;
    let mut ch_total = 0usize;
    let mut nulldata_cb = 0usize;
    let mut files_all_dropped = 0usize;
    let mut sample = Vec::new();
    for p in &files {
        let Ok(bytes) = a.extract(p) else { continue };
        let Ok(scene) = parse_nif(&bytes) else { continue };
        let mut file_cb = 0usize;
        let mut file_null = 0usize;
        for b in &scene.blocks {
            let Some(seq) = b.as_any().downcast_ref::<NiControllerSequence>() else { continue };
            seq_total += 1;
            for cb in &seq.controlled_blocks {
                // resolve controller type loosely: count all cbs
                file_cb += 1;
                if let Some(i) = cb.interpolator_ref.index() {
                    if let Some(ti) = scene.get_as::<NiTransformInterpolator>(i) {
                        if ti.data_ref.index().is_none() {
                            file_null += 1;
                        }
                    }
                }
            }
        }
        cb_transform += file_cb;
        nulldata_cb += file_null;
        let clips = import_kf(&scene);
        let ch: usize = clips.iter().map(|c| c.channels.len()).sum();
        ch_total += ch;
        if ch == 0 && file_cb > 0 {
            files_all_dropped += 1;
            if sample.len() < 12 { sample.push(format!("{p} cb={file_cb} null={file_null}")); }
        }
    }
    println!("kf files: {}", files.len());
    println!("sequences: {seq_total}");
    println!("controlled blocks (all types): {cb_transform}");
    println!("  cb -> NiTransformInterpolator with NULL data_ref: {nulldata_cb}");
    println!("imported transform channels: {ch_total}");
    println!("files where every channel dropped: {files_all_dropped}");
    for s in sample { println!("  {s}"); }
}
