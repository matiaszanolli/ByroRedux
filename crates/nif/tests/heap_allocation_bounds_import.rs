//! Heap-allocation regression test for the NIF import tier (#3673).
//!
//! `heap_allocation_bounds.rs` measures `parse_nif` only. That leaves the
//! import walk, which materializes renderer-facing position/color/normal/UV
//! vectors and imported nodes, outside the CI allocation contract. This
//! sibling binary measures parsing and `import_nif_scene` together so a
//! dropped `Vec::with_capacity` or per-vertex growth loop in `import/` fails
//! at CI cadence.
//!
//! This is a separate integration-test binary because `dhat::Profiler` is a
//! process-global singleton. It is intentionally gated behind `dhat-heap`:
//!
//! ```bash
//! cargo test -p byroredux-nif --features dhat-heap --test heap_allocation_bounds_import
//! ```

#![cfg(feature = "dhat-heap")]

use byroredux_core::string::StringPool;
use byroredux_nif::import::import_nif_scene;
use byroredux_nif::parse_nif;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

static DHAT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn w8(buf: &mut Vec<u8>, value: u8) {
    buf.push(value);
}

fn w16(buf: &mut Vec<u8>, value: u16) {
    buf.extend_from_slice(&value.to_le_bytes());
}

fn w32(buf: &mut Vec<u8>, value: u32) {
    buf.extend_from_slice(&value.to_le_bytes());
}

fn wf32(buf: &mut Vec<u8>, value: f32) {
    buf.extend_from_slice(&value.to_le_bytes());
}

fn wsstr(buf: &mut Vec<u8>, value: &str) {
    w32(buf, value.len() as u32);
    buf.extend_from_slice(value.as_bytes());
}

fn wshort(buf: &mut Vec<u8>, value: &str) {
    w8(buf, (value.len() + 1) as u8);
    buf.extend_from_slice(value.as_bytes());
    w8(buf, 0);
}

fn ni_transform_identity(buf: &mut Vec<u8>) {
    for row in 0..3 {
        for col in 0..3 {
            wf32(buf, if row == col { 1.0 } else { 0.0 });
        }
    }
    for _ in 0..3 {
        wf32(buf, 0.0);
    }
    wf32(buf, 1.0);
}

/// FO4 `BSTriShape` with a non-zero packed position stream. The import walk
/// therefore has to materialize its per-vertex output vectors instead of
/// returning an empty mesh.
fn bs_tri_shape_block_with_vertices(num_vertices: u16) -> Vec<u8> {
    const VF_VERTEX: u64 = 0x001;
    let vertex_size_quads: u64 = 2; // 8 bytes: three half positions + padding.
    let vertex_desc = (VF_VERTEX << 44) | vertex_size_quads;

    let mut data = Vec::new();
    // NiObjectNET: no name, no extra data, no controller.
    w32(&mut data, 0xFFFFFFFF);
    w32(&mut data, 0);
    w32(&mut data, 0xFFFFFFFF);
    // NiAVObject: flags, transform, collision ref. FO4 has no properties list.
    w32(&mut data, 0);
    ni_transform_identity(&mut data);
    w32(&mut data, 0xFFFFFFFF);
    // BSTriShape bound and dedicated refs.
    for _ in 0..3 {
        wf32(&mut data, 0.0);
    }
    wf32(&mut data, 0.0);
    w32(&mut data, 0xFFFFFFFF); // skin ref
    w32(&mut data, 0xFFFFFFFF); // shader property ref
    w32(&mut data, 0xFFFFFFFF); // alpha property ref
    data.extend_from_slice(&vertex_desc.to_le_bytes());
    let num_triangles = num_vertices.saturating_sub(2) as u32;
    w32(&mut data, num_triangles); // num triangles (FO4 u32)
    w16(&mut data, num_vertices);
    w32(
        &mut data,
        8 * num_vertices as u32 + 6 * num_triangles,
    ); // packed vertices + u16 triangle indices

    for _ in 0..num_vertices {
        w16(&mut data, 0); // x half
        w16(&mut data, 0); // y half
        w16(&mut data, 0); // z half
        w16(&mut data, 0); // unused W/padding
    }
    for triangle in 0..num_triangles {
        w16(&mut data, triangle as u16);
        w16(&mut data, triangle as u16 + 1);
        w16(&mut data, triangle as u16 + 2);
    }
    data
}

