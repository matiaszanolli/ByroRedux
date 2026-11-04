//! Hygiene gate: a `file.rs:LINE` (or `file.rs#LLINE`) reference in a source
//! comment or an engine doc must point inside the file it names. (#6167)
//!
//! A module split leaves every such reference into the old file pointing past
//! its new, shorter end, and nothing else notices — 18 of them had piled up
//! by the 2026-10-19 tech-debt audit. This catches the out-of-range class
//! only (an in-range line that now means something else is invisible to it),
//! so the remedy is to name the symbol and its module instead of a line.
//!
//! A file name that resolves to several `.rs` files (`mod.rs`, `shader.rs`, …)
//! cannot be pinned to one target, so such a reference is stale only when its
//! line lies past the end of *every* candidate — a split-away `shader.rs:977`
//! can't hide behind two unrelated shorter `shader.rs` siblings (#6514).
//! `docs/audits/`, `docs/archive/` and `HISTORY.md` are exempt: they record the
//! tree as it was on the day they were written.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("workspace root")
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if matches!(name.as_ref(), "target" | ".git" | "nifskope" | "node_modules") {
                continue;
            }
            walk(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// Files that exist only upstream (a vendored/dependency crate), cited with a line number
/// on purpose: rapier's `control/character_controller.rs`, which the character controller
/// and the surface probe mirror.
const UPSTREAM_FILES: &[&str] = &["character_controller.rs"];
/// Docs whose subject is upstream code, so every `.rs:LINE` in them names the dependency's file.
const UPSTREAM_DOCS: &[&str] = &["dependency-update-followups-plan.md"];

/// Whether a reference to a basename with no in-tree file is a deliberate upstream citation.
fn is_upstream_reference(base: &str, file: &Path) -> bool {
    UPSTREAM_FILES.contains(&base)
        || file
            .file_name()
            .is_some_and(|name| UPSTREAM_DOCS.contains(&name.to_string_lossy().as_ref()))
}

/// Whether a `line` reference is out of range for every candidate file, given
/// each candidate's line count. No candidate at all is "not checkable", not stale.
fn is_stale(line: usize, candidate_lens: &[usize]) -> bool {
    candidate_lens.iter().max().is_some_and(|&max| line > max)
}

/// `(file name, line, 1-based source line, path as written)` for every
/// `<name>.rs:<n>` / `<name>.rs#L<n>` in `text`. The path keeps any directory
/// prefix (`crates/audio/src/lib.rs`) so an ambiguous basename can be narrowed.
fn line_references(text: &str) -> Vec<(String, usize, usize, String)> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = text[from..].find(".rs") {
        let end = from + at + 3;
        from = end;
        let tail = &text[end..];
        let digits_at = if let Some(rest) = tail.strip_prefix(":") {
            // #6581 — a reference wrapped at its colon (`npc_spawn.rs:` ending one
            // comment line, `// 366-367` opening the next) continues past the
            // newline and the comment marker.
            match rest.strip_prefix('\n') {
                Some(next) => tail.len() - next.trim_start().trim_start_matches(['/', '!', '*']).trim_start().len(),
                None => tail.len() - rest.len(),
            }
        } else if let Some(rest) = tail.strip_prefix("#L") {
            tail.len() - rest.len()
        } else {
            continue;
        };
        let digits: String = tail[digits_at..].chars().take_while(char::is_ascii_digit).collect();
        let Ok(line) = digits.parse::<usize>() else { continue };
        let mut start = from - 3;
        while start > 0 {
            let c = bytes[start - 1];
            if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' {
                start -= 1;
            } else {
                break;
            }
        }
        let name = text[start..from].to_string();
        let mut path_start = start;
        while path_start > 0 {
            let c = bytes[path_start - 1];
            if c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'/' | b'.') {
                path_start -= 1;
            } else {
                break;
            }
        }
        let path = text[path_start..from].trim_start_matches(['.', '/']).to_string();
        if name.len() > 3 {
            found.push((name, line, text[..start].matches('\n').count() + 1, path));
        }
    }
    found
}

