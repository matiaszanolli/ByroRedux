//! Heap-allocation regression test — geometry + particle parse paths
//! (#1381 / PERF-D2-NEW-03).
//!
//! Sibling of `heap_allocation_bounds.rs`, which bounds only the
//! single-NiNode node-base parser. PERF-D2-NEW-03 / #1381 explicitly
//! flags the geometry and particle parse paths as uncovered by the
//! alloc gate: a regression that re-introduced a per-vertex `push` loop
//! in `parse_geometry_data_base` or a per-particle allocation in
//! `parse_particles_data` would slip past the node-only bound. This
//! file adds the two allocation-heavy data parsers plus an emitter
//! modifier so those paths get a CI bound.
//!
//! It lives in its own `tests/` file (= its own test binary / process)
//! rather than as a second test in `heap_allocation_bounds.rs` because
//! `dhat`'s profiler is a process singleton — two `dhat::Profiler`
//! scopes in one binary collide under the default parallel test runner
//! ("creating a profiler while a profiler is already running").
//!
//! Gated on the `dhat-heap` cargo feature so default `cargo test` runs
//! without the global-allocator override:
//!
//! ```bash
//! cargo test -p byroredux-nif --features dhat-heap --test heap_allocation_bounds_geometry
//! ```
//!
//! Runs in CI as part of the `nif-heap-allocation-bounds` job (`ci.yml`),
//! alongside `heap_allocation_bounds`. See #1763 — pre-fix neither file
//! ran under CI's default `cargo test --workspace` job at all.

#![cfg(feature = "dhat-heap")]

use byroredux_nif::parse_nif;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

// Byte-writer helpers — duplicated from the sibling file because each
// `tests/` binary is independent (a `tests/common/` refactor to share
// them is out of scope for this gate landing).
fn w8(buf: &mut Vec<u8>, v: u8) {
    buf.push(v);
}
fn w16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_le_bytes());
}
fn w32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}
fn wf32(buf: &mut Vec<u8>, v: f32) {
    buf.extend_from_slice(&v.to_le_bytes());
}
fn wsstr(buf: &mut Vec<u8>, s: &str) {
    w32(buf, s.len() as u32);
    buf.extend_from_slice(s.as_bytes());
}
fn wshort(buf: &mut Vec<u8>, s: &str) {
    let len = s.len() + 1;
    w8(buf, len as u8);
    buf.extend_from_slice(s.as_bytes());
    w8(buf, 0);
}

