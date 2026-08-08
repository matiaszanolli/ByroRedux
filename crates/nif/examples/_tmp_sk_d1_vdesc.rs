//! TEMP scratch: Skyrim SE dimension-1 BSTriShape vertex_desc survey.
//! Validates the sequential packed-vertex parser's implied field offsets
//! against the BSVertexDesc offset nibbles declared on disk.
use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::tri_shape::{BsTriShape, BsTriShapeKind};
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;

const VF_VERTEX: u16 = 0x001;
const VF_UVS: u16 = 0x002;
const VF_UVS_2: u16 = 0x004;
const VF_NORMALS: u16 = 0x008;
const VF_TANGENTS: u16 = 0x010;
const VF_COLORS: u16 = 0x020;
const VF_SKINNED: u16 = 0x040;
const VF_LAND: u16 = 0x080;
const VF_EYE: u16 = 0x100;
const VF_INSTANCE: u16 = 0x200;
const VF_FULL: u16 = 0x400;

fn main() {
    let mut attr_bits = [0usize; 12];
    let mut descs: BTreeMap<u64, usize> = BTreeMap::new();
    let mut kinds: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut shapes = 0usize;
    let mut files = 0usize;

    for path in std::env::args().skip(1) {
        let Ok(arc) = BsaArchive::open(&path) else { eprintln!("open fail {path}"); continue };
        let names: Vec<String> = arc.list_files().into_iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
            .map(|s| s.to_string()).collect();
        eprintln!("{path}: {} nifs", names.len());
        for name in &names {
            let Ok(bytes) = arc.extract(name) else { continue };
            let Ok(scene) = parse_nif(&bytes) else { continue };
            files += 1;
            for block in scene.blocks.iter() {
                let Some(s) = block.as_any().downcast_ref::<BsTriShape>() else { continue };
                shapes += 1;
                let attrs = ((s.vertex_desc >> 44) & 0xFFF) as u16;
                for b in 0..12 { if attrs & (1 << b) != 0 { attr_bits[b] += 1; } }
                *descs.entry(s.vertex_desc).or_default() += 1;
                let k = match &s.kind {
                    BsTriShapeKind::Plain => "Plain",
                    BsTriShapeKind::MeshLOD { .. } => "MeshLOD",
                    BsTriShapeKind::SubIndex(_) => "SubIndex",
                    BsTriShapeKind::Dynamic { .. } => "Dynamic",
                };
                *kinds.entry(k).or_default() += 1;
            }
        }
    }

    eprintln!("files parsed = {files}, BsTriShape blocks = {shapes}");
    let names = ["VERTEX","UVS","UVS_2","NORMALS","TANGENTS","COLORS","SKINNED","LAND","EYE","INSTANCE","FULL_PREC","bit11"];
    for b in 0..12 { println!("attr bit {b:2} {:10} = {}", names[b], attr_bits[b]); }
    println!("--- kinds: {kinds:?}");
    println!("--- distinct vertex_desc = {}", descs.len());

    // Offset-nibble validation for the top descriptors.
    let mut sorted: Vec<_> = descs.iter().collect();
    sorted.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
    println!("desc                 count   size uv1 uv2 nrm tan col skn lnd eye | seq: uv1 nrm tan col skn eye");
    for (desc, count) in sorted.iter().take(25) {
        let d = **desc;
        let attrs = ((d >> 44) & 0xFFF) as u16;
        let nib = |i: u32| ((d >> (i * 4)) & 0xF) as usize;
        // sequential-parse implied byte offsets
        let full = attrs & VF_FULL != 0; // SSE always full anyway
        let mut off = 0usize;
        let (mut suv1, mut snrm, mut stan, mut scol, mut sskn, mut seye) = (99,99,99,99,99,99);
        if attrs & VF_VERTEX != 0 { off += if full {16} else {8}; }
        if attrs & VF_UVS != 0 { suv1 = off; off += 4; }
        if attrs & VF_NORMALS != 0 { snrm = off; off += 4; }
        if attrs & VF_TANGENTS != 0 && attrs & VF_NORMALS != 0 { stan = off; off += 4; }
        if attrs & VF_COLORS != 0 { scol = off; off += 4; }
        if attrs & VF_SKINNED != 0 { sskn = off; off += 12; }
        if attrs & VF_EYE != 0 { seye = off; off += 4; }
        let _ = (VF_UVS_2, VF_LAND, VF_INSTANCE);
        println!(
            "0x{d:016x} {count:7}   {:2}  {:2}  {:2}  {:2}  {:2}  {:2}  {:2}  {:2}  {:2} | {:3} {:3} {:3} {:3} {:3} {:3}  end={} declared_size={}",
            nib(0)*4, nib(2)*4, nib(3)*4, nib(4)*4, nib(5)*4, nib(6)*4, nib(7)*4, nib(8)*4, nib(9)*4,
            suv1, snrm, stan, scol, sskn, seye, off, nib(0)*4
        );
    }
}
