//! Real-data censuses of the Havok archives the decoders are measured against, and the one
//! way they find them (`#[ignore]`d: `cargo test -p byroredux-hkx -- --ignored`).
//!
//! Every figure pinned below was measured on the shipped archive, not estimated: a change
//! that moves one is either a decoder regression or a new fact to re-measure, and the test
//! says which file moved it. Under `BYROREDUX_REQUIRE_GAME_DATA` an absent archive fails
//! instead of skipping green, and a set `BYROREDUX_<GAME>_DATA` override is binding (#3850).

use crate::animation::MAX_TRANSFORM_SAMPLES;
use crate::behavior::MAX_GRAPH_STEPS;
use crate::packfile::Packfile;
use crate::{
    behavior_graph_steps, decode_animation, decode_behavior_graph, decode_character, decode_project, decode_skeleton,
};
use byroredux_plugin::esm::test_paths;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// `archive` inside the game install the `env` override (or `default`) names.
fn archive_file(env: &str, default: &str, archive: &str) -> Option<PathBuf> {
    let path = test_paths::data_dir(env, default).join(archive);
    if path.is_file() {
        return Some(path);
    }
    if std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok_and(|v| v != "0") {
        panic!("BYROREDUX_REQUIRE_GAME_DATA is set, but {path:?} is not a file ({env} unset, or pointing at the wrong install)");
    }
    eprintln!("SKIP: {} not found (no game data?)", path.display());
    None
}

/// A Skyrim-family `.bsa` of the install `env` / `default` name.
pub(crate) fn bsa_archive(env: &str, default: &str, archive: &str) -> Option<byroredux_bsa::BsaArchive> {
    Some(byroredux_bsa::BsaArchive::open(archive_file(env, default, archive)?).unwrap())
}

pub(crate) fn skyrim_se_animations() -> Option<byroredux_bsa::BsaArchive> {
    bsa_archive(test_paths::SKYRIM_SE_ENV, test_paths::SKYRIM_SE_DEFAULT, "Skyrim - Animations.bsa")
}

pub(crate) fn fallout_4_animations() -> Option<byroredux_bsa::Ba2Archive> {
    Some(byroredux_bsa::Ba2Archive::open(archive_file(test_paths::FO4_ENV, test_paths::FO4_DEFAULT, "Fallout4 - Animations.ba2")?).unwrap())
}

/// Every `.hkx` of `archive` with its bytes; an entry that will not extract is a failure here.
fn hkx_files(names: &[String], extract: &mut dyn FnMut(&str) -> Option<Vec<u8>>) -> Vec<(String, Vec<u8>)> {
    names
        .iter()
        .filter(|name| name.to_ascii_lowercase().ends_with(".hkx"))
        .map(|name| (name.to_ascii_lowercase(), extract(name).unwrap_or_else(|| panic!("{name} does not extract"))))
        .collect()
}

/// The two figures a clip census compares against the decoder's caps.
#[derive(Default)]
struct ClipTally {
    spline: usize,
    lossless: usize,
    additive: usize,
    with_reference_frame: usize,
    reference_frame_dropped: usize,
    partial: usize,
    max_frames: u32,
    max_tracks: usize,
    max_samples: usize,
}

