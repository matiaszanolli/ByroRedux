//! Decode every animation clip (and skeleton) of a Fallout 4 animation archive and
//! tally the failures: the check behind reading Havok 2014 spline-compressed clips.
//!
//! ```text
//! cargo run --release -p byroredux-hkx --example clip_census_ba2 -- \
//!     "<Fallout 4>/Data/Fallout4 - Animations.ba2" [path-substring]
//! ```

use byroredux_bsa::Ba2Archive;
use byroredux_hkx::{decode_animation, decode_skeleton};
use std::collections::BTreeMap;

fn main() {
    let mut args = std::env::args().skip(1);
    let archive = Ba2Archive::open(args.next().expect("archive path")).expect("open");
    let filter = args.next().map(|f| f.to_ascii_lowercase());
    let (mut clips_ok, mut skeletons_ok, mut jumpy_clips) = (0, 0, 0);
    let mut errors: BTreeMap<String, (usize, String)> = BTreeMap::new();
    for path in archive.list_files() {
        let lower = path.to_ascii_lowercase();
        if !lower.ends_with(".hkx") || lower.contains("\\behaviors\\") {
            continue;
        }
        if filter.as_ref().is_some_and(|f| !lower.contains(f)) {
            continue;
        }
        let Ok(bytes) = archive.extract(path) else { continue };
        let skeleton = lower.ends_with("skeleton.hkx");
        let result = if skeleton {
            decode_skeleton(&bytes).map(|_| ())
        } else if lower.contains("\\animations\\") {
            decode_animation(&bytes).map(|clip| {
                // A rotation that flips between neighbouring samples means a wrong
                // quaternion decode: count clips with any such jump.
                let jumps = clip
                    .tracks
                    .iter()
                    .flat_map(|track| track.windows(2))
                    .filter(|pair| {
                        let (a, b) = (pair[0].rotation, pair[1].rotation);
                        (a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]).abs() < 0.8
                    })
                    .count();
                if jumps > 0 {
                    jumpy_clips += 1;
                }
            })
        } else {
            continue;
        };
        match result {
            Ok(()) if skeleton => skeletons_ok += 1,
            Ok(()) => clips_ok += 1,
            Err(error) => {
                let entry = errors.entry(error.to_string()).or_insert((0, path.to_owned()));
                entry.0 += 1;
            }
        }
    }
    println!("clips ok {clips_ok} ({jumpy_clips} with a rotation jump between samples), skeletons ok {skeletons_ok}");
    for (error, (count, example)) in errors {
        println!("  {count:6}  {error}   e.g. {example}");
    }
}
