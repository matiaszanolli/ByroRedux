//! Regression tests for #1016 / REN-D2-005: defence-in-depth caps
//! on `pending_vertices` / `pending_indices` growth. The pure-
//! function `check_pool_growth` is exercised here with mock cap
//! values so the test doesn't need to allocate gigabytes.
use super::*;

#[test]
fn growth_below_soft_cap_is_clean() {
    let warned = check_pool_growth(0, 100, 1000, 2000, "vertex").unwrap();
    assert!(!warned, "growth fully under soft cap must not warn");
}

#[test]
fn growth_crossing_soft_cap_signals_warn() {
    // 900 → 1100 crosses soft cap 1000.
    let warned = check_pool_growth(900, 1100, 1000, 2000, "vertex").unwrap();
    assert!(warned, "growth that crosses soft cap must signal warn");
}

#[test]
fn second_growth_beyond_soft_cap_does_not_re_signal() {
    // 1500 → 1600 is fully past soft cap 1000; the warn was already
    // signalled on the crossing growth, this growth should be silent.
    let warned = check_pool_growth(1500, 1600, 1000, 2000, "vertex").unwrap();
    assert!(
        !warned,
        "growth fully above soft cap (already warned) must NOT re-signal"
    );
}

#[test]
fn growth_exceeding_hard_cap_returns_err() {
    // 1500 → 2100 exceeds hard cap 2000.
    let result = check_pool_growth(1500, 2100, 1000, 2000, "vertex");
    assert!(result.is_err(), "growth past hard cap must return Err");
    let err_msg = format!("{:?}", result.unwrap_err());
    assert!(
        err_msg.contains("hard cap"),
        "err message should mention hard cap; got: {err_msg}",
    );
    assert!(
        err_msg.contains("REN-D2-005"),
        "err message should reference the issue id for grep-ability; got: {err_msg}",
    );
}

#[test]
fn growth_landing_exactly_at_hard_cap_is_allowed() {
    // 1500 → 2000 is exactly at hard cap — within bounds.
    let result = check_pool_growth(1500, 2000, 1000, 2000, "vertex");
    assert!(
        result.is_ok(),
        "growth landing exactly at hard cap must be allowed"
    );
}

#[test]
fn shipping_caps_have_sane_relative_sizing() {
    // The hard caps must not be below the warning thresholds. They are
    // intentionally equal today: the former warning-only allowance let
    // per-mesh and BLAS duplicates escalate into a host-memory OOM.
    const {
        assert!(VERTEX_POOL_HARD_CAP >= VERTEX_POOL_SOFT_CAP);
        assert!(INDEX_POOL_HARD_CAP >= INDEX_POOL_SOFT_CAP);
    }
    // At Vertex = 104 B, hard cap 4M = 416 MB. At u32 indices,
    // hard cap 16M = 64 MB. Sanity-check: vertex cap is the bigger
    // memory commitment of the two.
    let vertex_bytes = VERTEX_POOL_HARD_CAP * std::mem::size_of::<Vertex>();
    let index_bytes = INDEX_POOL_HARD_CAP * 4;
    assert!(
        vertex_bytes > index_bytes,
        "vertex cap should be larger memory budget than index cap (got {} vs {} bytes)",
        vertex_bytes,
        index_bytes,
    );
}

#[test]
fn closed_scene_geometry_admission_rejects_further_uploads() {
    let mut registry = MeshRegistry::new();
    registry.scene_geometry_admission_closed = true;
    assert!(!registry.scene_geometry_admission_open());
    assert!(registry
        .accumulate_global_geometry(&[], &[])
        .expect_err("closed admission must reject before accepting geometry")
        .to_string()
        .contains("admission is closed"));
}

