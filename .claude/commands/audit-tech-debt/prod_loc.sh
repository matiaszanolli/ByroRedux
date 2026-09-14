# Production-LOC estimate for one Rust source file — Dimension 1's primary
# measure (#3081). Sourced by SKILL.md Phase 1 step 5:
#
#     source .claude/commands/audit-tech-debt/prod_loc.sh
#     prod_loc_self_test     # must print "ok" before any figure is trusted
#     prod_loc crates/renderer/src/vulkan/context/mod.rs
#
# Pure-test files by this codebase's own naming convention (`tests.rs` /
# `*_tests.rs`, or anything under a `tests/` dir) report 0: their
# `#[cfg(test)] #[path = "..."] mod <name>;` gate lives in the PARENT file that
# declares them, so no in-file marker exists to detect it from.
#
# Everything else: total LOC minus every line inside a `#[cfg(test)]`- (or
# `#[cfg(all(test, ...))]`-) gated item. A braced item is tracked by brace
# depth, so scattered test blocks are all excluded; a `;`-terminated item (an
# external `mod tests;`, a test-only `use`) excludes only its own lines,
# attributes included. The attribute may be indented (an impl-level test fn).
#
# Braces are counted on CODE only (#4336). The previous inline awk counted
# every `{`/`}` on the line, so braces inside string literals moved the depth
# counter: test format strings (`"{call}"`) and source-scanning tests
# (`src.split("\n}\n")`) closed a test block early — counting the remaining
# tests as production (`translate/effects.rs` read 2093 against a true 1681) —
# or held one open, hiding real production code (`context/draw.rs` read 1890
# against 1957). Stripped before counting: `//` comments, nested `/* */`
# comments, string literals (escapes, multi-line), raw strings (`r"…"`,
# `r#"…"#`, `br#"…"#`) and char literals (`'{'`, `'\''`, `'\u{7B}'`).
# Lifetimes (`'a`) are left alone.
#
# `prod_loc_self_test` runs fixtures covering each of those shapes and returns
# non-zero on drift. Run it after touching the awk.

prod_loc() {
    case "$1" in
        */tests/*|*tests.rs) echo 0; return ;;
    esac
    awk '
        # Code-only copy of a line. Carries block-comment depth (blk) and
        # string state (str: 0 none, 1 "…", 2 raw) across lines.
        function code_only(s,    out, i, n, c, p, j, h, k) {
            out = ""; n = length(s); i = 1
            while (i <= n) {
                c = substr(s, i, 1)
                if (blk > 0) {
                    if (substr(s, i, 2) == "*/") { blk--; i += 2 }
                    else if (substr(s, i, 2) == "/*") { blk++; i += 2 }
                    else i++
                    continue
                }
                if (str == 1) {
                    if (c == "\\") i += 2
                    else { if (c == "\"") str = 0; i++ }
                    continue
                }
                if (str == 2) {
                    if (c == "\"" && substr(s, i + 1, hashes) == closer) { str = 0; i += 1 + hashes }
                    else i++
                    continue
                }
                if (substr(s, i, 2) == "//") break
                if (substr(s, i, 2) == "/*") { blk++; i += 2; continue }
                if (c == "\"") { str = 1; i++; continue }
                if (c == "r") {
                    p = (i > 1) ? substr(s, i - 1, 1) : ""
                    if (p == "b") p = (i > 2) ? substr(s, i - 2, 1) : ""
                    if (p !~ /[A-Za-z0-9_]/) {
                        j = i + 1; h = 0
                        while (substr(s, j, 1) == "#") { h++; j++ }
                        if (substr(s, j, 1) == "\"") {
                            str = 2; hashes = h; closer = ""
                            for (k = 0; k < h; k++) closer = closer "#"
                            i = j + 1
                            continue
                        }
                    }
                }
                if (c == "'\''") {
                    if (substr(s, i + 1, 1) == "\\") {
                        j = index(substr(s, i + 3), "'\''")
                        if (j > 0) { i = i + 3 + j; continue }
                    } else if (substr(s, i + 2, 1) == "'\''") {
                        i += 3; continue
                    }
                }
                out = out c
                i++
            }
            return out
        }
        function braces(s,    u, d) {
            u = s; d = gsub(/\{/, "", u)
            u = s; d -= gsub(/\}/, "", u)
            return d
        }
        { t = code_only($0) }
        in_test {
            depth += braces(t)
            if (depth <= 0) in_test = 0
            next
        }
        /^[[:space:]]*#\[cfg\((all\()?test[,)]/ {
            pending = 1
            sub(/^[[:space:]]*#\[cfg\((all\()?test[^]]*\]/, "", t)
        }
        pending {
            if (t ~ /\{/) {
                pending = 0; in_test = 1; depth = braces(t)
                if (depth <= 0) in_test = 0
            } else if (t ~ /;/) {
                pending = 0
            }
            next
        }
        { prod++ }
        END { print prod + 0 }
    ' "$1"
}

prod_loc_self_test() {
    local dir fail=0
    dir=$(mktemp -d) || return 1

    # Braces inside test-module strings: the effects.rs shape (#4336).
    cat > "$dir/strings_in_test_module.rs" <<'RS'
fn prod_a() {
    let s = "{";
}

#[cfg(test)]
mod tests {
    #[test]
    fn scans_itself() {
        let src = include_str!("x.rs");
        for body in src.split("\n}\n") {
            assert!(body.contains("{call}"), "{body}");
        }
    }
}
fn prod_b() {}
RS
    # Production code with braces in comments, chars, raw and multi-line
    # strings, around a top-level and an indented test item.
    cat > "$dir/braces_in_prod.rs" <<'RS'
// a comment with { brace
fn f() -> char { '{' }
/* block { comment
   still } { comment */
const R: &str = r#"}}"#;
#[cfg(test)]
fn helper() {
    let _ = '}';
}
fn g<'a>(x: &'a str) -> &'a str { x }
const S: &str = "multi
line { string";
    #[cfg(test)]
    fn indented() {
        let _ = "}";
    }
const Q: char = '\'';
fn h() {}
RS
    # `;`-terminated test items with extra attributes, and cfg(all(test, ..)).
    cat > "$dir/semicolon_items.rs" <<'RS'
fn a() {}
#[cfg(test)]
#[path = "a_tests.rs"]
mod a_tests;
#[cfg(all(test, feature = "inspect"))]
mod inspect_tests {
}
fn b() {}
RS
    printf 'fn x() {}\n' > "$dir/pure_tests.rs"

    # One call per fixture rather than a split "name count" loop: zsh does not
    # word-split unquoted expansions, and this file is sourced from either shell.
    _prod_loc_expect() {
        local got
        got=$(prod_loc "$dir/$1")
        if [ "$got" != "$2" ]; then
            echo "prod_loc self-test FAILED: $1 = $got, expected $2" >&2
            fail=1
        fi
    }
    _prod_loc_expect strings_in_test_module.rs 5
    _prod_loc_expect braces_in_prod.rs 10
    _prod_loc_expect semicolon_items.rs 2
    _prod_loc_expect pure_tests.rs 0
    rm -rf "$dir"
    [ "$fail" = 0 ] && echo "prod_loc self-test: ok"
    return $fail
}
