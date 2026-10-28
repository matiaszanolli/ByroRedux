# SKY-D5-01: skyrimle_bsa_corpus_full_extract_zero_errors passes silently when BYROREDUX_SKYRIMLE_DATA is unset

Labels: low, import-pipeline, test-gap, bug, game:skyrim, legacy-compat
Source: docs/audits/AUDIT_SKYRIM_2026-10-27.md

**Source**: `docs/audits/AUDIT_SKYRIM_2026-10-27.md` finding `SKY-D5-01` (Dimension 5, archives + corpus gates) · Severity LOW (test-gap) · Status NEW (the #6184 shape) · Game: Skyrim LE.

**Location**: `crates/bsa/tests/bsa_real.rs:99-103` (`skyrimle_data_dir`), `:322-326` (`skyrimle_bsa_corpus_full_extract_zero_errors`).

## Description
Every other game in `bsa_real.rs` resolves through `data_dir(env, fallback)` (:55-75). That helper falls back to the dev-machine path and calls `require_game_data` on the strict lane. `skyrimle_data_dir` is env-only. It has no Wine-prefix fallback, although `_audit-common.md` and `crates/nif/tests/common/mod.rs` (`Game::SkyrimLE.default_path()`) both use one, and it never fails the strict lane. With the var unset, the test prints an `eprintln` and returns `ok`.

## Evidence
Current helper:
```rust
fn skyrimle_data_dir() -> Option<PathBuf> {
    std::env::var_os("BYROREDUX_SKYRIMLE_DATA")
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
}
```
This run, `cargo test --release -p byroredux-bsa --test bsa_real -- --ignored --exact skyrimle_bsa_corpus_full_extract_zero_errors` printed `[Skyrim LE] set BYROREDUX_SKYRIMLE_DATA to enable corpus sweep` and reported `ok`. The LE NIF corpus in the same shell ran 22,466 NIFs through its fallback. With the var set, the sweep extracts 9 archives / 132,755 entries with 0 errors.

## Impact
The LE BSA v104 extraction gate reads green without having run. The "LE extracts every entry" guard is only as good as the operator's environment, and `BYROREDUX_REQUIRE_GAME_DATA` cannot make it fail.

## Related
#6184, #5903, #3850.

## Suggested Fix
Route it through `data_dir("BYROREDUX_SKYRIMLE_DATA", <Wine-prefix Data path>)` so it gets the fallback and `require_game_data` strictness.

## Completeness Checks
- [ ] **SIBLING**: Other env-only data-dir helpers in `crates/*/tests/` checked for the same silent pass (#6184 allowlist)
- [ ] **TESTS**: With `BYROREDUX_REQUIRE_GAME_DATA=1` and no data, the LE sweep fails rather than passes

