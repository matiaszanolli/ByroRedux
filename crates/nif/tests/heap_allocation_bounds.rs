//! Heap-allocation regression test (#1247 / NIF-D6-INFO-03).
//!
//! All four allocation-hygiene architectural pins (NIF-PERF-01 #832,
//! NIF-PERF-02 #833, NIF-PERF-03 #831, #408 blanket sweep) shipped as
//! code-review-only fixes — every audit had to re-run grep checks to
//! confirm they still held. This test promotes the verification from
//! audit-cadence to CI-cadence by parsing a representative synthetic
//! NIF inside a `dhat::Profiler` scope and asserting upper bounds on
//! both block count and byte total.
//!
//! Gated on the `dhat-heap` cargo feature so default `cargo test`
//! runs without paying the global-allocator override:
//!
//! ```bash
//! cargo test -p byroredux-nif --features dhat-heap --test heap_allocation_bounds
//! ```
//!
//! CI should run this alongside the default test job. Failures here
//! mean a future block-parser change re-introduced an
//! `or_insert(name.to_string())`-class pattern (#832), dropped a
//! `read_pod_vec` for a per-element push loop (#833), discarded an
//! `allocate_vec` binding (#831), or grew a `Vec::with_capacity`
//! call site outside the `allocate_vec` / `read_pod_vec` family
//! (#408 / #1245).
//!
//! Bounds are intentionally loose at first landing — the goal is to
//! catch order-of-magnitude regressions, not micro-shifts. Tighten as
//! follow-up work lands. See [`#1247`].

#![cfg(feature = "dhat-heap")]

use byroredux_nif::parse_nif;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

// `dhat::Profiler` is a PROCESS-GLOBAL singleton — creating a second one
// while the first is live panics ("a profiler is already running").
// cargo runs `#[test]`s in parallel threads, so every profiler-using
// test here must serialize through this lock and let its profiler drop
// before the next acquires it. Declare the guard BEFORE the profiler so
// drop order (reverse) releases the profiler first, then the lock.
// `into_inner` ignores poisoning so one failing test doesn't cascade.
static DHAT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

// ── Synthetic Skyrim SE fixture (single NiNode root) ────────────────
//
// Mirrors `tests/synthetic_fixtures.rs::build_skyrim_se_nif` — kept
// inline here so this test file is fully self-contained (each
// `tests/` file is its own binary, so sharing helpers requires a
// `tests/common/` refactor that's out of scope for the gate landing).

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

/// Build a minimal Skyrim SE NIF — single empty NiNode root.
fn build_skyrim_se_minimal_nif() -> Vec<u8> {
    let mut nif = Vec::new();
    nif.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
    w32(&mut nif, 0x14020007);
    w8(&mut nif, 1);
    w32(&mut nif, 12);
    let num_blocks: u32 = 1;
    w32(&mut nif, num_blocks);
    w32(&mut nif, 100);
    wshort(&mut nif, "ByroRedux Test");
    wshort(&mut nif, "");
    wshort(&mut nif, "");
    w16(&mut nif, 1);
    wsstr(&mut nif, "NiNode");
    for _ in 0..num_blocks {
        w16(&mut nif, 0);
    }
    let block_sizes_offset = nif.len();
    for _ in 0..num_blocks {
        w32(&mut nif, 0);
    }
    w32(&mut nif, 1);
    w32(&mut nif, 10);
    wsstr(&mut nif, "Scene Root");
    w32(&mut nif, 0);
    let block_start = nif.len();
    w32(&mut nif, 0);
    w32(&mut nif, 0);
    w32(&mut nif, 0xFFFFFFFF);
    w32(&mut nif, 0x0E);
    for _ in 0..3 {
        wf32(&mut nif, 0.0);
    }
    wf32(&mut nif, 1.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 1.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 1.0);
    wf32(&mut nif, 1.0);
    w32(&mut nif, 0xFFFFFFFF);
    w32(&mut nif, 0);
    w32(&mut nif, 0);
    let block_size = (nif.len() - block_start) as u32;
    nif[block_sizes_offset..block_sizes_offset + 4].copy_from_slice(&block_size.to_le_bytes());
    nif
}

