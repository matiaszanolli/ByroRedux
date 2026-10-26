//! Hygiene gate: a `file.rs:LINE` (or `file.rs#LLINE`) reference in a source
//! comment or an engine doc must point inside the file it names. (#6167)
//!
//! A module split leaves every such reference into the old file pointing past
//! its new, shorter end, and nothing else notices — 18 of them had piled up
//! by the 2026-10-19 tech-debt audit. This catches the out-of-range class
//! only (an in-range line that now means something else is invisible to it),
//! so the remedy is to name the symbol and its module instead of a line.
//!
//! Only references whose file name resolves to exactly one `.rs` file in the
//! workspace are checked — `mod.rs` and friends are ambiguous and skipped — and
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

/// `(file name, line)` for every `<name>.rs:<n>` / `<name>.rs#L<n>` in `text`.
fn line_references(text: &str) -> Vec<(String, usize, usize)> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = text[from..].find(".rs") {
        let end = from + at + 3;
        from = end;
        let tail = &text[end..];
        let digits_at = if let Some(rest) = tail.strip_prefix(":") {
            tail.len() - rest.len()
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
        if name.len() > 3 {
            found.push((name, line, text[..start].matches('\n').count() + 1));
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
    let scanned = files
        .iter()
        .filter(|f| f.extension().is_some_and(|e| e == "rs"))
        .chain(docs.iter().filter(|f| f.extension().is_some_and(|e| e == "md")));
    for file in scanned {
        if file.file_name() == Some(this_file.as_os_str()) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(file) else { continue };
        for (name, line, at) in line_references(&text) {
            let base = name.rsplit('/').next().unwrap();
            let Some([target]) = sources.get(base).map(Vec::as_slice) else { continue };
            let len = *line_counts.entry(target.clone()).or_insert_with(|| {
                std::fs::read_to_string(target).map_or(0, |t| t.lines().count())
            });
            if line > len {
                stale.push(format!(
                    "{}:{at}: `{name}:{line}` but {} has {len} lines",
                    file.strip_prefix(&root).unwrap().display(),
                    target.strip_prefix(&root).unwrap().display(),
                ));
            }
        }
    }
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
    let refs: Vec<_> = refs.into_iter().map(|(n, l, _)| (n, l)).collect();
    assert_eq!(
        refs,
        vec![("bar_baz.rs".to_string(), 42), ("a-b.rs".to_string(), 7)]
    );
}
