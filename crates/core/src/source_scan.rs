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

/// [`production_text`] for a file that MAY have no test module — a scan
/// over a list of files where some carry tests and some do not. With no
/// `#[cfg(test)] mod` the whole file is production text, which is correct
/// (there is nothing to cut), and with one the cut is the same strong
/// needle, so a `#[cfg(test)] fn`/`use` earlier in the file cannot truncate
/// the scan the way `find("#[cfg(test)]")` did (#5469). Prefer
/// [`production_text`] when the file is known to carry a test module: it
/// fails loudly if that stops being true, this does not.
pub fn production_text_or_whole(src: &str) -> &str {
    src.split_once("\n#[cfg(test)]\nmod ")
        .map_or(src, |(production, _)| production)
}

/// The inverse of [`strip_test_modules`]: the bodies of every top-level
/// `#[cfg(test)] mod … { … }` block, concatenated, so a scan that must only see
/// test code cannot be satisfied by production code between the modules
/// (#4411; moved here from `material_translate` by #6149). A block ends at the
/// first `}` in column 0, which holds for rustfmt-formatted top-level items.
pub fn test_module_text(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(start) = rest.find("\n#[cfg(test)]\nmod ") {
        let after = &rest[start + 1..];
        match after.find("\n}\n") {
            Some(end) => {
                out.push_str(&after[..end]);
                rest = &after[end + 3..];
            }
            None => {
                out.push_str(after);
                return out;
            }
        }
    }
    out
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

/// Lines of code in the function whose header begins with `signature`: the
/// lines from the signature's own line through the line holding the closing
/// brace of its body, **minus blank lines and comment-only lines** (`//`, `///`,
/// `//!`). The body's extent is found by brace depth, skipping string,
/// raw-string and char literals and `//` comments, so a `{}` format placeholder
/// cannot end it early. For the function-size pins that stop a split function
/// from quietly regrowing (#6143): they budget logic, so documenting a function
/// can no longer break its pin while adding code still does (#6720).
/// `signature` must be unique enough to name one function. Panics if it is not
/// found or never closes.
pub fn function_line_count(src: &str, signature: &str) -> usize {
    code_line_count(function_text(src, signature))
}

/// Lines of `text` that are neither blank nor comment-only. A line whose first
/// non-space characters are `//` is a comment; a trailing `//` after code does
/// not make the line one. Block comments (`/* */`) are not recognised — none of
/// the budgeted functions use them.
pub fn code_line_count(text: &str) -> usize {
    text.lines()
        .filter(|line| {
            let line = line.trim_start();
            !line.is_empty() && !line.starts_with("//")
        })
        .count()
}

/// The text of the function whose header begins with `signature`, from the
/// signature through its closing brace — the extent `function_line_count`
/// measures. For order pins that must compare call sites *inside one function*:
/// needles found across a whole file compare where helpers are defined, not the
/// order the function runs them in (#6494). Panics like `function_line_count`.
pub fn function_text<'a>(src: &'a str, signature: &str) -> &'a str {
    let at = src
        .find(signature)
        .unwrap_or_else(|| panic!("`{signature}` not found in source"));
    let end = module_end(&src[at..]).unwrap_or_else(|| panic!("`{signature}` never closes"));
    &src[at..at + end]
}

/// The name an item declaration line introduces: `fn`, `struct`, `enum`,
/// `trait`, `type`, `static`, `const`, `mod` or `union`, after any visibility
/// and `async` / `unsafe` / `extern "…"` / `default` qualifiers. `None` for
/// anything else (`impl`, `use`, a statement, a blank line).
fn declared_item_name(line: &str) -> Option<&str> {
    let mut rest = line.trim_start();
    if let Some(after) = rest.strip_prefix("pub") {
        rest = after.trim_start();
        if rest.starts_with('(') {
            rest = rest[rest.find(')')? + 1..].trim_start();
        }
    }
    loop {
        let before = rest;
        for qualifier in ["async ", "unsafe ", "default ", "extern "] {
            if let Some(after) = rest.strip_prefix(qualifier) {
                rest = after.trim_start();
                if qualifier == "extern " && rest.starts_with('"') {
                    rest = rest[1..].split_once('"')?.1.trim_start();
                }
            }
        }
        if rest.len() == before.len() {
            break;
        }
    }
    let (keyword, mut tail) = rest.split_once(' ')?;
    if keyword == "const" {
        // `const fn name` is a function; `const NAME: T` is a constant.
        if let Some(after) = tail.trim_start().strip_prefix("fn ") {
            tail = after;
        }
    } else if !["fn", "struct", "enum", "trait", "type", "static", "mod", "union"].contains(&keyword) {
        return None;
    }
    let tail = tail.trim_start();
    let tail = tail.strip_prefix("mut ").unwrap_or(tail);
    let end = tail.find(|c: char| !(c.is_alphanumeric() || c == '_')).unwrap_or(tail.len());
    (end > 0).then(|| &tail[..end])
}