/// Headline allocation-bound regression: parse a one-NiNode Skyrim SE
/// NIF and assert that the parser stays within a generous heap budget.
///
/// Bounds picked empirically on the first landing — actual measured
/// values were `~80 blocks / ~6 KB`. We pin at **5× actual** (400
/// blocks / 32 KB) so the gate catches order-of-magnitude regressions
/// without false-positive churn on every refactor. Tighten in
/// follow-up work as the corpus and the allocation discipline
/// stabilise (see audit-skill `audit-nif` Dim 6).
///
/// The test asserts on `max_blocks` / `max_bytes` (peak live), NOT
/// `total_blocks` / `total_bytes` (lifetime cumulative) — peak is the
/// metric we care about for "did the parser keep its working set
/// bounded." Lifetime cumulative is sensitive to short-lived
/// allocations (string interning churn, scratch vectors) that don't
/// affect steady-state memory pressure.
#[test]
fn parse_skyrim_se_single_node_stays_within_heap_budget() {
    let nif_bytes = build_skyrim_se_minimal_nif();

    // The profiler is `Drop`-tied — collected stats reflect everything
    // allocated while it's live. Snapshot is taken inside the scope so
    // teardown allocations (the Vec / scene cleanup) don't pollute the
    // measurement.
    let _dhat_guard = DHAT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _profiler = dhat::Profiler::builder().testing().build();
    let scene = parse_nif(&nif_bytes).expect("synthetic Skyrim SE NIF should parse");
    let stats = dhat::HeapStats::get();

    assert_eq!(scene.blocks.len(), 1, "fixture has one NiNode block");

    // 400 blocks ≈ 5× measured baseline on 2026-05-23.
    assert!(
        stats.max_blocks < 400,
        "max_blocks regression: {} >= 400 — a future block parser likely \
         re-introduced an or_insert(name.to_string()) / Vec::with_capacity \
         hot-loop pattern that the #408 / #832 / #833 sweep removed. \
         See #1247 + the audit-nif Dim 6 checklist.",
        stats.max_blocks
    );

    // 32 KB ≈ 5× measured baseline on 2026-05-23.
    assert!(
        stats.max_bytes < 32 * 1024,
        "max_bytes regression: {} >= 32768 — a future block parser likely \
         dropped a read_pod_vec for a per-element push loop, or sized \
         a scratch buffer with the wrong element-count K. See #1247 + \
         the audit-nif Dim 6 checklist.",
        stats.max_bytes
    );
}

// ── #1381 (PERF-D2-NEW-03) — geometry + particle path coverage ──────
//
// The headline gate above only walks a bare NiNode, leaving the
// geometry (BSTriShape) and particle (NiPSysEmitter) block parsers —
// the ones that actually do the bulk per-element allocation the #832 /
// #833 / #408 discipline guards — uncovered. This fixture adds one of
// each so a regression in their allocation hygiene fails at CI cadence.
//
// Both blocks use the minimal (empty-geometry / zeroed-field) wire
// layout the per-block unit tests already pin
// (`tri_shape_skin_vertex_tests::minimal_bs_tri_shape_bytes`,
// `particle::parse_sphere_emitter_consumes_full_block`). A non-zero
// packed-vertex BSTriShape would exercise the vertex-vector path too but
// needs a hand-encoded `BSVertexDesc` + matching packed data — a future
// tightening, tracked alongside #1247.

