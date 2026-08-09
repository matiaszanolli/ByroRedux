//! TEMP scratch: FO4 audit dimension-4 probe.
//! BSVER distribution, BSTriShape vertex-desc / precision census,
//! BSSubIndexTriShape segmentation stats, collision shape-type census.
use byroredux_bsa::Ba2Archive;
use byroredux_nif::blocks::tri_shape::{BsTriShape, BsTriShapeKind};
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;

const VF_VERTEX: u16 = 0x001;
const VF_TANGENTS: u16 = 0x010;
const VF_SKINNED: u16 = 0x040;
const VF_FULL: u16 = 0x400;

fn main() {
    let mut bsvers: BTreeMap<u32, usize> = BTreeMap::new();
    let mut kinds: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut blocktypes: BTreeMap<String, usize> = BTreeMap::new();
    let mut shapes = 0usize;
    let mut full_prec = 0usize;
    let mut half_prec = 0usize;
    let mut has_vertex = 0usize;
    let mut skinned = 0usize;
    let mut tangents_flag = 0usize;
    // sub-index stats
    let mut si_total = 0usize;
    let mut si_empty = 0usize;
    let mut si_with_segments = 0usize;
    let mut si_with_shared = 0usize;
    let mut si_cutoff_over8 = 0usize;
    let mut cutoff_hist: BTreeMap<usize, usize> = BTreeMap::new();
    let mut recovered = 0usize;
    let mut truncated = 0usize;
    let mut si_segments_total = 0usize;
    let mut si_subsegments_total = 0usize;
    let mut si_ssf_named = 0usize;
    // precision-vs-bsver cross tab
    let mut prec_by_bsver: BTreeMap<(u32, bool), usize> = BTreeMap::new();
    let mut files = 0usize;
    let mut parse_fail = 0usize;
    let limit: usize = std::env::var("LIMIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);

    for path in std::env::args().skip(1) {
        let Ok(arc) = Ba2Archive::open(&path) else {
            eprintln!("open fail {path}");
            continue;
        };
        let names: Vec<String> = arc
            .list_files()
            .into_iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
            .map(|s| s.to_string())
            .collect();
        eprintln!("{path}: {} nifs", names.len());
        for name in names.iter().take(limit) {
            let Ok(bytes) = arc.extract(name) else {
                continue;
            };
            let scene = match parse_nif(&bytes) {
                Ok(s) => s,
                Err(_) => {
                    parse_fail += 1;
                    continue;
                }
            };
            files += 1;
            recovered += scene.recovered_blocks;
            if scene.truncated {
                truncated += 1;
            }
            *bsvers.entry(scene.bsver).or_default() += 1;
            for block in scene.blocks.iter() {
                let tn = block.block_type_name();
                if tn.starts_with("bhk") || tn.starts_with("Bhk") || tn.starts_with("hk") {
                    *blocktypes.entry(tn.to_string()).or_default() += 1;
                }
                let Some(s) = block.as_any().downcast_ref::<BsTriShape>() else {
                    continue;
                };
                shapes += 1;
                let attrs = ((s.vertex_desc >> 44) & 0xFFF) as u16;
                if attrs & VF_VERTEX != 0 {
                    has_vertex += 1;
                }
                if attrs & VF_SKINNED != 0 {
                    skinned += 1;
                }
                if attrs & VF_TANGENTS != 0 {
                    tangents_flag += 1;
                }
                let fp = attrs & VF_FULL != 0;
                if fp {
                    full_prec += 1
                } else {
                    half_prec += 1
                }
                *prec_by_bsver.entry((scene.bsver, fp)).or_default() += 1;
                let k = match &s.kind {
                    BsTriShapeKind::Plain => "Plain",
                    BsTriShapeKind::MeshLOD { .. } => "MeshLOD",
                    BsTriShapeKind::SubIndex(_) => "SubIndex",
                    BsTriShapeKind::Dynamic { .. } => "Dynamic",
                };
                *kinds.entry(k).or_default() += 1;
                if let BsTriShapeKind::SubIndex(d) = &s.kind {
                    si_total += 1;
                    if d.num_segments == 0 && d.segments.is_empty() && d.shared.is_none() {
                        si_empty += 1;
                    }
                    if !d.segments.is_empty() {
                        si_with_segments += 1;
                    }
                    si_segments_total += d.segments.len();
                    si_subsegments_total += d
                        .segments
                        .iter()
                        .map(|x| x.sub_segments.len())
                        .sum::<usize>();
                    if let Some(sh) = &d.shared {
                        si_with_shared += 1;
                        if !sh.ssf_filename.is_empty() {
                            si_ssf_named += 1;
                        }
                        for p in &sh.per_segment_data {
                            *cutoff_hist.entry(p.cut_offsets.len()).or_default() += 1;
                        }
                        if sh.per_segment_data.iter().any(|p| p.cut_offsets.len() > 8) {
                            si_cutoff_over8 += 1;
                        }
                    }
                }
            }
        }
    }

    println!("files parsed = {files}, parse_fail = {parse_fail}");
    println!("--- bsver dist: {bsvers:?}");
    println!("--- BsTriShape blocks = {shapes}");
    println!("    VF_VERTEX={has_vertex} VF_SKINNED={skinned} VF_TANGENTS={tangents_flag}");
    println!("    full_precision={full_prec} half_precision={half_prec}");
    println!("--- kinds: {kinds:?}");
    println!("--- prec_by_bsver (bsver, full_prec) -> count: {prec_by_bsver:?}");
    println!("--- SubIndex: total={si_total} empty={si_empty} with_segments={si_with_segments} with_shared={si_with_shared}");
    println!("    segments_total={si_segments_total} subsegments_total={si_subsegments_total} ssf_named={si_ssf_named} cutoffs>8={si_cutoff_over8}");
    println!("--- cut_offsets len histogram: {cutoff_hist:?}");
    println!("--- recovered_blocks total = {recovered}, truncated files = {truncated}");
    println!("--- bhk block types:");
    for (k, v) in &blocktypes {
        println!("    {k:45} {v}");
    }
}
