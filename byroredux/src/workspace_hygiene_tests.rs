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

/// #3749 (TD9-2026-08-30-01) — recursively collect every source file
/// under `dir` whose bare `#[ignore]` line (no `= "reason"`) makes a gated
/// test's skip condition undiscoverable without reading the function body.
/// Only exact `#[ignore]` attribute lines count — doc-comment prose that
/// merely *mentions* `` `#[ignore]` `` (there are ~20 of these, explaining
/// the convention to a reader) is deliberately excluded by requiring the
/// trimmed line to equal the attribute token itself, not just contain it.
fn collect_bare_ignores(dir: &std::path::Path, out: &mut Vec<(std::path::PathBuf, usize)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("ignore-reason guard: unreadable dir entry").path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // `target/` holds generated + vendored sources (build scripts,
            // proc-macro expansions) this guard has no business scanning,
            // and it dwarfs the real tree by orders of magnitude.
            if name == "target" || name == ".git" {
                continue;
            }
            collect_bare_ignores(&path, out);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (idx, line) in contents.lines().enumerate() {
            if line.trim() == "#[ignore]" {
                out.push((path.clone(), idx + 1));
            }
        }
    }
}

/// #3749 — the fix *is* the test: TD9-2026-08-30-01 found 80% of
/// `#[ignore]`d tests carried no machine-readable reason, which had
/// already produced two wrong audit baselines (#3440, #3456) built by
/// someone who couldn't tell "environment-dependent bench" from "needs
/// Starfield's 9 GB CDB on disk" without opening every function body.
/// 138 bare sites across 27 files were converted to `#[ignore = "..."]`
/// in the fix commit; this scan is what keeps that count at zero for
/// every `#[ignore]` added after it, workspace-wide.
#[test]
fn every_ignore_attribute_carries_a_reason() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest.join("..");

    let mut found = Vec::new();
    collect_bare_ignores(&workspace_root, &mut found);

    assert!(
        found.is_empty(),
        "found {} bare `#[ignore]` attribute(s) with no reason string — \
         give each one `#[ignore = \"needs <GAME> game data on disk\"]` (or \
         whatever the true skip condition is; see the ~20 existing reason \
         strings for the established phrasing) so a reader — or an audit \
         building a baseline from `--ignored` output — doesn't have to open \
         the function body to learn why it's gated (#3749):\n{}",
        found.len(),
        found
            .iter()
            .map(|(p, line)| format!("{}:{line}", p.display()))
            .collect::<Vec<_>>()
            .join("\n"),
    );
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
