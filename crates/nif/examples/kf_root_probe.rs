//! Print where a `.kf` clip puts its body: for each clip in each named KF of a mesh archive, the
//! accumulation root and the first and last translation of the root, `NonAccum` and pelvis nodes.
//! A sit-enter clip is one whose root drops onto the seat; a stance clip (sneak, crouch) is one
//! whose `NonAccum` height is lowered. Used by PRD `ai-packages-completion` AIP-SIT-001 and
//! AIP-UW-006.
//!
//! ```text
//! cargo run --release -p byroredux-nif --example kf_root_probe -- <meshes.bsa> <path.kf> [more.kf ...]
//! ```

use byroredux_bsa::BsaArchive;

fn main() {
    let mut args = std::env::args().skip(1);
    let bsa = BsaArchive::open(&args.next().expect("usage: kf_root_probe <meshes.bsa> <path.kf> ...")).expect("open");
    for path in args {
        let Ok(bytes) = bsa.extract(&path) else {
            println!("{path}: MISSING");
            continue;
        };
        let scene = match byroredux_nif::parse_nif(&bytes) {
            Ok(scene) => scene,
            Err(error) => {
                println!("{path}: parse failed: {error}");
                continue;
            }
        };
        for clip in byroredux_nif::anim::import_kf(&scene) {
            println!(
                "{path} :: {} dur={:.2}s accum={:?} channels={}",
                clip.name,
                clip.duration,
                clip.accum_root_name,
                clip.channels.len()
            );
            let mut names: Vec<_> = clip.channels.keys().cloned().collect();
            names.sort();
            for name in names {
                let lower = name.to_ascii_lowercase();
                if lower != "bip01" && !lower.contains("nonaccum") && lower != "bip01 pelvis" {
                    continue;
                }
                let channel = &clip.channels[&name];
                match (channel.translation_keys.first(), channel.translation_keys.last()) {
                    (Some(first), Some(last)) => println!(
                        "    {name}: {} keys  first=({:.1},{:.1},{:.1}) last=({:.1},{:.1},{:.1})",
                        channel.translation_keys.len(),
                        first.value.x,
                        first.value.y,
                        first.value.z,
                        last.value.x,
                        last.value.y,
                        last.value.z
                    ),
                    _ => println!("    {name}: no translation keys"),
                }
            }
        }
    }
}
