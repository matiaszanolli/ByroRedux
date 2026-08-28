# Issue #3472

**Title**: ECS-2026-08-27-03: `settings_io::save_to_path` writes-then-renames without an `fsync`, diverging from the project's own atomic-write doctrine one crate over

**Labels**: low, ecs, save-load, bug

**Filed**: 2026-08-27 via `/audit-publish docs/audits/AUDIT_ECS_2026-08-27.md`

---

**Source**: `docs/audits/AUDIT_ECS_2026-08-27.md` — finding `ECS-2026-08-27-03` (LOW, Dimension 7: P2 gameplay-slice lifecycles — `settings_io.rs`). Audited at `HEAD = 969d81c8`; re-verified against current code at publish time.

## Description

`settings_io::save_to_path` creates a `.tmp` sibling with `fs::write`, then `fs::rename`s it over the target. It never flushes the temp file to disk before the rename, never re-reads to verify the bytes landed, and never syncs the parent directory afterward.

`crates/save/src/disk.rs` — the M45 save path, whose module doc calls this "the crash-safe dance" — does all three: `f.sync_all()?` on the temp file, a read-back verification against the written bytes, and a parent-directory `dir_file.sync_all()?` with an explicit SAVE-D3-01 comment saying a successful rename is not durable until the parent directory is synced. Two writers in the same binary, two different durability contracts, and only one of them documents which it is.

## Evidence

```rust
// byroredux/src/settings_io.rs — save_to_path
let temp_path = temporary_path(path);
fs::write(&temp_path, source)?;
if let Err(rename_error) = fs::rename(&temp_path, path) {
```

versus

```rust
// crates/save/src/disk.rs — write_slot
let mut f = fs::File::create(&tmp_path)?;
f.write_all(bytes)?;
f.flush()?;
f.sync_all()?;
…
fs::rename(&tmp_path, &final_path)?;
// SAVE-D3-01 — a successful `rename` isn't durable until the parent
// directory's own metadata is fsynced …
if let Ok(dir_file) = fs::File::open(dir) {
    dir_file.sync_all()?;
}
```

## Impact

A crash or power loss in the window between the rename hitting the directory journal and the temp file's data reaching the platter leaves a zero-length or truncated `settings.toml`. The loader degrades gracefully (a `toml::from_str` failure is logged and skipped in `settings_io.rs`), so the user loses their control bindings and preferences rather than the session — hence LOW, not the save path's severity.

## Suggested fix

Replace the `fs::write` + `fs::rename` pair with the same `File::create` → `write_all` → `sync_all` → `rename` → parent `sync_all` sequence `crates/save/src/disk.rs` already implements, or factor that helper into a shared `atomic_write` used by both. If the weaker contract is deliberate, say so in the module doc and name `disk.rs` as the contrast.

## Related

#1714 / SAVE-D2-01 (the save-format compatibility doctrine), SAVE-D3-01 (the parent-directory sync this file lacks).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (any other write-then-rename site in `byroredux/` outside `crates/save/src/disk.rs`)
- [ ] **TESTS**: A regression test pins this specific fix (a temp-dir round trip asserting the durable sequence, e.g. through a shared `atomic_write` helper's own unit test)
