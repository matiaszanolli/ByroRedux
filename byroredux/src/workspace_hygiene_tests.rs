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

/// Recursively collect every `examples/tmp_*` / `examples/_tmp_*` path
/// under `dir` (#5114 — the bare `tmp_` spelling used to slip past the
/// `_tmp_`-only match).
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
            .is_some_and(|name| {
                (name.starts_with("_tmp_") || name.starts_with("tmp_")) && name.ends_with(".rs")
            });
        if is_tmp_probe {
            out.push(path);
        }
    }
}

/// #5114 — collect every example file whose leading `//!` module doc
/// self-describes it as disposable. Three `tmp_fo4_d4_*` probes were
/// routed to issue reports three times (09-14, 09-16, 09-21) while no
/// tech-debt sweep picked them up, because the filename guard only
/// matched `_tmp_`; eleven committed examples said some variant of
/// "Throwaway" / "One-off" / "TEMP … not for commit" in plain text and
/// stayed. A self-description is the machine-readable half of the rule
/// the `.gitignore` entry and `collect_tmp_examples` enforce by name.
fn collect_self_described_disposable_examples(
    dir: &std::path::Path,
    out: &mut Vec<std::path::PathBuf>,
) {
    // Matched as the *start* of a doc line (`//! Throwaway …`), per the
    // audit's regex — prose that merely mentions the words mid-sentence
    // ("hundreds of one-off values", spt_tail.rs) is describing data, not
    // the file, and must not trip the guard.
    //
    // #5475 / #5330 — the marker list is the class, not the commit: the
    // M42 `cond_dump.rs` probe landed as `//! TEMP:` and three census
    // examples as `//! Scratch:` while the guard only knew "temp
    // scratch" (one phrase). "temp:" / "temp " / "scratch" cover the
    // spellings that have actually shipped; a word merely *containing*
    // them ("temporal") does not match — every entry is a prefix of the
    // trimmed doc text, so "temporal" fails "temp " (no space) and
    // "temp:" (no colon).
    const LINE_START_MARKERS: &[&str] = &[
        "throwaway",
        "one-off",
        "temp scratch",
        "temp:",
        "temp ",
        "scratch",
    ];
    // Stronger than anything the audit named — the file itself says it
    // never belonged in the tree — so it matches anywhere in the doc.
    const ANYWHERE_MARKERS: &[&str] = &["not for commit"];
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("workspace hygiene guard: unreadable dir entry").path();
        if path.is_dir() {
            collect_self_described_disposable_examples(&path, out);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        // The leading `//!` block only: a keeper that cites a deleted
        // throwaway in a later comment (provenance notes, historic
        // framing) is not self-describing.
        let flagged = contents
            .lines()
            .take_while(|line| line.starts_with("//!"))
            .any(|line| {
                let text = line
                    .trim_start_matches("//!")
                    .trim_start()
                    .to_lowercase();
                LINE_START_MARKERS.iter().any(|m| text.starts_with(m))
                    || ANYWHERE_MARKERS.iter().any(|m| text.contains(m))
            });
        if flagged {
            out.push(path);
        }
    }
}

/// Visit every `.rs` file under `dir` with its contents, skipping `target/`
/// (generated + vendored sources — build scripts, proc-macro expansions —
/// that dwarf the real tree by orders of magnitude) and `.git/`.
fn visit_workspace_rs_files(dir: &std::path::Path, visit: &mut dyn FnMut(&std::path::Path, &str)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry
            .expect("workspace hygiene scan: unreadable dir entry")
            .path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "target" || name == ".git" {
                continue;
            }
            visit_workspace_rs_files(&path, visit);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        if let Ok(contents) = std::fs::read_to_string(&path) {
            visit(&path, &contents);
        }
    }
}

/// #3749 (TD9-2026-08-30-01) — collect every source file line whose bare
/// `#[ignore]` (no `= "reason"`) makes a gated test's skip condition
/// undiscoverable without reading the function body. Only exact
/// `#[ignore]` attribute lines count — doc-comment prose that merely
/// *mentions* `` `#[ignore]` `` (there are ~20 of these, explaining the
/// convention to a reader) is deliberately excluded by requiring the
/// trimmed line to equal the attribute token itself, not just contain it.
fn collect_bare_ignores(dir: &std::path::Path, out: &mut Vec<(std::path::PathBuf, usize)>) {
    visit_workspace_rs_files(dir, &mut |path, contents| {
        for (idx, line) in contents.lines().enumerate() {
            if line.trim() == "#[ignore]" {
                out.push((path.to_path_buf(), idx + 1));
            }
        }
    });
}

