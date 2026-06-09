//! Real-data CSG reader regression (M49 — FO4 Shared precombine geometry).
//!
//! Validates the `Fallout4 - Geometry.csg` container parse + PSG
//! addressing against ground truth taken from the
//! `BSPackedCombinedSharedGeomDataExtra` block of
//! `meshes\precombined\0000e2db_02be5e11_oc.nif` (object 0): PSG
//! `data_offset = 152449338`, `num_verts = 770`, on-disk vertex stride
//! 20, `tri_total = 1010`. The triangle block sits at `data_offset +
//! num_verts*20`; its u16 indices are 0-based and dense
//! (`max == num_verts - 1`). A wrong header parse, chunk-size model, or
//! boundary stitch would corrupt those indices. Full spec:
//! `docs/engine/fo4-csg-format.md`.
//!
//! Gated `#[ignore]` on `BYROREDUX_FO4_DATA`; opt-in via:
//! ```sh
//! cargo test -p byroredux-bsa --test csg_real -- --ignored
//! ```

use byroredux_bsa::{CsgArchive, CSG_CHUNK_SIZE};
use std::path::PathBuf;

fn fo4_data_dir() -> Option<PathBuf> {
    if let Ok(v) = std::env::var("BYROREDUX_FO4_DATA") {
        let p = PathBuf::from(&v);
        if p.is_dir() {
            return Some(p);
        }
        eprintln!("BYROREDUX_FO4_DATA points to {v:?} which is not a directory; using default");
    }
    let p = PathBuf::from("/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data");
    p.is_dir().then_some(p)
}

#[test]
#[ignore]
fn fallout4_geometry_csg_header_and_object_decode() {
    let Some(data) = fo4_data_dir() else {
        eprintln!("Skipping: BYROREDUX_FO4_DATA not set and default path missing");
        return;
    };
    let path = data.join("Fallout4 - Geometry.csg");
    if !path.is_file() {
        eprintln!("Skipping: {path:?} not found");
        return;
    }

    let csg = CsgArchive::open(&path).expect("open Fallout4 - Geometry.csg");

    // Header constants from the Steam build (240 043 177 bytes).
    assert_eq!(csg.num_objects(), 32_370, "object-table count");
    assert_eq!(csg.num_chunks(), 6_841, "chunk-table count");

    // All chunks but the last inflate to 64 KiB; the last is partial.
    let psg_len = csg.psg_len().expect("psg_len");
    let expected_full = (csg.num_chunks() as u64 - 1) * CSG_CHUNK_SIZE as u64;
    assert!(
        psg_len > expected_full && psg_len < expected_full + CSG_CHUNK_SIZE as u64,
        "psg_len {psg_len} should be (n-1)*64KiB + partial-last"
    );

    // Ground-truth object 0 of 0000e2db_02be5e11_oc.nif.
    const DATA_OFFSET: u64 = 152_449_338;
    const NUM_VERTS: usize = 770;
    const PSG_STRIDE: usize = 20; // runtime 28 with FULLPREC → half pos = 20 on disk
    const TRI_TOTAL: usize = 1010;

    let vert_bytes = NUM_VERTS * PSG_STRIDE;
    let buf = csg
        .read_psg(DATA_OFFSET, vert_bytes + TRI_TOTAL * 6)
        .expect("read_psg object 0");
    assert_eq!(buf.len(), vert_bytes + TRI_TOTAL * 6);

    // Triangle block: u16×3 indices into [0, NUM_VERTS), dense to nv-1.
    let mut max_idx = 0u16;
    for t in 0..TRI_TOTAL {
        let base = vert_bytes + t * 6;
        for k in 0..3 {
            let idx = u16::from_le_bytes(buf[base + k * 2..base + k * 2 + 2].try_into().unwrap());
            assert!(
                (idx as usize) < NUM_VERTS,
                "triangle {t} index {idx} >= num_verts {NUM_VERTS} — wrong stride/offset/chunk model"
            );
            max_idx = max_idx.max(idx);
        }
    }
    assert_eq!(
        max_idx as usize,
        NUM_VERTS - 1,
        "index buffer should reference every vertex up to nv-1"
    );

    // First triangle of LOD0 is (0,1,2) for this object.
    let t0: Vec<u16> = (0..3)
        .map(|k| {
            u16::from_le_bytes(
                buf[vert_bytes + k * 2..vert_bytes + k * 2 + 2]
                    .try_into()
                    .unwrap(),
            )
        })
        .collect();
    assert_eq!(t0, vec![0, 1, 2], "first triangle");
}
