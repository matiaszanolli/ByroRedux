# #5143: TOOL-D4-2026-09-29-01: the shared "Windows rename" fallback runs on any `atomic_write` error on every OS and overwrites the good file with the temp's possibly-partial bytes

**Labels**: medium, bug, tech-debt

**Source report**: `docs/audits/AUDIT_TOOLING_2026-09-29.md`
**Severity**: MEDIUM
**Dimension**: Boot Handoff & Persistence (exposure: players and launcher users — `~/.byroredux/profiles.toml`, `settings.toml` input bindings, per-Play `boot.toml`)

## Location
- `crates/boot-request/src/lib.rs` (`BootRequest::save`)
- `crates/game-detect/src/overrides.rs` (`merge_into_file`)
- `crates/settings-io/src/lib.rs` (`save_to_path`)
- `crates/core/src/atomic_file.rs` (`atomic_write`)

## Description
Came in with the #4758 fix (`63c0aee3b`).
- `atomic_write` returns `Err` from any of: create, `write_all`, `sync_all`, read-back, `rename`, or the parent-directory fsync.
- All three callers handle it the same way: `Err(_) if path.exists() => fs::write(path, fs::read(&temp)?)`. No `cfg(windows)` and no check of which step failed; the comment calls it Windows-only, the code is not.
- Concrete cases:
  1. **Disk full.** `write_all` fails with a partial temp on disk (`atomic_write` only removes the temp on a read-back mismatch). `fs::read(&temp)` succeeds and `fs::write(path, …)` truncates the user's good file and writes the partial bytes. The next settings load fails to parse, and TOOL-D4-02 then drops everything. A `profiles.toml` loses its hand-authored `[profiles.*]` blocks.
  2. **EIO on `sync_all`.** The unsynced bytes are copied over the destination non-atomically — exactly what `atomic_write` exists to prevent.
  3. **Parent-directory fsync fails after a successful rename.** The temp is gone, so the fallback's `fs::read` returns NotFound and a completed write is reported as failed.
- The premise is also wrong: Rust's `std::fs::rename` on Windows already replaces an existing destination (`MOVEFILE_REPLACE_EXISTING` / `FILE_RENAME_FLAG_REPLACE_IF_EXISTS`). Where rename does fail there (sharing violation), the fallback's `fs::write` would usually fail too.
- The fallback and `atomic_temp_path` are copied verbatim three times instead of living in `core::atomic_file`.

## Evidence
```rust
// crates/boot-request/src/lib.rs (same shape in overrides.rs and settings-io)
match byroredux_core::atomic_file::atomic_write(path, &temp_path, text.as_bytes()) {
    Ok(()) => Ok(()),
    Err(rename_error) if path.exists() => {
        // Windows does not replace an existing destination with rename.
        fs::write(path, fs::read(&temp_path)...?)
```

## Impact
The durability contract #3472/#4758 set out to establish is voided on the one failure (disk full) it most needs to survive. Data at risk: user-edited profiles and bindings.

## Related
#4758, #3472, TOOL-D4-2026-09-29-02. The save ring (`crates/save/src/disk.rs`) uses `atomic_write` without this fallback. Label gap: launcher / settings-io have no own label → `tech-debt`.

## Suggested Fix
Delete the fallback. If a Windows sharing-violation retry is really wanted, move it into `atomic_file`, gate it to `cfg(windows)` and to an error from the `rename` step only. Keep one shared `atomic_temp_path`.

Validated at HEAD 9fcfdc3fc: the `if path.exists()` → `fs::write(path, fs::read(&temp_path)?)` branch is present in all three crates, ungated; `atomic_write` leaves a partial temp on `write_all`/`sync_all` failure.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (all three callers + save ring)
- [ ] **TESTS**: A regression test pins this specific fix (inject a write failure and assert the destination is untouched)
