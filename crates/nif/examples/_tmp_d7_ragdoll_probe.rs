//! TEMP scratch: D7 PHYSAL FNV ragdoll reference-slice probe.
use byroredux_bsa::BsaArchive;
use byroredux_core::string::StringPool;
use byroredux_nif::import::{import_nif_scene, ImportedJointKind};
use byroredux_nif::parse_nif;
use std::collections::{HashMap, HashSet, VecDeque};

fn main() {
    let bsa = std::env::args()
        .nth(1)
        .expect("usage: <bsa> <skeleton path>");
    let path = std::env::args()
        .nth(2)
        .expect("usage: <bsa> <skeleton path>");
    let arc = BsaArchive::open(&bsa).expect("open bsa");
    let bytes = arc.extract(&path).expect("extract skeleton");
    let scene = parse_nif(&bytes).expect("parse");
    println!("havok_scale {}", scene.havok_scale);

    // Count raw constraint blocks by type name.
    let mut ctypes: HashMap<String, usize> = HashMap::new();
    for b in scene.blocks.iter() {
        let n = b.block_type_name();
        if n.starts_with("bhk") && n.contains("Constraint") {
            *ctypes.entry(n.to_string()).or_default() += 1;
        }
    }
    println!("raw constraint blocks: {ctypes:?}");

    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);
    let rag = imported.ragdoll.expect("ragdoll must thread");
    println!(
        "bodies={} constraints={}",
        rag.bodies.len(),
        rag.constraints.len()
    );

    // Node name map, mirroring nif_loader.rs (first spawn wins).
    let mut node_by_name: HashMap<&str, usize> = HashMap::new();
    let mut dup = 0usize;
    for (i, n) in imported.nodes.iter().enumerate() {
        if let Some(name) = n.name.as_ref() {
            if node_by_name.insert(name.as_ref(), i).is_some() {
                dup += 1;
                println!("  DUPLICATE node name: {name}");
            }
        }
    }
    println!("named nodes: {} (dupes {dup})", node_by_name.len());

    let mut unresolved = 0;
    for b in &rag.bodies {
        if !node_by_name.contains_key(b.bone_name.as_ref()) {
            println!("  UNRESOLVED bone: {}", b.bone_name);
            unresolved += 1;
        }
    }
    println!("unresolved ragdoll bones: {unresolved}");

    // Connected components of the constraint graph (mirrors orient_tree).
    let n = rag.bodies.len();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for c in &rag.constraints {
        adj[c.body_a].push(c.body_b);
        adj[c.body_b].push(c.body_a);
    }
    let mut seen = vec![false; n];
    let mut comps = 0;
    for s in 0..n {
        if seen[s] {
            continue;
        }
        comps += 1;
        seen[s] = true;
        let mut q = VecDeque::from([s]);
        while let Some(p) = q.pop_front() {
            for &c in &adj[p] {
                if !seen[c] {
                    seen[c] = true;
                    q.push_back(c);
                }
            }
        }
    }
    println!("constraint graph components: {comps} (1 == single tree)");

    // Shapes + mass + damping census.
    let mut shape_hist: HashMap<&'static str, usize> = HashMap::new();
    let mut zero_mass = 0;
    for b in &rag.bodies {
        let k: &'static str = match &b.shape {
            byroredux_core::ecs::components::CollisionShape::Ball { .. } => "Ball",
            byroredux_core::ecs::components::CollisionShape::Cuboid { .. } => "Cuboid",
            byroredux_core::ecs::components::CollisionShape::Capsule { .. } => "Capsule",
            byroredux_core::ecs::components::CollisionShape::ConvexHull { .. } => "ConvexHull",
            byroredux_core::ecs::components::CollisionShape::TriMesh { .. } => "TriMesh",
            byroredux_core::ecs::components::CollisionShape::Compound { .. } => "Compound",
            _ => "Other",
        };
        *shape_hist.entry(k).or_default() += 1;
        if b.mass == 0.0 {
            zero_mass += 1;
        }
    }
    println!("body shapes: {shape_hist:?} zero_mass={zero_mass}");

    // Joint limit census — how many joints have degenerate/zero limits.
    let mut degenerate = 0;
    let mut plane_asym = 0;
    for c in &rag.constraints {
        match &c.kind {
            ImportedJointKind::Ragdoll {
                cone_max,
                twist_min,
                twist_max,
                plane_min,
                plane_max,
                ..
            } => {
                if *cone_max == 0.0 && *twist_min == 0.0 && *twist_max == 0.0 {
                    degenerate += 1;
                }
                if (plane_min.abs() - plane_max.abs()).abs() > 1e-3 {
                    plane_asym += 1;
                }
                println!("  Ragdoll cone={cone_max:.3} twist=[{twist_min:.3},{twist_max:.3}] plane=[{plane_min:.3},{plane_max:.3}]  {} <-> {}", rag.bodies[c.body_a].bone_name, rag.bodies[c.body_b].bone_name);
            }
            ImportedJointKind::LimitedHinge {
                min_angle,
                max_angle,
                ..
            } => {
                if *min_angle == 0.0 && *max_angle == 0.0 {
                    degenerate += 1;
                }
                println!(
                    "  Hinge   [{min_angle:.3},{max_angle:.3}]  {} <-> {}",
                    rag.bodies[c.body_a].bone_name, rag.bodies[c.body_b].bone_name
                );
            }
        }
    }
    println!(
        "degenerate-limit joints: {degenerate}; asymmetric-plane ragdoll joints: {plane_asym}"
    );

    // Bone-name set of ragdoll bodies vs skeleton subtree children that are not bodies.
    let bodyset: HashSet<&str> = rag.bodies.iter().map(|b| b.bone_name.as_ref()).collect();
    println!("ragdoll bone names: {:?}", bodyset);
}
