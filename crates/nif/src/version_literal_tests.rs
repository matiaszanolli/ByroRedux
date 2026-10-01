//! #5119 (REG-2026-09-29-03) — source scan for #1042's rule: block parsers
//! gate on named constants (`NifVersion::V4_2_1_0`,
//! `version::bsver::FO3_FNV`, the `NifVariant` feature-flag helpers), never
//! on a bare packed literal or a bare decimal `bsver` comparison.
//!
//! #1042 swept both shapes to zero and the sweep held at audit time (0 bare
//! `NifVersion(0x…)` in non-test sources, all 37 bare `bsver <op> N` hits in
//! comments or strings) — but nothing enforced it, so a reintroduced literal
//! would have compiled silently. This guard re-runs the sweep on every test
//! run: it walks `src/`, cuts each file's production text, strips comments
//! and string/char literals (the 37 comment hits must not false-positive),
//! and rejects both needles. `version.rs` is exempt: it is the constants
//! authority the rule routes through.

use std::path::{Path, PathBuf};

fn walk_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("nif version scan: unreadable dir entry").path();
        if path.is_dir() {
            walk_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// A file's production text: everything before its first `#[cfg(test)] mod`
/// module (the two-line shape every test declaration in this crate uses; a
/// `#[cfg(test)] use` import does not match the needle and stays in). The
/// literals the rule bans do appear inside test modules legitimately —
/// synthetic fixtures exercise old wire versions — so the cut is what keeps
/// this a production rule.
fn production_text(src: &str) -> &str {
    src.split_once("\n#[cfg(test)]\nmod ")
        .map_or(src, |(production, _)| production)
}

/// Blank out comment text and string/char literal contents, preserving
/// newlines and every other byte offset (violations report line numbers).
/// Rough but deliberately over-eager on exotic literals: over-blanking can
/// only miss a needle in code that was never valid Rust to begin with.
fn strip_comments_and_strings(src: &str) -> String {
    #[derive(PartialEq, Clone, Copy)]
    enum State {
        Code,
        Line,
        Block(usize),
        Str,
        Char,
        Raw(usize),
    }

    let chars: Vec<(usize, char)> = src.char_indices().collect();
    let mut out = String::with_capacity(src.len());
    let mut state = State::Code;
    let mut i = 0;

    // The hash count of a raw string starting at `chars[i] == 'r'`.
    fn raw_hash_count(chars: &[(usize, char)], i: usize) -> Option<usize> {
        let mut j = i + 1;
        let mut hashes = 0;
        while chars.get(j).map_or(false, |&(_, c)| c == '#') {
            hashes += 1;
            j += 1;
        }
        (chars.get(j).map_or(false, |&(_, c)| c == '"')).then_some(hashes)
    }
    let prev_is_ident = |chars: &[(usize, char)], i: usize| {
        i > 0
            && (chars[i - 1].1.is_ascii_alphanumeric() || chars[i - 1].1 == '_')
    };

    while i < chars.len() {
        let (_, c) = chars[i];
        match state {
            State::Code => {
                if c == '/'
                    && chars.get(i + 1).map_or(false, |&(_, n)| n == '/')
                {
                    state = State::Line;
                    out.push_str("  ");
                    i += 2;
                } else if c == '/'
                    && chars.get(i + 1).map_or(false, |&(_, n)| n == '*')
                {
                    state = State::Block(1);
                    out.push_str("  ");
                    i += 2;
                } else if c == '"' {
                    state = State::Str;
                    out.push(' ');
                    i += 1;
                } else if c == '\'' {
                    let n1 = chars.get(i + 1).map_or('\0', |&(_, n)| n);
                    let n2 = chars.get(i + 2).map_or('\0', |&(_, n)| n);
                    if n1 == '\\' || n2 == '\'' {
                        // Char literal — blank through the closing tick.
                        state = State::Char;
                    }
                    out.push(' ');
                    i += 1;
                } else if c == 'r'
                    && !prev_is_ident(&chars, i)
                    && raw_hash_count(&chars, i).is_some()
                {
                    let hashes = raw_hash_count(&chars, i).unwrap_or(0);
                    state = State::Raw(hashes);
                    out.push(' ');
                    i += 1;
                } else if c == 'b'
                    && chars.get(i + 1).map_or(false, |&(_, n)| n == 'r')
                    && raw_hash_count(&chars, i + 1).is_some()
                {
                    // `br#"…"#` byte raw string — skip the prefix, let the
                    // Raw arm blank the contents.
                    state = State::Raw(raw_hash_count(&chars, i + 1).unwrap_or(0));
                    out.push_str("  ");
                    i += 2;
                } else {
                    out.push(c);
                    i += 1;
                }
            }
            State::Line => {
                if c == '\n' {
                    state = State::Code;
                    out.push('\n');
                } else {
                    out.push(' ');
                }
                i += 1;
            }
            State::Block(depth) => {
                if c == '/'
                    && chars.get(i + 1).map_or(false, |&(_, n)| n == '*')
                {
                    state = State::Block(depth + 1);
                    i += 1;
                } else if c == '*'
                    && chars.get(i + 1).map_or(false, |&(_, n)| n == '/')
                {
                    state = if depth == 1 {
                        State::Code
                    } else {
                        State::Block(depth - 1)
                    };
                    i += 1;
                } else if c == '\n' {
                    out.push('\n');
                }
                i += 1;
            }
            State::Str => {
                if c == '\\' {
                    out.push_str("  ");
                    i += 2;
                } else if c == '"' {
                    state = State::Code;
                    out.push(' ');
                    i += 1;
                } else {
                    if c == '\n' {
                        out.push('\n');
                    } else {
                        out.push(' ');
                    }
                    i += 1;
                }
            }
            State::Char => {
                if c == '\\' {
                    out.push_str("  ");
                    i += 2;
                } else if c == '\'' {
                    state = State::Code;
                    out.push(' ');
                    i += 1;
                } else {
                    out.push(' ');
                    i += 1;
                }
            }
            State::Raw(hashes) => {
                if c == '"' {
                    let closes = chars
                        .get(i + 1..i + 1 + hashes)
                        .map_or(false, |tail| tail.iter().all(|&(_, ch)| ch == '#'));
                    if closes {
                        state = State::Code;
                        out.push(' ');
                        i += 1 + hashes;
                    } else {
                        out.push(' ');
                        i += 1;
                    }
                } else {
                    if c == '\n' {
                        out.push('\n');
                    } else {
                        out.push(' ');
                    }
                    i += 1;
                }
            }
        }
    }
    out
}

/// `NifVersion(0x…)` with a bare packed hex literal — the shape #1042
/// retired in favour of the named `NifVersion::V*` constants.
fn find_nif_version_hex_literals(code: &str) -> Vec<usize> {
    let bytes = code.as_bytes();
    let mut out = Vec::new();
    for (at, _) in code.match_indices("NifVersion") {
        let mut i = at + "NifVersion".len();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if bytes.get(i) != Some(&b'(') {
            continue;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if code[i..].to_ascii_lowercase().starts_with("0x") {
            out.push(at);
        }
    }
    out
}

/// `bsver <op> <decimal>` / `bsver() <op> <decimal>` — the shape #1042
/// retired in favour of the named `version::bsver::*` thresholds. A named
/// constant on the right-hand side never matches.
fn find_bare_bsver_comparisons(code: &str) -> Vec<usize> {
    let bytes = code.as_bytes();
    let is_ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut out = Vec::new();
    for (at, _) in code.match_indices("bsver") {
        // `bsver` must be a whole identifier (`bsver_thresholds` is not a
        // comparison site; `stream.bsver()` is — the dot is not an ident char).
        if at > 0 && is_ident(bytes[at - 1]) {
            continue;
        }
        let mut i = at + "bsver".len();
        if bytes.get(i).copied().is_some_and(is_ident) {
            continue;
        }
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if code[i..].starts_with("()") {
            i += 2;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
        }
        let Some(op) = [">=", "<=", "==", "!=", "<", ">"]
            .iter()
            .find(|op| code[i..].starts_with(**op))
        else {
            continue;
        };
        i += op.len();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if bytes.get(i).is_some_and(u8::is_ascii_digit) {
            out.push(at);
        }
    }
    out
}

fn line_of(code: &str, offset: usize) -> usize {
    code[..offset].bytes().filter(|b| *b == b'\n').count() + 1
}

fn violations_in(rel_path: &str, src: &str) -> Vec<String> {
    let code = strip_comments_and_strings(production_text(src));
    let mut out = Vec::new();
    for (needle, sites) in [
        ("NifVersion(0x…)", find_nif_version_hex_literals(&code)),
        ("bare bsver comparison", find_bare_bsver_comparisons(&code)),
    ] {
        for at in sites {
            out.push(format!("{rel_path}:{}  {needle}", line_of(&code, at)));
        }
    }
    out
}

fn is_exempt(rel_path: &str) -> bool {
    // Test files (tests.rs, *_tests.rs, dispatch_tests/) hold the synthetic
    // fixtures the production cut cannot cover when a whole file IS a test
    // module; version.rs is the constants authority the rule routes through.
    rel_path == "src/version.rs" || rel_path.contains("test")
}

#[test]
fn no_bare_version_literals_or_bsver_numeric_comparisons() {
    let src_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files(&src_root, &mut files);
    assert!(
        files.len() > 50,
        "the nif source scan found only {} files — did src/ move? (#5119)",
        files.len()
    );

    let mut violations = Vec::new();
    for path in files {
        let rel_path = path
            .strip_prefix(src_root.parent().expect("src/ has a parent"))
            .expect("walked files live under src/")
            .display()
            .to_string();
        if is_exempt(&rel_path) {
            continue;
        }
        let src = std::fs::read_to_string(&path).expect("readable nif source");
        violations.extend(violations_in(&rel_path, &src));
    }

    assert!(
        violations.is_empty(),
        "bare version literals / bsver numeric comparisons are back — use the \
         named constants (NifVersion::V*, version::bsver::*, the NifVariant \
         feature-flag helpers) so the gate survives refactors (#1042, #5119):\n{}",
        violations.join("\n")
    );
}

/// The needles match code, not the comment/string mentions the #1042 sweep
/// found 37 of — and named constants on the right-hand side stay accepted.
#[test]
fn the_needles_flag_live_code_but_not_comment_or_constant_spellings() {
    for live in [
        "let v = NifVersion(0x1401_0001);",
        "NifVersion ( 0x04000002 )",
        "if stream.bsver() >= 34 {",
        "let modern = bsver == 10;",
        "bsver > 21",
    ] {
        let stripped = strip_comments_and_strings(live);
        assert!(
            !find_nif_version_hex_literals(&stripped).is_empty()
                || !find_bare_bsver_comparisons(&stripped).is_empty(),
            "{live:?} must be flagged — the needle stopped matching"
        );
    }

    for sanctioned in [
        "// NifVersion(0x04000002) is the Morrowind packed form",
        "/// at `bsver >= 10`, i.e. #BSVER# #GT# 9",
        "let v = NifVersion::V20_2_0_7;",
        "if stream.bsver() >= crate::version::bsver::FO3_FNV {",
        "if stream.bsver() == version::bsver::FO76 {",
        "let bsver_thresholds = version::bsver::SKYRIM_SE;",
        "let msg = \"NifVersion(0x04000002) in a string literal\";",
        "let raw = r#\"bsver >= 34 raw\"#;",
        "let byte_raw = br#\"NifVersion(0x04000002)\"#;",
    ] {
        let stripped = strip_comments_and_strings(sanctioned);
        assert!(
            find_nif_version_hex_literals(&stripped).is_empty()
                && find_bare_bsver_comparisons(&stripped).is_empty(),
            "{sanctioned:?} must not be flagged — the needle false-positives"
        );
    }
}

/// The production cut excludes a trailing test module, so a fixture's
/// synthetic literals do not trip the tree scan.
#[test]
fn the_production_cut_excludes_a_trailing_test_module() {
    let src = "fn a() {}\n\n#[cfg(test)]\nmod fixtures {\n    NifVersion(0x1401_0001);\n}\n";
    let stripped = strip_comments_and_strings(production_text(src));
    assert!(find_nif_version_hex_literals(&stripped).is_empty());
}