/// Build an FNV-era (20.2.0.7, user_version 11, bsver 34) NIF whose
/// block array is parsed linearly by `parse_nif` — the blocks need not
/// be wired into the NiNode reference graph for their parsers to run.
/// Blocks: NiNode root, NiTriShapeData (3 verts / 1 tri — the geometry
/// vec-allocation path), NiPSysData (the particle-data parser), and
/// NiPSysSphereEmitter (the emitter-modifier path). Byte layouts mirror
/// the per-block unit-test fixtures in `blocks/particle.rs` and
/// `blocks/tri_shape/ni_tri_shape.rs` exactly.
fn build_fnv_geometry_particle_nif() -> Vec<u8> {
    let mut nif = Vec::new();
    nif.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
    w32(&mut nif, 0x14020007); // version 20.2.0.7
    w8(&mut nif, 1); // little-endian
    w32(&mut nif, 11); // user_version (FNV)
    let num_blocks: u32 = 4;
    w32(&mut nif, num_blocks);
    w32(&mut nif, 34); // bsver = 34 (FNV)
    wshort(&mut nif, "ByroRedux Test");
    wshort(&mut nif, ""); // process_script (bsver < 131)
    wshort(&mut nif, ""); // export_script
                          // bsver 34 < 103 → no max_filepath

    // Block type table.
    w16(&mut nif, 4);
    wsstr(&mut nif, "NiNode");
    wsstr(&mut nif, "NiTriShapeData");
    wsstr(&mut nif, "NiPSysData");
    wsstr(&mut nif, "NiPSysSphereEmitter");
    // Block type indices: block i → type i.
    for t in 0..num_blocks as u16 {
        w16(&mut nif, t);
    }
    // Block sizes — one u32 per block, patched after each block is built.
    let block_sizes_offset = nif.len();
    for _ in 0..num_blocks {
        w32(&mut nif, 0);
    }
    // String table: one entry ("Root") for the NiNode name.
    w32(&mut nif, 1);
    w32(&mut nif, 4);
    wsstr(&mut nif, "Root");
    // Groups.
    w32(&mut nif, 0);

    // Patch block `idx`'s size slot with the bytes consumed since `start`.
    let patch = |nif: &mut Vec<u8>, idx: usize, start: usize| {
        let size = (nif.len() - start) as u32;
        let off = block_sizes_offset + idx * 4;
        nif[off..off + 4].copy_from_slice(&size.to_le_bytes());
    };

    // ── Block 0: NiNode root (bsver 34 carries the properties list) ──
    let b0 = nif.len();
    w32(&mut nif, 0); // name = string index 0 ("Root")
    w32(&mut nif, 0); // num_extra_data
    w32(&mut nif, 0xFFFFFFFF); // controller_ref
    w32(&mut nif, 0x0E); // flags (bsver > 26)
    for _ in 0..3 {
        wf32(&mut nif, 0.0); // translation
    }
    for v in [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
        wf32(&mut nif, v); // rotation (identity)
    }
    wf32(&mut nif, 1.0); // scale
    w32(&mut nif, 0); // num_properties (bsver <= 34)
    w32(&mut nif, 0xFFFFFFFF); // collision_ref
    w32(&mut nif, 0); // num_children
    w32(&mut nif, 0); // num_effects (bsver < 130)
    patch(&mut nif, 0, b0);

    // ── Block 1: NiTriShapeData — 3 vertices, 1 triangle ──
    // NiGeometryData base (mirrors parse_geometry_data_base).
    let b1 = nif.len();
    w32(&mut nif, 0); // group_id (since 10.1.0.114)
    w16(&mut nif, 3); // num_vertices
    w8(&mut nif, 0); // keep_flags (since 10.1.0.0)
    w8(&mut nif, 0); // compress_flags
    w8(&mut nif, 1); // has_vertices
    for _ in 0..3 {
        wf32(&mut nif, 0.0); // vertex.x
        wf32(&mut nif, 0.0); // vertex.y
        wf32(&mut nif, 0.0); // vertex.z
    }
    w16(&mut nif, 0); // data_flags (since 10.0.1.0); bsver 34 → no material_crc
    w8(&mut nif, 0); // has_normals
    for _ in 0..3 {
        wf32(&mut nif, 0.0); // bounding-sphere center
    }
    wf32(&mut nif, 0.0); // bounding-sphere radius
    w8(&mut nif, 0); // has_vertex_colors
                     // num_uv_sets = data_flags & 1 = 0 → has_uv false (no UV arrays)
    w16(&mut nif, 0); // consistency_flags (since 10.0.1.0)
    w32(&mut nif, 0xFFFFFFFF); // additional_data_ref (since 20.0.0.4)
                               // NiTriShapeData-specific: triangles.
    w16(&mut nif, 1); // num_triangles
    w32(&mut nif, 3); // num_triangle_points (num_triangles * 3)
    w8(&mut nif, 1); // has_triangles (since 10.0.1.3)
    w16(&mut nif, 0); // triangle vertex 0
    w16(&mut nif, 1); // triangle vertex 1
    w16(&mut nif, 2); // triangle vertex 2
    w16(&mut nif, 0); // num_match_groups
    patch(&mut nif, 1, b1);

    // ── Block 2: NiPSysData (BS202 psys path — per-particle arrays absent) ──
    let b2 = nif.len();
    w32(&mut nif, 0); // group_id
    w16(&mut nif, 0); // num_vertices (BS Max Vertices)
    w8(&mut nif, 0); // keep_flags
    w8(&mut nif, 0); // compress_flags
    w8(&mut nif, 0); // has_vertices
    w16(&mut nif, 0); // data_flags; bsver 34 → no material_crc
    w8(&mut nif, 0); // has_normals
    for _ in 0..3 {
        wf32(&mut nif, 0.0); // bounding-sphere center
    }
    wf32(&mut nif, 0.0); // bounding-sphere radius
    w8(&mut nif, 0); // has_vertex_colors
    w16(&mut nif, 0); // consistency_flags
    w32(&mut nif, 0xFFFFFFFF); // additional_data_ref
                               // NiPSysData post-base (is_bs_202 = true: only bool headers).
    w8(&mut nif, 0); // has_radii (since 10.1.0.0)
    w16(&mut nif, 0); // num_active_particles
    w8(&mut nif, 0); // has_sizes
    w8(&mut nif, 0); // has_rotations (since 10.0.1.0)
    w8(&mut nif, 0); // has_rotation_angles (since 20.0.0.4)
    w8(&mut nif, 0); // has_rotation_axes
    w8(&mut nif, 0); // has_texture_indices (BS202)
    w8(&mut nif, 0); // num_subtexture_offsets (u8 for bsver <= 34)
                     // bsver 34 not > 34 → no aspect-ratio block
    w8(&mut nif, 0); // has_rotation_speeds (since 20.0.0.2)
                     // is_bs_202 → num_added / added_particles_base absent
    patch(&mut nif, 2, b2);

    // ── Block 3: NiPSysSphereEmitter (modifier base + emitter base) ──
    let b3 = nif.len();
    w32(&mut nif, 0xFFFFFFFF); // name index (-1 → None)
    w32(&mut nif, 0); // order
    w32(&mut nif, 0xFFFFFFFF); // target_ref
    w8(&mut nif, 1); // active
                     // Emitter base: 14 f32 (radius_variation present since 10.4.0.1).
    for _ in 0..14 {
        wf32(&mut nif, 0.0);
    }
    w32(&mut nif, 0xFFFFFFFF); // emitter_object_ref (volume emitter base)
    wf32(&mut nif, 0.0); // radius (sphere-specific)
    patch(&mut nif, 3, b3);

    nif
}

