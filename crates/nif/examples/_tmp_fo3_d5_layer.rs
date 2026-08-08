//! TEMP scratch: FO3 D5 — Havok collision-filter (layer) census, both at the
//! rigid-body level and at the per-sub-shape level inside hkPackedNiTriStripsData
//! (which the parser currently skips).
use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::collision::*;
use byroredux_nif::import::collision::extract_collision;
use byroredux_nif::parse_nif;
use byroredux_nif::types::BlockRef;
use byroredux_core::ecs::components::collision::CollisionShape;
use std::collections::BTreeMap;

fn fol(l: u8) -> &'static str {
    match l {
        0=>"UNIDENTIFIED",1=>"STATIC",2=>"ANIMSTATIC",3=>"TRANSPARENT",4=>"CLUTTER",5=>"WEAPON",
        6=>"PROJECTILE",7=>"SPELL",8=>"BIPED",9=>"TREES",10=>"PROPS",11=>"WATER",12=>"TRIGGER",
        13=>"TERRAIN",14=>"TRAP",15=>"NONCOLLIDABLE",16=>"CLOUD_TRAP",17=>"GROUND",18=>"PORTAL",
        19=>"DEBRIS_SMALL",20=>"DEBRIS_LARGE",21=>"ACOUSTIC_SPACE",22=>"ACTORZONE",
        23=>"PROJECTILEZONE",24=>"GASTRAP",25=>"SHELLCASING",26=>"TRANSPARENT_SMALL",
        27=>"INVISIBLE_WALL",28=>"TRANSPARENT_SMALL_ANIM",29=>"DEADBIP",30=>"CHARCONTROLLER",
        31=>"AVOIDBOX",32=>"COLLISIONBOX",33=>"CAMERASPHERE",34=>"DOORDETECTION",35=>"CAMERAPICK",
        36=>"ITEMPICK",37=>"LINEOFSIGHT",38=>"PATHPICK",39=>"CUSTOMPICK",40=>"SPELLEXPLOSION",
        41=>"DROPPINGPICK",42=>"NULL",_=>"?",
    }
}

