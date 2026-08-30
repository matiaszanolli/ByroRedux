//! TEMP scratch (audit 2026-08-30): FO3 .kf corpus — parse rate, clip yield,
//! B-spline reachability, empty-clip rate.
use byroredux_bsa::BsaArchive;
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;

fn main() {
    let root = "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data";
    let archives = [
        "Fallout - Meshes.bsa",
        "Anchorage - Main.bsa",
        "BrokenSteel - Main.bsa",
        "PointLookout - Main.bsa",
        "ThePitt - Main.bsa",
        "Zeta - Main.bsa",
    ];
    let mut total = 0usize;
    let mut parsed = 0usize;
    let mut failed: Vec<String> = Vec::new();
    let mut clips = 0usize;
    let mut zero_clip: Vec<String> = Vec::new();
    let mut zero_channel_clips = 0usize;
    let mut bspline_files = 0usize;
    let mut bspline_blocks = 0usize;
    let mut blk: BTreeMap<String, usize> = BTreeMap::new();

    for arc_name in archives {
        let Ok(arc) = BsaArchive::open(&format!("{root}/{arc_name}")) else {
            continue;
        };
        for name in arc
            .list_files()
            .into_iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".kf"))
        {
            total += 1;
            let Ok(bytes) = arc.extract(name) else {
                failed.push(format!("{name} (extract)"));
                continue;
            };
            let scene = match parse_nif(&bytes) {
                Ok(s) => s,
                Err(e) => {
                    if failed.len() < 10 {
                        failed.push(format!("{name} ({e})"));
                    }
                    continue;
                }
            };
            parsed += 1;
            let mut fb = 0usize;
            for b in scene.blocks.iter() {
                let t = b.block_type_name();
                *blk.entry(t.to_string()).or_default() += 1;
                if t.contains("BSpline") {
                    fb += 1;
                    bspline_blocks += 1;
                }
            }
            if fb > 0 {
                bspline_files += 1;
            }
            let cs = byroredux_nif::anim::import_kf(&scene);
            clips += cs.len();
            if cs.is_empty() {
                if zero_clip.len() < 10 {
                    zero_clip.push(name.to_string());
                }
            }
            for c in &cs {
                if c.channels.is_empty() {
                    zero_channel_clips += 1;
                }
            }
        }
        eprintln!("done {arc_name}");
    }
    println!(
        "FO3 .kf entries={total} parsed={parsed} failed={}",
        total - parsed
    );
    for f in &failed {
        println!("   FAIL {f}");
    }
    println!(
        "clips imported = {clips}; zero-clip files = {}",
        zero_clip.len()
    );
    for z in &zero_clip {
        println!("   ZERO {z}");
    }
    println!("clips with zero channels = {zero_channel_clips}");
    println!("B-spline blocks = {bspline_blocks} in {bspline_files} .kf files");
    let mut v: Vec<_> = blk.into_iter().collect();
    v.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    println!("top .kf block types: {:?}", &v[..v.len().min(20)]);
}
