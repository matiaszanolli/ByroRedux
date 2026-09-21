//! Decode-verdict probe for standalone HKX animation clips — the P2
//! combat-tail fixture pass (`docs/engine/p2-combat-anim-sound-fixture.md`)
//! used this to pin the Draugr family, and the animation wiring will reuse
//! it to print annotation payloads before choosing hit-timing markers.
//!
//! ```text
//! cargo run -p byroredux-hkx --example probe_clip -- <file.hkx> [more.hkx ...]
//! ```
//!
//! Standalone clips (Draugr attacks, cart idles) carry the animation only —
//! the `hkaSkeleton` lives in the rig file, so skeleton decode failing on a
//! clip is expected, and vice versa.

use byroredux_hkx::{decode_skeleton, decode_spline_animation};

fn main() {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).expect("read");
        println!("== {path} ({} bytes)", bytes.len());
        match decode_skeleton(&bytes) {
            Ok(s) => println!(
                "  skeleton: {} bones, root={}",
                s.bones.len(),
                s.bones.first().map(|b| b.name.as_str()).unwrap_or("?")
            ),
            Err(e) => println!("  skeleton: not a rig ({e})"),
        }
        match decode_spline_animation(&bytes) {
            Ok(a) => println!(
                "  animation: dur={:.2}s frames={} tracks={} annotations={}",
                a.duration,
                a.num_frames,
                a.tracks.len(),
                a.annotations.len()
            ),
            Err(e) => println!("  animation: not a clip ({e})"),
        }
    }
}