/// `Fallout4 - Animations.ba2`, every `.hkx` in it: what each is, and that each clip decodes.
///
/// The clips are Havok 2014 splines, plus 856 `hkaLosslessCompressedAnimation` (#5897, 589 of them
/// additive deltas), and the longest is a 95-track voice lip-sync clip of 6,149 frames: 584,155
/// samples, 1.7x under `MAX_TRANSFORM_SAMPLES`. The 44 files that are not clips are the 43
/// skeleton-only cloth and hair rigs under `characterassets` and one bare animation container.
#[test]
#[ignore = "needs Fallout 4 game data on disk"]
fn fallout_4_animations_archive_census_is_exact() {
    let Some(archive) = fallout_4_animations() else { return };
    let names: Vec<String> = archive.list_files().iter().map(|name| name.to_string()).collect();
    let files = hkx_files(&names, &mut |name| archive.extract(name).ok());

    let (mut skeletons, mut behaviors, mut characters, mut projects) = (0, 0, 0, 0);
    let mut tally = ClipTally::default();
    let (mut bare_containers, mut rigs) = (Vec::new(), 0);
    for (path, bytes) in &files {
        if path.ends_with("skeleton.hkx") {
            decode_skeleton(bytes).unwrap_or_else(|error| panic!("{path}: {error}"));
            skeletons += 1;
        } else if path.contains("\\behaviors\\") {
            behaviors += 1;
        } else if path.contains("\\characters\\") {
            characters += 1;
        } else if decode_project(bytes).is_ok() {
            projects += 1;
        } else {
            match decode_animation(bytes) {
                Ok(clip) => {
                    let lossless = Packfile::parse(bytes).unwrap().object("hkaLosslessCompressedAnimation").is_ok();
                    tally.spline += usize::from(!lossless);
                    tally.lossless += usize::from(lossless);
                    tally.additive += usize::from(clip.additive);
                    tally.with_reference_frame += usize::from(clip.reference_frame.is_some());
                    tally.reference_frame_dropped += usize::from(clip.reference_frame_dropped.is_some());
                    tally.partial += usize::from(!clip.dropped_bones.is_empty());
                    tally.max_frames = tally.max_frames.max(clip.num_frames);
                    tally.max_tracks = tally.max_tracks.max(clip.tracks.len());
                    tally.max_samples = tally.max_samples.max(clip.tracks.len() * clip.num_frames as usize);
                }
                // A file that is not a clip: the cloth and hair rigs (28 hair, 15 clothing and
                // seaweed) hold a skeleton and no animation; the one under `animations` is a
                // container with no animation object in it.
                Err(_) if path.contains("\\characterassets\\") => rigs += 1,
                Err(error) => bare_containers.push(format!("{path}: {error}")),
            }
        }
    }
    assert_eq!((skeletons, behaviors, characters, projects), (121, 328, 222, 235));
    assert_eq!((tally.spline, tally.lossless), (13_514, 856), "clips by kind");
    assert_eq!(tally.additive, 848, "clips marked additive (589 lossless, 259 spline)");
    assert_eq!(tally.with_reference_frame, 13_543, "clips with extracted motion");
    assert_eq!((tally.reference_frame_dropped, tally.partial), (0, 0), "no clip loses a frame or a track");
    assert_eq!((tally.max_frames, tally.max_tracks, tally.max_samples), (6_149, 120, 584_155));
    assert!(tally.max_samples <= MAX_TRANSFORM_SAMPLES, "the cap sits above the largest shipped clip");
    assert_eq!(rigs, 43, "cloth and hair rigs are the only non-clips outside `animations`");
    assert_eq!(
        bare_containers,
        ["meshes\\actors\\deathclaw\\animations\\deathclaw_specialidleb.hkx: HKX object class 'hkaSplineCompressedAnimation' is missing"],
        "the one animation that is a bare hkaAnimationContainer",
    );
    assert_eq!(files.len(), 121 + 328 + 222 + 235 + 14_370 + 43 + 1, "every .hkx is accounted for");
}

/// The behavior graphs and characters of the same archive: all decode, and the largest takes 115,605
/// steps - the figure `MAX_GRAPH_STEPS` is set 8x above.
#[test]
#[ignore = "needs Fallout 4 game data on disk"]
fn fallout_4_behavior_graph_census_is_exact() {
    let Some(archive) = fallout_4_animations() else { return };
    let names: Vec<String> = archive.list_files().iter().map(|name| name.to_string()).collect();
    let (mut behaviors, mut characters, mut max_steps, mut worst) = (0, 0, 0, String::new());
    for (path, bytes) in hkx_files(&names, &mut |name| archive.extract(name).ok()) {
        if path.contains("\\behaviors\\") {
            decode_behavior_graph(&bytes).unwrap_or_else(|error| panic!("{path}: {error}"));
            let steps = behavior_graph_steps(&bytes).unwrap();
            if steps > max_steps {
                (max_steps, worst) = (steps, path);
            }
            behaviors += 1;
        } else if path.contains("\\characters\\") {
            decode_character(&bytes).unwrap_or_else(|error| panic!("{path}: {error}"));
            characters += 1;
        }
    }
    assert_eq!((behaviors, characters), (328, 222));
    assert_eq!((max_steps, worst.as_str()), (115_605, "meshes\\actors\\character\\_1stperson\\behaviors\\gunbehavior.hkx"));
    assert!(max_steps * 8 <= MAX_GRAPH_STEPS, "the step cap keeps 8x headroom over the shipped maximum");
}

/// Skyrim SE's behavior graphs: 420 decode (the largest takes 353 steps); the three that do not are
/// the `_lod` variants of the goat and the frostbite spider, which carry no string data.
#[test]
#[ignore = "needs Skyrim SE game data on disk"]
fn skyrim_se_behavior_graph_census_is_exact() {
    let Some(archive) = skyrim_se_animations() else { return };
    let names: Vec<String> = archive.list_files().iter().map(|name| name.to_string()).collect();
    let (mut decoded, mut max_steps) = (0, 0);
    let mut rejects = BTreeMap::new();
    for (path, bytes) in hkx_files(&names, &mut |name| archive.extract(name).ok()) {
        if !path.contains("\\behaviors\\") {
            continue;
        }
        match behavior_graph_steps(&bytes) {
            Ok(steps) => {
                decoded += 1;
                max_steps = max_steps.max(steps);
            }
            Err(error) => {
                rejects.insert(path, error.to_string());
            }
        }
    }
    assert_eq!((decoded, max_steps), (420, 353));
    let missing = "HKX object class 'hkbBehaviorGraphStringData' is missing";
    let expected: BTreeMap<String, String> = [
        "meshes\\actors\\frostbitespider\\behaviors\\frostbitespiderbehavior_lod.hkx",
        "meshes\\actors\\goat\\behaviors\\goatbehavior_lod.hkx",
        "meshes\\actors\\goat\\behaviors\\noncombatidle_lod.hkx",
    ]
    .into_iter()
    .map(|path| (path.to_owned(), missing.to_owned()))
    .collect();
    assert_eq!(rejects, expected);
}