/// The first `` `…` `` span of `doc` that is not the label of an intra-doc link.
fn first_backtick_span(doc: &str) -> Option<&str> {
    let mut rest = doc;
    let mut consumed = 0;
    loop {
        let open = rest.find('`')?;
        let after = &rest[open + 1..];
        let close = after.find('`')?;
        let linked = doc[..consumed + open].ends_with('[');
        if !linked {
            return Some(&after[..close]);
        }
        let advance = open + close + 2;
        consumed += advance;
        rest = &rest[advance..];
    }
}

/// 1-based first lines of `///` doc blocks that look spliced onto the wrong
/// item (#6373, the #5809 / #6064 / #6268 class): the block's first backticked
/// identifier names an item declared within the next 40 lines at the same
/// indentation, yet the block documents a *different* item. That is what a new
/// item inserted into the middle of an existing item's doc leaves behind: the
/// contract of the later item now heads the earlier one.
///
/// A heuristic with a precise failure mode: a doc that names a nearby sibling
/// before its own subject (rare) is flagged; fix it by leading with the subject.
///
/// A block is also flagged when it fuses two items' docs (#6585, see
/// `fuses_two_docs`): the shape an insertion leaves when the displaced doc opens
/// with an issue number, a record name or prose rather than a name.
pub fn spliced_doc_blocks(src: &str) -> Vec<usize> {
    const LOOKAHEAD: usize = 40;
    let lines: Vec<&str> = src.lines().collect();
    let is_doc = |l: &str| {
        let t = l.trim_start();
        t.starts_with("///") && !t.starts_with("////")
    };
    let indent = |l: &str| l.len() - l.trim_start().len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if !is_doc(lines[i]) {
            i += 1;
            continue;
        }
        let first = i;
        while i < lines.len() && is_doc(lines[i]) {
            i += 1;
        }
        // The documented item: skip attributes, which may span lines.
        let mut at = i;
        let mut depth = 0i32;
        while at < lines.len() {
            let t = lines[at].trim_start();
            if depth == 0 && !t.starts_with("#[") && !t.starts_with("#![") {
                break;
            }
            depth += t.matches('[').count() as i32 - t.matches(']').count() as i32;
            at += 1;
        }
        let Some(item_line) = lines.get(at) else { break };
        let Some(own) = declared_item_name(item_line) else { continue };
        if fuses_two_docs(&lines, first, i, at) {
            out.push(first + 1);
            continue;
        }
        // First backtick span of the block, when it is a bare identifier.
        let doc: String = lines[first..i]
            .iter()
            .map(|l| l.trim_start().trim_start_matches('/'))
            .collect::<Vec<_>>()
            .join(" ");
        // An intra-doc link (``[`name`]``) is a reference, not the subject.
        let Some(span) = first_backtick_span(&doc) else { continue };
        // Only a doc that LEADS with the name is claiming that name as its
        // subject; prose that merely mentions a sibling later is not.
        let lead = doc.trim_start();
        if !lead.starts_with('`') {
            continue;
        }
        // "`sibling`'s companion" documents the companion, not the sibling.
        if lead[span.len() + 2..].starts_with('\'') {
            continue;
        }
        if span == own || span.is_empty() || !span.chars().all(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let want = indent(item_line);
        let named_below = lines[at + 1..lines.len().min(at + 1 + LOOKAHEAD)]
            .iter()
            .any(|l| indent(l) == want && declared_item_name(l) == Some(span));
        if named_below {
            out.push(first + 1);
        }
    }
    out
}

/// The text of one `///` line, marker and surrounding space removed.
fn doc_line_text(line: &str) -> &str {
    line.trim_start().trim_start_matches('/').trim()
}

