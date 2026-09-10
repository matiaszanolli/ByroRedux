# #4087 — INC-2026-09-09-02

the boot `SOURCES` completeness gate walks two hard-coded directories, so a new `boot/` subdirectory is invisible to it

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_INCREMENTAL_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4087 --json state`).

---

- **Severity**: LOW
- **Dimension**: concurrency / scheduler source-shape coverage (`/audit-concurrency` Dim 4), test-gap
- **Location**: [`byroredux/src/boot/mod.rs:576-605`](../../byroredux/src/boot/mod.rs)
- **Changed in**: `byroredux/src/boot/mod.rs` (commit `8c5e02aa`)
- **Status**: NEW. Dedup: no issue title matches `SOURCES`, `boot/`, or the gate's name;
  no prior report in `docs/audits/` covers it (the file is one day old). Verified against
  code, not issue absence.
- **Description**: `the_concat_list_covers_every_file_in_the_boot_directory` exists
  because "a new file carrying registrations that nobody adds to `SOURCES` is invisible
  to the scheduler's source-shape tests: they keep passing while covering strictly less
  than they claim to." Its own directory walk is hard-coded to exactly two directories,
  so a file under any *third* directory — `boot/schedule/extra/foo.rs`, or a future
  `boot/registries/` — reproduces precisely the hole the gate was written to close, and
  the gate stays green.
- **Evidence**:
  ```rust
  // byroredux/src/boot/mod.rs:586
  for dir in [root.clone(), root.join("schedule")] {
      let prefix = if dir == root { "" } else { "schedule/" };
      for entry in std::fs::read_dir(&dir).unwrap() {
  ```
  Contrast the sibling gate in the same range, which *does* recurse:
  ```rust
  // crates/renderer/src/vulkan/image.rs:455-462
  let mut stack = vec![root.clone()];
  while let Some(dir) = stack.pop() {
      for entry in std::fs::read_dir(&dir)… { if path.is_dir() { stack.push(path); continue; } …
  ```
- **Impact**: coverage loss only; no runtime effect. Scheduler declared-access
  invariants (the boot deadlock proof) are the thing that would silently stop being
  checked, which is why it is worth closing rather than tolerating.
- **Related**: INC-2026-09-09-03 (same gate family), #3855.
- **Suggested Fix**: replace the two-element array with the same explicit
  directory-stack walk `image.rs` uses, deriving `prefix` from `strip_prefix(&root)`.

---