/// Build a valid FO4 scene with one root node and one child mesh. The 256
/// vertex payload gives the import vectors enough slope for a reverted
/// per-vertex `push` allocation pattern to become visible in the peak.
fn build_fo4_import_nif(num_vertices: u16) -> Vec<u8> {
    let mut nif = Vec::new();
    nif.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
    w32(&mut nif, 0x14020007); // version 20.2.0.7
    w8(&mut nif, 1); // little-endian
    w32(&mut nif, 12); // user_version
    w32(&mut nif, 2); // NiNode + BSTriShape
    w32(&mut nif, 130); // FO4 BSVER
    wshort(&mut nif, "ByroRedux Test");
    wshort(&mut nif, ""); // process script
    wshort(&mut nif, ""); // export script
    wshort(&mut nif, ""); // max filepath

    w16(&mut nif, 2);
    wsstr(&mut nif, "NiNode");
    wsstr(&mut nif, "BSTriShape");
    w16(&mut nif, 0); // block 0 -> NiNode
    w16(&mut nif, 1); // block 1 -> BSTriShape

    let block_sizes_offset = nif.len();
    w32(&mut nif, 0);
    w32(&mut nif, 0);
    w32(&mut nif, 1); // string table count
    w32(&mut nif, 10); // max string length
    wsstr(&mut nif, "Scene Root");
    w32(&mut nif, 0); // groups

    // NiNode root: one child, no effects (FO4 removed the effects list).
    let root_start = nif.len();
    w32(&mut nif, 0); // name -> "Scene Root"
    w32(&mut nif, 0); // extra-data count
    w32(&mut nif, 0xFFFFFFFF); // controller
    w32(&mut nif, 0x0E); // flags
    ni_transform_identity(&mut nif);
    w32(&mut nif, 0xFFFFFFFF); // collision ref
    w32(&mut nif, 1); // child count
    w32(&mut nif, 1); // child -> BSTriShape
    let root_size = (nif.len() - root_start) as u32;
    nif[block_sizes_offset..block_sizes_offset + 4].copy_from_slice(&root_size.to_le_bytes());

    let shape_start = nif.len();
    nif.extend_from_slice(&bs_tri_shape_block_with_vertices(num_vertices));
    let shape_size = (nif.len() - shape_start) as u32;
    nif[block_sizes_offset + 4..block_sizes_offset + 8]
        .copy_from_slice(&shape_size.to_le_bytes());
    nif
}

#[test]
fn parse_and_import_stay_within_heap_budget() {
    let nif_bytes = build_fo4_import_nif(256);

    // Keep dhat's process-global profiler serialized within this binary even
    // though this currently has one profiler test; that invariant prevents a
    // future second fixture from introducing a flaky singleton collision.
    let _dhat_guard = DHAT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _profiler = dhat::Profiler::builder().testing().build();
    let scene = parse_nif(&nif_bytes).expect("synthetic FO4 import NIF should parse");
    assert_eq!(scene.blocks.len(), 2, "fixture has root and mesh blocks");
    let imported = import_nif_scene(&scene, &mut StringPool::new());
    let stats = dhat::HeapStats::get();

    assert_eq!(imported.nodes.len(), 1, "fixture has one imported root node");
    assert_eq!(imported.meshes.len(), 1, "fixture has one imported mesh");
    assert_eq!(
        imported.meshes[0].positions.len(),
        256,
        "import must materialize every packed vertex"
    );

    // Initial landing headroom is deliberately broad (~5x the measured
    // parse+import peak on the 256-vertex fixture). This catches a dropped
    // pre-sizing reservation or order-of-magnitude allocation regression
    // without making harmless allocator/refactor shifts CI failures.
    assert!(
        stats.max_blocks < 100,
        "max_blocks regression: {} >= 100 — import_nif_scene likely reverted \
         an output-vector reservation to Vec::new() + per-vertex push growth. \
         See #3673 / #1247.",
        stats.max_blocks
    );
    assert!(
        stats.max_bytes < 160 * 1024,
        "max_bytes regression: {} >= 163840 — import_nif_scene likely regressed \
         its per-vertex output-vector allocation discipline. See #3673 / #1247.",
        stats.max_bytes
    );
}
