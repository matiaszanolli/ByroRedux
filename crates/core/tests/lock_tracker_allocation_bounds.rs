//! #3680 (PERF-D1-2026-08-30-04) — regression pin: with the lock-order
//! detector disabled (the default — `BYRO_LOCK_ORDER_CHECK` unset, the
//! mode `CLAUDE.md`'s Quick Reference documents for `cargo run`/
//! `cargo test`), acquiring a read lock while other locks are already
//! held on the thread must not cost any MORE heap allocation than
//! acquiring one in isolation. Pre-fix, `track_read`/`track_write`
//! unconditionally built a `held_others: Vec<(TypeId, &str)>` before
//! ever checking whether the detector was enabled — `ENABLED`'s own doc
//! comment promised "the per-acquire fast path is one relaxed load",
//! which was not true.
//!
//! Own test binary (mirrors `crates/nif/tests/heap_allocation_bounds.rs`)
//! so the counting `#[global_allocator]` override applies only here, not
//! to the rest of `byroredux-core`'s test suite. Unlike that file this
//! one needs no `dhat` dependency or feature gate — a minimal counting
//! wrapper over `System` is enough to answer "did this block allocate
//! more than that block", which is all a differential comparison needs.
//!
//! Deliberately differential (baseline vs. nested), not an absolute
//! "must allocate exactly zero" assertion: `World::query`'s own
//! `QueryRead` construction may have some constant per-call allocation
//! cost unrelated to lock-order tracking, and that cost should cancel
//! out between the two measurements. What must NOT scale with how many
//! other locks are already held is the `held_others` collection this
//! issue is about.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct CountingAlloc;

static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        // SAFETY: delegates verbatim to `System`, the same allocator
        // this binary would use without the override — only the count
        // observation is added.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: see `alloc` above.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

use byroredux_core::ecs::components::{GlobalTransform, MeshHandle, Transform};
use byroredux_core::ecs::World;

fn allocs_during<R>(f: impl FnOnce() -> R) -> (usize, R) {
    let before = ALLOC_COUNT.load(Ordering::Relaxed);
    let result = f();
    let after = ALLOC_COUNT.load(Ordering::Relaxed);
    (after - before, result)
}

#[test]
fn nested_read_lock_costs_no_more_than_an_isolated_one_when_disabled() {
    assert!(
        std::env::var_os("BYRO_LOCK_ORDER_CHECK").is_none(),
        "this test measures the default-disabled fast path; it is meaningless \
         (and may legitimately differ) with BYRO_LOCK_ORDER_CHECK set"
    );

    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Transform::IDENTITY);
    world.insert(e, GlobalTransform::IDENTITY);
    world.insert(e, MeshHandle(1));

    // Warm-up: the first acquisition of each type touches lazily
    // initialized statics (thread-local HashMap, etc.) that legitimately
    // allocate once. Excluded from both measurements below so they
    // compare the STEADY-STATE per-acquisition cost only.
    {
        let _t = world.query::<Transform>();
        let _g = world.query::<GlobalTransform>();
        let _m = world.query::<MeshHandle>();
    }

    // Baseline: acquire `Transform` alone, nothing else held. Measures the
    // constant per-`QueryRead` construction cost with no other locks to
    // report.
    let (baseline, _) = allocs_during(|| {
        let _t = world.query::<Transform>().unwrap();
    });

    // Nested: the identical `Transform` acquisition, but with two other
    // read locks already held on this thread going in — exactly the
    // "locks already held" scenario #3680 is about
    // (`collect_static_mesh_draws` holds ~24 concurrently). The two
    // setup acquisitions happen OUTSIDE the measured window; only the
    // marginal cost of the `Transform` acquisition itself, now with
    // `held_others` non-empty, is compared against `baseline` above.
    let _g = world.query::<GlobalTransform>().unwrap();
    let _m = world.query::<MeshHandle>().unwrap();
    let (nested, _) = allocs_during(|| {
        let _t = world.query::<Transform>().unwrap();
    });

    assert_eq!(
        nested, baseline,
        "acquiring Transform while 2 other read locks were already held allocated \
         {nested} time(s) vs {baseline} for the identical acquisition in isolation \
         — the held_others Vec is being built even though the lock-order detector \
         is disabled (#3680)",
    );
}