/// #4766 (TD9-2026-09-22-01) — collect every `#[test]` attribute line that
/// follows another `#[test]` with only comments, blank lines or other
/// attributes in between. That shape is what a new test inserted *between*
/// an existing test's attribute and its `fn` produces: the new function
/// gets two `#[test]`s (harmless) and the old one loses its attribute and
/// silently leaves the test registry. rustc only warns
/// (`duplicate_macro_attributes`), and the CI clippy gate never builds
/// `#[cfg(test)]` code.
fn collect_stacked_test_attributes(
    dir: &std::path::Path,
    out: &mut Vec<(std::path::PathBuf, usize)>,
) {
    visit_workspace_rs_files(dir, &mut |path, contents| {
        let mut open_test_attribute = false;
        for (idx, line) in contents.lines().enumerate() {
            let line = line.trim();
            if line == "#[test]" {
                if open_test_attribute {
                    out.push((path.to_path_buf(), idx + 1));
                }
                open_test_attribute = true;
            } else if !(line.is_empty() || line.starts_with("//") || line.starts_with("#[")) {
                open_test_attribute = false;
            }
        }
    });
}

#[test]
fn no_test_attribute_is_stacked_on_another() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut found = Vec::new();
    collect_stacked_test_attributes(&manifest.join(".."), &mut found);
    assert!(
        found.is_empty(),
        "found {} `#[test]` attribute(s) stacked on another `#[test]` — the \
         function that lost its attribute is no longer a test (#4766):\n{}",
        found.len(),
        found
            .iter()
            .map(|(p, line)| format!("{}:{line}", p.display()))
            .collect::<Vec<_>>()
            .join("\n"),
    );
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
fn hygiene_scan_roots() -> Vec<std::path::PathBuf> {
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
    roots
}

