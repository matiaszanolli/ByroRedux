//! Throwaway: list vanilla FO3 NIFs whose header BSVER != 34, with the
//! block types they carry — the transitional-export band that several
//! version gates (falloff defaults, refraction, parallax, grow-fade
//! base scale) treat as "unreachable on retail".
use byroredux_bsa::BsaArchive;
use byroredux_nif::header::NifHeader;

fn main() {
    for path in std::env::args().skip(1) {
        let Ok(archive) = BsaArchive::open(&path) else { continue };
        let short = path.rsplit('/').next().unwrap_or(&path).to_string();
        for name in archive.list_files().into_iter().filter(|n| n.to_ascii_lowercase().ends_with(".nif")).map(|s| s.to_string()).collect::<Vec<_>>() {
            let Ok(bytes) = archive.extract(&name) else { continue };
            let Ok((h, _)) = NifHeader::parse(&bytes) else { continue };
            if h.user_version_2 == 34 { continue; }
            let mut types: Vec<String> = h.block_types.iter().map(|s| s.to_string()).collect();
            types.sort();
            types.dedup();
            println!("{short} | {name} | ver={} bsver={} uv={} | {}", h.version, h.user_version_2, h.user_version, types.join(","));
        }
    }
}
