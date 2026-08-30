//! THROWAWAY audit probe (audit-nif 2026-08-30, Dim 1).
//! End-of-stream anchor check for the `no_block_sizes` (Oblivion-era) path.
//!
//! Walks every block with `parse_block` (no recovery), then verifies the
//! remaining bytes are EXACTLY the NIF Footer (nif.xml `<struct name="Footer">`
//! since 3.3.0.13: `Num Roots` uint + `Roots` Ref[Num Roots]).
//! Any residue is cumulative parser drift that nothing in the engine detects,
//! because `parse_nif` never reads the footer.
//!
//! usage: _tmp_a0830_d1_footer <archive>

use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_nif::corpus::is_nif_entry;
use byroredux_nif::{blocks::parse_block, header::NifHeader, stream::NifStream};
use std::collections::BTreeMap;
use std::env;

enum Arch {
    Bsa(BsaArchive),
    Ba2(Ba2Archive),
}
impl Arch {
    fn list(&self) -> Vec<String> {
        match self {
            Arch::Bsa(a) => a.list_files().into_iter().map(|s| s.to_string()).collect(),
            Arch::Ba2(a) => a.list_files().into_iter().map(|s| s.to_string()).collect(),
        }
    }
    fn extract(&self, p: &str) -> std::io::Result<Vec<u8>> {
        match self {
            Arch::Bsa(a) => a.extract(p),
            Arch::Ba2(a) => a.extract(p),
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let path = &args[1];
    let arch = if let Ok(a) = BsaArchive::open(path) {
        Arch::Bsa(a)
    } else {
        match Ba2Archive::open(path) {
            Ok(a) => Arch::Ba2(a),
            Err(e) => {
                eprintln!("open {path}: {e}");
                return;
            }
        }
    };
    let files: Vec<String> = arch
        .list()
        .into_iter()
        .filter(|f| is_nif_entry(f))
        .collect();

    let mut ok = 0u64;
    let mut nbs = 0u64;
    let mut hdr_err = 0u64;
    // residue -> count, and the block type that was parsed last
    let mut residue: BTreeMap<i64, u64> = BTreeMap::new();
    let mut bad_last_type: BTreeMap<String, u64> = BTreeMap::new();
    let mut parse_err: BTreeMap<String, u64> = BTreeMap::new();
    let mut examples: BTreeMap<String, String> = BTreeMap::new();

    for f in &files {
        let Ok(bytes) = arch.extract(f) else { continue };
        let Ok((header, off)) = NifHeader::parse(&bytes) else {
            hdr_err += 1;
            continue;
        };
        let has_sizes = !header.block_sizes.is_empty() && header.num_blocks > 0;
        if has_sizes {
            continue;
        }
        nbs += 1;
        let block_bytes = &bytes[off..];
        let mut stream = NifStream::new(block_bytes, &header);
        let mut last_type = String::new();
        let mut failed = false;
        for i in 0..header.num_blocks as usize {
            let Some(tn) = header.block_type_name(i).map(|s| s.to_string()) else {
                failed = true;
                break;
            };
            last_type = tn.clone();
            if parse_block(&tn, &mut stream, None).is_err() {
                *parse_err.entry(tn).or_insert(0) += 1;
                failed = true;
                break;
            }
        }
        if failed {
            continue;
        }
        let pos = stream.position() as usize;
        let remaining = block_bytes.len() as i64 - pos as i64;
        // Footer: u32 num_roots + num_roots * 4
        let expected = if remaining >= 4 {
            let nr = u32::from_le_bytes(block_bytes[pos..pos + 4].try_into().unwrap()) as i64;
            if nr >= 0 && nr < 4096 {
                4 + nr * 4
            } else {
                -1
            }
        } else {
            -1
        };
        let r = remaining - expected;
        if expected >= 0 && r == 0 {
            ok += 1;
        } else {
            *residue
                .entry(if expected < 0 { i64::MIN } else { r })
                .or_insert(0) += 1;
            *bad_last_type.entry(last_type.clone()).or_insert(0) += 1;
            examples.entry(last_type).or_insert_with(|| f.clone());
        }
    }
    println!(
        "#FOOTER\t{path}\tnifs={}\tno_block_sizes={nbs}\tfooter_exact={ok}\theader_err={hdr_err}",
        files.len()
    );
    for (r, c) in &residue {
        println!("RESIDUE\t{r}\t{c}");
    }
    for (t, c) in &bad_last_type {
        println!(
            "LASTTYPE\t{t}\t{c}\t{}",
            examples.get(t).cloned().unwrap_or_default()
        );
    }
    for (t, c) in &parse_err {
        println!("PARSEERR\t{t}\t{c}");
    }
}
