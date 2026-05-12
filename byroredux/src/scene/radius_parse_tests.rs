//! Regression tests for [`parse_exterior_radius`] — issue #531.
//!
//! The CLI `--radius N` argument pre-fix was silently ignored
//! (hardcoded `3` in the `load_exterior_cells` call). These tests
//! pin the clamp bounds + the fallback behaviour so a future
//! refactor that tries to "simplify" the parse (e.g. remove the
//! clamp, loosen the Err-fallback to 0) gets caught.

use super::parse_exterior_radius;

#[test]
fn parses_valid_radius_verbatim() {
    assert_eq!(parse_exterior_radius("1"), 1);
    assert_eq!(parse_exterior_radius("3"), 3);
    assert_eq!(parse_exterior_radius("5"), 5);
    assert_eq!(parse_exterior_radius("7"), 7);
}

#[test]
fn clamps_below_one_to_one() {
    assert_eq!(parse_exterior_radius("0"), 1);
    assert_eq!(parse_exterior_radius("-5"), 1);
}

#[test]
fn clamps_above_seven_to_seven() {
    assert_eq!(parse_exterior_radius("8"), 7);
    assert_eq!(
        parse_exterior_radius("100"),
        7,
        "accidental large input must not load 40k cells"
    );
}

#[test]
fn falls_back_to_default_on_parse_failure() {
    // Non-numeric input → fall back to 3 (default 7×7 grid).
    assert_eq!(parse_exterior_radius("foo"), 3);
    assert_eq!(parse_exterior_radius(""), 3);
    assert_eq!(parse_exterior_radius("3.5"), 3);
}

#[test]
fn trims_whitespace_before_parse() {
    assert_eq!(parse_exterior_radius("  5  "), 5);
}
