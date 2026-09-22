# TOOL-D4-2026-09-22-01: BootRequest::save and RootOverrides::merge_into_file still use bare fs::write, not the project's atomic-write doctrine

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4758

## Description
`atomic_write` (temp → fsync → read-back → rename → parent fsync, `crates/core/src/atomic_file.rs:38`) is the project's documented durability contract; `settings-io` was moved onto it by #3472 specifically because "there is no reason for two writers in one binary to have two different durability contracts." Two more writers remain un-migrated, confirmed at HEAD:
- `BootRequest::save` (`crates/boot-request/src/lib.rs:293-309`) — writes the launcher→engine `boot.toml` handoff via bare `fs::write(path, text)` (line 307).
- `RootOverrides::merge_into_file` (`crates/game-detect/src/overrides.rs:96-136`) — rewrites the user's hand-edited `~/.byroredux/profiles.toml`, merge-preserving hand-authored `[profiles.*]`/`[defaults]` blocks per its own doc comment, via bare `std::fs::write(path, text)` (line 133).

`boot-request` deliberately has no `byroredux-core` dependency — confirmed by reading `crates/boot-request/Cargo.toml`'s own comment ("Deliberately dependency-free beyond serde + toml... anything heavier here would leak a GPU/engine dependency into the launcher") and its `[dependencies]` block (`serde`, `toml`, `thiserror` only) — so it cannot call `atomic_write` without a dependency decision.

## Evidence
```rust
// crates/boot-request/src/lib.rs:307
fs::write(path, text).map_err(|source| BootRequestError::Write { ... })
```
```rust
// crates/game-detect/src/overrides.rs:133
std::fs::write(path, text).map_err(|source| OverrideError::Write { ... })
```

## Impact
A crash/power-loss mid-write leaves a truncated file. For `boot.toml`, low value at risk (the launcher regenerates it fresh every `Play`, so a torn file only fails the *next* attempt with a clear parse error). For `profiles.toml`, the file carries hand-authored user content that a torn `byro-detect --write` could destroy alongside the freshly-detected `[roots]` table — this is the higher-value path.

## Related
#3472 (closed) fixed this exact class for `settings-io`; its own rename-failure fallback (`fs::write` over the destination on Windows rename failure) is separately, deliberately non-atomic per an in-place comment — not part of this finding.

## Suggested Fix
Add `byroredux-core` as a dependency of `boot-request` and reuse `atomic_write` directly, or extract it into a dependency-light shared crate both `boot-request` and `game-detect` can use.

Source: docs/audits/AUDIT_TOOLING_2026-09-22.md (TOOL-D4-2026-09-22-01)

## Completeness Checks
- [ ] **SIBLING**: Both writers (`BootRequest::save` and `RootOverrides::merge_into_file`) migrated together, given they share the same root cause and fix shape
- [ ] **TESTS**: A regression test simulates a torn write (or at minimum asserts the new path uses `atomic_write`) for both call sites
