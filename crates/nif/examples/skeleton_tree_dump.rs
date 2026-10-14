//! #5015 probe: dump a NIF's NiNode hierarchy with each node's LOCAL
//! translation (Z-up, as authored) and rotation matrix, one JSON line per node:
//! `{"name":..,"parent":..,"t":[x,y,z],"r":[[..],[..],[..]]}`.
//!
//! Lets the XRGD analysis compare a pose entry's parent-relative offset with
//! the skeleton's own bone offsets (no `serde_json` here; fields are plain).
//!
//! Usage: skeleton_tree_dump <path.nif>

use byroredux_nif::blocks::node::NiNode;

fn main() {
    let path = std::env::args().nth(1).expect("usage: skeleton_tree_dump <path.nif>");
    let bytes = std::fs::read(&path).expect("read");
    let scene = byroredux_nif::parse_nif(&bytes).expect("parse");
    let mut parent = vec![None; scene.blocks.len()];
    for (i, _) in scene.blocks.iter().enumerate() {
        if let Some(node) = scene.get_as::<NiNode>(i) {
            for child in &node.children {
                if let Some(c) = child.index() {
                    parent[c] = Some(i);
                }
            }
        }
    }
    for (i, _) in scene.blocks.iter().enumerate() {
        let Some(node) = scene.get_as::<NiNode>(i) else { continue };
        let name = node.av.net.name.as_deref().unwrap_or("");
        let parent_name = parent[i]
            .and_then(|p| scene.get_as::<NiNode>(p))
            .and_then(|p| p.av.net.name.as_deref())
            .unwrap_or("");
        let t = node.av.transform.translation;
        let r = node.av.transform.rotation.rows;
        println!(
            "{{\"name\":\"{name}\",\"parent\":\"{parent_name}\",\"t\":[{:?},{:?},{:?}],\"r\":{:?}}}",
            t.x, t.y, t.z, r
        );
    }
}