#[test]
fn no_tmp_scratch_examples_are_committed() {
    let mut found = Vec::new();
    for root in hygiene_scan_roots() {
        collect_tmp_examples(&root, &mut found);
    }

    assert!(
        found.is_empty(),
        "found {} committed `tmp_*`/`_tmp_*` scratch example(s), which \
         #3150/#3746 swept to zero — either delete them or, if genuinely \
         worth keeping, drop the `_tmp_` prefix and give them a real \
         documented purpose (see watr_wind_census.rs / esm_dim8_bench.rs / \
         sf_smoke.rs):\n{}",
        found.len(),
        found
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

/// #5114 — the module-doc half of the probe guard. `dump_bgsm.rs`,
/// `roof_probe.rs` and friends predate the `_tmp_` filename rule and
/// announced their own disposability in their first doc lines; the
/// filename guard could never see them.
#[test]
fn no_committed_example_self_describes_as_disposable() {
    let mut found = Vec::new();
    for root in hygiene_scan_roots() {
        collect_self_described_disposable_examples(&root, &mut found);
    }

    assert!(
        found.is_empty(),
        "found {} committed example(s) whose module doc self-describes as \
         throwaway / one-off / TEMP / Scratch / \"not for commit\" — either \
         delete them or give them a real documented purpose like \
         watr_wind_census.rs (a probe that states what it measures and \
         why it stays), and drop the self-description (#5114, #5475):\n{}",
        found.len(),
        found
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

/// #5475 / #5330 — the classifier knows the marker spellings that have
/// actually shipped (`//! TEMP:`, `//! Scratch:`), not just the one
/// phrase the #5114 guard debuted with, and still leaves prose that
/// merely contains a marker word ("temporal", a cited historic doc)
/// alone. Fixture lives in a process-temp dir — the guard's other tests
/// scan the real tree, which cannot contain the flagged shapes (their
/// own assertions forbid it).
#[test]
fn disposable_example_classifier_knows_the_shipped_marker_spellings() {
    let root = std::env::temp_dir().join(format!(
        "byro-hygiene-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp fixture dir");
    let write = |name: &str, doc: &str| {
        std::fs::write(root.join(name), format!("{doc}\nfn main() {{}}\n"))
            .expect("temp fixture file")
    };
    write("temp_colon.rs", "//! TEMP: dump one PACK's conditions.");
    write("scratch.rs", "//! Scratch: dump every NiAlphaProperty.");
    write("temp_word.rs", "//! One-off probe with a temp scratch prefix.");
    write("keeper.rs", "//! Census of XCLW heights; kept for WATAL follow-ups.");
    // A keeper whose LATER comment cites the historic marker wording —
    // only the leading `//!` block is scanned.
    write(
        "provenance.rs",
        "//! Census probe; the payload above is the point.\n//! (The old doc said \"Scratch:\" — provenance note only.)\nfn main() {}",
    );

    let mut found = Vec::new();
    collect_self_described_disposable_examples(&root, &mut found);
    let mut names: Vec<String> = found
        .iter()
        .map(|p| p.file_name().unwrap_or_default().to_string_lossy().into_owned())
        .collect();
    names.sort();

    // Drop the trailing-slash root naming variance by comparing basenames.
    assert_eq!(
        names,
        vec![
            "scratch.rs".to_string(),
            "temp_colon.rs".to_string(),
            "temp_word.rs".to_string(),
        ],
        "TEMP:/Scratch:/temp-scratch doc lines must flag; prose and \
         provenance notes must not (#5475/#5330)"
    );

    std::fs::remove_dir_all(&root).expect("temp fixture cleanup");
}

/// #5470 — no string literal embeds a run of 6+ spaces. A Rust `"…\⏎
///    …"` continuation strips the newline and the leading whitespace,
/// but prose literals were authored (or re-flowed) as one physical line
/// with the continuation's indentation left inside — rendered text
/// reading `stamped onto loaded              exterior cell roots`. The
/// sweep collapsed every prose run; this gate keeps the class out.
///
/// Exemptions, each a real shape this scan must not touch:
/// - content containing `\n` — source-scan needles, whose spaces match
///   aligned code (`.find("pub fn new(\n        buffer: …")`);
/// - literals on a `.contains(` / `.find(` line — output-matching
///   needles against rendered or aligned text;
/// - the aligned-table files below — column-aligned help/output tables
///   where the run IS the formatting (the audit's `display.rs` /
///   `nif_stats.rs` / `probe_form.rs` trio, plus the same shape
///   elsewhere).
#[test]
fn no_string_literal_embeds_a_space_run() {
    let exempt: Vec<std::path::PathBuf> = [
        "tools/byro-dbg/src/display.rs",
        "crates/nif/examples/nif_stats.rs",
        "crates/plugin/examples/probe_form.rs",
        "byroredux/src/commands/assets.rs",
        "byroredux/src/commands/scene.rs",
        "byroredux/src/commands/world_info.rs",
        "byroredux/src/commands/view.rs",
        "byroredux/src/commands/depth.rs",
        "byroredux/src/commands_tests.rs",
        "byroredux/src/sf_smoke.rs",
        "crates/renderer/src/vulkan/groundcover.rs",
        "crates/renderer/src/vulkan/groundcover/construct.rs",
        "crates/nif/examples/ambient_light_census.rs",
        "crates/nif/examples/d5_coverage.rs",
        "crates/nif/examples/import_probe.rs",
        "crates/plugin/examples/dump_navmesh.rs",
        "crates/plugin/examples/obl_info_probe.rs",
        "crates/plugin/examples/sf_smoke.rs",
        "crates/scripting/examples/mq101_conformance.rs",
        "crates/scripting/examples/quest_fragment_populate.rs",
        "tools/byro-detect/src/main.rs",
        "tools/byro-launcher/src/main.rs",
    ]
    .iter()
    .map(|rel| {
        // Canonicalise: `visit` receives `..`-free paths from the walks,
        // and a textual `byroredux/../byroredux/...` never compares equal.
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(rel)
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from(rel))
    })
    .collect();

    let mut offenders: Vec<String> = Vec::new();
    let mut visit = |path: &std::path::Path, contents: &str| {
        // Both sides canonicalised: the walks build `..`-bearing paths
        // (`byroredux/../crates/…`), which never compare equal textually.
        let Ok(path) = path.canonicalize() else {
            return;
        };
        if exempt.contains(&path) {
            return;
        }
        for (index, line) in contents.lines().enumerate() {
            if line.contains(".contains(\"") || line.contains(".find(\"") {
                continue;
            }
            for span in string_literal_spans(line) {
                if span.contains("\\n") || span.starts_with(' ') {
                    continue;
                }
                if let Some(start) = embedded_space_run(&span) {
                    offenders.push(format!(
                        "{}:{}: …{}…",
                        path.display(),
                        index + 1,
                        &span[start.0..start.1],
                    ));
                }
            }
        }
    };
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    visit_workspace_rs_files(&manifest.join("src"), &mut visit);
    visit_workspace_rs_files(&manifest.join("examples"), &mut visit);
    visit_workspace_rs_files(&manifest.join("tests"), &mut visit);
    for root in [
        manifest.join("../crates"),
        manifest.join("../tools"),
    ] {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                visit_workspace_rs_files(&path, &mut visit);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                if let Ok(contents) = std::fs::read_to_string(&path) {
                    visit(&path, &contents);
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "found {} string literal(s) embedding a run of 6+ spaces — the \
         collapsed-continuation shape of #5470. Re-wrap the literal with a \
         backslash line continuation; if the run is column-aligned table \
         formatting, extend the exemption list with the file and a one-line \
         reason:\n{}",
        offenders.len(),
        offenders.join("\n"),
    );
}

/// Byte range of a run of 6+ spaces with a non-space on both sides,
/// inside one string-literal body. `None` when no such run exists.
fn embedded_space_run(span: &str) -> Option<(usize, usize)> {
    let bytes = span.as_bytes();
    let mut run_start = None;
    for (i, byte) in bytes.iter().enumerate() {
        if *byte == b' ' {
            if run_start.is_none() {
                run_start = Some(i);
            }
        } else if let Some(start) = run_start {
            if i - start >= 6 && start > 0 {
                return Some((start, i));
            }
            run_start = None;
        }
    }
    None
}

/// The double-quoted spans of one source line. Single-file helper for
/// [`no_string_literal_embeds_a_space_run`] — no escape handling beyond
/// a closing quote, which no scanned literal pairs mid-span.
fn string_literal_spans(line: &str) -> Vec<String> {
    let mut spans = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            if let Some(end) = line[i + 1..].find('"') {
                spans.push(line[i + 1..i + 1 + end].to_string());
                i += end + 2;
                continue;
            }
            break;
        }
        i += 1;
    }
    spans
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
        // #4042 — `.rs`-only missed the one file where this claim is most
        // damaging: `triangle.frag`, the shader whose own module doc block
        // it sits four lines below. Extended to the GLSL shader
        // extensions; the match logic below is plain text scanning, not
        // Rust-syntax-aware, so it generalizes to GLSL comments unchanged.
        // Deliberately NOT `.md`: `docs/audits/*.md` alone carries dozens
        // of accurate, dated, historical `classify_pbr` mentions in free
        // narrative prose the HISTORIC_MARKERS heuristic was never
        // designed to parse (verified — adding it turns this into a
        // several-dozen-report cleanup unrelated to this fix's actual
        // finding, not a one-line correction).
        let ext = path.extension().and_then(|e| e.to_str());
        if !matches!(ext, Some("rs" | "vert" | "frag" | "comp" | "glsl")) {
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

/// #4552 — #3073/#4444 put the parallax defaults behind
/// `DEFAULT_PARALLAX_HEIGHT_SCALE` / `DEFAULT_PARALLAX_MAX_PASSES` so a retune
/// moves every synthetic `MaterialTextureHandles` producer together. #4444's
/// sweep missed the `.btr` distant-terrain spawner and the Cornell harness,
/// which would have split distant terrain by game on the next retune. No
/// production source may restate either literal; tests may.
#[test]
fn no_production_source_restates_the_parallax_default_literals() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest.join("..");
    let mut found = Vec::new();
    visit_workspace_rs_files(&workspace_root, &mut |path, contents| {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let in_test_dir = path.components().any(|c| c.as_os_str() == "tests");
        if name.ends_with("_tests.rs") || in_test_dir {
            return;
        }
        // Tolerant cut (most files have no test module) on the strong
        // needle: a `#[cfg(test)] fn` / `use` must not truncate the scan
        // (#5469).
        let production = byroredux_core::source_scan::production_text_or_whole(contents);
        for (idx, line) in production.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            let compact: String = code.split_whitespace().collect();
            if compact.contains("parallax_height_scale:0.04")
                || compact.contains("parallax_max_passes:4.0")
            {
                found.push(format!("{}:{}  {}", path.display(), idx + 1, line.trim()));
            }
        }
    });
    assert!(
        found.is_empty(),
        "restate the parallax defaults through DEFAULT_PARALLAX_HEIGHT_SCALE / \
         DEFAULT_PARALLAX_MAX_PASSES, not literals (#4444 / #4552):\n{}",
        found.join("\n")
    );
}

/// #5469 — #5100 centralised the source-scan cut in `core::source_scan`
/// (`production_text` / `production_text_or_whole` / `strip_test_modules`),
/// but 25+ inline copies survived it with the WEAK needle: a bare
/// `find`/`split` on `#[cfg(test)]` also truncates at a `#[cfg(test)] fn` or
/// `use` (the hazard the core doc warns about), and several kept the
/// whole-file fallback #4842 removed. A guard that copies a neighbour
/// inherits whichever needle it finds, so no source may spell the weak cut
/// outside the helper itself; use the helper (strong needle
/// `\n#[cfg(test)]\nmod `).
///
/// The allowlist is exactly the sites that cannot or should not use it:
/// the helper's own home, `boot/mod.rs`'s module-name lister (not a cut),
/// `texture_registry_tests.rs` + `dynamic_rgba.rs` (`mod.rs` declares its
/// test modules block-less via `#[path]`, which the strong needle cannot
/// see), and the `pex` / `facegen` crates, which do not depend on
/// `byroredux-core` (a new dependency needs approval).
#[test]
fn no_source_scan_cuts_with_the_weak_cfg_test_needle() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest.join("..");
    // Composed at run time so this file does not contain the needles it
    // searches for.
    let attr = ["#[cfg(", "test)]"].concat();
    let weak: Vec<String> = [
        format!("find(\"{attr}\")"),
        format!("rfind(\"{attr}\")"),
        format!("split(\"{attr}\")"),
        // #5824 — the bare (no leading newline) split_once was the 18-site
        // survivor spelling: it truncates at a `#[cfg(test)] fn` / `use`
        // too, which is the hazard the strong needle exists to avoid.
        format!("split_once(\"{attr}\")"),
        format!("split_once(\"\\n{attr}\")"),
    ]
    .to_vec();
    const ALLOWED: &[&str] = &[
        "crates/core/src/source_scan.rs",
        "byroredux/src/boot/mod.rs",
        "crates/renderer/src/texture_registry_tests.rs",
        "crates/renderer/src/texture_registry/dynamic_rgba.rs",
        "crates/pex/src/decompile/boolean.rs",
        "crates/facegen/src/lib.rs",
    ];
    let mut found = Vec::new();
    visit_workspace_rs_files(&workspace_root, &mut |path, contents| {
        let relative = path
            .strip_prefix(&workspace_root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let relative = relative.trim_start_matches("./");
        if ALLOWED.iter().any(|allowed| relative.ends_with(allowed)) {
            return;
        }
        for (idx, line) in contents.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue; // prose mentioning the old cut
            }
            if weak.iter().any(|needle| line.contains(needle.as_str())) {
                found.push(format!("{relative}:{}  {}", idx + 1, trimmed));
            }
        }
    });
    assert!(
        found.is_empty(),
        "weak `#[cfg(test)]` source cut outside core::source_scan — use \
         `production_text` (file known to carry a test module),\
         `production_text_or_whole` (may have none) or `strip_test_modules` \
         (interleaved test modules) (#5469):\n{}",
        found.join("\n")
    );
}

/// #5332 — README ≤ 600 and ROADMAP ≤ 800 are measured ceilings (#5111),
/// but the `wc -l` check lived only in session-close Step 6: a `Fix #N`
/// commit that edits the docs between closes could breach the cap
/// silently (ROADMAP hit 818 through `1dc538108`), and the next close
/// inherited the trim. `include_str!` files are cargo-fingerprinted, so
/// this runs on every `cargo test -p byroredux` — the cap now holds per
/// commit. When a legitimate addition crosses a ceiling, trim before the
/// commit lands rather than deferring to the next close.
#[test]
fn readme_and_roadmap_hold_their_line_ceilings() {
    let readme = include_str!("../../README.md");
    let roadmap = include_str!("../../ROADMAP.md");
    let readme_lines = readme.lines().count();
    let roadmap_lines = roadmap.lines().count();
    assert!(
        readme_lines <= 600,
        "README.md is {readme_lines} lines against its 600-line ceiling (#5111) — \
         trim before committing (#5332)"
    );
    assert!(
        roadmap_lines <= 800,
        "ROADMAP.md is {roadmap_lines} lines against its 800-line ceiling (#5111) — \
         trim before committing (#5332)"
    );
}

/// #5009 — production code in the bin crate must resolve archive content
/// through the `asset_provider` layer, never by opening `BsaArchive` /
/// `Ba2Archive` directly. The two stragglers this swept (debug_load's
/// `resolve_nif_bytes`, `load_nif_from_args`' `--mesh` arm) both carried the
/// pre-#4658 loop shape: first-listed-wins (inverting #3637), extract
/// errors dropped on the floor, a re-open per request, and BSA-only
/// dispatch — so a BA2 `--bsa` never resolved at all. The provider route
/// (`TextureProvider::extract_mesh` / `extract_mesh_exact`) carries
/// last-wins precedence, #4658's named corrupt-entry fall-through, BA2
/// dispatch and the `Archive::open_shared` handle cache. `asset_provider`
/// itself is the layer that owns the raw readers; everything under
/// `#[cfg(test)]` is exempt (fixture builders legitimately open archives
/// directly to construct ground truth).
#[test]
fn no_production_code_opens_archives_outside_the_provider_layer() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    // Composed at run time so this file does not contain the needles it
    // searches for.
    let needles: Vec<String> = [
        ["byroredux_bsa::Bsa", "Archive::open("].concat(),
        ["byroredux_bsa::Ba2", "Archive::open("].concat(),
        ["Bsa", "Archive::open("].concat(),
        ["Ba2", "Archive::open("].concat(),
    ]
    .to_vec();
    let mut found = Vec::new();
    visit_workspace_rs_files(&manifest.join("src"), &mut |path, contents| {
        let relative = path
            .strip_prefix(manifest.parent().unwrap_or(manifest))
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if relative.contains("asset_provider/") {
            return; // the provider layer owns the raw readers
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let in_test_dir = path.components().any(|c| c.as_os_str() == "tests");
        // `npc_spawn/tests.rs` — a bare `#[cfg(test)] mod tests;` sibling
        // file, not a directory.
        if name.ends_with("_tests.rs") || name == "tests.rs" || in_test_dir {
            return; // fixture / ground-truth builders
        }
        // Strip interleaved `#[cfg(test)] mod` blocks; a `#[cfg(test)] fn`
        // or `use` must not truncate the scan (#5469's strong needle).
        let production = byroredux_core::source_scan::production_text_or_whole(contents);
        for (idx, line) in production.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            if needles.iter().any(|needle| line.contains(needle.as_str())) {
                found.push(format!("{}:{}  {}", relative, idx + 1, line.trim()));
            }
        }
    });
    assert!(
        found.is_empty(),
        "raw archive open outside the provider layer — resolve through \
         TextureProvider::extract_mesh / extract_mesh_exact (BA2 dispatch, \
         #3637 last-listed-wins, #4658 named corrupt entries, open_shared \
         caching) instead of BsaArchive/Ba2Archive::open (#5009):\n{}",
        found.join("\n"),
    );
}

