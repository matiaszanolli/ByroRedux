//! Helpers for this crate's source-scan tests — tests that `include_str!` a
//! source file and assert on its text because the property they pin (a phase
//! ordering, a fast-path gate, a doc claim) needs a running world to exercise.

/// A source file's production text: everything before its first
/// `#[cfg(test)] mod`.
///
/// `include_str!` brings the file's test modules in too, and those spell out
/// every needle the tests search for. A scan over the whole text is therefore
/// satisfied by the test's own literals whether or not the production code it
/// guards still exists. Scan this instead.
///
/// The cut matches `#[cfg(test)]` followed by `mod`, so a `#[cfg(test)] use`
/// import earlier in the file does not truncate production code. It assumes the
/// file's test modules trail its production code, which holds for every file
/// here that self-scans; a needle that lives after the first test module fails
/// the scan loudly rather than passing.
///
/// The same helper lives in `byroredux-renderer` (`source_scan`); `cfg(test)`
/// items do not cross crate boundaries.
pub(crate) fn production_text(src: &str) -> &str {
    src.split_once("\n#[cfg(test)]\nmod ")
        .expect("source has no `#[cfg(test)] mod` — nothing to cut, so it is not a self-scan")
        .0
}

#[cfg(test)]
mod tests {
    use super::production_text;

    #[test]
    fn cuts_at_the_first_test_module_and_not_at_a_test_only_import() {
        let src = "fn a() {}\n#[cfg(test)]\nuse x;\nfn b() {}\n#[cfg(test)]\nmod tests {\n    needle\n}\n";
        let production = production_text(src);
        assert_eq!(production, "fn a() {}\n#[cfg(test)]\nuse x;\nfn b() {}");
        assert!(!production.contains("needle"));
    }
}
