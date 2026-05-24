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
