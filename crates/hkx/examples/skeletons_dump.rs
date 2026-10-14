//! Dump every `hkaSkeleton` of a packfile with its bone names, parents and
//! reference pose (#5015: the second skeleton of a character's `skeleton.hkx`
//! is the ragdoll's, whose bone order the `XRGD` pose entries index).
//!
//! ```text
//! cargo run -p byroredux-hkx --example skeletons_dump -- <skeleton.hkx>
//! ```
fn main() {
    let bytes = std::fs::read(std::env::args().nth(1).expect("a packfile")).expect("read");
    for (n, skeleton) in byroredux_hkx::decode_skeletons(&bytes).expect("decode").iter().enumerate() {
        println!("# skeleton {n} '{}' — {} bones", skeleton.name, skeleton.bones.len());
        for (i, b) in skeleton.bones.iter().enumerate() {
            println!("{n} {i:3} {:?} parent={} pose={:?}", b.name, b.parent_index, b.reference_pose);
        }
    }
}
