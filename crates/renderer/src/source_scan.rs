//! Helpers for the crate's source-scan tests — tests that `include_str!` a
//! source file and assert on its text because the property they pin (an
//! ordering, a gate, a teardown sequence) needs a device to exercise.

/// A source file's production text: everything before its first
/// `#[cfg(test)] mod`.
///
/// `include_str!` brings in the file's test modules too, and those spell out
/// every needle the tests search for. A scan over the whole text is therefore
/// satisfied by the test's own literals whether or not the production code it
/// guards still exists — the defect class of #3442, #4604 and #4842. Scan this
/// instead.
///
/// The cut matches `#[cfg(test)]` followed by `mod`, so a `#[cfg(test)] use`
/// import earlier in the file does not truncate production code. A file whose
/// test modules are interleaved with production code (`context/draw.rs`) is not
/// served by this — a needle that lives after the first test module fails the
/// scan loudly rather than passing — and slices the function it guards
/// instead.
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
