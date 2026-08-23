//! Accounting rules for the EX-08 ownership soak gate (#2374).
//!
//! Everything here is pure arithmetic over sampled counts, so it runs in
//! `cargo test` with no Vulkan device and no game data. The live soak supplies
//! numbers; these tests own the judgement applied to them.

use super::*;

/// Build a snapshot whose every field is distinct, so any ordering mismatch
/// between `classes()` and `write_values()` shows up as a value swap rather
/// than a coincidence.
fn distinct() -> OwnershipSnapshot {
    let mut s = OwnershipSnapshot::default();
    let values: Vec<u64> = (1..=s.classes().len() as u64).map(|v| v * 10).collect();
    s.write_values(&values);
    s
}

#[test]
fn classes_cover_every_field() {
    // Pins the class count. Adding a field to `OwnershipSnapshot` without
    // adding it to `classes()` leaves it out of the gate entirely — the leak
    // it represents would then be invisible, which is the exact failure mode
    // EX-08 exists to prevent. Bump this deliberately, alongside `classes()`
    // *and* `write_values()`.
    assert_eq!(OwnershipSnapshot::default().classes().len(), 23);
}

#[test]
fn precombine_mesh_rows_above_baseline_is_a_leak() {
    // EX-15 / #2369 — a precombine-owned entity that outlives its cell's
    // unload is exactly the "double geometry that never goes away" failure
    // mode the class exists to catch, distinct from a generic
    // `cell_root_rows` surplus that could be any owner type.
    let mut base = OwnershipSnapshot::default();
    base.precombine_mesh_rows = 12;
    let mut leaked = base;
    leaked.precombine_mesh_rows = 18;

    let mut t = OwnershipTracker::new();
    t.set_baseline(base);
    t.record_cycle(leaked);

    let findings = t.evaluate();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].class, "precombine_mesh_rows");
    assert_eq!(findings[0].kind, FindingKind::NotReclaimed);
}

#[test]
fn class_names_are_unique() {
    let classes = OwnershipSnapshot::default().classes();
    let mut names: Vec<&str> = classes.iter().map(|c| c.name).collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(before, names.len(), "duplicate class name in classes()");
}

#[test]
fn classes_and_write_values_agree_on_order() {
    // Round-trip: read every value out positionally, write it back, and expect
    // the identical struct. A field written into the wrong slot in
    // `write_values` — the copy-paste hazard in a 20-line assignment run —
    // fails here.
    let original = distinct();
    let values: Vec<u64> = original.classes().iter().map(|c| c.value).collect();
    let mut rebuilt = OwnershipSnapshot::default();
    rebuilt.write_values(&values);
    assert_eq!(original, rebuilt);
}

#[test]
fn max_with_is_elementwise() {
    let mut a = OwnershipSnapshot::default();
    a.transform_rows = 10;
    a.physics_bodies = 3;
    let mut b = OwnershipSnapshot::default();
    b.transform_rows = 4;
    b.physics_bodies = 9;

    let m = a.max_with(&b);
    assert_eq!(m.transform_rows, 10);
    assert_eq!(m.physics_bodies, 9);
    // Untouched fields stay at the shared default rather than picking up
    // whatever happened to sit next to them.
    assert_eq!(m.blas_entries, 0);
}

#[test]
fn no_baseline_means_no_verdict() {
    let mut t = OwnershipTracker::new();
    t.record_cycle(OwnershipSnapshot::default());
    // A soak that never established a baseline cannot claim anything about
    // reclamation; silence is correct, and the report says so explicitly.
    assert!(t.evaluate().is_empty());
    assert!(t.report().iter().any(|l| l.contains("no baseline")));
}

#[test]
fn clean_return_to_baseline_passes() {
    let mut base = OwnershipSnapshot::default();
    base.transform_rows = 100;
    base.physics_bodies = 5;
    base.cell_root_index_entries = 1;

    let mut t = OwnershipTracker::new();
    t.set_baseline(base);
    for _ in 0..6 {
        t.record_cycle(base);
    }
    assert!(t.evaluate().is_empty());
    assert!(t.report().iter().any(|l| l.contains("ownership: PASS")));
}