/// #5818 / #5596 — little-endian reads in the bin crate go through
/// `byroredux_core::bytes` (`le_u32_at` and friends), whose slice bound
/// cannot overflow the way a hand-rolled `bytes.get(at..at + 4)` or a
/// raw `from_le_bytes` fed by a file-declared offset can. tree_lod.rs
/// re-grew the raw form three days after #5596 swept it; this keeps the
/// class out. (Crates below the bin may use `from_le_bytes` where they
/// own fixed-size header buffers — `core::bytes` itself is the seam.)
#[test]
fn no_production_code_reads_le_ints_by_hand() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let needle = ["from_", "le_bytes("].concat();
    let mut found = Vec::new();
    visit_workspace_rs_files(&manifest.join("src"), &mut |path, contents| {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let in_test_dir = path.components().any(|c| c.as_os_str() == "tests");
        if name.ends_with("_tests.rs") || name == "tests.rs" || in_test_dir {
            return;
        }
        let production = byroredux_core::source_scan::production_text_or_whole(contents);
        for (idx, line) in production.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            if line.contains(&needle) {
                found.push(format!("{}:{}  {}", path.display(), idx + 1, line.trim()));
            }
        }
    });
    assert!(
        found.is_empty(),
        "raw from_le_bytes read in bin-crate production code — use the \
         byroredux_core::bytes::le_*_at helpers so a file-declared offset \
         near usize::MAX is a miss, not an overflow (#5596/#5818):\n{}",
        found.join("\n"),
    );
}

