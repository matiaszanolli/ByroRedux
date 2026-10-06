//! Support for source-scan tests — tests that `include_str!` a source file and
//! assert on its text because the property they pin cannot be exercised at
//! run time (an OS-level failure path, a removed branch).
//!
//! Plain `pub` rather than `#[cfg(test)]` so other crates' test modules can
//! call it: `cfg(test)` items do not cross crate boundaries — which is how
//! this helper came to be copied into renderer, physics and the bin crate
//! with three different needles before being centralized here (#5100).

/// A source file's production text: everything before its first
/// `#[cfg(test)] mod`.
///
/// `include_str!` brings the file's test modules in too, and those spell out
/// every needle the tests search for. A scan over the whole text is therefore
/// satisfied by the test's own literals whether or not the production code it
/// guards still exists (#4604, #5164). Scan this instead.
///
/// The cut matches `#[cfg(test)]` followed by `mod`, so a `#[cfg(test)] use`
/// import earlier in the file does not truncate production code. It assumes
/// the file's test modules trail its production code; a needle that lives
/// after the first test module fails the scan loudly rather than passing.
pub fn production_text(src: &str) -> &str {
    src.split_once("\n#[cfg(test)]\nmod ")
        .expect("source has no `#[cfg(test)] mod` — nothing to cut, so it is not a self-scan")
        .0
}

/// Occurrences of `ident` in `src` as a whole identifier — not as part of a
/// longer one (`temp_path` does not match `temp_path_2` or `my_temp_path`).
pub fn count_identifier(src: &str, ident: &str) -> usize {
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    src.match_indices(ident)
        .filter(|&(at, _)| {
            let before = src[..at].chars().next_back();
            let after = src[at + ident.len()..].chars().next();
            !before.is_some_and(is_ident) && !after.is_some_and(is_ident)
        })
        .count()
}