/// Minimal Skyrim SE BSTriShape body — empty geometry (0 verts / 0 tris).
/// Layout mirrors `minimal_bs_tri_shape_bytes` in the per-block tests:
/// NiObjectNET(12) + flags(4) + transform(52) + collision_ref(4) +
/// center(12) + radius(4) + 3 refs(12) + vertex_desc(8) +
/// num_triangles(2) + num_vertices(2) + data_size(4) +
/// particle_data_size(4) = 120 bytes.
fn bs_tri_shape_block() -> Vec<u8> {
    let mut d = Vec::new();
    // NiObjectNET: name=-1 (no string), extra_data count=0, controller=-1
    w32(&mut d, 0xFFFFFFFF);
    w32(&mut d, 0);
    w32(&mut d, 0xFFFFFFFF);
    // NiAVObject (SSE, no properties): flags + transform + collision_ref
    w32(&mut d, 0); // flags
    for _ in 0..3 {
        wf32(&mut d, 0.0); // translation
    }
    for row in 0..3 {
        for col in 0..3 {
            wf32(&mut d, if row == col { 1.0 } else { 0.0 }); // identity rotation
        }
    }
    wf32(&mut d, 1.0); // scale
    w32(&mut d, 0xFFFFFFFF); // collision_ref
                             // BSTriShape: center(3) + radius + skin/shader/alpha refs + vertex_desc
    for _ in 0..3 {
        wf32(&mut d, 0.0); // bound center
    }
    wf32(&mut d, 0.0); // bound radius
    w32(&mut d, 0xFFFFFFFF); // skin_ref
    w32(&mut d, 0xFFFFFFFF); // shader_property_ref
    w32(&mut d, 0xFFFFFFFF); // alpha_property_ref
    w32(&mut d, 0); // vertex_desc low (no attrs, stride 0)
    w32(&mut d, 0); // vertex_desc high
    w16(&mut d, 0); // num_triangles (SSE bsver<130: u16)
    w16(&mut d, 0); // num_vertices
    w32(&mut d, 0); // data_size — 0 ⇒ no vertex/triangle loops
    w32(&mut d, 0); // particle_data_size (SSE unconditional, #341)
    d
}

/// Minimal v20.2.0.7 NiPSysSphereEmitter body — 77 bytes:
/// modifier base(13) + emitter base 14×f32(56) + volume object ref(4) +
/// sphere radius(4). Mirrors `parse_sphere_emitter_consumes_full_block`.
fn ni_psys_sphere_emitter_block() -> Vec<u8> {
    let mut d = Vec::new();
    // NiPSysModifierBase: name=-1 (string index), order, target_ref=-1, active
    w32(&mut d, 0xFFFFFFFF); // name (none)
    w32(&mut d, 0); // order
    w32(&mut d, 0xFFFFFFFF); // target_ref (-1)
    w8(&mut d, 1); // active
                   // Emitter base — 14 floats (nif.xml order), zeroed.
    for _ in 0..14 {
        wf32(&mut d, 0.0);
    }
    w32(&mut d, 0xFFFFFFFF); // volume emitter object ref (-1)
    wf32(&mut d, 1.5); // sphere radius
    d
}