/// #3443 — the size gate must be consulted **before** the duplicate
/// allocation, not only after it has already failed. #3298's chunked
/// path allocated a second full-size generation unconditionally whenever
/// an old one existed, which left `geometry_rebuild_needs_idle` reachable
/// only from `rebuild_geometry_ssbo_atomic_fallback` — i.e. only after an
/// `Err`. The hazard the threshold guards is the opposite case: on a
/// constrained device the duplicate allocation *succeeds* (driver
/// residency / system-memory spill) and escalates to an unrecoverable
/// `VK_ERROR_DEVICE_LOST` later, with no `Err` to catch.
///
/// Source-level because the arm needs a live Vulkan device to execute;
/// the predicate's own behaviour is covered by the test below.
#[test]
fn chunked_rebuild_consults_the_idle_threshold_before_duplicating() {
    // #3451 — the chunked rebuild moved to `mesh/geometry_ssbo.rs`.
    // This test stays here with its `check_pool_growth` siblings, but
    // must read the file the code actually lives in; `include_str!` on
    // the old path kept resolving and the `.expect` below would report
    // "must exist" for a function that does.
    let src = include_str!("geometry_ssbo.rs");
    let body = src
        .split_once("fn rebuild_geometry_ssbo_inner(")
        .expect("rebuild_geometry_ssbo_inner must exist")
        .1;
    let body = body
        .split_once("fn rebuild_geometry_ssbo_atomic_fallback(")
        .map_or(body, |(before, _)| before);

    let gate = body
        .find("geometry_rebuild_needs_idle(projected_bytes, has_existing_buffers, live_budget)")
        .expect(
            "the chunked path must consult geometry_rebuild_needs_idle — without it every \
             rebuild duplicates the largest non-texture allocation class, past the 80% \
             live-budget line (or at >= 256 MiB with no budget reading), and routes \
             around #2374",
        );
    let allocate = body
        .find("try_allocate_empty_geometry_buffers(")
        .expect("the chunked path must allocate the replacement generation");
    assert!(
        gate < allocate,
        "the size gate must precede the duplicate allocation: a post-hoc Err check \
         cannot see an allocation that succeeds and dies later (#3443)"
    );
}

/// Without a `VK_EXT_memory_budget` reading the fixed 256 MiB rule holds.
#[test]
fn large_rebuilds_idle_only_when_replacing_existing_buffers() {
    let threshold = GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES;
    assert!(!geometry_rebuild_needs_idle(threshold - 1, true, None));
    assert!(geometry_rebuild_needs_idle(threshold, true, None));
    assert!(geometry_rebuild_needs_idle(threshold + 1, true, None));
    assert!(
        !geometry_rebuild_needs_idle(threshold * 2, false, None),
        "initial build has no old generation to reclaim",
    );
}

/// With a live reading, the gate is the approaching-OOM line (80% of the
/// live budget) after adding the duplicate, not the fixed 256 MiB.
#[test]
fn live_budget_gates_duplication_at_the_approaching_oom_line() {
    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * MIB;
    // Replacing an existing generation, with a live (usage, budget).
    let idles = |projected: u64, usage: u64, budget: u64| {
        geometry_rebuild_needs_idle(projected, true, Some((usage, budget)))
    };
    // 12 GB dev card mid-crossing: a ~430 MiB FO4 rebuild streams.
    assert!(!idles(430 * MIB, 3 * GIB, 11 * GIB));
    // Exactly at the line duplicates; one byte past it idles.
    let budget = 10 * GIB;
    let line = budget / 5 * 4;
    assert!(!idles(GIB, line - GIB, budget));
    assert!(idles(GIB + 1, line - GIB, budget));
    // Nearly full 6 GB card: even a sub-256 MiB rebuild idles now,
    // where the fixed rule would have duplicated it.
    assert!(idles(128 * MIB, 4400 * MIB, 5500 * MIB));
    // Stacked generations count through usage: the same rebuild that
    // fits at low usage is refused once earlier ones are still resident.
    assert!(!idles(900 * MIB, 2 * GIB, 6 * GIB));
    assert!(idles(900 * MIB, 4 * GIB, 6 * GIB));
    // A zero budget can never admit a duplicate.
    assert!(idles(1, 0, 0));
    assert!(
        !geometry_rebuild_needs_idle(GIB, false, Some((0, 0))),
        "initial build has no old generation to reclaim",
    );
}
