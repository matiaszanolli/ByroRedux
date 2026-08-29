//! #3330 probe — authored vs surfaced ragdoll constraint counts plus a
//! union-find over the surfaced joint graph, for named NIFs inside a BSA.
//!
//! Usage: ragdoll_components <archive> <substr>

use byroredux_bsa::BsaArchive;

fn find(parent: &mut Vec<usize>, x: usize) -> usize {
    if parent[x] != x {
        let r = find(parent, parent[x]);
        parent[x] = r;
    }
    parent[x]
}

fn main() {
    let bsa = std::env::args().nth(1).expect("usage: <archive> <substr>");
    let needle = std::env::args()
        .nth(2)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let archive = BsaArchive::open(&bsa).unwrap();
    let names: Vec<String> = archive
        .list_files()
        .iter()
        .filter(|f| {
            let l = f.to_ascii_lowercase();
            l.ends_with(".nif") && l.contains(&needle)
        })
        .map(|f| f.to_string())
        .collect();
    for name in names {
        let Ok(bytes) = archive.extract(&name) else {
            continue;
        };
        let Ok(scene) = byroredux_nif::parse_nif(&bytes) else {
            continue;
        };
        // Authored constraint blocks of any kind.
        let authored = scene
            .blocks
            .iter()
            .filter(|b| {
                let a = b.as_any();
                a.downcast_ref::<byroredux_nif::blocks::collision::BhkConstraint>()
                    .is_some()
                    || a.downcast_ref::<byroredux_nif::blocks::collision::BhkBreakableConstraint>()
                        .is_some()
            })
            .count();
        let Some(rag) = byroredux_nif::import::collision::extract_ragdoll(&scene) else {
            continue;
        };
        if rag.bodies.is_empty() {
            continue;
        }
        let n = rag.bodies.len();
        let mut parent: Vec<usize> = (0..n).collect();
        for j in &rag.constraints {
            let (a, b) = (find(&mut parent, j.body_a), find(&mut parent, j.body_b));
            if a != b {
                parent[a] = b;
            }
        }
        let mut groups: std::collections::BTreeMap<usize, Vec<&str>> = Default::default();
        for i in 0..n {
            let r = find(&mut parent, i);
            groups.entry(r).or_default().push(&rag.bodies[i].bone_name);
        }
        println!(
            "{name}: {authored} authored -> {} surfaced, {} component(s), {n} bodies",
            rag.constraints.len(),
            groups.len()
        );
        if groups.len() > 1 {
            for g in groups.values() {
                println!("    {:?}", g);
            }
        }
    }
}