/// Build a Skyrim SE NIF with three blocks: NiNode root, a BSTriShape
/// child, and an (unparented) NiPSysSphereEmitter. `parse_nif` walks
/// every declared block regardless of scene-graph connectivity, so the
/// emitter is exercised even though no NiParticleSystem owns it.
fn build_skyrim_se_geometry_particle_nif() -> Vec<u8> {
    let mut nif = Vec::new();
    nif.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
    w32(&mut nif, 0x14020007); // version
    w8(&mut nif, 1); // little-endian
    w32(&mut nif, 12); // user_version
    let num_blocks: u32 = 3;
    w32(&mut nif, num_blocks);
    w32(&mut nif, 100); // bsver (Skyrim SE)
    wshort(&mut nif, "ByroRedux Test"); // author
    wshort(&mut nif, ""); // process_script
    wshort(&mut nif, ""); // export_script

    // Block type table: NiNode, BSTriShape, NiPSysSphereEmitter.
    w16(&mut nif, 3);
    wsstr(&mut nif, "NiNode");
    wsstr(&mut nif, "BSTriShape");
    wsstr(&mut nif, "NiPSysSphereEmitter");
    // Block type indices: block i → type i.
    for i in 0..num_blocks {
        w16(&mut nif, i as u16);
    }
    // Block sizes — patched after each block is appended.
    let block_sizes_offset = nif.len();
    for _ in 0..num_blocks {
        w32(&mut nif, 0);
    }
    // String table: one string ("Scene Root", the NiNode name).
    w32(&mut nif, 1);
    w32(&mut nif, 10);
    wsstr(&mut nif, "Scene Root");
    w32(&mut nif, 0); // num_groups

    let patch_size = |nif: &mut Vec<u8>, idx: usize, size: u32| {
        let off = block_sizes_offset + idx * 4;
        nif[off..off + 4].copy_from_slice(&size.to_le_bytes());
    };

    // Block 0 — NiNode root, one child (the BSTriShape, block 1).
    let b0 = nif.len();
    w32(&mut nif, 0); // name = string 0 ("Scene Root")
    w32(&mut nif, 0); // num_extra_data
    w32(&mut nif, 0xFFFFFFFF); // controller_ref
    w32(&mut nif, 0x0E); // flags
    for _ in 0..3 {
        wf32(&mut nif, 0.0); // translation
    }
    for row in 0..3 {
        for col in 0..3 {
            wf32(&mut nif, if row == col { 1.0 } else { 0.0 });
        }
    }
    wf32(&mut nif, 1.0); // scale
    w32(&mut nif, 0xFFFFFFFF); // collision_ref
    w32(&mut nif, 1); // num_children
    w32(&mut nif, 1); // child[0] → block 1 (BSTriShape)
    w32(&mut nif, 0); // num_effects
    let s0 = (nif.len() - b0) as u32;
    patch_size(&mut nif, 0, s0);

    // Block 1 — BSTriShape.
    let b1 = nif.len();
    nif.extend_from_slice(&bs_tri_shape_block());
    let s1 = (nif.len() - b1) as u32;
    patch_size(&mut nif, 1, s1);

    // Block 2 — NiPSysSphereEmitter.
    let b2 = nif.len();
    nif.extend_from_slice(&ni_psys_sphere_emitter_block());
    let s2 = (nif.len() - b2) as u32;
    patch_size(&mut nif, 2, s2);

    nif
}

/// #1381 — geometry + particle allocation-bound gate. Parse a Skyrim SE
/// NIF carrying a BSTriShape and a NiPSysSphereEmitter under dhat and
/// assert the parser stays within a generous heap budget, so a future
/// regression in the geometry / particle block parsers' allocation
/// discipline (the #832 / #833 / #408 family) fails at CI cadence rather
/// than only under audit-cadence grep.
#[test]
fn parse_skyrim_se_geometry_particle_stays_within_heap_budget() {
    let nif_bytes = build_skyrim_se_geometry_particle_nif();

    let _dhat_guard = DHAT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _profiler = dhat::Profiler::builder().testing().build();
    let scene = parse_nif(&nif_bytes).expect("synthetic geometry+particle NIF should parse");
    let stats = dhat::HeapStats::get();

    assert_eq!(
        scene.blocks.len(),
        3,
        "fixture declares NiNode + BSTriShape + NiPSysSphereEmitter"
    );

    // Measured 15 blocks / ~1.5 KB on 2026-06-21 (three minimal blocks).
    // Same loose-headroom philosophy as the headline gate: pin at ~8×
    // measured so the gate catches an order-of-magnitude regression (a
    // per-element push loop or a dropped read_pod_vec re-appearing in the
    // geometry/particle path) without churning on refactors.
    assert!(
        stats.max_blocks < 120,
        "max_blocks regression: {} >= 120 (measured ~15) — a geometry/particle \
         block parser likely re-introduced an or_insert(name.to_string()) / \
         per-element push pattern. See #1381 / #1247 + the audit-nif Dim 6 \
         checklist.",
        stats.max_blocks
    );
    assert!(
        stats.max_bytes < 12 * 1024,
        "max_bytes regression: {} >= 12288 (measured ~1.5 KB) — a geometry/\
         particle block parser likely dropped a read_pod_vec for a \
         per-element push loop. See #1381 / #1247 + the audit-nif Dim 6 \
         checklist.",
        stats.max_bytes
    );
}
