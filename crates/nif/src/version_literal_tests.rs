//! #5119 (REG-2026-09-29-03) — source scan for #1042's rule: block parsers
//! gate on named constants (`NifVersion::V4_2_1_0`,
//! `version::bsver::FO3_FNV`, the `NifVariant` feature-flag helpers), never
//! on a bare packed literal or a bare decimal `bsver` comparison.
//!
//! #1042 swept both shapes to zero and the sweep held at audit time (0 bare
//! `NifVersion(0x…)` in non-test sources, all 37 bare `bsver <op> N` hits in
//! comments or strings) — but nothing enforced it, so a reintroduced literal
//! would have compiled silently. This guard re-runs the sweep on every test
//! run: it walks `src/`, strips comments and string/char literals (the 37
//! comment hits must not false-positive), blanks every `#[cfg(test)]`-gated
//! `mod` item (the synthetic fixtures legitimately exercise old wire
//! versions), and rejects both needles. `version.rs` is exempt: it is the
//! constants authority the rule routes through.

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

/// Blank every `#[cfg(test)]`-gated `mod` item (both the `mod name;`
/// file-declaration and the inline `mod name { … }`) out of
/// already-stripped text, preserving newlines and every other byte
/// offset so violations keep reporting real line numbers.
///
/// #5258 — the old production cut truncated the file at the FIRST
/// `\n#[cfg(test)]\nmod `, which is right for a positive scan (a needle
/// after the cut fails loudly) but wrong for this *negative* scan: any
/// production code after an early test-module declaration was never
/// scanned and nothing reported it (controller/mod.rs hid lines 22-902 —
/// every controller parser and 8 live version gates; texture.rs hid the
/// `NiTextureEffect` parser; import/walk/mod.rs hid its name resolvers).
/// Braces inside literals are not a hazard here: the input is the output
/// of [`strip_comments_and_strings`], which has already blanked comment
/// and literal contents — and the same stripping is what keeps a
/// `#[cfg(test)]` *mention* in a comment or string from triggering a
/// blank.
fn blank_test_gated_mods(code: &str) -> String {
    let needle = "#[cfg(test)]";
    let bytes = code.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut copied = 0;
    let mut i = 0;
    while let Some(found) = code[i..].find(needle) {
        let at = i + found;
        i = at + needle.len();
        // The attribute gates the next item; whitespace (including blank
        // lines) between attribute and item does not change that.
        let mut cursor = i;
        while matches!(bytes.get(cursor), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            cursor += 1;
        }
        let rest = &code[cursor..];
        let Some(after_mod) = rest.strip_prefix("mod ") else {
            // A `#[cfg(test)]`-gated non-`mod` item (a `use`, a field):
            // leave it — it is not where fixture literals live.
            continue;
        };
        // The item ends either at the declaration's `;` or at the end of
        // the inline module's brace-matched body.
        let end = match after_mod.find('{') {
            Some(brace) if after_mod[..brace].find(';').is_none() => {
                let body_at = cursor + "mod ".len() + brace;
                let mut depth = 0usize;
                let mut body_end = bytes.len();
                for (offset, byte) in bytes[body_at..].iter().enumerate() {
                    match byte {
                        b'{' => depth += 1,
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                body_end = body_at + offset + 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                body_end
            }
            _ => {
                // `mod name;` — blank through the first `;`, which cannot
                // belong to a body because an inline module's `{` came
                // first when there was one.
                after_mod.find(';').map_or(cursor + rest.len(), |s| {
                    cursor + "mod ".len() + s + 1
                })
            }
        };
        // Keep the removed range's newlines so surviving lines keep their
        // line numbers in the violation report.
        out.extend_from_slice(&code.as_bytes()[copied..at]);
        out.extend(code[at..end].bytes().map(|b| if b == b'\n' { b'\n' } else { b' ' }));
        copied = end;
        i = end;
    }
    out.extend_from_slice(&code.as_bytes()[copied..]);
    String::from_utf8(out).expect("blanking preserves UTF-8 boundaries")
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
        while chars.get(j).is_some_and(|&(_, c)| c == '#') {
            hashes += 1;
            j += 1;
        }
        (chars.get(j).is_some_and(|&(_, c)| c == '"')).then_some(hashes)
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
                    && chars.get(i + 1).is_some_and(|&(_, n)| n == '/')
                {
                    state = State::Line;
                    out.push_str("  ");
                    i += 2;
                } else if c == '/'
                    && chars.get(i + 1).is_some_and(|&(_, n)| n == '*')
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
                    && chars.get(i + 1).is_some_and(|&(_, n)| n == 'r')
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
                    && chars.get(i + 1).is_some_and(|&(_, n)| n == '*')
                {
                    state = State::Block(depth + 1);
                    i += 1;
                } else if c == '*'
                    && chars.get(i + 1).is_some_and(|&(_, n)| n == '/')
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
                        .is_some_and(|tail| tail.iter().all(|&(_, ch)| ch == '#'));
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
    let code = blank_test_gated_mods(&strip_comments_and_strings(src));
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
    // fixtures the mod blanker cannot see when a whole file IS a test
    // module declared without its own `#[cfg(test)]` attribute;
    // version.rs is the constants authority the rule routes through.
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

/// The blanker excludes test-gated modules — trailing (the common shape)
/// AND early ones — so a fixture's synthetic literals do not trip the
/// scan while every production item around them stays scanned.
#[test]
fn test_gated_modules_are_blanked_without_hiding_what_surrounds_them() {
    // Trailing inline module (the old cut's exact case).
    let src = "fn a() {}\n\n#[cfg(test)]\nmod fixtures {\n    NifVersion(0x1401_0001);\n}\n";
    let code = blank_test_gated_mods(&strip_comments_and_strings(src));
    assert!(find_nif_version_hex_literals(&code).is_empty());
    assert!(code.contains("fn a() {}"));

    // #5258 — an EARLY test module must hide only itself: production
    // code after it stays scanned. The old first-occurrence cut never
    // scanned the second function.
    let src = "mod pre { }\n\n#[cfg(test)]\nmod fixtures;\n\nfn parser() {\n    let v = NifVersion(0x1401_0001);\n}\n\n#[cfg(test)]\nmod more_fixtures;\n";
    let code = blank_test_gated_mods(&strip_comments_and_strings(src));
    assert!(
        !find_nif_version_hex_literals(&code).is_empty(),
        "a bare literal after an early test-module declaration must be \
         scanned — the cut hid every production item after it (#5258)"
    );
    assert!(!code.contains("fixtures"));

    // A `#[cfg(test)]`-gated non-mod item (the walk/mod.rs use) stays,
    // and a mention inside a comment cannot trigger a blank: stripping
    // runs before the blanker.
    let src = "#[cfg(test)]\npub(super) use something;\n// mentions #[cfg(test)] mod here\nfn real() {}\n#[cfg(test)]\nmod tests {\n    NifVersion(0x1401_0001);\n}\n";
    let code = blank_test_gated_mods(&strip_comments_and_strings(src));
    assert!(code.contains("pub(super) use something;"));
    assert!(code.contains("fn real() {}"));
    assert!(find_nif_version_hex_literals(&code).is_empty());

    // Line numbers survive the blank: the guard reports `file:line`.
    let src = "fn a() {}\n#[cfg(test)]\nmod fixtures {\n    x();\n}\nfn b() {}\n";
    let code = blank_test_gated_mods(&strip_comments_and_strings(src));
    let line_of = |text: &str, needle: &str| {
        text.lines().position(|l| l.contains(needle)).unwrap() + 1
    };
    assert_eq!(line_of(&code, "fn b() {}"), line_of(src, "fn b() {}"));
}
