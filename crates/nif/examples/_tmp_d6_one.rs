use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::controller::NiControllerSequence;
use byroredux_nif::blocks::interpolator::NiTransformInterpolator;
use byroredux_nif::parse_nif;

fn main() {
    let bsa = std::env::args().nth(1).unwrap();
    let path = std::env::args().nth(2).unwrap();
    let a = BsaArchive::open(&bsa).unwrap();
    let bytes = a.extract(&path).unwrap();
    let scene = parse_nif(&bytes).unwrap();
    let clips = byroredux_nif::anim::import_kf(&scene);
    println!("clips imported: {}", clips.len());
    for c in &clips {
        println!(
            "  '{}' dur={} channels={} float={} bool={} textkeys={}",
            c.name,
            c.duration,
            c.channels.len(),
            c.float_channels.len(),
            c.bool_channels.len(),
            c.text_keys.len()
        );
    }
    for b in &scene.blocks {
        let Some(seq) = b.as_any().downcast_ref::<NiControllerSequence>() else {
            continue;
        };
        println!(
            "seq '{}' start={} stop={} cb={}",
            seq.name.as_deref().unwrap_or("?"),
            seq.start_time,
            seq.stop_time,
            seq.controlled_blocks.len()
        );
        for (n, cb) in seq.controlled_blocks.iter().enumerate().take(6) {
            let Some(idx) = cb.interpolator_ref.index() else {
                continue;
            };
            if let Some(i) = scene.get_as::<NiTransformInterpolator>(idx) {
                println!("   cb{n} node={:?} type={:?} data_null={} pose t=({:.3},{:.3},{:.3}) r=({:.3},{:.3},{:.3},{:.3}) s={:.3}",
                    cb.node_name.as_deref(), cb.controller_type.as_deref(),
                    i.data_ref.index().is_none(),
                    i.transform.translation.x, i.transform.translation.y, i.transform.translation.z,
                    i.transform.rotation[0], i.transform.rotation[1], i.transform.rotation[2], i.transform.rotation[3],
                    i.transform.scale);
            }
        }
    }
}
