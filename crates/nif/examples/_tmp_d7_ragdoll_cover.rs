//! TEMP scratch: D7 — which skeleton bones are covered by the ragdoll
//! writeback (body bones + their descendants) vs left on the animated pose,
//! and whether the vanilla FNV body mesh skins to any uncovered bone.
use byroredux_bsa::BsaArchive;
use byroredux_core::string::StringPool;
use byroredux_nif::import::import_nif_scene;
use byroredux_nif::parse_nif;
use std::collections::{HashMap, HashSet};

fn main() {
    let bsa = std::env::args().nth(1).unwrap();
    let skel = std::env::args().nth(2).unwrap();
    let arc = BsaArchive::open(&bsa).expect("open bsa");

    let bytes = arc.extract(&skel).expect("extract skeleton");
    let scene = parse_nif(&bytes).expect("parse");
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);
    let rag = imported.ragdoll.as_ref().expect("ragdoll");

    // node index -> name; children adjacency
    let n = imported.nodes.len();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, nd) in imported.nodes.iter().enumerate() {
        if let Some(p) = nd.parent_node { children[p].push(i); }
    }
    let name_of = |i: usize| imported.nodes[i].name.as_deref().unwrap_or("<unnamed>").to_string();
    let mut idx_by_name: HashMap<&str, usize> = HashMap::new();
    for (i, nd) in imported.nodes.iter().enumerate() {
        if let Some(nm) = nd.name.as_ref() { idx_by_name.entry(nm.as_ref()).or_insert(i); }
    }

    let bodies: HashSet<usize> = rag.bodies.iter()
        .filter_map(|b| idx_by_name.get(b.bone_name.as_ref()).copied())
        .collect();

    // Covered = body bones + all descendants of body bones.
    let mut covered: HashSet<usize> = bodies.clone();
    let mut stack: Vec<usize> = bodies.iter().copied().collect();
    while let Some(p) = stack.pop() {
        for &c in &children[p] { if covered.insert(c) { stack.push(c); } }
    }
    let uncovered: Vec<usize> = (0..n).filter(|i| !covered.contains(i)).collect();
    println!("skeleton nodes={n} bodies={} covered={} uncovered={}", bodies.len(), covered.len(), uncovered.len());
    for i in &uncovered {
        println!("  UNCOVERED: {} (parent {:?})", name_of(*i), imported.nodes[*i].parent_node.map(name_of));
    }

    // Now: the vanilla body meshes' skin bones.
    for mesh_path in [
        r"meshes\characters\_male\upperbody.nif",
        r"meshes\characters\_male\lefthand.nif",
        r"meshes\characters\_male\righthand.nif",
        r"meshes\characters\head\headhuman.nif",
    ] {
        let Ok(mb) = arc.extract(mesh_path) else { println!("{mesh_path}: not found"); continue; };
        let Ok(msc) = parse_nif(&mb) else { println!("{mesh_path}: parse fail"); continue; };
        let mut p2 = StringPool::new();
        let mi = import_nif_scene(&msc, &mut p2);
        let mut uncov_bones: HashSet<String> = HashSet::new();
        let mut total_bones: HashSet<String> = HashSet::new();
        for m in &mi.meshes {
            if let Some(sk) = m.skin.as_ref() {
                for b in &sk.bones {
                    total_bones.insert(b.name.to_string());
                    match idx_by_name.get(b.name.as_ref()) {
                        Some(i) if covered.contains(i) => {}
                        Some(_) => { uncov_bones.insert(b.name.to_string()); }
                        None => { uncov_bones.insert(format!("{} (NOT IN SKELETON)", b.name)); }
                    }
                }
            }
        }
        println!("{mesh_path}: {} skin bones, {} NOT covered by ragdoll writeback", total_bones.len(), uncov_bones.len());
        let mut v: Vec<_> = uncov_bones.into_iter().collect();
        v.sort();
        for b in v { println!("    uncovered skin bone: {b}"); }
    }
}
