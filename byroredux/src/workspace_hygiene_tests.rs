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

/// #3869 — every mention of the deleted render-time `Material::classify_pbr`
/// must be framed as historic.
///
/// PBR resolves once at the parse-time `translate_material` boundary; the
/// per-draw classifier was removed in the NIFAL refactor and the live
/// symbol is the free function `classify_pbr_keyword`. A doc that says the
/// classifier "is shared with `Material::classify_pbr`" asserts a
/// render-time consumer that does not exist, which contradicts
/// `docs/engine/nifal.md`'s no-render-time-fallback rule — the rule whose
/// violation `_audit-severity.md` scores HIGH.
///
/// This has now been fixed four times as a single-file edit (#1321, #1522,
/// #1624, #3869) and come back each time; #1624's own SIBLING check
/// asserted "no other doc names the deleted `Material::classify_pbr` as
/// live" while this very file falsified it. A tree-wide gate is what turns
/// that check from a claim into a fact.
fn collect_live_classify_pbr_claims(
    dir: &std::path::Path,
    out: &mut Vec<(std::path::PathBuf, usize, String)>,
) {
    const HISTORIC_MARKERS: &[&str] = &[
        "deleted",
        "removed",
        "was ",
        "used to",
        "no longer",
        "former",
        "pre-canonical",
        "no per-draw",
        "once mirrored",
    ];
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry
            .expect("classify_pbr guard: unreadable dir entry")
            .path();
        if path.is_dir() {
            // `target/` is build output and `.claude/issues/` holds
            // immutable issue snapshots (TD10-001 / #1156) — neither is
            // source this gate can or should ask anyone to edit.
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name != "target" && name != ".git" {
                collect_live_classify_pbr_claims(&path, out);
            }
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        // This guard's own doc prose necessarily quotes the dead name to
        // explain what it forbids.
        if path.file_name().and_then(|n| n.to_str()) == Some("workspace_hygiene_tests.rs") {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        let lines: Vec<&str> = source.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            // Match `classify_pbr` only as a whole identifier. That
            // excludes the live `classify_pbr_keyword`, the test names
            // derived from it (`classify_pbr_scrap_metal_is_...`), and this
            // guard's own helpers — all of which merely contain the
            // substring and assert nothing about a render-time classifier.
            let is_ident = |c: char| c.is_alphanumeric() || c == '_';
            let bytes = line.as_bytes();
            let Some(at) = line.match_indices("classify_pbr").find(|(i, _)| {
                let before_ok = *i == 0 || !is_ident(bytes[i - 1] as char);
                let after = i + "classify_pbr".len();
                let after_ok = after >= bytes.len() || !is_ident(bytes[after] as char);
                before_ok && after_ok
            }) else {
                continue;
            };
            let _ = at;
            // A wrapped doc comment can carry its marker on the line above.
            let context = format!("{} {}", lines.get(idx.wrapping_sub(1)).unwrap_or(&""), line)
                .to_lowercase();
            if HISTORIC_MARKERS.iter().any(|m| context.contains(m)) {
                continue;
            }
            out.push((path.clone(), idx + 1, line.trim().to_owned()));
        }
    }
}

#[test]
fn no_source_file_frames_the_deleted_classify_pbr_as_live() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest.join("..");

    let mut found = Vec::new();
    collect_live_classify_pbr_claims(&workspace_root, &mut found);

    assert!(
        found.is_empty(),
        "found {} mention(s) of the deleted render-time `Material::classify_pbr` \
         with no historic framing. The live symbol is the free function \
         `classify_pbr_keyword`; PBR resolves once at the parse-time \
         `translate_material` boundary and there is no per-draw fallback. \
         Either name `classify_pbr_keyword` or say the render-time one was \
         deleted/removed (#3869, the fourth recurrence of #1321/#1522/#1624):\n{}",
        found.len(),
        found
            .iter()
            .map(|(p, line, text)| format!("{}:{line}  {text}", p.display()))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}
