//! TEMP scratch (audit 2026-08-16): does the per-node `is_editor_marker`
//! NAME filter drop real Oblivion geometry? Counts geometry blocks whose
//! own name matches the filter.
use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::tri_shape::NiTriShape;
use byroredux_nif::parse_nif;
use std::collections::HashMap;

fn is_editor_marker(name: &str) -> bool {
    let sw = |p: &str| {
        name.len() >= p.len() && name.as_bytes()[..p.len()].eq_ignore_ascii_case(p.as_bytes())
    };
    sw("editormarker")
        || sw("marker_")
        || name.eq_ignore_ascii_case("markerx")
        || sw("marker:")
        || sw("mapmarker")
}

fn main() {
    let mut hits: HashMap<String, usize> = HashMap::new();
    let mut hit_files: HashMap<String, String> = HashMap::new();
    let mut files = 0usize;
    for path in std::env::args().skip(1) {
        let Ok(arc) = BsaArchive::open(&path) else {
            continue;
        };
        let names: Vec<String> = arc.list_files().iter().map(|s| s.to_string()).collect();
        for name in &names {
            if !name.to_ascii_lowercase().ends_with(".nif") {
                continue;
            }
            let Ok(bytes) = arc.extract(name) else {
                continue;
            };
            let Ok(scene) = parse_nif(&bytes) else {
                continue;
            };
            files += 1;
            for i in 0..scene.len() {
                let Some(s) = scene.get_as::<NiTriShape>(i) else {
                    continue;
                };
                let Some(nm) = s.av.net.name.as_deref() else {
                    continue;
                };
                if is_editor_marker(nm) {
                    *hits.entry(nm.to_string()).or_default() += 1;
                    hit_files.entry(nm.to_string()).or_insert(name.clone());
                }
            }
        }
    }
    println!("files scanned: {files}");
    println!(
        "distinct geometry-node names matching is_editor_marker: {}",
        hits.len()
    );
    let mut v: Vec<_> = hits.into_iter().collect();
    v.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    let total: usize = v.iter().map(|(_, c)| *c).sum();
    println!("total geometry blocks dropped by the name filter: {total}");
    for (n, c) in v.iter().take(40) {
        println!("  {c:6}  {n:40}  e.g. {}", hit_files[n]);
    }
}
