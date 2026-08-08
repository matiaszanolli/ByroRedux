//! TEMP scratch: FO3 dimension-5 (Havok collision import) survey.
use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::collision::*;
use byroredux_nif::import::collision::{
    examine_collision_kind, extract_collision, summarize_collision_authoring, CollisionAuthoring,
};
use byroredux_nif::parse_nif;
use byroredux_core::ecs::components::collision::{CollisionShape, MotionType};
use std::collections::BTreeMap;

fn shape_name(s: &CollisionShape) -> &'static str {
    match s {
        CollisionShape::Ball { .. } => "Ball",
        CollisionShape::Cuboid { .. } => "Cuboid",
        CollisionShape::Capsule { .. } => "Capsule",
        CollisionShape::Cylinder { .. } => "Cylinder",
        CollisionShape::ConvexHull { .. } => "ConvexHull",
        CollisionShape::TriMesh { .. } => "TriMesh",
        CollisionShape::Compound { .. } => "Compound",
    }
}

fn main() {
    let mut kind_hist: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut shape_hist: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut motion_hist: BTreeMap<String, usize> = BTreeMap::new();
    let mut raw_motion_hist: BTreeMap<u8, usize> = BTreeMap::new();
    let mut blocktype_hist: BTreeMap<String, usize> = BTreeMap::new();
    let mut classic_ok = 0usize;
    let mut classic_fail = 0usize;
    let mut classic_fail_files: Vec<String> = Vec::new();
    let mut nfiles = 0usize;
    let mut files_with_coll = 0usize;
    // material histogram from shapes we can see
    let mut havok_scale_hist: BTreeMap<String, usize> = BTreeMap::new();
    // ConvexList / MultiSphere detail
    let mut cvxlist_children: BTreeMap<usize, usize> = BTreeMap::new();
    let mut cvxlist_resolved_children: BTreeMap<usize, usize> = BTreeMap::new();
    let mut multisphere_count = 0usize;
    // list shape children lost
    let mut list_lost = 0usize;
    let mut list_total_children = 0usize;
    // blend collision objects
    let mut blend_objs = 0usize;
    let mut blend_extract_ok = 0usize;

    for path in std::env::args().skip(1) {
        let Ok(arc) = BsaArchive::open(&path) else { eprintln!("skip {path}"); continue };
        let files: Vec<String> = arc
            .list_files()
            .into_iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
            .map(|s| s.to_string())
            .collect();
        eprintln!("{path}: {} nifs", files.len());
        for name in &files {
            let Ok(bytes) = arc.extract(name) else { continue };
            let hdr = byroredux_nif::header::NifHeader::parse(&bytes).ok();
            let Ok(scene) = parse_nif(&bytes) else { continue };
            nfiles += 1;
            *havok_scale_hist.entry(format!("{:.4}", scene.havok_scale)).or_default() += 1;
            let sum = summarize_collision_authoring(&scene);
            if sum.classic + sum.new_physics + sum.phantom > 0 {
                files_with_coll += 1;
            }
            // header type names by index
            let names: Vec<String> = match &hdr {
                Some((h, _)) => h
                    .block_type_indices
                    .iter()
                    .map(|&i| h.block_types.get(i as usize).map(|s| s.to_string()).unwrap_or_default())
                    .collect(),
                None => Vec::new(),
            };
            for (i, block) in scene.blocks.iter().enumerate() {
                let tn = names.get(i).cloned().unwrap_or_else(|| block.block_type_name().to_string());
                if tn.starts_with("bhk") || tn.starts_with("hk") {
                    *blocktype_hist.entry(tn.clone()).or_default() += 1;
                }
                if tn == "bhkBlendCollisionObject" {
                    blend_objs += 1;
                }
                // ConvexList detail
                if let Some(cl) = block.as_any().downcast_ref::<BhkConvexListShape>() {
                    *cvxlist_children.entry(cl.sub_shapes.len()).or_default() += 1;
                    let mut ok = 0;
                    for r in &cl.sub_shapes {
                        if let Some(ci) = r.index() {
                            if let Some(cb) = scene.get(ci) {
                                let n = names.get(ci).cloned().unwrap_or_else(|| cb.block_type_name().to_string());
                                if n != "NiUnknown" { ok += 1; }
                            }
                        }
                    }
                    *cvxlist_resolved_children.entry(ok).or_default() += 1;
                }
                if block.as_any().is::<BhkMultiSphereShape>() { multisphere_count += 1; }
                if let Some(ls) = block.as_any().downcast_ref::<BhkListShape>() {
                    list_total_children += ls.sub_shape_refs.len();
                }
                if let Some(rb) = block.as_any().downcast_ref::<BhkRigidBody>() {
                    *raw_motion_hist.entry(rb.motion_type).or_default() += 1;
                }
                // collision-object blocks (equivalently reachable via AVObject.collision_ref)
                if !(block.as_any().is::<BhkCollisionObject>()
                    || block.as_any().is::<BhkNPCollisionObject>()
                    || block.as_any().is::<BhkPCollisionObject>()) { continue; }
                let cref = byroredux_nif::types::BlockRef(i as u32);
                let kind = examine_collision_kind(&scene, cref);
                let kname = match kind {
                    CollisionAuthoring::None => "None",
                    CollisionAuthoring::Classic => "Classic",
                    CollisionAuthoring::NewPhysicsStub => "NP",
                    CollisionAuthoring::Phantom => "Phantom",
                    CollisionAuthoring::Unrecognised => "Unrecognised",
                };
                *kind_hist.entry(kname).or_default() += 1;
                let got = extract_collision(&scene, cref);
                if kind == CollisionAuthoring::Classic {
                    match &got {
                        Some((s, rb)) => {
                            classic_ok += 1;
                            *shape_hist.entry(shape_name(s)).or_default() += 1;
                            let m = match rb.motion_type {
                                MotionType::Dynamic => "Dynamic",
                                MotionType::Static => "Static",
                                MotionType::Keyframed => "Keyframed",
                                MotionType::CharacterKinematic => "CharKinematic",
                            };
                            *motion_hist.entry(m.to_string()).or_default() += 1;
                            if names.get(cref.index().unwrap()).map(|s| s.as_str()) == Some("bhkBlendCollisionObject") {
                                blend_extract_ok += 1;
                            }
                        }
                        None => {
                            classic_fail += 1;
                            if classic_fail_files.len() < 40 {
                                let coll_i = cref.index().unwrap();
                                let inner = scene
                                    .get(coll_i)
                                    .and_then(|b| b.as_any().downcast_ref::<BhkCollisionObject>())
                                    .and_then(|c| c.body_ref.index())
                                    .map(|bi| names.get(bi).cloned().unwrap_or_default())
                                    .unwrap_or_else(|| "<no body>".into());
                                let shp = scene
                                    .get(coll_i)
                                    .and_then(|b| b.as_any().downcast_ref::<BhkCollisionObject>())
                                    .and_then(|c| c.body_ref.index())
                                    .and_then(|bi| scene.get_as::<BhkRigidBody>(bi))
                                    .and_then(|rb| rb.shape_ref.index())
                                    .map(|si| names.get(si).cloned().unwrap_or_default())
                                    .unwrap_or_else(|| "<no shape>".into());
                                classic_fail_files.push(format!("{name} coll@{coll_i} body={inner} shape={shp}"));
                            }
                        }
                    }
                }
            }
        }
    }
    let _ = list_lost;
    println!("files parsed = {nfiles}, files with collision authoring = {files_with_coll}");
    println!("havok_scale: {havok_scale_hist:?}");
    println!("collision-object kinds at collision_ref: {kind_hist:?}");
    println!("classic extract ok={classic_ok} fail={classic_fail}");
    println!("shape hist: {shape_hist:?}");
    println!("engine motion hist: {motion_hist:?}");
    println!("raw hkMotionType hist: {raw_motion_hist:?}");
    println!("bhk block type hist: {blocktype_hist:#?}");
    println!("convexlist child-count hist: {cvxlist_children:?}");
    println!("convexlist resolvable-children hist: {cvxlist_resolved_children:?}");
    println!("multisphere blocks = {multisphere_count}");
    println!("list total children = {list_total_children}");
    println!("blend collision objects = {blend_objs}, extract ok = {blend_extract_ok}");
    println!("--- sample classic failures ---");
    for f in &classic_fail_files { println!("{f}"); }
}
