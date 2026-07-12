// Throwaway research probe (M42 sit-enter): dump a KF clip's transform
// channels — especially the accum root (Bip01) translation, to see whether a
// clip lowers the body onto the seat.
use byroredux_nif::anim::import_kf;
use std::fs;

fn main() {
    let path = std::env::args().nth(1).expect("usage: kf_channel_dump <file.kf>");
    let bytes = fs::read(&path).unwrap();
    let scene = byroredux_nif::parse_nif(&bytes).unwrap();
    let clips = import_kf(&scene);
    println!("clips: {}", clips.len());
    for c in &clips {
        println!(
            "\nclip '{}' dur={:.3}s cycle={:?} accum_root={:?} channels={}",
            c.name, c.duration, c.cycle_type, c.accum_root_name, c.channels.len(),
        );
        let mut names: Vec<_> = c.channels.iter().collect();
        names.sort_by_key(|(n, _)| n.to_string());
        for (name, ch) in names {
            // Only report channels that carry translation (the ones that could
            // move/lower the skeleton), plus the accum root regardless.
            let is_root = c.accum_root_name.as_deref() == Some(name.as_ref());
            if ch.translation_keys.len() <= 1 && !is_root {
                continue;
            }
            let t0 = ch.translation_keys.first().map(|k| k.value);
            let t1 = ch.translation_keys.last().map(|k| k.value);
            println!(
                "  {:<24}{} tKeys={:>3} startT={:?} endT={:?}",
                name.as_ref(),
                if is_root { " [ACCUM]" } else { "" },
                ch.translation_keys.len(),
                t0.map(rd),
                t1.map(rd),
            );
        }
    }
}

fn rd(v: [f32; 3]) -> [f32; 3] {
    [(v[0] * 100.0).round() / 100.0, (v[1] * 100.0).round() / 100.0, (v[2] * 100.0).round() / 100.0]
}
