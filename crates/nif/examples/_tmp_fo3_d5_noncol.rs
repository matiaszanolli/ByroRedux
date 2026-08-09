use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::collision::*;
use byroredux_nif::import::collision::extract_collision;
use byroredux_nif::parse_nif;
use byroredux_nif::types::BlockRef;
use std::collections::BTreeMap;
fn main() {
    let mut flag_hist: BTreeMap<u16, usize> = BTreeMap::new();
    let mut noncol: Vec<String> = Vec::new();
    let mut weapon: Vec<String> = Vec::new();
    let mut proj: Vec<String> = Vec::new();
    let mut biped_files: std::collections::BTreeSet<String> = Default::default();
    let mut biped_per_file: BTreeMap<String, usize> = BTreeMap::new();
    for path in std::env::args().skip(1) {
        let Ok(arc) = BsaArchive::open(&path) else {
            continue;
        };
        for name in arc
            .list_files()
            .into_iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
            .map(|s| s.to_string())
        {
            let Ok(bytes) = arc.extract(&name) else {
                continue;
            };
            let Ok(scene) = parse_nif(&bytes) else {
                continue;
            };
            for (i, block) in scene.blocks.iter().enumerate() {
                let Some(co) = block.as_any().downcast_ref::<BhkCollisionObject>() else {
                    continue;
                };
                *flag_hist.entry(co.flags).or_default() += 1;
                let Some(bi) = co.body_ref.index() else {
                    continue;
                };
                let Some(rb) = scene.get_as::<BhkRigidBody>(bi) else {
                    continue;
                };
                let l = (rb.havok_filter & 0xFF) as u8;
                if extract_collision(&scene, BlockRef(i as u32)).is_none() {
                    continue;
                }
                match l {
                    15 => {
                        if !noncol.contains(&name) {
                            noncol.push(name.clone())
                        }
                    }
                    5 => {
                        if weapon.len() < 12 && !weapon.contains(&name) {
                            weapon.push(name.clone())
                        }
                    }
                    6 => {
                        if !proj.contains(&name) {
                            proj.push(name.clone())
                        }
                    }
                    8 => {
                        biped_files.insert(name.clone());
                        *biped_per_file.entry(name.clone()).or_default() += 1;
                    }
                    _ => {}
                }
            }
        }
    }
    println!("bhkCollisionObject flags hist: {flag_hist:?}");
    println!("\nNONCOLLIDABLE files ({}):", noncol.len());
    for f in &noncol {
        println!("  {f}");
    }
    println!("\nPROJECTILE files ({}):", proj.len());
    for f in proj.iter().take(20) {
        println!("  {f}");
    }
    println!("\nWEAPON sample: {weapon:?}");
    println!("\nBIPED files = {}", biped_files.len());
    let mut v: Vec<_> = biped_per_file.into_iter().collect();
    v.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    for (f, c) in v.into_iter().take(10) {
        println!("  {c:4}  {f}");
    }
}