#[test]
fn line_references_stay_inside_the_files_they_name() {
    let root = workspace_root();
    let mut files = Vec::new();
    for dir in ["crates", "byroredux", "tools"] {
        walk(&root.join(dir), &mut files);
    }
    let mut docs = Vec::new();
    walk(&root.join("docs/engine"), &mut docs);

    let mut sources: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for file in files.iter().filter(|f| f.extension().is_some_and(|e| e == "rs")) {
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        sources.entry(name).or_default().push(file.clone());
    }

    let this_file = Path::new(file!()).file_name().unwrap().to_owned();
    let mut line_counts: HashMap<PathBuf, usize> = HashMap::new();
    let mut stale = Vec::new();
    let mut missing = Vec::new();
    let scanned = files
        .iter()
        .filter(|f| f.extension().is_some_and(|e| e == "rs"))
        .chain(docs.iter().filter(|f| f.extension().is_some_and(|e| e == "md")));
    for file in scanned {
        if file.file_name() == Some(this_file.as_os_str()) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(file) else { continue };
        for (name, line, at, cited) in line_references(&text) {
            let base = name.rsplit('/').next().unwrap();
            let Some(all) = sources.get(base) else {
                // #6523 — a basename that resolves to no in-tree file is the case a
                // split leaves behind (`references.rs:840` after `references/`), and
                // the one a line number can never be checked against. It is stale
                // unless it cites an upstream crate's file.
                if !is_upstream_reference(base, file) {
                    missing.push(format!(
                        "{}:{at}: `{name}:{line}` names no `.rs` file in this tree (a split removed it?)",
                        file.strip_prefix(&root).unwrap().display(),
                    ));
                }
                continue;
            };
            // A cite that names directories (`crates/audio/src/lib.rs:859`) is
            // checked against the files that path actually matches; a bare
            // basename, or a path matching nothing, against every same-named file.
            let narrowed: Vec<&PathBuf> = all
                .iter()
                .filter(|f| cited.contains('/') && f.to_string_lossy().ends_with(&format!("/{cited}")))
                .collect();
            let targets: Vec<&PathBuf> = if narrowed.is_empty() { all.iter().collect() } else { narrowed };
            let lens: Vec<usize> = targets
                .iter()
                .map(|target| {
                    *line_counts.entry((*target).clone()).or_insert_with(|| {
                        std::fs::read_to_string(target).map_or(0, |t| t.lines().count())
                    })
                })
                .collect();
            if is_stale(line, &lens) {
                let longest = lens.iter().max().unwrap();
                stale.push(format!(
                    "{}:{at}: `{name}:{line}` but the longest of {} `{base}` file(s) has {longest} lines",
                    file.strip_prefix(&root).unwrap().display(),
                    targets.len(),
                ));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "line references into a `.rs` file that no longer exists in the tree — a split deleted \
         it, so the number means nothing. Cite the symbol and its module instead; a genuine \
         upstream-crate citation goes in `UPSTREAM_FILES` or the doc in `UPSTREAM_DOCS` (#6523):\n{}",
        missing.join("\n")
    );
    assert!(
        stale.is_empty(),
        "line references past the end of the file they name — cite the symbol and its module \
         instead of a line number (#6167):\n{}",
        stale.join("\n")
    );
}

#[test]
fn the_scanner_reads_both_reference_spellings() {
    let refs = line_references("see `foo/bar_baz.rs:42` and [x](../a-b.rs#L7); not `x.rs:` or a.rsx:3");
    let refs: Vec<_> = refs.into_iter().map(|(n, l, _, _)| (n, l)).collect();
    assert_eq!(
        refs,
        vec![("bar_baz.rs".to_string(), 42), ("a-b.rs".to_string(), 7)]
    );
}

#[test]
fn the_scanner_follows_a_reference_wrapped_at_its_colon() {
    // #6581: `crates/core/src/ecs/systems.rs` cited `npc_spawn.rs:` / `// 366-367`
    // across two comment lines, so the deleted file's line number went unseen.
    let refs = line_references("// Mirrors npc_spawn.rs:\n        // 366-367. This is\nsee x.rs:\n/// prose");
    let refs: Vec<_> = refs.into_iter().map(|(n, l, src, _)| (n, l, src)).collect();
    assert_eq!(refs, vec![("npc_spawn.rs".to_string(), 366, 1)]);
}

#[test]
fn an_ambiguous_basename_is_stale_when_past_every_candidate() {
    // #6514: `shader.rs` matches two live files (both short); a cite into the
    // split-away 1,000-line original must still be flagged, not skipped.
    assert!(is_stale(977, &[120, 300]));
    // In range for at least one candidate: can't be called stale.
    assert!(!is_stale(250, &[120, 300]));
    // Unique basename keeps the old behaviour.
    assert!(is_stale(11, &[10]));
    assert!(!is_stale(10, &[10]));
    // No such file: nothing to check against.
    assert!(!is_stale(5, &[]));
}

#[test]
fn the_scanner_keeps_the_cited_directory_prefix() {
    let refs = line_references("see `crates/audio/src/lib.rs:859` and [x](../a-b.rs#L7) and bare.rs:3");
    let paths: Vec<_> = refs.into_iter().map(|(_, _, _, p)| p).collect();
    assert_eq!(paths, vec!["crates/audio/src/lib.rs", "a-b.rs", "bare.rs"]);
}

#[test]
fn a_reference_to_a_split_away_file_is_not_an_upstream_citation() {
    // #6523: `references.rs` was split into `references/`; a line number into it can
    // never be checked, so it must be reported rather than skipped.
    assert!(!is_upstream_reference("references.rs", Path::new("byroredux/src/a.rs")));
    assert!(!is_upstream_reference("tri_shape.rs", Path::new("docs/engine/x.md")));
    // The deliberate exceptions: rapier's file, and a doc whose subject is upstream code.
    assert!(is_upstream_reference("character_controller.rs", Path::new("crates/physics/src/a.rs")));
    assert!(is_upstream_reference(
        "anything.rs",
        Path::new("docs/engine/dependency-update-followups-plan.md")
    ));
}