#[test]
fn exact_class_above_baseline_is_a_leak() {
    let mut base = OwnershipSnapshot::default();
    base.physics_bodies = 5;
    let mut leaked = base;
    leaked.physics_bodies = 9;

    let mut t = OwnershipTracker::new();
    t.set_baseline(base);
    t.record_cycle(leaked);

    let findings = t.evaluate();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].class, "physics_bodies");
    assert_eq!(findings[0].kind, FindingKind::NotReclaimed);
    assert!(findings[0].detail.contains("+4"), "{}", findings[0].detail);
}

#[test]
fn exact_class_below_baseline_is_not_a_finding() {
    // Ending *under* baseline is not a leak. It happens legitimately when the
    // baseline is taken with a cell resident and the soak ends fully unloaded.
    let mut base = OwnershipSnapshot::default();
    base.transform_rows = 100;
    let mut fewer = base;
    fewer.transform_rows = 40;

    let mut t = OwnershipTracker::new();
    t.set_baseline(base);
    t.record_cycle(fewer);
    assert!(t.evaluate().is_empty());
}

#[test]
fn bounded_class_above_baseline_is_not_a_leak() {
    // The mesh/texture registries never reuse handles (#372) and the sound
    // cache retains across cells by design. Holding them to an exact return
    // would fail every single run, so the gate must not.
    let mut base = OwnershipSnapshot::default();
    base.meshes_registry = 100;
    base.textures_registry = 50;
    base.sound_cache_entries = 4;
    base.entities_spawned = 1_000;

    let mut settled = base;
    settled.meshes_registry = 180;
    settled.textures_registry = 90;
    settled.sound_cache_entries = 11;
    settled.entities_spawned = 9_000;
    // `meshes_registry` / `textures_registry` / `entities_spawned` are
    // Monotonic; `sound_cache_entries` is Bounded. None may raise
    // NotReclaimed.

    let mut t = OwnershipTracker::new();
    t.set_baseline(base);
    t.record_cycle(settled);
    assert!(
        t.evaluate().is_empty(),
        "bounded classes must not raise NotReclaimed: {:?}",
        t.evaluate()
    );
}

#[test]
fn only_the_final_cycle_decides_reclamation() {
    // A mid-run sample can sit high because unload hysteresis has not yet
    // evicted the trailing cells. Only the settled end state is evidence.
    let mut base = OwnershipSnapshot::default();
    base.transform_rows = 100;
    let mut spike = base;
    spike.transform_rows = 900;

    let mut t = OwnershipTracker::new();
    t.set_baseline(base);
    t.record_cycle(spike);
    t.record_cycle(base);
    assert!(t.evaluate().is_empty());
    // …but the spike is still visible in the high-water mark.
    assert_eq!(t.high_water().transform_rows, 900);
}

#[test]
fn monotonic_growth_needs_enough_cycles() {
    let base = OwnershipSnapshot::default();
    let mut t = OwnershipTracker::new();
    t.set_baseline(base);

    // Three rising cycles is below the threshold — a load ramp, not a trend.
    // Uses a Bounded class: `meshes_registry` is Monotonic and exempt.
    for n in 1..MIN_CYCLES_FOR_GROWTH as u64 {
        let mut s = base;
        s.sound_cache_entries = n * 10;
        t.record_cycle(s);
    }
    assert!(
        !t.evaluate()
            .iter()
            .any(|f| f.kind == FindingKind::MonotonicGrowth),
        "growth claimed before {} cycles",
        MIN_CYCLES_FOR_GROWTH
    );

    // One more rise crosses it.
    let mut s = base;
    s.sound_cache_entries = MIN_CYCLES_FOR_GROWTH as u64 * 10;
    t.record_cycle(s);
    assert!(
        t.evaluate()
            .iter()
            .any(|f| f.kind == FindingKind::MonotonicGrowth),
        "growth not detected at {} cycles",
        MIN_CYCLES_FOR_GROWTH
    );
}