/// Allocation-bound regression over the geometry + particle parse paths
/// (#1381 / PERF-D2-NEW-03). Bounds are pinned at **~5× the measured
/// baseline** so the gate catches order-of-magnitude regressions (a
/// re-introduced per-vertex / per-particle allocation loop) without
/// false-positive churn on benign refactors. As with the sibling test,
/// the assertion is on `max_blocks` / `max_bytes` (peak live), not the
/// lifetime-cumulative totals.
#[test]
fn parse_geometry_particle_stays_within_heap_budget() {
    let nif_bytes = build_fnv_geometry_particle_nif();

    let _profiler = dhat::Profiler::builder().testing().build();
    let scene = parse_nif(&nif_bytes).expect("synthetic geometry+particle NIF should parse");
    let stats = dhat::HeapStats::get();

    assert_eq!(
        scene.blocks.len(),
        4,
        "fixture has NiNode + NiTriShapeData + NiPSysData + NiPSysSphereEmitter — \
         a count below 4 means a block parser under-read and the block_size \
         recovery demoted it to NiUnknown (still 4 blocks) or truncation \
         dropped it"
    );

    // 100 blocks ≈ 5× the measured baseline (18 blocks) on 2026-06-13.
    assert!(
        stats.max_blocks < 100,
        "max_blocks regression: {} >= 100 — a future geometry/particle \
         parser change likely re-introduced a per-vertex or per-particle \
         allocation loop the read_pod_vec / allocate_vec family removed. \
         See #1381 + #1247 + the audit-nif Dim 6 checklist.",
        stats.max_blocks
    );

    // 6 KB ≈ 5× the measured baseline (~815 B) on 2026-06-13.
    assert!(
        stats.max_bytes < 6 * 1024,
        "max_bytes regression: {} >= 6144 — a future geometry/particle \
         parser change likely dropped a bulk read_pod_vec for a push loop \
         or sized a scratch buffer with the wrong element count. \
         See #1381 + #1247 + the audit-nif Dim 6 checklist.",
        stats.max_bytes
    );
}
