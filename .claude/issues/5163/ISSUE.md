# #5163 — TOOL-D4-2026-10-02-02: `atomic_write` leaves its temp on failure, and #5143's unique temp names turn that into accumulation

Labels: low,tech-debt,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/5163

From `docs/audits/AUDIT_TOOLING_2026-10-02.md` (HEAD `e737f06bf`).

- **Severity**: LOW
- **Dimension**: Boot Handoff & Persistence
- **Exposure**: any user whose config disk fails a write (disk full, EIO)
- **Location**: `crates/core/src/atomic_file.rs:19-25` (`atomic_temp_path`), `:55-84` (`atomic_write`); callers `crates/settings-io/src/lib.rs:243-259`, `crates/boot-request/src/lib.rs:307`, `crates/game-detect/src/overrides.rs:133-137`
- **Status**: NEW. It was introduced by the #5143 fix (`2e95f0bbf`).
- **Description**: `atomic_write` removes the temp only in its read-back-mismatch arm. If create, `write_all`, `flush`, `sync_all` or `rename` fails, the temp is left behind holding whatever bytes reached it. Before `2e95f0bbf`, two things limited the damage:
  - settings-io staged through the fixed name `settings.toml.tmp`, which the next attempt overwrote.
  - The (now removed) fallback branch deleted the temp.

  Now every writer uses `.{name}.{pid}.{counter}.tmp`, and the callers do not clean up. Each failed save leaves one more hidden file.
- **Evidence**: `atomic_file.rs:62-66` uses `?`-propagation on every step before the read-back. The only `remove_file` is at `:72`, inside the mismatch branch. The old helper is visible in `git show 2e95f0bbf^:crates/settings-io/src/lib.rs` (`temporary_path`, `:274-278`).
- **Impact**: This happens on the disk-full path #5143 was written for. Every settings change in the in-game menu saves, and each failed save leaves another partial `.settings.toml.<pid>.<n>.tmp` in the config directory. That consumes space on a disk that is already full. There is no data loss.
- **Related**: #5143, TOOL-D4-03
- **Suggested Fix**: In `atomic_write`, best-effort `remove_file(tmp_path)` on every error before the rename returns, and after a failed rename. A guard test can inject the failure with a temp path under a missing directory.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other writers / frontends / request variants named above)
- [ ] **TESTS**: A regression test pins this specific fix