#[test]
fn bounded_classes_are_still_subject_to_growth() {
    // "Bounded" exempts a class from the exact-return check, not from the
    // unbounded-growth check. A cache that never stops growing is not bounded,
    // and that is precisely the shape EX-08 asks the soak to fail on.
    let base = OwnershipSnapshot::default();
    let mut t = OwnershipTracker::new();
    t.set_baseline(base);
    for n in 1..=MIN_CYCLES_FOR_GROWTH as u64 {
        let mut s = base;
        s.sound_cache_entries = n;
        t.record_cycle(s);
    }
    let findings = t.evaluate();
    let growth: Vec<_> = findings
        .iter()
        .filter(|f| f.kind == FindingKind::MonotonicGrowth)
        .collect();
    assert_eq!(growth.len(), 1);
    assert_eq!(growth[0].class, "sound_cache_entries");
}

#[test]
fn bouncing_values_are_not_growth() {
    // Steady-state churn — a cache filling and evicting — must not read as a
    // leak, or the gate gets silenced within a week of landing.
    let base = OwnershipSnapshot::default();
    let mut t = OwnershipTracker::new();
    t.set_baseline(base);
    for v in [10_u64, 14, 11, 15, 12, 13] {
        let mut s = base;
        s.sound_cache_entries = v;
        t.record_cycle(s);
    }
    assert!(
        !t.evaluate()
            .iter()
            .any(|f| f.kind == FindingKind::MonotonicGrowth),
        "non-monotonic series flagged as growth"
    );
}

#[test]
fn flat_series_is_not_growth() {
    // Strictly-increasing is the test, so a plateau must not trip it.
    let base = OwnershipSnapshot::default();
    let mut t = OwnershipTracker::new();
    t.set_baseline(base);
    for _ in 0..MIN_CYCLES_FOR_GROWTH + 2 {
        let mut s = base;
        s.sound_cache_entries = 42;
        t.record_cycle(s);
    }
    assert!(t.evaluate().is_empty());
}

#[test]
fn high_water_folds_baseline_and_every_cycle() {
    let mut base = OwnershipSnapshot::default();
    base.blas_entries = 7;
    let mut t = OwnershipTracker::new();
    t.set_baseline(base);

    let mut mid = OwnershipSnapshot::default();
    mid.blas_entries = 3;
    mid.terrain_tiles = 25;
    t.record_cycle(mid);

    // Baseline contributes its own peak even though no cycle matched it.
    assert_eq!(t.high_water().blas_entries, 7);
    assert_eq!(t.high_water().terrain_tiles, 25);
}

#[test]
fn report_names_every_class_and_flags_the_leak() {
    let mut base = OwnershipSnapshot::default();
    base.terrain_tiles = 2;
    let mut leaked = base;
    leaked.terrain_tiles = 20;

    let mut t = OwnershipTracker::new();
    t.set_baseline(base);
    t.record_cycle(leaked);
    let report = t.report();
    let joined = report.join("\n");

    // Every class appears, so an operator reading the artifact sees the full
    // picture rather than only the failures.
    for class in OwnershipSnapshot::default().classes() {
        assert!(
            joined.contains(class.name),
            "report omits class {}",
            class.name
        );
    }
    assert!(joined.contains("LEAK"));
    assert!(joined.contains("ownership: FAIL NOT-RECLAIMED terrain_tiles"));
}

#[test]
fn multiple_leaks_are_all_reported() {
    // The gate must not stop at the first finding — a soak run is expensive
    // and should surface every leaked owner in one pass.
    let mut base = OwnershipSnapshot::default();
    base.physics_bodies = 1;
    base.script_timer_rows = 1;
    base.particle_emitters = 1;

    let mut leaked = base;
    leaked.physics_bodies = 2;
    leaked.script_timer_rows = 5;
    leaked.particle_emitters = 9;

    let mut t = OwnershipTracker::new();
    t.set_baseline(base);
    t.record_cycle(leaked);

    let names: Vec<&str> = t.evaluate().iter().map(|f| f.class).collect();
    assert_eq!(
        names,
        vec!["physics_bodies", "script_timer_rows", "particle_emitters"]
    );
}

#[test]
fn monotonic_classes_are_exempt_from_both_checks() {
    // The three watermark classes rise on every cycle of every correct run:
    // entity ids never repeat, and dropped mesh/texture slots stay as
    // placeholders so re-entering a cell always issues fresh handles (#372).
    // Failing on that would make the gate fire on a perfectly clean engine,
    // which is how gates get switched off.
    let base = OwnershipSnapshot::default();
    let mut t = OwnershipTracker::new();
    t.set_baseline(base);
    for n in 1..=MIN_CYCLES_FOR_GROWTH as u64 + 2 {
        let mut s = base;
        s.entities_spawned = n * 1_000;
        s.meshes_registry = n * 200;
        s.textures_registry = n * 50;
        t.record_cycle(s);
    }
    assert!(
        t.evaluate().is_empty(),
        "monotonic watermarks must not produce findings: {:?}",
        t.evaluate()
    );
}

