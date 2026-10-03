//! Support for source-scan tests — tests that `include_str!` a source file and
//! assert on its text because the property they pin cannot be exercised at
//! run time (an OS-level failure path, a removed branch).
//!
//! Plain `pub` rather than `#[cfg(test)]` so other crates' test modules can
//! call it: `cfg(test)` items do not cross crate boundaries. The renderer and
//! physics crates carry crate-private copies of [`production_text`] that
//! predate this module.

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
    fn counts_whole_identifiers_only() {
        let src = "let temp_path = x; f(&temp_path); temp_path_2; my_temp_path";
        assert_eq!(count_identifier(src, "temp_path"), 2);
    }
}
