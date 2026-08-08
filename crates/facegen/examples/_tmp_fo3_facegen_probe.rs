// Dimension 6 throwaway: parse a real FO3-extracted .egm/.egt/.tri triple
// (extracted separately from Fallout - Meshes.bsa) and report basic
// structural stats, mirroring parse_real_facegen.rs's FNV assertions.
use byroredux_facegen::{EgmFile, EgtFile, TriHeader};

fn main() {
    let mut args = std::env::args().skip(1);
    let egm_path = args.next().expect("usage: <egm> <egt> <tri>");
    let egt_path = args.next().expect("usage: <egm> <egt> <tri>");
    let tri_path = args.next().expect("usage: <egm> <egt> <tri>");

    let egm_bytes = std::fs::read(&egm_path).expect("read egm");
    println!("egm bytes: {}", egm_bytes.len());
    let egm = EgmFile::parse(&egm_bytes).expect("parse egm");
    println!(
        "egm: num_vertices={} fggs_morphs={} fgga_morphs={}",
        egm.num_vertices,
        egm.fggs_morphs.len(),
        egm.fgga_morphs.len()
    );

    let egt_bytes = std::fs::read(&egt_path).expect("read egt");
    println!("egt bytes: {}", egt_bytes.len());
    let egt = EgtFile::parse(&egt_bytes).expect("parse egt");
    println!(
        "egt: {}x{} fgts_morphs={}",
        egt.width,
        egt.height,
        egt.fgts_morphs.len()
    );

    let tri_bytes = std::fs::read(&tri_path).expect("read tri");
    println!("tri bytes: {}", tri_bytes.len());
    let hdr = TriHeader::parse(&tri_bytes).expect("parse tri header");
    println!(
        "tri header: num_vertices={} num_triangles={}",
        hdr.num_vertices, hdr.num_triangles
    );
}
