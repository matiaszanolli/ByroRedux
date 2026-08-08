//! TEMP scratch: FO4 audit dimension-9 probe.
//! Traces import_nif_scene on representative FO4 content: settlement
//! workshop item, creature (deathclaw/super mutant), power-armor frame,
//! modular weapon. Reports mesh count, material_path, skinned vs rigid,
//! BSConnectPoint presence.
use byroredux_bsa::Ba2Archive;
use byroredux_core::string::StringPool;
use byroredux_nif::import::import_nif_scene;
use byroredux_nif::parse_nif;

fn trace_one(arc: &Ba2Archive, path: &str, pool: &mut StringPool) {
    let Ok(bytes) = arc.extract(path) else {
        println!("  [MISS] {path}");
        return;
    };
    let scene = match parse_nif(&bytes) {
        Ok(s) => s,
        Err(e) => {
            println!("  [PARSE FAIL] {path}: {e:?}");
            return;
        }
    };
    let imported = import_nif_scene(&scene, pool);

    println!("=== {path}");
    println!(
        "  bsver=0x{:08x} truncated={} recovered_blocks={}",
        scene.bsver, scene.truncated, scene.recovered_blocks
    );
    println!("  meshes={}", imported.meshes.len());
    for (i, m) in imported.meshes.iter().enumerate() {
        let mp = m
            .material
            .material_path
            .and_then(|s| pool.resolve(s))
            .map(|s| s.to_string())
            .unwrap_or_else(|| "<null>".to_string());
        println!(
            "    [{i}] name={:?} verts={} tris={} skinned={} material_path={mp}",
            m.name,
            m.positions.len(),
            m.indices.len() / 3,
            m.skin.is_some(),
        );
    }
    println!(
        "  attach_points(exposed)={:?}",
        imported
            .attach_points
            .as_ref()
            .map(|v| v.iter().map(|p| p.name.clone()).collect::<Vec<_>>())
    );
    println!(
        "  child_attach_connections={:?}",
        imported
            .child_attach_connections
            .as_ref()
            .map(|c| (&c.point_names, c.skinned))
    );
    println!("  bsx_flags={:?}", imported.bsx_flags);
    println!("  ragdoll={}", imported.ragdoll.is_some());
    println!("  furniture_markers={}", imported.furniture_markers.len());
    println!();
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: _tmp_d9_fo4_trace <ba2-path> [ba2-path2 ...] -- (paths hardcoded below)");
    }

    // Candidate representative paths — probed via substring search below
    // since exact vanilla paths vary by archive layout.
    let categories: &[(&str, &[&str])] = &[
        (
            "settlement/workshop",
            &["workshop\\", "settlement\\", "settleobjects"],
        ),
        ("creature/deathclaw", &["deathclaw"]),
        ("creature/supermutant", &["supermutant"]),
        ("power armor frame", &["powerarmor\\frame", "armor\\powerarmor"]),
        ("modular weapon", &["weapons\\10mmpistol", "weapons\\1911"]),
    ];

    for ba2_path in &args {
        let Ok(arc) = Ba2Archive::open(ba2_path) else {
            eprintln!("open fail {ba2_path}");
            continue;
        };
        let all: Vec<String> = arc
            .list_files()
            .into_iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
            .map(|s| s.to_string())
            .collect();
        println!("### {ba2_path}: {} nifs total", all.len());
        let mut pool = StringPool::new();

        for (label, needles) in categories {
            let matches: Vec<&String> = all
                .iter()
                .filter(|n| {
                    let lower = n.to_ascii_lowercase();
                    needles.iter().any(|needle| lower.contains(needle))
                })
                .take(3)
                .collect();
            println!("--- category: {label} ({} matches shown, capped 3)", matches.len());
            for m in matches {
                trace_one(&arc, m, &mut pool);
            }
        }
    }
}
