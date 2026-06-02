# Issues 1423 + 1424

## #1423 FFI-02: No test coverage for the cxx bridge
**File:** `crates/cxx-bridge/src/lib.rs:1`
**Domain:** cxx
**Fix:** Add `#[cfg(test)]` module with a test calling `ffi::native_hello()`.
The C++ side is compiled via `cxx_build` + `build.rs` into a static lib, so the
test can call through the real FFI. `native_hello()` is in `unsafe extern "C++"`
so the call requires `unsafe {}`.

## #1424 ANIM-02: B-spline comment incorrectly scopes to Skyrim/FO4
**Files:**
- `crates/nif/src/blocks/mod.rs:832` — "Skyrim / FO4" should include FO3/FNV
- `crates/nif/src/anim/transform.rs:47` — same stale scope comment
**Domain:** animation
**Fix:** Update both comments to state the interpolator is reachable on all
games from FO3/FNV onwards, not just Skyrim/FO4. No code changes — the
dispatch path is already game-agnostic.