/// A source file's production text with **every** `#[cfg(test)] mod` block
/// removed by brace matching — for files whose test modules are interleaved
/// with production code, where [`production_text`]'s first cut would
/// silently drop the production code that follows an inner test module (the
/// #4069 lesson).
///
/// An item-level `#[cfg(test)]` (not immediately followed by `mod`) is kept:
/// searching the whole remainder for a `mod` would let the attribute bind to
/// some distant module and strip every production line between them. A
/// block-less `mod foo;` is skipped explicitly for the same reason — brace
/// matching from there would swallow the next item whole.
///
/// The module's extent is found by brace depth from its opening brace,
/// skipping string, raw-string and char literals and `//` comments so a
/// brace inside one cannot end the module early.
pub fn strip_test_modules(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(at) = rest.find("#[cfg(test)]") {
        out.push_str(&rest[..at]);
        let after = &rest[at..];
        const ATTR: &str = "#[cfg(test)]";
        let body = &after[ATTR.len()..];
        let lead = body.len() - body.trim_start().len();
        let trimmed = body.trim_start();
        let vis = ["pub(crate) ", "pub(super) ", "pub "]
            .into_iter()
            .find(|v| trimmed.starts_with(v))
            .map_or(0, str::len);
        let Some(mod_at) = trimmed[vis..]
            .starts_with("mod ")
            .then_some(ATTR.len() + lead + vis)
        else {
            // Item-level attribute — step past it and keep the text.
            out.push_str(&after[..ATTR.len()]);
            rest = &after[ATTR.len()..];
            continue;
        };
        let tail = &after[mod_at..];
        let brace = tail.find('{');
        let semi = tail.find(';');
        match (brace, semi) {
            // `mod foo;` — no body here to strip.
            (Some(b), Some(sc)) if sc < b => {
                out.push_str(&after[..mod_at + sc + 1]);
                rest = &after[mod_at + sc + 1..];
            }
            (None, Some(sc)) => {
                out.push_str(&after[..mod_at + sc + 1]);
                rest = &after[mod_at + sc + 1..];
            }
            (Some(_), _) => {
                let close = module_end(tail).expect("test module closes");
                rest = &tail[close..];
            }
            (None, None) => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Byte offset just past the brace that closes the first `{…}` in `src`,
/// skipping string, raw-string and char literals and `//` comments so a
/// brace inside one cannot end the module early.
fn module_end(src: &str) -> Option<usize> {
    let b = src.as_bytes();
    let (mut i, mut depth) = (0usize, 0usize);
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'r' if matches!(b.get(i + 1), Some(b'#' | b'"'))
                && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_')) =>
            {
                let hashes = b[i + 1..].iter().take_while(|&&c| c == b'#').count();
                if b.get(i + 1 + hashes) != Some(&b'"') {
                    i += 1;
                    continue;
                }
                let mut terminator = vec![b'"'];
                terminator.extend(std::iter::repeat_n(b'#', hashes));
                let from = i + 2 + hashes;
                let len = b[from..]
                    .windows(terminator.len())
                    .position(|w| w == terminator.as_slice())?;
                i = from + len + terminator.len();
                continue;
            }
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
            }
            // A char literal (`'{'`, `'\\''`); a lifetime (`'a`) has no
            // closing quote two or three bytes on and is left alone.
            b'\'' => {
                if b.get(i + 1) == Some(&b'\\') {
                    if let Some(end) = b[i + 2..].iter().position(|&c| c == b'\'') {
                        i += 2 + end;
                    }
                } else if b.get(i + 2) == Some(&b'\'') {
                    i += 2;
                }
            }
            b'{' => depth += 1,
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Every `.rs` file under `dir`, recursively. For the tests that walk a
/// crate's own source to derive what a guard must cover, rather than naming
/// it.
pub fn rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("readable source directory") {
        let path = entry.expect("readable directory entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuts_at_the_first_test_module_and_not_at_a_test_only_import() {
        let src = "fn a() {}\n#[cfg(test)]\nuse x;\nfn b() {}\n#[cfg(test)]\nmod tests {\n    needle\n}\n";
        let production = production_text(src);
        assert_eq!(production, "fn a() {}\n#[cfg(test)]\nuse x;\nfn b() {}");
        assert!(!production.contains("needle"));
    }

    #[test]
    fn strip_test_modules_keeps_production_after_an_inner_test_module() {
        let src = "fn a() {}\n#[cfg(test)]\nmod early {\n    needle\n}\nfn b() {}\n\
                   #[cfg(test)]\nuse keep_me;\nfn c() {}\nmod plain;\n";
        let stripped = strip_test_modules(src);
        assert!(
            stripped.contains("fn a()") && stripped.contains("fn b()") && stripped.contains("fn c()"),
            "all production items survive: {stripped}"
        );
        assert!(!stripped.contains("needle"), "test bodies are gone: {stripped}");
        assert!(
            stripped.contains("use keep_me;") && stripped.contains("mod plain;"),
            "item-level cfg(test) and block-less mod declarations are kept: {stripped}"
        );
    }

    /// A brace inside a string / char literal or a comment inside a test
    /// module must not end the module early and leak test text into the
    /// scan (#5100 — the matcher merged from the renderer's copy).
    #[test]
    fn strip_test_modules_skips_braces_inside_literals_and_comments() {
        let src = "fn a() {}\n#[cfg(test)]\nmod tests {\n    let s = \"}\";\n    // }\n\
                   let c = '}';\n    let r = r#\"{ {\"#;\n    needle\n}\nfn b() {}\n";
        let stripped = strip_test_modules(src);
        assert!(stripped.contains("fn b()"), "production after the module survives");
        assert!(
            !stripped.contains("needle"),
            "the module must strip whole — a literal brace ended it early: {stripped}"
        );
    }

    #[test]
    fn counts_whole_identifiers_only() {
        let src = "let temp_path = x; f(&temp_path); temp_path_2; my_temp_path";
        assert_eq!(count_identifier(src, "temp_path"), 2);
    }
}
