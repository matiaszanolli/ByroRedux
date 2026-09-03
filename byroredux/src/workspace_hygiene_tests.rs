//! #3746 (TD8-2026-08-30-01) — self-enforcing companion to the
//! `.gitignore` rule added alongside it. The `.gitignore` entry
//! (`crates/*/examples/_tmp_*`) stops a *fresh* scratch probe from ever
//! reaching `git add`, but it can't catch one that was force-added
//! (`git add -f`) or that predates the rule reappearing after a revert.
//! This test scans the actual on-disk tree the same way `cargo
//! build --examples` would discover targets, independent of git, so it
//! also holds for a tarball checkout with no `.git` directory at all.
//!
//! #3150/#3746 swept 98 accumulated `_tmp_*` probes tree-wide
//! (2026-09-02); this is what keeps that count at zero going forward.

/// Recursively collect every `examples/_tmp_*` path under `dir`.
fn collect_tmp_examples(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        // A workspace with no `examples/` directory at all (most crates)
        // is not an error — nothing to scan.
        return;
    };
    for entry in entries {
        let path = entry.expect("workspace hygiene guard: unreadable dir entry").path();
        if path.is_dir() {
            collect_tmp_examples(&path, out);
            continue;
        }
        let is_tmp_probe = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|name| name.starts_with("_tmp_") && name.ends_with(".rs"));
        if is_tmp_probe {
            out.push(path);
        }
    }
}

/// #3746 — every `crates/*/examples/` directory in the workspace, plus
/// `byroredux/examples` (none exist there today, but a future one should
/// be covered automatically rather than needing this test widened).
/// Enumerated the same way `discover_scan_roots` in
/// `save_io/registry_completeness_tests.rs` discovers `src/` roots — from
/// the manifest directory outward, not a hand-maintained list.
#[test]
fn no_tmp_scratch_examples_are_committed() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_crates = manifest.join("../crates");

    let mut roots = vec![manifest.join("examples")];
    let entries = std::fs::read_dir(&workspace_crates).unwrap_or_else(|e| {
        panic!(
            "workspace hygiene guard can't read {} ({e}); the workspace \
             crates/ directory moved.",
            workspace_crates.display()
        )
    });
    for entry in entries {
        let path = entry.expect("workspace hygiene guard: unreadable crates/ entry").path();
        if path.is_dir() {
            roots.push(path.join("examples"));
        }
    }

    let mut found = Vec::new();
    for root in &roots {
        collect_tmp_examples(root, &mut found);
    }

    assert!(
        found.is_empty(),
        "found {} committed `_tmp_*` scratch example(s), which #3150/#3746 \
         swept to zero — either delete them or, if genuinely worth keeping, \
         drop the `_tmp_` prefix and give them a real documented purpose \
         (see watr_wind_census.rs / esm_dim8_bench.rs / sf_smoke.rs):\n{}",
        found.len(),
        found
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\n"),
    );
}