fn main() {
    let mut layer_hist: BTreeMap<String, usize> = BTreeMap::new();
    let mut layer_files: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut sub_layer_hist: BTreeMap<String, usize> = BTreeMap::new();
    let mut sub_mat_hist: BTreeMap<u32, usize> = BTreeMap::new();
    let mut sub_count_hist: BTreeMap<usize, usize> = BTreeMap::new();
    let mut mixed_layer_meshes = 0usize;
    let mut mixed_mat_meshes = 0usize;
    let mut mixed_layer_examples: Vec<String> = Vec::new();
    let mut zero_radius = 0usize;
    let mut degenerate = 0usize;

    for path in std::env::args().skip(1) {
        let Ok(arc) = BsaArchive::open(&path) else { continue };
        let files: Vec<String> = arc.list_files().into_iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".nif")).map(|s| s.to_string()).collect();
        for name in &files {
            let Ok(bytes) = arc.extract(name) else { continue };
            let Ok((hdr, data_start)) = byroredux_nif::header::NifHeader::parse(&bytes) else { continue };
            let Ok(scene) = parse_nif(&bytes) else { continue };
            // Byte offset of each block (block_sizes present on FO3).
            let mut offsets = Vec::with_capacity(hdr.block_sizes.len());
            let mut off = data_start;
            for &sz in &hdr.block_sizes { offsets.push(off); off += sz as usize; }

            for (i, block) in scene.blocks.iter().enumerate() {
                // ---- per-sub-shape filters inside hkPackedNiTriStripsData ----
                if let Some(d) = block.as_any().downcast_ref::<HkPackedNiTriStripsData>() {
                    if let (Some(&o), Some(&sz)) = (offsets.get(i), hdr.block_sizes.get(i)) {
                        let end = o + sz as usize;
                        if end <= bytes.len() && end >= 2 {
                            // Recompute where the sub-shape array starts.
                            let t = d.triangles.len();
                            let v = d.vertices.len();
                            // FO3: 4 + 8*T + 4 + 1 + 12*V  (uncompressed) then 2 + 12*N
                            let mut cur = o + 4 + 8 * t + 4 + 1 + 12 * v;
                            if cur + 2 > end { cur = o + 4 + 8 * t + 4 + 1 + 6 * v; }
                            if cur + 2 <= end {
                                let n = u16::from_le_bytes([bytes[cur], bytes[cur + 1]]) as usize;
                                if cur + 2 + 12 * n == end {
                                    *sub_count_hist.entry(n).or_default() += 1;
                                    let mut layers = std::collections::BTreeSet::new();
                                    let mut mats = std::collections::BTreeSet::new();
                                    for k in 0..n {
                                        let b = cur + 2 + 12 * k;
                                        let filter = u32::from_le_bytes([bytes[b],bytes[b+1],bytes[b+2],bytes[b+3]]);
                                        let mat = u32::from_le_bytes([bytes[b+8],bytes[b+9],bytes[b+10],bytes[b+11]]);
                                        let l = (filter & 0xFF) as u8;
                                        layers.insert(l);
                                        mats.insert(mat);
                                        *sub_layer_hist.entry(format!("{l:02}:{}", fol(l))).or_default() += 1;
                                        *sub_mat_hist.entry(mat).or_default() += 1;
                                    }
                                    if layers.len() > 1 {
                                        mixed_layer_meshes += 1;
                                        if mixed_layer_examples.len() < 10 {
                                            mixed_layer_examples.push(format!("{name} layers={:?}", layers.iter().map(|&l| fol(l)).collect::<Vec<_>>()));
                                        }
                                    }
                                    if mats.len() > 1 { mixed_mat_meshes += 1; }
                                }
                            }
                        }
                    }
                }
                // ---- rigid-body level layer ----
                if !block.as_any().is::<BhkCollisionObject>() { continue; }
                let co = block.as_any().downcast_ref::<BhkCollisionObject>().unwrap();
                let Some(bi) = co.body_ref.index() else { continue };
                let Some(rb) = scene.get_as::<BhkRigidBody>(bi) else { continue };
                let layer = (rb.havok_filter & 0xFF) as u8;
                let key = format!("{layer:02}:{}", fol(layer));
                if let Some((s, bd)) = extract_collision(&scene, BlockRef(i as u32)) {
                    let mt = format!("{:?}", bd.motion_type);
                    *layer_hist.entry(format!("{key}/{mt}")).or_default() += 1;
                    let e = layer_files.entry(key).or_default();
                    if e.len() < 2 { e.push(name.clone()); }
                    match &s {
                        CollisionShape::Ball { radius } if *radius <= 0.0 => zero_radius += 1,
                        CollisionShape::Cuboid { half_extents } if half_extents.min_element() <= 0.0 => degenerate += 1,
                        _ => {}
                    }
                }
            }
        }
    }
    println!("== LIVE collider count by rigid-body Havok layer ==");
    for (k, v) in &layer_hist { println!("{v:7}  {k}   e.g. {:?}", layer_files.get(k).map(|f| &f[..])); }
    println!("\n== hkPackedNiTriStripsData sub-shape count histogram (N -> #data blocks) ==\n{sub_count_hist:?}");
    println!("\n== per-sub-shape layer histogram ==");
    for (k, v) in &sub_layer_hist { println!("{v:7}  {k}"); }
    println!("\nmeshes with >1 distinct sub-shape LAYER = {mixed_layer_meshes}");
    println!("meshes with >1 distinct sub-shape MATERIAL = {mixed_mat_meshes}");
    println!("distinct sub-shape materials = {}", sub_mat_hist.len());
    println!("sub-shape material hist = {sub_mat_hist:?}");
    for e in &mixed_layer_examples { println!("  {e}"); }
    println!("zero_radius={zero_radius} degenerate_cuboid={degenerate}");
}
