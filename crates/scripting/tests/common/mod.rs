//! Shared helpers for the scripting integration-test binaries.
//!
//! #5855 — one skip-with-honour gate: an absent install only skips when
//! `BYROREDUX_REQUIRE_GAME_DATA` is unset. A strict lane fails instead of
//! reporting a vacuous ok — the #3850 contract, applied to the real-data
//! tests that used to `return` unconditionally.

/// Panic in a strict lane (`BYROREDUX_REQUIRE_GAME_DATA` set); a normal
/// run falls through to the caller's skip message.
pub fn require_or_skip(what: &str, tried: &dyn std::fmt::Display) {
    if std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok_and(|v| v != "0") {
        panic!(
            "BYROREDUX_REQUIRE_GAME_DATA is set, but {what} is unavailable \
             (tried {tried})"
        );
    }
}
