//! Integration tests exercising the FaceGen parsers against real
//! vanilla FNV / FO3 content. Gated `#[ignore]` because the
//! fixtures live in proprietary BSAs that can't ship in the repo.
//!
//! Opt in:
//!
//! ```bash
//! cargo test -p byroredux-facegen --test parse_real_facegen -- --ignored
//! ```
//!
//! `BYROREDUX_FNV_DATA` may override the default Steam install path;
//! the test self-skips when the data dir isn't present so a CI
//! environment without the game library doesn't fail the run.

use byroredux_bsa::BsaArchive;
use byroredux_facegen::{EgmFile, EgtFile, TriHeader};

const FNV_DEFAULT_DATA: &str = "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data";
const FNV_MESH_BSA: &str = "Fallout - Meshes.bsa";

fn data_dir() -> Option<std::path::PathBuf> {
    if let Ok(val) = std::env::var("BYROREDUX_FNV_DATA") {
        let p = std::path::PathBuf::from(val);
        if p.is_dir() {
            return Some(p);
        }
    }
    let p = std::path::PathBuf::from(FNV_DEFAULT_DATA);
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

fn extract(inner: &str) -> Option<Vec<u8>> {
    let dir = data_dir()?;
    let bsa_path = dir.join(FNV_MESH_BSA);
    let archive = BsaArchive::open(&bsa_path).ok()?;
    archive.extract(inner).ok()
}

#[test]
#[ignore]
fn parse_vanilla_headhuman_egm() {
    let Some(bytes) = extract(r"meshes\characters\head\headhuman.egm") else {
        eprintln!("FNV data not available; skipping");
        return;
    };
    // Vanilla baseline (verified on 2026-04-29 against unmodded FNV
    // GOTY): 695 904 bytes, 1449 vertices, 50 sym + 30 asym morphs.
    assert_eq!(
        bytes.len(),
        695_904,
        "vanilla headhuman.egm baseline byte count drifted",
    );
    let egm = EgmFile::parse(&bytes).expect("parse");
    assert_eq!(egm.num_vertices, 1449);
    assert_eq!(egm.fggs_morphs.len(), 50);
    assert_eq!(egm.fgga_morphs.len(), 30);
    for morph in egm.fggs_morphs.iter().chain(egm.fgga_morphs.iter()) {
        assert_eq!(morph.deltas.len(), 1449);
    }
    // NaN sentinel deltas DO appear in vanilla `headhuman.egm` —
    // FaceGen's authoring pipeline stores "no displacement" as
    // a half-float NaN bit-pattern on some entries (verified by
    // dumping non-finite indices on 2026-04-29). The Phase 3b
    // morph evaluator must guard against NaN propagation when
    // it sums `weight * delta` per vertex; the parser layer
    // preserves the on-disk bytes verbatim.
    let nan_count: usize = egm
        .fggs_morphs
        .iter()
        .chain(egm.fgga_morphs.iter())
        .map(|m| {
            m.deltas
                .iter()
                .flatten()
                .filter(|c| !c.is_finite())
                .count()
        })
        .sum();
    eprintln!("vanilla headhuman.egm: {} non-finite delta components", nan_count);
}

#[test]
#[ignore]
fn parse_vanilla_headhuman_egt() {
    let Some(bytes) = extract(r"meshes\characters\head\headhuman.egt") else {
        eprintln!("FNV data not available; skipping");
        return;
    };
    assert_eq!(
        bytes.len(),
        9_830_664,
        "vanilla headhuman.egt baseline byte count drifted",
    );
    let egt = EgtFile::parse(&bytes).expect("parse");
    assert_eq!(egt.width, 256);
    assert_eq!(egt.height, 256);
    assert_eq!(egt.fgts_morphs.len(), 50);
    for morph in &egt.fgts_morphs {
        assert_eq!(morph.pixels.len(), 65_536);
    }
}

#[test]
#[ignore]
fn parse_vanilla_headhuman_tri_header() {
    let Some(bytes) = extract(r"meshes\characters\head\headhuman.tri") else {
        eprintln!("FNV data not available; skipping");
        return;
    };
    let hdr = TriHeader::parse(&bytes).expect("parse");
    // Vanilla headhuman.nif has 1211 verts / 2294 tris — the .tri
    // header must agree.
    assert_eq!(hdr.num_vertices, 1211);
    assert_eq!(hdr.num_triangles, 2294);
}