#[test]
fn live_slot_counts_are_not_exempt() {
    // The residency counterparts of the monotonic registries must still be
    // held to an exact return — they are the classes that actually answer
    // "did the GPU resources come back?".
    let mut base = OwnershipSnapshot::default();
    base.meshes_live_slots = 100;
    base.texture_live_slots = 40;
    let mut leaked = base;
    leaked.meshes_live_slots = 130;
    leaked.texture_live_slots = 55;

    let mut t = OwnershipTracker::new();
    t.set_baseline(base);
    t.record_cycle(leaked);

    let names: Vec<&str> = t.evaluate().iter().map(|f| f.class).collect();
    assert_eq!(names, vec!["meshes_live_slots", "texture_live_slots"]);
}

#[test]
fn report_includes_the_per_cycle_series() {
    // A surplus that saturates reads very differently from one that keeps
    // climbing, and baseline/final/high-water alone cannot distinguish them.
    let base = OwnershipSnapshot::default();
    let mut t = OwnershipTracker::new();
    t.set_baseline(base);
    for v in [5_u64, 9, 9, 9] {
        let mut s = base;
        s.transform_rows = v;
        t.record_cycle(s);
    }
    let joined = t.report().join("\n");
    assert!(joined.contains("per-cycle series"), "{joined}");
    assert!(joined.contains("5 9 9 9"), "{joined}");
}

#[test]
fn reachability_counts_keep_the_growth_check() {
    // Reclassifying `meshes_in_use` / `textures_in_use` to Bounded removes the
    // exact-return check because the handle *set* legitimately changes between
    // visits (#372). It must NOT remove the growth check — a real leak climbs
    // every cycle, and that is still a failure.
    let base = OwnershipSnapshot::default();
    let mut t = OwnershipTracker::new();
    t.set_baseline(base);
    for n in 1..=MIN_CYCLES_FOR_GROWTH as u64 {
        let mut s = base;
        s.meshes_in_use = 600 + n * 20;
        t.record_cycle(s);
    }
    let growth: Vec<&str> = t
        .evaluate()
        .iter()
        .filter(|f| f.kind == FindingKind::MonotonicGrowth)
        .map(|f| f.class)
        .collect();
    assert_eq!(growth, vec!["meshes_in_use"]);
}

#[test]
fn oscillating_reachability_is_accepted() {
    // The literal FNV WastelandNV series that drove the reclassification.
    // Non-monotonic movement inside a fixed band, with residency flat, is the
    // engine behaving correctly — the gate must stay silent on it.
    let mut base = OwnershipSnapshot::default();
    base.meshes_in_use = 620;
    base.meshes_live_slots = 718;
    let mut t = OwnershipTracker::new();
    t.set_baseline(base);
    for v in [620_u64, 715, 591, 620, 715] {
        let mut s = base;
        s.meshes_in_use = v;
        t.record_cycle(s);
    }
    assert!(
        t.evaluate().is_empty(),
        "clean oscillation flagged: {:?}",
        t.evaluate()
    );
}

// ── ImageHealth (EX-05 / #2736) ─────────────────────────────────

#[test]
fn image_health_is_clean_only_when_both_totals_are_zero() {
    use crate::ecs::resources::ImageHealth;
    assert!(ImageHealth::default().is_clean());
    assert!(!ImageHealth {
        total_non_finite_rgb: 1,
        ..Default::default()
    }
    .is_clean());
    // Alpha alone must also fail: a NaN alpha propagates through blending and
    // is just as capable of poisoning the frame as a NaN colour.
    assert!(!ImageHealth {
        total_non_finite_alpha: 1,
        ..Default::default()
    }
    .is_clean());
    // A clean current frame does not clear a historical detection.
    assert!(!ImageHealth {
        last_non_finite_rgb: 0,
        total_non_finite_rgb: 42,
        ..Default::default()
    }
    .is_clean());
}
