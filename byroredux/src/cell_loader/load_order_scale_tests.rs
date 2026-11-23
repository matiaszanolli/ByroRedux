//! LMC-PERF-001 / LMC-PERF-002 — the cost of the load-order fold grows linearly with the plugins and records it folds.
//!
//! The fold walks each plugin on the rayon pool, then merges, notes provenance and notes list merges in load order. Each
//! of those is meant to cost per record, not per pair of plugins. This builds profiles of light plugins (the plugin kind
//! a 1,000-plugin profile can hold today: regular slots stop at `0xFB`, LMC-ID-004) and times the whole fold as plugins
//! and records grow. A step that is quadratic in the plugin count shows as time per record rising with it.
//!
//! Two dependency shapes: *wide*, where every plugin masters only the base plugin (how mod lists are mostly built), and
//! *chain*, where every plugin also masters the one before it (the worst case for the transitive master closure).
//! Timing is of wall clock on whatever machine runs it, so the test is opt-in and prints its table.

use super::tests::{build_tes4_with_masters, build_weap, wrap_group};
use super::{parse_record_indexes_in_load_order, transitive_masters};
use std::path::PathBuf;
use std::time::Instant;

/// Records the base plugin holds, which the plugins override.
const BASE_FORMS: u32 = 1_000;
/// A light plugin's TES4 flag.
const ESL: u32 = 0x200;

/// One plugin shape of a profile.
#[derive(Clone, Copy)]
struct Shape {
    /// Records each plugin defines.
    own: u32,
    /// Records of the base plugin each plugin overrides.
    overrides: u32,
    /// Whether each plugin also masters the one before it.
    chain: bool,
}

/// `count` light plugins and their base, written to `dir`; the paths in load order.
fn write_profile(dir: &std::path::Path, count: u32, shape: Shape) -> Vec<String> {
    let mut paths = Vec::with_capacity(count as usize + 1);
    let base: Vec<u8> = (0..BASE_FORMS).flat_map(|i| build_weap(0x0000_0800 + i, b"Base\0")).collect();
    let mut bytes = build_tes4_with_masters(0, &[]);
    bytes.extend_from_slice(&wrap_group(b"WEAP", &base));
    let path: PathBuf = dir.join("base.esm");
    std::fs::write(&path, &bytes).unwrap();
    paths.push(path.to_string_lossy().into_owned());
    for plugin in 0..count {
        let mut masters = vec!["base.esm".to_owned()];
        if shape.chain && plugin > 0 {
            masters.push(format!("p{:04}.esl", plugin - 1));
        }
        let masters_ref: Vec<&str> = masters.iter().map(String::as_str).collect();
        let own_top = masters.len() as u32;
        let mut records = Vec::new();
        for i in 0..shape.own {
            records.extend(build_weap((own_top << 24) | (0x0000_0800 + i), b"Own\0"));
        }
        for i in 0..shape.overrides {
            records.extend(build_weap((plugin.wrapping_mul(shape.overrides) + i) % BASE_FORMS + 0x0000_0800, b"Over\0"));
        }
        let mut bytes = build_tes4_with_masters(ESL, &masters_ref);
        bytes.extend_from_slice(&wrap_group(b"WEAP", &records));
        let path = dir.join(format!("p{plugin:04}.esl"));
        std::fs::write(&path, &bytes).unwrap();
        paths.push(path.to_string_lossy().into_owned());
    }
    paths
}

/// The fold of `count` plugins of `shape`: seconds, and the records it folded.
fn fold_seconds(count: u32, shape: Shape) -> (f64, u32) {
    let dir = tempfile::tempdir().unwrap();
    let paths = write_profile(dir.path(), count, shape);
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let started = Instant::now();
    let (index, order) = parse_record_indexes_in_load_order(&refs).expect("fold");
    let seconds = started.elapsed().as_secs_f64();
    assert_eq!(order.len(), count as usize + 1, "every plugin activates");
    assert_eq!(index.items.len() as u32, BASE_FORMS + count * shape.own, "every form is in the merged index");
    (seconds, BASE_FORMS + count * (shape.own + shape.overrides))
}

