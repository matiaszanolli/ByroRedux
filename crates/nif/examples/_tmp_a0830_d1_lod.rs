//! THROWAWAY audit probe — list the Oblivion NIFs whose block walk leaves
//! residue past the Footer, with per-file geometry stats.
use byroredux_bsa::BsaArchive;
use byroredux_nif::corpus::is_nif_entry;
use byroredux_nif::{blocks::parse_block, header::NifHeader, stream::NifStream};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let a = BsaArchive::open(&args[1]).unwrap();
    let files: Vec<String> = a
        .list_files()
        .into_iter()
        .map(|s| s.to_string())
        .filter(|f| is_nif_entry(f))
        .collect();
    for f in &files {
        let Ok(bytes) = a.extract(f) else { continue };
        let Ok((header, off)) = NifHeader::parse(&bytes) else {
            continue;
        };
        if !header.block_sizes.is_empty() {
            continue;
        }
        let bb = &bytes[off..];
        let mut s = NifStream::new(bb, &header);
        let mut ok = true;
        let mut sizes = vec![];
        for i in 0..header.num_blocks as usize {
            let Some(tn) = header.block_type_name(i).map(|x| x.to_string()) else {
                ok = false;
                break;
            };
            let p0 = s.position();
            if parse_block(&tn, &mut s, None).is_err() {
                ok = false;
                break;
            }
            sizes.push((tn, s.position() - p0));
        }
        if !ok {
            continue;
        }
        let pos = s.position() as usize;
        let rem = bb.len() as i64 - pos as i64;
        if rem < 4 {
            continue;
        }
        let nr = u32::from_le_bytes(bb[pos..pos + 4].try_into().unwrap()) as i64;
        let exp = if nr >= 0 && nr < 4096 { 4 + nr * 4 } else { -1 };
        if exp >= 0 && rem - exp == 0 {
            continue;
        }
        println!(
            "FILE\t{f}\tv={}\tbsver={}\tnum_blocks={}\tresidue={}\tnum_roots_raw={nr}",
            header.version,
            header.user_version_2,
            header.num_blocks,
            rem - exp
        );
        for (t, c) in &sizes {
            println!("  BLK\t{t}\t{c}");
        }
    }
}
