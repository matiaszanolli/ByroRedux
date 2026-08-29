//! #3328 probe — how many embedded float / colour channels the importer
//! surfaces across an archive.
use byroredux_bsa::BsaArchive;

fn main() {
    let bsa = std::env::args().nth(1).expect("usage: <archive>");
    let archive = BsaArchive::open(&bsa).unwrap();
    let (mut files, mut clips, mut floats, mut colors) = (0usize, 0usize, 0usize, 0usize);
    for f in archive.list_files() {
        let l = f.to_ascii_lowercase();
        if !(l.ends_with(".nif") || l.ends_with(".kf")) {
            continue;
        }
        let Ok(bytes) = archive.extract(f) else {
            continue;
        };
        let Ok(scene) = byroredux_nif::parse_nif(&bytes) else {
            continue;
        };
        files += 1;
        if l.ends_with(".kf") {
            for clip in byroredux_nif::anim::import_kf(&scene) {
                clips += 1;
                floats += clip.float_channels.len();
                colors += clip.color_channels.len();
            }
        } else if let Some(clip) = byroredux_nif::anim::import_embedded_animations(&scene) {
            clips += 1;
            floats += clip.float_channels.len();
            colors += clip.color_channels.len();
        }
    }
    println!("files={files} clips={clips} float_channels={floats} color_channels={colors}");
}
