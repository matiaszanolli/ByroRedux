# SAFE-2026-10-27-02: Two comment-less test `unsafe { set_var }` blocks in `test_paths.rs` that contradict the file's own "not unit-tested" note

Labels: medium, safety, esm-plugin, bug
Source: docs/audits/AUDIT_SAFETY_2026-10-27.md

**Audit report**: `docs/audits/AUDIT_SAFETY_2026-10-27.md` -- finding `SAFE-2026-10-27-02` (MEDIUM)
**Location (re-mapped to current code)**: `crates/plugin/src/esm/test_paths.rs` (~:245-251 blocks in `require_or_skip_fails_only_when_the_strict_lane_asks_for_game_data`; contradicting doc ~:274-277 on the #3850 test). Premise re-read at HEAD: the `set_var(REQUIRE_GAME_DATA_ENV, "0")` and `set_var(..., "1")` blocks have no SAFETY comment; the first `remove_var` block has one, the restore block has one.
**Status at filing**: NEW (introduced by 3e4bfab51, Fix #6184). Severity MEDIUM is the `_audit-severity` floor for an uncommented `unsafe` block; the code is test-only.

## Description
`require_or_skip_fails_only_when_the_strict_lane_asks_for_game_data` toggles `BYROREDUX_REQUIRE_GAME_DATA` with four `unsafe` env edits. Two of them (`set_var(..., "0")` and `set_var(..., "1")`) carry no SAFETY comment; `clippy::undocumented_unsafe_blocks --tests` flags exactly these two. The note the other blocks lean on ("see `data_dir_accessors_...`") argues that the touched vars are read by no other test. Meanwhile the #3850 test's doc ~25 lines below still says this exact variable is "deliberately not unit-tested: it is read process-globally by every resolver in the workspace, so setting it here would turn every concurrently-running real-data test in this binary into a panic."

## Evidence
- `warning: unsafe block missing a safety comment --> crates/plugin/src/esm/test_paths.rs:245:9` and `:249:9` (`cargo clippy -p byroredux-plugin --tests --no-deps -- -A clippy::all -W clippy::undocumented_unsafe_blocks`).
- Every in-crate `require_or_skip` caller (`esm/reader.rs`, `records/actor/tests.rs` x2, `cell/tests/wrld.rs`) is `#[ignore]`d.

## Impact
No production impact. In the default test process the race is latent. Under `cargo test -p byroredux-plugin -- --include-ignored` on a machine with game data, a concurrently running real-data test that hits a missing file while the flag is `"1"` would panic spuriously. The safety argument is unstated and the file contradicts itself. Crates outside the renderer have no `undocumented_unsafe_blocks` gate, which is why this landed.

## Related
#6184 (origin, closed).

## Suggested Fix
1. Give the two blocks a SAFETY line stating why the race is acceptable (every reader is `#[ignore]`d).
2. Update the #3850 doc so it no longer claims the variable is untested.
3. Optionally serialize env-mutating tests behind a shared `Mutex` so `--include-ignored` is also sound.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)