/// Time the fold at growing plugin counts and print microseconds per record; the largest must cost per record no more
/// than a few times the smallest, and a shape that costs per pair of plugins would not.
fn scales_with_plugins(shape: Shape, label: &str) {
    let counts = [250u32, 500, 1000, 2000, 4000];
    eprintln!("{label}: {} own + {} overriding records per plugin", shape.own, shape.overrides);
    eprintln!("  plugins     records   seconds   µs/record");
    let mut per_record = Vec::new();
    for count in counts {
        // The best of three: a fold shares the machine with whatever else runs on it.
        let (seconds, records) = (0..3).map(|_| fold_seconds(count, shape)).min_by(|a, b| a.0.total_cmp(&b.0)).unwrap();
        let micros = seconds * 1e6 / f64::from(records);
        eprintln!("  {count:7} {records:11} {seconds:9.3} {micros:11.2}");
        per_record.push(micros);
    }
    let (first, last) = (per_record[0], *per_record.last().unwrap());
    assert!(last <= first * 3.0, "{label}: time per record rose from {first:.2} to {last:.2} µs as plugins grew 16-fold: superlinear");
}

#[test]
#[ignore = "a timing measurement: folds profiles of up to 4,000 light plugins"]
fn the_fold_costs_per_record_as_a_wide_profile_grows() {
    scales_with_plugins(Shape { own: 100, overrides: 20, chain: false }, "wide");
}

#[test]
#[ignore = "a timing measurement: folds profiles of up to 4,000 light plugins"]
fn the_fold_costs_per_record_as_a_dependency_chain_grows() {
    scales_with_plugins(Shape { own: 100, overrides: 20, chain: true }, "chain");
}

/// The other axis: a fixed 500 plugins, records per plugin growing.
#[test]
#[ignore = "a timing measurement: folds 500 light plugins at growing record counts"]
fn the_fold_costs_per_record_as_plugins_grow_fatter() {
    eprintln!("500 plugins, records per plugin growing");
    eprintln!("  own   records   seconds   µs/record");
    let mut per_record = Vec::new();
    for own in [25u32, 100, 400, 1600] {
        let shape = Shape { own, overrides: own / 5, chain: false };
        let (seconds, records) = (0..3).map(|_| fold_seconds(500, shape)).min_by(|a, b| a.0.total_cmp(&b.0)).unwrap();
        let micros = seconds * 1e6 / f64::from(records);
        eprintln!("  {own:4} {records:9} {seconds:9.3} {micros:11.2}");
        per_record.push(micros);
    }
    let (first, last) = (per_record[0], *per_record.last().unwrap());
    assert!(last <= first * 3.0, "time per record rose from {first:.2} to {last:.2} µs as records per plugin grew 64-fold: superlinear");
}

/// The bitset closure answers what a plain walk of the master graph answers: wide masters, chains and diamonds, across a
/// 64-plugin word boundary.
#[test]
fn the_master_closure_matches_a_walk_of_the_graph() {
    // A deterministic pseudo-random DAG: each plugin masters up to three earlier ones, one of them often the one before it.
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move |bound: u32| {
        state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        ((state >> 33) as u32) % bound.max(1)
    };
    let dependencies: Vec<Vec<u32>> = (0..300u32)
        .map(|plugin| {
            let mut masters = Vec::new();
            if plugin > 0 && next(3) > 0 {
                masters.push(plugin - 1);
            }
            for _ in 0..next(3) {
                if plugin > 0 {
                    masters.push(next(plugin));
                }
            }
            masters.sort_unstable();
            masters.dedup();
            masters
        })
        .collect();
    let closure = transitive_masters(&dependencies);
    // The reference: every plugin reachable through the master edges.
    let reaches = |from: u32| {
        let mut seen = std::collections::HashSet::new();
        let mut stack = dependencies[from as usize].clone();
        while let Some(master) = stack.pop() {
            if seen.insert(master) {
                stack.extend(dependencies[master as usize].iter().copied());
            }
        }
        seen
    };
    for plugin in 0..300u32 {
        let expected = reaches(plugin);
        for master in 0..300u32 {
            assert_eq!(closure.depends_on(plugin, master), expected.contains(&master), "plugin {plugin} over master {master}");
        }
    }
    assert!(!closure.depends_on(5, 5), "a plugin does not master itself");
    assert!(!closure.depends_on(9999, 0) && !closure.depends_on(0, 9999), "an unknown plugin masters nothing");
}
