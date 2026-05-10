//! `spt_walk` — debug example that runs `parse_spt` on a single
//! file and prints the last few parsed entries plus the bail offset.
//! Used during Phase 1.3 dictionary refinement to pinpoint
//! misclassifications.

use byroredux_bsa::BsaArchive;
use byroredux_spt::parse_spt;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bytes = match args.len() {
        1 => std::fs::read(&args[0]).expect("read loose .spt"),
        2 => {
            let archive = BsaArchive::open(&args[0]).expect("open BSA");
            archive.extract(&args[1]).expect("extract")
        }
        _ => {
            eprintln!("usage: spt_walk <bsa> <path-in-bsa>  |  <loose-path>");
            std::process::exit(2);
        }
    };
    let scene = parse_spt(&bytes).expect("parse");
    println!("entries     = {}", scene.entries.len());
    println!("tail_offset = {}", scene.tail_offset);
    println!("reached_eof = {}", scene.reached_eof);
    println!("unknown_tags = {:?}", scene.unknown_tags);
    println!();
    let n = scene.entries.len();
    let start = n.saturating_sub(20);
    println!("Last {} entries:", n - start);
    for entry in &scene.entries[start..] {
        let v = match &entry.value {
            byroredux_spt::SptValue::Bare => "Bare".to_string(),
            byroredux_spt::SptValue::U8(b) => format!("U8({})", b),
            byroredux_spt::SptValue::U32(raw) => {
                format!("U32({}=0x{:08x}, f32={})", raw, raw, f32::from_bits(*raw))
            }
            byroredux_spt::SptValue::Vec3(v) => format!("Vec3({:?})", v),
            byroredux_spt::SptValue::Fixed(b) => format!("Fixed({} bytes)", b.len()),
            byroredux_spt::SptValue::String(s) => format!("String(len={}, {:?})", s.len(), s),
            byroredux_spt::SptValue::ArrayBytes { stride, count, bytes } => {
                format!("ArrayBytes(stride={}, count={}, total={} bytes)", stride, count, bytes.len())
            }
        };
        println!("  off {:6}  tag {:5}  {}", entry.offset, entry.tag, v);
    }
}