/// #5856 — count a crate's example *targets* the way cargo discovers them:
/// every top-level `examples/*.rs` plus every `examples/<name>/main.rs`.
/// Non-target files (`examples/common/archives.rs`, a shared module) are
/// not counted and not linked.
fn count_example_targets(root: &std::path::Path) -> (usize, usize) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return (0, 0);
    };
    let (mut files, mut loc) = (0, 0);
    for entry in entries.flatten() {
        let path = entry.path();
        let is_target = if path.is_dir() {
            path.join("main.rs").is_file()
        } else {
            path.extension().and_then(|e| e.to_str()) == Some("rs")
        };
        if !is_target {
            continue;
        }
        let source = path.join("main.rs");
        let source = if source.is_file() { source } else { path };
        if let Ok(contents) = std::fs::read_to_string(&source) {
            files += 1;
            loc += contents.lines().count();
        }
    }
    (files, loc)
}

/// #5856 (TD9-2026-10-15-04) — 44 one-liner examples accreted in three
/// days (+35%) because the disposable-probe guards (#3746/#5114) only see
/// the `tmp_` prefix and throwaway markers, not purposeful-but-tiny
/// duplicates. The fix folded each crate's open-archive-print-one-thing
/// family into one `inspect.rs` with subcommands; these measured ceilings
/// (files / LOC per crate, at the fold commit) keep accretion from
/// silently recurring — the same per-commit contract as the README /
/// ROADMAP ceilings above (#5332). A new one-liner belongs in the crate's
/// `inspect.rs` subcommand table; a genuinely new multi-page census raises
/// its ceiling here, re-measured, in the same commit.
///
/// Crates are listed by their `crates/<name>` directory (or the bin's
/// `byroredux`); a crate whose `examples/` appears with no entry fails —
/// add the measured pair.
#[test]
fn example_targets_hold_their_per_crate_ceilings() {
    const CEILINGS: &[(&str, usize, usize)] = &[
        ("byroredux", 3, 660),
        ("bsa", 15, 1099),
        ("fsr3-sys", 1, 284),
        ("hkx", 14, 863),
        ("menuxml", 1, 197),
        ("nif", 38, 4289),
        ("pex", 3, 1215),
        ("plugin", 64, 7788),
        ("scripting", 13, 4755),
        ("sfmaterial", 2, 1094),
        ("spt", 6, 1561),
    ];
    let by_entry = |root: &std::path::Path| {
        // `crates/<name>/examples` or `byroredux/examples` → crate name.
        root.parent()
            .and_then(|c| c.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string()
    };
    let mut measured: Vec<(String, usize, usize)> = Vec::new();
    for root in hygiene_scan_roots() {
        if !root.is_dir() {
            continue;
        }
        let (files, loc) = count_example_targets(&root);
        measured.push((by_entry(&root), files, loc));
    }
    let mut failures = Vec::new();
    for (name, files, loc) in &measured {
        let Some(&(_, file_cap, loc_cap)) = CEILINGS.iter().find(|(n, _, _)| n == name) else {
            failures.push(format!(
                "{name}: no ceiling entry (measured {files} files / {loc} LOC) — \
                 add the measured pair to CEILINGS (#5856)"
            ));
            continue;
        };
        if *files > file_cap || *loc > loc_cap {
            failures.push(format!(
                "{name}: {files} files / {loc} LOC exceeds ceiling \
                 {file_cap} files / {loc_cap} LOC — fold the new one-liner \
                 into this crate's inspect.rs subcommands, or re-measure and \
                 raise the ceiling here in the same commit (#5856)"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "example-target ceilings breached:\n{}",
        failures.join("\n"),
    );
}
