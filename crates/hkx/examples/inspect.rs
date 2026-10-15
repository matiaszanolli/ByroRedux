//! One entry point for the hkx "open a packfile, print one thing" probes
//! (#5856 folded six 13-18-line examples into subcommands; the multi-page
//! censuses stay as their own examples).
//!
//! ```text
//! cargo run -p byroredux-hkx --example inspect -- classes <file.hkx>
//! cargo run -p byroredux-hkx --example inspect -- objects <file.hkx> <class> [bytes-per-object]
//! cargo run -p byroredux-hkx --example inspect -- clip <file.hkx>
//! cargo run -p byroredux-hkx --example inspect -- skeleton <skeleton.hkx> [max-bones]
//! cargo run -p byroredux-hkx --example inspect -- skeletons <file.hkx>
//! cargo run -p byroredux-hkx --example inspect -- start-clip <archive.ba2> <behavior path>
//! ```

use byroredux_bsa::Ba2Archive;

fn main() {
    let mut args = std::env::args().skip(1);
    let command = args.next().expect("a subcommand (see the module doc)");
    let rest = args.collect::<Vec<_>>();
    match command.as_str() {
        "classes" => classes(&rest),
        "objects" => objects(&rest),
        "clip" => clip(&rest),
        "skeleton" => skeleton(&rest),
        "skeletons" => skeletons(&rest),
        "start-clip" => start_clip(&rest),
        other => panic!("unknown subcommand {other:?} (see the module doc)"),
    }
}

/// `classes <file.hkx>` — how many objects of each class the packfile holds.
fn classes(args: &[String]) {
    let bytes = std::fs::read(&args[0]).expect("read");
    for (class, count) in byroredux_hkx::class_census(&bytes).expect("census") {
        println!("{count:6} {class}");
    }
}

/// `objects <file.hkx> <class> [bytes-per-object]` — the 8-byte slots of
/// every object of a class: the evidence for a Havok Behavior class layout.
fn objects(args: &[String]) {
    let size: usize = args.get(2).map_or(0xA8, |n| n.parse().expect("a byte count"));
    let bytes = std::fs::read(&args[0]).expect("read");
    for line in byroredux_hkx::dump_objects(&bytes, &args[1], size).expect("dump") {
        println!("{line}");
    }
}

/// `clip <file.hkx>` — a spline animation's duration, tracks and annotations.
fn clip(args: &[String]) {
    let bytes = std::fs::read(&args[0]).expect("read");
    match byroredux_hkx::decode_spline_animation(&bytes) {
        Ok(animation) => println!("{animation:#?}"),
        Err(error) => println!("not a spline animation: {error}"),
    }
}

/// `skeleton <skeleton.hkx> [max-bones]` — bone names and parents.
fn skeleton(args: &[String]) {
    let max: usize = args.get(1).map_or(12, |n| n.parse().expect("a count"));
    let skeleton = byroredux_hkx::decode_skeleton(&std::fs::read(&args[0]).expect("read")).expect("decode");
    println!("{} bones", skeleton.bones.len());
    for (index, bone) in skeleton.bones.iter().take(max).enumerate() {
        println!("{index:3} {} (parent {:?})", bone.name, bone.parent_index);
    }
}

/// `skeletons <file.hkx>` — every `hkaSkeleton` with names, parents and the
/// reference pose (#5015: a character file's second skeleton is the ragdoll's).
fn skeletons(args: &[String]) {
    let bytes = std::fs::read(&args[0]).expect("read");
    for (n, skeleton) in byroredux_hkx::decode_skeletons(&bytes).expect("decode").iter().enumerate() {
        println!("# skeleton {n} '{}' — {} bones", skeleton.name, skeleton.bones.len());
        for (i, b) in skeleton.bones.iter().enumerate() {
            println!("{n} {i:3} {:?} parent={} pose={:?}", b.name, b.parent_index, b.reference_pose);
        }
    }
}

/// `start-clip <archive.ba2> <behavior path>` — the idle clip an actor
/// standing in a Fallout 4 behavior graph starts in.
fn start_clip(args: &[String]) {
    let archive = Ba2Archive::open(&args[0]).expect("open");
    let graph =
        byroredux_hkx::decode_behavior_graph(&archive.extract(&args[1]).expect("extract")).expect("decode");
    let mut loader = |reference: &str| {
        let path = format!(
            "meshes\\actors\\character\\{}",
            reference.trim_start_matches(['\\', '/']).to_ascii_lowercase()
        );
        let bytes = archive.extract(&path).ok()?;
        byroredux_hkx::decode_behavior_graph(&bytes).ok()
    };
    println!("{:?}", graph.start_clip(&mut loader).map(|clip| clip.animation));
}