/// The backticked identifiers (three characters or more) of one doc line, leaving out intra-doc
/// link labels (``[`name`]``), which refer to an item rather than naming the subject.
fn doc_line_names(text: &str) -> Vec<&str> {
    let mut names = Vec::new();
    let mut from = 0;
    while let Some(open) = text[from..].find('`').map(|at| from + at) {
        let Some(close) = text[open + 1..].find('`').map(|at| open + 1 + at) else { break };
        let span = &text[open + 1..close];
        let linked = text[..open].ends_with('[');
        if !linked && span.len() >= 3 && span.chars().all(|c| c.is_alphanumeric() || c == '_') {
            names.push(span);
        }
        from = close + 1;
    }
    names
}

/// #6585 — the second splice shape: an item inserted between an existing item's doc and the item
/// itself fuses the two docs into one block above the new item, and leaves the old item below it with
/// no doc. Three signals together, so that a doc which merely mentions its neighbour does not match:
/// - the item `x` declared next at the same indentation has no doc of its own;
/// - the block's first line names something `x` carries and the documented item does not;
/// - a later line of the block opens a new sentence (the previous line ends one) on a name the
///   documented item carries and `x` does not. That line is where the inserted item's doc begins.
fn fuses_two_docs(lines: &[&str], first: usize, end: usize, at: usize) -> bool {
    const LOOKAHEAD: usize = 40;
    let is_doc = |l: &str| {
        let t = l.trim_start();
        t.starts_with("///") && !t.starts_with("////")
    };
    let indent = |l: &str| l.len() - l.trim_start().len();
    let want = indent(lines[at]);
    let mut next = None;
    for (k, line) in lines.iter().enumerate().take(lines.len().min(at + 1 + LOOKAHEAD)).skip(at + 1) {
        if line.trim().is_empty() {
            continue;
        }
        if indent(line) < want {
            return false;
        }
        if indent(line) == want && declared_item_name(line).is_some() {
            next = Some(k);
            break;
        }
    }
    let Some(x_at) = next else { return false };
    let mut above = x_at - 1;
    while above > at && lines[above].trim_start().starts_with("#[") {
        above -= 1;
    }
    if is_doc(lines[above]) {
        return false;
    }
    let mut x_end = x_at + 1;
    while x_end < lines.len().min(x_at + LOOKAHEAD) {
        let line = lines[x_end];
        let t = line.trim_start();
        let item_boundary = declared_item_name(line).is_some() || is_doc(line) || t.starts_with("#[");
        if !t.is_empty() && indent(line) <= want && item_boundary {
            break;
        }
        x_end += 1;
    }
    let documented = lines[at..x_at].join("\n").to_lowercase();
    let neighbour = lines[x_at..x_end].join("\n").to_lowercase();
    let names_only = |text: &str, here: &str, not_there: &str| {
        doc_line_names(text).iter().any(|name| {
            let name = name.to_lowercase();
            here.contains(&name) && !not_there.contains(&name)
        })
    };
    if !names_only(doc_line_text(lines[first]), &neighbour, &documented) {
        return false;
    }
    (first + 1..end).any(|k| {
        let line = doc_line_text(lines[k]);
        let opens = line.starts_with(|c: char| c.is_uppercase() || c == '`' || c == '#');
        doc_line_text(lines[k - 1]).ends_with('.') && opens && names_only(line, &documented, &neighbour)
    })
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
    fn production_text_or_whole_cuts_at_a_test_module_or_keeps_everything() {
        // A test module: same cut as `production_text`.
        let with_mod = "fn a() {}\n#[cfg(test)]\nmod tests { fn t() {} }\n";
        assert_eq!(production_text_or_whole(with_mod), "fn a() {}");
        // An item-level gate before the module does NOT truncate (the weak
        // `#[cfg(test)]` cut would have stopped at the `use`).
        let item_gate = "#[cfg(test)]\nuse x::y;\nfn a() {}\n#[cfg(test)]\nmod tests {}\n";
        assert!(production_text_or_whole(item_gate).contains("fn a()"));
        // No test module at all: the whole file is production.
        let none = "fn a() {}\nfn b() {}\n";
        assert_eq!(production_text_or_whole(none), none);
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

    /// #6143 — the size pins count a function from its signature line to the
    /// line of its closing brace, counting only braces that are code. #6720 —
    /// and only lines of code: blank and comment-only lines are free.
    #[test]
    fn function_line_count_spans_signature_to_closing_brace() {
        let src = "fn before() {}\n\n/// docs are not counted\npub(super) fn target(\n    a: u32,\n) -> u32 {\n\
                   \x20   let s = \"}\";\n    // }\n    let c = '{';\n    format!(\"{x}\");\n    a\n}\nfn after() {}\n";
        // Signature line through closing brace is 9 lines; the `// }` comment line is not code.
        assert_eq!(function_line_count(src, "fn target("), 8);
        assert_eq!(function_line_count(src, "fn before("), 1);
        // #6494 — the same extent as text: the order pins search inside it.
        let target = function_text(src, "fn target(");
        assert!(target.starts_with("fn target(") && target.ends_with("    a\n}"));
        assert!(!target.contains("fn after("));
        assert_eq!(function_text(src, "fn before("), "fn before() {}");
    }

    /// #6720 — adding a comment or a blank line never moves the count; adding a
    /// code line always does.
    #[test]
    fn function_line_count_ignores_blank_and_comment_lines() {
        let bare = "fn f() {\n    let a = 1;\n    a\n}\n";
        let commented = "fn f() {\n    // why\n\n    /// doc-style\n    let a = 1; // trailing\n    //! inner\n    a\n}\n";
        let grown = "fn f() {\n    let a = 1;\n    let b = 2;\n    a\n}\n";
        assert_eq!(function_line_count(bare, "fn f("), 4);
        assert_eq!(function_line_count(commented, "fn f("), 4, "comments and blanks are free");
        assert_eq!(function_line_count(grown, "fn f("), 5, "a code line is counted");
        assert_eq!(code_line_count("  // c\n\n  x\n  y // t\n"), 2);
    }

    #[test]
    #[should_panic(expected = "not found")]
    fn function_line_count_panics_when_the_function_is_gone() {
        function_line_count("fn a() {}", "fn b(");
    }

    #[test]
    fn counts_whole_identifiers_only() {
        let src = "let temp_path = x; f(&temp_path); temp_path_2; my_temp_path";
        assert_eq!(count_identifier(src, "temp_path"), 2);
    }

    #[test]
    fn declared_item_names_read_through_qualifiers() {
        assert_eq!(declared_item_name("pub(crate) async fn run(x: u8)"), Some("run"));
        assert_eq!(declared_item_name("    pub const fn make() -> u8 {"), Some("make"));
        assert_eq!(declared_item_name("const LIMIT: u32 = 4;"), Some("LIMIT"));
        assert_eq!(declared_item_name("pub unsafe extern \"C\" fn raw()"), Some("raw"));
        assert_eq!(declared_item_name("static mut COUNTER: u8 = 0;"), Some("COUNTER"));
        assert_eq!(declared_item_name("impl Foo {"), None);
        assert_eq!(declared_item_name("let x = 1;"), None);
    }

    /// #6373 — the shape every site had: the doc written for `b` heads `a`
    /// (`a` keeps its own doc glued underneath), and `b` has none.
    #[test]
    fn a_doc_block_naming_the_item_below_is_flagged() {
        let spliced = "/// `later` returns the thing.\n/// Does a's own job.\nfn a() {}\n\nfn later() {}\n";
        assert_eq!(spliced_doc_blocks(spliced), vec![1]);
        // Attributes between the doc and the item (even multi-line ones) are skipped.
        let attributed = "/// `later` does it.\n#[instrument(\n    skip_all,\n)]\nfn a() {}\nfn later() {}\n";
        assert_eq!(spliced_doc_blocks(attributed), vec![1]);
    }

    #[test]
    fn well_placed_and_unrelated_docs_are_not_flagged() {
        // The subject leads and is the item below.
        assert!(spliced_doc_blocks("/// `a` does it.\nfn a() {}\nfn b() {}\n").is_empty());
        // The first span is not an item declared nearby.
        assert!(spliced_doc_blocks("/// Reads `n` bytes.\nfn a(n: usize) {}\nfn b() {}\n").is_empty());
        // A same-named item at another indentation is not a sibling.
        assert!(spliced_doc_blocks("/// `b` first.\nfn a() {\n    fn b() {}\n}\n").is_empty());
        // Beyond the lookahead window.
        let far = format!("/// `z` first.\nfn a() {{}}\n{}fn z() {{}}\n", "\n".repeat(60));
        assert!(spliced_doc_blocks(&far).is_empty());
        // A possessive documents something other than the named sibling.
        assert!(spliced_doc_blocks("/// `b`'s companion.\nfn a() {}\nfn b() {}\n").is_empty());
        // Prose that mentions a sibling after its own subject is not a claim on it.
        assert!(spliced_doc_blocks("/// Does a's job, like `b`.\nfn a() {}\nfn b() {}\n").is_empty());
        // A non-identifier first span is skipped.
        assert!(spliced_doc_blocks("/// `Vec<u8>` of `b`.\nfn a() {}\nfn b() {}\n").is_empty());
    }

    /// Join fixture lines: a multi-line string literal would put each `///` at column 0, where the
    /// workspace splice guard reads this file's own fixtures as doc blocks.
    fn src(lines: &[&str]) -> String {
        lines.join("\n")
    }

    /// #6585 — the fused shape, from the three splices the first signal let through: the old item's doc
    /// leads the block, the inserted item's doc follows as a new sentence, and the old item below is left
    /// undocumented. `story_events.rs` led with an issue number, `pack.rs` with a record name and
    /// `sf_smoke.rs` with prose, so none opened on the old item's name.
    #[test]
    fn a_doc_fused_from_two_items_docs_is_flagged() {
        let story_events = src(&[
            "/// #5434 — `KILL` (Actor Kill), raised from every live death producer.",
            "/// Slots: R1 = victim, R2 = killer.",
            "/// M43 — `OAAT` (Actor Attach): an actor was placed into a loaded cell.",
            "pub(crate) fn raise_actor_attach_story_event(world: &World, actor: EntityId) {",
            "    const OAAT: [u8; 4] = *b\"OAAT\";",
            "}",
            "",
            "pub(crate) fn raise_kill_story_event(world: &World, victim: EntityId) {",
            "    raise(KILL, victim);",
            "}",
        ]);
        assert_eq!(spliced_doc_blocks(&story_events), vec![1]);
        let pack = src(&[
            "/// FO3/FNV package procedure-type index for `Sandbox` — idle activities. See M42.",
            "/// `PKDT` Interrupt Flags bit: *Random conversations*.",
            "pub const PKDT_INTERRUPT_RANDOM_CONVERSATIONS: u16 = 0x0002;",
            "",
            "pub const PROCEDURE_SANDBOX: u32 = 12;",
        ]);
        assert_eq!(spliced_doc_blocks(&pack), vec![1]);
        let sf_smoke = src(&[
            "/// Smoke-test a single cell under the current `GameKind` dispatch.",
            "/// The function never returns: print, exit.",
            "/// Which non-`statics` index map holds `form_id`, if any (#2637).",
            "fn nonstatic_base_type(index: &EsmIndex, form_id: u32) -> Option<&str> {",
            "    index.lookup(form_id)",
            "}",
            "",
            "pub fn run(path: &str, game: GameKind) {}",
        ]);
        assert_eq!(spliced_doc_blocks(&sf_smoke), vec![1]);
    }

    /// The near misses the fused-doc signal must leave alone, each taken from the tree: a helper's doc
    /// naming the caller below it, through an intra-doc link or in a sentence that runs across the line
    /// break; and a fused-looking block whose neighbour has a doc of its own.
    #[test]
    fn a_doc_that_only_mentions_its_neighbour_is_not_fused() {
        let linked = src(&[
            "/// Maximum nesting depth [`lower_statements`] recurses through.",
            "/// Capped at `MAX_STMT_DEPTH`, the smaller bound.",
            "const MAX_CONDITIONAL_DEPTH: u32 = MAX_STMT_DEPTH;",
            "",
            "fn lower_statements(depth: u32) {}",
        ]);
        assert!(spliced_doc_blocks(&linked).is_empty());
        let mid_sentence = src(&[
            "/// Shared control flow behind `sample_scalar`; after the last key holds the",
            "/// last key's value. `time_of` extracts the time a key type carries.",
            "fn sample_keyed(time_of: impl Fn() -> f32) {}",
            "",
            "fn sample_scalar() { sample_keyed(|| 0.0) }",
        ]);
        assert!(spliced_doc_blocks(&mid_sentence).is_empty(), "the restart must open a sentence on a new line");
        let documented_neighbour = src(&[
            "/// Raised for `KILL` deaths. M43 — the attach.",
            "/// `OAAT` (Actor Attach): an actor was placed.",
            "fn raise_attach() { OAAT }",
            "",
            "/// The kill event.",
            "fn raise_kill() { KILL }",
        ]);
        assert!(spliced_doc_blocks(&documented_neighbour).is_empty(), "the neighbour keeps its own doc");
    }
}
